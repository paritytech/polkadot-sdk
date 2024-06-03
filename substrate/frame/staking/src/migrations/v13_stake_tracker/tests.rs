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

#![cfg(all(test, not(feature = "runtime-benchmarks")))]

use crate::{
	mock::{
		bond_nominator, bond_validator, run_to_block, AllPalletsWithSystem, ExtBuilder,
		MigratorServiceWeight, Staking, System, TargetBagsList, Test as T,
	},
	weights::{SubstrateWeight, WeightInfo as _},
	Nominators,
};
use frame_election_provider_support::SortedListProvider;
use frame_support::traits::OnRuntimeUpgrade;
use pallet_migrations::WeightInfo as _;

#[test]
fn mb_migration_target_list_simple_works() {
	ExtBuilder::default().has_stakers(false).build_and_execute(|| {
		bond_validator(1, 10);
		bond_validator(2, 20);
		bond_validator(3, 30);
		bond_nominator(4, 40, vec![1, 2]);
		bond_nominator(5, 50, vec![2, 3]);
		bond_nominator(6, 60, vec![3]);

		TargetBagsList::unsafe_clear();
		assert!(TargetBagsList::count() == 0);
		assert!(Staking::do_try_state(System::block_number()).is_err());

		// allocate 3 steps per block to do the full migration in one step.
		let limit = <T as pallet_migrations::Config>::WeightInfo::progress_mbms_none() +
			pallet_migrations::Pallet::<T>::exec_migration_max_weight() +
			SubstrateWeight::<T>::v13_mmb_step() * 3;
		MigratorServiceWeight::set(&limit);

		// migrate 3 nominators.
		AllPalletsWithSystem::on_runtime_upgrade(); // onboard MBMs
		run_to_block(2);

		// stakes of each validators are correct (sum of self_stake and nominations stake).
		assert_eq!(TargetBagsList::get_score(&1).unwrap(), 10 + 40);
		assert_eq!(TargetBagsList::get_score(&2).unwrap(), 20 + 40 + 50);
		assert_eq!(TargetBagsList::get_score(&3).unwrap(), 30 + 50 + 60);

		assert_eq!(TargetBagsList::count(), 3);

		// migration done, try state checks pass.
		assert!(Staking::do_try_state(System::block_number()).is_ok());
	})
}

#[test]
fn mb_migration_target_list_multiple_steps_works() {
	ExtBuilder::default().has_stakers(false).build_and_execute(|| {
		bond_validator(1, 10);
		bond_validator(2, 20);
		bond_validator(3, 30);
		bond_nominator(4, 40, vec![1, 2]);
		bond_nominator(5, 50, vec![2, 3]);
		bond_nominator(6, 60, vec![3]);

		TargetBagsList::unsafe_clear();
		assert!(TargetBagsList::count() == 0);
		assert!(Staking::do_try_state(System::block_number()).is_err());

		// allocate 1 step (i.e. 1 nominator) per block.
		let limit = <T as pallet_migrations::Config>::WeightInfo::progress_mbms_none() +
			pallet_migrations::Pallet::<T>::exec_migration_max_weight() +
			SubstrateWeight::<T>::v13_mmb_step();
		MigratorServiceWeight::set(&limit);

		AllPalletsWithSystem::on_runtime_upgrade(); // onboard MBMs

		// starts from last bonded nominator (6).
		let mut migrating = Nominators::<T>::iter().map(|(n, _)| n);
		assert_eq!(migrating.next(), Some(6));
		run_to_block(2);

		// 6 nominates 3, thus target list node 3 has self stake + stake of 6.
		assert_eq!(TargetBagsList::get_score(&3).unwrap(), 30 + 60);
		assert_eq!(TargetBagsList::count(), 1);

		// next block, migrates nominator 5.
		assert_eq!(migrating.next(), Some(5));
		run_to_block(3);

		// 5 nominates 2 and 3. stakes are updated as expected.
		assert_eq!(TargetBagsList::get_score(&3).unwrap(), 30 + 60 + 50);
		assert_eq!(TargetBagsList::get_score(&2).unwrap(), 20 + 50);
		assert_eq!(TargetBagsList::count(), 2);

		// last block, migrates nominator 4.
		assert_eq!(migrating.next(), Some(4));
		run_to_block(4);

		// 4 nominates 1 and 2. stakes are updated as expected.
		assert_eq!(TargetBagsList::get_score(&2).unwrap(), 20 + 50 + 40);
		assert_eq!(TargetBagsList::get_score(&1).unwrap(), 10 + 40);
		assert_eq!(TargetBagsList::count(), 3);

		// migration done, try state checks pass.
		assert_eq!(migrating.next(), None);
		assert!(Staking::do_try_state(System::block_number()).is_ok());
	})
}

#[test]
fn mb_migration_target_list_chilled_validator_works() {
	ExtBuilder::default().has_stakers(false).build_and_execute(|| {
		// TODO
	})
}

#[test]
fn mb_migration_target_list_dangling_validators_works() {
	ExtBuilder::default().has_stakers(false).build_and_execute(|| {
		// TODO
	})
}

#[test]
fn mb_migration_target_list_duplicate_validators_works() {
	ExtBuilder::default().has_stakers(false).build_and_execute(|| {
		// TODO
	})
}
