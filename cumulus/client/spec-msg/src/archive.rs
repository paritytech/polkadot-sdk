// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! The sender-side archive: root-keyed storage of the own chain's sends,
//! and the serving half of the `/spec-msg/exchange` protocol.
//!
//! # Layout
//!
//! Three kinds of state, all mirrored to the client's auxiliary store
//! (exact-key reads only — everything is reconstructable on restart):
//!
//! - per stream: the payloads and leaf hashes keyed `(stream, position)` — positions are dense by
//!   construction, mirroring the sender pallet's storage layout — plus the live frontier, the
//!   payload floor (watermark pruning) and the floor frontier (horizon pruning; proof generation
//!   over the unpruned tail extends from it);
//! - per block: the frontier *boundary* — every stream's leaf count as of that block plus the
//!   recomputed `StreamsRoot` over them. Extension proofs are built from block boundaries; the
//!   boundary chain (hash, number) also anchors reorg rewinds;
//! - the root index: `StreamsRoot → boundary`, how a request's `under` is resolved. The archive
//!   recomputes every root itself — it never takes one from the runtime — so a divergence surfaces
//!   as an unknown root, never as a wrong proof.
//!
//! # Serving
//!
//! [`SpecMsgArchive::serve_messages`] / [`SpecMsgArchive::serve_event`] are
//! pure functions of the request against the retained state: resolve
//! `under` to a boundary, cap everything at the stream's leaf count *under
//! that root*, and prove under exactly that root — payloads, `start_peaks`
//! and the extension from the recomputed frontier to the stream's entry,
//! then the tree walk up to `under`. `max_bytes = 0` serves pure lift
//! material (payload-free). Requests that cannot be served (unknown root,
//! payloads below the watermark, material below the horizon) fail cleanly;
//! the requester falls back per liftability's three layers.
//!
//! # Retention
//!
//! Channel payloads are prunable below the peer's confirmation watermark
//! ([`SpecMsgArchive::prune_payloads`]; the worker drives it from the
//! `out_channels()` register views). Leaf hashes and boundaries are kept to
//! the [`SERVING_HORIZON`] regardless ([`SpecMsgArchive::prune_horizon`]):
//! extension material is serveable from any block boundary within 25 h,
//! with the boundary frontier at the pruning point retained — exactly what
//! appending and proof generation over the unpruned tail require.

use std::{
	collections::{BTreeMap, VecDeque},
	sync::Arc,
	time::Duration,
};

use codec::{Decode, Encode};
use sc_client_api::AuxStore;
use sp_runtime::traits::{Block as BlockT, NumberFor, One, SaturatedConversion};

use cumulus_primitives_spec_messaging::{
	compute_streams_root, hash_leaf, prove_stream, EventRequest, EventResponse, MessagePosition,
	MessagesRequest, MessagesResponse, MmrFrontier, MmrRoot, SpecHasher, StreamId, StreamsRoot,
	TreeInclusionProof, LEAF_VERSION,
};
use polkadot_core_primitives::Hash;

use crate::{
	nodes::{HistoricNodes, LeafHashes},
	LOG_TARGET,
};

/// How long extension material stays serveable, counted from a block
/// boundary's archiving time (v0.5 §Liftability): leaf hashes, boundaries
/// and with them the resolvable `under` roots are retained this long. The
/// normative obligation is "any block boundary within 25 h"; the flat
/// window is the default policy knob on top of the watermark rule, not the
/// rule itself.
pub const SERVING_HORIZON: Duration = Duration::from_secs(25 * 60 * 60);

/// Server-side hard cap on served payload bytes per response, regardless of
/// the request's `max_bytes` ("the server may cap harder"). Leaves ample
/// headroom under [`crate::MAX_RESPONSE_SIZE`] for proofs and envelope.
const MAX_SERVED_PAYLOAD_BYTES: usize = 4 * 1024 * 1024;

/// Errors mutating or loading the archive.
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
	/// The imported block does not extend the archive's tip.
	#[error("imported block does not extend the archive tip")]
	NotChild,
	/// A rewind target that is not an archived boundary.
	#[error("rewind target is not an archived block")]
	UnknownBlock,
	/// Retained material the operation relies on is missing.
	#[error("archive is missing retained material: {0}")]
	Corrupt(&'static str),
	/// The auxiliary store failed.
	#[error("auxiliary store error: {0}")]
	Aux(#[from] sp_blockchain::Error),
}

/// Why a request was refused. Never sent to the peer — the transport-level
/// refusal carries no detail ("serve or fail, nothing resolved
/// server-side") — but logged and returned to local callers.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ServeError {
	/// The named `under` root is not (or no longer) retained.
	#[error("unknown or no longer retained root")]
	UnknownRoot,
	/// The stream has no entry under the named root.
	#[error("stream has no entry under the named root")]
	UnknownStream,
	/// The requested position lies beyond the stream's leaf count under the
	/// named root.
	#[error("position beyond the stream's head under the named root")]
	BeyondHead,
	/// The requested payloads are pruned (below the confirmation
	/// watermark). Extension material may still be serveable.
	#[error("payloads pruned below the confirmation watermark")]
	PayloadsPruned,
	/// Proof material below the serving horizon is no longer retained.
	#[error("required material is below the serving horizon")]
	BelowHorizon,
	/// The archive's own state is inconsistent — a local bug, never the
	/// requester's fault.
	#[error("internal archive inconsistency")]
	Internal,
}

/// One stream's live state.
#[derive(Clone, Debug, Decode, Default, Encode, Eq, PartialEq)]
struct StreamState {
	/// The live frontier over ALL of the stream's messages; its leaf count
	/// is the next append position.
	frontier: MmrFrontier,
	/// Frontier at the horizon floor: leaf hashes below its leaf count are
	/// pruned, its peaks tile them for proof generation.
	floor: MmrFrontier,
	/// Positions below hold no payload (watermark pruning).
	payload_floor: u64,
}

/// One block's frontier boundary.
#[derive(Clone, Debug, Decode, Encode)]
struct Boundary<H, N> {
	/// The block's hash — the rewind anchor.
	hash: H,
	/// The block's number — the boundary's aux key.
	number: N,
	/// Local wall-clock seconds when archived; drives the serving horizon.
	archived_at: u64,
	/// The recomputed `StreamsRoot` over all stream roots as of this block;
	/// `None` while no stream exists (a block with no streams commits
	/// nothing).
	root: Option<StreamsRoot>,
	/// Every stream's leaf count as of this block, in canonical order.
	counts: Vec<(StreamId, u64)>,
}

impl<H, N> Boundary<H, N> {
	fn count(&self, stream: &StreamId) -> Option<u64> {
		self.counts
			.binary_search_by_key(stream, |(id, _)| *id)
			.ok()
			.map(|index| self.counts[index].1)
	}
}

/// The archive's registry record: which streams exist, which boundary
/// numbers are retained. Everything else is keyed off these.
#[derive(Decode, Default, Encode)]
struct Meta<N> {
	streams: Vec<StreamId>,
	/// Retained boundary numbers, inclusive; `None` when empty.
	boundaries: Option<(N, N)>,
}

/// Aux-store keys, all under one prefix. Positions are big-endian so key
/// order matches position order (cosmetic — reads are exact-key).
mod keys {
	use super::StreamId;

	const PREFIX: &[u8] = b"spec_msg_archive";

	pub(super) fn meta() -> Vec<u8> {
		[PREFIX, b"/meta"].concat()
	}

	pub(super) fn stream(id: &StreamId) -> Vec<u8> {
		[PREFIX, b"/stream/", &id.to_bytes()[..]].concat()
	}

	pub(super) fn boundary(number: &[u8]) -> Vec<u8> {
		[PREFIX, b"/boundary/", number].concat()
	}

	pub(super) fn payload(id: &StreamId, position: u64) -> Vec<u8> {
		[PREFIX, b"/payload/", &id.to_bytes()[..], &position.to_be_bytes()[..]].concat()
	}

	pub(super) fn leaf(id: &StreamId, position: u64) -> Vec<u8> {
		[PREFIX, b"/leaf/", &id.to_bytes()[..], &position.to_be_bytes()[..]].concat()
	}
}

/// The sender-side archive. See the module docs.
///
/// Concurrency is the caller's: the worker mutates, the request handler
/// reads — wrap in a `parking_lot::RwLock` (both are done by
/// [`crate::run_spec_msg_archiver`] / [`crate::SpecMsgRequestHandler`]).
pub struct SpecMsgArchive<Block: BlockT, AUX> {
	aux: Arc<AUX>,
	streams: BTreeMap<StreamId, StreamState>,
	/// Oldest → newest; numbers contiguous, hashes parent-linked.
	boundaries: VecDeque<Boundary<Block::Hash, NumberFor<Block>>>,
	/// `StreamsRoot` → number of the NEWEST boundary bearing it (idle
	/// blocks repeat their parent's root; any bearer serves identically,
	/// the newest one is retained longest).
	root_index: BTreeMap<StreamsRoot, NumberFor<Block>>,
}

impl<Block: BlockT<Hash = Hash>, AUX: AuxStore> SpecMsgArchive<Block, AUX> {
	/// Loads the archive persisted in `aux` (empty store = empty archive).
	pub fn load(aux: Arc<AUX>) -> Result<Self, ArchiveError> {
		let meta: Meta<NumberFor<Block>> = read_aux(&*aux, &keys::meta())?.unwrap_or_default();

		let mut streams = BTreeMap::new();
		for id in &meta.streams {
			let state = read_aux(&*aux, &keys::stream(id))?
				.ok_or(ArchiveError::Corrupt("stream state missing"))?;
			streams.insert(*id, state);
		}

		let mut boundaries = VecDeque::new();
		if let Some((first, last)) = meta.boundaries {
			let mut number = first;
			loop {
				let boundary = read_aux(&*aux, &keys::boundary(&number.encode()))?
					.ok_or(ArchiveError::Corrupt("boundary missing"))?;
				boundaries.push_back(boundary);
				if number == last {
					break;
				}
				number += One::one();
			}
		}

		let mut archive = Self { aux, streams, boundaries, root_index: BTreeMap::new() };
		archive.rebuild_root_index();
		Ok(archive)
	}

	/// The newest archived block, if any.
	pub fn tip(&self) -> Option<(Block::Hash, NumberFor<Block>)> {
		self.boundaries.back().map(|boundary| (boundary.hash, boundary.number))
	}

	/// Whether `hash` is an archived boundary (rewind anchor).
	pub fn contains_block(&self, hash: &Block::Hash) -> bool {
		self.boundaries.iter().rev().any(|boundary| boundary.hash == *hash)
	}

	/// Whether requests naming `root` can currently be resolved.
	pub fn serves_root(&self, root: &StreamsRoot) -> bool {
		self.root_index.contains_key(root)
	}

	/// Appends one block of the own chain: its sends (the
	/// `outbound_messages()` runtime API output, canonical stream order)
	/// and its frontier boundary. The first imported block may be any block
	/// — the archive is only correct if no stream had messages before it
	/// (fresh chains; returning nodes re-execute or rebuild) — thereafter
	/// blocks must chain onto the tip; reorgs go through
	/// [`Self::rewind_to`] first.
	///
	/// Returns the recomputed `StreamsRoot` as of this block.
	pub fn import_block(
		&mut self,
		hash: Block::Hash,
		parent: Block::Hash,
		number: NumberFor<Block>,
		sends: Vec<(StreamId, Vec<Vec<u8>>)>,
	) -> Result<Option<StreamsRoot>, ArchiveError> {
		self.import_block_at(hash, parent, number, sends, now_secs())
	}

	/// [`Self::import_block`] with an explicit archiving timestamp —
	/// deterministic replay and horizon tests.
	pub fn import_block_at(
		&mut self,
		hash: Block::Hash,
		parent: Block::Hash,
		number: NumberFor<Block>,
		sends: Vec<(StreamId, Vec<Vec<u8>>)>,
		archived_at: u64,
	) -> Result<Option<StreamsRoot>, ArchiveError> {
		if let Some(tip) = self.boundaries.back() {
			if tip.hash != parent || number != tip.number + One::one() {
				return Err(ArchiveError::NotChild);
			}
		}

		let mut inserts: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();

		for (stream, payloads) in sends {
			if payloads.is_empty() {
				continue;
			}
			let state = self.streams.entry(stream).or_default();
			for payload in payloads {
				let position = state.frontier.leaf_count;
				let leaf = hash_leaf::<SpecHasher>(LEAF_VERSION, &payload);
				inserts.push((keys::payload(&stream, position), payload));
				inserts.push((keys::leaf(&stream, position), leaf.as_bytes().to_vec()));
				state.frontier.append_leaf(leaf);
			}
			inserts.push((keys::stream(&stream), state.encode()));
		}

		let entries: BTreeMap<StreamId, MmrRoot> =
			self.streams.iter().map(|(id, state)| (*id, state.frontier.root())).collect();
		let root = compute_streams_root(&entries);
		let counts = self
			.streams
			.iter()
			.map(|(id, state)| (*id, state.frontier.leaf_count))
			.collect();

		let boundary = Boundary { hash, number, archived_at, root, counts };
		inserts.push((keys::boundary(&number.encode()), boundary.encode()));
		if let Some(root) = root {
			self.root_index.insert(root, number);
		}
		self.boundaries.push_back(boundary);

		// The boundary range moves every block, so the (small) meta record
		// is rewritten regardless of whether a stream was born.
		inserts.push((keys::meta(), self.meta().encode()));

		write_aux(&*self.aux, inserts, Vec::new())?;
		Ok(root)
	}

	/// Rewinds the archive to the archived block `hash`, dropping every
	/// boundary above it and truncating the streams to their leaf counts at
	/// that boundary — the reorg path: rewind to the common ancestor, then
	/// import the new branch.
	pub fn rewind_to(&mut self, hash: Block::Hash) -> Result<(), ArchiveError> {
		let target_index = self
			.boundaries
			.iter()
			.rposition(|boundary| boundary.hash == hash)
			.ok_or(ArchiveError::UnknownBlock)?;
		if target_index + 1 == self.boundaries.len() {
			return Ok(());
		}

		let mut inserts: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
		let mut deletes: Vec<Vec<u8>> = Vec::new();

		let ids: Vec<StreamId> = self.streams.keys().copied().collect();
		for id in ids {
			let state = self.streams.get(&id).expect("iterating own keys; qed").clone();
			let target = self.boundaries[target_index].count(&id);
			if target == Some(state.frontier.leaf_count) {
				continue;
			}
			// Recompute the truncated frontier BEFORE deleting leaf hashes.
			let new_frontier = match target {
				None | Some(0) => MmrFrontier::default(),
				Some(count) => self
					.nodes_for(&id, &state)
					.frontier_at(count)
					.ok_or(ArchiveError::Corrupt("leaf hashes for rewind missing"))?,
			};
			for position in new_frontier.leaf_count..state.frontier.leaf_count {
				deletes.push(keys::leaf(&id, position));
				if position >= state.payload_floor {
					deletes.push(keys::payload(&id, position));
				}
			}
			if target.is_none() {
				// The stream did not exist at the target block.
				self.streams.remove(&id);
				deletes.push(keys::stream(&id));
			} else {
				let state = self.streams.get_mut(&id).expect("checked above; qed");
				state.payload_floor = state.payload_floor.min(new_frontier.leaf_count);
				state.frontier = new_frontier;
				inserts.push((keys::stream(&id), state.encode()));
			}
		}

		for boundary in self.boundaries.iter().skip(target_index + 1) {
			deletes.push(keys::boundary(&boundary.number.encode()));
		}
		self.boundaries.truncate(target_index + 1);
		self.rebuild_root_index();
		inserts.push((keys::meta(), self.meta().encode()));

		write_aux(&*self.aux, inserts, deletes)?;
		Ok(())
	}

	/// Prunes `stream`'s payloads below `below` — the confirmation
	/// watermark reported by the pallet's channel views. Leaf hashes are
	/// kept (to the serving horizon): watermark-pruned ranges refuse
	/// payload requests but still serve extension material.
	pub fn prune_payloads(
		&mut self,
		stream: &StreamId,
		below: MessagePosition,
	) -> Result<(), ArchiveError> {
		let Some(state) = self.streams.get_mut(stream) else { return Ok(()) };
		let new_floor = below.0.min(state.frontier.leaf_count);
		if new_floor <= state.payload_floor {
			return Ok(());
		}
		let deletes = (state.payload_floor..new_floor)
			.map(|position| keys::payload(stream, position))
			.collect();
		state.payload_floor = new_floor;
		let inserts = vec![(keys::stream(stream), state.encode())];
		write_aux(&*self.aux, inserts, deletes)?;
		Ok(())
	}

	/// Drops boundaries archived before `cutoff_secs` (typically `now -`
	/// [`SERVING_HORIZON`]) and advances each stream's horizon floor to its
	/// leaf count at the oldest retained boundary: leaf hashes below are
	/// deleted, the boundary frontier at the pruning point is retained. The
	/// newest boundary always stays — current roots are always serveable.
	pub fn prune_horizon(&mut self, cutoff_secs: u64) -> Result<(), ArchiveError> {
		let mut dropped = Vec::new();
		while self.boundaries.len() > 1 &&
			self.boundaries.front().map_or(false, |b| b.archived_at < cutoff_secs)
		{
			dropped.push(self.boundaries.pop_front().expect("len checked above; qed"));
		}
		if dropped.is_empty() {
			return Ok(());
		}

		let mut inserts: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
		let mut deletes: Vec<Vec<u8>> = Vec::new();
		for boundary in &dropped {
			deletes.push(keys::boundary(&boundary.number.encode()));
		}

		let oldest = self.boundaries.front().expect("newest boundary retained; qed").clone();
		let ids: Vec<StreamId> = self.streams.keys().copied().collect();
		for id in ids {
			let state = self.streams.get(&id).expect("iterating own keys; qed").clone();
			// Streams born after the oldest retained boundary keep their
			// (lower) floor.
			let Some(new_floor) = oldest.count(&id) else { continue };
			if new_floor <= state.floor.leaf_count {
				continue;
			}
			// Compute the new floor frontier BEFORE deleting leaf hashes.
			let floor = self
				.nodes_for(&id, &state)
				.frontier_at(new_floor)
				.ok_or(ArchiveError::Corrupt("leaf hashes for horizon floor missing"))?;
			for position in state.floor.leaf_count..new_floor {
				deletes.push(keys::leaf(&id, position));
				if position >= state.payload_floor {
					// Payloads below the horizon floor are unreachable (no
					// peaks can be built below it) — dead weight even if the
					// watermark stalls.
					deletes.push(keys::payload(&id, position));
				}
			}
			let state = self.streams.get_mut(&id).expect("iterating own keys; qed");
			state.payload_floor = state.payload_floor.max(new_floor);
			state.floor = floor;
			inserts.push((keys::stream(&id), state.encode()));
		}

		self.rebuild_root_index();
		inserts.push((keys::meta(), self.meta().encode()));
		write_aux(&*self.aux, inserts, deletes)?;
		Ok(())
	}

	/// Serves a [`MessagesRequest`]: payloads from `start` (none for
	/// `max_bytes = 0` — pure lift material), the `start` peaks, the
	/// extension to the stream's entry under `under` and the tree walk up
	/// to it. Pure function of the request; errors are local-only.
	pub fn serve_messages(
		&self,
		request: &MessagesRequest,
	) -> Result<MessagesResponse, ServeError> {
		let boundary = self.boundary_for(&request.under).ok_or(ServeError::UnknownRoot)?;
		let count = boundary.count(&request.stream).ok_or(ServeError::UnknownStream)?;
		let start = request.start.0;
		if start > count {
			return Err(ServeError::BeyondHead);
		}
		let state = self.streams.get(&request.stream).ok_or_else(|| {
			tracing::error!(
				target: LOG_TARGET,
				stream = ?request.stream,
				"Boundary lists a stream the archive does not know",
			);
			ServeError::Internal
		})?;

		let mut payloads = Vec::new();
		if request.max_bytes > 0 && start < count {
			if start < state.payload_floor {
				return Err(ServeError::PayloadsPruned);
			}
			let budget = (request.max_bytes as usize).min(MAX_SERVED_PAYLOAD_BYTES);
			let mut used = 0usize;
			for position in start..count {
				let payload =
					self.payload(&request.stream, position).ok_or(ServeError::Internal)?;
				// Always serve at least one payload — a single message
				// larger than `max_bytes` must not stall the stream.
				if !payloads.is_empty() && used + payload.len() > budget {
					break;
				}
				used += payload.len();
				payloads.push(payload);
			}
		}
		let served_to = start + payloads.len() as u64;

		let nodes = self.nodes_for(&request.stream, state);
		let start_peaks = nodes.frontier_at(start).ok_or(ServeError::BelowHorizon)?.peaks.to_vec();
		let extension = nodes.extension(served_to, count).ok_or(ServeError::BelowHorizon)?;
		let tree_proof = self.tree_proof_at(boundary, &request.stream)?;

		Ok(MessagesResponse {
			base: MessagePosition(start),
			leaf_version: LEAF_VERSION,
			payloads,
			start_peaks,
			extension,
			tree_proof,
		})
	}

	/// Serves an [`EventRequest`]: the one leaf (`at`, or the head as of
	/// `under`) with its inclusion proof and the tree walk up to `under`.
	pub fn serve_event(&self, request: &EventRequest) -> Result<EventResponse, ServeError> {
		let boundary = self.boundary_for(&request.under).ok_or(ServeError::UnknownRoot)?;
		let count = boundary.count(&request.stream).ok_or(ServeError::UnknownStream)?;
		let position = match request.at {
			Some(position) => position.0,
			None => count.checked_sub(1).ok_or(ServeError::BeyondHead)?,
		};
		if position >= count {
			return Err(ServeError::BeyondHead);
		}
		let state = self.streams.get(&request.stream).ok_or(ServeError::Internal)?;
		if position < state.payload_floor {
			return Err(ServeError::PayloadsPruned);
		}
		let payload = self.payload(&request.stream, position).ok_or(ServeError::Internal)?;
		let inclusion = self
			.nodes_for(&request.stream, state)
			.inclusion(count, position)
			.ok_or(ServeError::BelowHorizon)?;
		let tree_proof = self.tree_proof_at(boundary, &request.stream)?;

		Ok(EventResponse { payload, inclusion, tree_proof })
	}

	/// The tree inclusion proof of `stream`'s entry under `boundary`'s
	/// root, over the recomputed entry set — sanity-checked against the
	/// boundary's root, so a corrupted archive refuses rather than serving
	/// an unverifiable proof.
	fn tree_proof_at(
		&self,
		boundary: &Boundary<Block::Hash, NumberFor<Block>>,
		stream: &StreamId,
	) -> Result<TreeInclusionProof, ServeError> {
		let mut entries = BTreeMap::new();
		for (id, count) in &boundary.counts {
			let state = self.streams.get(id).ok_or(ServeError::Internal)?;
			let root = if *count == state.frontier.leaf_count {
				state.frontier.root()
			} else {
				self.nodes_for(id, state).root_at(*count).ok_or(ServeError::BelowHorizon)?
			};
			entries.insert(*id, root);
		}
		if compute_streams_root(&entries) != boundary.root {
			tracing::error!(
				target: LOG_TARGET,
				number = ?boundary.number,
				"Recomputed streams root diverges from the archived boundary",
			);
			return Err(ServeError::Internal);
		}
		prove_stream(&entries, stream).ok_or(ServeError::UnknownStream)
	}

	/// Resolves `under` to its (newest) boundary.
	fn boundary_for(
		&self,
		under: &StreamsRoot,
	) -> Option<&Boundary<Block::Hash, NumberFor<Block>>> {
		let number = self.root_index.get(under)?;
		let first = self.boundaries.front()?.number;
		let index = (*number - first).saturated_into::<u64>() as usize;
		let boundary = self.boundaries.get(index)?;
		debug_assert_eq!(boundary.root, Some(*under));
		Some(boundary)
	}

	/// Historic MMR state for `stream` over the retained leaf hashes.
	fn nodes_for(
		&self,
		stream: &StreamId,
		state: &StreamState,
	) -> HistoricNodes<AuxLeaves<'_, AUX>> {
		HistoricNodes::new(AuxLeaves { aux: &*self.aux, stream: *stream }, &state.floor)
	}

	/// The retained payload at `(stream, position)`; logs on store errors.
	fn payload(&self, stream: &StreamId, position: u64) -> Option<Vec<u8>> {
		match self.aux.get_aux(&keys::payload(stream, position)) {
			Ok(payload) => payload,
			Err(error) => {
				tracing::error!(target: LOG_TARGET, ?error, "Auxiliary store read failed");
				None
			},
		}
	}

	fn meta(&self) -> Meta<NumberFor<Block>> {
		Meta {
			streams: self.streams.keys().copied().collect(),
			boundaries: self
				.boundaries
				.front()
				.zip(self.boundaries.back())
				.map(|(first, last)| (first.number, last.number)),
		}
	}

	fn rebuild_root_index(&mut self) {
		self.root_index.clear();
		for boundary in &self.boundaries {
			if let Some(root) = boundary.root {
				self.root_index.insert(root, boundary.number);
			}
		}
	}
}

/// Leaf-hash access backed by the aux store.
struct AuxLeaves<'a, AUX> {
	aux: &'a AUX,
	stream: StreamId,
}

impl<AUX: AuxStore> LeafHashes for AuxLeaves<'_, AUX> {
	fn leaf_hash(&self, index: u64) -> Option<Hash> {
		match self.aux.get_aux(&keys::leaf(&self.stream, index)) {
			Ok(Some(bytes)) if bytes.len() == 32 => Some(Hash::from_slice(&bytes)),
			Ok(Some(_)) | Ok(None) => None,
			Err(error) => {
				tracing::error!(target: LOG_TARGET, ?error, "Auxiliary store read failed");
				None
			},
		}
	}
}

/// Local wall-clock seconds since the unix epoch (0 on a pre-epoch clock —
/// such boundaries are simply pruned at the first horizon sweep).
pub(crate) fn now_secs() -> u64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|elapsed| elapsed.as_secs())
		.unwrap_or(0)
}

fn read_aux<T: Decode, AUX: AuxStore>(aux: &AUX, key: &[u8]) -> Result<Option<T>, ArchiveError> {
	let Some(bytes) = aux.get_aux(key)? else { return Ok(None) };
	T::decode(&mut &bytes[..])
		.map(Some)
		.map_err(|_| ArchiveError::Corrupt("undecodable record"))
}

fn write_aux<AUX: AuxStore>(
	aux: &AUX,
	inserts: Vec<(Vec<u8>, Vec<u8>)>,
	deletes: Vec<Vec<u8>>,
) -> Result<(), ArchiveError> {
	let inserts: Vec<(&[u8], &[u8])> =
		inserts.iter().map(|(key, value)| (&key[..], &value[..])).collect();
	let deletes: Vec<&[u8]> = deletes.iter().map(|key| &key[..]).collect();
	aux.insert_aux(&inserts, &deletes)?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		test_support::*,
		verify::{verify_event_response, verify_messages_response, VerifyError},
	};
	use cumulus_primitives_spec_messaging::test_utils::StreamFixture;

	/// All payloads of `stream` up to `count`, as the import fixtures
	/// produce them.
	fn all_payloads(stream: &StreamId, count: u64) -> Vec<Vec<u8>> {
		(0..count).map(|position| payload(stream, position)).collect()
	}

	/// An independently built fixture over the same payloads — the
	/// cross-check generation path.
	fn fixture(stream: StreamId, count: u64) -> StreamFixture {
		StreamFixture::from_payloads(stream, &all_payloads(&stream, count))
	}

	fn messages_request(
		stream: StreamId,
		start: u64,
		under: StreamsRoot,
		max_bytes: u32,
	) -> MessagesRequest {
		MessagesRequest { stream, start: MessagePosition(start), under, max_bytes }
	}

	#[test]
	fn archive_round_trip_across_restarts() {
		let (aux, mut archive) = new_archive();
		let streams = [channel(2001), channel(2002)];
		let roots = import_blocks(&mut archive, &streams, 5, 2, 1_000);
		let under = roots[4];

		// The full backlog under the head root: payloads reproduced with
		// verifying proofs — trust-free (no frontier) and from an own
		// frontier mid-stream.
		let request = messages_request(streams[0], 0, under, u32::MAX);
		let response = archive.serve_messages(&request).expect("serves under the head root");
		assert_eq!(response.payloads, all_payloads(&streams[0], 10));
		assert_eq!(response.base, MessagePosition(0));
		let verified = verify_messages_response(&request, &response, None).expect("proofs verify");
		assert_eq!(verified.head, 10);
		assert_eq!(verified.end, fixture(streams[0], 10).frontier_at(10));

		let own = fixture(streams[0], 10).frontier_at(4);
		let resume = messages_request(streams[0], 4, under, u32::MAX);
		let resumed = archive.serve_messages(&resume).expect("serves mid-stream");
		assert_eq!(resumed.payloads, all_payloads(&streams[0], 10)[4..].to_vec());
		let verified =
			verify_messages_response(&resume, &resumed, Some(&own)).expect("proofs verify");
		assert_eq!(verified.head, 10);

		// Restart: reload over the same store — byte-identical serving.
		drop(archive);
		let archive: TestArchive = SpecMsgArchive::load(aux).expect("persisted archive loads");
		assert_eq!(archive.tip(), Some((block_hash(5), 5)));
		assert_eq!(archive.serve_messages(&request).expect("still serves"), response);
		assert_eq!(archive.serve_messages(&resume).expect("still serves"), resumed);
	}

	#[test]
	fn serves_under_older_roots() {
		let (_aux, mut archive) = new_archive();
		let streams = [channel(2001), channel(2002)];
		let roots = import_blocks(&mut archive, &streams, 5, 2, 1_000);

		for (index, under) in roots.iter().enumerate() {
			let count = (index as u64 + 1) * 2;
			let request = messages_request(streams[1], 0, *under, u32::MAX);
			let response = archive.serve_messages(&request).expect("serves under a retained root");
			// Capped at the stream's leaf count UNDER THAT ROOT — never a
			// newer payload, and the proofs land on exactly that root.
			assert_eq!(response.payloads, all_payloads(&streams[1], count));
			let verified =
				verify_messages_response(&request, &response, None).expect("proofs verify");
			assert_eq!(verified.head, count);

			// Positions beyond that root's head are refused.
			let beyond = messages_request(streams[1], count + 1, *under, u32::MAX);
			assert_eq!(archive.serve_messages(&beyond), Err(ServeError::BeyondHead));
		}

		// Unknown roots and unknown streams are refused.
		let unknown_root = StreamsRoot(Hash::repeat_byte(0xAA));
		let request = messages_request(streams[0], 0, unknown_root, 0);
		assert_eq!(archive.serve_messages(&request), Err(ServeError::UnknownRoot));
		let request = messages_request(channel(9_999), 0, roots[4], 0);
		assert_eq!(archive.serve_messages(&request), Err(ServeError::UnknownStream));
	}

	#[test]
	fn chunked_refetch_covers_the_backlog() {
		let (_aux, mut archive) = new_archive();
		let stream = channel(2001);
		let roots = import_blocks(&mut archive, &[stream], 6, 3, 1_000);
		let under = roots[5];

		// Payloads are 16 bytes each; 40 bytes of budget = 2 per chunk.
		let mut own = MmrFrontier::default();
		let mut collected = Vec::new();
		let mut chunks = 0;
		loop {
			let request = messages_request(stream, own.leaf_count, under, 40);
			let response = archive.serve_messages(&request).expect("serves the next chunk");
			// Every chunk verifies independently — no trust carried between
			// responses: both from the tracked frontier and trust-free.
			let verified = verify_messages_response(&request, &response, Some(&own))
				.expect("chunk verifies against the tracked frontier");
			assert_eq!(
				verify_messages_response(&request, &response, None)
					.expect("chunk verifies trust-free"),
				verified,
			);
			collected.extend(response.payloads);
			own = verified.end;
			chunks += 1;
			if own.leaf_count == verified.head {
				break;
			}
			assert!(chunks < 64, "chunked fetch must make progress");
		}
		assert_eq!(collected, all_payloads(&stream, 18));
		assert!(chunks >= 9, "the byte budget must actually chunk the backlog");
	}

	#[test]
	fn max_bytes_zero_serves_lift_material() {
		let (_aux, mut archive) = new_archive();
		let streams = [channel(2001), channel(2002)];
		let roots = import_blocks(&mut archive, &streams, 5, 2, 1_000);
		let under = roots[4];
		let fixture = fixture(streams[0], 10);

		// Payload-free from a consumed position: exactly the extension +
		// tree proof pair a requires lift carries.
		let request = messages_request(streams[0], 7, under, 0);
		let response = archive.serve_messages(&request).expect("serves lift material");
		assert!(response.payloads.is_empty());
		assert!(!response.extension.is_empty());
		let verified = verify_messages_response(&request, &response, Some(&fixture.frontier_at(7)))
			.expect("lift material verifies");
		assert_eq!(verified.head, 10);
		assert_eq!(verified.end.leaf_count, 7);

		// Caught up: the empty extension = the tree-proof-only case.
		let request = messages_request(streams[0], 10, under, 0);
		let response = archive.serve_messages(&request).expect("serves the caught-up case");
		assert!(response.payloads.is_empty());
		assert!(response.extension.is_empty());
		let verified =
			verify_messages_response(&request, &response, Some(&fixture.frontier_at(10)))
				.expect("tree-proof-only case verifies");
		assert_eq!(verified.head, 10);
	}

	#[test]
	fn event_head_read_pins_the_head() {
		let (_aux, mut archive) = new_archive();
		let streams = [channel(2001), ack(2001)];
		let roots = import_blocks(&mut archive, &streams, 5, 2, 1_000);
		let (under_old, under_new) = (roots[2], roots[4]);
		let fixture = fixture(streams[1], 10);

		// `at: None` = the head as of `under` — the ack-register read.
		let request = EventRequest { stream: streams[1], under: under_new, at: None };
		let response = archive.serve_event(&request).expect("serves the head");
		assert_eq!(response.payload, payload(&streams[1], 9));
		let verified = verify_event_response(&request, &response).expect("head read verifies");
		assert_eq!(verified.position, MessagePosition(9));
		assert_eq!(verified.frontier, Some(fixture.frontier_at(10)));

		// A specific position under an older root.
		let request_at =
			EventRequest { stream: streams[1], under: under_old, at: Some(MessagePosition(3)) };
		let response_at = archive.serve_event(&request_at).expect("serves the position");
		let verified = verify_event_response(&request_at, &response_at).expect("verifies");
		assert_eq!(verified.position, MessagePosition(3));
		assert_eq!(verified.frontier, None);

		// An old head served as the current head fails the requester's
		// check: `under` pins the leaf count.
		let old_head = EventRequest { stream: streams[1], under: under_old, at: None };
		let old_response = archive.serve_event(&old_head).expect("legitimate under the old root");
		assert_eq!(verify_event_response(&request, &old_response), Err(VerifyError::RootMismatch),);

		// Positions beyond the old root's head are refused, not resolved.
		let beyond =
			EventRequest { stream: streams[1], under: under_old, at: Some(MessagePosition(7)) };
		assert_eq!(archive.serve_event(&beyond), Err(ServeError::BeyondHead));
	}

	#[test]
	fn pruning_watermark_then_horizon() {
		let (aux, mut archive) = new_archive();
		let streams = [channel(2001), channel(2002)];
		let roots = import_blocks(&mut archive, &streams, 8, 2, 10_000);
		let under = roots[7];

		// Watermark pruning: payloads below 6 gone, hashes retained.
		archive
			.prune_payloads(&streams[0], MessagePosition(6))
			.expect("pruning succeeds");

		// Payload requests into the pruned range are refused...
		let request = messages_request(streams[0], 0, under, u32::MAX);
		assert_eq!(archive.serve_messages(&request), Err(ServeError::PayloadsPruned));
		let event = EventRequest { stream: streams[0], under, at: Some(MessagePosition(2)) };
		assert_eq!(archive.serve_event(&event), Err(ServeError::PayloadsPruned));

		// ...but the same range still serves extension material (lifts) and
		// unpruned payloads from the watermark on.
		let lift = messages_request(streams[0], 0, under, 0);
		let response = archive.serve_messages(&lift).expect("extension material still serves");
		verify_messages_response(&lift, &response, None).expect("still verifies");
		let request = messages_request(streams[0], 6, under, u32::MAX);
		let response = archive.serve_messages(&request).expect("unpruned payloads serve");
		assert_eq!(response.payloads, all_payloads(&streams[0], 16)[6..].to_vec());

		// Horizon sweep: boundaries archived before 10_005 (blocks 1-4)
		// drop; the floor advances to block 5's counts (10 messages).
		archive.prune_horizon(10_005).expect("horizon sweep succeeds");

		for old in &roots[..4] {
			let request = messages_request(streams[0], 10, *old, 0);
			assert_eq!(archive.serve_messages(&request), Err(ServeError::UnknownRoot));
		}

		// Material below the horizon floor is refused cleanly (the
		// receiver falls back per liftability's three layers)...
		let below = messages_request(streams[0], 6, under, 0);
		assert_eq!(archive.serve_messages(&below), Err(ServeError::BelowHorizon));

		// ...while any retained block boundary still serves: the boundary
		// frontier was retained at the pruning point.
		for (index, old) in roots.iter().enumerate().skip(4) {
			let count = (index as u64 + 1) * 2;
			let request = messages_request(streams[1], 10, *old, u32::MAX);
			let response = archive.serve_messages(&request).expect("retained boundary serves");
			assert_eq!(response.payloads, all_payloads(&streams[1], count)[10..].to_vec());
			verify_messages_response(&request, &response, None).expect("still verifies");
		}

		// Appending continues over the retained floor, and a restart
		// reloads the pruned state faithfully.
		let sends = vec![(streams[0], vec![payload(&streams[0], 16)])];
		let new_root = archive
			.import_block_at(block_hash(9), block_hash(8), 9, sends, 10_009)
			.expect("extends the tip")
			.expect("streams exist");
		let request = messages_request(streams[0], 10, new_root, u32::MAX);
		let response = archive.serve_messages(&request).expect("serves under the new root");
		assert_eq!(response.payloads.last(), Some(&payload(&streams[0], 16)));
		verify_messages_response(&request, &response, None).expect("verifies");

		drop(archive);
		let archive: TestArchive = SpecMsgArchive::load(aux).expect("pruned archive loads");
		assert_eq!(archive.serve_messages(&request).expect("still serves"), response);
		assert_eq!(archive.serve_messages(&below), Err(ServeError::BelowHorizon));
	}

	#[test]
	fn replay_of_runtime_api_output_reproduces_responses() {
		// The rebuild path for non-executing sync: replaying the same
		// `outbound_messages()` output (fetched or re-executed) into a
		// fresh archive reproduces byte-identical responses — archiving
		// timestamps deliberately differ to pin that they cannot leak into
		// serving.
		let streams = [channel(2001), ack(2001), channel(2002)];
		let (_aux_a, mut a) = new_archive();
		let roots_a = import_blocks(&mut a, &streams, 6, 2, 1_000);
		let (_aux_b, mut b) = new_archive();
		let roots_b = import_blocks(&mut b, &streams, 6, 2, 999_000);
		assert_eq!(roots_a, roots_b);

		for under in roots_a {
			for stream in &streams {
				for (start, max_bytes) in [(0u64, u32::MAX), (3, 40), (5, 0)] {
					let request = messages_request(*stream, start, under, max_bytes);
					assert_eq!(a.serve_messages(&request), b.serve_messages(&request));
				}
				let head = EventRequest { stream: *stream, under, at: None };
				assert_eq!(a.serve_event(&head), b.serve_event(&head));
			}
		}
	}

	#[test]
	fn rewind_replaces_the_reorged_branch() {
		let (aux, mut archive) = new_archive();
		let stream = channel(2001);
		let roots = import_blocks(&mut archive, &[stream], 5, 2, 1_000);

		// Blocks 4 and 5 are reorged away.
		archive.rewind_to(block_hash(3)).expect("block 3 is archived");
		assert_eq!(archive.tip(), Some((block_hash(3), 3)));
		let request = messages_request(stream, 0, roots[4], u32::MAX);
		assert_eq!(archive.serve_messages(&request), Err(ServeError::UnknownRoot));

		// The replacement branch carries different payloads.
		let fork_payloads = vec![b"fork a".to_vec(), b"fork b".to_vec()];
		let fork_root = archive
			.import_block_at(
				block_hash(104),
				block_hash(3),
				4,
				vec![(stream, fork_payloads.clone())],
				1_004,
			)
			.expect("fork block extends the rewound tip")
			.expect("stream exists");
		assert_ne!(fork_root, roots[3]);

		// Serving under the fork root: the shared prefix plus the fork's
		// payloads, verifying against the fork root.
		let request = messages_request(stream, 6, fork_root, u32::MAX);
		let response = archive.serve_messages(&request).expect("serves the fork branch");
		assert_eq!(response.payloads, fork_payloads);
		let verified = verify_messages_response(&request, &response, None).expect("verifies");
		assert_eq!(verified.head, 8);
		// The still-shared root of block 3 remains serveable.
		let request = messages_request(stream, 0, roots[2], u32::MAX);
		verify_messages_response(
			&request,
			&archive.serve_messages(&request).expect("shared prefix serves"),
			None,
		)
		.expect("verifies");

		// Imports must chain: a block not extending the tip is rejected.
		let bad = archive.import_block_at(block_hash(6), block_hash(5), 6, Vec::new(), 1_006);
		assert!(matches!(bad, Err(ArchiveError::NotChild)));

		// The rewound state survives a restart.
		drop(archive);
		let archive: TestArchive = SpecMsgArchive::load(aux).expect("rewound archive loads");
		assert_eq!(archive.tip(), Some((block_hash(104), 4)));
		assert!(archive.serves_root(&fork_root));
		assert!(!archive.serves_root(&roots[4]));
	}
}
