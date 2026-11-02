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

//! The fetch protocol: off-chain wire types and their verifier.
//!
//! The relay chain decodes none of this; it only matches the `StreamsRoot` commitments in the UMP
//! signals. The two ends of the fetch protocol are different chains' collators, so the encodings
//! and error variant indices are normative for interoperability.
//!
//! - [`MessagesRequest`] / [`MessagesResponse`] and [`FetchError`] are the single request/response
//! pair for all off-chain reads: ordered channel consumption, lossy head reads (`start: None`),
//! range reads, gap leaves and proofs-only lift material (`max_bytes: 0`).
//! - [`verify_messages`] authenticates a [`MessagesResponse`] against its [`MessagesRequest`].

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::ConstU32;
use sp_runtime::BoundedVec;

use crate::{
	mmr::{
		leaf_hash, MMRExtensionProof, MessagePosition, MmrFrontier, MmrRoot, ProofError,
		MAX_MMR_LEAF_COUNT,
	},
	stream::StreamId,
	streams_root::{streams_root_from_proof, StreamProof, StreamsRoot},
};

/// Why verifying a [`MessagesResponse`] failed. [`RootMismatch`](VerifyError::RootMismatch) is
/// benign when a peer served under a different root; the rest indicate a malformed or forged
/// response.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VerifyError {
	/// The embedded MMR extension proof did not verify.
	Proof(ProofError),
	/// The `StreamsRoot` keyed-tree walk (`tree_proof`) did not resolve.
	TreeProof,
	/// Everything verified, but the recomputed `StreamsRoot` is not the requested `under`.
	RootMismatch,
	/// For `start: Some(pos)`, the response's `frontier.leaf_count()` is not `pos`.
	UnexpectedBase,
	/// For an inbound channel fetch, the response's `frontier` does not match the receiver's
	/// local stored frontier.
	FrontierMismatch,
	/// For `start: None` (a head request), the response does not end at
	/// `frontier.leaf_count() + 1`.
	NotAtHead,
	/// The payloads total more than the request's `max_bytes`, or `max_bytes == 0` and the
	/// response carries any payload, even an empty one.
	ExceedsBudget,
	/// A payload exceeded the protocol's single-message bound ([`MAX_SPECULATIVE_MESSAGE_LEN`]).
	PayloadTooLarge,
	/// The response is structurally malformed independently of its proofs (e.g. `frontier +
	/// payloads` overflows `MAX_MMR_LEAF_COUNT`).
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

/// The fetch protocol's request. Every off-chain read (channel consumption, register and event
/// head reads, range reads, gap leaves, lift material) uses this request with different parameters.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub struct MessagesRequest {
	/// A stream of the serving chain.
	pub stream: StreamId,
	/// The `StreamsRoot` the response must verify under: one the requester has already verified at
	/// its tier.
	pub under: StreamsRoot,
	/// First leaf wanted. `None` means the head under `under`; the server resolves the position.
	pub start: Option<MessagePosition>,
	/// Payload budget; the server returns payloads from `start` until the budget runs out (it may
	/// cap lower) and covers the rest with `extension`. `0` asks for proofs only (lift material).
	pub max_bytes: u32,
}

/// Response: the frontier before the first served leaf, the payloads from `frontier.leaf_count()`
/// on, and the proofs binding them to the requested `StreamsRoot`. Nothing is trusted: roots bind
/// their leaf counts, so a wrong frontier or count cannot verify under `under`.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub struct MessagesResponse {
	/// The stream just before the first leaf served. Its `leaf_count()` is the start position.
	/// Decodes through [`MmrFrontier::from_parts`].
	pub frontier: MmrFrontier,
	/// The payloads, in MMR order; payload `i` sits at position `frontier.leaf_count() + i`. At
	/// most [`MAX_MESSAGES_PER_RESPONSE`], rejected at the length prefix.
	pub payloads: BoundedVec<Vec<u8>, ConstU32<MAX_MESSAGES_PER_RESPONSE>>,
	/// From the frontier after `payloads` to the stream's state under `under`; the identity
	/// extension when `payloads` already reach it.
	pub extension: MMRExtensionProof,
	/// From the stream's root to `under`.
	pub tree_proof: StreamProof,
}

/// What the fetch server sends back for one [`MessagesRequest`].
pub type FetchResponse = Result<MessagesResponse, FetchError>;

/// Typed refusal returned by the fetch server when it cannot serve a [`MessagesRequest`].
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	PartialEq,
	Eq,
	Debug,
	TypeInfo,
)]
pub enum FetchError {
	/// The stream has no entry under `under`.
	#[codec(index = 0)]
	UnknownStream,
	/// The server does not know `under` (not yet seen, or pruned).
	#[codec(index = 1)]
	UnknownRoot,
	/// `start` is below the server's retention boundary; `lowest` is the first position it can
	/// serve.
	#[codec(index = 2)]
	OutsideHorizon { lowest: MessagePosition },
	/// `start` is past the stream's leaf count under `under`.
	#[codec(index = 3)]
	BeyondHead,
}

/// Verify a [`MessagesResponse`] against its [`MessagesRequest`]:
/// - for `start: Some(pos)`, `resp.frontier.leaf_count() == pos.0`;
/// - for `start: None` (a head request), the stream under `under` ends at
///   `resp.frontier.leaf_count() + 1`;
/// - total payload bytes are within `req.max_bytes` (`0` means proofs only);
/// - the payloads, `extension` and `tree_proof` recompute to `req.under` for `req.stream`.
///
/// For a channel fetch, use [`verify_channel_messages`] to also check that `resp.frontier`
/// equals the stored frontier from `consumed_streams()`.
pub fn verify_messages(req: &MessagesRequest, resp: &MessagesResponse) -> Result<(), VerifyError> {
	let base = resp.frontier.leaf_count();
	match req.start {
		Some(start) if base != start.0 => return Err(VerifyError::UnexpectedBase),
		Some(_) => {},
		None => {
			let after_payloads = base
				.checked_add(resp.payloads.len() as u64)
				.ok_or(VerifyError::MalformedResponse)?;
			let end_count = if resp.extension.leaf_count == 0 {
				after_payloads
			} else {
				resp.extension.leaf_count
			};
			if base.checked_add(1) != Some(end_count) {
				return Err(VerifyError::NotAtHead);
			}
		},
	}

	let total = resp.payloads.iter().map(|p| p.len() as u64).fold(0u64, u64::saturating_add);
	if total > req.max_bytes as u64 || (req.max_bytes == 0 && !resp.payloads.is_empty()) {
		return Err(VerifyError::ExceedsBudget);
	}

	verify_messages_response(req.stream, req.under, resp)
}

/// Verify a [`MessagesResponse`] for an inbound channel stream against its [`MessagesRequest`] and
/// the receiver's local stored frontier (from `consumed_streams()`).
///
/// Ensures `resp.frontier == *stored_frontier` (both leaf count and peaks) before verifying the
/// response against `under`.
pub fn verify_channel_messages(
	req: &MessagesRequest,
	resp: &MessagesResponse,
	stored_frontier: &MmrFrontier,
) -> Result<(), VerifyError> {
	if resp.frontier != *stored_frontier {
		return Err(VerifyError::FrontierMismatch);
	}
	verify_messages(req, resp)
}

/// Verify a [`MessagesResponse`] for `stream` under `under`: the same check the requires-lift
/// performs. Crate-internal: it does not bind `resp.frontier` to a requested start or head rule;
/// [`verify_messages`] does.
pub(crate) fn verify_messages_response(
	stream: StreamId,
	under: StreamsRoot,
	resp: &MessagesResponse,
) -> Result<(), VerifyError> {
	let mut frontier = resp.frontier.clone();
	// The frontier after the payloads must stay within `MAX_MMR_LEAF_COUNT`, or it would not
	// decode. Checked up front; `try_append` refuses it too.
	frontier
		.leaf_count()
		.checked_add(resp.payloads.len() as u64)
		.filter(|end| *end <= MAX_MMR_LEAF_COUNT)
		.ok_or(VerifyError::MalformedResponse)?;
	// Defense-in-depth against a crafted response inflating the hashed input; the transport also
	// caps total response size.
	resp.payloads.iter().try_for_each(|p| check_payload_len(p))?;

	for payload in &resp.payloads {
		frontier
			.try_append(leaf_hash(payload))
			.map_err(|_| VerifyError::MalformedResponse)?;
	}

	let current = resp.extension.calculate_root(&frontier)?;
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		lift::{build_requires, ConsumptionRecord, Interval, LiftsBySource, RequiresLift},
		mmr::{MmrRoot, SpecMerge},
		streams_root::{gen_stream_proof, streams_root, TreeStep},
		test_utils::{ch, frontier_at, mmr_size},
	};
	use alloc::collections::BTreeMap;
	use codec::Encode;
	use mmr_lib::{
		helper::get_peak_map,
		util::{MemMMR, MemStore},
	};
	use polkadot_core_primitives::Hash;
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
		let leaves: Vec<Hash> = payloads.iter().map(|p| leaf_hash(p)).collect();

		// Sender's current stream root (6 leaves) and the StreamsRoot committing it.
		let r_current = current_root(&leaves, 6);
		let under = streams_root(&BTreeMap::from([(stream, r_current)])).unwrap();
		let (_r, tree_proof) =
			gen_stream_proof(&BTreeMap::from([(stream, r_current)]), stream).unwrap();

		// Fetch: frontier at 2, payloads [2,3] only → a real extension (4 → 6).
		let resp = MessagesResponse {
			frontier: frontier_at(&leaves, 2),
			payloads: vec![payloads[2].clone(), payloads[3].clone()].try_into().unwrap(),
			extension: ancestry(&leaves, 4, 6),
			tree_proof: tree_proof.clone(),
		};
		assert_eq!(verify_messages_response(stream, under, &resp), Ok(()));

		// Requires: receiver consumed messages 2,3 (frontier 2 → 4); lift the endpoint (4 → 6).
		let record = ConsumptionRecord {
			entries: BTreeMap::from([(
				source,
				vec![(
					stream,
					Interval {
						start: frontier_at(&leaves, 2).root(),
						end: frontier_at(&leaves, 4),
					},
				)],
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

	/// A response for the whole stream from `frontier = empty` verifies under the committed root.
	#[test]
	fn messages_response_round_trips_under_streams_root() {
		let payloads: Vec<Vec<u8>> = (0..4u8).map(|i| vec![i, i + 1, i + 2]).collect();

		let store = MemStore::<Hash>::default();
		let mut src = MemMMR::<Hash, SpecMerge>::new(0, &store);
		for p in &payloads {
			src.push(leaf_hash(p)).unwrap();
		}
		let stream_root = bound(src.mmr_size(), src.get_root().unwrap());

		let stream = ch(2000);
		let entries = BTreeMap::from([(stream, stream_root)]);
		let under = streams_root(&entries).unwrap();
		let (_r, tree_proof) = gen_stream_proof(&entries, stream).unwrap();

		let resp = MessagesResponse {
			frontier: MmrFrontier::new(),
			payloads: payloads.clone().try_into().unwrap(),
			extension: MMRExtensionProof::identity(),
			tree_proof,
		};

		assert_eq!(verify_messages_response(stream, under, &resp), Ok(()));
		let mut mmr = MmrFrontier::new();
		for p in &resp.payloads {
			mmr.append(leaf_hash(p));
		}
		assert_eq!(mmr.root().0, stream_root);
	}

	#[test]
	fn verify_messages_binds_base_to_request_start() {
		let payloads: Vec<Vec<u8>> = (0..4u8).map(|i| vec![i, i + 1]).collect();
		let store = MemStore::<Hash>::default();
		let mut src = MemMMR::<Hash, SpecMerge>::new(0, &store);
		for p in &payloads {
			src.push(leaf_hash(p)).unwrap();
		}
		let stream = ch(2000);
		let entries = BTreeMap::from([(stream, bound(src.mmr_size(), src.get_root().unwrap()))]);
		let under = streams_root(&entries).unwrap();
		let (_r, tree_proof) = gen_stream_proof(&entries, stream).unwrap();

		let resp = MessagesResponse {
			frontier: MmrFrontier::new(),
			payloads: payloads.clone().try_into().unwrap(),
			extension: MMRExtensionProof::identity(),
			tree_proof,
		};

		let total_bytes = payloads.iter().map(|p| p.len() as u32).sum::<u32>();

		// start == Some(0) and within budget → dispatches and authenticates.
		let req = MessagesRequest {
			stream,
			under,
			start: Some(MessagePosition(0)),
			max_bytes: total_bytes,
		};
		assert_eq!(verify_messages(&req, &resp), Ok(()));

		// start != frontier.leaf_count() → rejected up front (before any crypto).
		let req_wrong = MessagesRequest {
			stream,
			under,
			start: Some(MessagePosition(2)),
			max_bytes: total_bytes,
		};
		assert_eq!(verify_messages(&req_wrong, &resp), Err(VerifyError::UnexpectedBase));

		// Payloads exceed the byte budget → ExceedsBudget.
		let req_tight = MessagesRequest {
			stream,
			under,
			start: Some(MessagePosition(0)),
			max_bytes: total_bytes - 1,
		};
		assert_eq!(verify_messages(&req_tight, &resp), Err(VerifyError::ExceedsBudget));

		// `max_bytes == 0` requests a payload-free (lift-material) proof, so any payload trips it.
		let req_liftonly =
			MessagesRequest { stream, under, start: Some(MessagePosition(0)), max_bytes: 0 };
		assert_eq!(verify_messages(&req_liftonly, &resp), Err(VerifyError::ExceedsBudget));
	}

	#[test]
	fn verify_channel_messages_enforces_stored_frontier_equality() {
		let t = two_streams();
		let (stream, ref payloads, ref leaves, ref tree_proof) = t.streams[0];

		let stored_frontier = frontier_at(leaves, 2);
		let honest_resp = MessagesResponse {
			frontier: stored_frontier.clone(),
			payloads: vec![payloads[2].clone(), payloads[3].clone()].try_into().unwrap(),
			extension: ancestry(leaves, 4, 6),
			tree_proof: tree_proof.clone(),
		};
		let req = MessagesRequest {
			stream,
			under: t.under,
			start: Some(MessagePosition(2)),
			max_bytes: 1024,
		};

		// Exact match passes.
		assert_eq!(verify_channel_messages(&req, &honest_resp, &stored_frontier), Ok(()));

		// Different leaf count fails.
		let other_count_frontier = frontier_at(leaves, 1);
		assert_eq!(
			verify_channel_messages(&req, &honest_resp, &other_count_frontier),
			Err(VerifyError::FrontierMismatch)
		);

		// Same leaf count but different peaks (e.g. from an alternate branch/fork) fails.
		let alt_leaves = vec![H256::repeat_byte(0xAA), H256::repeat_byte(0xBB)];
		let alt_frontier = frontier_at(&alt_leaves, 2);
		assert_eq!(stored_frontier.leaf_count(), alt_frontier.leaf_count());
		assert_ne!(stored_frontier.peaks(), alt_frontier.peaks());
		assert_eq!(
			verify_channel_messages(&req, &honest_resp, &alt_frontier),
			Err(VerifyError::FrontierMismatch)
		);
	}

	#[test]
	fn verify_messages_enforces_head_rule_for_start_none() {
		let t = two_streams();
		let (stream, ref payloads, ref leaves, ref tree_proof) = t.streams[0];

		// Honest head response for a 6-leaf stream: frontier at 5, payload[5], identity extension.
		let head_resp = MessagesResponse {
			frontier: frontier_at(leaves, 5),
			payloads: vec![payloads[5].clone()].try_into().unwrap(),
			extension: MMRExtensionProof::identity(),
			tree_proof: tree_proof.clone(),
		};
		let head_req = MessagesRequest { stream, under: t.under, start: None, max_bytes: 1024 };
		assert_eq!(verify_messages(&head_req, &head_resp), Ok(()));
		// The verified frontier's leaf_count is the head's position (5).
		assert_eq!(head_resp.frontier.leaf_count(), 5);

		// Budget too small for the head payload: server returns frontier at 5, no payloads, and
		// extension 5 -> 6, proving the head position (5) without the payload.
		let pos_only_resp = MessagesResponse {
			frontier: frontier_at(leaves, 5),
			payloads: BoundedVec::new(),
			extension: ancestry(leaves, 5, 6),
			tree_proof: tree_proof.clone(),
		};
		let small_req = MessagesRequest { stream, under: t.under, start: None, max_bytes: 1 };
		assert_eq!(verify_messages(&small_req, &pos_only_resp), Ok(()));

		// Passing off an older leaf (index 3, with extension 4 -> 6) as the head: rejected by the
		// head rule (`NotAtHead`) even though the proofs verify under `under`.
		let stale_head = fetch(&t.streams[0], 3, 4);
		assert_eq!(verify_messages(&head_req, &stale_head), Err(VerifyError::NotAtHead));

		// Multi-leaf range [4, 5] reaching the head: starts at 4, ends at 6 != 4 + 1 → NotAtHead.
		let two_leaves = fetch(&t.streams[0], 4, 6);
		assert_eq!(verify_messages(&head_req, &two_leaves), Err(VerifyError::NotAtHead));
	}

	#[test]
	fn zero_budget_refuses_even_empty_payloads() {
		let payloads: Vec<Vec<u8>> = vec![Vec::new(), Vec::new()];
		let stream = ch(2000);
		let entries = BTreeMap::from([(
			stream,
			frontier_at(&payloads.iter().map(|p| leaf_hash(p)).collect::<Vec<_>>(), 2)
				.root()
				.0,
		)]);
		let under = streams_root(&entries).unwrap();
		let (_r, tree_proof) = gen_stream_proof(&entries, stream).unwrap();
		let resp = MessagesResponse {
			frontier: MmrFrontier::new(),
			payloads: payloads.clone().try_into().unwrap(),
			extension: MMRExtensionProof::identity(),
			tree_proof,
		};
		let req = |max_bytes| MessagesRequest {
			stream,
			under,
			start: Some(MessagePosition(0)),
			max_bytes,
		};
		assert_eq!(verify_messages(&req(1), &resp), Ok(()));
		assert_eq!(verify_messages(&req(0), &resp), Err(VerifyError::ExceedsBudget));
	}

	#[test]
	fn a_response_past_the_payload_cap_fails_to_decode() {
		let cap = MAX_MESSAGES_PER_RESPONSE as usize;
		let encoded = |count: usize| {
			(
				MmrFrontier::new(),
				vec![Vec::<u8>::new(); count],
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
			src.push(leaf_hash(p)).unwrap();
		}
		let stream = ch(2000);
		let entries = BTreeMap::from([(stream, bound(src.mmr_size(), src.get_root().unwrap()))]);
		let under = streams_root(&entries).unwrap();
		let (_r, tree_proof) = gen_stream_proof(&entries, stream).unwrap();

		let resp = MessagesResponse {
			frontier: MmrFrontier::new(),
			payloads: payloads.try_into().unwrap(),
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
	fn frontier_at_ceiling_rejects_extra_payload_without_panicking() {
		let stream = ch(2000);
		let under = StreamsRoot(H256::repeat_byte(0xff));

		// A frontier at `MAX_MMR_LEAF_COUNT` is a valid frontier, but appending a payload would
		// carry the count past the ceiling. Rejected before anything is appended.
		let at_ceiling = MessagesResponse {
			frontier: MmrFrontier::from_parts(vec![H256::zero(); 1], MAX_MMR_LEAF_COUNT).unwrap(),
			payloads: vec![vec![0x01]].try_into().unwrap(),
			extension: MMRExtensionProof::identity(),
			tree_proof: StreamProof { steps: Default::default() },
		};
		assert_eq!(
			verify_messages_response(stream, under, &at_ceiling),
			Err(VerifyError::MalformedResponse),
		);
	}

	#[test]
	fn fetch_error_variant_indices_are_frozen() {
		assert_eq!(FetchError::UnknownStream.encode(), vec![0x00]);
		assert_eq!(FetchError::UnknownRoot.encode(), vec![0x01]);
		assert_eq!(
			FetchError::OutsideHorizon { lowest: MessagePosition(7) }.encode(),
			vec![0x02, 0x07, 0, 0, 0, 0, 0, 0, 0],
		);
		assert_eq!(FetchError::BeyondHead.encode(), vec![0x03]);
	}

	#[test]
	fn fetch_wire_frozen_layout() {
		let request = MessagesRequest {
			stream: StreamId::Channel { recipient: ParaId::from(2000), domain: 0, num: 0 },
			under: StreamsRoot(H256::repeat_byte(0x11)),
			start: Some(MessagePosition(5)),
			max_bytes: 1024,
		};
		let mut expected = vec![0x00, 0x00, 0x00, 0x07, 0xD0, 0x00, 0x00, 0x00]; // stream
		expected.extend([0x11; 32]); // under
		expected.extend([0x01, 0x05, 0, 0, 0, 0, 0, 0, 0]); // start: Some, u64 LE
		expected.extend([0x00, 0x04, 0, 0]); // max_bytes: u32 LE
		assert_eq!(request.encode(), expected);
		assert_eq!(MessagesRequest::decode(&mut &expected[..]).unwrap(), request);

		let head = MessagesRequest { start: None, ..request };
		let mut expected = vec![0x00, 0x00, 0x00, 0x07, 0xD0, 0x00, 0x00, 0x00];
		expected.extend([0x11; 32]);
		expected.push(0x00); // start: None
		expected.extend([0x00, 0x04, 0, 0]);
		assert_eq!(head.encode(), expected);

		let response = MessagesResponse {
			frontier: MmrFrontier::from_parts(vec![H256::repeat_byte(0x22)], 1).unwrap(),
			payloads: BoundedVec::truncate_from(vec![vec![0xAA]]),
			extension: MMRExtensionProof {
				leaf_count: 3,
				connecting_nodes: BoundedVec::truncate_from(vec![H256::repeat_byte(0x33)]),
			},
			tree_proof: StreamProof {
				steps: BoundedVec::truncate_from(vec![TreeStep {
					split_bit: 7,
					sibling: H256::repeat_byte(0x44),
				}]),
			},
		};
		let mut expected = vec![0x01, 0, 0, 0, 0, 0, 0, 0, 0x04]; // frontier: count, 1 peak
		expected.extend([0x22; 32]);
		expected.extend([0x04, 0x04, 0xAA]); // payloads: 1 payload of 1 byte
		expected.extend([0x0C, 0x04]); // extension: compact count 3, 1 node
		expected.extend([0x33; 32]);
		expected.extend([0x04, 0x07]); // tree_proof: 1 step, split bit 7
		expected.extend([0x44; 32]);
		assert_eq!(response.encode(), expected);
		assert_eq!(MessagesResponse::decode(&mut &expected[..]).unwrap(), response);

		let served: FetchResponse = Ok(response);
		assert_eq!(served.encode(), [&[0x00][..], &expected[..]].concat());
		let refused: FetchResponse = Err(FetchError::BeyondHead);
		assert_eq!(refused.encode(), vec![0x01, 0x03]);
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
			core::array::from_fn(|s| payloads[s].iter().map(|p| leaf_hash(p)).collect());
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
		let extension =
			if end == 6 { MMRExtensionProof::identity() } else { ancestry(leaves, end, 6) };
		MessagesResponse {
			frontier: frontier_at(leaves, base),
			payloads: payloads[base..end].to_vec().try_into().unwrap(),
			extension,
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
			under: t.under,
			start: Some(MessagePosition(2)),
			max_bytes: u32::MAX,
		};
		assert_eq!(verify_messages(&req, &resp), Err(VerifyError::PayloadTooLarge));

		// At the bound the size check passes; the forged payload then fails the proof.
		resp.payloads[1].pop();
		assert_eq!(verify_messages(&req, &resp), Err(VerifyError::RootMismatch));
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
			let req = MessagesRequest {
				stream,
				under: t.under,
				start: Some(MessagePosition(2)),
				max_bytes,
			};
			assert_eq!(verify_messages(&req, &resp), Ok(()));
		}
	}

	#[test]
	fn tree_proof_failure_surfaces_as_treeproof() {
		let t = two_streams();
		let (stream, ..) = t.streams[0];
		let mut resp = fetch(&t.streams[0], 5, 6);
		resp.tree_proof = StreamProof {
			steps: vec![TreeStep { split_bit: 255, sibling: H256::zero() }].try_into().unwrap(),
		};
		assert_eq!(verify_messages_response(stream, t.under, &resp), Err(VerifyError::TreeProof));
	}
}
