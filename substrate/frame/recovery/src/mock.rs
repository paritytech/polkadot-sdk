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

//! Test utilities

use super::*;

use crate as recovery;
use crate::HoldReason;
use frame::{
	deps::sp_io, testing_prelude::*, token::fungible::HoldConsideration, traits::LinearStoragePrice,
};

type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		Recovery: recovery,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u128>;
}

parameter_types! {
	pub const ExistentialDeposit: u64 = 1;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type Balance = u128;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
}

parameter_types! {
	pub const MaxFriendsPerConfig: u32 = 128;
	pub const MaxConfigsPerAccount: u32 = 128;

	pub const FriendGroupsHoldReason: RuntimeHoldReason = RuntimeHoldReason::Recovery(HoldReason::FriendGroups);
}

impl Config for Test {
	type RuntimeCall = RuntimeCall;
	type RuntimeHoldReason = RuntimeHoldReason;
	type BlockNumberProvider = System;
	type Currency = Balances;
	type FriendGroupsConsideration = HoldConsideration<
		u64,
		Balances,
		FriendGroupsHoldReason,
		LinearStoragePrice<ConstU128<5>, ConstU128<1>, u128>, // 5 + n
	>;
	type AttemptConsideration = ();
	type InheritorConsideration = ();
	type MaxFriendsPerConfig = MaxFriendsPerConfig;
	type MaxConfigsPerAccount = MaxConfigsPerAccount;
	type WeightInfo = ();
}

pub type BalancesCall = pallet_balances::Call<Test>;
pub type RecoveryCall = super::Call<Test>;

pub const ALICE: u64 = 1;
pub const BOB: u64 = 2;
pub const CHARLIE: u64 = 3;
pub const DAVE: u64 = 4;
pub const EVE: u64 = 5;
pub const FERDIE: u64 = 6;

pub const START_BALANCE: u128 = 1000;

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![
			(ALICE, START_BALANCE),
			(BOB, START_BALANCE),
			(CHARLIE, START_BALANCE),
			(DAVE, START_BALANCE),
			(EVE, START_BALANCE),
			(FERDIE, START_BALANCE),
		],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();
	let mut ext: sp_io::TestExternalities = t.into();
	ext.execute_with(|| System::set_block_number(1));
	ext
}
