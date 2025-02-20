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

//! Tests mock for `pallet-assets-freezer`.

use crate as pallet_assets_holder;
pub use crate::*;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{derive_impl, traits::AsEnsureOriginWithArg};
use scale_info::TypeInfo;
use sp_runtime::BuildStorage;

pub type AccountId = <Test as frame_system::Config>::AccountId;
pub type Balance = <Test as pallet_balances::Config>::Balance;
pub type AssetId = <Test as pallet_assets::Config>::AssetId;
type Block = frame_system::mocking::MockBlock<Test>;

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
	pub type AssetsHolder = pallet_assets_holder;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u64>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig as pallet_balances::DefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig as pallet_assets::DefaultConfig)]
impl pallet_assets::Config for Test {
	// type AssetAccountDeposit = ConstU64<1>;
	type CreateOrigin = AsEnsureOriginWithArg<frame_system::EnsureSigned<u64>>;
	type ForceOrigin = frame_system::EnsureRoot<u64>;
	type Currency = Balances;
	type Holder = AssetsHolder;
}

#[derive(
	Decode,
	DecodeWithMemTracking,
	Encode,
	MaxEncodedLen,
	PartialEq,
	Eq,
	Ord,
	PartialOrd,
	TypeInfo,
	Debug,
	Clone,
	Copy,
)]
pub enum DummyHoldReason {
	Governance,
	Staking,
	Other,
}

impl VariantCount for DummyHoldReason {
	// Intentionally set below the actual count of variants, to allow testing for `can_freeze`
	const VARIANT_COUNT: u32 = 3;
}

impl Config for Test {
	type RuntimeHoldReason = DummyHoldReason;
	type RuntimeEvent = RuntimeEvent;
}

pub fn new_test_ext(execute: impl FnOnce()) -> sp_io::TestExternalities {
	let t = RuntimeGenesisConfig {
		assets: pallet_assets::GenesisConfig {
			assets: vec![(1, 0, true, 1)],
			metadata: vec![],
			accounts: vec![(1, 1, 100)],
			next_asset_id: None,
		},
		system: Default::default(),
		balances: Default::default(),
	}
	.build_storage()
	.unwrap();
	let mut ext: sp_io::TestExternalities = t.into();
	ext.execute_with(|| {
		System::set_block_number(1);
		execute();
		frame_support::assert_ok!(AssetsHolder::do_try_state());
	});

	ext
}
