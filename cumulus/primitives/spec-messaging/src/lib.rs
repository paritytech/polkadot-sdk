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

//! Primitives for the Speculative Messaging protocol (design v0.5).
//!
//! Speculative Messaging replaces HRMP: each parachain accumulates its
//! outgoing messages in per-stream Merkle Mountain Ranges (MMRs), commits to
//! *all* of them with one hash per block — the [`StreamsRoot`], root of a
//! keyed commitment tree over the stream roots — and payloads travel
//! off-chain between collators. The relay chain only matches receiver
//! `Requires` entries against a small window of recent sender
//! `StreamsRoot`s; everything below the hash is proven parachain-side.
//!
//! # What this crate provides
//!
//! - [`StreamId`] — the structured, relay-invisible stream identifier with its manual, canonical,
//!   consensus-critical 8-byte encoding.
//! - MMR machinery ([`mmr`]): [`mmr::SpecHasher`] / [`mmr::SpecMerge`] (domain-tagged merging over
//!   `mmr_lib`), [`mmr::hash_leaf`], [`MmrRoot`], [`MessagePosition`], [`MmrFrontier`],
//!   [`MmrInclusionProof`] and [`MMRExtensionProof`].
//! - The stream commitment tree ([`tree`]): consensus-critical node hashing, a reference
//!   builder/prover and [`TreeInclusionProof`].
//! - The consumption record ([`record`]): [`Interval`], [`ConsumptionRecord`] — what a block's
//!   consumption did, stitched and lifted by the `validate_block` wrapper.
//! - Requires lifts ([`lift`]): [`RequiresLift`] and its canonical per-source transport
//!   [`LiftsBySource`], carried in the POV.
//! - The fetch protocol wire types ([`wire`]): [`MessagesRequest`] / [`MessagesResponse`],
//!   [`EventRequest`] / [`EventResponse`] — every response independently verifiable against a
//!   requester-named root.
//! - Channel-layer payloads ([`channel`]): [`SpecMsgKind`], [`SpecMsgSignal`], [`Register`],
//!   [`WindowGrant`], [`ChannelId`].
//!
//! The relay-side commitment types ([`StreamsRoot`], `RequiresSet`) live in
//! `polkadot-primitives` (they are embedded in `UMPSignal`s) and are
//! re-exported here for convenience.

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod channel;
pub mod lift;
pub mod mmr;
pub mod record;
pub mod stream_id;
pub mod tree;
pub mod wire;

pub use channel::{ChannelId, Register, SpecMsgKind, SpecMsgSignal, WindowGrant};
pub use lift::{LiftsBySource, RequiresLift};
pub use mmr::{
	hash_leaf, MMRExtensionProof, MessagePosition, MmrError, MmrFrontier, MmrInclusionProof,
	MmrRoot, SpecHasher, SpecMerge,
};
pub use record::{ConsumptionRecord, Interval};
pub use stream_id::{StreamId, StreamIdError, STREAM_ID_LEN};
pub use tree::{
	compute_streams_root, prove_stream, tree_inner_hash, tree_leaf_hash, TreeError,
	TreeInclusionProof,
};
pub use wire::{EventRequest, EventResponse, MessagesRequest, MessagesResponse};

// Relay-side commitment types (UMPSignal-embedded), re-exported for
// convenience.
pub use polkadot_primitives::{RequiresSet, RequiresSetError, StreamsRoot};

// Domain tags: the same message structure used in different contexts (leaf
// vs inner node, message MMR vs commitment tree) must never collide on the
// same hash. See the design's "Leaf Hashing and Domain Separation" for the
// attack this prevents (RFC 6962 §2.1-style tagging).

/// Tag for a message MMR leaf node.
pub const LEAF_TAG: u8 = 0x1;

/// Tag for a message MMR inner node.
pub const INNER_TAG: u8 = 0x2;

/// Tag for message MMR peak bagging.
pub const PEAK_TAG: u8 = 0x3;

/// Tag hashed to produce the defined root of an *empty* MMR frontier
/// (`mmr_lib` errors on empty MMRs; the protocol needs a value).
pub const EMPTY_TAG: u8 = 0x4;

/// Tag for a stream commitment tree leaf node.
pub const TREE_LEAF_TAG: u8 = 0x5;

/// Tag for a stream commitment tree inner node.
pub const TREE_INNER_TAG: u8 = 0x6;

/// Version of the leaf *preimage layout* (`LEAF_TAG ++ LEAF_VERSION ++
/// payload`), not of the payload. Makes format epochs hash-disjoint; must be
/// present from leaf #0 (the MMR is append-only and can never be rehashed).
pub const LEAF_VERSION: u8 = 0x0;

/// Consensus engine id of the header digest carrying the sender's
/// [`StreamsRoot`] (`DigestItem::Consensus(SPMS_ENGINE_ID, streams_root)`),
/// at most one per header. Protocol standard, not chain-internal: foreign
/// nodes verify fetch responses against this digest directly.
pub const SPMS_ENGINE_ID: sp_runtime::ConsensusEngineId = *b"SPMS";
