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

//! Primitives for the Speculative Messaging protocol.
//!
//! Speculative Messaging lets parachains exchange messages without waiting for
//! full relay-chain confirmation. Each sender parachain accumulates its outgoing
//! messages per stream into a Merkle Mountain Range (MMR) and commits every
//! stream's root into a single `StreamsRoot` — a keyed commitment tree
//! (`polkadot_primitives::v9::StreamsRoot` — relay-visible, so it lives in
//! `polkadot-primitives`). The relay chain then matches sender commitments
//! (`Provides`) against receiver expectations (`Requires`/`RequiresSet`), allowing
//! both sides to process messages speculatively and confirm them after the fact.
//!
//! This crate holds the **parachain-side** primitives that build those
//! commitments and the off-chain wire types.
//!
//! # Key types
//!
//! - [`mmr::SpecMerge`] — the domain-tagged `mmr_lib::Merge` that backs each per-stream MMR;
//!   [`mmr::root_from_peaks`] derives a stream root from peaks-only state. Inclusion and ancestry
//!   proofs come from `mmr_lib` itself.
//! - [`message`] — the off-chain protocol messages: the fetch protocol
//!   (`MessagesRequest`/`MessagesResponse` + [`message::verify_messages_response`]), the lossy
//!   event/register read (`EventRequest`/`EventResponse` + [`message::verify_event_response`]), the
//!   `/spec-msg/exchange` envelope (`ExchangeRequest`/`ExchangeResponse`) multiplexing the two, and
//!   the protocol's concrete instantiation (`SpecHasher`, [`message::leaf_hash`] for a payload →
//!   MMR leaf, the payload bound).
//! - [`lift`] — the requires-lift / consumption-record POV types (`RequiresLift`,
//!   `ConsumptionRecord`, `MMRExtensionProof`, `MmrInclusionProof`, …) and the `build_requires`
//!   synthesizer.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod flow_control;
pub mod lift;
pub mod message;
pub mod mmr;
pub mod stream;
pub mod streams_root;

pub use flow_control::{Register, SpecMsgKind, SpecMsgSignal, WindowGrant};
pub use lift::{
	build_requires, build_requires_entry, stitch, ConsumptionRecord, Interval, LiftError,
	MMRExtensionProof, MmrFrontier, MmrInclusionProof, MmrRoot, ProofError, RequiresLift,
	TreeInclusionProof,
};
pub use message::{
	leaf_hash, verify_event, verify_event_response, verify_exchange, verify_messages,
	verify_messages_response, verify_positional_event_response, EventRequest, EventResponse,
	ExchangeRequest, ExchangeResponse, ExchangeVerified, MaxSpeculativeMessageLen, MessagesRequest,
	MessagesResponse, SpecHasher, VerifiedEvent, VerifyError, MAX_SPECULATIVE_MESSAGE_LEN,
};
pub use mmr::MessagePosition;
pub use stream::{StreamId, STREAM_ID_LEN};
pub use streams_root::{StreamProof, StreamsRoot};

// Domain Tags to ensure that the same message structure used in different contexts (e.g. leaf vs
// inner node) do not collide on the same hash. Tag values are part of the hash preimages. The MMR
// tags are `LEAF/INNER/PEAK/EMPTY = 0x1/0x2/0x3/0x4` and the commitment-tree tags `0x5/0x6`,
// matching the poc-mvp branch (encoding spec §1).

/// Tag for a leaf node.
pub const LEAF_TAG: u8 = 0x1;

/// Tag for an inner node.
pub const INNER_TAG: u8 = 0x2;

/// Tag for a peak.
pub const PEAK_TAG: u8 = 0x3;

/// Tag hashed to produce the defined root of an *empty* MMR frontier, `H(EMPTY_TAG)`
/// (see [`mmr::empty_root`]): `mmr_lib` errors on empty MMRs, but the protocol needs a
/// comparable value — the `Interval.start` of a stream's first-ever consumption is
/// exactly this root.
pub const EMPTY_TAG: u8 = 0x4;

/// Tag for a `StreamsRoot` commitment-tree leaf: `H(STREAMS_LEAF_TAG ++ key[8] ++
/// stream_root[32])`. Distinct from [`LEAF_TAG`] so a trie leaf can never collide with an MMR
/// message leaf.
pub const STREAMS_LEAF_TAG: u8 = 0x5;

/// Tag for a `StreamsRoot` commitment-tree inner (branch) node:
/// `H(STREAMS_INNER_TAG ++ split_bit ++ left[32] ++ right[32])`. Distinct from [`INNER_TAG`].
pub const STREAMS_INNER_TAG: u8 = 0x6;

// Leaf versioning to allow for future changes to the leaf structure without
// breaking compatibility with old messages.

/// Leaf Version.
pub const LEAF_VERSION: u8 = 0x0;

/// Consensus-digest engine id carrying the sender's **`StreamsRoot`** (spec-msg v0.5): the root of
/// a keyed commitment tree over `(StreamId, current stream root)` for every active stream,
/// committed each block so the head is self-sufficient. A foreign receiver reads this root from the
/// header (anchored by the relay's `para_heads`/`paras::Heads`) and proves the stream it consumes
/// against it, obtaining the sender's committed stream root without a sender-state proof (see the
/// `streams_root` module).
///
/// This is the sole spec-msg header digest in v0.5 (it supersedes the pre-v0.5 flat-`provides`
/// digest); the name matches the design's `SPMS_ENGINE_ID`.
///
/// A foreign node verifies against this digest **directly** — a pure function of header, response
/// and proofs, no chain state — so its format is **protocol-standard, not chain-internal**: the
/// engine-id value is consensus-visible across chains, frozen once anything cross-chain ships.
/// `*b"SPMS"` is the encoding spec's proposed-final value (§7.4), pending design sign-off.
pub const SPMS_ENGINE_ID: sp_runtime::ConsensusEngineId = *b"SPMS";
