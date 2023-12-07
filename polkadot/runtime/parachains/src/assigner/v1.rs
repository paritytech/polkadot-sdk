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
//! Provides blockspace assignments for both coretime (including on-demand) and legacy auction
//! assignments.

use scale_info::TypeInfo;

use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::{
	codec::{Decode, Encode},
	RuntimeDebug,
};

use primitives::{CoreIndex, Id as ParaId};

use crate::{
	assigner_coretime, assigner_parachains as assigner_legacy, configuration, paras,
	scheduler::common::{
		Assignment, AssignmentProvider, AssignmentProviderConfig, FixedAssignmentProvider,
		V0Assignment,
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
		+ assigner_coretime::Config
		+ assigner_legacy::Config
	{
	}
}

/// Assignments as of this top-level assignment provider.
#[derive(Encode, Decode, TypeInfo, RuntimeDebug, PartialEq, Clone)]
pub enum UnifiedAssignment<Coretime, Legacy> {
	/// Assignment came from new Coretime assignment provider.
	#[codec(index = 0)]
	Coretime(Coretime),
	// Assignment came from new Coretime assignment provider.
	// Coretime(Coretime::CoretimeAssignmentProvider::AssignmentType),
	/// Assignment came from legacy auction based assignment provider.
	#[codec(index = 99)]
	LegacyAuction(Legacy),
}

/// Convenience type definition for `UnifiedAssignmentType`.
pub type UnifiedAssignmentType<T> = UnifiedAssignment<
	<assigner_coretime::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::AssignmentType,
	<assigner_legacy::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::AssignmentType,
>;

impl<OnDemand: Assignment, Legacy: Assignment> Assignment for UnifiedAssignment<OnDemand, Legacy> {
	fn para_id(&self) -> ParaId {
		match &self {
			Self::Coretime(coretime) => coretime.para_id(),
			Self::LegacyAuction(legacy) => legacy.para_id(),
		}
	}
}

impl<T: Config> AssignmentProvider<BlockNumberFor<T>> for Pallet<T> {
	type AssignmentType = UnifiedAssignmentType<T>;

	/// Pops an `Assignment` from a specified `CoreIndex`
	fn pop_assignment_for_core(core_idx: CoreIndex) -> Option<Self::AssignmentType> {
		let legacy_cores = <assigner_legacy::Pallet<T> as FixedAssignmentProvider<
			BlockNumberFor<T>,
		>>::session_core_count();

		if core_idx.0 < legacy_cores {
			<assigner_legacy::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::pop_assignment_for_core(
				core_idx,
			).map(UnifiedAssignment::LegacyAuction)
		} else {
			let core_idx = CoreIndex(core_idx.0 - legacy_cores);

			<assigner_coretime::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::pop_assignment_for_core(
				core_idx,
			)
			.map(UnifiedAssignment::Coretime)
		}
	}

	fn report_processed(assignment: Self::AssignmentType) {
		match assignment {
			UnifiedAssignment::LegacyAuction(assignment) =>
				<assigner_legacy::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::report_processed(
					assignment,
				),
			UnifiedAssignment::Coretime(assignment) =>
				<assigner_coretime::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::report_processed(
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
			UnifiedAssignment::Coretime(assignment) =>
				<assigner_coretime::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::push_back_assignment(
				assignment,
			),
		}
	}

	fn get_provider_config(core_idx: CoreIndex) -> AssignmentProviderConfig<BlockNumberFor<T>> {
		let legacy_cores = <assigner_legacy::Pallet<T> as FixedAssignmentProvider<
			BlockNumberFor<T>,
		>>::session_core_count();

		if core_idx.0 < legacy_cores {
			<assigner_legacy::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::get_provider_config(
				core_idx,
			)
		} else {
			let core_idx = CoreIndex(core_idx.0 - legacy_cores);

			<assigner_coretime::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::get_provider_config(
				core_idx,
			)
		}
	}
	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn get_mock_assignment(core_idx: CoreIndex, para_id: ParaId) -> Self::AssignmentType {
		let legacy_cores = <assigner_legacy::Pallet<T> as FixedAssignmentProvider<
			BlockNumberFor<T>,
		>>::session_core_count();

		if core_idx.0 < legacy_cores {
			UnifiedAssignment::LegacyAuction(<assigner_legacy::Pallet<T> as AssignmentProvider<
				BlockNumberFor<T>,
			>>::get_mock_assignment(core_idx, para_id))
		} else {
			let core_idx = CoreIndex(core_idx.0 - legacy_cores);

			UnifiedAssignment::Coretime(<assigner_coretime::Pallet<T> as AssignmentProvider<
				BlockNumberFor<T>,
			>>::get_mock_assignment(core_idx, para_id))
		}
	}
}

impl<T: Config> FixedAssignmentProvider<BlockNumberFor<T>> for Pallet<T> {
	fn session_core_count() -> u32 {
		let legacy_cores = <assigner_legacy::Pallet<T> as FixedAssignmentProvider<
			BlockNumberFor<T>,
		>>::session_core_count();
		let coretime_cores = <assigner_coretime::Pallet<T> as FixedAssignmentProvider<
			BlockNumberFor<T>,
		>>::session_core_count();

		legacy_cores.saturating_add(coretime_cores)
	}
}

pub fn migrate_assignment_v0_to_v1<T: Config>(
	old: V0Assignment,
	core: CoreIndex,
) -> UnifiedAssignmentType<T> {
	let legacy_cores = <assigner_legacy::Pallet<T> as FixedAssignmentProvider<
		BlockNumberFor<T>,
	>>::session_core_count();

	if core.0 < legacy_cores {
		UnifiedAssignment::LegacyAuction(assigner_legacy::ParachainsAssignment::from_v0_assignment(
			old,
		))
	} else {
		// We are not subtracting `legacy_cores` from `core` here, as this was not done before for
		// on-demand. Therefore we keep it as is, so the book keeping will affect the correct core
		// in the underlying on-demand assignment provider.
		UnifiedAssignment::Coretime(assigner_coretime::CoretimeAssignment::from_v0_assignment(
			old, core,
		))
	}
}
