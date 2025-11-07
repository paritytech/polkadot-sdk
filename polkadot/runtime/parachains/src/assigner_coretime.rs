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

//! # DEPRECATED: AssignerCoretime Pallet (Stub for Migration)
//!
//! **⚠️ THIS PALLET IS DEPRECATED ⚠️**
//!
//! This pallet exists only to facilitate storage migration to the scheduler pallet.
//! The functionality has been moved to `scheduler::assigner_coretime` submodule.
//!
//! **This pallet should be removed after all networks have successfully migrated to v4.**
//!
//! ## Migration Path
//!
//! Storage items `CoreSchedules` and `CoreDescriptors` are migrated from this pallet
//! to the `Scheduler` pallet via the `scheduler::migration::v4::MigrateV3ToV4` migration.

use alloc::vec::Vec;
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
use pallet_broker::CoreAssignment;
use polkadot_primitives::CoreIndex;

/// Fraction expressed as a nominator with an assumed denominator of 57,600.
#[derive(
	RuntimeDebug,
	Clone,
	Copy,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
)]
pub struct PartsOf57600(pub u16);

impl PartsOf57600 {
	pub const ZERO: Self = Self(0);
	pub const FULL: Self = Self(57600);

	pub fn new_saturating(v: u16) -> Self {
		Self::ZERO.saturating_add(Self(v))
	}

	pub fn is_full(&self) -> bool {
		*self == Self::FULL
	}

	pub fn saturating_add(self, rhs: Self) -> Self {
		let inner = self.0.saturating_add(rhs.0);
		if inner > 57600 {
			Self(57600)
		} else {
			Self(inner)
		}
	}

	pub fn saturating_sub(self, rhs: Self) -> Self {
		Self(self.0.saturating_sub(rhs.0))
	}
}

/// Assignments as they are scheduled by block number for a particular core.
#[derive(Encode, Decode, TypeInfo)]
#[cfg_attr(test, derive(PartialEq, RuntimeDebug))]
pub struct Schedule<N> {
	/// Original assignments
	pub assignments: Vec<(CoreAssignment, PartsOf57600)>,
	/// When do our assignments become invalid, if at all?
	pub end_hint: Option<N>,
	/// The next queued schedule for this core.
	pub next_schedule: Option<N>,
}

/// Descriptor for a core.
#[derive(Encode, Decode, TypeInfo, Default)]
#[cfg_attr(test, derive(PartialEq, RuntimeDebug, Clone))]
pub struct CoreDescriptor<N> {
	/// Meta data about the queued schedules for this core.
	pub queue: Option<QueueDescriptor<N>>,
	/// Currently performed work.
	pub current_work: Option<WorkState<N>>,
}

/// Pointers into `CoreSchedules` for a particular core.
#[derive(Encode, Decode, TypeInfo, Copy, Clone)]
#[cfg_attr(test, derive(PartialEq, RuntimeDebug))]
pub struct QueueDescriptor<N> {
	/// First scheduled item, that is not yet active.
	pub first: N,
	/// Last scheduled item.
	pub last: N,
}

/// Work state for a core.
#[derive(Encode, Decode, TypeInfo)]
#[cfg_attr(test, derive(PartialEq, RuntimeDebug, Clone))]
pub struct WorkState<N> {
	/// Assignments with current state.
	pub assignments: Vec<(CoreAssignment, AssignmentState)>,
	/// When do our assignments become invalid if at all?
	pub end_hint: Option<N>,
	/// Position in the assignments we are currently in.
	pub pos: u16,
	/// Step width
	pub step: PartsOf57600,
}

/// Assignment state.
#[derive(Encode, Decode, TypeInfo)]
#[cfg_attr(test, derive(PartialEq, RuntimeDebug, Clone, Copy))]
pub struct AssignmentState {
	/// Ratio of the core this assignment has.
	pub ratio: PartsOf57600,
	/// How many parts are remaining in this round?
	pub remaining: PartsOf57600,
}

#[allow(deprecated)]
#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {}

	/// Old CoreSchedules storage - migrated to Scheduler pallet in v4.
	///
	/// **DEPRECATED:** This storage will be empty after migration completes.
	#[pallet::storage]
	pub type CoreSchedules<T: Config> = StorageMap<
		_,
		Twox256,
		(BlockNumberFor<T>, CoreIndex),
		Schedule<BlockNumberFor<T>>,
		OptionQuery,
	>;

	/// Old CoreDescriptors storage - migrated to Scheduler pallet in v4.
	///
	/// **DEPRECATED:** This storage will be empty after migration completes.
	#[pallet::storage]
	pub type CoreDescriptors<T: Config> =
		StorageMap<_, Twox256, CoreIndex, CoreDescriptor<BlockNumberFor<T>>, ValueQuery>;
}

pub use pallet::*;
