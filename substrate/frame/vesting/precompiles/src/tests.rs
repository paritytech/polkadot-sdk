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

use crate::{IVesting, Vesting, VestingBalance, mock::*};
use alloy_core::sol_types::{SolCall, SolInterface, SolValue};
use frame_support::{
	traits::{Currency, VestingSchedule},
	weights::Weight,
};
use pallet_revive::{
	AddressMapper, Code, ExecReturnValue, TransactionLimits,
	evm::CallTracer,
	precompiles::{Precompile, U256},
	test_utils::builder::{BareCallBuilder, BareInstantiateBuilder, Contract},
	tracing::trace,
};
use pallet_revive_fixtures::{Caller, FixtureType, compile_module_with_type};
use sp_core::H160;
use test_case::test_case;

type CurrencyOf<T> = <T as pallet_vesting::Config>::Currency;

const ALICE: u64 = 1;
const TARGET: u64 = 0xBEEF;

fn precompile_address() -> H160 {
	H160(Vesting::<Test>::MATCHER.base_address())
}

fn target_h160() -> H160 {
	<Test as pallet_revive::Config>::AddressMapper::to_address(&TARGET)
}

fn target_alloy() -> alloy_core::primitives::Address {
	alloy_core::primitives::Address::from_slice(target_h160().as_bytes())
}

fn add_vesting_schedule(who: &u64, locked: u64) {
	let per_block = locked / 20;
	<pallet_vesting::Pallet<Test> as VestingSchedule<u64>>::add_vesting_schedule(
		who, locked, per_block, 0,
	)
	.expect("adding vesting schedule should succeed");
}

fn bare_call(input: &IVesting::IVestingCalls) -> BareCallBuilder<Test> {
	BareCallBuilder::<Test>::bare_call(RuntimeOrigin::signed(ALICE), precompile_address())
		.data(input.abi_encode())
}

fn decode_balance(result: &ExecReturnValue) -> U256 {
	assert!(!result.did_revert(), "unexpected revert: {:?}", result.data);
	U256::from_big_endian(&<[u8; 32]>::abi_decode(&result.data).unwrap())
}

// ---------------------------------------------------------------------------
// vest()
// ---------------------------------------------------------------------------

#[test]
fn vest_succeeds_with_active_schedule() {
	new_test_ext().execute_with(|| {
		let locked: VestingBalance<Test> = 10_000;
		CurrencyOf::<Test>::make_free_balance_be(&ALICE, locked * 10);
		add_vesting_schedule(&ALICE, locked);

		let input = IVesting::IVestingCalls::vest(IVesting::vestCall {});
		let result = bare_call(&input).build_and_unwrap_result();
		assert!(!result.did_revert());
		assert!(result.data.is_empty());
	});
}

#[test]
fn vest_actually_unlocks_funds() {
	new_test_ext().execute_with(|| {
		let locked: VestingBalance<Test> = 10_000;
		CurrencyOf::<Test>::make_free_balance_be(&ALICE, locked * 10);
		add_vesting_schedule(&ALICE, locked);

		// Advance past the full schedule so vest() has something to unlock.
		frame_system::Pallet::<Test>::set_block_number(21);

		let input = IVesting::IVestingCalls::vest(IVesting::vestCall {});
		let result = bare_call(&input).build_and_unwrap_result();
		assert!(!result.did_revert());

		// After vest(), the vesting schedule should be removed entirely.
		let schedule =
			<pallet_vesting::Pallet<Test> as VestingSchedule<u64>>::vesting_balance(&ALICE);
		assert_eq!(schedule, None, "vesting schedule should be removed after full vest");
	});
}

// ---------------------------------------------------------------------------
// vestOther()
// ---------------------------------------------------------------------------

#[test]
fn vest_other_succeeds_with_active_schedule() {
	new_test_ext().execute_with(|| {
		let locked: VestingBalance<Test> = 10_000;
		CurrencyOf::<Test>::make_free_balance_be(&TARGET, locked * 10);
		add_vesting_schedule(&TARGET, locked);

		let input =
			IVesting::IVestingCalls::vestOther(IVesting::vestOtherCall { target: target_alloy() });
		let result = bare_call(&input).build_and_unwrap_result();
		assert!(!result.did_revert());
		assert!(result.data.is_empty());
	});
}

#[test]
fn vest_other_actually_unlocks_funds_for_target() {
	new_test_ext().execute_with(|| {
		let locked: VestingBalance<Test> = 10_000;
		CurrencyOf::<Test>::make_free_balance_be(&TARGET, locked * 10);
		add_vesting_schedule(&TARGET, locked);

		frame_system::Pallet::<Test>::set_block_number(21);

		let input =
			IVesting::IVestingCalls::vestOther(IVesting::vestOtherCall { target: target_alloy() });
		let result = bare_call(&input).build_and_unwrap_result();
		assert!(!result.did_revert());

		let schedule =
			<pallet_vesting::Pallet<Test> as VestingSchedule<u64>>::vesting_balance(&TARGET);
		assert_eq!(schedule, None, "target's vesting schedule should be removed after full vest");
	});
}

// ---------------------------------------------------------------------------
// vestingBalance() & vestingBalanceOf()
// ---------------------------------------------------------------------------

#[test]
fn vesting_balance_returns_correct_locked_amount() {
	new_test_ext().execute_with(|| {
		let locked: VestingBalance<Test> = 10_000;
		CurrencyOf::<Test>::make_free_balance_be(&ALICE, locked * 10);
		add_vesting_schedule(&ALICE, locked);

		let input = IVesting::IVestingCalls::vestingBalance(IVesting::vestingBalanceCall {});

		// At block 1 with per_block=500 (10_000/20), one block has vested: 10_000 - 500 = 9_500.
		let result = bare_call(&input).build_and_unwrap_result();
		assert_eq!(decode_balance(&result), U256::from(9_500u64));

		// Advance to block 10: half the 20-block schedule has elapsed.
		frame_system::Pallet::<Test>::set_block_number(10);
		let result = bare_call(&input).build_and_unwrap_result();
		assert_eq!(decode_balance(&result), U256::from(5_000u64));
	});
}

#[test]
fn vesting_balance_returns_zero_when_no_schedule() {
	new_test_ext().execute_with(|| {
		let input = IVesting::IVestingCalls::vestingBalance(IVesting::vestingBalanceCall {});
		let result = bare_call(&input).build_and_unwrap_result();
		assert_eq!(decode_balance(&result), U256::zero());
	});
}

#[test]
fn vesting_balance_returns_zero_when_fully_vested() {
	new_test_ext().execute_with(|| {
		let locked: VestingBalance<Test> = 10_000;
		CurrencyOf::<Test>::make_free_balance_be(&ALICE, locked * 10);
		add_vesting_schedule(&ALICE, locked);

		frame_system::Pallet::<Test>::set_block_number(21);

		let input = IVesting::IVestingCalls::vestingBalance(IVesting::vestingBalanceCall {});
		let result = bare_call(&input).build_and_unwrap_result();
		assert_eq!(decode_balance(&result), U256::zero());
	});
}

#[test]
fn vesting_balance_of_returns_correct_locked_amount() {
	new_test_ext().execute_with(|| {
		let locked: VestingBalance<Test> = 10_000;
		CurrencyOf::<Test>::make_free_balance_be(&TARGET, locked * 10);
		add_vesting_schedule(&TARGET, locked);

		let input = IVesting::IVestingCalls::vestingBalanceOf(IVesting::vestingBalanceOfCall {
			target: target_alloy(),
		});
		let result = bare_call(&input).build_and_unwrap_result();
		assert_eq!(decode_balance(&result), U256::from(9_500u64));
	});
}

#[test]
fn vesting_balance_of_returns_zero_when_no_schedule() {
	new_test_ext().execute_with(|| {
		let input = IVesting::IVestingCalls::vestingBalanceOf(IVesting::vestingBalanceOfCall {
			target: target_alloy(),
		});
		let result = bare_call(&input).build_and_unwrap_result();
		assert_eq!(decode_balance(&result), U256::zero());
	});
}

#[test]
fn vesting_balance_aggregates_multiple_schedules() {
	new_test_ext().execute_with(|| {
		CurrencyOf::<Test>::make_free_balance_be(&ALICE, 500_000);
		add_vesting_schedule(&ALICE, 10_000);
		add_vesting_schedule(&ALICE, 20_000);

		let input = IVesting::IVestingCalls::vestingBalance(IVesting::vestingBalanceCall {});
		let result = bare_call(&input).build_and_unwrap_result();
		// At block 1: schedule 1 locked = 10_000 - 500 = 9_500,
		//             schedule 2 locked = 20_000 - 1_000 = 19_000.
		// Total = 28_500.
		assert_eq!(decode_balance(&result), U256::from(28_500u64));
	});
}

// ---------------------------------------------------------------------------
// vestedTransfer()
// ---------------------------------------------------------------------------

#[test]
fn vested_transfer_succeeds() {
	new_test_ext().execute_with(|| {
		let locked: VestingBalance<Test> = 10_000;
		CurrencyOf::<Test>::make_free_balance_be(&ALICE, locked * 10);

		let input = IVesting::IVestingCalls::vestedTransfer(IVesting::vestedTransferCall {
			target: target_alloy(),
			locked: alloy_core::primitives::U256::from(locked),
			perBlock: alloy_core::primitives::U256::from(500u64),
			startingBlock: alloy_core::primitives::U256::from(0u64),
		});
		let result = bare_call(&input).build_and_unwrap_result();
		assert!(!result.did_revert());
		assert!(result.data.is_empty());

		// Verify the schedule was created with the correct locked amount.
		// At block 1 with per_block=500, one block vested: 10_000 - 500 = 9_500.
		let balance =
			<pallet_vesting::Pallet<Test> as VestingSchedule<u64>>::vesting_balance(&TARGET);
		assert_eq!(balance, Some(9_500), "locked amount should match the schedule");
	});
}

// ---------------------------------------------------------------------------
// Read-only & delegate-call guards (test vectors)
// ---------------------------------------------------------------------------

struct GuardTestCase {
	name: &'static str,
	input: IVesting::IVestingCalls,
	reject_read_only: bool,
	reject_delegate: bool,
}

fn guard_test_cases() -> Vec<GuardTestCase> {
	vec![
		GuardTestCase {
			name: "vest",
			input: IVesting::IVestingCalls::vest(IVesting::vestCall {}),
			reject_read_only: true,
			reject_delegate: true,
		},
		GuardTestCase {
			name: "vestOther",
			input: IVesting::IVestingCalls::vestOther(IVesting::vestOtherCall {
				target: target_alloy(),
			}),
			reject_read_only: true,
			reject_delegate: true,
		},
		GuardTestCase {
			name: "vestedTransfer",
			input: IVesting::IVestingCalls::vestedTransfer(IVesting::vestedTransferCall {
				target: target_alloy(),
				locked: alloy_core::primitives::U256::from(10_000u64),
				perBlock: alloy_core::primitives::U256::from(500u64),
				startingBlock: alloy_core::primitives::U256::from(0u64),
			}),
			reject_read_only: true,
			reject_delegate: true,
		},
		GuardTestCase {
			name: "vestingBalance",
			input: IVesting::IVestingCalls::vestingBalance(IVesting::vestingBalanceCall {}),
			reject_read_only: false,
			reject_delegate: false,
		},
		GuardTestCase {
			name: "vestingBalanceOf",
			input: IVesting::IVestingCalls::vestingBalanceOf(IVesting::vestingBalanceOfCall {
				target: target_alloy(),
			}),
			reject_read_only: false,
			reject_delegate: false,
		},
	]
}

/// Deploys the fixture `name` compiled with `fixture_type`, passing `constructor` as its
/// constructor data.
fn deploy(fixture_type: FixtureType, name: &str, constructor: Vec<u8>) -> Contract<Test> {
	// ALICE pays the storage deposit.
	CurrencyOf::<Test>::make_free_balance_be(&ALICE, 1_000_000_000_000_000);
	let (code, _) = compile_module_with_type(name, fixture_type).unwrap();
	BareInstantiateBuilder::<Test>::bare_instantiate(
		RuntimeOrigin::signed(ALICE),
		Code::Upload(code),
	)
	.constructor_data(constructor)
	.transaction_limits(TransactionLimits::WeightAndDeposit {
		weight_limit: Weight::MAX,
		deposit_limit: u64::MAX,
	})
	.build_and_unwrap_contract()
}

/// Calls the precompile with `input` through `caller`, as a static call if `static_call` is set and
/// as a delegate call otherwise, and returns the error of the precompile's frame.
fn static_or_delegate_call_precompile(
	caller: H160,
	input: &IVesting::IVestingCalls,
	static_call: bool,
) -> Option<String> {
	let callee = precompile_address().0.into();
	let data = input.abi_encode().into();
	let call = if static_call {
		Caller::staticCallCall { _callee: callee, _data: data, _gas: u64::MAX }.abi_encode()
	} else {
		Caller::delegateCall { _callee: callee, _data: data, _gas: u64::MAX }.abi_encode()
	};
	let mut tracer = CallTracer::new(Default::default());
	trace(&mut tracer, || {
		BareCallBuilder::<Test>::bare_call(RuntimeOrigin::signed(ALICE), caller)
			.data(call)
			.build_and_unwrap_result()
	});
	tracer.collect_trace().unwrap().calls.pop().unwrap().error
}

fn assert_guard(
	caller: H160,
	case: &GuardTestCase,
	read_only: bool,
	should_reject: bool,
	expected_error: &pallet_revive::Error<Test>,
) {
	let context = if read_only { "read-only" } else { "delegate call" };
	assert_eq!(
		static_or_delegate_call_precompile(caller, &case.input, read_only),
		should_reject.then(|| expected_error.as_str().to_string()),
		"{}: wrong error reported for {context}",
		case.name
	);
}

#[test_case(FixtureType::Solc; "solc")]
#[test_case(FixtureType::Resolc; "resolc")]
fn read_only_guards(fixture_type: FixtureType) {
	new_test_ext().execute_with(|| {
		let caller = deploy(fixture_type, "Caller", Vec::new()).addr;
		let expected_error = pallet_revive::Error::<Test>::StateChangeDenied;
		for case in guard_test_cases() {
			assert_guard(caller, &case, true, case.reject_read_only, &expected_error);
		}
	});
}

#[test_case(FixtureType::Solc; "solc")]
#[test_case(FixtureType::Resolc; "resolc")]
fn delegate_call_guards(fixture_type: FixtureType) {
	new_test_ext().execute_with(|| {
		let caller = deploy(fixture_type, "Caller", Vec::new()).addr;
		let expected_error = pallet_revive::Error::<Test>::PrecompileDelegateDenied;
		for case in guard_test_cases() {
			assert_guard(caller, &case, false, case.reject_delegate, &expected_error);
		}
	});
}

// ---------------------------------------------------------------------------
// Stipend-limited frames
// ---------------------------------------------------------------------------

mod stipend_limited_frame {
	use super::*;
	use alloy_core::sol_types::SolConstructor;
	use pallet_revive::test_utils::with_parameter;
	use pallet_revive_fixtures::{CallingReceiver, StipendSender};
	use test_case::test_case;

	const SCHEDULE_AMOUNT: u64 = 10_000;

	struct StipendTestCase {
		name: &'static str,
		input: IVesting::IVestingCalls,
		changes_state: bool,
		/// Creates the state the call needs to succeed, given the receiver's account.
		setup: fn(u64),
		/// Reads the state the call would change, given the receiver's account.
		read_state: fn(u64) -> bool,
	}

	#[derive(Clone, Copy, Debug, PartialEq)]
	struct StipendTestCaseOutcome {
		denied: bool,
		state_changed: bool,
	}

	fn stipend_test_cases() -> Vec<StipendTestCase> {
		vec![
			StipendTestCase {
				name: "vest",
				input: IVesting::IVestingCalls::vest(IVesting::vestCall {}),
				changes_state: true,
				setup: add_fully_vested_schedule,
				read_state: |receiver| pallet_vesting::Vesting::<Test>::contains_key(receiver),
			},
			StipendTestCase {
				name: "vestOther",
				input: IVesting::IVestingCalls::vestOther(IVesting::vestOtherCall {
					target: target_alloy(),
				}),
				changes_state: true,
				setup: |_| add_fully_vested_schedule(TARGET),
				read_state: |_| pallet_vesting::Vesting::<Test>::contains_key(TARGET),
			},
			StipendTestCase {
				name: "vestedTransfer",
				input: IVesting::IVestingCalls::vestedTransfer(IVesting::vestedTransferCall {
					target: target_alloy(),
					locked: alloy_core::primitives::U256::from(SCHEDULE_AMOUNT),
					perBlock: alloy_core::primitives::U256::from(500u64),
					startingBlock: alloy_core::primitives::U256::from(0u64),
				}),
				changes_state: true,
				setup: |receiver| {
					CurrencyOf::<Test>::make_free_balance_be(&receiver, SCHEDULE_AMOUNT * 10);
				},
				read_state: |_| pallet_vesting::Vesting::<Test>::contains_key(TARGET),
			},
			StipendTestCase {
				name: "vestingBalance",
				input: IVesting::IVestingCalls::vestingBalance(IVesting::vestingBalanceCall {}),
				changes_state: false,
				setup: |_| {},
				read_state: |_| false,
			},
			StipendTestCase {
				name: "vestingBalanceOf",
				input: IVesting::IVestingCalls::vestingBalanceOf(IVesting::vestingBalanceOfCall {
					target: target_alloy(),
				}),
				changes_state: false,
				setup: |_| {},
				read_state: |_| false,
			},
		]
	}

	/// Makes `vesting_call` from a `CallingReceiver` reached with `gas_limit`, after its `setup`,
	/// and returns whether it was denied and whether it changed what `read_state` returns.
	fn call_precompile_via_receiver(
		fixture_type: FixtureType,
		gas_limit: u64,
		vesting_call: &StipendTestCase,
	) -> StipendTestCaseOutcome {
		let StipendTestCase { input, setup, read_state, .. } = vesting_call;
		new_test_ext().execute_with(|| {
			let receiver = deploy(
				fixture_type,
				"CallingReceiver",
				CallingReceiver::constructorCall {
					_target: precompile_address().0.into(),
					_data: input.abi_encode().into(),
				}
				.abi_encode(),
			);
			let sender = deploy(
				fixture_type,
				"StipendSender",
				StipendSender::constructorCall { _receiver: receiver.addr.0.into() }.abi_encode(),
			);
			setup(receiver.account_id);
			let state_before = read_state(receiver.account_id);
			let result =
				BareCallBuilder::<Test>::bare_call(RuntimeOrigin::signed(ALICE), sender.addr)
					.data(
						StipendSender::isCallWithGasDeniedCall { gasLimit: gas_limit }.abi_encode(),
					)
					.build_and_unwrap_result();
			StipendTestCaseOutcome {
				denied: bool::abi_decode(&result.data).unwrap(),
				state_changed: read_state(receiver.account_id) != state_before,
			}
		})
	}

	/// Gives `who` the vested funds and a schedule that has fully vested, so vesting removes it.
	fn add_fully_vested_schedule(who: u64) {
		CurrencyOf::<Test>::make_free_balance_be(&who, SCHEDULE_AMOUNT);
		let start = frame_system::Pallet::<Test>::block_number();
		// `per_block` equals the amount, so the schedule is fully vested one block after `start`.
		<pallet_vesting::Pallet<Test> as VestingSchedule<u64>>::add_vesting_schedule(
			&who,
			SCHEDULE_AMOUNT,
			SCHEDULE_AMOUNT,
			start,
		)
		.unwrap();
		frame_system::Pallet::<Test>::set_block_number(start + 1);
	}

	#[test_case(FixtureType::Solc; "solc")]
	#[test_case(FixtureType::Resolc; "resolc")]
	fn stipend_limited_guards(fixture_type: FixtureType) {
		// High enough that 2300 gas pays for the operation under test, so only the stipend check
		// can deny it.
		const RAISED_GAS_SCALE: u32 = 50_000_000;
		for vesting_call in stipend_test_cases() {
			let [with_2300_gas, with_2299_gas, with_2301_gas] =
				with_parameter(GasScale::get, GasScale::set, RAISED_GAS_SCALE, || {
					[2300, 2299, 2301].map(|gas_limit| {
						call_precompile_via_receiver(fixture_type, gas_limit, &vesting_call)
					})
				});
			assert_eq!(
				with_2300_gas,
				StipendTestCaseOutcome { denied: vesting_call.changes_state, state_changed: false },
				"{}: wrong outcome for the receiver of a `send`",
				vesting_call.name
			);
			assert_eq!(
				[with_2299_gas, with_2301_gas],
				[StipendTestCaseOutcome {
					denied: false,
					state_changed: vesting_call.changes_state,
				}; 2],
				"{}: wrong outcome for a call that is not a `send`",
				vesting_call.name
			);
		}
	}
}
