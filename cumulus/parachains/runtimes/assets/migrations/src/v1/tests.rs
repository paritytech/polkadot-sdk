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

//! Tests for the foreign assets migration.

use frame_support::{
	construct_runtime, derive_impl,
	migrations::SteppedMigration,
	parameter_types,
	traits::{AsEnsureOriginWithArg, OnRuntimeUpgrade},
	weights::{Weight, WeightMeter},
};
use frame_system::{EnsureRoot, EnsureSigned};
use pallet_assets::{Asset, AssetDetails, AssetStatus};
use sp_io::TestExternalities;
use sp_runtime::BuildStorage;
use xcm::{v3, v4};

use super::{old, weights, Migration};

construct_runtime! {
	pub struct Runtime {
		System: frame_system,
		Balances: pallet_balances,
		ForeignAssets: pallet_assets,
		Migrations: pallet_migrations,
	}
}

type Block = frame_system::mocking::MockBlock<Runtime>;
type AccountId = u64;
type Balance = u64;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
	type AccountId = AccountId;
	type AccountData = pallet_balances::AccountData<Balance>;
	type MultiBlockMigrator = Migrations;
}

parameter_types! {
	pub storage MigratorServiceWeight: Weight = Weight::from_parts(100, 100); // do not use in prod
}

#[derive_impl(pallet_migrations::config_preludes::TestDefaultConfig)]
impl pallet_migrations::Config for Runtime {
	#[cfg(not(feature = "runtime-benchmarks"))]
	type Migrations =
		(crate::v1::Migration<Runtime, (), crate::v1::weights::SubstrateWeight<Runtime>>,);
	#[cfg(feature = "runtime-benchmarks")]
	type Migrations = pallet_migrations::mock_helpers::MockedMigrations;
	type MaxServiceWeight = MigratorServiceWeight;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type AccountStore = System;
	type Balance = Balance;
}

#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig)]
impl pallet_assets::Config for Runtime {
	type AssetId = v4::Location;
	type AssetIdParameter = v4::Location;
	type Balance = Balance;
	type Currency = Balances;
	type CreateOrigin = AsEnsureOriginWithArg<EnsureSigned<AccountId>>;
	type ForceOrigin = EnsureRoot<AccountId>;
	type Freezer = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = XcmBenchmarkHelper;
}

/// Simple conversion of `u32` into an `AssetId` for use in benchmarking.
#[cfg(feature = "runtime-benchmarks")]
pub struct XcmBenchmarkHelper;
#[cfg(feature = "runtime-benchmarks")]
impl pallet_assets::BenchmarkHelper<v4::Location> for XcmBenchmarkHelper {
	fn create_asset_id_parameter(id: u32) -> v4::Location {
		v4::Location::new(1, [v4::Junction::Parachain(id)])
	}
}

pub(crate) fn new_test_ext() -> TestExternalities {
	let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

	let test_account = 1;
	let initial_balance = 1000;
	pallet_balances::GenesisConfig::<Runtime> { balances: vec![(test_account, initial_balance)] }
		.assimilate_storage(&mut t)
		.unwrap();

	t.into()
}

#[test]
fn migration_works() {
	new_test_ext().execute_with(|| {
		let mock_asset_details = mock_asset_details();

		// Insert a bunch of items in the old map.
		for i in 0..1024 {
			let key = v3::Location::new(1, [v3::Junction::Parachain(2004), v3::Junction::PalletInstance(50), v3::Junction::GeneralIndex(i)]);
			old::Asset::<Runtime, ()>::insert(key, mock_asset_details.clone());
		}

		// Give the migration some limit.
		let limit = <<Runtime as pallet_migrations::Config>::WeightInfo as pallet_migrations::WeightInfo>::progress_mbms_none() +
			pallet_migrations::Pallet::<Runtime>::exec_migration_max_weight() +
			<weights::SubstrateWeight::<Runtime> as weights::WeightInfo>::conversion_step() * 16;
		MigratorServiceWeight::set(&limit);

		System::set_block_number(1);
		AllPalletsWithSystem::on_runtime_upgrade(); // onboard MBMs

		// Perform one step of the migration.
		assert!(Migration::<Runtime, (), ()>::step(None, &mut WeightMeter::new()).unwrap().is_none());

		for i in 0..1024 {
			let new_key = v4::Location::new(1, [v4::Junction::Parachain(2004), v4::Junction::PalletInstance(50), v4::Junction::GeneralIndex(i)]);
			assert_eq!(Asset::<Runtime>::get(new_key), Some(mock_asset_details.clone()));
		}
	})
}

fn mock_asset_details() -> AssetDetails<Balance, AccountId, Balance> {
	AssetDetails {
		owner: 0,
		issuer: 0,
		admin: 0,
		freezer: 0,
		supply: Default::default(),
		deposit: Default::default(),
		min_balance: 1u32.into(),
		is_sufficient: false,
		accounts: Default::default(),
		sufficients: Default::default(),
		approvals: Default::default(),
		status: AssetStatus::Live,
	}
}
