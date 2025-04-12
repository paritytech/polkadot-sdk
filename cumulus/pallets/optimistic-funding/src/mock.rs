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

use crate::{self as pallet_optimistic_funding, GetRank};
use crate::constants::EXISTENTIAL_DEPOSIT;
use frame_support::{
	parameter_types,
	traits::{
		ConstU32, ConstU64, EnsureOrigin, Currency, Hooks,
	},
	PalletId,
};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;
type Balance = u64;
type BlockNumber = u64;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		OptimisticFunding: pallet_optimistic_funding,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

impl system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = SS58Prefix;
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
	type RuntimeTask = ();
	type SingleBlockMigrations = ();
	type MultiBlockMigrator = ();
	type PreInherents = ();
	type PostInherents = ();
	type PostTransactions = ();
	type ExtensionsWeightInfo = ();
}

parameter_types! {
	pub const MaxLocks: u32 = 50;
	pub const MaxReserves: u32 = 50;
}

impl pallet_balances::Config for Test {
	type MaxLocks = MaxLocks;
	type MaxReserves = MaxReserves;
	type ReserveIdentifier = [u8; 8];
	type Balance = Balance;
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = frame_support::traits::ConstU64<1>;
	type AccountStore = System;
	type WeightInfo = ();
	type FreezeIdentifier = ();
	type MaxFreezes = ();
	type RuntimeHoldReason = ();
	type RuntimeFreezeReason = ();
	type DoneSlashHandler = ();
}

parameter_types! {
	pub const FundingPeriod: BlockNumber = 100;
	pub const MinimumRequestAmount: Balance = 100;
	pub const MaximumRequestAmount: Balance = 1000;
	pub const RequestDeposit: Balance = 10;
	pub const MaxActiveRequests: u32 = 100;
	pub const OptimisticFundingPalletId: PalletId = PalletId(*b"opt/fund");
}

// Mock implementation of GetRank for testing
pub struct MockRankedMembers;
impl GetRank<AccountId> for MockRankedMembers {
	fn get_rank(who: &AccountId) -> Option<u16> {
		// For testing, we'll assign ranks based on account ID
		// Account 1 has rank 0, Account 2 has rank 1, etc.
		if *who > 0 && *who < 8 {
			Some((*who - 1) as u16)
		} else {
			None
		}
	}
}

pub struct MockTreasuryOrigin;
impl EnsureOrigin<RuntimeOrigin> for MockTreasuryOrigin {
	type Success = AccountId;

	fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
		match o.clone().into() {
			Ok(system::RawOrigin::Signed(who)) if who == treasury_account() => Ok(who),
			_ => Err(o),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn successful_origin() -> RuntimeOrigin {
		RuntimeOrigin::from(system::RawOrigin::Signed(treasury_account()))
	}
}

impl pallet_optimistic_funding::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type FundingPeriod = FundingPeriod;
	type MinimumRequestAmount = MinimumRequestAmount;
	type MaximumRequestAmount = MaximumRequestAmount;
	type RequestDeposit = RequestDeposit;
	type MaxActiveRequests = MaxActiveRequests;
	type TreasuryOrigin = MockTreasuryOrigin;
	type WeightInfo = ();
	type PalletId = OptimisticFundingPalletId;
	type RankedMembers = MockRankedMembers;
}

// Helper function to get the treasury account ID
pub fn treasury_account() -> AccountId {
	OptimisticFunding::treasury_account()
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default()
		.build_storage()
		.unwrap();

	pallet_balances::GenesisConfig::<Test> {
		balances: vec![
			(1, 10000),
			(2, 10000),
			(3, 10000),
			(4, 10000),
			(5, 10000),
			(6, 10000),
			(7, 10000),
			(8, 10000),
			(9, 10000),
			(treasury_account(), 10000),
		],
		dev_accounts: None,
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

pub fn run_to_block(n: u64) {
	while System::block_number() < n {
		if System::block_number() > 1 {
			OptimisticFunding::on_finalize(System::block_number());
			System::on_finalize(System::block_number());
		}
		System::set_block_number(System::block_number() + 1);
		System::on_initialize(System::block_number());
		OptimisticFunding::on_initialize(System::block_number());
	}
}
