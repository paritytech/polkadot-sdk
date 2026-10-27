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
//! - [`leaf_hash`] turns a payload into an MMR leaf. A message is a bounded payload keyed by
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
use sp_runtime::{traits::Hash as HashT, BoundedVec};

use crate::{
	lift::{MMRExtensionProof, MmrInclusionProof, ProofError},
	mmr::{MessagePosition, MmrFrontier, MmrRoot, MAX_MMR_HASHES, MAX_MMR_LEAF_COUNT},
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
	/// The payloads total more than the request's `max_bytes`, or `max_bytes == 0` and the
	/// response carries any payload, even an empty one. Only surfaced by [`verify_messages`].
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

/// Maximum payloads in one [`MessagesResponse`], enforced at decode. The byte budget does not bound
/// the count: an empty payload costs one wire byte and no budget, yet is hashed into a leaf, so
/// without this bound one response could carry millions. A consumer fetches the rest under a later
/// request.
pub const MAX_MESSAGES_PER_RESPONSE: u32 = 1024;

/// Hash a payload into a stream MMR leaf: `H(LEAF_TAG ++ leaf_version ++ payload)`. Source, stream
/// and position are structural and not in the preimage. Versions are hash-disjoint, so only the
/// correct one reproduces a committed root.
pub fn leaf_hash(leaf_version: u8, payload: &[u8]) -> Hash {
	let mut preimage = Vec::with_capacity(2 + payload.len());
	preimage.push(LEAF_TAG);
	preimage.push(leaf_version);
	preimage.extend_from_slice(payload);
	<SpecHasher as HashT>::hash(&preimage)
}

/// Request for a range of a stream's messages, verified under a chosen `StreamsRoot`.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub struct MessagesRequest {
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
	/// The payloads, in MMR order; payload `i` has position `base + i`. At most
	/// [`MAX_MESSAGES_PER_RESPONSE`], rejected at the length prefix.
	pub payloads: BoundedVec<Vec<u8>, ConstU32<MAX_MESSAGES_PER_RESPONSE>>,
	/// The stream's peak set at `base`, for a consumer that holds no frontier. Fabricated peaks
	/// cannot extend to a committed root.
	pub start_peaks: BoundedVec<Hash, ConstU32<MAX_MMR_HASHES>>,
	/// From the frontier recomputed over `payloads` to the stream's current root; the identity
	/// extension when the payloads already reach it.
	pub extension: MMRExtensionProof,
	/// From the stream's root to the `StreamsRoot` the response verifies under.
	pub tree_proof: StreamProof,
}

/// Verify a [`MessagesResponse`] against its [`MessagesRequest`]: `resp.base == req.start`, total
/// payload bytes within `req.max_bytes` (`0` means proofs only), then the payloads under
/// `req.stream` / `req.under`. On `Ok`, `resp.payloads` are verified. The public entry for the
/// fetch protocol.
pub fn verify_messages(req: &MessagesRequest, resp: &MessagesResponse) -> Result<(), VerifyError> {
	if resp.base != req.start {
		return Err(VerifyError::UnexpectedBase);
	}
	let total = resp.payloads.iter().map(|p| p.len() as u64).fold(0u64, u64::saturating_add);
	if total > req.max_bytes as u64 || (req.max_bytes == 0 && !resp.payloads.is_empty()) {
		return Err(VerifyError::ExceedsBudget);
	}
	verify_messages_response(req.stream, req.under, resp)
}

/// Verify a [`MessagesResponse`] for `stream` under `under`: the same check the requires-lift
/// performs. Crate-internal: it does not bind `resp.base` to a requested start; [`verify_messages`]
/// does.
pub(crate) fn verify_messages_response(
	stream: StreamId,
	under: StreamsRoot,
	resp: &MessagesResponse,
) -> Result<(), VerifyError> {
	// `base` and `start_peaks` are untrusted; `from_parts` rejects an inconsistent shape and a
	// `base` past the ceiling.
	let mut frontier = MmrFrontier::from_parts(resp.start_peaks.to_vec(), resp.base.0)
		.ok_or(VerifyError::MalformedResponse)?;
	// The frontier after the payloads must stay within `MAX_MMR_LEAF_COUNT`, or it would not
	// decode. Checked up front; `try_append` refuses it too.
	resp.base
		.0
		.checked_add(resp.payloads.len() as u64)
		.filter(|end| *end <= MAX_MMR_LEAF_COUNT)
		.ok_or(VerifyError::MalformedResponse)?;
	// Defense-in-depth against a crafted response inflating the hashed input; the transport also
	// caps total response size.
	resp.payloads.iter().try_for_each(|p| check_payload_len(p))?;

	for payload in &resp.payloads {
		frontier
			.try_append(leaf_hash(resp.leaf_version, payload))
			.map_err(|_| VerifyError::MalformedResponse)?;
	}

	let current = resp.extension.verify(&frontier)?;
	bind_under(stream, under, current, &resp.tree_proof)
}

fn check_payload_len(payload: &[u8]) -> Result<(), VerifyError> {
	if payload.len() as u64 > MAX_SPECULATIVE_MESSAGE_LEN as u64 {
		return Err(VerifyError::PayloadTooLarge);
	}
	Ok(())
}

/// Walk `tree_proof` from `stream_root` to a `StreamsRoot` and require it to be `under`:
/// [`TreeProof`](VerifyError::TreeProof) if the walk is malformed,
/// [`RootMismatch`](VerifyError::RootMismatch) if it ends elsewhere.
fn bind_under(
	stream: StreamId,
	under: StreamsRoot,
	stream_root: MmrRoot,
	tree_proof: &StreamProof,
) -> Result<(), VerifyError> {
	let root =
		streams_root_from_proof(stream, stream_root.0, tree_proof).ok_or(VerifyError::TreeProof)?;
	(root == under).then_some(()).ok_or(VerifyError::RootMismatch)
}

fn event_leaf(resp: &EventResponse) -> Result<Hash, VerifyError> {
	check_payload_len(&resp.payload)?;
	Ok(leaf_hash(resp.leaf_version, &resp.payload))
}

/// Request for a single event of a lossy stream (`Ack` / `Broadcast` / `Private`), verified under a
/// chosen `StreamsRoot`. The server either serves or fails; nothing is resolved server-side.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub struct EventRequest {
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
/// lossy consumption records as its interval endpoint; a positional read yields none, since an
/// inclusion proof reconstructs a root, not a frontier.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum VerifiedEvent {
	/// Head (register / latest-event) read: the head position and the stream frontier at that
	/// head.
	Head { position: MessagePosition, frontier: MmrFrontier },
	/// Positional read: the requested position.
	Positional { position: MessagePosition },
}

/// Verify an [`EventResponse`] against its [`EventRequest`]: `req.at` selects head (`None`) or leaf
/// `k`, so a head response is never accepted for a positional request or vice versa. On `Ok`,
/// `resp.payload` is verified. The public entry for event reads.
pub fn verify_event(
	req: &EventRequest,
	resp: &EventResponse,
) -> Result<VerifiedEvent, VerifyError> {
	match req.at {
		None => {
			let (position, frontier) = verify_event_response(req.stream, req.under, resp)?;
			Ok(VerifiedEvent::Head { position, frontier })
		},
		Some(position) => {
			verify_positional_event_response(req.stream, req.under, position, resp)?;
			Ok(VerifiedEvent::Positional { position })
		},
	}
}

/// Verify an [`EventResponse`] as the head of `stream` under `under`; returns the position and the
/// frontier at that head. Crate-internal; [`verify_event`] is the public entry.
///
/// The frontier is returned because lossy consumption needs it as the interval endpoint.
pub(crate) fn verify_event_response(
	stream: StreamId,
	under: StreamsRoot,
	resp: &EventResponse,
) -> Result<(MessagePosition, MmrFrontier), VerifyError> {
	let (position, frontier) = resp.inclusion.verify_head(event_leaf(resp)?)?;
	bind_under(stream, under, frontier.root(), &resp.tree_proof)?;
	Ok((position, frontier))
}

/// Verify an [`EventResponse`] as the leaf at `position` of `stream` under `under`.
/// Crate-internal; [`verify_event`] is the public entry. Same chain as the head read, with
/// [`MmrInclusionProof::verify_leaf`].
pub(crate) fn verify_positional_event_response(
	stream: StreamId,
	under: StreamsRoot,
	position: MessagePosition,
	resp: &EventResponse,
) -> Result<(), VerifyError> {
	let root = resp.inclusion.verify_leaf(position, event_leaf(resp)?)?;
	bind_under(stream, under, root, &resp.tree_proof)
}

/// Verified outcome of an [`ExchangeResponse`]: a fetch, or a [`VerifiedEvent`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ExchangeVerified {
	/// Answer to an [`ExchangeRequest::Messages`]; the response's payloads are verified.
	Messages,
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
			verify_messages(mreq, mresp).map(|()| ExchangeVerified::Messages)
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
		mmr::{MmrRoot, SpecMerge},
		streams_root::{gen_stream_proof, streams_root, TreeStep},
		test_utils::{ch, frontier_at, mmr_size},
	};
	use alloc::collections::BTreeMap;
	use codec::Encode;
	use mmr_lib::{
		helper::get_peak_map,
		leaf_index_to_mmr_size,
		util::{MemMMR, MemStore},
	};
	use polkadot_parachain_primitives::primitives::Id as ParaId;
	use sp_core::H256;

	/// The stream root of an `mmr_lib` MMR: its bag bound to the leaf count.
	fn bound(mmr_size: u64, bag: Hash) -> Hash {
		MmrRoot::from_bag(get_peak_map(mmr_size), bag).0
	}

	/// The stream root over the first `n` leaves.
	fn current_root(leaves: &[Hash], n: usize) -> Hash {
		let store = MemStore::<Hash>::default();
		let mut mmr = MemMMR::<Hash, SpecMerge>::new(0, &store);
		for l in &leaves[..n] {
			mmr.push(*l).unwrap();
		}
		bound(mmr.mmr_size(), mmr.get_root().unwrap())
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
			connecting_nodes: BoundedVec::truncate_from(
				ap.prev_peaks_proof.proof_items().iter().map(|(_, h)| *h).collect::<Vec<_>>(),
			),
		}
	}

	/// A 6-message stream commits a `StreamsRoot`; a partial fetch (messages 2..3, extension 4→6)
	/// verifies under it, and consuming the same messages lifts to the same root through
	/// `build_requires`. Fetch and lift run the same `extension` + `tree_proof` check.
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
			payloads: vec![payloads[2].clone(), payloads[3].clone()].try_into().unwrap(),
			start_peaks: BoundedVec::truncate_from(frontier_at(&leaves, 2).peaks().to_vec()),
			extension: ancestry(&leaves, 4, 6),
			tree_proof: tree_proof.clone(),
		};
		assert_eq!(verify_messages_response(stream, under, &resp), Ok(()));

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
		// FROZEN consensus vector: `H(0x01 ‖ 0x00 ‖ "hello")`. Any change to `LEAF_TAG`,
		// `LEAF_VERSION`, the preimage layout or the hasher breaks this.
		assert_eq!(
			leaf_hash(crate::LEAF_VERSION, b"hello"),
			Hash::from(hex_literal::hex!(
				"cd31917fb8992dae762dbaaf276d8eb65aa89cdfb87daf69e05f8c08b490e78b"
			))
		);
	}

	/// A response for the whole stream from `base = 0` verifies under the committed root.
	#[test]
	fn messages_response_round_trips_under_streams_root() {
		let payloads: Vec<Vec<u8>> = (0..4u8).map(|i| vec![i, i + 1, i + 2]).collect();

		// Sender stream MMR over the payload leaves; its root is the stream root.
		let store = MemStore::<Hash>::default();
		let mut src = MemMMR::<Hash, SpecMerge>::new(0, &store);
		for p in &payloads {
			src.push(leaf_hash(0, p)).unwrap();
		}
		let stream_root = bound(src.mmr_size(), src.get_root().unwrap());

		let stream = ch(2000);
		let entries = BTreeMap::from([(stream, stream_root)]);
		let under = streams_root(&entries).unwrap();
		let (_r, tree_proof) = gen_stream_proof(&entries, stream).unwrap();

		// Full stream from base 0: empty start_peaks, identity extension (payloads reach the root).
		let resp = MessagesResponse {
			base: MessagePosition(0),
			leaf_version: 0,
			payloads: payloads.clone().try_into().unwrap(),
			start_peaks: BoundedVec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof,
		};

		assert_eq!(verify_messages_response(stream, under, &resp), Ok(()));
		// Sanity: the recomputed root matches the sender's.
		let mut mmr = MmrFrontier::new();
		for p in &resp.payloads {
			mmr.append(leaf_hash(0, p));
		}
		assert_eq!(mmr.root().0, stream_root);
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
		let entries = BTreeMap::from([(stream, bound(src.mmr_size(), src.get_root().unwrap()))]);
		let under = streams_root(&entries).unwrap();
		let (_r, tree_proof) = gen_stream_proof(&entries, stream).unwrap();

		// Full stream from base 0 (identity extension) verifies under `under`.
		let resp = MessagesResponse {
			base: MessagePosition(0),
			leaf_version: 0,
			payloads: payloads.clone().try_into().unwrap(),
			start_peaks: BoundedVec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof,
		};

		// 4 payloads × 2 bytes = 8 bytes; a budget that comfortably fits them.
		let total_bytes = payloads.iter().map(|p| p.len() as u32).sum::<u32>();

		// start == base and within budget → dispatches and authenticates.
		let req =
			MessagesRequest { stream, start: MessagePosition(0), under, max_bytes: total_bytes };
		assert_eq!(verify_messages(&req, &resp), Ok(()));

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
	fn zero_budget_refuses_even_empty_payloads() {
		// A stream of two empty payloads: they total 0 bytes, so only the "proofs only" rule
		// refuses them under `max_bytes == 0`.
		let payloads: Vec<Vec<u8>> = vec![Vec::new(), Vec::new()];
		let stream = ch(2000);
		let entries = BTreeMap::from([(
			stream,
			frontier_at(&payloads.iter().map(|p| leaf_hash(0, p)).collect::<Vec<_>>(), 2)
				.root()
				.0,
		)]);
		let under = streams_root(&entries).unwrap();
		let (_r, tree_proof) = gen_stream_proof(&entries, stream).unwrap();
		let resp = MessagesResponse {
			base: MessagePosition(0),
			leaf_version: 0,
			payloads: payloads.clone().try_into().unwrap(),
			start_peaks: BoundedVec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof,
		};
		let req =
			|max_bytes| MessagesRequest { stream, start: MessagePosition(0), under, max_bytes };
		assert_eq!(verify_messages(&req(1), &resp), Ok(()));
		assert_eq!(verify_messages(&req(0), &resp), Err(VerifyError::ExceedsBudget));
	}

	#[test]
	fn a_response_past_the_payload_cap_fails_to_decode() {
		// Empty payloads cost one wire byte each and no budget, but each would be hashed: the
		// count is bounded at the length prefix, before anything is allocated or hashed.
		let cap = MAX_MESSAGES_PER_RESPONSE as usize;
		let encoded = |count: usize| {
			(
				MessagePosition(0),
				0u8,
				vec![Vec::<u8>::new(); count],
				Vec::<Hash>::new(),
				MMRExtensionProof::identity(),
				StreamProof { steps: Default::default() },
			)
				.encode()
		};
		assert!(MessagesResponse::decode(&mut &encoded(cap)[..]).is_ok());
		assert!(MessagesResponse::decode(&mut &encoded(cap + 1)[..]).is_err());
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
		let entries = BTreeMap::from([(stream, bound(src.mmr_size(), src.get_root().unwrap()))]);
		let under = streams_root(&entries).unwrap();
		let (_r, tree_proof) = gen_stream_proof(&entries, stream).unwrap();

		let resp = MessagesResponse {
			base: MessagePosition(0),
			leaf_version: 0,
			payloads: payloads.try_into().unwrap(),
			start_peaks: BoundedVec::new(),
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
			payloads: vec![vec![0x01]].try_into().unwrap(),
			start_peaks: BoundedVec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof: StreamProof { steps: Default::default() },
		};
		assert_eq!(
			verify_messages_response(stream, under, &crafted),
			Err(VerifyError::MalformedResponse),
		);

		// A `base` past the ceiling. One set bit means one peak passes the shape check, so the
		// ceiling is what rejects.
		let too_large = MessagesResponse {
			base: MessagePosition(1 << 63),
			leaf_version: 0,
			payloads: vec![vec![0x01]].try_into().unwrap(),
			start_peaks: BoundedVec::truncate_from(vec![H256::zero(); 1]),
			extension: MMRExtensionProof::identity(),
			tree_proof: StreamProof { steps: Default::default() },
		};
		assert_eq!(
			verify_messages_response(stream, under, &too_large),
			Err(VerifyError::MalformedResponse),
		);

		// A `base` at the ceiling is a valid frontier, but its next leaf would carry the count past
		// it: the frontier would no longer decode. Rejected before anything is appended.
		let at_ceiling = MessagesResponse {
			base: MessagePosition(MAX_MMR_LEAF_COUNT),
			leaf_version: 0,
			payloads: vec![vec![0x01]].try_into().unwrap(),
			start_peaks: BoundedVec::truncate_from(vec![H256::zero(); 1]),
			extension: MMRExtensionProof::identity(),
			tree_proof: StreamProof { steps: Default::default() },
		};
		assert_eq!(
			verify_messages_response(stream, under, &at_ceiling),
			Err(VerifyError::MalformedResponse),
		);

		// `base = u64::MAX` (64 set bits, 64 peaks) once reached the `base + payloads` counter
		// guard; `from_parts`'s ceiling now rejects it first. Kept so both stay covered.
		let overflow = MessagesResponse {
			base: MessagePosition(u64::MAX),
			leaf_version: 0,
			payloads: vec![vec![0x01]].try_into().unwrap(),
			start_peaks: BoundedVec::truncate_from(vec![H256::zero(); MAX_MMR_HASHES as usize]),
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
			payloads: Vec::new().try_into().unwrap(),
			start_peaks: BoundedVec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof: StreamProof { steps: Default::default() },
		};
		let eresp = EventResponse {
			payload: Vec::new(),
			leaf_version: 0,
			inclusion: MmrInclusionProof { mmr_size: 0, items: BoundedVec::new() },
			tree_proof: StreamProof { steps: Default::default() },
		};
		assert_eq!(ExchangeResponse::Messages(mresp).encode()[0], 0x00);
		assert_eq!(ExchangeResponse::Event(eresp).encode()[0], 0x01);
	}

	/// The head event verifies under the committed root; a wrong root or payload does not.
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
		let stream_root = bound(src.mmr_size(), src.get_root().unwrap());

		let entries = BTreeMap::from([(stream, stream_root)]);
		let under = streams_root(&entries).unwrap();
		let (_r, tree_proof) = gen_stream_proof(&entries, stream).unwrap();

		// Inclusion proof for the head leaf (index 5).
		let head = payloads.len() - 1;
		let mproof = src.gen_proof(vec![positions[head]]).unwrap();
		let inclusion = MmrInclusionProof {
			mmr_size: mmr_size(payloads.len()),
			items: BoundedVec::truncate_from(mproof.proof_items().to_vec()),
		};

		let resp = EventResponse {
			payload: payloads[head].clone(),
			leaf_version: 0,
			inclusion,
			tree_proof,
		};

		// Head verify returns the frontier too (needed as the lossy-consumption interval endpoint).
		let (pos, frontier) = verify_event_response(stream, under, &resp).expect("head verifies");
		assert_eq!(pos, MessagePosition(head as u64));
		assert_eq!(frontier.leaf_count(), 6);
		assert_eq!(frontier.root().0, stream_root);

		// The request-aware dispatcher (`at = None`) yields the same head result.
		let req = EventRequest { stream, under, at: None };
		assert_eq!(
			verify_event(&req, &resp),
			Ok(VerifiedEvent::Head { position: MessagePosition(head as u64), frontier }),
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
		// At the bound the size check passes; the forged payload then fails the proof.
		big.payload.pop();
		assert_eq!(verify_event_response(stream, under, &big), Err(VerifyError::RootMismatch));

		// Structurally invalid inclusion proof (invalid mmr_size) → surfaced as `Proof(..)`.
		let mut bogus = resp.clone();
		bogus.inclusion.mmr_size = 2;
		assert_eq!(
			verify_event_response(stream, under, &bogus),
			Err(VerifyError::Proof(ProofError::InvalidProof)),
		);
	}

	#[test]
	fn event_response_rejects_a_forged_head_position() {
		// The two peaks of 3 leaves also form a frontier of `2^47 + 1` leaves, and bag identically.
		// A server claiming that size in `mmr_size` must not pass the head off at position `2^47`.
		let stream = ch(2000);
		let payloads: Vec<Vec<u8>> = (0..3u8).map(|i| vec![i]).collect();
		let leaves: Vec<Hash> = payloads.iter().map(|p| leaf_hash(0, p)).collect();
		let entries = BTreeMap::from([(stream, frontier_at(&leaves, 3).root().0)]);
		let under = streams_root(&entries).unwrap();
		let (_r, tree_proof) = gen_stream_proof(&entries, stream).unwrap();

		let forged_count = (1u64 << 47) + 1;
		let resp = EventResponse {
			payload: payloads[2].clone(),
			leaf_version: 0,
			inclusion: MmrInclusionProof {
				mmr_size: leaf_index_to_mmr_size(forged_count - 1),
				items: BoundedVec::truncate_from(frontier_at(&leaves, 2).peaks().to_vec()),
			},
			tree_proof,
		};
		assert_eq!(verify_event_response(stream, under, &resp), Err(VerifyError::RootMismatch));
	}

	#[test]
	fn positional_event_response_round_trips_under_streams_root() {
		let stream = ch(2000);
		let payloads: Vec<Vec<u8>> = (0..6u8).map(|i| vec![i, i.wrapping_add(7)]).collect();

		let store = MemStore::<Hash>::default();
		let mut src = MemMMR::<Hash, SpecMerge>::new(0, &store);
		let positions: Vec<u64> =
			payloads.iter().map(|p| src.push(leaf_hash(0, p)).unwrap()).collect();
		let stream_root = bound(src.mmr_size(), src.get_root().unwrap());

		let entries = BTreeMap::from([(stream, stream_root)]);
		let under = streams_root(&entries).unwrap();
		let (_r, tree_proof) = gen_stream_proof(&entries, stream).unwrap();

		// Inclusion proof for a non-head leaf (index 2): the positional read path.
		let at = 2usize;
		let mproof = src.gen_proof(vec![positions[at]]).unwrap();
		let inclusion = MmrInclusionProof {
			mmr_size: mmr_size(payloads.len()),
			items: BoundedVec::truncate_from(mproof.proof_items().to_vec()),
		};

		let resp =
			EventResponse { payload: payloads[at].clone(), leaf_version: 0, inclusion, tree_proof };

		// Correct position → payload authenticated under `under`.
		assert_eq!(
			verify_positional_event_response(stream, under, MessagePosition(at as u64), &resp),
			Ok(()),
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
			Ok(VerifiedEvent::Positional { position: MessagePosition(at as u64) }),
		);
	}

	#[test]
	fn event_response_tree_proof_failure_surfaces_as_treeproof() {
		// A split bit past the key width fails the tree walk (`TreeProof`, not `RootMismatch`).
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
				items: BoundedVec::truncate_from(mproof.proof_items().to_vec()),
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
		let mentries =
			BTreeMap::from([(mstream, bound(msrc.mmr_size(), msrc.get_root().unwrap()))]);
		let munder = streams_root(&mentries).unwrap();
		let (_r, mtree) = gen_stream_proof(&mentries, mstream).unwrap();
		let mresp = MessagesResponse {
			base: MessagePosition(0),
			leaf_version: 0,
			payloads: mpayloads.clone().try_into().unwrap(),
			start_peaks: BoundedVec::new(),
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
		let eentries =
			BTreeMap::from([(estream, bound(esrc.mmr_size(), esrc.get_root().unwrap()))]);
		let eunder = streams_root(&eentries).unwrap();
		let (_r2, etree) = gen_stream_proof(&eentries, estream).unwrap();
		let ehead = epayloads.len() - 1;
		let eproof = esrc.gen_proof(vec![epos[ehead]]).unwrap();
		let eresp = EventResponse {
			payload: epayloads[ehead].clone(),
			leaf_version: 0,
			inclusion: MmrInclusionProof {
				mmr_size: mmr_size(epayloads.len()),
				items: BoundedVec::truncate_from(eproof.proof_items().to_vec()),
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
			Ok(ExchangeVerified::Messages),
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

	/// Two committed 6-leaf streams with distinct payloads, and the `StreamsRoot` over both.
	struct TwoStreams {
		under: StreamsRoot,
		/// Per stream: payloads, leaves, tree proof.
		streams: [(StreamId, Vec<Vec<u8>>, Vec<Hash>, StreamProof); 2],
	}

	fn two_streams() -> TwoStreams {
		let ids = [ch(2000), ch(3000)];
		let payloads: [Vec<Vec<u8>>; 2] =
			core::array::from_fn(|s| (0..6u8).map(|i| vec![s as u8, i]).collect());
		let leaves: [Vec<Hash>; 2] =
			core::array::from_fn(|s| payloads[s].iter().map(|p| leaf_hash(0, p)).collect());
		let entries: BTreeMap<_, _> =
			(0..2).map(|s| (ids[s], frontier_at(&leaves[s], 6).root().0)).collect();
		let under = streams_root(&entries).unwrap();
		let streams = core::array::from_fn(|s| {
			let (_, proof) = gen_stream_proof(&entries, ids[s]).unwrap();
			(ids[s], payloads[s].clone(), leaves[s].clone(), proof)
		});
		TwoStreams { under, streams }
	}

	/// The honest fetch of `payloads[base..end]` with a real extension from `end` to 6.
	fn fetch(
		(_, payloads, leaves, tree_proof): &(StreamId, Vec<Vec<u8>>, Vec<Hash>, StreamProof),
		base: usize,
		end: usize,
	) -> MessagesResponse {
		MessagesResponse {
			base: MessagePosition(base as u64),
			leaf_version: 0,
			payloads: payloads[base..end].to_vec().try_into().unwrap(),
			start_peaks: BoundedVec::truncate_from(frontier_at(leaves, base).peaks().to_vec()),
			extension: ancestry(leaves, end, 6),
			tree_proof: tree_proof.clone(),
		}
	}

	/// The honest head read of a 6-leaf stream.
	fn head(
		(_, payloads, leaves, tree_proof): &(StreamId, Vec<Vec<u8>>, Vec<Hash>, StreamProof),
	) -> EventResponse {
		let store = MemStore::<Hash>::default();
		let mut mmr = MemMMR::<Hash, SpecMerge>::new(0, &store);
		let positions: Vec<u64> = leaves.iter().map(|l| mmr.push(*l).unwrap()).collect();
		let proof = mmr.gen_proof(vec![positions[5]]).unwrap();
		EventResponse {
			payload: payloads[5].clone(),
			leaf_version: 0,
			inclusion: MmrInclusionProof {
				mmr_size: mmr_size(6),
				items: BoundedVec::truncate_from(proof.proof_items().to_vec()),
			},
			tree_proof: tree_proof.clone(),
		}
	}

	#[test]
	fn a_forged_tail_after_a_genuine_prefix_does_not_verify() {
		let t = two_streams();
		let (stream, ..) = t.streams[0];
		let honest = fetch(&t.streams[0], 2, 4);
		assert_eq!(verify_messages_response(stream, t.under, &honest), Ok(()));

		// Leaf 2 is genuine and leaf 3 forged; the extension from 4 to 6 is the real one.
		let mut forged = honest.clone();
		forged.payloads[1] = vec![0xAA];
		assert_eq!(
			verify_messages_response(stream, t.under, &forged),
			Err(VerifyError::RootMismatch)
		);

		// A forged leaf appended past the genuine ones, with the real extension from 5 to 6.
		let mut appended = fetch(&t.streams[0], 2, 4);
		appended.payloads.try_push(vec![0xAA]).unwrap();
		appended.extension = ancestry(&t.streams[0].2, 5, 6);
		assert_eq!(
			verify_messages_response(stream, t.under, &appended),
			Err(VerifyError::RootMismatch)
		);
	}

	#[test]
	fn a_messages_payload_past_the_bound_is_too_large() {
		let t = two_streams();
		let (stream, ..) = t.streams[0];
		let mut resp = fetch(&t.streams[0], 2, 4);
		resp.payloads[1] = vec![0; MAX_SPECULATIVE_MESSAGE_LEN as usize + 1];
		let req = MessagesRequest {
			stream,
			start: MessagePosition(2),
			under: t.under,
			max_bytes: u32::MAX,
		};
		assert_eq!(verify_messages(&req, &resp), Err(VerifyError::PayloadTooLarge));

		// At the bound the size check passes; the forged payload then fails the proof.
		resp.payloads[1].pop();
		assert_eq!(verify_messages(&req, &resp), Err(VerifyError::RootMismatch));
	}

	#[test]
	fn a_wrong_leaf_version_is_a_root_mismatch() {
		let t = two_streams();
		let (stream, ..) = t.streams[0];
		let mut resp = fetch(&t.streams[0], 2, 4);
		resp.leaf_version = 1;
		assert_eq!(
			verify_messages_response(stream, t.under, &resp),
			Err(VerifyError::RootMismatch)
		);

		let mut event = head(&t.streams[0]);
		event.leaf_version = 1;
		assert_eq!(verify_event_response(stream, t.under, &event), Err(VerifyError::RootMismatch));
	}

	#[test]
	fn material_of_another_stream_is_a_root_mismatch() {
		let t = two_streams();
		let (a, ..) = t.streams[0];
		let (b, ..) = t.streams[1];

		// Stream A's payloads and MMR proofs with stream B's tree proof.
		let mut resp = fetch(&t.streams[0], 2, 4);
		resp.tree_proof = t.streams[1].3.clone();
		assert_eq!(verify_messages_response(a, t.under, &resp), Err(VerifyError::RootMismatch));

		// Stream B's whole response, offered as stream A.
		let resp = fetch(&t.streams[1], 2, 4);
		assert_eq!(verify_messages_response(b, t.under, &resp), Ok(()));
		assert_eq!(verify_messages_response(a, t.under, &resp), Err(VerifyError::RootMismatch));

		let event = head(&t.streams[1]);
		assert!(verify_event_response(b, t.under, &event).is_ok());
		assert_eq!(verify_event_response(a, t.under, &event), Err(VerifyError::RootMismatch));
	}

	#[test]
	fn a_response_without_payloads_verifies_and_advances_nothing() {
		// Valid lift material, so a server can always answer this way; the consumer's frontier
		// stays at `base`, and it must fetch again.
		let t = two_streams();
		let (stream, ..) = t.streams[0];
		let resp = fetch(&t.streams[0], 2, 2);
		assert!(resp.payloads.is_empty());
		for max_bytes in [0, u32::MAX] {
			let req =
				MessagesRequest { stream, start: MessagePosition(2), under: t.under, max_bytes };
			assert_eq!(verify_messages(&req, &resp), Ok(()));
		}
	}
}
