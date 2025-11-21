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

use crate::{
	address::AddressMapper,
	test_utils::{builder::Contract, ALICE, DJANGO, DJANGO_ADDR},
	tests::{
		builder,
		test_utils::{get_balance, get_contract_checked},
		Contracts, ExtBuilder, RuntimeOrigin, Test,
	},
	BalanceOf, Code, Config, Pallet, H160,
};
use alloy_core::sol_types::{SolCall, SolConstructor};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{
	compile_module_with_type, FixtureType, Terminate, TerminateCaller, TerminateDelegator,
};
use pretty_assertions::assert_eq;
use test_case::{test_case, test_matrix};

/// Decode a contract return value into an error string.
fn decode_error(output: &[u8]) -> String {
	use alloy_core::sol_types::SolError;
	alloy_core::sol! { error Error(string); }
	Error::abi_decode_validate(output).unwrap().0
}

const METHOD_PRECOMPILE: u8 = 0;
const METHOD_DELEGATE_CALL: u8 = 1;
const METHOD_SYSCALL: u8 = 2;

#[test_matrix(
	[FixtureType::Solc, FixtureType::Resolc],
	[METHOD_PRECOMPILE, METHOD_SYSCALL]
)]
fn base_case(fixture_type: FixtureType, method: u8) {
	let (code, _) = compile_module_with_type("Terminate", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(
				Terminate::constructorCall {
					skip: true,
					method,
					beneficiary: DJANGO_ADDR.0.into(),
				}
				.abi_encode(),
			)
			.build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(
				Terminate::terminateCall { method, beneficiary: DJANGO_ADDR.0.into() }.abi_encode(),
			)
			.build_and_unwrap_result();

		assert!(result.data.is_empty());
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn precompile_fails_in_constructor(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Terminate", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let result = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(
				Terminate::constructorCall {
					skip: false,
					method: METHOD_PRECOMPILE,
					beneficiary: DJANGO_ADDR.0.into(),
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();

		assert!(result.result.did_revert());
		assert_eq!(
			decode_error(result.result.data.as_ref()),
			"terminate pre-compile cannot be called from the constructor"
		);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn syscall_passes_in_constructor(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Terminate", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let result = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(
				Terminate::constructorCall {
					skip: false,
					method: METHOD_SYSCALL,
					beneficiary: DJANGO_ADDR.0.into(),
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();

		assert!(!result.result.did_revert());
		assert!(result.result.data.is_empty());
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn precompile_fails_for_direct_delegate(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Terminate", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(
				Terminate::constructorCall {
					skip: true,
					method: METHOD_PRECOMPILE,
					beneficiary: DJANGO_ADDR.0.into(),
				}
				.abi_encode(),
			)
			.build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(
				Terminate::terminateCall {
					method: METHOD_DELEGATE_CALL,
					beneficiary: DJANGO_ADDR.0.into(),
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();

		assert!(result.did_revert());
		assert_eq!(
			decode_error(result.data.as_ref()),
			"illegal to call this pre-compile via delegate call",
		);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn precompile_fails_for_indirect_delegate(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Terminate", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(
				Terminate::constructorCall {
					skip: true,
					method: METHOD_PRECOMPILE,
					beneficiary: DJANGO_ADDR.0.into(),
				}
				.abi_encode(),
			)
			.build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(
				Terminate::indirectDelegateTerminateCall { beneficiary: DJANGO_ADDR.0.into() }
					.abi_encode(),
			)
			.build_and_unwrap_result();

		assert!(result.did_revert());
		assert_eq!(
			decode_error(result.data.as_ref()),
			"illegal to call this pre-compile via delegate call",
		);
	});
}

/// In this test TerminateDelegator terminates itself by making a delegatecall to Terminate.
/// The SYSCALL terminate method shall work in this case because TerminateDelegator is created and
/// terminated in the same tx.
#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn syscall_passes_for_direct_delegate_same_tx(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Terminate", fixture_type).unwrap();
	let (caller_code, _) = compile_module_with_type("TerminateCaller", fixture_type).unwrap();
	let (delegator_code, _) = compile_module_with_type("TerminateDelegator", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		if fixture_type == FixtureType::Resolc {
			// Need to pre-upload code for PVM
			let _ = <Pallet<Test>>::upload_code(
				RuntimeOrigin::signed(ALICE.clone()),
				code.clone(),
				<BalanceOf<Test>>::MAX,
			);
			let _ = <Pallet<Test>>::upload_code(
				RuntimeOrigin::signed(ALICE.clone()),
				delegator_code.clone(),
				<BalanceOf<Test>>::MAX,
			);
		}

		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code))
				.native_value(125)
				.build_and_unwrap_contract();

		let result = builder::bare_call(caller_addr)
			.data(
				TerminateCaller::delegateCallTerminateCall {
					value: alloy_core::primitives::U256::from(123_000_000u64),
					method: METHOD_SYSCALL,
					beneficiary: DJANGO_ADDR.0.into(),
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();
		assert!(!result.did_revert());

		let decoded =
			TerminateCaller::delegateCallTerminateCall::abi_decode_returns(&result.data).unwrap();
		let addr = H160::from_slice(decoded._1.as_slice());
		let delegator_addr = H160::from_slice(decoded._0.as_slice());

		assert_eq!(
			get_balance(&DJANGO),
			123 + min_balance,
			"unexpected django balance after terminate"
		);
		assert!(
			get_contract_checked(&addr).is_some(),
			"Terminate contract should still exist after terminate"
		);
		assert!(
			get_contract_checked(&delegator_addr).is_none(),
			"TerminateDelegator contract should not exist after terminate"
		);
	});
}

/// In this test TerminateDelegator terminates itself by making a delegatecall to Terminate.
/// The SYSCALL shall send funds from TerminateDelegator to the beneficiary but TerminateDelegator
/// shall not be truly terminated.
#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn syscall_passes_for_direct_delegate(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Terminate", fixture_type).unwrap();
	let (delegator_code, _) = compile_module_with_type("TerminateDelegator", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code.clone()))
			.constructor_data(
				Terminate::constructorCall {
					skip: true,
					method: METHOD_SYSCALL,
					beneficiary: DJANGO_ADDR.0.into(),
				}
				.abi_encode(),
			)
			.build_and_unwrap_contract();
		let Contract { addr: delegator_addr, .. } =
			builder::bare_instantiate(Code::Upload(delegator_code.clone()))
				.native_value(123)
				.build_and_unwrap_contract();
		let account_delegator = <Test as Config>::AddressMapper::to_account_id(&delegator_addr);

		let result = builder::bare_call(delegator_addr)
			.data(
				TerminateDelegator::delegateCallTerminateCall {
					terminate_addr: addr.0.into(),
					method: METHOD_SYSCALL,
					beneficiary: DJANGO_ADDR.0.into(),
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();
		assert!(!result.did_revert());

		assert_eq!(
			get_balance(&DJANGO),
			123 + min_balance,
			"unexpected django balance after terminate"
		);
		assert_eq!(
			get_balance(&account_delegator),
			min_balance,
			"unexpected delegator balance after terminate"
		);
		assert!(
			get_contract_checked(&addr).is_some(),
			"Terminate contract should still exist after terminate"
		);
		assert!(
			get_contract_checked(&delegator_addr).is_some(),
			"TerminateDelegator contract should still exist after terminate"
		);
	});
}

#[test_matrix(
	[FixtureType::Solc, FixtureType::Resolc],
	[FixtureType::Solc, FixtureType::Resolc]
)]
fn terminate_shall_rollback_if_subsequent_frame_fails(
	caller_type: FixtureType,
	callee_type: FixtureType,
) {
	let (code, _) = compile_module_with_type("Terminate", callee_type).unwrap();
	let (caller_code, _) = compile_module_with_type("TerminateCaller", caller_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(
				Terminate::constructorCall {
					skip: true,
					method: METHOD_PRECOMPILE,
					beneficiary: DJANGO_ADDR.0.into(),
				}
				.abi_encode(),
			)
			.build_and_unwrap_contract();
		let account = <Test as Config>::AddressMapper::to_account_id(&addr);

		assert!(get_contract_checked(&addr).is_some(), "contract does not exist after create");
		assert_eq!(get_balance(&account), min_balance, "unexpected contract balance after create");

		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code))
				.native_value(125)
				.build_and_unwrap_contract();
		let caller_account = <Test as Config>::AddressMapper::to_account_id(&caller_addr);

		let result = builder::bare_call(caller_addr)
			.data(
				TerminateCaller::revertAfterTerminateCall {
					terminate_addr: addr.0.into(),
					method: METHOD_PRECOMPILE,
					beneficiary: DJANGO_ADDR.0.into(),
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();

		assert!(result.did_revert(), "revertAfterTerminateCall did not revert");
		assert!(
			get_contract_checked(&addr).is_some(),
			"contract does not exist after reverted terminate"
		);
		assert_eq!(
			get_balance(&account),
			min_balance,
			"unexpected contract balance after reverted terminate"
		);

		assert_eq!(get_balance(&DJANGO), 0, "unexpected DJANGO balance after reverted terminate");

		assert_eq!(
			get_balance(&caller_account),
			125 + min_balance,
			"unexpected caller balance after reverted terminate"
		);
	});
}

/// This test does the following in the same transaction:
/// 1. deploy Terminate contract
/// 2. terminate the Terminate contract
/// 3. send funds to the Terminate contract
/// The funds that were sent after termination shall be credited to the beneficiary.
/// Deploying an EVM contract from a PVM contract (or vice versa) is not supported.
#[test_matrix(
	[FixtureType::Solc, FixtureType::Resolc],
	[METHOD_PRECOMPILE, METHOD_SYSCALL]
)]
fn sent_funds_after_terminate_shall_be_credited_to_beneficiary_base_case(
	fixture_type: FixtureType,
	method: u8,
) {
	let (code, _) = compile_module_with_type("Terminate", fixture_type).unwrap();
	let (caller_code, _) = compile_module_with_type("TerminateCaller", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		if fixture_type == FixtureType::Resolc {
			// Need to pre-upload code for PVM
			let _ = <Pallet<Test>>::upload_code(
				RuntimeOrigin::signed(ALICE.clone()),
				code.clone(),
				<BalanceOf<Test>>::MAX,
			);
		}
		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code))
				.native_value(125)
				.build_and_unwrap_contract();
		let caller_account = <Test as Config>::AddressMapper::to_account_id(&caller_addr);

		assert_eq!(
			get_balance(&caller_account),
			125 + min_balance,
			"unexpected caller balance before terminate"
		);

		let result = builder::bare_call(caller_addr)
			.data(
				TerminateCaller::sendFundsAfterTerminateAndCreateCall {
					value: alloy_core::primitives::U256::from(123_000_000u64),
					method,
					beneficiary: DJANGO_ADDR.0.into(),
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();

		assert!(
			!result.did_revert(),
			"sendFundsAfterTerminateAndCreateCall reverted: {}",
			decode_error(&result.data)
		);
		let decoded =
			TerminateCaller::sendFundsAfterTerminateAndCreateCall::abi_decode_returns(&result.data)
				.unwrap();
		let addr = H160::from_slice(decoded.0.as_slice());
		assert!(get_contract_checked(&addr).is_none(), "contract still exists after terminate");
		assert_eq!(
			get_balance(&DJANGO),
			123 + min_balance,
			"unexpected DJANGO balance after terminate"
		);
		let account = <Test as Config>::AddressMapper::to_account_id(&addr);
		assert_eq!(get_balance(&account), 0, "ucontract has balance after terminate");
	});
}

/// This test does *not* create and terminate the Terminate contract in the same transaction.
/// Therefore, the SYSCALL terminate method does not be transferred to beneficiary.
#[test_matrix(
	[FixtureType::Solc, FixtureType::Resolc],
	[FixtureType::Solc, FixtureType::Resolc]
)]
fn sent_funds_after_terminate_shall_be_credited_to_beneficiary_precompile(
	caller_type: FixtureType,
	callee_type: FixtureType,
) {
	let (code, _) = compile_module_with_type("Terminate", callee_type).unwrap();
	let (caller_code, _) = compile_module_with_type("TerminateCaller", caller_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(
				Terminate::constructorCall {
					skip: true,
					method: METHOD_PRECOMPILE,
					beneficiary: DJANGO_ADDR.0.into(),
				}
				.abi_encode(),
			)
			.build_and_unwrap_contract();
		let account = <Test as Config>::AddressMapper::to_account_id(&addr);

		assert!(get_contract_checked(&addr).is_some(), "contract does not exist after create");
		assert_eq!(get_balance(&account), min_balance, "unexpected contract balance after create");

		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code))
				.native_value(125)
				.build_and_unwrap_contract();
		let caller_account = <Test as Config>::AddressMapper::to_account_id(&caller_addr);

		assert_eq!(
			get_balance(&caller_account),
			125 + min_balance,
			"unexpected caller balance before terminate"
		);

		let result = builder::bare_call(caller_addr)
			.data(
				TerminateCaller::sendFundsAfterTerminateCall {
					terminate_addr: addr.0.into(),
					value: alloy_core::primitives::U256::from(123_000_000u64),
					method: METHOD_PRECOMPILE,
					beneficiary: DJANGO_ADDR.0.into(),
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();

		assert!(
			!result.did_revert(),
			"sendFundsAfterTerminateCall reverted: {}",
			decode_error(&result.data)
		);
		assert!(
			result.data.is_empty(),
			"sendFundsAfterTerminateCall returned unexpected data: {:?}",
			result.data
		);
		assert!(get_contract_checked(&addr).is_none(), "contract still exists after terminate");
		assert_eq!(
			get_balance(&DJANGO),
			123 + min_balance,
			"unexpected DJANGO balance after terminate"
		);
		assert_eq!(get_balance(&account), 0, "contract has balance after terminate");
	});
}

/// This test does *not* create and terminate the Terminate contract in the same transaction.
/// Therefore, the SYSCALL terminate method does not be transferred to beneficiary.
#[test_matrix(
	[FixtureType::Solc, FixtureType::Resolc],
	[FixtureType::Solc, FixtureType::Resolc]
)]
fn sent_funds_after_terminate_shall_not_be_credited_to_beneficiary_syscall(
	caller_type: FixtureType,
	callee_type: FixtureType,
) {
	let (code, _) = compile_module_with_type("Terminate", callee_type).unwrap();
	let (caller_code, _) = compile_module_with_type("TerminateCaller", caller_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(
				Terminate::constructorCall {
					skip: true,
					method: METHOD_PRECOMPILE,
					beneficiary: DJANGO_ADDR.0.into(),
				}
				.abi_encode(),
			)
			.build_and_unwrap_contract();
		let account = <Test as Config>::AddressMapper::to_account_id(&addr);

		assert!(get_contract_checked(&addr).is_some(), "contract does not exist after create");
		assert_eq!(get_balance(&account), min_balance, "unexpected contract balance after create");

		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code))
				.native_value(125)
				.build_and_unwrap_contract();
		let caller_account = <Test as Config>::AddressMapper::to_account_id(&caller_addr);

		assert_eq!(
			get_balance(&caller_account),
			125 + min_balance,
			"unexpected caller balance before terminate"
		);

		let result = builder::bare_call(caller_addr)
			.data(
				TerminateCaller::sendFundsAfterTerminateCall {
					terminate_addr: addr.0.into(),
					value: alloy_core::primitives::U256::from(123_000_000u64),
					method: METHOD_SYSCALL,
					beneficiary: DJANGO_ADDR.0.into(),
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();

		assert!(
			!result.did_revert(),
			"sendFundsAfterTerminateCall reverted: {}",
			decode_error(&result.data)
		);
		assert!(
			result.data.is_empty(),
			"sendFundsAfterTerminateCall returned unexpected data: {:?}",
			result.data
		);
		assert!(get_contract_checked(&addr).is_some(), "contract does not exist after terminate");
		assert_eq!(get_balance(&DJANGO), 0, "unexpected DJANGO balance after terminate");
		assert_eq!(
			get_balance(&account),
			123 + min_balance,
			"unexpected contract balance after terminate"
		);
	});
}

#[test_matrix(
	[FixtureType::Solc, FixtureType::Resolc],
	[METHOD_SYSCALL, METHOD_PRECOMPILE],
	[METHOD_SYSCALL, METHOD_PRECOMPILE]
)]
fn terminate_twice(fixture_type: FixtureType, method1: u8, method2: u8) {
	let (code, _) = compile_module_with_type("Terminate", fixture_type).unwrap();
	let (caller_code, _) = compile_module_with_type("TerminateCaller", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let min_balance = Contracts::min_balance();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		if fixture_type == FixtureType::Resolc {
			// Need to pre-upload code for PVM
			let _ = <Pallet<Test>>::upload_code(
				RuntimeOrigin::signed(ALICE.clone()),
				code.clone(),
				<BalanceOf<Test>>::MAX,
			);
		}
		let Contract { addr: caller_addr, .. } =
			builder::bare_instantiate(Code::Upload(caller_code))
				.native_value(125)
				.build_and_unwrap_contract();
		let result = builder::bare_call(caller_addr)
			.data(
				TerminateCaller::createAndTerminateTwiceCall {
					value: alloy_core::primitives::U256::from(123_000_000u64),
					method1,
					method2,
					beneficiary: DJANGO_ADDR.0.into(),
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();
		assert!(
			!result.did_revert(),
			"createAndTerminateTwiceCall reverted: {}",
			decode_error(&result.data)
		);

		let decoded =
			TerminateCaller::createAndTerminateTwiceCall::abi_decode_returns(&result.data).unwrap();
		let addr = H160::from_slice(decoded.0.as_slice());
		let account = <Test as Config>::AddressMapper::to_account_id(&addr);
		assert!(get_contract_checked(&addr).is_none(), "contract still exists after terminate");
		assert_eq!(get_balance(&account), 0, "unexpected contract balance after terminate");
		assert_eq!(
			get_balance(&DJANGO),
			123 + min_balance,
			"unexpected DJANGO balance after terminate"
		);
	});
}
