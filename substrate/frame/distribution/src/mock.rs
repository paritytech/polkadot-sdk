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

//! Test environment for Distribution pallet.

use crate as pallet_distribution;
pub use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU128, ConstU16, ConstU32, ConstU64, OnFinalize, OnInitialize},
	PalletId,
};
pub use sp_core::H256;
pub use sp_runtime::{
	traits::{AccountIdConversion, BlakeTwo256, IdentityLookup},
	BuildStorage,
};

pub type Block = frame_system::mocking::MockBlock<Test>;
pub type Balance = u128;
pub type AccountId = u64;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub struct Test {
		System: frame_system,
		Balances: pallet_balances,
		Distribution: pallet_distribution,
	}
);

// Feel free to remove more items from this, as they are the same as
// `frame_system::config_preludes::TestDefaultConfig`. We have only listed the full `type` list here
// for verbosity. Same for `pallet_balances::Config`.
// https://paritytech.github.io/polkadot-sdk/master/frame_support/attr.derive_impl.html
#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type AccountId = AccountId;
	type AccountData = pallet_balances::AccountData<Balance>;
	type Block = Block;
	type Lookup = IdentityLookup<Self::AccountId>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type Balance = Balance;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
}

parameter_types! {
	pub const PotId: PalletId = PalletId(*b"py/potid");
	pub const Period: u32 = 1;
	pub const MaxProjects:u32 = 50;
	pub const EpochDurationBlocks:u32 = 5;
}
impl pallet_distribution::Config for Test {
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type NativeBalance = Balances;
	type PotId = PotId;
	type RuntimeHoldReason = RuntimeHoldReason;
	type Scheduler = Scheduler;
	type BufferPeriod = Period;
	type MaxProjects = MaxProjects;
	type EpochDurationBlocks = EpochDurationBlocks;
	type BlockNumberProvider = System;
	type Preimages = Preimage;
	type WeightInfo = ();
}
//Define some accounts and use them
pub const ALICE: AccountId = 10;
pub const BOB: AccountId = 11;
pub const DAVE: AccountId = 12;
pub const EVE: AccountId = 13;
pub const BSX: Balance = 100_000_000_000;

pub fn expect_events(e: Vec<RuntimeEvent>) {
	e.into_iter().for_each(frame_system::Pallet::<Test>::assert_has_event);
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let pot_account = PotId::get().into_account_truncating();

	pallet_balances::GenesisConfig::<Test> {
		balances: vec![
			(ALICE, 200_000 * BSX),
			(BOB, 200_000 * BSX),
			(DAVE, 150_000 * BSX),
			(EVE, 150_000 * BSX),
			(pot_account, 150_000_000 * BSX),
		],
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}
