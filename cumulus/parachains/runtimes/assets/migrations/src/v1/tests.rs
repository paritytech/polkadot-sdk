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

use codec::Encode;
use frame_support::{
	construct_runtime, derive_impl, migrations::SteppedMigration, traits::AsEnsureOriginWithArg,
	weights::WeightMeter, Hashable,
};
use frame_system::{EnsureRoot, EnsureSigned};
use hex_display::HexDisplayExt;
use pallet_assets::{Asset, AssetDetails, AssetStatus};
use sp_io::{hashing, storage, TestExternalities};
use sp_runtime::BuildStorage;
use xcm::{v3, v4};

use super::{mock_asset_details, old, Migration};

construct_runtime! {
	pub struct Runtime {
		System: frame_system,
		Balances: pallet_balances,
		ForeignAssets: pallet_assets,
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
		let key = v3::Location::new(1, [v3::Junction::Parachain(2004)]);
		let mock_asset_details = mock_asset_details();
		old::Asset::<Runtime, ()>::insert(key.clone(), mock_asset_details);

		// Perform one step of the migration.
		let cursor = Migration::<Runtime>::step(None, &mut WeightMeter::new()).unwrap().unwrap();
		// Second time works.
		assert!(Migration::<Runtime>::step(Some(cursor), &mut WeightMeter::new())
			.unwrap()
			.is_none());

		let new_key = v4::Location::new(1, [v4::Junction::Parachain(2004)]);
		assert!(Asset::<Runtime>::contains_key(new_key));
	})
}
