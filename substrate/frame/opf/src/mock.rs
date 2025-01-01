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

//! Test environment for OPF pallet.
use crate as pallet_opf;
pub use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU32, EqualPrivilegeOnly, OnFinalize, OnInitialize},
	weights::Weight,
	PalletId,
};
pub use sp_runtime::{
	traits::{AccountIdConversion, IdentityLookup},
	BuildStorage,
};

pub use frame_system::EnsureRoot;
pub type Block = frame_system::mocking::MockBlock<Test>;
pub type Balance = u64;
pub type AccountId = u64;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub struct Test {
		System: frame_system,
		Balances: pallet_balances,
		Preimage: pallet_preimage,
		Scheduler: pallet_scheduler,
		Opf: pallet_opf,
	}
);

parameter_types! {
	pub MaxWeight: Weight = Weight::from_parts(2_000_000_000_000, u64::MAX);
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type AccountId = AccountId;
	type AccountData = pallet_balances::AccountData<Balance>;
	type Block = Block;
	type Lookup = IdentityLookup<Self::AccountId>;
}

impl pallet_preimage::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type Currency = Balances;
	type ManagerOrigin = EnsureRoot<u64>;
	type Consideration = ();
}
impl pallet_scheduler::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeOrigin = RuntimeOrigin;
	type PalletsOrigin = OriginCaller;
	type RuntimeCall = RuntimeCall;
	type MaximumWeight = MaxWeight;
	type ScheduleOrigin = EnsureRoot<u64>;
	type MaxScheduledPerBlock = ConstU32<100>;
	type WeightInfo = ();
	type OriginPrivilegeCmp = EqualPrivilegeOnly;
	type Preimages = Preimage;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

parameter_types! {
    pub const PotId: PalletId = PalletId(*b"py/potid");
	pub const MaxProjects:u32 = 50;
	pub const TemporaryRewards: Balance = 100_000;
	pub const VoteLockingPeriod:u32 = 10;
	pub const VotingPeriod:u32 = 30;
}
impl pallet_opf::Config for Test {
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type NativeBalance = Balances;
	type PotId = PotId;
	type MaxProjects = MaxProjects;
	type VotingPeriod = VotingPeriod;
	type ClaimingPeriod = VotingPeriod;
	type VoteValidityPeriod = VotingPeriod;
	type BlockNumberProvider = System;
	type TemporaryRewards = TemporaryRewards;
	type Preimages = Preimage;
	type Scheduler = Scheduler;
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