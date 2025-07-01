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

use super::*;
use crate::{
	self as pallet_origin_and_gate,
};
use frame_support::{
	assert_ok, assert_err,
	traits::{ConstU32, ConstU64, Everything},
	derive_impl,
};
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

// Import mock directly instead of through module import
#[path = "./mock.rs"]
mod mock;
use mock::*;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		OriginAndGate: pallet_origin_and_gate,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<u64>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
	type BlockHashCount = ConstU64<250>;
	type RuntimeTask = ();
	type PreInherents = ();
	type PostInherents = ();
	type PostTransactions = ();
	type SingleBlockMigrations = ();
	type MultiBlockMigrator = ();
	type ExtensionsWeightInfo = ();
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type Hashing = BlakeTwo256;
	type OriginId = u8;
	type MaxApprovals = ConstU32<100>;
	type ProposalLifetime = ConstU64<100>;
	type WeightInfo = ();
}

// This function basically just builds a genesis storage key/value store according to
// our desired mockup.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default()
		.build_storage()
		.unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

/// Helper function to create a remark call that can be used for testing
fn make_remark_call(text: &str) -> Result<Box<<Test as Config>::RuntimeCall>, &'static str> {
    // Try to parse the text as a u64
    let value = match text.parse::<u64>() {
        Ok(v) => v,
        Err(_) => return Err("Failed to parse input as u64"),
    };

    let remark = self::Call::<Test>::set_dummy {
        new_value: value,
    };
    Ok(Box::new(RuntimeCall::OriginAndGate(remark)))
}

#[test]
fn ensure_origin_works_with_and_gate() {
	new_test_ext().execute_with(|| {
		// Test AliceAndBob origin combination
		let call = make_remark_call("1000").unwrap();
		let call_hash = <<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

		// Alice proposes
		assert_ok!(OriginAndGate::propose(
			mock::RuntimeOrigin::signed(ALICE),
			call.clone(),
			ALICE_ORIGIN_ID,
			None,
		));

		// Test AliceAndBob origin directly and should fail without Bob's approval
		assert_err!(
			AliceAndBob::ensure_origin(mock::RuntimeOrigin::signed(ALICE)),
			DispatchError::Other("Origin check failed"),
		);

		// // Bob approves
		// assert_ok!(OriginAndGate::approve(
		// 	mock::RuntimeOrigin::signed(BOB),
		// 	call_hash,
		// 	ALICE_ORIGIN_ID,
		// 	BOB_ORIGIN_ID,
		// ));

		// // Now the AliceAndBob gate should pass for this call
		// // This would be tested in actual usage when the call is executed
		// // For test purposes, we're verifying the proposal is marked executed
		// assert!(Proposals::<Test>::contains_key(call_hash, ALICE_ORIGIN_ID));
		// let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
		// assert_eq!(proposal.status, ProposalStatus::Executed);
	});
}
