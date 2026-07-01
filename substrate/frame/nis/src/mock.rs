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

use frame::{runtime::prelude::*, testing_prelude::*, traits::StorageMapShim};

use crate::{self as pallet_nis, *};

pub type Balance = u64;

type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
#[frame_construct_runtime]
mod runtime {
	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeError,
		RuntimeEvent,
		RuntimeFreezeReason,
		RuntimeHoldReason,
		RuntimeOrigin,
		RuntimeTask
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system;
	#[runtime::pallet_index(1)]
	pub type Balances = pallet_balances<Instance1>;
	#[runtime::pallet_index(2)]
	pub type NisBalances = pallet_balances<Instance2>;
	#[runtime::pallet_index(3)]
	pub type Nis = pallet_nis;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
}

impl pallet_balances::Config<pallet_balances::Instance1> for Test {
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
	type DoneSlashHandler = ();
}

impl pallet_balances::Config<pallet_balances::Instance2> for Test {
	type Balance = u128;
	type DustRemoval = ();
	type RuntimeEvent = RuntimeEvent;
	type ExistentialDeposit = ConstU128<1>;
	type AccountStore = StorageMapShim<
		pallet_balances::Account<Test, pallet_balances::Instance2>,
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
	type DoneSlashHandler = ();
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
	type CurrencyBalance = <Self as pallet_balances::Config<pallet_balances::Instance1>>::Balance;
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
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkSetup = ();
}

// This function basically just builds a genesis storage key/value store according to
// our desired mockup.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test, pallet_balances::Instance1> {
		balances: vec![(1, 100), (2, 100), (3, 100), (4, 100)],
		..Default::default()
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
