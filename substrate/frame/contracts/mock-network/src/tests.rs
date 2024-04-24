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
use codec::{Decode, Encode};
use frame_support::{
	assert_err,
	traits::{fungibles::Mutate, Currency},
};
use pallet_contracts::{test_utils::builder::*, Code};
use pallet_contracts_fixtures::compile_module;
use pallet_contracts_uapi::ReturnErrorCode;
use xcm::{v4::prelude::*, VersionedLocation, VersionedXcm};
use xcm_simulator::TestExt;

macro_rules! assert_return_code {
	( $x:expr , $y:expr $(,)? ) => {{
		assert_eq!(u32::from_le_bytes($x.data[..].try_into().unwrap()), $y as u32);
	}};
}

fn bare_call(dest: sp_runtime::AccountId32) -> BareCallBuilder<parachain::Runtime> {
	BareCallBuilder::<parachain::Runtime>::bare_call(ALICE, dest)
}

/// Instantiate the tests contract, and fund it with some balance and assets.
fn instantiate_test_contract(name: &str) -> AccountId {
	let (wasm, _) = compile_module::<Runtime>(name).unwrap();

	// Instantiate contract.
	let contract_addr = ParaA::execute_with(|| {
		BareInstantiateBuilder::<parachain::Runtime>::bare_instantiate(ALICE, Code::Upload(wasm))
			.build_and_unwrap_account_id()
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
		let assets: Asset = (Here, amount).into();
		let beneficiary = AccountId32 { network: None, id: BOB.clone().into() };

		// The XCM used to transfer funds to Bob.
		let message: Xcm<()> = Xcm::builder_unsafe()
			.withdraw_asset(assets.clone())
			.deposit_asset(assets, beneficiary)
			.build();

		let result = bare_call(contract_addr.clone())
			.data(VersionedXcm::V4(message).encode())
			.build();

		assert_eq!(result.gas_consumed, result.gas_required);
		assert_return_code!(&result.result.unwrap(), ReturnErrorCode::Success);

		// Check if the funds are subtracted from the account of Alice and added to the account of
		// Bob.
		let initial = INITIAL_BALANCE;
		assert_eq!(ParachainBalances::free_balance(BOB), initial + amount);
		assert_eq!(ParachainBalances::free_balance(&contract_addr), initial - amount);
	});
}

#[test]
fn test_xcm_execute_incomplete() {
	MockNet::reset();

	let contract_addr = instantiate_test_contract("xcm_execute");
	let amount = 10 * CENTS;

	// Execute XCM instructions through the contract.
	ParaA::execute_with(|| {
		let assets: Asset = (Here, amount).into();
		let beneficiary = AccountId32 { network: None, id: BOB.clone().into() };

		// The XCM used to transfer funds to Bob.
		let message: Xcm<()> = Xcm::builder_unsafe()
			.withdraw_asset(assets.clone())
			// This will fail as the contract does not have enough balance to complete both
			// withdrawals.
			.withdraw_asset((Here, INITIAL_BALANCE))
			.buy_execution(assets.clone(), Unlimited)
			.deposit_asset(assets, beneficiary)
			.build();

		let result = bare_call(contract_addr.clone())
			.data(VersionedXcm::V4(message).encode())
			.build();

		assert_eq!(result.gas_consumed, result.gas_required);
		assert_return_code!(&result.result.unwrap(), ReturnErrorCode::XcmExecutionFailed);

		assert_eq!(ParachainBalances::free_balance(BOB), INITIAL_BALANCE);
		assert_eq!(ParachainBalances::free_balance(&contract_addr), INITIAL_BALANCE - amount);
	});
}

#[test]
fn test_xcm_execute_filtered_call() {
	MockNet::reset();

	let contract_addr = instantiate_test_contract("xcm_execute");

	ParaA::execute_with(|| {
		// `remark`  should be rejected, as it is not allowed by our CallFilter.
		let call = parachain::RuntimeCall::System(frame_system::Call::remark { remark: vec![] });
		let message: Xcm<parachain::RuntimeCall> = Xcm::builder_unsafe()
			.transact(OriginKind::Native, Weight::MAX, call.encode())
			.build();
		let result = bare_call(contract_addr.clone())
			.data(VersionedXcm::V4(message).encode())
			.build()
			.result;
		assert_err!(result, frame_system::Error::<parachain::Runtime>::CallFiltered);
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
		let message: Xcm<parachain::RuntimeCall> = Xcm::builder_unsafe()
			.transact(OriginKind::Native, 1_000_000_000, transact_call.encode())
			.expect_transact_status(MaybeErrorCode::Success)
			.build();

		let result = bare_call(contract_addr.clone())
			.data(VersionedXcm::V4(message).encode())
			.build_and_unwrap_result();

		assert_return_code!(&result, ReturnErrorCode::XcmExecutionFailed);

		// Funds should not change hands as the XCM transact failed.
		assert_eq!(ParachainBalances::free_balance(BOB), INITIAL_BALANCE);
	});
}

#[test]
fn test_xcm_send() {
	MockNet::reset();
	let contract_addr = instantiate_test_contract("xcm_send");
	let amount = 1_000 * CENTS;
	let fee = parachain::estimate_message_fee(4); // Accounts for the `DescendOrigin` instruction added by `send_xcm`

	// Send XCM instructions through the contract, to transfer some funds from the contract
	// derivative account to Alice on the relay chain.
	ParaA::execute_with(|| {
		let dest = VersionedLocation::V4(Parent.into());
		let assets: Asset = (Here, amount).into();
		let beneficiary = AccountId32 { network: None, id: ALICE.clone().into() };

		let message: Xcm<()> = Xcm::builder()
			.withdraw_asset(assets.clone())
			.buy_execution((Here, fee), Unlimited)
			.deposit_asset(assets, beneficiary)
			.build();

		let result = bare_call(contract_addr.clone())
			.data((dest, VersionedXcm::V4(message)).encode())
			.build_and_unwrap_result();

		let mut data = &result.data[..];
		XcmHash::decode(&mut data).expect("Failed to decode xcm_send message_id");
	});

	Relay::execute_with(|| {
		let derived_contract_addr = &parachain_account_sovereign_account_id(1, contract_addr);
		assert_eq!(
			INITIAL_BALANCE - amount,
			relay_chain::Balances::free_balance(derived_contract_addr)
		);
		assert_eq!(INITIAL_BALANCE + amount - fee, relay_chain::Balances::free_balance(ALICE));
	});
}
