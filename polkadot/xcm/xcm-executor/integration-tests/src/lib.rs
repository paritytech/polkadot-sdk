// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg(test)]

use codec::Encode;
use frame_support::{dispatch::GetDispatchInfo, weights::Weight};
use polkadot_service::chain_spec::get_account_id_from_seed;
use polkadot_test_client::{
	BlockBuilderExt, ClientBlockImportExt, DefaultTestClientBuilderExt, InitPolkadotBlockBuilder,
	TestClientBuilder, TestClientBuilderExt,
};
use polkadot_test_runtime::{pallet_test_notifier, xcm_config::XcmConfig};
use polkadot_test_service::construct_extrinsic;
use sp_core::sr25519;
use sp_runtime::traits::Block;
use sp_state_machine::InspectState;
use xcm::{latest::prelude::*, VersionedResponse, VersionedXcm};
use xcm_executor::traits::WeightBounds;

#[test]
fn basic_buy_fees_message_executes() {
	sp_tracing::try_init_simple();
	let mut client = TestClientBuilder::new().build();

	let msg = Xcm(vec![
		WithdrawAsset((Parent, 100).into()),
		BuyExecution { fees: (Parent, 1).into(), weight_limit: Unlimited },
		DepositAsset { assets: Wild(AllCounted(1)), beneficiary: Parent.into() },
	]);

	let mut block_builder = client.init_polkadot_block_builder();

	let execute = construct_extrinsic(
		&client,
		polkadot_test_runtime::RuntimeCall::Xcm(pallet_xcm::Call::execute {
			message: Box::new(VersionedXcm::from(msg)),
			max_weight: Weight::from_parts(1_000_000_000, 1024 * 1024),
		}),
		sp_keyring::Sr25519Keyring::Alice,
		0,
	);

	block_builder.push_polkadot_extrinsic(execute).expect("pushes extrinsic");

	let block = block_builder.build().expect("Finalizes the block").block;
	let block_hash = block.hash();

	futures::executor::block_on(client.import(sp_consensus::BlockOrigin::Own, block))
		.expect("imports the block");

	client.state_at(block_hash).expect("state should exist").inspect_state(|| {
		assert!(polkadot_test_runtime::System::events().iter().any(|r| matches!(
			r.event,
			polkadot_test_runtime::RuntimeEvent::Xcm(pallet_xcm::Event::Attempted {
				outcome: Outcome::Complete { .. }
			}),
		)));
	});
}

#[test]
fn transact_recursion_limit_works() {
	sp_tracing::try_init_simple();
	let mut client = TestClientBuilder::new().build();

	let base_xcm = |call: polkadot_test_runtime::RuntimeCall| {
		Xcm(vec![
			WithdrawAsset((Here, 1_000).into()),
			BuyExecution { fees: (Here, 1).into(), weight_limit: Unlimited },
			Transact {
				origin_kind: OriginKind::Native,
				require_weight_at_most: call.get_dispatch_info().weight,
				call: call.encode().into(),
			},
		])
	};
	let mut call: Option<polkadot_test_runtime::RuntimeCall> = None;
	// set up transacts with recursive depth of 11
	for depth in (1..12).rev() {
		let mut msg;
		match depth {
			// this one should fail with `XcmError::ExceedsStackLimit`
			11 => {
				msg = Xcm(vec![ClearOrigin]);
			},
			// this one checks that the inner one (depth 11) fails as expected,
			// itself should not fail => should have outcome == Complete
			10 => {
				let inner_call = call.take().unwrap();
				let expected_transact_status =
					sp_runtime::DispatchError::Module(sp_runtime::ModuleError {
						index: 27,
						error: [24, 0, 0, 0],
						message: Some("LocalExecutionIncomplete"),
					})
					.encode()
					.into();
				msg = base_xcm(inner_call);
				msg.inner_mut().push(ExpectTransactStatus(expected_transact_status));
			},
			// these are the outer 9 calls that expect `ExpectTransactStatus(Success)`
			d if d >= 1 && d <= 9 => {
				let inner_call = call.take().unwrap();
				msg = base_xcm(inner_call);
				msg.inner_mut().push(ExpectTransactStatus(MaybeErrorCode::Success));
			},
			_ => unreachable!(),
		}
		let max_weight = <XcmConfig as xcm_executor::Config>::Weigher::weight(&mut msg).unwrap();
		call = Some(polkadot_test_runtime::RuntimeCall::Xcm(pallet_xcm::Call::execute {
			message: Box::new(VersionedXcm::from(msg.clone())),
			max_weight,
		}));
	}

	let mut block_builder = client.init_polkadot_block_builder();

	let execute = construct_extrinsic(&client, call.unwrap(), sp_keyring::Sr25519Keyring::Alice, 0);

	block_builder.push_polkadot_extrinsic(execute).expect("pushes extrinsic");

	let block = block_builder.build().expect("Finalizes the block").block;
	let block_hash = block.hash();

	futures::executor::block_on(client.import(sp_consensus::BlockOrigin::Own, block))
		.expect("imports the block");

	client.state_at(block_hash).expect("state should exist").inspect_state(|| {
		let events = polkadot_test_runtime::System::events();
		// verify 10 pallet_xcm calls were successful
		assert_eq!(
			polkadot_test_runtime::System::events()
				.iter()
				.filter(|r| matches!(
					r.event,
					polkadot_test_runtime::RuntimeEvent::Xcm(pallet_xcm::Event::Attempted {
						outcome: Outcome::Complete { .. }
					}),
				))
				.count(),
			10
		);
		// verify transaction fees have been paid
		assert!(events.iter().any(|r| matches!(
			&r.event,
			polkadot_test_runtime::RuntimeEvent::TransactionPayment(
				pallet_transaction_payment::Event::TransactionFeePaid {
					who: payer,
					..
				}
			) if *payer == sp_keyring::Sr25519Keyring::Alice.into(),
		)));
	});
}

#[test]
fn query_response_fires() {
	use pallet_test_notifier::Event::*;
	use pallet_xcm::QueryStatus;
	use polkadot_test_runtime::RuntimeEvent::TestNotifier;

	sp_tracing::try_init_simple();
	let mut client = TestClientBuilder::new().build();

	let mut block_builder = client.init_polkadot_block_builder();

	let execute = construct_extrinsic(
		&client,
		polkadot_test_runtime::RuntimeCall::TestNotifier(
			pallet_test_notifier::Call::prepare_new_query {},
		),
		sp_keyring::Sr25519Keyring::Alice,
		0,
	);

	block_builder.push_polkadot_extrinsic(execute).expect("pushes extrinsic");

	let block = block_builder.build().expect("Finalizes the block").block;
	let block_hash = block.hash();

	futures::executor::block_on(client.import(sp_consensus::BlockOrigin::Own, block))
		.expect("imports the block");

	let mut query_id = None;
	client.state_at(block_hash).expect("state should exist").inspect_state(|| {
		for r in polkadot_test_runtime::System::events().iter() {
			match r.event {
				TestNotifier(QueryPrepared(q)) => query_id = Some(q),
				_ => (),
			}
		}
	});
	let query_id = query_id.unwrap();

	let mut block_builder = client.init_polkadot_block_builder();

	let response = Response::ExecutionResult(None);
	let max_weight = Weight::from_parts(1_000_000, 1024 * 1024);
	let querier = Some(Here.into());
	let msg = Xcm(vec![QueryResponse { query_id, response, max_weight, querier }]);
	let msg = Box::new(VersionedXcm::from(msg));

	let execute = construct_extrinsic(
		&client,
		polkadot_test_runtime::RuntimeCall::Xcm(pallet_xcm::Call::execute {
			message: msg,
			max_weight: Weight::from_parts(1_000_000_000, 1024 * 1024),
		}),
		sp_keyring::Sr25519Keyring::Alice,
		1,
	);

	block_builder.push_polkadot_extrinsic(execute).expect("pushes extrinsic");

	let block = block_builder.build().expect("Finalizes the block").block;
	let block_hash = block.hash();

	futures::executor::block_on(client.import(sp_consensus::BlockOrigin::Own, block))
		.expect("imports the block");

	client.state_at(block_hash).expect("state should exist").inspect_state(|| {
		assert!(polkadot_test_runtime::System::events().iter().any(|r| matches!(
			r.event,
			polkadot_test_runtime::RuntimeEvent::Xcm(pallet_xcm::Event::ResponseReady {
				query_id: q,
				response: Response::ExecutionResult(None),
			}) if q == query_id,
		)));
		assert_eq!(
			polkadot_test_runtime::Xcm::query(query_id),
			Some(QueryStatus::Ready {
				response: VersionedResponse::V4(Response::ExecutionResult(None)),
				at: 2u32.into()
			}),
		)
	});
}

#[test]
fn query_response_elicits_handler() {
	use pallet_test_notifier::Event::*;
	use polkadot_test_runtime::RuntimeEvent::TestNotifier;

	sp_tracing::try_init_simple();
	let mut client = TestClientBuilder::new().build();

	let mut block_builder = client.init_polkadot_block_builder();

	let execute = construct_extrinsic(
		&client,
		polkadot_test_runtime::RuntimeCall::TestNotifier(
			pallet_test_notifier::Call::prepare_new_notify_query {},
		),
		sp_keyring::Sr25519Keyring::Alice,
		0,
	);

	block_builder.push_polkadot_extrinsic(execute).expect("pushes extrinsic");

	let block = block_builder.build().expect("Finalizes the block").block;
	let block_hash = block.hash();

	futures::executor::block_on(client.import(sp_consensus::BlockOrigin::Own, block))
		.expect("imports the block");

	let mut query_id = None;
	client.state_at(block_hash).expect("state should exist").inspect_state(|| {
		for r in polkadot_test_runtime::System::events().iter() {
			match r.event {
				TestNotifier(NotifyQueryPrepared(q)) => query_id = Some(q),
				_ => (),
			}
		}
	});
	let query_id = query_id.unwrap();

	let mut block_builder = client.init_polkadot_block_builder();

	let response = Response::ExecutionResult(None);
	let max_weight = Weight::from_parts(1_000_000, 1024 * 1024);
	let querier = Some(Here.into());
	let msg = Xcm(vec![QueryResponse { query_id, response, max_weight, querier }]);

	let execute = construct_extrinsic(
		&client,
		polkadot_test_runtime::RuntimeCall::Xcm(pallet_xcm::Call::execute {
			message: Box::new(VersionedXcm::from(msg)),
			max_weight: Weight::from_parts(1_000_000_000, 1024 * 1024),
		}),
		sp_keyring::Sr25519Keyring::Alice,
		1,
	);

	block_builder.push_polkadot_extrinsic(execute).expect("pushes extrinsic");

	let block = block_builder.build().expect("Finalizes the block").block;
	let block_hash = block.hash();

	futures::executor::block_on(client.import(sp_consensus::BlockOrigin::Own, block))
		.expect("imports the block");

	client.state_at(block_hash).expect("state should exist").inspect_state(|| {
		assert!(polkadot_test_runtime::System::events().iter().any(|r| matches!(
			&r.event,
			TestNotifier(ResponseReceived(
				location,
				q,
				Response::ExecutionResult(None),
			)) if *q == query_id && matches!(location.unpack(), (0, [Junction::AccountId32 { .. }])),
		)));
	});
}

/// Simulates a cross-chain message from Parachain to Parachain through Relay Chain
/// that deposits assets into the reserve of the destination.
/// Regression test for `DepositReserveAsset` changes in
/// <https://github.com/paritytech/polkadot-sdk/pull/3340>
#[test]
fn deposit_reserve_asset_works_for_any_xcm_sender() {
	sp_tracing::try_init_simple();
	let mut client = TestClientBuilder::new().build();

	// Init values for the simulated origin Parachain
	let amount_to_send: u128 = 1_000_000_000_000;
	let assets: Assets = (Parent, amount_to_send).into();
	let fee_asset_item = 0;
	let max_assets = assets.len() as u32;
	let fees = assets.get(fee_asset_item as usize).unwrap().clone();
	let weight_limit = Unlimited;
	let reserve = Location::parent();
	let dest = Location::new(1, [Parachain(2000)]);
	let beneficiary_id = get_account_id_from_seed::<sr25519::Public>("Alice");
	let beneficiary = Location::new(0, [AccountId32 { network: None, id: beneficiary_id.into() }]);

	// spends up to half of fees for execution on reserve and other half for execution on
	// destination
	let fee1 = amount_to_send.saturating_div(2);
	let fee2 = amount_to_send.saturating_sub(fee1);
	let fees_half_1 = Asset::from((fees.id.clone(), Fungible(fee1)));
	let fees_half_2 = Asset::from((fees.id.clone(), Fungible(fee2)));

	let reserve_context = <XcmConfig as xcm_executor::Config>::UniversalLocation::get();
	// identifies fee item as seen by `reserve` - to be used at reserve chain
	let reserve_fees = fees_half_1.reanchored(&reserve, &reserve_context).unwrap();
	// identifies fee item as seen by `dest` - to be used at destination chain
	let dest_fees = fees_half_2.reanchored(&dest, &reserve_context).unwrap();
	// identifies assets as seen by `reserve` - to be used at reserve chain
	let assets_reanchored = assets.reanchored(&reserve, &reserve_context).unwrap();
	// identifies `dest` as seen by `reserve`
	let dest = dest.reanchored(&reserve, &reserve_context).unwrap();
	// xcm to be executed at dest
	let xcm_on_dest = Xcm(vec![
		BuyExecution { fees: dest_fees, weight_limit: weight_limit.clone() },
		DepositAsset { assets: Wild(AllCounted(max_assets)), beneficiary },
	]);
	// xcm to be executed at reserve
	let msg = Xcm(vec![
		WithdrawAsset(assets_reanchored),
		ClearOrigin,
		BuyExecution { fees: reserve_fees, weight_limit },
		DepositReserveAsset { assets: Wild(AllCounted(max_assets)), dest, xcm: xcm_on_dest },
	]);

	let mut block_builder = client.init_polkadot_block_builder();

	// Simulate execution of an incoming XCM message at the reserve chain
	let execute = construct_extrinsic(
		&client,
		polkadot_test_runtime::RuntimeCall::Xcm(pallet_xcm::Call::execute {
			message: Box::new(VersionedXcm::from(msg)),
			max_weight: Weight::from_parts(1_000_000_000, 1024 * 1024),
		}),
		sp_keyring::Sr25519Keyring::Alice,
		0,
	);

	block_builder.push_polkadot_extrinsic(execute).expect("pushes extrinsic");

	let block = block_builder.build().expect("Finalizes the block").block;
	let block_hash = block.hash();

	futures::executor::block_on(client.import(sp_consensus::BlockOrigin::Own, block))
		.expect("imports the block");

	client.state_at(block_hash).expect("state should exist").inspect_state(|| {
		assert!(polkadot_test_runtime::System::events().iter().any(|r| matches!(
			r.event,
			polkadot_test_runtime::RuntimeEvent::Xcm(pallet_xcm::Event::Attempted {
				outcome: Outcome::Complete { .. }
			}),
		)));
	});
}
