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
	AccountInfo, BalanceWithDust, Code, Config, H160, Pallet, U256,
	address::AddressMapper,
	test_utils::{ALICE, ALICE_ADDR, builder::Contract},
	tests::{
		Balances, ExtBuilder, RuntimeOrigin, Test, builder, pallet_dummy,
		test_utils::{decode_error, evm_value, get_balance, set_balance_with_dust},
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

fn alloy(value: U256) -> AlloyU256 {
	AlloyU256::from_be_bytes(value.to_big_endian())
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
	addr
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn received_value_is_burned(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 1_000, 3);
		let balance = Pallet::<Test>::evm_balance(&addr);
		let issuance = Balances::total_issuance();

		let result = builder::bare_call(addr)
			.evm_value(evm_value(5, 0))
			.data(Burn::burnCall { amount: alloy(evm_value(5, 0)) }.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert());

		// The contract is left with what it had before the call.
		assert_eq!(Pallet::<Test>::evm_balance(&addr), balance);
		assert_eq!(issuance - Balances::total_issuance(), 5);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn whole_balance_is_burned_except_existential_deposit(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 1_000, 7);
		let account = <Test as Config>::AddressMapper::to_account_id(&addr);
		let issuance = Balances::total_issuance();

		let result = builder::bare_call(addr)
			.data(Burn::burnAllCall {}.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert());

		assert_eq!(Pallet::<Test>::evm_balance(&addr), U256::zero());
		assert_eq!(get_balance(&account), <Test as Config>::Currency::minimum_balance());
		// Dust is not part of the issuance.
		assert_eq!(issuance - Balances::total_issuance(), 1_000);
	});
}

/// The unit that is converted into dust has to be spendable as well: the contract holds exactly
/// one unit more than the whole units it burns.
#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn missing_dust_is_converted_from_balance(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 6, 0);
		let issuance = Balances::total_issuance();

		let result = builder::bare_call(addr)
			.data(Burn::burnCall { amount: alloy(evm_value(5, 10)) }.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert());

		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm_value(6, 0) - evm_value(5, 10));
		assert_eq!(issuance - Balances::total_issuance(), 6);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn only_dust_is_burned(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 1_000, 7);
		let issuance = Balances::total_issuance();

		let result = builder::bare_call(addr)
			.data(Burn::burnCall { amount: AlloyU256::ZERO }.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert());
		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm_value(1_000, 7));

		let result = builder::bare_call(addr)
			.data(Burn::burnCall { amount: alloy(evm_value(0, 4)) }.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert());
		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm_value(1_000, 3));

		assert_eq!(issuance, Balances::total_issuance());
	});
}

/// The burn in the constructor creates the account info of the contract. The dust it leaves has
/// to survive the construction.
#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn constructor_burns_received_value(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let (code, _) = compile_module_with_type("Burn", fixture_type).unwrap();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(Burn::constructorCall { amount: alloy(evm_value(2, 3)) }.abi_encode())
			.evm_value(evm_value(5, 0))
			.build_and_unwrap_contract();

		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm_value(5, 0) - evm_value(2, 3));
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

		let result = builder::bare_call(delegator)
			.data(Burn::burnCall { amount: alloy(evm_value(5, 10)) }.abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert());

		assert_eq!(Pallet::<Test>::evm_balance(&delegator), balance - evm_value(5, 10));
		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm_value(1_000, 7));
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

		// One unit of dust more than the contract can spend.
		let (success, output) =
			try_call(addr, Burn::tryBurnCall { amount: alloy(evm_value(1_000, 8)) }.abi_encode());
		assert!(!success);
		assert_eq!(decode_error(&output), "burn: insufficient balance");

		// An amount that does not fit the balance type.
		let (success, output) =
			try_call(addr, Burn::tryBurnCall { amount: AlloyU256::MAX }.abi_encode());
		assert!(!success);
		assert_eq!(decode_error(&output), "burn: amount out of range");
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
		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm_value(5, 7));

		let (success, output) =
			try_call(addr, Burn::tryBurnCall { amount: alloy(evm_value(6, 0)) }.abi_encode());
		assert!(!success);
		assert_eq!(decode_error(&output), "burn: insufficient balance");

		let (success, _) =
			try_call(addr, Burn::tryBurnCall { amount: alloy(evm_value(5, 7)) }.abi_encode());
		assert!(success);
		assert_eq!(Pallet::<Test>::evm_balance(&addr), U256::zero());
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn sending_value_reverts(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 1_000, 7);

		let (success, output) = try_call(
			addr,
			Burn::tryBurnWithValueCall {
				amount: alloy(evm_value(5, 0)),
				value: alloy(evm_value(2, 0)),
			}
			.abi_encode(),
		);
		assert!(!success);
		assert_eq!(decode_error(&output), "burn: msg.value must be zero");
		// The value is back with the contract.
		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm_value(1_000, 7));
		assert_eq!(Pallet::<Test>::evm_balance(&H160(SYSTEM_PRECOMPILE_ADDR)), U256::zero());
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn static_call_fails(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 1_000, 7);

		let (success, _) =
			try_call(addr, Burn::tryBurnStaticCall { amount: alloy(evm_value(5, 0)) }.abi_encode());
		assert!(!success);
	});
}

/// A burn is undone with the frame it happened in, including the unit converted into dust.
#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn reverted_burn_is_rolled_back(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 1_000, 0);

		let (success, output) = try_call(
			addr,
			Burn::tryBurnThenRevertCall { amount: alloy(evm_value(5, 10)) }.abi_encode(),
		);
		assert!(!success);
		// The burn itself went through.
		assert_eq!(decode_error(&output), "after burn");

		assert_eq!(Pallet::<Test>::evm_balance(&addr), evm_value(1_000, 0));
	});
}

/// A delegate call to the pre-compile would burn from whoever called the contract.
#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn delegate_call_to_precompile_reverts(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type, 1_000, 7);
		let alice = Pallet::<Test>::evm_balance(&ALICE_ADDR);

		let (success, output) = try_call(
			addr,
			Burn::tryBurnDelegateCall { amount: alloy(evm_value(5, 0)) }.abi_encode(),
		);
		assert!(!success);
		assert_eq!(decode_error(&output), "illegal to call this pre-compile via delegate call");

		assert_eq!(Pallet::<Test>::evm_balance(&ALICE_ADDR), alice);
	});
}

/// Code that runs behind a delegate call burns from the contract it runs as.
#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn proxy_burns_its_own_balance(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let implementation = deploy_with_salt(fixture_type, 1_000, 7, 0);
		let proxy = deploy_with_salt(fixture_type, 500, 0, 1);

		let result = builder::bare_call(proxy)
			.data(
				Burn::burnAsProxyCall {
					implementation: implementation.0.into(),
					amount: alloy(evm_value(5, 0)),
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();
		assert!(!result.did_revert());

		assert_eq!(Pallet::<Test>::evm_balance(&proxy), evm_value(495, 0));
		assert_eq!(Pallet::<Test>::evm_balance(&implementation), evm_value(1_000, 7));
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

		let result = builder::bare_call(H160(SYSTEM_PRECOMPILE_ADDR))
			.data(burn_input(evm_value(5, 0)))
			.build_and_unwrap_result();
		assert!(!result.did_revert());

		assert_eq!(Pallet::<Test>::evm_balance(&ALICE_ADDR), alice - evm_value(5, 0));
	});
}

/// Value sent with a transaction to the pre-compile is returned by the revert.
#[test]
fn direct_call_with_value_reverts() {
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let alice = Pallet::<Test>::evm_balance(&ALICE_ADDR);

		let result = builder::bare_call(H160(SYSTEM_PRECOMPILE_ADDR))
			.evm_value(evm_value(2, 0))
			.data(burn_input(evm_value(5, 0)))
			.build_and_unwrap_result();
		assert!(result.did_revert());
		assert_eq!(decode_error(&result.data), "burn: msg.value must be zero");

		assert_eq!(Pallet::<Test>::evm_balance(&ALICE_ADDR), alice);
		assert_eq!(Pallet::<Test>::evm_balance(&H160(SYSTEM_PRECOMPILE_ADDR)), U256::zero());
	});
}

#[test]
fn root_origin_fails() {
	ExtBuilder::default().build().execute_with(|| {
		let result = builder::bare_call(H160(SYSTEM_PRECOMPILE_ADDR))
			.origin(RuntimeOrigin::root())
			.data(burn_input(evm_value(5, 0)))
			.build()
			.result;
		assert_err!(result, DispatchError::RootNotAllowed);
	});
}
