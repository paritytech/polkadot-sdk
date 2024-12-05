// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Common traits and types used by the scheduler and assignment providers.

use frame_support::pallet_prelude::*;
use primitives::{CoreIndex, Id as ParaId};
use scale_info::TypeInfo;
use sp_std::prelude::*;

// Only used to link to configuration documentation.
#[allow(unused)]
use crate::configuration::HostConfiguration;

/// An assignment for a parachain scheduled to be backed and included in a relay chain block.
#[derive(Clone, Encode, Decode, PartialEq, TypeInfo, RuntimeDebug)]
pub struct Assignment {
	/// Assignment's ParaId
	pub para_id: ParaId,
}

impl Assignment {
	/// Create a new `Assignment`.
	pub fn new(para_id: ParaId) -> Self {
		Self { para_id }
	}
}

/// A set of variables required by the scheduler in order to operate.
pub struct AssignmentProviderConfig<BlockNumber> {
	/// How many times a collation can time out on availability.
	/// Zero timeouts still means that a collation can be provided as per the slot auction
	/// assignment provider.
	pub max_availability_timeouts: u32,

	/// How long the collator has to provide a collation to the backing group before being dropped.
	pub ttl: BlockNumber,
}

pub trait AssignmentProvider<BlockNumber> {
	/// How many cores are allocated to this provider.
	fn session_core_count() -> u32;

	/// Pops an [`Assignment`] from the provider for a specified [`CoreIndex`].
	/// The `concluded_para` field makes the caller report back to the provider
	/// which [`ParaId`] it processed last on the supplied [`CoreIndex`].
	fn pop_assignment_for_core(
		core_idx: CoreIndex,
		concluded_para: Option<ParaId>,
	) -> Option<Assignment>;

	/// Push back an already popped assignment. Intended for provider implementations
	/// that need to be able to keep track of assignments over session boundaries,
	/// such as the on demand assignment provider.
	fn push_assignment_for_core(core_idx: CoreIndex, assignment: Assignment);

	/// Returns a set of variables needed by the scheduler
	fn get_provider_config(core_idx: CoreIndex) -> AssignmentProviderConfig<BlockNumber>;
}
