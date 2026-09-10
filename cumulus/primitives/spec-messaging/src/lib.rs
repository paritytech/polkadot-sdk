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
//! messages per destination into a Merkle Mountain Range (MMR) and commits the
//! resulting root into a `CommitmentSet` (defined in `polkadot-primitives`). The relay chain
//! then matches sender commitments against receiver expectations, allowing both sides to process
//! messages speculatively and confirm them after the fact.
//!
//! # What this crate provides
//!
//! - [`outgoing_message::OutgoingMessage`] — the message type; call
//!   [`outgoing_message::OutgoingMessage::hash_leaf`] to produce the MMR leaf hash.
//! - [`mmr::SpecMerge`] — a `mmr_lib::Merge` adapter that plugs domain-tagged hashing into the MMR
//!   library. Callers construct an accumulator directly with `mmr_lib::MMR<Hash,
//!   SpecMerge<SpecHasher>, S>`.
//! - [`mmr::SpecHasher`] — the canonical hasher alias (`BlakeTwo256`); swap in one place to change
//!   the hash function across the entire protocol.
//! - Domain separation tags ([`LEAF_TAG`], [`INNER_TAG`], [`PEAK_TAG`]) and [`LEAF_VERSION`] — used
//!   in leaf and node hashing to prevent cross-context collisions.
//!
//! [`mmr_lib`]: sp_mmr_primitives::mmr_lib

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

/// MMR merge adapter ([`mmr::SpecMerge`]) for use with `mmr_lib::MMR`.
pub mod mmr;
/// Outgoing message type and leaf hashing.
pub mod outgoing_message;

// Domain Tags to ensure that the same message structure used in different
// contexts (e.g. leaf vs inner node) do not collide on the same hash.

/// Tag for a leaf node.
pub const LEAF_TAG: u8 = 0x1;

/// Tag for an inner node.
pub const INNER_TAG: u8 = 0x2;

/// Tag for a peak.
pub const PEAK_TAG: u8 = 0x3;

// Leaf versioning to allow for future changes to the leaf structure without
// breaking compatibility with old messages.

/// Leaf Version.
pub const LEAF_VERSION: u8 = 0x0;
