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

//! Test environment for NIS pallet.

use crate::{self as pallet_nis, Perquintill, WithMaximumOf};

use frame_support::{
	derive_impl, ord_parameter_types, parameter_types,
	traits::{fungible::Inspect, ConstU32, ConstU64, OnFinalize, OnInitialize, StorageMapShim},
	weights::Weight,
	PalletId,
};
use pallet_balances::{Instance1, Instance2};
use sp_core::ConstU128;
use sp_runtime::BuildStorage;

type Block = frame_system::mocking::MockBlock<Test>;

pub type Balance = u64;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances::<Instance1>,
		NisBalances: pallet_balances::<Instance2>,
		Nis: pallet_nis,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
}

impl pallet_balances::Config<Instance1> for Test {
	type Balance = Balance;
	type DustRemoval = ();
	type RuntimeEvent = RuntimeEvent;
	type ExistentialDeposit = ConstU64<1>;
	type AccountStore = System;
	type WeightInfo = ();
	type MaxLocks = ();
	type MaxReserves = ConstU32<1>;
	type ReserveIdentifier = [u8; 8];
	type FreezeIdentifier = ();
	type MaxFreezes = ();
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
}

impl pallet_balances::Config<Instance2> for Test {
	type Balance = u128;
	type DustRemoval = ();
	type RuntimeEvent = RuntimeEvent;
	type ExistentialDeposit = ConstU128<1>;
	type AccountStore = StorageMapShim<
		pallet_balances::Account<Test, Instance2>,
		u64,
		pallet_balances::AccountData<u128>,
	>;
	type WeightInfo = ();
	type MaxLocks = ();
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type FreezeIdentifier = ();
	type MaxFreezes = ();
	type RuntimeHoldReason = ();
	type RuntimeFreezeReason = ();
}

parameter_types! {
	pub IgnoredIssuance: Balance = Balances::total_balance(&0); // Account zero is ignored.
	pub const NisPalletId: PalletId = PalletId(*b"py/nis  ");
	pub static Target: Perquintill = Perquintill::zero();
	pub const MinReceipt: Perquintill = Perquintill::from_percent(1);
	pub const ThawThrottle: (Perquintill, u64) = (Perquintill::from_percent(25), 5);
	pub static MaxIntakeWeight: Weight = Weight::from_parts(2_000_000_000_000, 0);
}

ord_parameter_types! {
	pub const One: u64 = 1;
}

impl pallet_nis::Config for Test {
	type WeightInfo = ();
	type RuntimeEvent = RuntimeEvent;
	type PalletId = NisPalletId;
	type Currency = Balances;
	type CurrencyBalance = <Self as pallet_balances::Config<Instance1>>::Balance;
	type FundOrigin = frame_system::EnsureSigned<Self::AccountId>;
	type Deficit = ();
	type IgnoredIssuance = IgnoredIssuance;
	type Counterpart = NisBalances;
	type CounterpartAmount = WithMaximumOf<ConstU128<21_000_000u128>>;
	type Target = Target;
	type QueueCount = ConstU32<3>;
	type MaxQueueLen = ConstU32<3>;
	type FifoQueueLen = ConstU32<1>;
	type BasePeriod = ConstU64<3>;
	type MinBid = ConstU64<2>;
	type IntakePeriod = ConstU64<2>;
	type MaxIntakeWeight = MaxIntakeWeight;
	type MinReceipt = MinReceipt;
	type ThawThrottle = ThawThrottle;
	type RuntimeHoldReason = RuntimeHoldReason;
}

// This function basically just builds a genesis storage key/value store according to
// our desired mockup.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test, Instance1> {
		balances: vec![(1, 100), (2, 100), (3, 100), (4, 100)],
	}
	.assimilate_storage(&mut t)
	.unwrap();
	t.into()
}

// This function basically just builds a genesis storage key/value store according to
// our desired mockup, but without any balances.
#[cfg(feature = "runtime-benchmarks")]
pub fn new_test_ext_empty() -> sp_io::TestExternalities {
	frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into()
}

pub fn run_to_block(n: u64) {
	while System::block_number() < n {
		Nis::on_finalize(System::block_number());
		Balances::on_finalize(System::block_number());
		System::on_finalize(System::block_number());
		System::set_block_number(System::block_number() + 1);
		System::on_initialize(System::block_number());
		Balances::on_initialize(System::block_number());
		Nis::on_initialize(System::block_number());
	}
}
