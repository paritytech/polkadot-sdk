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

//! Tests for the `burn` function of the `System` pre-compile.

use crate::{
	AccountInfo, BalanceWithDust, Code, Config, H160, HoldReason, Pallet, U256,
	address::AddressMapper,
	test_utils::{ALICE, ALICE_ADDR, builder::Contract},
	tests::{
		Balances, BurnDestination, ExtBuilder, RuntimeEvent, RuntimeOrigin, System, Test, builder,
		pallet_dummy,
		test_utils::{get_balance, get_balance_on_hold, set_balance_with_dust},
	},
};
use alloy_core::{
	primitives::U256 as AlloyU256,
	sol_types::{SolCall, SolConstructor, SolInterface},
};
use frame_support::{
	assert_err, assert_ok,
	traits::fungible::{Inspect, Mutate, MutateFreeze},
};
use pallet_revive_fixtures::{Burn, FixtureType, compile_module_with_type};
use pallet_revive_uapi::{SYSTEM_PRECOMPILE_ADDR, precompiles::system::ISystem};
use pretty_assertions::assert_eq;
use sp_runtime::DispatchError;
use test_case::test_case;

/// Decode a contract return value into an error string.
fn decode_error(output: &[u8]) -> String {
	use alloy_core::sol_types::SolError;
	alloy_core::sol! { error Error(string); }
	Error::abi_decode_validate(output).unwrap().0
}

fn evm(value: u128, dust: u32) -> U256 {
	Pallet::<Test>::convert_native_to_evm(BalanceWithDust::new_unchecked::<Test>(value, dust))
}

fn alloy(value: U256) -> AlloyU256 {
	AlloyU256::from_be_bytes(value.to_big_endian())
}

fn total_issuance() -> u128 {
	<Test as Config>::Currency::total_issuance()
}

/// The amounts the native currency reported as burned, in order.
fn burned_events() -> Vec<u128> {
	System::events()
		.into_iter()
		.filter_map(|record| match record.event {
			RuntimeEvent::Balances(pallet_balances::Event::Burned { amount, .. }) => Some(amount),
			_ => None,
		})
		.collect()
}

fn contract_logs() -> usize {
	System::events()
		.into_iter()
		.filter(|record| {
			matches!(record.event, RuntimeEvent::Contracts(crate::Event::ContractEmitted { .. }))
		})
		.count()
}

/// Deploy the fixture and give it `value` and `dust` on top of the existential deposit.
fn deploy(fixture_type: FixtureType, value: u128, dust: u32) -> H160 {
	deploy_with_salt(fixture_type, value, dust, 0)
}

fn deploy_with_salt(fixture_type: FixtureType, value: u128, dust: u32, salt: u8) -> H160 {
	let (code, _) = compile_module_with_type("Burn", fixture_type).unwrap();
	let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
	let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
		.constructor_data(Burn::constructorCall { amount: AlloyU256::ZERO }.abi_encode())
		.salt(Some([salt; 32]))
		.build_and_unwrap_contract();
	set_balance_with_dust(&addr, BalanceWithDust::new_unchecked::<Test>(value, dust));
	System::reset_events();
	addr
}

#[test_case(FixtureType::Solc, 0; "solc without dust")]
#[test_case(FixtureType::Solc, 10; "solc with dust")]
#[test_case(FixtureType::Resolc, 0; "resolc without dust")]
#[test_case(FixtureType::Resolc, 10; "resolc with dust")]
fn received_value_is_burned(fixture_type: FixtureType, dust: u32) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 1_000, 3);
		let balance = Pallet::<Test>::evm_balance(&addr);
		let burn_destination = get_balance(&BurnDestination::get());
		let issuance = total_issuance();

		let result = builder::bare_call(addr)
			.evm_value(evm(5, dust))
			.data(Burn::buyCall {}.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert());

		// The contract is left with what it had before the call.
		assert_eq!(Pallet::<Test>::evm_balance(&addr), balance);
		assert_eq!(get_balance(&BurnDestination::get()), burn_destination);
		// Sending dust converts one unit of the sender's balance into dust, which is a burn of
		// its own. The contract already holds the dust it burns.
		let converted = if dust == 0 { 0 } else { 1 };
		assert_eq!(issuance - total_issuance(), 5 + converted);
		assert_eq!(burned_events().last(), Some(&5));
		assert_eq!(contract_logs(), 0);

		let sold = builder::bare_call(addr)
			.data(Burn::soldCall {}.abi_encode())
			.build_and_unwrap_result();
		assert_eq!(Burn::soldCall::abi_decode_returns(&sold.data).unwrap(), AlloyU256::from(3));
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn whole_balance_is_burned_except_existential_deposit(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 1_000, 7);
		let account = <Test as Config>::AddressMapper::to_account_id(&addr);
		let storage_hold = HoldReason::StorageDepositReserve.into();
		let held = get_balance_on_hold(&storage_hold, &account);
		assert!(held > 0);
		let issuance = total_issuance();

		let result = builder::bare_call(addr)
			.data(Burn::burnAllCall {}.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert());

		assert_eq!(Pallet::<Test>::evm_balance(&addr), U256::zero());
		assert_eq!(get_balance(&account), <Test as Config>::Currency::minimum_balance());
		// The storage deposit is held, not free, and stays out of reach.
		assert_eq!(get_balance_on_hold(&storage_hold, &account), held);
		// Dust is not part of the issuance.
		assert_eq!(issuance - total_issuance(), 1_000);
		assert_eq!(burned_events(), vec![1_000]);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn missing_dust_is_converted_from_balance(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 1_000, 0);
		let issuance = total_issuance();

		let result = builder::bare_call(addr)
			.data(Burn::burnCall { amount: alloy(evm(5, 10)) }.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert());

		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm(1_000, 0) - evm(5, 10));
		assert_eq!(issuance - total_issuance(), 6);
		assert_eq!(burned_events(), vec![1, 5]);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn only_dust_is_burned(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 1_000, 7);
		let issuance = total_issuance();

		let result = builder::bare_call(addr)
			.data(Burn::burnCall { amount: alloy(evm(0, 4)) }.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert());

		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm(1_000, 3));
		assert_eq!(issuance, total_issuance());
		assert_eq!(burned_events(), Vec::<u128>::new());
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn zero_is_a_noop(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 1_000, 7);
		let issuance = total_issuance();

		let result = builder::bare_call(addr)
			.data(Burn::burnCall { amount: AlloyU256::ZERO }.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert());

		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm(1_000, 7));
		assert_eq!(issuance, total_issuance());
		assert_eq!(burned_events(), Vec::<u128>::new());
	});
}

/// The dust a contract keeps after a burn in its constructor has to survive the construction.
/// Receiving dust along with the deployment creates the account info of the contract before the
/// constructor runs. Without it, the burn creates the account info.
#[test_case(FixtureType::Solc, 10; "solc with dust")]
#[test_case(FixtureType::Solc, 0; "solc without dust")]
#[test_case(FixtureType::Resolc, 10; "resolc with dust")]
#[test_case(FixtureType::Resolc, 0; "resolc without dust")]
fn constructor_burns_received_value(fixture_type: FixtureType, dust: u32) {
	ExtBuilder::default().build().execute_with(|| {
		let (code, _) = compile_module_with_type("Burn", fixture_type).unwrap();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		System::reset_events();

		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(Burn::constructorCall { amount: alloy(evm(2, 3)) }.abi_encode())
			.evm_value(evm(5, dust))
			.build_and_unwrap_contract();

		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm(5, dust) - evm(2, 3));
		// One unit is converted into dust before the burn: by the deployer when sending dust,
		// otherwise by the contract.
		assert_eq!(burned_events(), vec![1, 2]);

		let result = builder::bare_call(addr)
			.data(Burn::burnAllCall {}.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert());
		assert_eq!(Pallet::<Test>::evm_balance(&addr), U256::zero());
	});
}

/// An account delegating to the fixture burns from its own balance.
#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn delegated_account_burns_its_own_balance(fixture_type: FixtureType) {
	use crate::tests::eip7702::DelegationTestSetup;

	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 1_000, 7);
		let setup = DelegationTestSetup::default();
		setup.authorize(addr);
		let delegator = setup.signer.address;
		let balance = Pallet::<Test>::evm_balance(&delegator);
		let issuance = total_issuance();

		let result = builder::bare_call(delegator)
			.data(Burn::burnCall { amount: alloy(evm(5, 10)) }.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert());

		assert_eq!(Pallet::<Test>::evm_balance(&delegator), balance - evm(5, 10));
		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm(1_000, 7));
		assert_eq!(issuance - total_issuance(), 6);
		assert_eq!(AccountInfo::<Test>::get_delegation_target(&delegator), Some(addr));
	});
}

/// Call one of the `try*` functions and return what the pre-compile answered.
fn try_call(addr: H160, data: Vec<u8>) -> (bool, Vec<u8>) {
	let result = builder::bare_call(addr).data(data).build_and_unwrap_result();
	assert!(!result.did_revert());
	let returned = Burn::tryBurnCall::abi_decode_returns(&result.data).unwrap();
	(returned.success, returned.output.to_vec())
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn more_than_the_balance_reverts(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 1_000, 7);
		let issuance = total_issuance();

		// One unit of dust more than the contract can spend. The existential deposit does not
		// count.
		for amount in [evm(1_000, 8), evm(1_001, 0), evm(u128::MAX, 0)] {
			let (success, output) =
				try_call(addr, Burn::tryBurnCall { amount: alloy(amount) }.abi_encode());
			assert!(!success);
			assert_eq!(decode_error(&output), "burn: insufficient balance");
		}

		// An amount that does not fit the balance type.
		let (success, output) =
			try_call(addr, Burn::tryBurnCall { amount: AlloyU256::MAX }.abi_encode());
		assert!(!success);
		assert_eq!(decode_error(&output), "burn: amount out of range");

		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm(1_000, 7));
		assert_eq!(issuance, total_issuance());
		assert_eq!(burned_events(), Vec::<u128>::new());
	});
}

/// The unit that is converted into dust has to be spendable as well.
#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn dust_conversion_keeps_existential_deposit(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 1, 0);

		// The only spendable unit cannot be both burned and converted into dust.
		let (success, output) =
			try_call(addr, Burn::tryBurnCall { amount: alloy(evm(1, 5)) }.abi_encode());
		assert!(!success);
		assert_eq!(decode_error(&output), "burn: insufficient balance");
		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm(1, 0));

		let (success, _) =
			try_call(addr, Burn::tryBurnCall { amount: alloy(evm(0, 5)) }.abi_encode());
		assert!(success);
		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm(1, 0) - evm(0, 5));
		assert_eq!(burned_events(), vec![1]);
	});
}

/// Frozen funds are not spendable, so they cannot be burned.
#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn frozen_balance_cannot_be_burned(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 1_000, 7);
		let account = <Test as Config>::AddressMapper::to_account_id(&addr);
		// The freeze covers the existential deposit and the storage deposit hold. Five units
		// stay spendable.
		let total = <Test as Config>::Currency::total_balance(&account);
		assert_ok!(Balances::set_freeze(
			&pallet_dummy::FreezeReason::Foreign.into(),
			&account,
			total - 5
		));
		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm(5, 7));
		let issuance = total_issuance();

		let (success, output) =
			try_call(addr, Burn::tryBurnCall { amount: alloy(evm(6, 0)) }.abi_encode());
		assert!(!success);
		assert_eq!(decode_error(&output), "burn: insufficient balance");
		assert_eq!(issuance, total_issuance());

		let (success, _) =
			try_call(addr, Burn::tryBurnCall { amount: alloy(evm(5, 7)) }.abi_encode());
		assert!(success);
		assert_eq!(Pallet::<Test>::evm_balance(&addr), U256::zero());
		assert_eq!(issuance - total_issuance(), 5);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn sending_value_reverts(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 1_000, 7);
		let issuance = total_issuance();

		let (success, output) = try_call(
			addr,
			Burn::tryBurnWithValueCall { amount: alloy(evm(5, 0)), value: alloy(evm(2, 0)) }
				.abi_encode(),
		);
		assert!(!success);
		assert_eq!(decode_error(&output), "burn: value must not be sent along");

		// The value is back with the contract.
		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm(1_000, 7));
		assert_eq!(Pallet::<Test>::evm_balance(&H160(SYSTEM_PRECOMPILE_ADDR)), U256::zero());
		assert_eq!(issuance, total_issuance());
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn static_call_fails(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 1_000, 7);
		let issuance = total_issuance();

		let (success, output) =
			try_call(addr, Burn::tryBurnStaticCall { amount: alloy(evm(5, 0)) }.abi_encode());
		assert!(!success);
		assert!(output.is_empty());

		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm(1_000, 7));
		assert_eq!(issuance, total_issuance());
	});
}

/// A frame below a static call is read-only, even when it reaches the pre-compile with a
/// regular call.
#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn nested_static_call_fails(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 1_000, 7);
		let issuance = total_issuance();

		let (success, output) =
			try_call(addr, Burn::tryBurnNestedStaticCall { amount: alloy(evm(5, 0)) }.abi_encode());
		assert!(!success);
		assert!(output.is_empty());

		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm(1_000, 7));
		assert_eq!(issuance, total_issuance());
	});
}

/// A burn is undone with the frame it happened in, including the unit converted into dust.
#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn reverted_burn_is_rolled_back(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 1_000, 0);
		let issuance = total_issuance();

		let (success, output) =
			try_call(addr, Burn::tryBurnThenRevertCall { amount: alloy(evm(5, 10)) }.abi_encode());
		assert!(!success);
		// The burn itself went through.
		assert_eq!(decode_error(&output), "after burn");

		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm(1_000, 0));
		assert_eq!(issuance, total_issuance());
	});
}

/// A delegate call to the pre-compile would burn from whoever called the contract.
#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn delegate_call_to_precompile_reverts(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 1_000, 7);
		let alice = Pallet::<Test>::evm_balance(&ALICE_ADDR);
		let issuance = total_issuance();

		let (success, output) =
			try_call(addr, Burn::tryBurnDelegateCall { amount: alloy(evm(5, 0)) }.abi_encode());
		assert!(!success);
		assert_eq!(decode_error(&output), "illegal to call this pre-compile via delegate call");

		assert_eq!(Pallet::<Test>::evm_balance(&ALICE_ADDR), alice);
		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm(1_000, 7));
		assert_eq!(issuance, total_issuance());
	});
}

/// A contract that calls another contract does not pay for the burn of the callee.
#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn nested_call_burns_from_the_callee(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let caller = deploy_with_salt(fixture_type, 1_000, 7, 0);
		let callee = deploy_with_salt(fixture_type, 500, 0, 1);
		let alice = Pallet::<Test>::evm_balance(&ALICE_ADDR);
		let issuance = total_issuance();

		let result = builder::bare_call(caller)
			.data(
				Burn::burnViaCall { other: callee.0.into(), amount: alloy(evm(5, 10)) }
					.abi_encode(),
			)
			.build_and_unwrap_result();
		assert!(!result.did_revert());

		assert_eq!(Pallet::<Test>::evm_balance(&callee), evm(500, 0) - evm(5, 10));
		assert_eq!(Pallet::<Test>::evm_balance(&caller), evm(1_000, 7));
		assert_eq!(Pallet::<Test>::evm_balance(&ALICE_ADDR), alice);
		assert_eq!(issuance - total_issuance(), 6);
	});
}

/// Code that runs behind a delegate call burns from the contract it runs as.
#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn proxy_burns_its_own_balance(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let implementation = deploy_with_salt(fixture_type, 1_000, 7, 0);
		let proxy = deploy_with_salt(fixture_type, 500, 0, 1);
		let alice = Pallet::<Test>::evm_balance(&ALICE_ADDR);
		let issuance = total_issuance();

		let result = builder::bare_call(proxy)
			.data(
				Burn::burnAsProxyCall {
					implementation: implementation.0.into(),
					amount: alloy(evm(5, 0)),
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();
		assert!(!result.did_revert());

		assert_eq!(Pallet::<Test>::evm_balance(&proxy), evm(495, 0));
		assert_eq!(Pallet::<Test>::evm_balance(&implementation), evm(1_000, 7));
		assert_eq!(Pallet::<Test>::evm_balance(&ALICE_ADDR), alice);
		assert_eq!(issuance - total_issuance(), 5);
	});
}

fn burn_input(amount: U256) -> Vec<u8> {
	ISystem::ISystemCalls::burn(ISystem::burnCall { amount: alloy(amount) }).abi_encode()
}

/// An account that is no contract burns from its own balance.
#[test]
fn direct_call_burns_from_origin() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let alice = Pallet::<Test>::evm_balance(&ALICE_ADDR);
		let issuance = total_issuance();

		let result = builder::bare_call(H160(SYSTEM_PRECOMPILE_ADDR))
			.data(burn_input(evm(5, 0)))
			.build_and_unwrap_result();
		assert!(!result.did_revert());

		assert_eq!(Pallet::<Test>::evm_balance(&ALICE_ADDR), alice - evm(5, 0));
		assert_eq!(issuance - total_issuance(), 5);
	});
}

#[test]
fn root_origin_fails() {
	ExtBuilder::default().build().execute_with(|| {
		let issuance = total_issuance();

		let result = builder::bare_call(H160(SYSTEM_PRECOMPILE_ADDR))
			.origin(RuntimeOrigin::root())
			.data(burn_input(evm(5, 0)))
			.build()
			.result;
		assert_err!(result, DispatchError::RootNotAllowed);
		assert_eq!(issuance, total_issuance());
	});
}
