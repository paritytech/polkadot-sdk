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

// Tests for Multisig Pallet

#![cfg(test)]

use super::*;

use crate as multisig_reentrancy_attack;
use frame_support::{
	assert_noop, assert_ok, derive_impl,
	traits::{ConstU32, ConstU64, Contains},
};
use sp_runtime::{BuildStorage, TokenError};

type Block = frame_system::mocking::MockBlockU32<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		Multisig: pallet_multisig,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u64>;
	type BaseCallFilter = TestBaseCallFilter;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type ReserveIdentifier = [u8; 8];
	type AccountStore = System;
}

pub struct TestBaseCallFilter;
impl Contains<RuntimeCall> for TestBaseCallFilter {
	fn contains(c: &RuntimeCall) -> bool {
		match *c {
			RuntimeCall::Balances(_) => true,
			// Needed for benchmarking
			RuntimeCall::System(frame_system::Call::remark { .. }) => true,
			_ => false,
		}
	}
}
impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type Currency = Balances;
}

use pallet_balances::Call as BalancesCall;


fn now() -> Timepoint<u32> {
	Multisig::timepoint()
}

fn call_transfer(dest: u64, value: u64) -> Box<RuntimeCall> {
	Box::new(RuntimeCall::Balances(BalancesCall::transfer_allow_death { dest, value }))
}

#[test]
fn multisig_deposit_is_taken_and_returned() {
	new_test_ext().execute_with(|| {
		let multi = Multisig::multi_account_id(&[1, 2, 3][..], 2);
		assert_ok!(Balances::transfer_allow_death(RuntimeOrigin::signed(1), multi, 5));
		assert_ok!(Balances::transfer_allow_death(RuntimeOrigin::signed(2), multi, 5));
		assert_ok!(Balances::transfer_allow_death(RuntimeOrigin::signed(3), multi, 5));

		let call = call_transfer(6, 15);
		let call_weight = call.get_dispatch_info().weight;
		assert_ok!(Multisig::as_multi(
			RuntimeOrigin::signed(1),
			2,
			vec![2, 3],
			None,
			call.clone(),
			Weight::zero()
		));
	});
}
