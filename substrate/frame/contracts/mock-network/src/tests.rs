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
	parachain::{self, Runtime},
	parachain_account_sovereign_account_id,
	primitives::{AccountId, CENTS},
	relay_chain, MockNet, ParaA, ParachainBalances, Relay, ALICE, BOB, INITIAL_BALANCE,
};
use assert_matches::assert_matches;
use codec::{Decode, Encode};
use frame_support::{
	assert_err,
	pallet_prelude::Weight,
	traits::{fungibles::Mutate, Currency},
};
use pallet_balances::{BalanceLock, Reasons};
use pallet_contracts::{Code, CollectEvents, DebugInfo, Determinism};
use pallet_contracts_fixtures::compile_module;
use xcm::{v4::prelude::*, VersionedLocation, VersionedXcm};
use xcm_simulator::TestExt;

type ParachainContracts = pallet_contracts::Pallet<parachain::Runtime>;

/// Instantiate the tests contract, and fund it with some balance and assets.
fn instantiate_test_contract(name: &str) -> AccountId {
	let (wasm, _) = compile_module::<Runtime>(name).unwrap();

	// Instantiate contract.
	let contract_addr = ParaA::execute_with(|| {
		ParachainContracts::bare_instantiate(
			ALICE,
			0,
			Weight::MAX,
			None,
			Code::Upload(wasm),
			vec![],
			vec![],
			DebugInfo::UnsafeDebug,
			CollectEvents::Skip,
		)
		.result
		.unwrap()
		.account_id
	});

	// Funds contract account with some balance and assets.
	ParaA::execute_with(|| {
		parachain::Balances::make_free_balance_be(&contract_addr, INITIAL_BALANCE);
		parachain::Assets::mint_into(0u32.into(), &contract_addr, INITIAL_BALANCE).unwrap();
	});
	Relay::execute_with(|| {
		let sovereign_account = parachain_account_sovereign_account_id(1u32, contract_addr.clone());
		relay_chain::Balances::make_free_balance_be(&sovereign_account, INITIAL_BALANCE);
	});

	contract_addr
}

#[test]
fn test_xcm_execute() {
	MockNet::reset();

	let contract_addr = instantiate_test_contract("xcm_execute");

	// Execute XCM instructions through the contract.
	ParaA::execute_with(|| {
		let amount: u128 = 10 * CENTS;

		// The XCM used to transfer funds to Bob.
		let message: Xcm<()> = Xcm(vec![
			WithdrawAsset(vec![(Here, amount).into()].into()),
			DepositAsset {
				assets: All.into(),
				beneficiary: AccountId32 { network: None, id: BOB.clone().into() }.into(),
			},
		]);

		let result = ParachainContracts::bare_call(
			ALICE,
			contract_addr.clone(),
			0,
			Weight::MAX,
			None,
			VersionedXcm::V4(message).encode(),
			DebugInfo::UnsafeDebug,
			CollectEvents::UnsafeCollect,
			Determinism::Enforced,
		)
		.result
		.unwrap();

		let mut data = &result.data[..];
		let outcome = Outcome::decode(&mut data).expect("Failed to decode xcm_execute Outcome");
		assert_matches!(outcome, Outcome::Complete { .. });

		// Check if the funds are subtracted from the account of Alice and added to the account of
		// Bob.
		let initial = INITIAL_BALANCE;
		assert_eq!(parachain::Assets::balance(0, contract_addr), initial);
		assert_eq!(ParachainBalances::free_balance(BOB), initial + amount);
	});
}

#[test]
fn test_xcm_execute_filtered_call() {
	MockNet::reset();

	let contract_addr = instantiate_test_contract("xcm_execute");

	ParaA::execute_with(|| {
		// `remark`  should be rejected, as it is not allowed by our CallFilter.
		let call = parachain::RuntimeCall::System(frame_system::Call::remark { remark: vec![] });
		let message: Xcm<parachain::RuntimeCall> = Xcm(vec![Transact {
			origin_kind: OriginKind::Native,
			require_weight_at_most: Weight::MAX,
			call: call.encode().into(),
		}]);

		let result = ParachainContracts::bare_call(
			ALICE,
			contract_addr.clone(),
			0,
			Weight::MAX,
			None,
			VersionedXcm::V4(message).encode(),
			DebugInfo::UnsafeDebug,
			CollectEvents::UnsafeCollect,
			Determinism::Enforced,
		);

		assert_err!(result.result, frame_system::Error::<parachain::Runtime>::CallFiltered);
	});
}

#[test]
fn test_xcm_execute_reentrant_call() {
	MockNet::reset();

	let contract_addr = instantiate_test_contract("xcm_execute");

	ParaA::execute_with(|| {
		let transact_call = parachain::RuntimeCall::Contracts(pallet_contracts::Call::call {
			dest: contract_addr.clone(),
			gas_limit: 1_000_000.into(),
			storage_deposit_limit: None,
			data: vec![],
			value: 0u128,
		});

		// The XCM used to transfer funds to Bob.
		let message: Xcm<parachain::RuntimeCall> = Xcm(vec![
			Transact {
				origin_kind: OriginKind::Native,
				require_weight_at_most: 1_000_000_000.into(),
				call: transact_call.encode().into(),
			},
			ExpectTransactStatus(MaybeErrorCode::Success),
		]);

		let result = ParachainContracts::bare_call(
			ALICE,
			contract_addr.clone(),
			0,
			Weight::MAX,
			None,
			VersionedXcm::V4(message).encode(),
			DebugInfo::UnsafeDebug,
			CollectEvents::UnsafeCollect,
			Determinism::Enforced,
		)
		.result
		.unwrap();

		let mut data = &result.data[..];
		let outcome = Outcome::decode(&mut data).expect("Failed to decode xcm_execute Outcome");
		assert_matches!(
			outcome,
			Outcome::Incomplete { used: _, error: XcmError::ExpectationFalse }
		);

		// Funds should not change hands as the XCM transact failed.
		assert_eq!(ParachainBalances::free_balance(BOB), INITIAL_BALANCE);
	});
}

#[test]
fn test_xcm_send() {
	MockNet::reset();
	let contract_addr = instantiate_test_contract("xcm_send");
	let fee = parachain::estimate_message_fee(4); // Accounts for the `DescendOrigin` instruction added by `send_xcm`

	// Send XCM instructions through the contract, to lock some funds on the relay chain.
	ParaA::execute_with(|| {
		let dest = Location::from(Parent);
		let dest = VersionedLocation::V4(dest);

		let message: Xcm<()> = Xcm(vec![
			WithdrawAsset((Here, fee).into()),
			BuyExecution { fees: (Here, fee).into(), weight_limit: WeightLimit::Unlimited },
			LockAsset { asset: (Here, 5 * CENTS).into(), unlocker: (Parachain(1)).into() },
		]);
		let message = VersionedXcm::V4(message);
		let exec = ParachainContracts::bare_call(
			ALICE,
			contract_addr.clone(),
			0,
			Weight::MAX,
			None,
			(dest, message).encode(),
			DebugInfo::UnsafeDebug,
			CollectEvents::UnsafeCollect,
			Determinism::Enforced,
		);

		let mut data = &exec.result.unwrap().data[..];
		XcmHash::decode(&mut data).expect("Failed to decode xcm_send message_id");
	});

	Relay::execute_with(|| {
		// Check if the funds are locked on the relay chain.
		assert_eq!(
			relay_chain::Balances::locks(&parachain_account_sovereign_account_id(1, contract_addr)),
			vec![BalanceLock { id: *b"py/xcmlk", amount: 5 * CENTS, reasons: Reasons::All }]
		);
	});
}
