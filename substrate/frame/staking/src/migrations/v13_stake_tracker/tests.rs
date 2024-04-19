// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

use crate::{
	migrations::v13_stake_tracker::weights::{SubstrateWeight, WeightInfo as _},
	mock::{
		clear_target_list, run_to_block, AllPalletsWithSystem, ExtBuilder, MigratorServiceWeight,
		Staking, System, Test as T, TargetBagsList,
	},
	Validators,
};
use frame_support::traits::OnRuntimeUpgrade;
use frame_election_provider_support::SortedListProvider;
use pallet_migrations::WeightInfo as _;

#[test]
fn mb_migration_target_list_simple_works() {
	ExtBuilder::default().build_and_execute(|| {
		// simulates an empty target list which is the case before the migrations.
		clear_target_list();
		// try state fails since the target list count != number of validators.
		assert!(Staking::do_try_state(System::block_number()).is_err());

		// Give it enough weight to do 2 target migrations per block.
		let limit = <T as pallet_migrations::Config>::WeightInfo::progress_mbms_none() +
			pallet_migrations::Pallet::<T>::exec_migration_max_weight() +
			SubstrateWeight::<T>::step() * 2;
		MigratorServiceWeight::set(&limit);

		// start stepped migrations.
		System::set_block_number(1);
		AllPalletsWithSystem::on_runtime_upgrade(); // onboard MBMs

		// 1 step, should migrate 2 targets.
		run_to_block(2);
        assert_eq!(TargetBagsList::iter().count(), 2);
		// migration not completed yet, the one target missing.
		assert!(Staking::do_try_state(System::block_number()).is_err());

        // next step completes migration.
		run_to_block(3);
        assert_eq!(TargetBagsList::iter().count() as u32, Validators::<T>::count());

		// migration done, try state checks pass.
		assert!(Staking::do_try_state(System::block_number()).is_ok());
	})
}
