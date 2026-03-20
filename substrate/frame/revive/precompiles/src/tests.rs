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

//! Unit tests for the vesting precompile.

use crate::{Vesting, VestingBalance, mock::*};
use alloy_core::sol_types::SolValue;
use frame_support::traits::{Currency, VestingSchedule};
use pallet_revive::{
	AddressMapper,
	precompiles::{Error, Precompile, U256},
};
use pallet_revive_uapi::precompiles::vesting::IVesting;

type CurrencyOf<T> = <T as pallet_vesting::Config>::Currency;

fn precompile_address() -> [u8; 20] {
	Vesting::<Test>::MATCHER.base_address()
}

/// Add a vesting schedule that locks `locked` tokens over 20 blocks starting at block 0.
fn add_vesting_schedule(who: &u64, locked: u64) {
	let per_block = locked / 20;
	<pallet_vesting::Pallet<Test> as VestingSchedule<u64>>::add_vesting_schedule(
		who, locked, per_block, 0,
	)
	.expect("adding vesting schedule should succeed");
}

/// Helper: call the precompile with the given input via `CallSetup`, returning the result.
fn call_vesting(input: &IVesting::IVestingCalls) -> Result<alloc::vec::Vec<u8>, Error> {
	let mut call_setup = pallet_revive::call_builder::CallSetup::<Test>::default();
	let (mut ext, _) = call_setup.ext();
	Vesting::<Test>::call(&precompile_address(), input, &mut ext)
}

/// Helper: call the precompile in read-only mode.
fn call_vesting_read_only(input: &IVesting::IVestingCalls) -> Result<alloc::vec::Vec<u8>, Error> {
	let mut call_setup = pallet_revive::call_builder::CallSetup::<Test>::default();
	let (mut ext, _) = call_setup.ext();
	ext.set_read_only(true);
	Vesting::<Test>::call(&precompile_address(), input, &mut ext)
}

/// Helper: call the precompile in delegate-call mode.
fn call_vesting_delegate(input: &IVesting::IVestingCalls) -> Result<alloc::vec::Vec<u8>, Error> {
	let mut call_setup = pallet_revive::call_builder::CallSetup::<Test>::default();
	let (mut ext, _) = call_setup.ext();
	ext.set_delegate_call(true);
	Vesting::<Test>::call(&precompile_address(), input, &mut ext)
}

/// Decode a U256 from abi-encoded precompile output.
fn decode_balance(raw: &[u8]) -> U256 {
	U256::from_big_endian(&<[u8; 32]>::abi_decode(raw).unwrap())
}

// ---------------------------------------------------------------------------
// vest()
// ---------------------------------------------------------------------------

#[test]
fn vest_succeeds_with_active_schedule() {
	new_test_ext().execute_with(|| {
		let mut call_setup = pallet_revive::call_builder::CallSetup::<Test>::default();
		let caller = call_setup.contract().caller;

		let locked: VestingBalance<Test> = 10_000;
		CurrencyOf::<Test>::make_free_balance_be(&caller, locked * 10);
		add_vesting_schedule(&caller, locked);

		let input = IVesting::IVestingCalls::vest(IVesting::vestCall {});
		let (mut ext, _) = call_setup.ext();
		let result = Vesting::<Test>::call(&precompile_address(), &input, &mut ext);
		assert!(result.is_ok());
		assert!(result.unwrap().is_empty());
	});
}

#[test]
fn vest_reverts_with_no_schedule() {
	new_test_ext().execute_with(|| {
		let input = IVesting::IVestingCalls::vest(IVesting::vestCall {});
		let result = call_vesting(&input);
		assert!(result.is_err());
		match result.unwrap_err() {
			Error::Revert(msg) => {
				let msg_str = &msg.reason;
				assert!(
					msg_str.contains("vest failed"),
					"expected 'vest failed' in error, got: {msg_str}"
				);
			},
			other => panic!("expected Revert error, got: {other:?}"),
		}
	});
}

#[test]
fn vest_reverts_in_read_only_context() {
	new_test_ext().execute_with(|| {
		let input = IVesting::IVestingCalls::vest(IVesting::vestCall {});
		let result = call_vesting_read_only(&input);
		assert!(result.is_err());
		match result.unwrap_err() {
			Error::Error(exec_err) => {
				let expected: sp_runtime::DispatchError =
					pallet_revive::Error::<Test>::StateChangeDenied.into();
				assert_eq!(exec_err.error, expected);
			},
			other => panic!("expected Error(StateChangeDenied), got: {other:?}"),
		}
	});
}

#[test]
fn vest_reverts_under_delegate_call() {
	new_test_ext().execute_with(|| {
		let input = IVesting::IVestingCalls::vest(IVesting::vestCall {});
		let result = call_vesting_delegate(&input);
		assert!(result.is_err());
		match result.unwrap_err() {
			Error::Revert(msg) => {
				let msg_str = &msg.reason;
				assert!(
					msg_str.contains("delegate call"),
					"expected 'delegate call' in error, got: {msg_str}"
				);
			},
			other => panic!("expected Revert error about delegate call, got: {other:?}"),
		}
	});
}

#[test]
fn vest_actually_unlocks_funds() {
	new_test_ext().execute_with(|| {
		let mut call_setup = pallet_revive::call_builder::CallSetup::<Test>::default();
		let caller = call_setup.contract().caller;

		let locked: VestingBalance<Test> = 10_000;
		CurrencyOf::<Test>::make_free_balance_be(&caller, locked * 10);
		add_vesting_schedule(&caller, locked);

		// Advance past the full schedule so vest() has something to unlock.
		frame_system::Pallet::<Test>::set_block_number(21);

		let input = IVesting::IVestingCalls::vest(IVesting::vestCall {});
		let (mut ext, _) = call_setup.ext();
		let result = Vesting::<Test>::call(&precompile_address(), &input, &mut ext);
		assert!(result.is_ok());

		// After vest(), the vesting schedule should be removed entirely.
		let schedule =
			<pallet_vesting::Pallet<Test> as VestingSchedule<u64>>::vesting_balance(&caller);
		assert_eq!(schedule, None, "vesting schedule should be removed after full vest");
	});
}

// ---------------------------------------------------------------------------
// vestOther()
// ---------------------------------------------------------------------------

#[test]
fn vest_other_succeeds_with_active_schedule() {
	new_test_ext().execute_with(|| {
		let mut call_setup = pallet_revive::call_builder::CallSetup::<Test>::default();

		let target_addr = pallet_revive::precompiles::H160::from_low_u64_be(0xBEEF);
		let target_account =
			<Test as pallet_revive::Config>::AddressMapper::to_account_id(&target_addr);
		let locked: VestingBalance<Test> = 10_000;
		CurrencyOf::<Test>::make_free_balance_be(&target_account, locked * 10);
		add_vesting_schedule(&target_account, locked);

		let input = IVesting::IVestingCalls::vestOther(IVesting::vestOtherCall {
			target: alloy_core::primitives::Address::from_slice(target_addr.as_bytes()),
		});
		let address = precompile_address();
		let (mut ext, _) = call_setup.ext();
		let result = Vesting::<Test>::call(&address, &input, &mut ext);
		assert!(result.is_ok());
		assert!(result.unwrap().is_empty());
	});
}

#[test]
fn vest_other_actually_unlocks_funds_for_target() {
	new_test_ext().execute_with(|| {
		let mut call_setup = pallet_revive::call_builder::CallSetup::<Test>::default();

		let target_addr = pallet_revive::precompiles::H160::from_low_u64_be(0xBEEF);
		let target_account =
			<Test as pallet_revive::Config>::AddressMapper::to_account_id(&target_addr);
		let locked: VestingBalance<Test> = 10_000;
		CurrencyOf::<Test>::make_free_balance_be(&target_account, locked * 10);
		add_vesting_schedule(&target_account, locked);

		// Advance past the full schedule.
		frame_system::Pallet::<Test>::set_block_number(21);

		let input = IVesting::IVestingCalls::vestOther(IVesting::vestOtherCall {
			target: alloy_core::primitives::Address::from_slice(target_addr.as_bytes()),
		});
		let address = precompile_address();
		let (mut ext, _) = call_setup.ext();
		let result = Vesting::<Test>::call(&address, &input, &mut ext);
		assert!(result.is_ok());

		// After vestOther(), the target's vesting schedule should be removed.
		let schedule = <pallet_vesting::Pallet<Test> as VestingSchedule<u64>>::vesting_balance(
			&target_account,
		);
		assert_eq!(schedule, None, "target's vesting schedule should be removed after full vest");
	});
}

#[test]
fn vest_other_reverts_with_no_schedule_on_target() {
	new_test_ext().execute_with(|| {
		let target_addr = pallet_revive::precompiles::H160::from_low_u64_be(0xBEEF);
		let input = IVesting::IVestingCalls::vestOther(IVesting::vestOtherCall {
			target: alloy_core::primitives::Address::from_slice(target_addr.as_bytes()),
		});
		let result = call_vesting(&input);
		assert!(result.is_err());
		match result.unwrap_err() {
			Error::Revert(msg) => {
				let msg_str = &msg.reason;
				assert!(
					msg_str.contains("vestOther failed"),
					"expected 'vestOther failed' in error, got: {msg_str}"
				);
			},
			other => panic!("expected Revert error, got: {other:?}"),
		}
	});
}

#[test]
fn vest_other_reverts_in_read_only_context() {
	new_test_ext().execute_with(|| {
		let target_addr = pallet_revive::precompiles::H160::from_low_u64_be(0xBEEF);
		let input = IVesting::IVestingCalls::vestOther(IVesting::vestOtherCall {
			target: alloy_core::primitives::Address::from_slice(target_addr.as_bytes()),
		});
		let result = call_vesting_read_only(&input);
		assert!(result.is_err());
		match result.unwrap_err() {
			Error::Error(exec_err) => {
				let expected: sp_runtime::DispatchError =
					pallet_revive::Error::<Test>::StateChangeDenied.into();
				assert_eq!(exec_err.error, expected);
			},
			other => panic!("expected Error(StateChangeDenied), got: {other:?}"),
		}
	});
}

#[test]
fn vest_other_reverts_under_delegate_call() {
	new_test_ext().execute_with(|| {
		let target_addr = pallet_revive::precompiles::H160::from_low_u64_be(0xBEEF);
		let input = IVesting::IVestingCalls::vestOther(IVesting::vestOtherCall {
			target: alloy_core::primitives::Address::from_slice(target_addr.as_bytes()),
		});
		let result = call_vesting_delegate(&input);
		assert!(result.is_err());
		match result.unwrap_err() {
			Error::Revert(msg) => {
				let msg_str = &msg.reason;
				assert!(
					msg_str.contains("delegate call"),
					"expected 'delegate call' in error, got: {msg_str}"
				);
			},
			other => panic!("expected Revert error about delegate call, got: {other:?}"),
		}
	});
}

// ---------------------------------------------------------------------------
// vestingBalance()
// ---------------------------------------------------------------------------

#[test]
fn vesting_balance_returns_correct_locked_amount() {
	new_test_ext().execute_with(|| {
		let mut call_setup = pallet_revive::call_builder::CallSetup::<Test>::default();
		let caller = call_setup.contract().caller;

		let locked: VestingBalance<Test> = 10_000;
		CurrencyOf::<Test>::make_free_balance_be(&caller, locked * 10);
		add_vesting_schedule(&caller, locked);

		let input = IVesting::IVestingCalls::vestingBalance(IVesting::vestingBalanceCall {});

		// At block 1 with per_block=500 (10_000/20), one block has vested: 10_000 - 500 = 9_500.
		let (mut ext, _) = call_setup.ext();
		let result = Vesting::<Test>::call(&precompile_address(), &input, &mut ext);
		let raw = result.unwrap();
		let balance = decode_balance(&raw);
		let per_block = locked / 20;
		let expected_locked = locked - per_block;
		assert_eq!(balance, U256::from(expected_locked));

		// Advance to block 10: half the 20-block schedule has elapsed.
		// per_block = 500, 10 blocks vested => 10_000 - 5_000 = 5_000 locked.
		frame_system::Pallet::<Test>::set_block_number(10);
		let (mut ext, _) = call_setup.ext();
		let result = Vesting::<Test>::call(&precompile_address(), &input, &mut ext);
		let raw = result.unwrap();
		let balance = decode_balance(&raw);
		assert_eq!(balance, U256::from(5_000u64));
	});
}

#[test]
fn vesting_balance_returns_zero_when_no_schedule() {
	new_test_ext().execute_with(|| {
		let input = IVesting::IVestingCalls::vestingBalance(IVesting::vestingBalanceCall {});
		let result = call_vesting(&input);
		let raw = result.unwrap();
		let balance = decode_balance(&raw);
		assert_eq!(balance, U256::zero());
	});
}

#[test]
fn vesting_balance_works_in_read_only_context() {
	new_test_ext().execute_with(|| {
		let input = IVesting::IVestingCalls::vestingBalance(IVesting::vestingBalanceCall {});
		let result = call_vesting_read_only(&input);
		assert!(result.is_ok(), "vestingBalance should succeed in read-only context");
	});
}

#[test]
fn vesting_balance_works_under_delegate_call() {
	new_test_ext().execute_with(|| {
		let input = IVesting::IVestingCalls::vestingBalance(IVesting::vestingBalanceCall {});
		let result = call_vesting_delegate(&input);
		assert!(result.is_ok(), "vestingBalance should succeed under delegate call");
	});
}

// ---------------------------------------------------------------------------
// vestingBalanceOf()
// ---------------------------------------------------------------------------

#[test]
fn vesting_balance_of_returns_correct_locked_amount() {
	new_test_ext().execute_with(|| {
		let mut call_setup = pallet_revive::call_builder::CallSetup::<Test>::default();

		let target_addr = pallet_revive::precompiles::H160::from_low_u64_be(0xBEEF);
		let target_account =
			<Test as pallet_revive::Config>::AddressMapper::to_account_id(&target_addr);
		let locked: VestingBalance<Test> = 10_000;
		CurrencyOf::<Test>::make_free_balance_be(&target_account, locked * 10);
		add_vesting_schedule(&target_account, locked);

		let input = IVesting::IVestingCalls::vestingBalanceOf(IVesting::vestingBalanceOfCall {
			target: alloy_core::primitives::Address::from_slice(target_addr.as_bytes()),
		});
		let (mut ext, _) = call_setup.ext();
		let result = Vesting::<Test>::call(&precompile_address(), &input, &mut ext);
		let raw = result.unwrap();
		let balance = decode_balance(&raw);
		// At block 1 with per_block=500 (10_000/20), one block has vested: 10_000 - 500 = 9_500.
		let per_block = locked / 20;
		let expected_locked = locked - per_block;
		assert_eq!(balance, U256::from(expected_locked));
	});
}

#[test]
fn vesting_balance_of_returns_zero_when_no_schedule() {
	new_test_ext().execute_with(|| {
		let target_addr = pallet_revive::precompiles::H160::from_low_u64_be(0xDEAD);
		let input = IVesting::IVestingCalls::vestingBalanceOf(IVesting::vestingBalanceOfCall {
			target: alloy_core::primitives::Address::from_slice(target_addr.as_bytes()),
		});
		let result = call_vesting(&input);
		let raw = result.unwrap();
		let balance = decode_balance(&raw);
		assert_eq!(balance, U256::zero());
	});
}

#[test]
fn vesting_balance_of_works_in_read_only_context() {
	new_test_ext().execute_with(|| {
		let target_addr = pallet_revive::precompiles::H160::from_low_u64_be(0xBEEF);
		let input = IVesting::IVestingCalls::vestingBalanceOf(IVesting::vestingBalanceOfCall {
			target: alloy_core::primitives::Address::from_slice(target_addr.as_bytes()),
		});
		let result = call_vesting_read_only(&input);
		assert!(result.is_ok(), "vestingBalanceOf should succeed in read-only context");
	});
}

#[test]
fn vesting_balance_of_works_under_delegate_call() {
	new_test_ext().execute_with(|| {
		let target_addr = pallet_revive::precompiles::H160::from_low_u64_be(0xBEEF);
		let input = IVesting::IVestingCalls::vestingBalanceOf(IVesting::vestingBalanceOfCall {
			target: alloy_core::primitives::Address::from_slice(target_addr.as_bytes()),
		});
		let result = call_vesting_delegate(&input);
		assert!(result.is_ok(), "vestingBalanceOf should succeed under delegate call");
	});
}

// ---------------------------------------------------------------------------
// Multiple vesting schedules
// ---------------------------------------------------------------------------

#[test]
fn vesting_balance_aggregates_multiple_schedules() {
	new_test_ext().execute_with(|| {
		let mut call_setup = pallet_revive::call_builder::CallSetup::<Test>::default();
		let caller = call_setup.contract().caller;

		CurrencyOf::<Test>::make_free_balance_be(&caller, 500_000);

		// Schedule 1: 10_000 locked over 20 blocks (per_block = 500).
		add_vesting_schedule(&caller, 10_000);
		// Schedule 2: 20_000 locked over 20 blocks (per_block = 1_000).
		add_vesting_schedule(&caller, 20_000);

		let input = IVesting::IVestingCalls::vestingBalance(IVesting::vestingBalanceCall {});
		let (mut ext, _) = call_setup.ext();
		let result = Vesting::<Test>::call(&precompile_address(), &input, &mut ext);
		let raw = result.unwrap();
		let balance = decode_balance(&raw);

		// At block 1: schedule 1 locked = 10_000 - 500 = 9_500,
		//             schedule 2 locked = 20_000 - 1_000 = 19_000.
		// Total = 28_500.
		assert_eq!(balance, U256::from(28_500u64));
	});
}

// ---------------------------------------------------------------------------
// Fully vested (edge case)
// ---------------------------------------------------------------------------

#[test]
fn vesting_balance_returns_zero_when_fully_vested() {
	new_test_ext().execute_with(|| {
		let mut call_setup = pallet_revive::call_builder::CallSetup::<Test>::default();
		let caller = call_setup.contract().caller;

		let locked: VestingBalance<Test> = 10_000;
		CurrencyOf::<Test>::make_free_balance_be(&caller, locked * 10);
		add_vesting_schedule(&caller, locked);

		// Advance to block 21 so the 20-block schedule is fully vested.
		frame_system::Pallet::<Test>::set_block_number(21);

		let input = IVesting::IVestingCalls::vestingBalance(IVesting::vestingBalanceCall {});
		let (mut ext, _) = call_setup.ext();
		let result = Vesting::<Test>::call(&precompile_address(), &input, &mut ext);
		let raw = result.unwrap();
		let balance = decode_balance(&raw);
		assert_eq!(balance, U256::zero(), "fully vested schedule should report 0 locked");
	});
}
