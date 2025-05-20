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

//! Core primitives for Aura in Cumulus.
//!
//! In particular, this exposes the [`AuraUnincludedSegmentApi`] which is used to regulate
//! the behavior of Aura within a parachain context.

#![cfg_attr(not(feature = "std"), no_std)]

pub use sp_consensus_aura::Slot;

sp_api::decl_runtime_apis! {
	/// This runtime API is used to inform potential block authors whether they will
	/// have the right to author at a slot, assuming they have claimed the slot.
	///
	/// In particular, this API allows Aura-based parachains to regulate their "unincluded segment",
	/// which is the section of the head of the chain which has not yet been made available in the
	/// relay chain.
	///
	/// When the unincluded segment is short, Aura chains will allow authors to create multiple
	/// blocks per slot in order to build a backlog. When it is saturated, this API will limit
	/// the amount of blocks that can be created.
	///
	/// Changes:
	/// - Version 2: Update to `can_build_upon` to take a relay chain `Slot` instead of a parachain `Slot`.
	#[api_version(2)]
	pub trait AuraUnincludedSegmentApi {
		/// Whether it is legal to extend the chain, assuming the given block is the most
		/// recently included one as-of the relay parent that will be built against, and
		/// the given relay chain slot.
		///
		/// This should be consistent with the logic the runtime uses when validating blocks to
		/// avoid issues.
		///
		/// When the unincluded segment is empty, i.e. `included_hash == at`, where at is the block
		/// whose state we are querying against, this must always return `true` as long as the slot
		/// is more recent than the included block itself.
		fn can_build_upon(included_hash: Block::Hash, slot: Slot) -> bool;
	}
}
