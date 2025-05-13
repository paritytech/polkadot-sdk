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
	parachain, parachain_account_sovereign_account_id, primitives::CENTS, relay_chain, MockNet,
	ParaA, ParachainBalances, Relay, ALICE, BOB, INITIAL_BALANCE,
};
use codec::{Decode, Encode};
use frame_support::traits::{fungibles::Mutate, Currency};
use frame_system::RawOrigin;
use pallet_revive::{
	precompiles::alloy::sol_types::{SolInterface, SolValue},
	precompiles::builtin::xcm::IXcm,
	test_utils::{self, builder::*},
	Code, DepositLimit, ExecReturnValue,
};
use pallet_revive_fixtures::compile_module;
use pallet_revive_uapi::{ReturnErrorCode, ReturnFlags};
use sp_core::H160;
use xcm::{v4::prelude::*, VersionedLocation, VersionedXcm};
use xcm_simulator::TestExt;

macro_rules! assert_return_code {
	($x:expr, $y:expr $(,)?) => {{
		assert_eq!(u32::from_le_bytes($x.data[..].try_into().unwrap()), $y as u32);
	}};
}

fn bare_call(dest: H160) -> BareCallBuilder<parachain::Runtime> {
	BareCallBuilder::<parachain::Runtime>::bare_call(RawOrigin::Signed(ALICE).into(), dest)
}

/// Instantiate the tests contract, and fund it with some balance and assets.
fn instantiate_test_contract(name: &str) -> Contract<parachain::Runtime> {
	let (wasm, _) = compile_module(name).unwrap();

	// Instantiate contract.
	let contract = ParaA::execute_with(|| {
		BareInstantiateBuilder::<parachain::Runtime>::bare_instantiate(
			RawOrigin::Signed(ALICE).into(),
			Code::Upload(wasm),
		)
		.storage_deposit_limit(DepositLimit::Balance(1_000_000_000_000))
		.build_and_unwrap_contract()
	});

	// Funds contract account with some balance and assets.
	ParaA::execute_with(|| {
		parachain::Balances::make_free_balance_be(&contract.account_id, INITIAL_BALANCE);
		parachain::Assets::mint_into(0u32.into(), &contract.account_id, INITIAL_BALANCE).unwrap();
	});

	Relay::execute_with(|| {
		let sovereign_account =
			parachain_account_sovereign_account_id(1u32, contract.account_id.clone());
		relay_chain::Balances::make_free_balance_be(&sovereign_account, INITIAL_BALANCE);
	});

	contract
}

fn to_fixed_non_zero(precompile_id: u16) -> H160 {
	let mut address = [0u8; 20];
	address[16] = (precompile_id >> 8) as u8;
	address[17] = (precompile_id & 0xFF) as u8;

	H160::from(address)
}

#[test]
fn test_xcm_execute() {
	MockNet::reset();

	let Contract { addr, account_id } = instantiate_test_contract("xcm_execute");

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

		let result = bare_call(addr).data(VersionedXcm::V4(message).encode()).build();

		assert_eq!(result.gas_consumed, result.gas_required);
		assert_return_code!(&result.result.unwrap(), ReturnErrorCode::Success);

		// Check if the funds are subtracted from the account of Alice and added to the account of
		// Bob.
		let initial = INITIAL_BALANCE;
		assert_eq!(ParachainBalances::free_balance(BOB), initial + amount);
		assert_eq!(ParachainBalances::free_balance(&account_id), initial - amount);
	});
}

#[test]
fn test_xcm_execute_incomplete() {
	MockNet::reset();

	let Contract { addr, account_id } = instantiate_test_contract("xcm_execute");
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

		let result = bare_call(addr).data(VersionedXcm::V4(message).encode()).build();

		assert_eq!(result.gas_consumed, result.gas_required);
		assert_return_code!(&result.result.unwrap(), ReturnErrorCode::XcmExecutionFailed);

		assert_eq!(ParachainBalances::free_balance(BOB), INITIAL_BALANCE);
		assert_eq!(ParachainBalances::free_balance(&account_id), INITIAL_BALANCE - amount);
	});
}

#[test]
fn test_xcm_execute_reentrant_call() {
	MockNet::reset();

	let Contract { addr, .. } = instantiate_test_contract("xcm_execute");

	ParaA::execute_with(|| {
		let transact_call = parachain::RuntimeCall::Contracts(pallet_revive::Call::call {
			dest: addr,
			gas_limit: 1_000_000.into(),
			storage_deposit_limit: test_utils::deposit_limit::<parachain::Runtime>(),
			data: vec![],
			value: 0u128,
		});

		let message: Xcm<parachain::RuntimeCall> = Xcm::builder_unsafe()
			.transact(OriginKind::Native, 1_000_000_000, transact_call.encode())
			.expect_transact_status(MaybeErrorCode::Success)
			.build();

		let result = bare_call(addr)
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
	let Contract { addr, account_id } = instantiate_test_contract("xcm_send");
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

		let result = bare_call(addr)
			.data((dest, VersionedXcm::V4(message)).encode())
			.build_and_unwrap_result();

		let mut data = &result.data[..];
		XcmHash::decode(&mut data).expect("Failed to decode xcm_send message_id");
	});

	Relay::execute_with(|| {
		let derived_contract_addr = &parachain_account_sovereign_account_id(1, account_id);
		assert_eq!(
			INITIAL_BALANCE - amount,
			relay_chain::Balances::free_balance(derived_contract_addr)
		);
		assert_eq!(INITIAL_BALANCE + amount - fee, relay_chain::Balances::free_balance(ALICE));
	});
}

#[test]
fn test_xcm_execute_reentrant_call_via_precompile() {
	MockNet::reset();

	ParaA::execute_with(|| {
		let initial_bob_balance = ParachainBalances::free_balance(BOB);

		let transact_call = parachain::RuntimeCall::Contracts(pallet_revive::Call::call {
			dest: to_fixed_non_zero(10),
			gas_limit: 1_000_000.into(),
			storage_deposit_limit: test_utils::deposit_limit::<parachain::Runtime>(),
			data: vec![],
			value: 0u128,
		});

		let message: Xcm<parachain::RuntimeCall> = Xcm::builder_unsafe()
			.transact(OriginKind::Native, 1_000_000_000, transact_call.encode())
			.expect_transact_status(MaybeErrorCode::Success)
			.build();

		let weight_params =
			IXcm::weightMessageCall { message: VersionedXcm::V4(message.clone()).encode().into() };
		let weight_call = IXcm::IXcmCalls::weightMessage(weight_params);
		let xcm_weight_results =
			bare_call(to_fixed_non_zero(10)).data(weight_call.abi_encode()).build();

		let weight_result = match xcm_weight_results.result {
			Ok(value) => value,
			Err(_) => ExecReturnValue { flags: ReturnFlags::REVERT, data: Vec::new() },
		};

		let weight: IXcm::Weight =
			IXcm::Weight::abi_decode(&weight_result.data[..], true).expect("Failed to weight");

		let xcm_execute_params =
			IXcm::xcmExecuteCall { message: VersionedXcm::V4(message).encode().into(), weight };

		let call = IXcm::IXcmCalls::xcmExecute(xcm_execute_params);
		let encoded_call = call.abi_encode();
		let results = bare_call(to_fixed_non_zero(10)).data(encoded_call).build();
		let result = match results.result {
			Ok(value) => value,
			Err(_) => ExecReturnValue { flags: ReturnFlags::REVERT, data: Vec::new() },
		};

		let final_bob_balance = ParachainBalances::free_balance(BOB);

		assert_eq!(
			result.flags,
			ReturnFlags::REVERT,
			"Expected transaction to revert due to reentrant call"
		);
		assert_eq!(final_bob_balance, initial_bob_balance, "Bob's balance should remain unchanged");
	});
}

#[test]
fn test_xcm_execute_incomplete_call_via_precompile() {
	MockNet::reset();
	let amount = 10 * CENTS;

	ParaA::execute_with(|| {
		let initial_bob_balance = ParachainBalances::free_balance(BOB);
		let initial_alice_balance = ParachainBalances::free_balance(ALICE);

		let assets: Asset = (Here, amount).into();
		let beneficiary = AccountId32 { network: None, id: BOB.clone().into() };

		let message: Xcm<()> = Xcm::builder_unsafe()
			.withdraw_asset(assets.clone())
			// This will fail as the contract does not have enough balance to complete both
			// withdrawals.
			.withdraw_asset((Here, INITIAL_BALANCE))
			.buy_execution(assets.clone(), Unlimited)
			.deposit_asset(assets, beneficiary)
			.build();

		// First, calculate the weight of the XCM message
		let weight_params =
			IXcm::weightMessageCall { message: VersionedXcm::V4(message.clone()).encode().into() };
		let weight_call = IXcm::IXcmCalls::weightMessage(weight_params);
		let xcm_weight_results =
			bare_call(to_fixed_non_zero(10)).data(weight_call.abi_encode()).build();

		let weight_result = match xcm_weight_results.result {
			Ok(value) => value,
			Err(_) => ExecReturnValue { flags: ReturnFlags::REVERT, data: Vec::new() },
		};

		let weight: IXcm::Weight =
			IXcm::Weight::abi_decode(&weight_result.data[..], true).expect("Failed to weight");

		let xcm_execute_params =
			IXcm::xcmExecuteCall { message: VersionedXcm::V4(message).encode().into(), weight };

		let call = IXcm::IXcmCalls::xcmExecute(xcm_execute_params);
		let encoded_call = call.abi_encode();
		bare_call(to_fixed_non_zero(10)).data(encoded_call).build();

		let final_bob_balance = ParachainBalances::free_balance(BOB);
		let final_alice_balance = ParachainBalances::free_balance(ALICE);

		assert_eq!(final_bob_balance, initial_bob_balance, "Bob's balance should remain unchanged");
		assert_eq!(
			final_alice_balance, initial_alice_balance,
			"Alice's balance should remain unchanged"
		);
	});
}

#[test]
fn test_xcm_execute_precompile() {
	MockNet::reset();
	let amount: u128 = 10 * CENTS;

	ParaA::execute_with(|| {
		let initial_alice_balance = ParachainBalances::free_balance(ALICE);
		let initial_bob_balance = ParachainBalances::free_balance(BOB);

		let assets: Asset = (Here, amount).into();
		let beneficiary = AccountId32 { network: None, id: BOB.clone().into() };

		let message: Xcm<()> = Xcm::builder_unsafe()
			.withdraw_asset(assets.clone())
			.deposit_asset(assets, beneficiary)
			.build();

		let weight_params =
			IXcm::weightMessageCall { message: VersionedXcm::V4(message.clone()).encode().into() };
		let weight_call = IXcm::IXcmCalls::weightMessage(weight_params);
		let xcm_weight_results =
			bare_call(to_fixed_non_zero(10)).data(weight_call.abi_encode()).build();

		let weight_result = match xcm_weight_results.result {
			Ok(value) => value,
			Err(_) => ExecReturnValue { flags: ReturnFlags::REVERT, data: Vec::new() },
		};

		let weight: IXcm::Weight =
			IXcm::Weight::abi_decode(&weight_result.data[..], true).expect("Failed to weight");

		let xcm_execute_params =
			IXcm::xcmExecuteCall { message: VersionedXcm::V4(message).encode().into(), weight };

		let call = IXcm::IXcmCalls::xcmExecute(xcm_execute_params);
		let encoded_call = call.abi_encode();

		bare_call(to_fixed_non_zero(10)).data(encoded_call).build();

		let final_alice_balance = ParachainBalances::free_balance(ALICE);
		let final_bob_balance = ParachainBalances::free_balance(BOB);

		assert_eq!(
			final_bob_balance,
			initial_bob_balance + amount,
			"Bob's balance should increase by the specified amount"
		);
		assert_eq!(
			final_alice_balance,
			initial_alice_balance - amount,
			"Alice's balance should decrease by the specified amount"
		);
	});
}

#[test]
fn test_xcm_send_precompile() {
	MockNet::reset();
	let amount = 1_000 * CENTS;
	let fee: u128 = parachain::estimate_message_fee(4);

	let sovereign_account_id = ParaA::execute_with(|| {
		let sovereign_account_id = parachain_account_sovereign_account_id(1, ALICE.clone());
		let initial_sovereign_balance =
			Relay::execute_with(|| relay_chain::Balances::free_balance(&sovereign_account_id));

		let initial_alice_relay_balance =
			Relay::execute_with(|| relay_chain::Balances::free_balance(ALICE));

		let dest = VersionedLocation::V4(Parent.into());
		let assets: Asset = (Here, amount).into();
		let beneficiary = AccountId32 { network: None, id: ALICE.clone().into() };

		let message: Xcm<()> = Xcm::builder()
			.withdraw_asset(assets.clone())
			.buy_execution((Here, fee), Unlimited)
			.deposit_asset(assets, beneficiary)
			.build();

		let xcm_send_params = IXcm::xcmSendCall {
			destination: dest.encode().into(),
			message: VersionedXcm::V4(message).encode().into(),
		};

		let call = IXcm::IXcmCalls::xcmSend(xcm_send_params);
		let encoded_call = call.abi_encode();
		let results = bare_call(to_fixed_non_zero(10)).data(encoded_call).build();
		let result = results.result.expect("Transaction should succeed");
		let mut data = &result.data[..];
		XcmHash::decode(&mut data).expect("Failed to decode xcm_send message_id");

		(sovereign_account_id, initial_sovereign_balance, initial_alice_relay_balance)
	});

	Relay::execute_with(|| {
		let (sovereign_account_id, initial_sovereign_balance, initial_alice_relay_balance) =
			sovereign_account_id;

		let final_sovereign_balance = relay_chain::Balances::free_balance(&sovereign_account_id);
		assert_eq!(
			final_sovereign_balance,
			initial_sovereign_balance - amount,
			"Sovereign account balance should decrease by the amount sent"
		);

		let final_alice_balance = relay_chain::Balances::free_balance(ALICE);
		assert_eq!(
			final_alice_balance,
			initial_alice_relay_balance + amount - fee,
			"Alice's balance should increase by amount minus fee"
		);
	});
}

#[test]
fn test_xcm_send_precompile_via_fixture() {
	MockNet::reset();
	let amount = 1_000 * CENTS;
	let fee: u128 = parachain::estimate_message_fee(4);
	let Contract { addr, .. } = instantiate_test_contract("call_and_return");

	ParaA::execute_with(|| {
		let dest = VersionedLocation::V4(Parent.into());
		let assets: Asset = (Here, amount).into();
		let beneficiary = AccountId32 { network: None, id: ALICE.clone().into() };

		let message: Xcm<()> = Xcm::builder()
			.withdraw_asset(assets.clone())
			.buy_execution((Here, fee), Unlimited)
			.deposit_asset(assets, beneficiary)
			.build();

		let xcm_send_params = IXcm::xcmSendCall {
			destination: dest.encode().into(),
			message: VersionedXcm::V4(message).encode().into(),
		};

		let call = IXcm::IXcmCalls::xcmSend(xcm_send_params);
		let encoded_call = call.abi_encode();
		let result = bare_call(addr)
			.data(
				(to_fixed_non_zero(10), 5000u64)
					.encode()
					.into_iter()
					.chain(encoded_call)
					.collect::<Vec<_>>(),
			)
			.build_and_unwrap_result();

		let mut data = &result.data[..];
		XcmHash::decode(&mut data).expect("Failed to decode xcm_send message_id");
	});

	Relay::execute_with(|| {
		assert_eq!(INITIAL_BALANCE + amount - fee, relay_chain::Balances::free_balance(ALICE));
	});
}
