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

//! The sender-side archive: the own chain's sends, keyed for the fetch protocol.
//!
//! - **Per stream:** every MMR node by position and every payload by leaf index, so `mmr_lib` can
//!   build a proof at any historic size.
//! - **Per block:** a `Boundary` with each stream's leaf count and root. The recomputed
//!   `StreamsRoot` is indexed only if it equals the header digest, so a divergence shows as an
//!   unknown root, never a wrong proof.
//! - **Per root:** the block that committed it, to resolve a request's `under`.
//!
//! [`Archive::retain`] prunes channels below their watermark and the rest past the serving horizon.

use crate::{
	store::{ArchiveStore, Batch},
	LOG_TARGET,
};
use codec::{Decode, Encode};
use core::{cell::RefCell, fmt::Debug};
use cumulus_primitives_spec_messaging::{
	leaf_hash,
	mmr::SpecMerge,
	streams_root::{gen_stream_proof, streams_root},
	EventRequest, EventResponse, MMRExtensionProof, MessagesRequest, MessagesResponse,
	MmrInclusionProof, MmrRoot, StreamId, StreamsRoot, LEAF_VERSION, MAX_MESSAGES_PER_RESPONSE,
};
use mmr_lib::{
	helper::{get_peaks, leaf_index_to_mmr_size, leaf_index_to_pos},
	MMRStoreReadOps, MMRStoreWriteOps, MMR,
};
use polkadot_core_primitives::Hash;
use std::collections::BTreeMap;

const PREFIX: &[u8] = b"spec-msg/archive/v1/";

/// Maximum payloads served per response, whatever the byte budget: each costs a store read, so
/// tiny leaves under a large budget would make a 1 KiB request cost many reads. The protocol's
/// decode bound, so a requester never refuses a full response.
pub const MAX_LEAVES_PER_RESPONSE: u64 = MAX_MESSAGES_PER_RESPONSE as u64;

/// Errors of the archive's write paths. Serving never errors: it refuses with `None`.
#[derive(Debug, PartialEq, Eq)]
pub enum ArchiveError {
	/// The block is not a child of the archive's tip.
	NotChild,
	/// A block the operation needs is not archived.
	UnknownBlock,
	/// An MMR node the operation needs is missing.
	MissingNode,
	/// The store refused a write.
	Store(String),
}

/// The archived tip: the newest block the archive reflects.
#[derive(Encode, Decode, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tip<H> {
	/// Block hash.
	pub hash: H,
	/// Block number.
	pub number: u64,
}

/// One stream's state at a block: leaf count and MMR root.
#[derive(Encode, Decode, Clone, Copy, Debug, PartialEq, Eq)]
struct StreamState {
	leaf_count: u64,
	root: Hash,
}

/// The stream states after a block with sends, and the root over them.
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq)]
struct BlockState {
	/// The recomputed `StreamsRoot`, if it matched the header digest. `None` means the block is
	/// archived but not served.
	root: Option<StreamsRoot>,
	/// Every stream with at least one leaf, in `StreamId` order.
	streams: Vec<(StreamId, StreamState)>,
}

impl BlockState {
	fn stream(&self, stream: &StreamId) -> Option<StreamState> {
		self.streams.iter().find(|(s, _)| s == stream).map(|(_, state)| *state)
	}

	fn entries(&self) -> BTreeMap<StreamId, Hash> {
		self.streams.iter().map(|(s, state)| (*s, state.root)).collect()
	}
}

/// What the archive keeps per block. The [`BlockState`] itself is stored once, under the block
/// that had the sends; a block without sends points at it, so the state at any block is two reads
/// away.
#[derive(Encode, Decode, Clone, Debug)]
struct Boundary<H> {
	number: u64,
	parent: H,
	/// Unix seconds at import; the serving horizon counts from it.
	imported_at: u64,
	/// The block whose [`BlockState`] holds this block's stream states: itself if it had sends,
	/// else the nearest ancestor that had them. `None` before any send.
	state_at: Option<H>,
}

/// Per stream: everything below `payload` has no payload, everything below `node` keeps only the
/// peaks at `node`.
#[derive(Encode, Decode, Clone, Copy, Default, Debug, PartialEq, Eq)]
struct Floors {
	payload: u64,
	node: u64,
}

/// The `mmr_lib` node count for `leaf_count` leaves.
fn mmr_size(leaf_count: u64) -> u64 {
	if leaf_count == 0 {
		0
	} else {
		leaf_index_to_mmr_size(leaf_count - 1)
	}
}

fn key(kind: u8, parts: &[&[u8]]) -> Vec<u8> {
	let mut key = PREFIX.to_vec();
	key.push(kind);
	for part in parts {
		key.extend_from_slice(part);
	}
	key
}

fn tip_key() -> Vec<u8> {
	key(b't', &[])
}

fn boundary_key<H: Encode>(hash: &H) -> Vec<u8> {
	key(b'b', &[&hash.encode()])
}

fn root_key(root: &StreamsRoot) -> Vec<u8> {
	key(b'r', &[root.0.as_bytes()])
}

fn node_key(stream: &StreamId, pos: u64) -> Vec<u8> {
	key(b'n', &[&stream.encode(), &pos.to_be_bytes()])
}

fn payload_key(stream: &StreamId, index: u64) -> Vec<u8> {
	key(b'p', &[&stream.encode(), &index.to_be_bytes()])
}

fn floors_key(stream: &StreamId) -> Vec<u8> {
	key(b'f', &[&stream.encode()])
}

fn state_key<H: Encode>(hash: &H) -> Vec<u8> {
	key(b's', &[&hash.encode()])
}

fn canon_key(number: u64) -> Vec<u8> {
	key(b'c', &[&number.to_be_bytes()])
}

fn oldest_key() -> Vec<u8> {
	key(b'o', &[])
}

fn pruned_state_key() -> Vec<u8> {
	key(b'x', &[])
}

/// One stream's MMR nodes, read from the store with an overlay for nodes written but not yet
/// committed.
struct Nodes<'a, S> {
	store: &'a S,
	stream: StreamId,
	pending: RefCell<BTreeMap<u64, Hash>>,
}

impl<'a, S: ArchiveStore> Nodes<'a, S> {
	fn new(store: &'a S, stream: StreamId) -> Self {
		Self { store, stream, pending: Default::default() }
	}

	fn get(&self, pos: u64) -> Option<Hash> {
		if let Some(hash) = self.pending.borrow().get(&pos) {
			return Some(*hash);
		}
		self.store
			.get(&node_key(&self.stream, pos))
			.and_then(|v| Hash::decode(&mut &v[..]).ok())
	}

	/// The peaks of the MMR with `leaf_count` leaves, highest first.
	fn peaks(&self, leaf_count: u64) -> Option<Vec<Hash>> {
		get_peaks(mmr_size(leaf_count)).into_iter().map(|pos| self.get(pos)).collect()
	}

	fn mmr(&self, leaf_count: u64) -> MMR<Hash, SpecMerge, &Self> {
		MMR::new(mmr_size(leaf_count), self)
	}
}

impl<S: ArchiveStore> MMRStoreReadOps<Hash> for &Nodes<'_, S> {
	fn get_elem(&self, pos: u64) -> mmr_lib::Result<Option<Hash>> {
		Ok(self.get(pos))
	}
}

impl<S: ArchiveStore> MMRStoreWriteOps<Hash> for &Nodes<'_, S> {
	fn append(&mut self, pos: u64, elems: Vec<Hash>) -> mmr_lib::Result<()> {
		let mut pending = self.pending.borrow_mut();
		for (i, elem) in elems.into_iter().enumerate() {
			pending.insert(pos + i as u64, elem);
		}
		Ok(())
	}
}

/// The sender-side archive. See the module docs.
pub struct Archive<H, S> {
	store: S,
	tip: Option<Tip<H>>,
	/// Every stream's state at the tip.
	streams: BTreeMap<StreamId, StreamState>,
}

impl<H, S> Archive<H, S>
where
	H: Encode + Decode + Copy + Eq + Debug,
	S: ArchiveStore,
{
	/// Open the archive in `store`, resuming at its persisted tip.
	pub fn open(store: S) -> Self {
		let tip: Option<Tip<H>> = store.get(&tip_key()).and_then(|v| Tip::decode(&mut &v[..]).ok());
		let mut archive = Self { store, tip, streams: BTreeMap::new() };
		if let Some(tip) = tip {
			archive.streams = archive.state_at(&tip.hash).unwrap_or_default();
		}
		archive
	}

	/// The archived tip, if any block was archived.
	pub fn tip(&self) -> Option<Tip<H>> {
		self.tip
	}

	/// Whether `hash` is archived.
	pub fn contains(&self, hash: &H) -> bool {
		self.boundary(hash).is_some()
	}

	fn boundary(&self, hash: &H) -> Option<Boundary<H>> {
		self.store
			.get(&boundary_key(hash))
			.and_then(|v| Boundary::decode(&mut &v[..]).ok())
	}

	fn block_state(&self, hash: &H) -> Option<BlockState> {
		self.store
			.get(&state_key(hash))
			.and_then(|v| BlockState::decode(&mut &v[..]).ok())
	}

	fn canon(&self, number: u64) -> Option<H> {
		self.store.get(&canon_key(number)).and_then(|v| H::decode(&mut &v[..]).ok())
	}

	fn floors(&self, stream: &StreamId) -> Floors {
		self.store
			.get(&floors_key(stream))
			.and_then(|v| Floors::decode(&mut &v[..]).ok())
			.unwrap_or_default()
	}

	/// The stream states after `hash`. `None` if `hash` is not archived; empty for a block before
	/// any send.
	fn state_at(&self, hash: &H) -> Option<BTreeMap<StreamId, StreamState>> {
		let boundary = self.boundary(hash)?;
		let state = boundary.state_at.and_then(|at| self.block_state(&at));
		Some(state.map(|s| s.streams.into_iter().collect()).unwrap_or_default())
	}

	/// Archive `hash`, a child of the tip (or the first block), with its sends and the
	/// `StreamsRoot` its header committed to.
	pub fn import_block(
		&mut self,
		hash: H,
		parent: H,
		number: u64,
		imported_at: u64,
		sends: Vec<(StreamId, Vec<Vec<u8>>)>,
		committed: Option<StreamsRoot>,
	) -> Result<(), ArchiveError> {
		if let Some(tip) = self.tip {
			if tip.hash != parent {
				return Err(ArchiveError::NotChild);
			}
		}

		let mut batch = Batch::default();
		let mut streams = self.streams.clone();
		let mut touched = false;
		for (stream, payloads) in sends {
			if payloads.is_empty() {
				continue;
			}
			touched = true;
			let first = streams.get(&stream).map_or(0, |s| s.leaf_count);
			let nodes = Nodes::new(&self.store, stream);
			let mut mmr = nodes.mmr(first);
			for (i, payload) in payloads.iter().enumerate() {
				mmr.push(leaf_hash(LEAF_VERSION, payload))
					.map_err(|_| ArchiveError::MissingNode)?;
				batch.insert.push((payload_key(&stream, first + i as u64), payload.clone()));
			}
			let leaf_count = first + payloads.len() as u64;
			let bag = mmr.get_root().map_err(|_| ArchiveError::MissingNode)?;
			let root = MmrRoot::from_bag(leaf_count, bag).0;
			mmr.commit().map_err(|_| ArchiveError::MissingNode)?;
			for (pos, node) in nodes.pending.take() {
				batch.insert.push((node_key(&stream, pos), node.encode()));
			}
			streams.insert(stream, StreamState { leaf_count, root });
		}

		let (state, state_at) = if touched {
			let entries = streams.iter().map(|(s, state)| (*s, state.root)).collect();
			let recomputed = streams_root(&entries);
			let root = if recomputed.is_some() && recomputed == committed {
				recomputed
			} else {
				log::warn!(
					target: LOG_TARGET,
					"Block {number} ({hash:?}): recomputed StreamsRoot {recomputed:?} differs from \
					 the header's {committed:?}; archived but not served",
				);
				None
			};
			(Some(BlockState { root, streams: streams.clone().into_iter().collect() }), Some(hash))
		} else {
			if committed.is_some() {
				log::warn!(
					target: LOG_TARGET,
					"Block {number} ({hash:?}) commits a StreamsRoot but sent nothing",
				);
			}
			let from = self.tip.and_then(|tip| self.boundary(&tip.hash)).and_then(|b| b.state_at);
			(None, from)
		};

		if let Some(state) = state {
			if let Some(root) = state.root {
				batch.insert.push((root_key(&root), hash.encode()));
			}
			batch.insert.push((state_key(&hash), state.encode()));
		}
		let boundary = Boundary { number, parent, imported_at, state_at };
		batch.insert.push((boundary_key(&hash), boundary.encode()));
		let tip = Tip { hash, number };
		batch.insert.push((tip_key(), tip.encode()));
		self.store.commit(batch).map_err(ArchiveError::Store)?;

		self.tip = Some(tip);
		self.streams = streams;
		Ok(())
	}

	/// Rewind to `ancestor` after a reorg: forget every block above it and every leaf they added.
	/// `ancestor` must be the tip or one of its archived ancestors, or the parent of the first
	/// archived block (the archive is then empty).
	pub fn rewind_to(&mut self, ancestor: H, ancestor_number: u64) -> Result<(), ArchiveError> {
		let Some(tip) = self.tip else { return Err(ArchiveError::UnknownBlock) };
		let mut batch = Batch::default();
		let mut current = tip.hash;
		let mut first_parent = None;
		while current != ancestor {
			let boundary = self.boundary(&current).ok_or(ArchiveError::UnknownBlock)?;
			batch.delete.push(boundary_key(&current));
			if boundary.state_at == Some(current) {
				if let Some(root) = self.block_state(&current).and_then(|s| s.root) {
					batch.delete.push(root_key(&root));
				}
				batch.delete.push(state_key(&current));
			}
			first_parent = Some(boundary.parent);
			current = boundary.parent;
			if boundary.number <= ancestor_number {
				break;
			}
		}
		if current != ancestor {
			return Err(ArchiveError::UnknownBlock);
		}

		// The ancestor is archived, or it is the parent of the first archived block.
		let (streams, new_tip) = match self.state_at(&ancestor) {
			Some(streams) => (streams, Some(Tip { hash: ancestor, number: ancestor_number })),
			None if first_parent == Some(ancestor) => (BTreeMap::new(), None),
			None => return Err(ArchiveError::UnknownBlock),
		};
		for (stream, state) in &self.streams {
			let keep = streams.get(stream).map_or(0, |s| s.leaf_count);
			for pos in mmr_size(keep)..mmr_size(state.leaf_count) {
				batch.delete.push(node_key(stream, pos));
			}
			for index in keep..state.leaf_count {
				batch.delete.push(payload_key(stream, index));
			}
		}
		match new_tip {
			Some(tip) => batch.insert.push((tip_key(), tip.encode())),
			None => batch.delete.push(tip_key()),
		}
		self.store.commit(batch).map_err(ArchiveError::Store)?;

		self.tip = new_tip;
		self.streams = streams;
		Ok(())
	}

	/// Resolve a request's `under` to the stream states it commits to.
	fn resolve(&self, under: &StreamsRoot) -> Option<BlockState> {
		let hash = self.store.get(&root_key(under)).and_then(|v| H::decode(&mut &v[..]).ok())?;
		let state = self.block_state(&hash)?;
		(state.root == Some(*under)).then_some(state)
	}

	/// Serve a [`MessagesRequest`] with at most `payload_cap` payload bytes. `None` refuses: the
	/// root is unknown, the stream absent under it, the start past its leaf count, or the range
	/// pruned.
	pub fn serve_messages(
		&self,
		req: &MessagesRequest,
		payload_cap: u32,
	) -> Option<MessagesResponse> {
		let state = self.resolve(&req.under)?;
		let leaf_count = state.stream(&req.stream)?.leaf_count;
		let start = req.start.0;
		if start > leaf_count {
			return None;
		}

		let budget = u64::from(req.max_bytes.min(payload_cap));
		let mut payloads = Vec::new();
		if budget > 0 && start < leaf_count {
			if start < self.floors(&req.stream).payload {
				return None;
			}
			let mut used = 0u64;
			for index in start..leaf_count.min(start.saturating_add(MAX_LEAVES_PER_RESPONSE)) {
				let payload = self.store.get(&payload_key(&req.stream, index))?;
				used += payload.len() as u64;
				if used > budget {
					break;
				}
				payloads.push(payload);
			}
		}
		let end = start + payloads.len() as u64;
		let payloads = payloads.try_into().ok()?;

		let nodes = Nodes::new(&self.store, req.stream);
		let start_peaks = nodes.peaks(start)?.try_into().ok()?;
		let extension = Self::extension(&nodes, end, leaf_count)?;
		let (_, tree_proof) = gen_stream_proof(&state.entries(), req.stream)?;
		Some(MessagesResponse {
			base: req.start,
			leaf_version: LEAF_VERSION,
			payloads,
			start_peaks,
			extension,
			tree_proof,
		})
	}

	/// The extension proof from `from` to `to` leaves.
	fn extension(nodes: &Nodes<'_, S>, from: u64, to: u64) -> Option<MMRExtensionProof> {
		if from == to {
			return Some(MMRExtensionProof::identity());
		}
		if from == 0 {
			// Nothing to extend from: the connecting nodes are the new MMR's peaks.
			return Some(MMRExtensionProof {
				leaf_count: to,
				connecting_nodes: nodes.peaks(to)?.try_into().ok()?,
			});
		}
		let proof = nodes.mmr(to).gen_ancestry_proof(mmr_size(from)).ok()?;
		Some(MMRExtensionProof {
			leaf_count: to,
			connecting_nodes: proof
				.prev_peaks_proof
				.proof_items()
				.iter()
				.map(|(_, h)| *h)
				.collect::<Vec<_>>()
				.try_into()
				.ok()?,
		})
	}

	/// Serve an [`EventRequest`]: the head under `under`, or the leaf at `at`. `None` refuses.
	pub fn serve_event(&self, req: &EventRequest) -> Option<EventResponse> {
		let state = self.resolve(&req.under)?;
		let leaf_count = state.stream(&req.stream)?.leaf_count;
		let index = match req.at {
			Some(at) => at.0,
			None => leaf_count.checked_sub(1)?,
		};
		if index >= leaf_count {
			return None;
		}
		let payload = self.store.get(&payload_key(&req.stream, index))?;
		let nodes = Nodes::new(&self.store, req.stream);
		let proof = nodes.mmr(leaf_count).gen_proof(vec![leaf_index_to_pos(index)]).ok()?;
		let inclusion = MmrInclusionProof {
			mmr_size: mmr_size(leaf_count),
			items: proof.proof_items().to_vec().try_into().ok()?,
		};
		let (_, tree_proof) = gen_stream_proof(&state.entries(), req.stream)?;
		Some(EventResponse { payload, leaf_version: LEAF_VERSION, inclusion, tree_proof })
	}

	/// Apply retention at `finalized`: drop boundaries imported before `horizon_cutoff` (never
	/// `finalized` itself); prune each channel below its watermark (none: keep all); keep every
	/// other stream from the oldest kept boundary's head. `watermarks` must be read at `finalized`,
	/// so a reverted read never prunes.
	pub fn retain(
		&mut self,
		finalized: H,
		watermarks: &BTreeMap<StreamId, u64>,
		horizon_cutoff: u64,
	) -> Result<(), ArchiveError> {
		let top = self.boundary(&finalized).ok_or(ArchiveError::UnknownBlock)?.number;
		let mut oldest = self.store.get(&oldest_key()).and_then(|v| u64::decode(&mut &v[..]).ok());

		// Index the newly finalized blocks by number, walking back to the last indexed one.
		let mut batch = Batch::default();
		let (mut current, mut number) = (finalized, top);
		while self.canon(number).is_none() {
			batch.insert.push((canon_key(number), current.encode()));
			oldest = Some(oldest.map_or(number, |o| o.min(number)));
			let Some(boundary) = self.boundary(&current) else { break };
			if number == 0 || !self.contains(&boundary.parent) {
				break;
			}
			(current, number) = (boundary.parent, number - 1);
		}
		let mut oldest = oldest.unwrap_or(top);
		// The pruning below reads the index.
		self.store.commit(core::mem::take(&mut batch)).map_err(ArchiveError::Store)?;

		// Drop boundaries past the horizon, oldest first. A dropped block's state may still be
		// what later blocks without sends point at, so the newest dropped state is kept and the
		// one before it deleted: anything pointing at that one is older, hence dropped already.
		let mut pruned_state: Option<H> =
			self.store.get(&pruned_state_key()).and_then(|v| H::decode(&mut &v[..]).ok());
		while oldest < top {
			let Some(hash) = self.canon(oldest) else { break };
			let Some(boundary) = self.boundary(&hash) else { break };
			if boundary.imported_at >= horizon_cutoff {
				break;
			}
			batch.delete.push(boundary_key(&hash));
			batch.delete.push(canon_key(oldest));
			if boundary.state_at == Some(hash) {
				if let Some(root) = self.block_state(&hash).and_then(|s| s.root) {
					batch.delete.push(root_key(&root));
				}
				if let Some(previous) = pruned_state.replace(hash) {
					batch.delete.push(state_key(&previous));
				}
			}
			oldest += 1;
		}
		batch.insert.push((oldest_key(), oldest.encode()));
		if let Some(hash) = pruned_state {
			batch.insert.push((pruned_state_key(), hash.encode()));
		}
		self.store.commit(batch).map_err(ArchiveError::Store)?;

		// Raise the floors.
		let mut batch = Batch::default();
		let kept = self.canon(oldest).and_then(|hash| self.state_at(&hash)).unwrap_or_default();
		let at_finalized = self.state_at(&finalized).unwrap_or_default();
		for (stream, state) in &at_finalized {
			let target = match stream {
				StreamId::Channel { .. } => {
					watermarks.get(stream).copied().unwrap_or(0).min(state.leaf_count)
				},
				// The head under the oldest kept root stays servable, with its inclusion path.
				_ => kept.get(stream).map_or(0, |s| s.leaf_count.saturating_sub(1)),
			};
			self.raise_floor(&mut batch, stream, target);
		}
		self.store.commit(batch).map_err(ArchiveError::Store)
	}

	/// Drop `stream`'s payloads below `target`, and its nodes below `target` except the peaks
	/// there. Floors only rise.
	fn raise_floor(&self, batch: &mut Batch, stream: &StreamId, target: u64) {
		let floors = self.floors(stream);
		if target <= floors.node && target <= floors.payload {
			return;
		}
		for index in floors.payload..target {
			batch.delete.push(payload_key(stream, index));
		}
		if target > floors.node {
			// Nodes below the old floor are gone already, except its peaks.
			let keep: Vec<u64> = get_peaks(mmr_size(target));
			let candidates = get_peaks(mmr_size(floors.node))
				.into_iter()
				.chain(mmr_size(floors.node)..mmr_size(target));
			for pos in candidates {
				if !keep.contains(&pos) {
					batch.delete.push(node_key(stream, pos));
				}
			}
		}
		let floors = Floors { payload: floors.payload.max(target), node: floors.node.max(target) };
		batch.insert.push((floors_key(stream), floors.encode()));
	}
}
