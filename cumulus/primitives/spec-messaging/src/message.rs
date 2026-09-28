// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Off-chain wire types and their verifiers.
//!
//! The relay chain decodes none of this; it only matches the `StreamsRoot` commitments in the UMP
//! signals. But the two ends of each protocol are different chains' collators, so the encodings and
//! the envelope variant indices are part of the wire format.
//!
//! - [`leaf_hash`] turns a payload into an MMR leaf. A v0.5 message is a bounded payload keyed by
//! `StreamId`; source, stream and position are structural, so there is no message struct.
//! - Fetch: [`MessagesRequest`] / [`MessagesResponse`], verified by [`verify_messages`] against a
//! `StreamsRoot` the requester has already verified. The `extension` + `tree_proof` check is the
//! same one the requires-lift performs.
//! - Event read: [`EventRequest`] / [`EventResponse`], the single-leaf read of an `Ack` /
//! `Broadcast` / `Private` stream, verified by [`verify_event`].
//! - Envelope: [`ExchangeRequest`] / [`ExchangeResponse`] multiplex both over `/spec-msg/exchange`,
//! verified by [`verify_exchange`], which also binds the response variant to the request's. Variant
//! indices are frozen.

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode};
use polkadot_core_primitives::Hash;
use scale_info::TypeInfo;
use sp_core::ConstU32;
use sp_runtime::traits::Hash as HashT;

use crate::{
	lift::{MMRExtensionProof, MmrInclusionProof, ProofError},
	mmr::{MessagePosition, MmrFrontier},
	stream::StreamId,
	streams_root::{streams_root_from_proof, StreamProof, StreamsRoot},
	SpecHasher, LEAF_TAG,
};

/// Why verifying an off-chain response failed. [`RootMismatch`](VerifyError::RootMismatch) is
/// benign (retry under a fresher root); the rest indicate a malformed or forged response.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VerifyError {
	/// An embedded MMR proof (inclusion or extension) did not verify.
	Proof(ProofError),
	/// The `StreamsRoot` keyed-tree walk (`tree_proof`) did not resolve.
	TreeProof,
	/// Everything verified, but the recomputed `StreamsRoot` is not the requested `under`. The
	/// peer served under a different root, stale or wrong. Not necessarily malicious.
	RootMismatch,
	/// The response's `base` is not the requested `start`. Only surfaced by [`verify_messages`].
	UnexpectedBase,
	/// The payloads total more than the request's `max_bytes`. With `max_bytes == 0` any payload
	/// exceeds it. Only surfaced by [`verify_messages`].
	ExceedsBudget,
	/// The response envelope variant does not match the request's. Only surfaced by
	/// [`verify_exchange`].
	VariantMismatch,
	/// A payload exceeded the protocol's single-message bound ([`MAX_SPECULATIVE_MESSAGE_LEN`]).
	PayloadTooLarge,
	/// The response is structurally malformed independently of its proofs (e.g. a `start_peaks`
	/// set inconsistent with `base`, or a `base + len` leaf-counter overflow).
	MalformedResponse,
}

impl From<ProofError> for VerifyError {
	fn from(e: ProofError) -> Self {
		VerifyError::Proof(e)
	}
}

/// Maximum size in bytes of a single speculative message payload.
pub const MAX_SPECULATIVE_MESSAGE_LEN: u32 = 102_400;

/// Bound for a single speculative message payload.
pub type MaxSpeculativeMessageLen = ConstU32<MAX_SPECULATIVE_MESSAGE_LEN>;

/// Hash a payload into a stream MMR leaf: `H(LEAF_TAG ++ leaf_version ++ payload)`. Source, stream
/// and position are structural and not in the preimage. Versions are hash-disjoint, so only the
/// correct one reproduces a committed root.
pub fn leaf_hash(leaf_version: u8, payload: &[u8]) -> Hash {
	let mut preimage = Vec::new();
	preimage.extend_from_slice(&LEAF_TAG.to_le_bytes());
	preimage.extend_from_slice(&leaf_version.to_le_bytes());
	preimage.extend_from_slice(payload);
	<SpecHasher as HashT>::hash(&preimage)
}

/// Request for a range of a stream's messages, verified under a chosen `StreamsRoot`.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub struct MessagesRequest {
	/// The requested stream of the serving chain.
	pub stream: StreamId,
	/// Where to start; typically the receiver's frontier leaf count.
	pub start: MessagePosition,
	/// The `StreamsRoot` the response must verify under: one the requester has already
	/// authenticated. Freshness is the requester's job; re-request under a newer root once
	/// verified.
	pub under: StreamsRoot,
	/// Response size bound; the server may cap harder. `0` requests a payload-free proof (lift
	/// material).
	pub max_bytes: u32,
}

/// Response: payloads from `base` on, plus the proofs binding them and everything before them to
/// the requested `StreamsRoot`. Nothing is trusted; fabricated peaks, payloads or proofs cannot
/// reproduce a committed root.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub struct MessagesResponse {
	/// Position of the first payload. A hint; a lie fails the proofs.
	pub base: MessagePosition,
	/// Leaf-format version for these payloads. A hint; only the correct version reproduces a
	/// committed root. A response never spans a version change.
	pub leaf_version: u8,
	/// The payloads, in MMR order; payload `i` has position `base + i`.
	pub payloads: Vec<Vec<u8>>,
	/// The stream's peak set at `base`, at most 64 hashes, for a consumer that holds no frontier.
	/// Fabricated peaks cannot extend to a committed root.
	pub start_peaks: Vec<Hash>,
	/// From the frontier recomputed over `payloads` to the stream's current root; the identity
	/// extension when the payloads already reach it.
	pub extension: MMRExtensionProof,
	/// From the stream's root to the `StreamsRoot` the response verifies under.
	pub tree_proof: StreamProof,
}

/// Verify a [`MessagesResponse`] against its [`MessagesRequest`]: `resp.base == req.start`, total
/// payload bytes within `req.max_bytes` (`0` means proofs only), then the payloads under
/// `req.stream` / `req.under`. The public entry for the fetch protocol.
pub fn verify_messages(
	req: &MessagesRequest,
	resp: &MessagesResponse,
) -> Result<Vec<Vec<u8>>, VerifyError> {
	if resp.base != req.start {
		return Err(VerifyError::UnexpectedBase);
	}
	// Total payload bytes must fit the requester's budget (the server may cap harder, i.e. return
	// less). `max_bytes == 0` requests proofs only, so any payload trips this.
	let total = resp.payloads.iter().map(|p| p.len() as u64).fold(0u64, u64::saturating_add);
	if total > req.max_bytes as u64 {
		return Err(VerifyError::ExceedsBudget);
	}
	verify_messages_response(req.stream, req.under, resp)
}

/// Verify a [`MessagesResponse`] for `stream` under `under` and return the payloads.
/// Crate-internal: it does not bind `resp.base` to a requested start; [`verify_messages`] does.
///
/// Recomputes the frontier from `start_peaks` and the hashed payloads, extends it to the stream's
/// current root, and walks the tree to a `StreamsRoot` that must equal `under`. The same check the
/// requires-lift performs.
pub(crate) fn verify_messages_response(
	stream: StreamId,
	under: StreamsRoot,
	resp: &MessagesResponse,
) -> Result<Vec<Vec<u8>>, VerifyError> {
	// `start_peaks` and `base` are untrusted. `MmrFrontier::from_parts` rejects an inconsistent
	// shape (one peak per set bit of `base`, which also caps `start_peaks` at 64) and a `base` past
	// `MAX_MMR_LEAF_COUNT`, so a crafted response can neither make `append` pop from an empty vec
	// nor reach the node-position arithmetic. A rejected response is unverifiable, without a panic.
	let mut frontier = MmrFrontier::from_parts(resp.start_peaks.clone(), resp.base.0)
		.ok_or(VerifyError::MalformedResponse)?;
	// `base + len` overflowing the leaf counter is unreachable under the ceiling. Kept so the
	// accumulator arithmetic is total on its own.
	resp.base
		.0
		.checked_add(resp.payloads.len() as u64)
		.ok_or(VerifyError::MalformedResponse)?;
	// No payload may exceed the protocol's single-message bound (defense-in-depth against a crafted
	// response inflating the hashed input; the transport also caps total response size).
	if resp
		.payloads
		.iter()
		.any(|p| p.len() as u64 > MAX_SPECULATIVE_MESSAGE_LEN as u64)
	{
		return Err(VerifyError::PayloadTooLarge);
	}

	// Append the response's payloads as leaves onto the frontier at `base`.
	for payload in &resp.payloads {
		frontier.append(leaf_hash(resp.leaf_version, payload));
	}

	// Extend to the stream's current root, then walk the tree to the committed `StreamsRoot`.
	let current = resp.extension.verify(&frontier)?;
	let root = streams_root_from_proof(stream, current.0, &resp.tree_proof)
		.ok_or(VerifyError::TreeProof)?;
	(root == under).then(|| resp.payloads.clone()).ok_or(VerifyError::RootMismatch)
}

/// Request for a single event of a lossy stream (`Ack` / `Broadcast` / `Private`), verified under a
/// chosen `StreamsRoot`. The server either serves or fails; nothing is resolved server-side.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub struct EventRequest {
	/// The requested stream of the serving chain.
	pub stream: StreamId,
	/// The `StreamsRoot` the response must verify under. Always explicit.
	pub under: StreamsRoot,
	/// A specific position, or `None` for the head as of `under`.
	pub at: Option<MessagePosition>,
}

/// Response carrying one event leaf and the proofs placing it under the requested `StreamsRoot`.
/// Nothing is trusted. `under` fixes the stream's leaf count, so the head is the leaf at `count -
/// 1`; an old leaf cannot be served as the head.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub struct EventResponse {
	/// The event payload; its position is proven by `inclusion`.
	pub payload: Vec<u8>,
	/// Leaf-format version for `payload`. A hint; only the correct version reproduces a committed
	/// root.
	pub leaf_version: u8,
	/// Single-leaf inclusion proof: the sibling path plus the other peaks.
	pub inclusion: MmrInclusionProof,
	/// From the stream's root (derived from `inclusion`) to the `StreamsRoot` the response
	/// verifies under.
	pub tree_proof: StreamProof,
}

/// The `/spec-msg/exchange` request envelope. Variant indices are frozen; foreign implementations
/// decode by them.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub enum ExchangeRequest {
	/// Ordered fetching or lift material. Answered by [`ExchangeResponse::Messages`].
	#[codec(index = 0)]
	Messages(MessagesRequest),
	/// Single-event read. Answered by [`ExchangeResponse::Event`].
	#[codec(index = 1)]
	Event(EventRequest),
}

/// The `/spec-msg/exchange` response envelope. No error variant: a server that cannot serve refuses
/// at the transport level.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub enum ExchangeResponse {
	/// Response to [`ExchangeRequest::Messages`].
	#[codec(index = 0)]
	Messages(MessagesResponse),
	/// Response to [`ExchangeRequest::Event`].
	#[codec(index = 1)]
	Event(EventResponse),
}

/// Outcome of verifying an [`EventResponse`]. A head read yields the stream [`MmrFrontier`], which
/// lossy consumption records as its interval endpoint; a positional read yields only the payload,
/// since an inclusion proof reconstructs a root, not a frontier.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum VerifiedEvent {
	/// Head (register / latest-event) read: the head position, the stream frontier at that head,
	/// and the payload.
	Head { position: MessagePosition, frontier: MmrFrontier, payload: Vec<u8> },
	/// Positional read: the requested position and its payload.
	Positional { position: MessagePosition, payload: Vec<u8> },
}

/// Verify an [`EventResponse`] against its [`EventRequest`]: `req.at` selects head (`None`) or leaf
/// `k`, so a head response is never accepted for a positional request or vice versa. The public
/// entry for event reads.
pub fn verify_event(
	req: &EventRequest,
	resp: &EventResponse,
) -> Result<VerifiedEvent, VerifyError> {
	match req.at {
		None => {
			let (position, frontier, payload) = verify_event_response(req.stream, req.under, resp)?;
			Ok(VerifiedEvent::Head { position, frontier, payload })
		},
		Some(position) => {
			let payload = verify_positional_event_response(req.stream, req.under, position, resp)?;
			Ok(VerifiedEvent::Positional { position, payload })
		},
	}
}

/// Verify an [`EventResponse`] as the head of `stream` under `under`; returns the position, the
/// frontier at that head, and the payload. Crate-internal; [`verify_event`] is the public entry.
///
/// Hashes the payload, verifies it as the last leaf ([`MmrInclusionProof::verify_head`]), bags the
/// frontier to the stream root, and walks the tree to a `StreamsRoot` that must equal `under`. The
/// frontier is returned because lossy consumption needs it as the interval endpoint and it is not
/// reconstructible from the response alone.
pub(crate) fn verify_event_response(
	stream: StreamId,
	under: StreamsRoot,
	resp: &EventResponse,
) -> Result<(MessagePosition, MmrFrontier, Vec<u8>), VerifyError> {
	if resp.payload.len() as u64 > MAX_SPECULATIVE_MESSAGE_LEN as u64 {
		return Err(VerifyError::PayloadTooLarge);
	}
	let leaf = leaf_hash(resp.leaf_version, &resp.payload);
	let (position, frontier) = resp.inclusion.verify_head(leaf)?;
	let root = frontier.root();
	let streams_root =
		streams_root_from_proof(stream, root.0, &resp.tree_proof).ok_or(VerifyError::TreeProof)?;
	(streams_root == under)
		.then(|| (position, frontier, resp.payload.clone()))
		.ok_or(VerifyError::RootMismatch)
}

/// Verify an [`EventResponse`] as the leaf at `position` of `stream` under `under`; returns the
/// payload. Crate-internal; [`verify_event`] is the public entry. Same chain as the head read, with
/// [`MmrInclusionProof::verify_leaf`].
pub(crate) fn verify_positional_event_response(
	stream: StreamId,
	under: StreamsRoot,
	position: MessagePosition,
	resp: &EventResponse,
) -> Result<Vec<u8>, VerifyError> {
	if resp.payload.len() as u64 > MAX_SPECULATIVE_MESSAGE_LEN as u64 {
		return Err(VerifyError::PayloadTooLarge);
	}
	let leaf = leaf_hash(resp.leaf_version, &resp.payload);
	let root = resp.inclusion.verify_leaf(position, leaf)?;
	let streams_root =
		streams_root_from_proof(stream, root.0, &resp.tree_proof).ok_or(VerifyError::TreeProof)?;
	(streams_root == under)
		.then(|| resp.payload.clone())
		.ok_or(VerifyError::RootMismatch)
}

/// Verified outcome of an [`ExchangeResponse`]: the payloads of a fetch, or a [`VerifiedEvent`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ExchangeVerified {
	/// Answer to an [`ExchangeRequest::Messages`]: the verified payloads.
	Messages(Vec<Vec<u8>>),
	/// Answer to an [`ExchangeRequest::Event`]: the verified event.
	Event(VerifiedEvent),
}

/// Verify a `/spec-msg/exchange` response against its request. The variants must match
/// ([`VariantMismatch`](VerifyError::VariantMismatch) otherwise); then dispatches to
/// [`verify_messages`] or [`verify_event`]. The top-level public entry.
pub fn verify_exchange(
	req: &ExchangeRequest,
	resp: &ExchangeResponse,
) -> Result<ExchangeVerified, VerifyError> {
	match (req, resp) {
		(ExchangeRequest::Messages(mreq), ExchangeResponse::Messages(mresp)) => {
			verify_messages(mreq, mresp).map(ExchangeVerified::Messages)
		},
		(ExchangeRequest::Event(ereq), ExchangeResponse::Event(eresp)) => {
			verify_event(ereq, eresp).map(ExchangeVerified::Event)
		},
		_ => Err(VerifyError::VariantMismatch),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		lift::{
			build_requires, ConsumptionRecord, Interval, LiftsBySource, MmrInclusionProof,
			RequiresLift,
		},
		mmr::{root_from_peaks, SpecMerge},
		streams_root::{gen_stream_proof, streams_root, TreeStep},
	};
	use alloc::collections::BTreeMap;
	use codec::Encode;
	use mmr_lib::{
		leaf_index_to_mmr_size,
		util::{MemMMR, MemStore},
	};
	use polkadot_parachain_primitives::primitives::Id as ParaId;
	use sp_core::H256;

	fn ch(recipient: u32) -> StreamId {
		StreamId::Channel { recipient: recipient.into(), domain: 0, num: 0 }
	}

	fn mmr_size(k: usize) -> u64 {
		if k == 0 {
			0
		} else {
			leaf_index_to_mmr_size((k - 1) as u64)
		}
	}

	/// Peaks-only frontier after the first `k` leaves.
	fn frontier_at(leaves: &[Hash], k: usize) -> MmrFrontier {
		let mut frontier = MmrFrontier::new();
		for l in &leaves[..k] {
			frontier.append(*l);
		}
		frontier
	}

	/// The stream root over the first `n` leaves.
	fn current_root(leaves: &[Hash], n: usize) -> Hash {
		let store = MemStore::<Hash>::default();
		let mut mmr = MemMMR::<Hash, SpecMerge>::new(0, &store);
		for l in &leaves[..n] {
			mmr.push(*l).unwrap();
		}
		mmr.get_root().unwrap()
	}

	/// O(log n) ancestry extension from `k` leaves to `n` leaves.
	fn ancestry(leaves: &[Hash], k: usize, n: usize) -> MMRExtensionProof {
		let store = MemStore::<Hash>::default();
		let mut mmr = MemMMR::<Hash, SpecMerge>::new(0, &store);
		for l in &leaves[..n] {
			mmr.push(*l).unwrap();
		}
		let ap = mmr.gen_ancestry_proof(mmr_size(k)).unwrap();
		MMRExtensionProof {
			leaf_count: n as u64,
			connecting_nodes: ap.prev_peaks_proof.proof_items().iter().map(|(_, h)| *h).collect(),
		}
	}

	/// End-to-end composition, exercising the non-trivial branches (`base > 0`, a *real* extension
	/// bridging an unconsumed tail): a sender stream of 6 messages commits a `StreamsRoot`; a
	/// partial fetch (base 2, messages 2..3, extension bridging 4→6) verifies under that root;
	/// and the receiver's consumption of those messages lifts (the same 4→6 extension +
	/// `tree_proof`) into the same `StreamsRoot` via `build_requires`. Fetch and requires run
	/// the same `extension` + `tree_proof` check, so this shows they compose.
	#[test]
	fn composition_fetch_verify_then_build_requires() {
		let source = ParaId::from(1000);
		let stream = ch(2000);

		let payloads: Vec<Vec<u8>> = (0..6u8).map(|i| vec![i, i.wrapping_add(10)]).collect();
		let leaves: Vec<Hash> = payloads.iter().map(|p| leaf_hash(0, p)).collect();

		// Sender's current stream root (6 leaves) and the StreamsRoot committing it.
		let r_current = current_root(&leaves, 6);
		let under = streams_root(&BTreeMap::from([(stream, r_current)])).unwrap();
		let (_r, tree_proof) =
			gen_stream_proof(&BTreeMap::from([(stream, r_current)]), stream).unwrap();

		// Fetch: base 2 (non-empty start_peaks), payloads [2,3] only → a real extension (4 → 6).
		let resp = MessagesResponse {
			base: MessagePosition(2),
			leaf_version: 0,
			payloads: vec![payloads[2].clone(), payloads[3].clone()],
			start_peaks: frontier_at(&leaves, 2).peaks().to_vec(),
			extension: ancestry(&leaves, 4, 6),
			tree_proof: tree_proof.clone(),
		};
		assert_eq!(
			verify_messages_response(stream, under, &resp),
			Ok(vec![payloads[2].clone(), payloads[3].clone()]),
		);

		// Requires: receiver consumed messages 2,3 (frontier 2 → 4); lift the endpoint (4 → 6).
		let record = ConsumptionRecord {
			entries: BTreeMap::from([(
				source,
				BTreeMap::from([(
					stream,
					Interval {
						start: frontier_at(&leaves, 2).root(),
						end: frontier_at(&leaves, 4),
					},
				)]),
			)]),
		};
		let mut lifts = BTreeMap::new();
		lifts.insert(
			source,
			vec![RequiresLift {
				advances: Vec::new(),
				extension: ancestry(&leaves, 4, 6),
				tree_proof,
			}],
		);

		let lifts = LiftsBySource::try_from(lifts).unwrap();
		let requires = build_requires(&[record], &lifts).unwrap().expect("consumed, so a set");
		// The fetch and the requires resolve to the SAME committed StreamsRoot.
		assert_eq!(requires.get(source), Some(&under));
	}

	#[test]
	fn leaf_hash_is_payload_only_and_version_disjoint() {
		let payload = b"hello";
		// Matches the explicit preimage `H(LEAF_TAG ++ version ++ payload)`.
		let mut pre = Vec::new();
		pre.push(LEAF_TAG);
		pre.push(0u8);
		pre.extend_from_slice(payload);
		assert_eq!(leaf_hash(0, payload), <SpecHasher as HashT>::hash(&pre));
		// Different version → different leaf (hash-disjoint domains).
		assert_ne!(leaf_hash(0, payload), leaf_hash(1, payload));
	}

	#[test]
	fn leaf_hash_frozen_vector() {
		// FROZEN consensus vector (encoding spec §3.1): `H(0x01 ‖ 0x00 ‖ "hello")`. Any change to
		// `LEAF_TAG`, `LEAF_VERSION`, the preimage layout or the hasher breaks this.
		assert_eq!(
			leaf_hash(crate::LEAF_VERSION, b"hello"),
			Hash::from(hex_literal::hex!(
				"cd31917fb8992dae762dbaaf276d8eb65aa89cdfb87daf69e05f8c08b490e78b"
			))
		);
	}

	/// End-to-end: a sender builds a stream MMR, commits its root into a `StreamsRoot`, and serves
	/// a response covering the whole stream from `base = 0`; the receiver authenticates it under
	/// the committed root and recovers the payloads.
	#[test]
	fn messages_response_round_trips_under_streams_root() {
		let payloads: Vec<Vec<u8>> = (0..4u8).map(|i| vec![i, i + 1, i + 2]).collect();

		// Sender stream MMR over the payload leaves; its root is the stream root.
		let store = MemStore::<Hash>::default();
		let mut src = MemMMR::<Hash, SpecMerge>::new(0, &store);
		for p in &payloads {
			src.push(leaf_hash(0, p)).unwrap();
		}
		let stream_root = src.get_root().unwrap();

		let stream = ch(2000);
		let entries = BTreeMap::from([(stream, stream_root)]);
		let under = streams_root(&entries).unwrap();
		let (_r, tree_proof) = gen_stream_proof(&entries, stream).unwrap();

		// Full stream from base 0: empty start_peaks, identity extension (payloads reach the root).
		let resp = MessagesResponse {
			base: MessagePosition(0),
			leaf_version: 0,
			payloads: payloads.clone(),
			start_peaks: Vec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof,
		};

		assert_eq!(verify_messages_response(stream, under, &resp), Ok(payloads));
		// Sanity: the recomputed root matches the sender's.
		let mut mmr = MmrFrontier::new();
		for p in &resp.payloads {
			mmr.append(leaf_hash(0, p));
		}
		assert_eq!(root_from_peaks(mmr.peaks()), Some(stream_root));
	}

	#[test]
	fn verify_messages_binds_base_to_request_start() {
		let payloads: Vec<Vec<u8>> = (0..4u8).map(|i| vec![i, i + 1]).collect();
		let store = MemStore::<Hash>::default();
		let mut src = MemMMR::<Hash, SpecMerge>::new(0, &store);
		for p in &payloads {
			src.push(leaf_hash(0, p)).unwrap();
		}
		let stream = ch(2000);
		let entries = BTreeMap::from([(stream, src.get_root().unwrap())]);
		let under = streams_root(&entries).unwrap();
		let (_r, tree_proof) = gen_stream_proof(&entries, stream).unwrap();

		// Full stream from base 0 (identity extension) verifies under `under`.
		let resp = MessagesResponse {
			base: MessagePosition(0),
			leaf_version: 0,
			payloads: payloads.clone(),
			start_peaks: Vec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof,
		};

		// 4 payloads × 2 bytes = 8 bytes; a budget that comfortably fits them.
		let total_bytes = payloads.iter().map(|p| p.len() as u32).sum::<u32>();

		// start == base and within budget → dispatches and authenticates.
		let req =
			MessagesRequest { stream, start: MessagePosition(0), under, max_bytes: total_bytes };
		assert_eq!(verify_messages(&req, &resp), Ok(payloads.clone()));

		// start != base → rejected up front (before any crypto), and distinct from RootMismatch:
		// the payloads do verify under `under`; only the answered range is wrong.
		let req_wrong =
			MessagesRequest { stream, start: MessagePosition(2), under, max_bytes: total_bytes };
		assert_eq!(verify_messages(&req_wrong, &resp), Err(VerifyError::UnexpectedBase));

		// Payloads exceed the byte budget → ExceedsBudget (not a proof/root failure).
		let req_tight = MessagesRequest {
			stream,
			start: MessagePosition(0),
			under,
			max_bytes: total_bytes - 1,
		};
		assert_eq!(verify_messages(&req_tight, &resp), Err(VerifyError::ExceedsBudget));

		// `max_bytes == 0` requests a payload-free (lift-material) proof, so any payload trips it.
		let req_liftonly =
			MessagesRequest { stream, start: MessagePosition(0), under, max_bytes: 0 };
		assert_eq!(verify_messages(&req_liftonly, &resp), Err(VerifyError::ExceedsBudget));
	}

	#[test]
	fn messages_response_rejects_wrong_root_or_tampered_payload() {
		let payloads: Vec<Vec<u8>> = (0..3u8).map(|i| vec![i]).collect();
		let store = MemStore::<Hash>::default();
		let mut src = MemMMR::<Hash, SpecMerge>::new(0, &store);
		for p in &payloads {
			src.push(leaf_hash(0, p)).unwrap();
		}
		let stream = ch(2000);
		let entries = BTreeMap::from([(stream, src.get_root().unwrap())]);
		let under = streams_root(&entries).unwrap();
		let (_r, tree_proof) = gen_stream_proof(&entries, stream).unwrap();

		let resp = MessagesResponse {
			base: MessagePosition(0),
			leaf_version: 0,
			payloads,
			start_peaks: Vec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof,
		};

		// Wrong `under` → recomputed root doesn't match the requested one.
		assert_eq!(
			verify_messages_response(stream, StreamsRoot(H256::repeat_byte(0xff)), &resp),
			Err(VerifyError::RootMismatch),
		);
		// Tampered payload → recomputed root diverges → also a mismatch against `under`.
		let mut bad = resp.clone();
		bad.payloads[0] = vec![0xAA];
		assert_eq!(verify_messages_response(stream, under, &bad), Err(VerifyError::RootMismatch));
	}

	#[test]
	fn malformed_frontier_rejects_without_panicking() {
		let stream = ch(2000);
		let under = StreamsRoot(H256::repeat_byte(0xff));

		// `start_peaks` shape inconsistent with `base` (empty peaks, base = 1): the pre-fix
		// pop-from-empty panic in `append`. `from_parts` must reject it cleanly, not panic.
		let crafted = MessagesResponse {
			base: MessagePosition(1),
			leaf_version: 0,
			payloads: vec![vec![0x01]],
			start_peaks: Vec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof: StreamProof { steps: Default::default() },
		};
		assert_eq!(
			verify_messages_response(stream, under, &crafted),
			Err(VerifyError::MalformedResponse),
		);

		// A `base` past the leaf-count ceiling must reject cleanly. It becomes the frontier's leaf
		// count, which the extension derives node positions from. One set bit, so one peak passes
		// the shape check and the ceiling is what rejects.
		let too_large = MessagesResponse {
			base: MessagePosition(1 << 63),
			leaf_version: 0,
			payloads: vec![vec![0x01]],
			start_peaks: vec![H256::zero(); 1],
			extension: MMRExtensionProof::identity(),
			tree_proof: StreamProof { steps: Default::default() },
		};
		assert_eq!(
			verify_messages_response(stream, under, &too_large),
			Err(VerifyError::MalformedResponse),
		);

		// `base = u64::MAX` (64 set bits, 64 peaks) once reached the `base + payloads` counter
		// guard; `from_parts`'s ceiling now rejects it first. Kept so both stay covered.
		let overflow = MessagesResponse {
			base: MessagePosition(u64::MAX),
			leaf_version: 0,
			payloads: vec![vec![0x01]],
			start_peaks: vec![H256::zero(); 64],
			extension: MMRExtensionProof::identity(),
			tree_proof: StreamProof { steps: Default::default() },
		};
		assert_eq!(
			verify_messages_response(stream, under, &overflow),
			Err(VerifyError::MalformedResponse),
		);
	}

	/// The exchange envelope's variant indices are wire-format-critical: foreign implementations
	/// decode by them, forever. Messages = 0x00, Event = 0x01 on both request and response.
	#[test]
	fn exchange_envelope_indices_are_frozen() {
		let mreq = MessagesRequest {
			stream: ch(1),
			start: MessagePosition(0),
			under: StreamsRoot(H256::zero()),
			max_bytes: 0,
		};
		let ereq = EventRequest { stream: ch(1), under: StreamsRoot(H256::zero()), at: None };
		assert_eq!(ExchangeRequest::Messages(mreq).encode()[0], 0x00);
		assert_eq!(ExchangeRequest::Event(ereq).encode()[0], 0x01);

		let mresp = MessagesResponse {
			base: MessagePosition(0),
			leaf_version: 0,
			payloads: Vec::new(),
			start_peaks: Vec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof: StreamProof { steps: Default::default() },
		};
		let eresp = EventResponse {
			payload: Vec::new(),
			leaf_version: 0,
			inclusion: MmrInclusionProof { mmr_size: 0, items: Vec::new() },
			tree_proof: StreamProof { steps: Default::default() },
		};
		assert_eq!(ExchangeResponse::Messages(mresp).encode()[0], 0x00);
		assert_eq!(ExchangeResponse::Event(eresp).encode()[0], 0x01);
	}

	/// End-to-end: a sender builds a stream MMR, commits its root into a `StreamsRoot`, and serves
	/// the head event with a single-leaf inclusion proof; the receiver authenticates it under
	/// the committed root, recovering the head position and payload. Wrong root / tampered payload
	/// must reject.
	#[test]
	fn event_response_round_trips_under_streams_root() {
		let stream = ch(2000);
		// 6 leaves: the head (index 5) has a non-trivial sibling path AND other peaks, exercising
		// both halves of `verify_head`'s item split.
		let payloads: Vec<Vec<u8>> = (0..6u8).map(|i| vec![i, i.wrapping_add(7)]).collect();

		let store = MemStore::<Hash>::default();
		let mut src = MemMMR::<Hash, SpecMerge>::new(0, &store);
		let positions: Vec<u64> =
			payloads.iter().map(|p| src.push(leaf_hash(0, p)).unwrap()).collect();
		let stream_root = src.get_root().unwrap();

		let entries = BTreeMap::from([(stream, stream_root)]);
		let under = streams_root(&entries).unwrap();
		let (_r, tree_proof) = gen_stream_proof(&entries, stream).unwrap();

		// Inclusion proof for the head leaf (index 5).
		let head = payloads.len() - 1;
		let mproof = src.gen_proof(vec![positions[head]]).unwrap();
		let inclusion = MmrInclusionProof {
			mmr_size: mmr_size(payloads.len()),
			items: mproof.proof_items().to_vec(),
		};

		let resp = EventResponse {
			payload: payloads[head].clone(),
			leaf_version: 0,
			inclusion,
			tree_proof,
		};

		// Head verify returns the frontier too (needed as the lossy-consumption interval endpoint).
		let (pos, frontier, payload) =
			verify_event_response(stream, under, &resp).expect("head verifies");
		assert_eq!(pos, MessagePosition(head as u64));
		assert_eq!(payload, payloads[head]);
		assert_eq!(frontier.leaf_count(), 6);
		assert_eq!(frontier.root().0, stream_root);

		// The request-aware dispatcher (`at = None`) yields the same head result.
		let req = EventRequest { stream, under, at: None };
		assert_eq!(
			verify_event(&req, &resp),
			Ok(VerifiedEvent::Head {
				position: MessagePosition(head as u64),
				frontier,
				payload: payloads[head].clone(),
			}),
		);

		// A head proof used *positionally* at a non-head index → rejected (the proof's shape
		// doesn't match that position; exact error is mmr_lib-dependent).
		assert!(verify_positional_event_response(stream, under, MessagePosition(0), &resp).is_err());
		// Wrong `under` → the recomputed root doesn't match the requested one.
		assert_eq!(
			verify_event_response(stream, StreamsRoot(H256::repeat_byte(0xff)), &resp),
			Err(VerifyError::RootMismatch),
		);
		// Tampered payload → verify_head succeeds structurally but derives a diverging root.
		let mut bad = resp.clone();
		bad.payload = vec![0xAA];
		assert_eq!(verify_event_response(stream, under, &bad), Err(VerifyError::RootMismatch));
		// Oversized payload → rejected before hashing.
		let mut big = resp.clone();
		big.payload = vec![0u8; MAX_SPECULATIVE_MESSAGE_LEN as usize + 1];
		assert_eq!(verify_event_response(stream, under, &big), Err(VerifyError::PayloadTooLarge));

		// Structurally invalid inclusion proof (invalid mmr_size) → surfaced as `Proof(..)`.
		let mut bogus = resp.clone();
		bogus.inclusion.mmr_size = 2;
		assert_eq!(
			verify_event_response(stream, under, &bogus),
			Err(VerifyError::Proof(ProofError::InvalidProof)),
		);
	}

	#[test]
	fn positional_event_response_round_trips_under_streams_root() {
		let stream = ch(2000);
		let payloads: Vec<Vec<u8>> = (0..6u8).map(|i| vec![i, i.wrapping_add(7)]).collect();

		let store = MemStore::<Hash>::default();
		let mut src = MemMMR::<Hash, SpecMerge>::new(0, &store);
		let positions: Vec<u64> =
			payloads.iter().map(|p| src.push(leaf_hash(0, p)).unwrap()).collect();
		let stream_root = src.get_root().unwrap();

		let entries = BTreeMap::from([(stream, stream_root)]);
		let under = streams_root(&entries).unwrap();
		let (_r, tree_proof) = gen_stream_proof(&entries, stream).unwrap();

		// Inclusion proof for a non-head leaf (index 2): the positional read path.
		let at = 2usize;
		let mproof = src.gen_proof(vec![positions[at]]).unwrap();
		let inclusion = MmrInclusionProof {
			mmr_size: mmr_size(payloads.len()),
			items: mproof.proof_items().to_vec(),
		};

		let resp =
			EventResponse { payload: payloads[at].clone(), leaf_version: 0, inclusion, tree_proof };

		// Correct position → payload authenticated under `under`.
		assert_eq!(
			verify_positional_event_response(stream, under, MessagePosition(at as u64), &resp),
			Ok(payloads[at].clone()),
		);
		// Wrong position (same proof) → rejected (exact error is mmr_lib-dependent).
		assert!(verify_positional_event_response(stream, under, MessagePosition(3), &resp).is_err());
		// Wrong `under` → the recomputed root doesn't match the requested one.
		assert_eq!(
			verify_positional_event_response(
				stream,
				StreamsRoot(H256::repeat_byte(0xff)),
				MessagePosition(at as u64),
				&resp,
			),
			Err(VerifyError::RootMismatch),
		);
		// Tampered payload → verify_leaf succeeds structurally but derives a diverging root.
		let mut bad = resp.clone();
		bad.payload = vec![0xAA];
		assert_eq!(
			verify_positional_event_response(stream, under, MessagePosition(at as u64), &bad),
			Err(VerifyError::RootMismatch),
		);

		// The request-aware dispatcher (`at = Some(2)`) yields the positional result.
		let req = EventRequest { stream, under, at: Some(MessagePosition(at as u64)) };
		assert_eq!(
			verify_event(&req, &resp),
			Ok(VerifiedEvent::Positional {
				position: MessagePosition(at as u64),
				payload: payloads[at].clone(),
			}),
		);
	}

	#[test]
	fn event_response_tree_proof_failure_surfaces_as_treeproof() {
		// A valid head inclusion proof but a structurally-broken tree proof (split_bit past the key
		// width) → the keyed-tree walk fails before any `under` comparison → TreeProof (distinct
		// from RootMismatch).
		let stream = ch(2000);
		let payloads: Vec<Vec<u8>> = (0..6u8).map(|i| vec![i]).collect();
		let store = MemStore::<Hash>::default();
		let mut src = MemMMR::<Hash, SpecMerge>::new(0, &store);
		let positions: Vec<u64> =
			payloads.iter().map(|p| src.push(leaf_hash(0, p)).unwrap()).collect();
		let head = payloads.len() - 1;
		let mproof = src.gen_proof(vec![positions[head]]).unwrap();
		let resp = EventResponse {
			payload: payloads[head].clone(),
			leaf_version: 0,
			inclusion: MmrInclusionProof {
				mmr_size: mmr_size(payloads.len()),
				items: mproof.proof_items().to_vec(),
			},
			// `split_bit = 255` is >= KEY_BITS, so `streams_root_from_proof` rejects the walk.
			tree_proof: StreamProof {
				steps: vec![TreeStep { split_bit: 255, sibling: H256::zero() }].try_into().unwrap(),
			},
		};
		assert_eq!(
			verify_event_response(stream, StreamsRoot(H256::repeat_byte(0xff)), &resp),
			Err(VerifyError::TreeProof),
		);
	}

	#[test]
	fn verify_exchange_dispatches_and_binds_variant() {
		// A valid Messages request/response pair.
		let mpayloads: Vec<Vec<u8>> = (0..3u8).map(|i| vec![i]).collect();
		let mstore = MemStore::<Hash>::default();
		let mut msrc = MemMMR::<Hash, SpecMerge>::new(0, &mstore);
		for p in &mpayloads {
			msrc.push(leaf_hash(0, p)).unwrap();
		}
		let mstream = ch(2000);
		let mentries = BTreeMap::from([(mstream, msrc.get_root().unwrap())]);
		let munder = streams_root(&mentries).unwrap();
		let (_r, mtree) = gen_stream_proof(&mentries, mstream).unwrap();
		let mresp = MessagesResponse {
			base: MessagePosition(0),
			leaf_version: 0,
			payloads: mpayloads.clone(),
			start_peaks: Vec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof: mtree,
		};
		let mbytes = mpayloads.iter().map(|p| p.len() as u32).sum::<u32>();
		let mreq = MessagesRequest {
			stream: mstream,
			start: MessagePosition(0),
			under: munder,
			max_bytes: mbytes,
		};

		// A valid Event (head) request/response pair.
		let epayloads: Vec<Vec<u8>> = (0..6u8).map(|i| vec![i, i + 1]).collect();
		let estore = MemStore::<Hash>::default();
		let mut esrc = MemMMR::<Hash, SpecMerge>::new(0, &estore);
		let epos: Vec<u64> =
			epayloads.iter().map(|p| esrc.push(leaf_hash(0, p)).unwrap()).collect();
		let estream = ch(3000);
		let eentries = BTreeMap::from([(estream, esrc.get_root().unwrap())]);
		let eunder = streams_root(&eentries).unwrap();
		let (_r2, etree) = gen_stream_proof(&eentries, estream).unwrap();
		let ehead = epayloads.len() - 1;
		let eproof = esrc.gen_proof(vec![epos[ehead]]).unwrap();
		let eresp = EventResponse {
			payload: epayloads[ehead].clone(),
			leaf_version: 0,
			inclusion: MmrInclusionProof {
				mmr_size: mmr_size(epayloads.len()),
				items: eproof.proof_items().to_vec(),
			},
			tree_proof: etree,
		};
		let ereq = EventRequest { stream: estream, under: eunder, at: None };

		// Matching variants → dispatched and authenticated.
		assert_eq!(
			verify_exchange(
				&ExchangeRequest::Messages(mreq.clone()),
				&ExchangeResponse::Messages(mresp.clone()),
			),
			Ok(ExchangeVerified::Messages(mpayloads)),
		);
		assert!(matches!(
			verify_exchange(
				&ExchangeRequest::Event(ereq.clone()),
				&ExchangeResponse::Event(eresp.clone()),
			),
			Ok(ExchangeVerified::Event(VerifiedEvent::Head { .. })),
		));

		// Cross-variant answers → VariantMismatch, before any crypto.
		assert_eq!(
			verify_exchange(&ExchangeRequest::Messages(mreq), &ExchangeResponse::Event(eresp)),
			Err(VerifyError::VariantMismatch),
		);
		assert_eq!(
			verify_exchange(&ExchangeRequest::Event(ereq), &ExchangeResponse::Messages(mresp)),
			Err(VerifyError::VariantMismatch),
		);
	}
}
