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

//! On demand assigner pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use super::{Pallet, *};
use crate::{
	configuration::{HostConfiguration, Pallet as ConfigurationPallet},
	paras::{Pallet as ParasPallet, ParaGenesisArgs, ParaKind, ParachainsCache},
	shared::Pallet as ParasShared,
};

use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use sp_runtime::traits::Bounded;

use primitives::{
	HeadData, Id as ParaId, SessionIndex, ValidationCode, ON_DEMAND_DEFAULT_QUEUE_MAX_SIZE,
};

// Constants for the benchmarking
const SESSION_INDEX: SessionIndex = 1;

// Initialize a parathread for benchmarking.
pub fn init_parathread<T>(para_id: ParaId)
where
	T: Config + crate::paras::Config + crate::shared::Config,
{
	ParasShared::<T>::set_session_index(SESSION_INDEX);
	let mut config = HostConfiguration::default();
	config.on_demand_cores = 1;
	ConfigurationPallet::<T>::force_set_active_config(config);
	let mut parachains = ParachainsCache::new();
	ParasPallet::<T>::initialize_para_now(
		&mut parachains,
		para_id,
		&ParaGenesisArgs {
			para_kind: ParaKind::Parathread,
			genesis_head: HeadData(vec![1, 2, 3, 4]),
			validation_code: ValidationCode(vec![1, 2, 3, 4]),
		},
	);
}

#[benchmarks]
mod benchmarks {
	use super::*;
	#[benchmark]
	fn assign_core() {
		// Setup
		let caller: T::AccountId = whitelisted_caller(); // TODO: Make this the broker parachain origin

		// Use valid assignment set with maximum number of assignments to maximize work
		let mut assignments: Vec<(CoreAssignment, PartsOf57600)> = vec![576u16; 100]
			.into_iter()
			.enumerate()
			.map(|(index, parts)| (CoreAssignment::Task(index as u32), parts))
			.collect();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.into()), CoreIndex(0), BlockNumberFor::<T>::from(5u32), assignments, Some(BlockNumberFor::<T>::from(20u32)))
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(
			crate::assigner_bulk::mock_helpers::GenesisConfigBuilder::default().build()
		),
		crate::mock::Test
	);
}
