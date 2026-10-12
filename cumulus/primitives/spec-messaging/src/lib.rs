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

//! Primitives for the Speculative Messaging protocol (v0.5).
//!
//! A sender parachain accumulates each outgoing stream into an MMR and commits every stream's root
//! into one `StreamsRoot`, a keyed commitment tree. The relay chain matches sender commitments
//! (`Provides`) against receiver expectations (`Requires`), so both sides can process messages
//! speculatively and confirm them later. `StreamsRoot` and `RequiresSet` are relay-visible and live
//! in `polkadot-primitives`; this crate holds the parachain-side primitives that build them and the
//! off-chain wire types.
//!
//! # Modules
//!
//! - [`stream`] — `StreamId` and its canonical 8-byte encoding.
//! - [`streams_root`] — the keyed commitment tree and its membership proofs.
//! - [`mmr`] — `MmrFrontier`, the peaks-only per-stream MMR state; `SpecMerge`, the tagged
//!   `mmr_lib::Merge` behind it.
//! - [`message`] — the off-chain protocol: fetch (`MessagesRequest`/`MessagesResponse`), event read
//!   (`EventRequest`/`EventResponse`), the `/spec-msg/exchange` envelope, and their verifiers.
//! - [`lift`] — the PoV-carried requires lifts, the consumption record, and `build_requires`.
//! - [`inherent`] — the node/runtime consumption interface: `ConsumedStream` out,
//!   `MessagingInherentData` in.
//! - [`channel`] — `ChannelId` and the per-direction channel states.
//! - [`flow_control`] — the channel-stream leaf payload and the receiver's `Register`.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod channel;
pub mod flow_control;
pub mod inherent;
pub mod lift;
pub mod message;
pub mod mmr;
pub mod stream;
pub mod streams_root;

pub use channel::{ChannelId, ChannelPhase, InChannelState, OutChannelState};
pub use flow_control::{Register, SpecMsgKind, SpecMsgSignal, WindowGrant};
pub use inherent::{
	ConsumeItem, ConsumedStream, MessagingInherentData, Payload, INHERENT_IDENTIFIER,
};
pub use lift::{
	build_requires, build_requires_entry, stitch, ConsumptionRecord, Interval, LiftError,
	LiftsBySource, MMRExtensionProof, MmrInclusionProof, ProofError, RequiresLift, SourceStreams,
	TreeInclusionProof,
};
pub use message::{
	leaf_hash, verify_event, verify_exchange, verify_messages, EventRequest, EventResponse,
	ExchangeRequest, ExchangeResponse, ExchangeVerified, MaxSpeculativeMessageLen, MessagesRequest,
	MessagesResponse, VerifiedEvent, VerifyError, MAX_SPECULATIVE_MESSAGE_LEN,
};
pub use mmr::{MessagePosition, MmrFrontier, MmrRoot};
pub use stream::{PrivateKind, StreamId, STREAM_ID_LEN};
pub use streams_root::{StreamProof, StreamsRoot};

/// The hash function for all of speculative messaging: leaves, MMR merges, stream and
/// commitment-tree roots. A protocol constant. Changing it is a consensus break, so nothing here
/// is generic over it.
pub type SpecHasher = sp_runtime::traits::BlakeTwo256;

// Domain tags. Each hash preimage starts with one, so a node in one role can never collide with a
// node in another (encoding spec §1).

/// MMR leaf.
pub const LEAF_TAG: u8 = 0x1;

/// MMR inner node.
pub const INNER_TAG: u8 = 0x2;

/// MMR peak bagging.
pub const PEAK_TAG: u8 = 0x3;

/// The empty MMR's root is `H(EMPTY_TAG)` ([`mmr::empty_root`]). `mmr_lib` has no root for an
/// empty MMR; the protocol needs one, since a stream's first consumption starts there.
pub const EMPTY_TAG: u8 = 0x4;

/// Commitment-tree leaf: `H(STREAMS_LEAF_TAG ++ key[8] ++ stream_root[32])`.
pub const STREAMS_LEAF_TAG: u8 = 0x5;

/// Commitment-tree inner node: `H(STREAMS_INNER_TAG ++ split_bit ++ left[32] ++ right[32])`.
pub const STREAMS_INNER_TAG: u8 = 0x6;

/// Leaf preimage version. Versions are hash-disjoint, so the layout can change without old leaves
/// colliding with new ones.
pub const LEAF_VERSION: u8 = 0x0;

/// Engine id of the consensus digest carrying the sender's `StreamsRoot`, deposited every block.
/// A receiver reads the root from the sender's header, anchored by the relay's `para_heads`, and
/// proves its stream against it. Foreign nodes parse this digest directly, so the format is
/// protocol-standard and frozen once anything cross-chain ships. `*b"SPMS"` is the encoding
/// spec's proposed value (§7.4).
pub const SPMS_ENGINE_ID: sp_runtime::ConsensusEngineId = *b"SPMS";
