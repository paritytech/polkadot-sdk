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

//! CREATE1 addresses of contracts instantiated by substrate transactions.

use crate::{
	BalanceOf, Code, Config, ExecConfig, SubstrateTxSigner,
	address::{AddressMapper, create1},
	test_utils::{ALICE, EVE, EVE_FALLBACK, WEIGHT_LIMIT, deposit_limit},
	tests::{
		ExtBuilder, RuntimeCall, RuntimeOrigin, System, Test, builder,
		test_utils::{dispatch_signed_tx, get_contract_checked},
	},
};
use frame_support::{
	assert_ok,
	storage::{TransactionOutcome, with_transaction},
	traits::fungible::Mutate,
};
use pallet_revive_fixtures::{FixtureType, compile_module_with_type};
use pretty_assertions::assert_eq;
use sp_core::H160;
use sp_runtime::{AccountId32, DispatchError};
use test_case::test_case;

const FUNDS: BalanceOf<Test> = 1_000_000_000_000_000;

fn instantiate(code: &[u8], value: BalanceOf<Test>, salt: Option<[u8; 32]>) -> RuntimeCall {
	RuntimeCall::Contracts(crate::Call::instantiate_with_code {
		value,
		weight_limit: WEIGHT_LIMIT,
		storage_deposit_limit: deposit_limit::<Test>(),
		code: code.to_vec(),
		data: vec![],
		salt,
	})
}

fn address(account: &AccountId32) -> H160 {
	<Test as Config>::AddressMapper::to_address(account)
}

fn assert_contract_at(deployer: &AccountId32, nonce: u32) {
	assert!(
		get_contract_checked(&create1(&address(deployer), nonce.into())).is_some(),
		"no contract at nonce {nonce}",
	);
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn batch_from_signer_uses_consecutive_nonces(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Dummy", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, FUNDS);

		assert_ok!(dispatch_signed_tx(
			&ALICE,
			RuntimeCall::Utility(pallet_utility::Call::batch_all {
				calls: vec![
					instantiate(&code, 0, None),
					instantiate(&code, 0, None),
					instantiate(&code, 0, None),
				],
			}),
		));
		for nonce in 0..3 {
			assert_contract_at(&ALICE, nonce);
		}
		assert_eq!(System::account_nonce(&ALICE), 3);

		// A CREATE2 instantiation also uses up the nonce the extrinsic was signed with.
		assert_ok!(dispatch_signed_tx(
			&ALICE,
			RuntimeCall::Utility(pallet_utility::Call::batch_all {
				calls: vec![instantiate(&code, 0, Some([1; 32])), instantiate(&code, 0, None)],
			}),
		));
		assert!(get_contract_checked(&create1(&address(&ALICE), 3)).is_none());
		assert_contract_at(&ALICE, 4);
		assert_eq!(System::account_nonce(&ALICE), 5);
		assert!(!SubstrateTxSigner::<Test>::exists());
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn failed_instantiation_keeps_tx_nonce(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Dummy", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, FUNDS);

		// The first instantiation cannot transfer its value. The second one still gets the
		// nonce the extrinsic was signed with.
		assert_ok!(dispatch_signed_tx(
			&ALICE,
			RuntimeCall::Utility(pallet_utility::Call::force_batch {
				calls: vec![instantiate(&code, FUNDS * 10, None), instantiate(&code, 0, None)],
			}),
		));
		assert_contract_at(&ALICE, 0);
		assert_eq!(System::account_nonce(&ALICE), 1);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn fallback_account_uses_own_nonces(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Dummy", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&EVE, FUNDS);
		let _ = <Test as Config>::Currency::set_balance(&EVE_FALLBACK, FUNDS);

		for nonce in 0..3 {
			assert_ok!(dispatch_signed_tx(
				&EVE,
				RuntimeCall::Contracts(crate::Call::dispatch_as_fallback_account {
					call: Box::new(instantiate(&code, 0, None)),
				}),
			));
			assert_contract_at(&EVE_FALLBACK, nonce);
			assert_eq!(System::account_nonce(&EVE_FALLBACK), nonce + 1);
			assert_eq!(System::account_nonce(&EVE), nonce + 1);
		}
		assert!(!SubstrateTxSigner::<Test>::exists());
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn pure_proxy_uses_own_nonces(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Dummy", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, FUNDS);
		assert_ok!(dispatch_signed_tx(
			&ALICE,
			RuntimeCall::Proxy(pallet_proxy::Call::create_pure {
				proxy_type: (),
				delay: 0,
				index: 0,
			}),
		));
		let pure = pallet_proxy::Pallet::<Test>::pure_account(&ALICE, &(), 0, None);
		let _ = <Test as Config>::Currency::set_balance(&pure, FUNDS);
		let as_pure = |call| {
			RuntimeCall::Proxy(pallet_proxy::Call::proxy {
				real: pure.clone(),
				force_proxy_type: None,
				call: Box::new(call),
			})
		};
		assert_ok!(dispatch_signed_tx(
			&ALICE,
			as_pure(RuntimeCall::Contracts(crate::Call::map_account {})),
		));
		assert_eq!(System::account_nonce(&ALICE), 2);

		for nonce in 0..2 {
			assert_ok!(dispatch_signed_tx(&ALICE, as_pure(instantiate(&code, 0, None))));
			assert_contract_at(&pure, nonce);
			assert_eq!(System::account_nonce(&pure), nonce + 1);
		}
		assert_eq!(System::account_nonce(&ALICE), 4);

		// The signer's instantiation in the same extrinsic still uses the signed nonce.
		assert_ok!(dispatch_signed_tx(
			&ALICE,
			RuntimeCall::Utility(pallet_utility::Call::batch_all {
				calls: vec![as_pure(instantiate(&code, 0, None)), instantiate(&code, 0, None)],
			}),
		));
		assert_contract_at(&pure, 2);
		assert_contract_at(&ALICE, 4);
		assert_eq!(System::account_nonce(&pure), 3);
		assert_eq!(System::account_nonce(&ALICE), 5);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn dry_run_matches_extrinsic(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Dummy", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, FUNDS);

		for nonce in 0..2 {
			// What the `instantiate` runtime API does.
			let dry_run = with_transaction(|| {
				crate::Pallet::<Test>::prepare_dry_run(&ALICE);
				let result = builder::bare_instantiate(Code::Upload(code.clone()))
					.origin(RuntimeOrigin::signed(ALICE))
					.salt(None)
					.exec_config(ExecConfig::new_substrate_tx().with_dry_run(None))
					.build();
				TransactionOutcome::Rollback(Ok::<_, DispatchError>(result))
			})
			.unwrap();
			let predicted = dry_run.result.unwrap().addr;
			assert_eq!(predicted, create1(&address(&ALICE), nonce.into()));
			assert_eq!(System::account_nonce(&ALICE), nonce);

			assert_ok!(dispatch_signed_tx(&ALICE, instantiate(&code, 0, None)));
			assert!(get_contract_checked(&predicted).is_some());
		}
	});
}
