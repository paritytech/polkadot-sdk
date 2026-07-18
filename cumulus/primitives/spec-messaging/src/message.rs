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

//! Off-chain message types + the protocol's concrete instantiation.
//!
//! Two things live here, both **off-chain** (the relay chain decodes none of them — it only matches
//! the `StreamsRoot` commitments in the `Provides`/`Requires` UMP signals):
//!
//! - **Concrete instantiation** — [`SpecHasher`] (the hasher), [`MaxSpeculativeMessageLen`] (the
//!   payload bound), and [`leaf_hash`] (a payload → MMR leaf). A v0.5 message is *just a bounded
//!   payload keyed by `StreamId`*; there is no message struct — source / stream / position are all
//!   structural (the sender's outbox, the `StreamId` key, the MMR leaf index), so only the payload
//!   and its hash are primitives.
//! - **Fetch protocol** — [`MessagesRequest`] / [`MessagesResponse`]: a collator fetches a range of
//!   a source stream's messages and authenticates the response ([`verify_messages_response`])
//!   against a `StreamsRoot` it has independently verified, reusing the same `extension` +
//!   `tree_proof` machinery as the requires-lift (see [`crate::lift`]). The lossy event/register
//!   read path (`EventRequest`/`EventResponse`) lands with the event-stream subsystem.

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use polkadot_core_primitives::Hash;
use scale_info::TypeInfo;
use sp_core::ConstU32;
use sp_runtime::traits::Hash as HashT;

use crate::{
	lift::{MMRExtensionProof, MmrFrontier},
	mmr::{Mmr, MmrAccumulator},
	stream::StreamId,
	streams_root::{streams_root_from_proof, StreamProof, StreamsRoot},
	LEAF_TAG,
};

/// Maximum size in bytes of a single speculative message payload.
pub const MAX_SPECULATIVE_MESSAGE_LEN: u32 = 102_400;

/// Bound for a single speculative message payload.
pub type MaxSpeculativeMessageLen = ConstU32<MAX_SPECULATIVE_MESSAGE_LEN>;

/// The hash function used throughout speculative messaging (leaf hashing, MMR merges, stream
/// roots). The crate primitives are generic over the hasher; this alias is the single concrete
/// choice for the protocol, so switching (e.g. to Keccak256) is a one-line change here.
pub type SpecHasher = sp_runtime::traits::BlakeTwo256;

/// Hash a payload into a stream MMR leaf under `leaf_version`: `H(LEAF_TAG ++ leaf_version ++
/// payload)`.
///
/// A pure function of `(leaf_version, payload)` — the only thing v0.5 needs of a "message". Source,
/// stream, and position are structural (the sender's outbox, the `StreamId` key, the MMR leaf
/// index), so none are in the preimage. `leaf_version` domains are hash-disjoint, so only the
/// correct version reproduces a committed root.
pub fn leaf_hash<H: HashT<Output = Hash>>(leaf_version: u8, payload: &[u8]) -> Hash {
	let mut preimage = Vec::new();
	preimage.extend_from_slice(&LEAF_TAG.to_le_bytes());
	preimage.extend_from_slice(&leaf_version.to_le_bytes());
	preimage.extend_from_slice(payload);
	<H as HashT>::hash(&preimage)
}

/// A leaf position within a stream's MMR (its leaf count). Newtype, per the design's
/// `MessagePosition` — a position is not an arbitrary `u64`.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Debug,
	Default,
	TypeInfo,
)]
pub struct MessagePosition(pub u64);

/// Off-chain request for a range of a stream's messages, to be verified under a chosen
/// `StreamsRoot`.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub struct MessagesRequest {
	/// The requested stream of the serving chain.
	pub stream: StreamId,
	/// Where to start — typically the receiver's frontier leaf count.
	pub start: MessagePosition,
	/// The `StreamsRoot` the response must verify under: the requester's chosen dependency, a root
	/// it has independently authenticated — never a newer, possibly-unconfirmed one (freshness is
	/// the requester's own job: re-request under a newer root once verified).
	pub under: StreamsRoot,
	/// Response size bound (the server may cap harder); fetching stays chunked and resumable no
	/// matter how large the backlog. `0` requests a payload-free proof (lift material).
	pub max_bytes: u32,
}

/// Off-chain response: payloads from `base` on, plus the proofs binding them — and everything
/// before them — to the requested `StreamsRoot`. Independently verifiable via
/// [`verify_messages_response`]; nothing is trusted (fabricated peaks/payloads/proofs cannot
/// reproduce a committed root).
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub struct MessagesResponse {
	/// Position of the first payload (trust-free hint; a lie only fails the proofs).
	pub base: MessagePosition,
	/// Leaf-format version for these payloads (trust-free hint: versions are hash-disjoint, so
	/// only the correct one reproduces a committed root). A response never spans a version
	/// change.
	pub leaf_version: u8,
	/// The payloads, in MMR order; payload `i` has position `base + i`.
	pub payloads: Vec<Vec<u8>>,
	/// The stream's peak set at `base` (≤ 64 hashes). Lets a consumer holding no frontier
	/// recompute; fabricated peaks cannot extend to a committed root.
	pub start_peaks: Vec<Hash>,
	/// From the frontier recomputed over `payloads` to the stream's entry under `under`; the
	/// identity extension when the payloads already reach it.
	pub extension: MMRExtensionProof,
	/// Walked from the extension's output, yields the `StreamsRoot` the response verifies under.
	pub tree_proof: StreamProof,
}

/// Verify a [`MessagesResponse`] for `stream` under `under`, returning the authenticated payloads
/// on success.
///
/// Recomputes the stream frontier from `start_peaks` + the hashed `payloads`, extends it to the
/// stream's current root (`extension`), and walks the keyed tree (`tree_proof`) to a `StreamsRoot`
/// — which must equal `under`. This is the same `extension` + `tree_proof` check the requires-lift
/// performs, so a fetched batch and a consumption lift authenticate identically.
pub fn verify_messages_response(
	stream: StreamId,
	under: StreamsRoot,
	resp: &MessagesResponse,
) -> Option<Vec<Vec<u8>>> {
	// `start_peaks` and `base` are untrusted. A well-formed frontier has exactly one peak per set
	// bit of its leaf count; reject any other shape so a crafted response cannot drive
	// `Mmr::append` into a pop-from-empty panic. This equality also bounds `start_peaks` to ≤ 64
	// (a `u64` has ≤ 64 set bits), enforcing the documented peak-set limit. Also reject a `base +
	// len` that would overflow the leaf counter (only reachable near `u64::MAX`, but keeps the
	// accumulator arithmetic total). A rejected response is simply unverifiable — the correct
	// outcome, reached without panicking.
	if resp.start_peaks.len() != resp.base.0.count_ones() as usize {
		return None;
	}
	resp.base.0.checked_add(resp.payloads.len() as u64)?;

	// Frontier at `base` (the response's peak set), then append the response's payloads as leaves.
	let mut mmr = Mmr::<SpecHasher>::from_parts(resp.start_peaks.clone(), resp.base.0);
	for payload in &resp.payloads {
		mmr.append(leaf_hash::<SpecHasher>(resp.leaf_version, payload));
	}
	let (peaks, leaf_count) = mmr.into_parts();
	let frontier = MmrFrontier { peaks, leaf_count };

	// Extend to the stream's current root, then walk the tree to the committed `StreamsRoot`.
	let current = resp.extension.verify(&frontier)?;
	let root = streams_root_from_proof(stream, current.0, &resp.tree_proof)?;
	(root == under).then(|| resp.payloads.clone())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		lift::{build_requires, ConsumptionRecord, Interval, RequiresLift},
		mmr::{root_from_peaks, SpecMerge},
		streams_root::{gen_stream_proof, streams_root},
	};
	use alloc::collections::BTreeMap;
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
		let mut mmr = Mmr::<SpecHasher>::new();
		for l in &leaves[..k] {
			mmr.append(*l);
		}
		let (peaks, leaf_count) = mmr.into_parts();
		MmrFrontier { peaks, leaf_count }
	}

	/// The stream root over the first `n` leaves.
	fn current_root(leaves: &[Hash], n: usize) -> Hash {
		let store = MemStore::<Hash>::default();
		let mut mmr = MemMMR::<Hash, SpecMerge<SpecHasher>>::new(0, &store);
		for l in &leaves[..n] {
			mmr.push(*l).unwrap();
		}
		mmr.get_root().unwrap()
	}

	/// O(log n) ancestry extension from `k` leaves to `n` leaves.
	fn ancestry(leaves: &[Hash], k: usize, n: usize) -> MMRExtensionProof {
		let store = MemStore::<Hash>::default();
		let mut mmr = MemMMR::<Hash, SpecMerge<SpecHasher>>::new(0, &store);
		for l in &leaves[..n] {
			mmr.push(*l).unwrap();
		}
		let ap = mmr.gen_ancestry_proof(mmr_size(k)).unwrap();
		MMRExtensionProof {
			new_mmr_size: ap.prev_peaks_proof.mmr_size(),
			connecting_nodes: ap.prev_peaks_proof.proof_items().to_vec(),
		}
	}

	/// End-to-end composition, exercising the non-trivial branches (`base > 0`, a *real* extension
	/// bridging an unconsumed tail): a sender stream of 6 messages commits a `StreamsRoot`; a
	/// **partial** fetch (base 2, messages 2..3, extension bridging 4→6) verifies under that root;
	/// and the receiver's consumption of those messages lifts (the same 4→6 extension +
	/// `tree_proof`) into the **same** `StreamsRoot` via `build_requires`. Fetch and requires run
	/// the identical `extension` + `tree_proof` check — so this demonstrates they compose.
	#[test]
	fn composition_fetch_verify_then_build_requires() {
		let source = ParaId::from(1000);
		let stream = ch(2000);

		let payloads: Vec<Vec<u8>> = (0..6u8).map(|i| vec![i, i.wrapping_add(10)]).collect();
		let leaves: Vec<Hash> = payloads.iter().map(|p| leaf_hash::<SpecHasher>(0, p)).collect();

		// Sender's current stream root (6 leaves) and the StreamsRoot committing it.
		let r_current = current_root(&leaves, 6);
		let under = streams_root(vec![(stream, r_current)]).unwrap();
		let (_r, tree_proof) = gen_stream_proof(vec![(stream, r_current)], stream).unwrap();

		// Fetch: base 2 (non-empty start_peaks), payloads [2,3] only → a real extension (4 → 6).
		let resp = MessagesResponse {
			base: MessagePosition(2),
			leaf_version: 0,
			payloads: vec![payloads[2].clone(), payloads[3].clone()],
			start_peaks: frontier_at(&leaves, 2).peaks,
			extension: ancestry(&leaves, 4, 6),
			tree_proof: tree_proof.clone(),
		};
		assert_eq!(
			verify_messages_response(stream, under, &resp),
			Some(vec![payloads[2].clone(), payloads[3].clone()]),
		);

		// Requires: receiver consumed messages 2,3 (frontier 2 → 4); lift the endpoint (4 → 6).
		let record = ConsumptionRecord {
			entries: BTreeMap::from([(
				source,
				vec![(
					stream,
					Interval {
						start: frontier_at(&leaves, 2).root().unwrap(),
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

		let requires = build_requires(&[record], &lifts).unwrap();
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
		assert_eq!(leaf_hash::<SpecHasher>(0, payload), <SpecHasher as HashT>::hash(&pre));
		// Different version → different leaf (hash-disjoint domains).
		assert_ne!(leaf_hash::<SpecHasher>(0, payload), leaf_hash::<SpecHasher>(1, payload));
	}

	/// End-to-end: a sender builds a stream MMR, commits its root into a `StreamsRoot`, and serves
	/// a response covering the whole stream from `base = 0`; the receiver authenticates it under
	/// the committed root and recovers the payloads.
	#[test]
	fn messages_response_round_trips_under_streams_root() {
		let payloads: Vec<Vec<u8>> = (0..4u8).map(|i| vec![i, i + 1, i + 2]).collect();

		// Sender stream MMR over the payload leaves; its root is the stream root.
		let store = MemStore::<Hash>::default();
		let mut src = MemMMR::<Hash, SpecMerge<SpecHasher>>::new(0, &store);
		for p in &payloads {
			src.push(leaf_hash::<SpecHasher>(0, p)).unwrap();
		}
		let stream_root = src.get_root().unwrap();

		let stream = ch(2000);
		let entries = vec![(stream, stream_root)];
		let under = streams_root(entries.clone()).unwrap();
		let (_r, tree_proof) = gen_stream_proof(entries, stream).unwrap();

		// Full stream from base 0: empty start_peaks, identity extension (payloads reach the root).
		let resp = MessagesResponse {
			base: MessagePosition(0),
			leaf_version: 0,
			payloads: payloads.clone(),
			start_peaks: Vec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof,
		};

		assert_eq!(verify_messages_response(stream, under, &resp), Some(payloads));
		// Sanity: the recomputed root matches the sender's.
		let mut mmr = Mmr::<SpecHasher>::new();
		for p in &resp.payloads {
			mmr.append(leaf_hash::<SpecHasher>(0, p));
		}
		assert_eq!(root_from_peaks::<SpecHasher>(mmr.peaks()), stream_root);
	}

	#[test]
	fn messages_response_rejects_wrong_root_or_tampered_payload() {
		let payloads: Vec<Vec<u8>> = (0..3u8).map(|i| vec![i]).collect();
		let store = MemStore::<Hash>::default();
		let mut src = MemMMR::<Hash, SpecMerge<SpecHasher>>::new(0, &store);
		for p in &payloads {
			src.push(leaf_hash::<SpecHasher>(0, p)).unwrap();
		}
		let stream = ch(2000);
		let entries = vec![(stream, src.get_root().unwrap())];
		let under = streams_root(entries.clone()).unwrap();
		let (_r, tree_proof) = gen_stream_proof(entries, stream).unwrap();

		let resp = MessagesResponse {
			base: MessagePosition(0),
			leaf_version: 0,
			payloads,
			start_peaks: Vec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof,
		};

		// Wrong `under`.
		assert_eq!(
			verify_messages_response(stream, StreamsRoot(H256::repeat_byte(0xff)), &resp),
			None
		);
		// Tampered payload → recomputed root diverges → fails.
		let mut bad = resp.clone();
		bad.payloads[0] = vec![0xAA];
		assert_eq!(verify_messages_response(stream, under, &bad), None);
	}

	#[test]
	fn malformed_frontier_rejects_without_panicking() {
		let stream = ch(2000);
		let under = StreamsRoot(H256::repeat_byte(0xff));

		// `start_peaks` shape inconsistent with `base` (empty peaks, base = 1): the pre-fix
		// pop-from-empty panic in `Mmr::append`. Must return None, not panic.
		let crafted = MessagesResponse {
			base: MessagePosition(1),
			leaf_version: 0,
			payloads: vec![vec![0x01]],
			start_peaks: Vec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof: StreamProof { steps: Vec::new() },
		};
		assert_eq!(verify_messages_response(stream, under, &crafted), None);

		// `base + payloads` overflowing the leaf counter must also reject cleanly. `base =
		// u64::MAX` has 64 set bits, so give 64 peaks to pass the shape check and hit the
		// overflow guard.
		let overflow = MessagesResponse {
			base: MessagePosition(u64::MAX),
			leaf_version: 0,
			payloads: vec![vec![0x01]],
			start_peaks: vec![H256::zero(); 64],
			extension: MMRExtensionProof::identity(),
			tree_proof: StreamProof { steps: Vec::new() },
		};
		assert_eq!(verify_messages_response(stream, under, &overflow), None);
	}
}
