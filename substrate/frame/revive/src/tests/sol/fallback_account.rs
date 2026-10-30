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

//! Dispatching pallet-revive calls as a fallback account.

use crate::{
	BalanceOf, Code, Config, Error, Pallet,
	address::{AddressMapper, create1},
	test_utils::{
		ALICE, EVE, EVE_ADDR, EVE_FALLBACK, WEIGHT_LIMIT, builder::Contract, deposit_limit,
	},
	tests::{
		ExtBuilder, RuntimeCall, RuntimeOrigin, Test, builder, test_utils::get_contract_checked,
	},
};
use frame_support::{
	assert_err_ignore_postinfo, assert_noop, assert_ok,
	traits::fungible::{Inspect, Mutate},
};
use pallet_revive_fixtures::{FixtureType, compile_module_with_type};
use pretty_assertions::assert_eq;
use sp_core::H256;
use sp_runtime::{DispatchError, traits::Dispatchable};
use test_case::test_case;

const FUNDS: BalanceOf<Test> = 1_000_000_000_000;

fn revive_calls(code: &[u8], code_hash: H256, dest: sp_core::H160) -> [RuntimeCall; 3] {
	[
		RuntimeCall::Contracts(crate::Call::instantiate_with_code {
			value: 0,
			weight_limit: WEIGHT_LIMIT,
			storage_deposit_limit: deposit_limit::<Test>(),
			code: code.to_vec(),
			data: vec![],
			salt: None,
		}),
		RuntimeCall::Contracts(crate::Call::instantiate {
			value: 0,
			weight_limit: WEIGHT_LIMIT,
			storage_deposit_limit: deposit_limit::<Test>(),
			code_hash,
			data: vec![],
			salt: Some([1; 32]),
		}),
		RuntimeCall::Contracts(crate::Call::call {
			dest,
			value: 0,
			weight_limit: WEIGHT_LIMIT,
			storage_deposit_limit: deposit_limit::<Test>(),
			data: vec![],
		}),
	]
}

fn as_fallback(call: RuntimeCall) -> RuntimeCall {
	RuntimeCall::Contracts(crate::Call::dispatch_as_fallback_account { call: Box::new(call) })
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn shadowed_fallback_account_is_rejected(fixture_type: FixtureType) {
	let (code, code_hash) = compile_module_with_type("Dummy", fixture_type).unwrap();

	ExtBuilder::default().existential_deposit(100).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&EVE, FUNDS);
		let _ = <Test as Config>::Currency::set_balance(&EVE_FALLBACK, FUNDS);
		assert_ok!(Pallet::<Test>::map_account(RuntimeOrigin::signed(EVE)));
		assert_eq!(<Test as Config>::AddressMapper::to_account_id(&EVE_ADDR), EVE);

		let [instantiate_with_code, instantiate, call] =
			revive_calls(&code, code_hash, create1(&EVE_ADDR, 0));
		assert_ok!(instantiate_with_code.clone().dispatch(RuntimeOrigin::signed(EVE)));
		assert!(get_contract_checked(&create1(&EVE_ADDR, 0)).is_some());

		for revive_call in [instantiate_with_code, instantiate, call] {
			assert_err_ignore_postinfo!(
				as_fallback(revive_call).dispatch(RuntimeOrigin::signed(EVE)),
				DispatchError::BadOrigin,
			);
		}

		// Funds stay recoverable from the fallback account.
		let transferable = <Test as Config>::Currency::balance(&EVE_FALLBACK);
		let eve_balance = <Test as Config>::Currency::balance(&EVE);
		assert_ok!(
			as_fallback(RuntimeCall::Balances(pallet_balances::Call::transfer_all {
				dest: EVE,
				keep_alive: false,
			}))
			.dispatch(RuntimeOrigin::signed(EVE))
		);
		assert_eq!(<Test as Config>::Currency::balance(&EVE_FALLBACK), 0);
		assert_eq!(<Test as Config>::Currency::balance(&EVE), eve_balance + transferable);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn fallback_account_of_unmapped_account_is_accepted(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Dummy", fixture_type).unwrap();

	ExtBuilder::default().existential_deposit(100).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&EVE, FUNDS);
		let _ = <Test as Config>::Currency::set_balance(&EVE_FALLBACK, FUNDS);
		let _ = <Test as Config>::Currency::set_balance(&ALICE, FUNDS);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code.clone())).build_and_unwrap_contract();

		let code_hash = get_contract_checked(&addr).unwrap().code_hash;
		let [instantiate_with_code, instantiate, call] = revive_calls(&code, code_hash, addr);
		assert_ok!(as_fallback(instantiate_with_code).dispatch(RuntimeOrigin::signed(EVE)));
		let instantiate = as_fallback(instantiate).dispatch(RuntimeOrigin::signed(EVE));
		if fixture_type == FixtureType::Solc {
			assert_err_ignore_postinfo!(instantiate, Error::<Test>::EvmConstructedFromHash);
		} else {
			assert_ok!(instantiate);
		}
		// `Dummy` has no fallback function, so reaching it reverts.
		assert_err_ignore_postinfo!(
			as_fallback(call.clone()).dispatch(RuntimeOrigin::signed(EVE)),
			Error::<Test>::ContractReverted,
		);

		// Without a mapping the account itself cannot act as the address.
		assert_noop!(
			call.dispatch(RuntimeOrigin::signed(EVE)).map_err(|e| e.error),
			Error::<Test>::AccountUnmapped,
		);
	});
}
