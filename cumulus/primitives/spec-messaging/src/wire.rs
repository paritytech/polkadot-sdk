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

//! The off-chain fetch protocol's wire types.
//!
//! Normative for interoperability: the two ends are implemented by
//! different chains' collators. Addressing is by *(stream, position)* and
//! trust is by *provides root* — never by block. Every request names the
//! `StreamsRoot` it is willing to depend on, and every response is
//! independently verifiable against exactly that root: no intermediate
//! state, no trust carried between responses, a bad peer wastes at most one
//! response.

use alloc::vec::Vec;
use polkadot_core_primitives::Hash;
use polkadot_primitives::StreamsRoot;

use crate::{
	mmr::{MMRExtensionProof, MessagePosition, MmrInclusionProof},
	stream_id::StreamId,
	tree::TreeInclusionProof,
};

/// "Give me messages of this stream from `start` on, verifiable under this
/// provides root."
///
/// One request serves ordered fetching AND pure lift material: with
/// `max_bytes = 0` the response is payload-free — exactly the extension +
/// tree proof pair a requires lift carries (empty extension = the
/// tree-proof-only case). Served by the sending chain's full nodes.
#[derive(
	Clone,
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	Debug,
	Eq,
	PartialEq,
	scale_info::TypeInfo,
)]
pub struct MessagesRequest {
	/// The requested stream of the serving chain.
	pub stream: StreamId,
	/// Typically the receiver's frontier leaf count.
	pub start: MessagePosition,
	/// The `StreamsRoot` the response must verify under — the requester's
	/// chosen dependency (newest, or newest *included*, per its tier
	/// policy). The requester can never be handed a dependency on a newer,
	/// possibly unconfirmed root; freshness is its own job: re-request
	/// under a newer root once that root is verified.
	pub under: StreamsRoot,
	/// Response size bound (the server may cap harder) — fetching stays
	/// chunked and resumable no matter how large the backlog. `0` requests
	/// pure lift material.
	pub max_bytes: u32,
}

/// Payloads from `base` on, plus the proofs binding them — and everything
/// before them — to the requested root.
///
/// Also the push-path object, unsolicited on block production, alongside
/// the new header: the receiver authenticates the header first and then
/// verifies the response against its digest root — exactly as if it had
/// pulled under it. The trust direction never inverts.
///
/// Verification (node-side): hash each payload into its leaf and append to
/// the tracked frontier in order; verify `extension` from the recomputed
/// frontier (yielding the stream's root under the target), walk
/// `tree_proof` from that (yielding a `StreamsRoot`), and compare against
/// the root the request named. A mismatch anywhere discards the response
/// and the peer.
#[derive(
	Clone,
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	Debug,
	Eq,
	PartialEq,
	scale_info::TypeInfo,
)]
pub struct MessagesResponse {
	/// Position of the first payload. Trust-free hint (a lie only fails the
	/// proofs); self-description for the push path.
	pub base: MessagePosition,
	/// Leaf-format version for these payloads — a trust-free hint like
	/// `base`: versions are hash-disjoint domains, so only the correct one
	/// can reproduce a committed root. A response never spans a version
	/// change; the server splits there.
	pub leaf_version: u8,
	/// The payloads (XCM or other data), in MMR order. Payload i has
	/// position `base + i`: gaps are unrepresentable, mirroring the
	/// sender's storage layout.
	pub payloads: Vec<Vec<u8>>,
	/// The stream's peak set at `base` — always present (≤ 64 hashes).
	/// Necessary for consumers holding no frontier (event subscribers
	/// fetching a range); for channel receivers a work-saving cross-check:
	/// compare against the own frontier before hashing the payloads.
	/// Trust-free either way: fabricated peaks cannot extend to a committed
	/// root.
	pub start_peaks: Vec<Hash>,
	/// From the frontier recomputed over `payloads` to the stream's entry
	/// under `under`; empty when the payloads reach it.
	pub extension: MMRExtensionProof,
	/// Yields, walked from the extension's output, the `StreamsRoot` the
	/// response verifies under — compared against the requested root.
	pub tree_proof: TreeInclusionProof,
}

/// "Give me one event of this stream, with proofs, under this provides
/// root."
///
/// The lossy consumer's request: subscribers verify by inclusion proof, not
/// recomputation — no frontier needed. Ack-register reads are exactly this
/// request pointed at the peer's `Ack` stream. Deliberately single-leaf:
/// ranges go through [`MessagesRequest`] + `start_peaks`.
#[derive(
	Clone,
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	Debug,
	Eq,
	PartialEq,
	scale_info::TypeInfo,
)]
pub struct EventRequest {
	/// The requested stream of the serving chain.
	pub stream: StreamId,
	/// The `StreamsRoot` the response must prove against. Always explicit:
	/// the receiver names what it is willing to depend on.
	pub under: StreamsRoot,
	/// Specific position, or `None` for the head as of `under`. The request
	/// is a pure function of `(stream, under, at)`: it either serves or
	/// fails (root unknown, or outside the serving horizon); nothing is
	/// resolved server-side.
	pub at: Option<MessagePosition>,
}

/// One event leaf with the proofs placing it under the requested root.
///
/// Head-ness comes with the check: `under` fixes the stream's leaf count,
/// the head is the leaf at `count − 1` — an old leaf cannot be served as
/// the head under that root.
#[derive(
	Clone,
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	Debug,
	Eq,
	PartialEq,
	scale_info::TypeInfo,
)]
pub struct EventResponse {
	/// The event payload; its position is proven by the inclusion proof.
	pub payload: Vec<u8>,
	/// MMR inclusion proof to the stream's root (sibling path plus the
	/// other peaks for root bagging).
	pub inclusion: MmrInclusionProof,
	/// Places the stream's root (computed from `inclusion`) under `under`.
	pub tree_proof: TreeInclusionProof,
}
