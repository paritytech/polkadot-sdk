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

mod mock_helpers;
#[cfg(test)]
mod tests;

use scale_info::TypeInfo;

use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::{
	codec::{Decode, Encode},
	RuntimeDebug,
};

use primitives::{CoreIndex, Id as ParaId};

use crate::{
	assigner_on_demand, assigner_parachains as assigner_legacy, configuration, paras,
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
	pub trait Config:
		frame_system::Config
		+ configuration::Config
		+ paras::Config
		+ assigner_on_demand::Config
		+ assigner_legacy::Config
	{
	}
}

/// Assignments as of this top-level assignment provider.
#[derive(Encode, Decode, TypeInfo, RuntimeDebug, PartialEq, Clone)]
pub enum UnifiedAssignment<OnDemand, Legacy> {
	/// Assignment came from on-demand assignment provider.
	#[codec(index = 0)]
	OnDemand(OnDemand),
	// Assignment came from new bulk assignment provider.
	// Bulk(Bulk::BulkAssignmentProvider::AssignmentType),
	/// Assignment came from legacy auction based assignment provider.
	#[codec(index = 99)]
	LegacyAuction(Legacy),
}

/// Convenience type definition for `UnifiedAssignmentType`.
pub type UnifiedAssignmentType<T> = UnifiedAssignment<
	<assigner_on_demand::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::AssignmentType,
	<assigner_legacy::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::AssignmentType,
>;

impl<OnDemand: Assignment, Legacy: Assignment> Assignment for UnifiedAssignment<OnDemand, Legacy> {
	fn para_id(&self) -> ParaId {
		match &self {
			Self::OnDemand(on_demand) => on_demand.para_id(),
			// Self::Bulk(bulk) => bulk.para_id(),
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
		let parachain_cores = <assigner_legacy::Pallet<T> as AssignmentProvider<
			BlockNumberFor<T>,
		>>::session_core_count();
		(0..parachain_cores).contains(&core_idx.0)
	}
}

impl<T: Config> AssignmentProvider<BlockNumberFor<T>> for Pallet<T> {
	type AssignmentType = UnifiedAssignmentType<T>;

	type OldAssignmentType = V0Assignment;

	// Sum of underlying versions ensures this version will always get increased on changes.
	const ASSIGNMENT_STORAGE_VERSION: AssignmentVersion =
		<assigner_legacy::Pallet<T>>::ASSIGNMENT_STORAGE_VERSION
			.saturating_add(<assigner_on_demand::Pallet<T>>::ASSIGNMENT_STORAGE_VERSION);

	fn migrate_old_to_current(
		old: Self::OldAssignmentType,
		core: CoreIndex,
	) -> Self::AssignmentType {
		if Self::is_legacy_core(&core) {
			UnifiedAssignment::LegacyAuction(<assigner_legacy::Pallet<T> as AssignmentProvider<
				BlockNumberFor<T>,
			>>::migrate_old_to_current(old, core))
		} else {
			UnifiedAssignment::OnDemand(<assigner_on_demand::Pallet<T> as AssignmentProvider<
				BlockNumberFor<T>,
			>>::migrate_old_to_current(old, core))
		}
	}

	fn session_core_count() -> u32 {
		let parachain_cores = <assigner_legacy::Pallet<T> as AssignmentProvider<
			BlockNumberFor<T>,
		>>::session_core_count();
		let on_demand_cores = <assigner_on_demand::Pallet<T> as AssignmentProvider<
			BlockNumberFor<T>,
		>>::session_core_count();

		parachain_cores.saturating_add(on_demand_cores)
	}

	/// Pops an `Assignment` from a specified `CoreIndex`
	fn pop_assignment_for_core(core_idx: CoreIndex) -> Option<Self::AssignmentType> {
		if Pallet::<T>::is_legacy_core(&core_idx) {
			<assigner_legacy::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::pop_assignment_for_core(
				core_idx,
			).map(UnifiedAssignment::LegacyAuction)
		} else {
			<assigner_on_demand::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::pop_assignment_for_core(
				core_idx,
			)
			.map(UnifiedAssignment::OnDemand)
		}
	}

	fn report_processed(assignment: Self::AssignmentType) {
		match assignment {
			UnifiedAssignment::LegacyAuction(assignment) =>
				<assigner_legacy::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::report_processed(
					assignment,
				),
			UnifiedAssignment::OnDemand(assignment) =>
				<assigner_on_demand::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::report_processed(
					assignment,
				),
		}
	}

	fn push_back_assignment(assignment: Self::AssignmentType) {
		match assignment {
			UnifiedAssignment::LegacyAuction(assignment) =>
				<assigner_legacy::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::push_back_assignment(
					assignment,
			),
			UnifiedAssignment::OnDemand(assignment) =>
				<assigner_on_demand::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::push_back_assignment(
				assignment,
			),
		}
	}

	fn get_provider_config(core_idx: CoreIndex) -> AssignmentProviderConfig<BlockNumberFor<T>> {
		if Pallet::<T>::is_legacy_core(&core_idx) {
			<assigner_legacy::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::get_provider_config(
				core_idx,
			)
		} else {
			<assigner_on_demand::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::get_provider_config(
				core_idx,
			)
		}
	}
}
