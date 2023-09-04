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
	tests::{
		compile_module,
		mock_network::{
			parachain::{self, Runtime, RuntimeOrigin},
			parachain_account_sovereign_account_id,
			primitives::{AccountId, CENTS},
			relay_chain, MockNet, ParaA, ParachainBalances, ParachainPalletXcm, Relay, ALICE, BOB,
			INITIAL_BALANCE,
		},
	},
	xcm::XCM,
	CollectEvents, DebugInfo, Determinism,
};
use codec::{Decode, Encode};
use frame_support::{
	assert_ok,
	pallet_prelude::Weight,
	traits::{fungibles::Mutate, Currency},
};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_balances::{BalanceLock, Reasons};
use pallet_contracts_primitives::Code;
use xcm::{v3::prelude::*, VersionedMultiLocation, VersionedXcm};
use xcm_executor::traits::{QueryHandler, QueryResponseStatus};
use xcm_simulator::TestExt;

type ParachainContracts = crate::Pallet<parachain::Runtime>;
type QueryId = <<parachain::Runtime as crate::Config>::Xcm as XCM<parachain::Runtime>>::QueryId;

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
		let fee = parachain::estimate_message_fee(4);

		// The XCM used to transfer funds to Bob.
		let message: xcm_simulator::Xcm<()> = Xcm(vec![
			WithdrawAsset(vec![(Parent, fee).into()].into()),
			BuyExecution { fees: (Parent, fee).into(), weight_limit: WeightLimit::Unlimited },
			WithdrawAsset(vec![(Here, amount).into()].into()),
			DepositAsset {
				assets: All.into(),
				beneficiary: AccountId32 { network: None, id: BOB.clone().into() }.into(),
			},
		]);
		let message = VersionedXcm::V3(message);

		// Execute the XCM message, through the contract.
		let max_weight = Weight::from_all(10_000_000_000);
		let data = (max_weight, message.clone()).encode();

		assert_ok!(
			ParachainContracts::bare_call(
				ALICE,
				contract_addr.clone(),
				0,
				Weight::MAX,
				None,
				data,
				DebugInfo::UnsafeDebug,
				CollectEvents::UnsafeCollect,
				Determinism::Enforced,
			)
			.result
		);

		// Check if the funds are subtracted from the account of Alice and added to the account of
		// Bob.
		let initial = INITIAL_BALANCE;
		assert_eq!(parachain::Assets::balance(0, contract_addr), initial - fee);
		assert_eq!(ParachainBalances::free_balance(BOB), initial + amount);
	});
}

#[test]
fn test_xcm_send() {
	MockNet::reset();
	let contract_addr = instantiate_test_contract("xcm_send");
	let fee = parachain::estimate_message_fee(4); // Accounts for the `DescendOrigin` instruction added by `send_xcm`

	ParaA::execute_with(|| {
		let dest = MultiLocation::from(Parent);
		let dest = VersionedMultiLocation::V3(dest);

		let message: xcm_simulator::Xcm<()> = Xcm(vec![
			WithdrawAsset((Here, fee).into()),
			BuyExecution { fees: (Here, fee).into(), weight_limit: WeightLimit::Unlimited },
			LockAsset { asset: (Here, 5 * CENTS).into(), unlocker: (Parachain(1)).into() },
		]);
		let message = VersionedXcm::V3(message);

		println!("msg: \n{:?}", message.clone().encode());

		assert_ok!(
			ParachainContracts::bare_call(
				ALICE,
				contract_addr.clone(),
				0,
				Weight::MAX,
				None,
				(dest, message).encode(),
				DebugInfo::UnsafeDebug,
				CollectEvents::UnsafeCollect,
				Determinism::Enforced,
			)
			.result
		);
	});

	Relay::execute_with(|| {
		// Check if the funds are locked on the relay chain.
		assert_eq!(
			relay_chain::Balances::locks(&parachain_account_sovereign_account_id(1, contract_addr)),
			vec![BalanceLock { id: *b"py/xcmlk", amount: 5 * CENTS, reasons: Reasons::All }]
		);
	});
}

#[test]
fn test_xcm_query() {
	MockNet::reset();
	let contract_addr = instantiate_test_contract("xcm_query");

	ParaA::execute_with(|| {
		let match_querier = MultiLocation::from(AccountId32 { network: None, id: ALICE.into() });
		let match_querier = VersionedMultiLocation::V3(match_querier);
		let timeout: BlockNumberFor<parachain::Runtime> = 1u32.into();

		println!("timeout encoded: {:?}", timeout.encode());
		println!("match_querier encoded len: {:?}", match_querier.clone().encode().len());
		println!("encoded: {:?}", (timeout, match_querier.clone()).encode());
		let exec = ParachainContracts::bare_call(
			ALICE,
			contract_addr.clone(),
			0,
			Weight::MAX,
			None,
			(timeout, match_querier).encode(),
			DebugInfo::UnsafeDebug,
			CollectEvents::UnsafeCollect,
			Determinism::Enforced,
		);

		let mut data = &exec.result.unwrap().data[..];

		let query_id = QueryId::decode(&mut data).expect("Failed to decode message");
		let response = ParachainPalletXcm::take_response(query_id);
		let expected_response = QueryResponseStatus::Pending { timeout };
		assert_eq!(response, expected_response);
	});
}

#[test]
fn test_xcm_take_response() {
	MockNet::reset();
	let contract_addr = instantiate_test_contract("xcm_take_response");
	ParaA::execute_with(|| {
		let querier: MultiLocation =
			(Parent, AccountId32 { network: None, id: ALICE.into() }).into();
		let responder = MultiLocation::from(AccountId32 {
			network: Some(NetworkId::ByGenesis([0u8; 32])),
			id: ALICE.into(),
		});
		let query_id = ParachainPalletXcm::new_query(responder, 1u32.into(), querier);

		let fee = parachain::estimate_message_fee(4);
		let message = Xcm(vec![
			WithdrawAsset(vec![(Parent, fee).into()].into()),
			BuyExecution { fees: (Parent, fee).into(), weight_limit: WeightLimit::Unlimited },
			QueryResponse {
				query_id,
				response: Response::ExecutionResult(None),
				max_weight: Weight::zero(),
				querier: Some(querier),
			},
		]);

		let call = |query_id: QueryId| {
			let exec = ParachainContracts::bare_call(
				ALICE,
				contract_addr.clone(),
				0,
				Weight::MAX,
				None,
				query_id.encode(),
				DebugInfo::UnsafeDebug,
				CollectEvents::UnsafeCollect,
				Determinism::Enforced,
			);

			QueryResponseStatus::<BlockNumberFor<parachain::Runtime>>::decode(
				&mut &exec.result.unwrap().data[..],
			)
			.expect("Failed to decode message")
		};

		// Query is not yet answered.
		assert_eq!(QueryResponseStatus::Pending { timeout: 1u32.into() }, call(query_id));

		ParachainPalletXcm::execute(
			RuntimeOrigin::signed(ALICE),
			Box::new(VersionedXcm::V3(message)),
			Weight::from_parts(1_000_000_000, 1_000_000_000),
		)
		.unwrap();

		// Query is answered.
		assert_eq!(
			QueryResponseStatus::Ready {
				response: Response::ExecutionResult(None),
				at: 1u32.into()
			},
			call(query_id)
		);

		// Query is not found. (Query was already answered)
		assert_eq!(QueryResponseStatus::NotFound, call(query_id));
	})
}
