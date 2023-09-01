// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

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
	pub trait AuraUnincludedSegmentApi {
		/// Whether it is legal to extend the chain, assuming the given block is the most
		/// recently included one as-of the relay parent that will be built against, and
		/// the given slot.
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
