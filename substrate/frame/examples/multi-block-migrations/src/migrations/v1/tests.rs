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
// limitations under the License.

#![cfg(all(test, not(feature = "runtime-benchmarks")))]

use crate::{
	migrations::{
		v1,
		v1::{weights, weights::WeightInfo as _},
	},
	mock::{
		new_test_ext, run_to_block, AllPalletsWithSystem, MigratorServiceWeight, Runtime as T,
		System,
	},
};
use frame_support::traits::OnRuntimeUpgrade;
use pallet_migrations::WeightInfo as _;

#[test]
fn lazy_migration_works() {
	new_test_ext().execute_with(|| {
		frame_support::__private::sp_tracing::try_init_simple();
		// Insert some values into the old storage map.
		for i in 0..1024 {
			v1::v0::MyMap::<T>::insert(i, i);
		}

		// Give it enough weight do do exactly 16 iterations:
		let limit = <T as pallet_migrations::Config>::WeightInfo::progress_mbms_none() +
			pallet_migrations::Pallet::<T>::exec_migration_max_weight() +
			weights::SubstrateWeight::<T>::step() * 16;
		MigratorServiceWeight::set(&limit);

		System::set_block_number(1);
		AllPalletsWithSystem::on_runtime_upgrade(); // onboard MBMs

		let mut last_decodable = 0;
		for block in 2..=65 {
			run_to_block(block);
			let mut decodable = 0;
			for i in 0..1024 {
				if crate::MyMap::<T>::get(i).is_some() {
					decodable += 1;
				}
			}

			assert_eq!(decodable, last_decodable + 16);
			last_decodable = decodable;
		}

		// Check that everything is decodable now:
		for i in 0..1024 {
			assert_eq!(crate::MyMap::<T>::get(i), Some(i as u64));
		}
	});
}
