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

//! The JAM collator: a parachain node whose backing chain is JAM instead of a
//! relay chain.
//!
//! Two tasks form the collator loop:
//!
//! - the [builder task](builder) authors on a wall-clock parachain-slot timer, anchoring at the JAM
//!   tip it caches, building on the deepest block the local database holds below the head JAM has
//!   accumulated rather than waiting for inclusion — or on a sibling of that head once it has stood
//!   still long enough to say the branch above it is lost — with the shared authoring primitives
//!   and no parachain inherent, one block per core that holds the para's authorizer, and feeds the
//!   channel. A block's timestamp is its anchor's and its Aura slot is the anchor's Aura slot, so
//!   the collator cannot pick its own time; the authorizer that signs a package is the one JAM uses
//!   at the package's anchor slot, and a turn stops at a block that changes the Aura set;
//! - the [collation task](collation) turns each block into a work package — naming the work package
//!   this node submitted for the parent block as its one prerequisite when the ledger still
//!   remembers it — submits it, follows `workPackageStatus` for every package in flight, and
//!   re-decides every package at each JAM block behind a pluggable [policy](collation::policy);
//!
//! The collation task writes the [authoring hold](AuthoringHold) and the builder reads it each
//! tick: while it is non-zero the builder stands down, because a new block would only deepen a
//! branch whose package has not landed yet.
//!
//! # The PoV store
//!
//! A work package carries its author's randomised signature, so it cannot be recomputed, not even
//! by its author: a block whose package was lost can only travel again in a new package. That
//! takes the block's PoV, and so the storage proof the block executes with. The
//! [PoV store](store::pov_store::BlockPovs) keeps it per block: the import path records it for
//! every block it re-executes, the collation task for every block this collator authored.
//!
//! Heads following reuses `cumulus_client_consensus_common::run_parachain_consensus` with
//! streams built from the parachain service's per-para state entry: `ParaInfo.head_data` holds
//! the head's header hash, which is resolved to the header from the local database.

pub(crate) mod authorizer;
pub(crate) mod builder;
pub(crate) mod chain;
pub(crate) mod code;
pub(crate) mod collation;
pub(crate) mod dev;
pub(crate) mod import;
pub(crate) mod message;
pub(crate) mod package;
pub(crate) mod service;
pub(crate) mod store;
#[cfg(test)]
mod tests;

pub(crate) use authorizer::{AuthoritySource, Authorizers, RuntimeApiAuthorities};
pub(crate) use chain::{
	para_head::para_header_stream,
	slots::{jam_slot_at, JAM_SLOT_DURATION_MS},
};
pub(crate) use code::ensure_code_blob_at;
pub(crate) use message::AuthoringHold;
pub(crate) use store::{hash_ledger, pov_store};

pub(crate) const LOG_TARGET: &str = "jam-collator";
