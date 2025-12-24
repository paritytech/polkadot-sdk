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

//! Migration tests for the DAP pallet.

use crate::{migrations, mock::*};
use frame_support::traits::{GetStorageVersion, OnRuntimeUpgrade, StorageVersion};
use sp_runtime::BuildStorage;

type DapPallet = crate::Pallet<Test>;

#[test]
fn check_migration_v0_1() {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> { balances: vec![(1, 100)], ..Default::default() }
		.assimilate_storage(&mut t)
		.unwrap();

	sp_io::TestExternalities::from(t).execute_with(|| {
		let buffer = DapPallet::buffer_account();

		// Given: on-chain storage version is 0, buffer account doesn't exist
		assert_eq!(DapPallet::on_chain_storage_version(), StorageVersion::new(0));
		assert!(!System::account_exists(&buffer));

		// When: run the versioned migration
		let _ = migrations::v1::InitBufferAccount::<Test>::on_runtime_upgrade();

		// Then: version updated to 1, buffer account created
		assert_eq!(DapPallet::on_chain_storage_version(), StorageVersion::new(1));
		assert!(System::account_exists(&buffer));
	});
}
