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

//! Tests for dummy-storage-value-example

use crate::*;
use frame_support::{assert_ok, traits::ConstU64};
use sp_core::H256;
// The testing primitives are very useful for avoiding having to work with signatures
// or public keys. `u64` is used as the `AccountId` and no `Signature`s are required.
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};
// Reexport crate as its pallet name for construct_runtime.
use crate as pallet_example_basic;

type Block = frame_system::mocking::MockBlock<Test>;

// For testing the pallet, we construct a mock runtime.
frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
		Example: pallet_example_basic::{Pallet, Call, Storage, Config<T>, Event<T>},
	}
);

impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type Nonce = u64;
	type Hash = H256;
	type RuntimeCall = RuntimeCall;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = ConstU64<250>;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<u64>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

impl pallet_balances::Config for Test {
	type MaxLocks = ();
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type Balance = u64;
	type DustRemoval = ();
	type RuntimeEvent = RuntimeEvent;
	type ExistentialDeposit = ConstU64<1>;
	type AccountStore = System;
	type WeightInfo = ();
	type FreezeIdentifier = ();
	type MaxFreezes = ();
	type RuntimeHoldReason = ();
	type MaxHolds = ();
}

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
}

// add consts for genesis

#[docify::export]
/// This function builds a genesis storage key/value store according to our desired mockup.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = RuntimeGenesisConfig {
		system: Default::default(),   // System pallet default genesis config
		balances: Default::default(), // Balances pallet default genesis config
		// here we specify our pallet's genesis state
		example: pallet_example_basic::GenesisConfig { dummy: 42, dummy_value_query: 24 },
	}
	.build_storage()
	.unwrap();
	t.into()
}

#[docify::export]
#[test]
fn accumulate_dummy_works() {
	new_test_ext().execute_with(|| {
		// 42 is the value we have from building genesis for our tests
		let genesis_val = 42;
		assert_eq!(Example::dummy(), Some(genesis_val));

		// accumulate the value in Dummy by 1
		assert_ok!(Example::accumulate_dummy(RuntimeOrigin::signed(1), 1));
		assert_eq!(Example::dummy(), Some(genesis_val + 1));

		// when we reset the storage, the value in state will be `None`
		let _ = Example::do_reset_dummy(RuntimeOrigin::signed(1));
		assert_eq!(Example::dummy(), None);

		// inserting a new value again should work
		assert_ok!(Example::accumulate_dummy(RuntimeOrigin::signed(1), genesis_val));
		assert_eq!(Example::dummy(), Some(genesis_val));
	});
}

#[docify::export]
#[test]
fn accumulate_dummy_value_query_works() {
	new_test_ext().execute_with(|| {
		// 24 is the value we have from building genesis for our mock runtime environment
		let genesis_val = 24;
		assert_eq!(Example::dummy_value_query(), genesis_val);

		// accumulate the value in DummyValueQuery by 1
		let _ = Example::accumulate_value_query(RuntimeOrigin::signed(1), 1);
		assert_eq!(Example::dummy_value_query(), genesis_val + 1);

		// when we reset the storage, the value in state will be `u32::default()`
		let _ = Example::do_reset_dummy(RuntimeOrigin::signed(1));
		assert_eq!(Example::dummy_value_query(), 0);
	});
}

#[docify::export]
#[test]
fn set_dummy_works() {
	new_test_ext().execute_with(|| {
		// calling set_dummy with root origin will replace what was previously in storage
		let test_val = 133;
		assert_ok!(Example::set_dummy(RuntimeOrigin::root(), test_val.into()));
		assert_eq!(Example::dummy(), Some(test_val));
	});
}
