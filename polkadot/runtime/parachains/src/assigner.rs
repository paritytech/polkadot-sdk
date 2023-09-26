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

//! The Polkadot multiplexing assignment provider.
//! Provides blockspace assignments for both bulk and on demand parachains.

use scale_info::TypeInfo;

use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::{
	codec::{Decode, Encode},
	RuntimeDebug,
};
use sp_std::prelude::*;

use primitives::{CoreIndex, Id as ParaId};

use crate::{
	configuration, paras,
	scheduler::common::{
		Assignment, AssignmentProvider, AssignmentProviderConfig, AssignmentVersion, V0Assignment,
	},
};

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + configuration::Config + paras::Config {
		type ParachainsAssignmentProvider: AssignmentProvider<BlockNumberFor<Self>>;
		type OnDemandAssignmentProvider: AssignmentProvider<BlockNumberFor<Self>>;
	}
}

// Aliases to make the impl more readable.
type ParachainAssigner<T> = <T as Config>::ParachainsAssignmentProvider;
type OnDemandAssigner<T> = <T as Config>::OnDemandAssignmentProvider;

/// Assignments as of this top-level assignment provider.
#[derive(Encode, Decode, PartialEq, TypeInfo, RuntimeDebug)]
enum GenericAssignment<T: Config> {
	/// Assignment came from on-demand assignment provider.
	OnDemand(<OnDemandAssigner<T> as AssignmentProvider<BlockNumberFor<T>>>::AssignmentType),
	// Assignment came from new bulk assignment provider.
	// Bulk(Bulk::BulkAssignmentProvider::AssignmentType),
	/// Assignment came from legacy auction based assignment provider.
	LegacyAuction(<ParachainAssigner<T> as AssignmentProvider<BlockNumberFor<T>>>::AssignmentType),
}

impl<T: Config> Assignment for GenericAssignment<T> {
	fn para_id(&self) -> &ParaId {
		match &self {
			Self::OnDemand(on_demand) => on_demand.para_id(),
			Self::Bulk(bulk) => bulk.para_id(),
			Self::LegacyAuction(legacy) => legacy.para_id(),
		}
	}
}

impl<T: Config> Pallet<T> {
	// Helper fn for the AssignmentProvider implementation.
	// Assumes that the first allocation of cores is to bulk parachains.
	// This function will return false if there are no cores assigned to the bulk parachain
	// assigner.
	fn is_legacy_core(core_idx: &CoreIndex) -> bool {
		let parachain_cores =
			<ParachainAssigner<T> as AssignmentProvider<BlockNumberFor<T>>>::session_core_count();
		(0..parachain_cores).contains(&core_idx.0)
	}

	const SANITY_CHECKED: () = Self::version_sanity_check();

	const fn version_sanity_check() {
		assert!(
			Self::ASSIGNMENT_STORAGE_VERSION >=
				<OnDemandAssigner<T> as AssignmentProvider<BlockNumberFor<T>>>::ASSIGNMENT_STORAGE_VERSION,
"Top-level assigner should always have at least as high a version as on-demand assigner - forgot bump?"
		);
		assert!(
			Self::ASSIGNMENT_STORAGE_VERSION >=
				<ParachainAssigner<T> as AssignmentProvider<BlockNumberFor<T>>>::ASSIGNMENT_STORAGE_VERSION,
"Top-level assigner should always have at least as high a version as the legacy assigner - forgot bump?"
		);
	}
}

impl<T: Config> AssignmentProvider<BlockNumberFor<T>> for Pallet<T> {
	type AssignmentType = GenericAssignment<T>;

	type OldAssignmentType = V0Assignment;

	const ASSIGNMENT_STORAGE_VERSION: AssignmentVersion = AssignmentVersion::new(1u16);

	fn migrate_old_to_current(
		old: Self::OldAssignmentType,
		core: CoreIndex,
	) -> Self::AssignmentType {
		// const _: () = assert!(Self::ASSIGNMENT_STORAGE_VERSION >= <OnDemandAssigner<T> as
		// AssignmentProvider<BlockNumberFor<T>>>::ASSIGNMENT_STORAGE_VERSION, "");

		if Self::is_legacy_core(&core) {
			GenericAssignment::LegacyAuction(<ParachainAssigner<T> as AssignmentProvider<
				BlockNumberFor<T>,
			>>::migrate_old_to_current(old, core))
		} else {
			GenericAssignment::OnDemand(<OnDemandAssigner<T> as AssignmentProvider<
				BlockNumberFor<T>,
			>>::migrate_old_to_current(old, core))
		}
	}

	fn session_core_count() -> u32 {
		let parachain_cores =
			<ParachainAssigner<T> as AssignmentProvider<BlockNumberFor<T>>>::session_core_count();
		let on_demand_cores =
			<OnDemandAssigner<T> as AssignmentProvider<BlockNumberFor<T>>>::session_core_count();

		parachain_cores.saturating_add(on_demand_cores)
	}

	/// Pops an `Assignment` from a specified `CoreIndex`
	fn pop_assignment_for_core(core_idx: CoreIndex) -> Option<Self::AssignmentType> {
		if Pallet::<T>::is_legacy_core(&core_idx) {
			<ParachainAssigner<T> as AssignmentProvider<BlockNumberFor<T>>>::pop_assignment_for_core(
				core_idx,
			)
		} else {
			<OnDemandAssigner<T> as AssignmentProvider<BlockNumberFor<T>>>::pop_assignment_for_core(
				core_idx,
			)
		}
	}

	fn push_back_assignment(assignment: Self::AssignmentType) {
		match assignment {
			GenericAssignment::OnDemand(assignment) =>
				<ParachainAssigner<T> as AssignmentProvider<BlockNumberFor<T>>>::push_back_assignment(
				assignment,
			),
			GenericAssignment::LegacyAuction(assignment) =>
				<OnDemandAssigner<T> as AssignmentProvider<BlockNumberFor<T>>>::push_back_assignment(
					assignment,
			),
		}
	}

	fn get_provider_config(core_idx: CoreIndex) -> AssignmentProviderConfig<BlockNumberFor<T>> {
		if Pallet::<T>::is_legacy_core(&core_idx) {
			<ParachainAssigner<T> as AssignmentProvider<BlockNumberFor<T>>>::get_provider_config(
				core_idx,
			)
		} else {
			<OnDemandAssigner<T> as AssignmentProvider<BlockNumberFor<T>>>::get_provider_config(
				core_idx,
			)
		}
	}
}
