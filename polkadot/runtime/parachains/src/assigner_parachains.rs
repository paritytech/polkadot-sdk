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

//! The bulk (parachain slot auction) blockspace assignment provider.
//! This provider is tightly coupled with the configuration and paras modules.

#[cfg(test)]
mod mock_helpers;
#[cfg(test)]
mod tests;

use frame_system::pallet_prelude::BlockNumberFor;
use primitives::CoreIndex;

use crate::{
	configuration, paras,
	scheduler::common::{Assignment, AssignmentProvider},
};

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + configuration::Config + paras::Config {}
}

impl<T: Config> AssignmentProvider<BlockNumberFor<T>> for Pallet<T> {
	fn pop_assignment_for_core(core_idx: CoreIndex) -> Option<Assignment> {
		<paras::Pallet<T>>::parachains()
			.get(core_idx.0 as usize)
			.copied()
			.map(Assignment::Bulk)
	}

	fn report_processed(_: Assignment) {}

	/// Bulk assignment has no need to push the assignment back on a session change,
	/// this is a no-op in the case of a bulk assignment slot.
	fn push_back_assignment(_: Assignment) {}

	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn get_mock_assignment(_: CoreIndex, para_id: primitives::Id) -> Assignment {
		Assignment::Bulk(para_id)
	}

	fn session_core_count() -> u32 {
		paras::Parachains::<T>::decode_len().unwrap_or(0) as u32
	}
}
