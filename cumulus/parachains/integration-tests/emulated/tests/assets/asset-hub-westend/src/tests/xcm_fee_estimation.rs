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

//! Tests to ensure correct XCM fee estimation for cross-chain asset transfers.

use crate::imports::*;

use emulated_integration_tests_common::test_can_estimate_and_pay_exact_fees;
use frame_support::dispatch::RawOrigin;
use xcm_runtime_apis::{
	dry_run::runtime_decl_for_dry_run_api::DryRunApiV1,
	fees::runtime_decl_for_xcm_payment_api::XcmPaymentApiV1,
};

fn sender_assertions(test: ParaToParaThroughAHTest) {
	type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
	PenpalA::assert_xcm_pallet_attempted_complete(None);

	assert_expected_events!(
		PenpalA,
		vec![
			RuntimeEvent::ForeignAssets(
				pallet_assets::Event::Burned { asset_id, owner, balance }
			) => {
				asset_id: *asset_id == Location::new(1, []),
				owner: *owner == test.sender.account_id,
				balance: *balance == test.args.amount,
			},
		]
	);
}

fn hop_assertions(test: ParaToParaThroughAHTest) {
	type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
	AssetHubWestend::assert_xcmp_queue_success(None);

	assert_expected_events!(
		AssetHubWestend,
		vec![
			RuntimeEvent::Balances(
				pallet_balances::Event::Burned { amount, .. }
			) => {
				amount: *amount == test.args.amount,
			},
		]
	);
}

fn receiver_assertions(test: ParaToParaThroughAHTest) {
	type RuntimeEvent = <PenpalB as Chain>::RuntimeEvent;
	PenpalB::assert_xcmp_queue_success(None);

	assert_expected_events!(
		PenpalB,
		vec![
			RuntimeEvent::ForeignAssets(
				pallet_assets::Event::Issued { asset_id, owner, .. }
			) => {
				asset_id: *asset_id == Location::new(1, []),
				owner: *owner == test.receiver.account_id,
			},
		]
	);
}

fn transfer_assets_para_to_para_through_ah_call(
	test: ParaToParaThroughAHTest,
) -> <PenpalA as Chain>::RuntimeCall {
	type RuntimeCall = <PenpalA as Chain>::RuntimeCall;

	let asset_hub_location: Location = PenpalA::sibling_location_of(AssetHubWestend::para_id());
	let custom_xcm_on_dest = Xcm::<()>(vec![DepositAsset {
		assets: Wild(AllCounted(test.args.assets.len() as u32)),
		beneficiary: test.args.beneficiary,
	}]);
	RuntimeCall::PolkadotXcm(pallet_xcm::Call::transfer_assets_using_type_and_then {
		dest: bx!(test.args.dest.into()),
		assets: bx!(test.args.assets.clone().into()),
		assets_transfer_type: bx!(TransferType::RemoteReserve(asset_hub_location.clone().into())),
		remote_fees_id: bx!(VersionedAssetId::from(AssetId(Location::parent()))),
		fees_transfer_type: bx!(TransferType::RemoteReserve(asset_hub_location.into())),
		custom_xcm_on_dest: bx!(VersionedXcm::from(custom_xcm_on_dest)),
		weight_limit: test.args.weight_limit,
	})
}

/// We are able to dry-run and estimate the fees for a multi-hop XCM journey.
/// Scenario: Alice on PenpalA has some WND and wants to send them to PenpalB.
/// We want to know the fees using the `DryRunApi` and `XcmPaymentApi`.
#[test]
fn multi_hop_works() {
	let destination = PenpalA::sibling_location_of(PenpalB::para_id());
	let sender = PenpalASender::get();
	let amount_to_send = 1_000_000_000_000;
	let asset_owner = PenpalAssetOwner::get();
	let assets: Assets = (Parent, amount_to_send).into();
	let relay_native_asset_location = Location::parent();
	let sender_as_seen_by_ah = AssetHubWestend::sibling_location_of(PenpalA::para_id());
	let sov_of_sender_on_ah =
		AssetHubWestend::sovereign_account_id_of(sender_as_seen_by_ah.clone());

	// fund Parachain's sender account
	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(asset_owner.clone()),
		relay_native_asset_location.clone(),
		sender.clone(),
		amount_to_send * 2,
	);

	// fund the Parachain Origin's SA on AssetHub with the native tokens held in reserve.
	AssetHubWestend::fund_accounts(vec![(sov_of_sender_on_ah.clone(), amount_to_send * 2)]);

	// Init values for Parachain Destination
	let beneficiary_id = PenpalBReceiver::get();

	let test_args = TestContext {
		sender: PenpalASender::get(),     // Bob in PenpalB.
		receiver: PenpalBReceiver::get(), // Alice.
		args: TestArgs::new_para(
			destination,
			beneficiary_id.clone(),
			amount_to_send,
			assets,
			None,
			0,
		),
	};
	let mut test = ParaToParaThroughAHTest::new(test_args);

	// We get them from the PenpalA closure.
	let mut delivery_fees_amount = 0;
	let mut remote_message = VersionedXcm::from(Xcm(Vec::new()));
	<PenpalA as TestExt>::execute_with(|| {
		type Runtime = <PenpalA as Chain>::Runtime;
		type OriginCaller = <PenpalA as Chain>::OriginCaller;

		let call = transfer_assets_para_to_para_through_ah_call(test.clone());
		let origin = OriginCaller::system(RawOrigin::Signed(sender.clone()));
		let result = Runtime::dry_run_call(origin, call).unwrap();
		// We filter the result to get only the messages we are interested in.
		let (destination_to_query, messages_to_query) = &result
			.forwarded_xcms
			.iter()
			.find(|(destination, _)| {
				*destination == VersionedLocation::from(Location::new(1, [Parachain(1000)]))
			})
			.unwrap();
		assert_eq!(messages_to_query.len(), 1);
		remote_message = messages_to_query[0].clone();
		let delivery_fees =
			Runtime::query_delivery_fees(destination_to_query.clone(), remote_message.clone())
				.unwrap();
		delivery_fees_amount = get_amount_from_versioned_assets(delivery_fees);
	});

	// These are set in the AssetHub closure.
	let mut intermediate_execution_fees = 0;
	let mut intermediate_delivery_fees_amount = 0;
	let mut intermediate_remote_message = VersionedXcm::from(Xcm::<()>(Vec::new()));
	<AssetHubWestend as TestExt>::execute_with(|| {
		type Runtime = <AssetHubWestend as Chain>::Runtime;
		type RuntimeCall = <AssetHubWestend as Chain>::RuntimeCall;

		// First we get the execution fees.
		let weight = Runtime::query_xcm_weight(remote_message.clone()).unwrap();
		intermediate_execution_fees = Runtime::query_weight_to_asset_fee(
			weight,
			VersionedAssetId::from(AssetId(Location::new(1, []))),
		)
		.unwrap();

		// We have to do this to turn `VersionedXcm<()>` into `VersionedXcm<RuntimeCall>`.
		let xcm_program = VersionedXcm::from(Xcm::<RuntimeCall>::from(
			remote_message.clone().try_into().unwrap(),
		));

		// Now we get the delivery fees to the final destination.
		let result =
			Runtime::dry_run_xcm(sender_as_seen_by_ah.clone().into(), xcm_program).unwrap();
		let (destination_to_query, messages_to_query) = &result
			.forwarded_xcms
			.iter()
			.find(|(destination, _)| {
				*destination == VersionedLocation::from(Location::new(1, [Parachain(2001)]))
			})
			.unwrap();
		// There's actually two messages here.
		// One created when the message we sent from PenpalA arrived and was executed.
		// The second one when we dry-run the xcm.
		// We could've gotten the message from the queue without having to dry-run, but
		// offchain applications would have to dry-run, so we do it here as well.
		intermediate_remote_message = messages_to_query[0].clone();
		let delivery_fees = Runtime::query_delivery_fees(
			destination_to_query.clone(),
			intermediate_remote_message.clone(),
		)
		.unwrap();
		intermediate_delivery_fees_amount = get_amount_from_versioned_assets(delivery_fees);
	});

	// Get the final execution fees in the destination.
	let mut final_execution_fees = 0;
	<PenpalB as TestExt>::execute_with(|| {
		type Runtime = <PenpalA as Chain>::Runtime;

		let weight = Runtime::query_xcm_weight(intermediate_remote_message.clone()).unwrap();
		final_execution_fees =
			Runtime::query_weight_to_asset_fee(weight, VersionedAssetId::from(Location::parent()))
				.unwrap();
	});

	// Dry-running is done.
	PenpalA::reset_ext();
	AssetHubWestend::reset_ext();
	PenpalB::reset_ext();

	// Fund accounts again.
	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(asset_owner),
		relay_native_asset_location.clone(),
		sender.clone(),
		amount_to_send * 2,
	);
	AssetHubWestend::fund_accounts(vec![(sov_of_sender_on_ah, amount_to_send * 2)]);

	// Actually run the extrinsic.
	let sender_assets_before = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(relay_native_asset_location.clone(), &sender)
	});
	let receiver_assets_before = PenpalB::execute_with(|| {
		type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(relay_native_asset_location.clone(), &beneficiary_id)
	});

	test.set_assertion::<PenpalA>(sender_assertions);
	test.set_assertion::<AssetHubWestend>(hop_assertions);
	test.set_assertion::<PenpalB>(receiver_assertions);
	let call = transfer_assets_para_to_para_through_ah_call(test.clone());
	test.set_call(call);
	test.assert();

	let sender_assets_after = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(relay_native_asset_location.clone(), &sender)
	});
	let receiver_assets_after = PenpalB::execute_with(|| {
		type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(relay_native_asset_location, &beneficiary_id)
	});

	// We know the exact fees on every hop.
	assert_eq!(
		sender_assets_after,
		sender_assets_before - amount_to_send - delivery_fees_amount /* This is charged directly
		                                                              * from the sender's
		                                                              * account. */
	);
	assert_eq!(
		receiver_assets_after,
		receiver_assets_before + amount_to_send -
			intermediate_execution_fees -
			intermediate_delivery_fees_amount -
			final_execution_fees
	);
}

#[test]
fn multi_hop_pay_fees_works() {
	test_can_estimate_and_pay_exact_fees!(
		PenpalA,
		AssetHubWestend,
		PenpalB,
		(Parent, 1_000_000_000_000u128),
		Penpal
	);
}
