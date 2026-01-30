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

//! Tests mock for `pallet-assets-precompiles`.

pub use super::*;
use frame_support::{derive_impl, parameter_types, traits::AsEnsureOriginWithArg};
use sp_runtime::BuildStorage;
use xcm::v5::{Junction, Location};

#[frame_support::runtime]
mod runtime {
	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeTask,
		RuntimeHoldReason,
		RuntimeFreezeReason
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system;
	#[runtime::pallet_index(10)]
	pub type Balances = pallet_balances;
	#[runtime::pallet_index(20)]
	pub type Assets = pallet_assets;
	#[runtime::pallet_index(21)]
	pub type Revive = pallet_revive;
	#[runtime::pallet_index(22)]
	pub type ForeignAssets = super::foreign_assets;
}

type Block = frame_system::mocking::MockBlock<Test>;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u64>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig as pallet_balances::DefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

parameter_types! {
	pub const AssetDeposit: u64 = 1;
	pub const AssetAccountDeposit: u64 = 1;
	pub const ApprovalDeposit: u64 = 1;
	pub const MetadataDepositBase: u64 = 1;
	pub const MetadataDepositPerByte: u64 = 1;
}

/// A benchmark helper that creates Location instances from u32 IDs.
#[cfg(feature = "runtime-benchmarks")]
pub struct LocationBenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_assets::BenchmarkHelper<Location, ()> for LocationBenchmarkHelper {
	fn create_asset_id_parameter(id: u32) -> Location {
		Location::new(1, [Junction::Parachain(id)])
	}
	fn create_reserve_id_parameter(_id: u32) -> () {
		()
	}
}

impl pallet_assets::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Balance = u64;
	type AssetId = Location;
	type AssetIdParameter = Location;
	type CreateOrigin = AsEnsureOriginWithArg<frame_system::EnsureSigned<u64>>;
	type ForceOrigin = frame_system::EnsureRoot<u64>;
	type Currency = Balances;
	type AssetDeposit = AssetDeposit;
	type AssetAccountDeposit = AssetAccountDeposit;
	type MetadataDepositBase = MetadataDepositBase;
	type MetadataDepositPerByte = MetadataDepositPerByte;
	type ApprovalDeposit = ApprovalDeposit;
	type StringLimit = frame_support::traits::ConstU32<50>;
	type Freezer = ();
	type Extra = ();
	type WeightInfo = ();
	type RemoveItemsLimit = frame_support::traits::ConstU32<1000>;
	type CallbackHandle = ();
	type ReserveData = ();
	type Holder = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = LocationBenchmarkHelper;
}

impl foreign_assets::pallet::Config for Test {}

#[derive_impl(pallet_revive::config_preludes::TestDefaultConfig)]
impl pallet_revive::Config for Test {
	type AddressMapper = pallet_revive::TestAccountMapper<Self>;
	type Balance = u64;
	type Currency = Balances;
	type Precompiles = (ERC20<Self, ForeignIdConfig<0x0220, Self>>,);
}

/// Helper to create a test Location from a parachain ID.
pub fn test_location(para_id: u32) -> Location {
	Location::new(1, [Junction::Parachain(para_id)])
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let asset_location = test_location(1000);
	let t = RuntimeGenesisConfig {
		assets: pallet_assets::GenesisConfig {
			assets: vec![(asset_location.clone(), 0, true, 1)],
			metadata: vec![],
			accounts: vec![(asset_location, 1, 100)],
			next_asset_id: None,
			reserves: vec![],
		},
		system: Default::default(),
		balances: Default::default(),
		revive: Default::default(),
	}
	.build_storage()
	.unwrap();
	let mut ext: sp_io::TestExternalities = t.into();
	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
