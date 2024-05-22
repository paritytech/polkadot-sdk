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

use sp_keyring::AccountKeyring::Alice;
use sp_runtime::{generic, MultiSignature};
use xcm_fee_payment_runtime_api::{
	dry_run::runtime_decl_for_xcm_dry_run_api::XcmDryRunApiV1,
	fees::runtime_decl_for_xcm_payment_api::XcmPaymentApiV1,
};

/// We are able to dry-run and estimate the fees for a teleport between relay and system para.
/// Scenario: Alice on Westend relay chain wants to teleport WND to Asset Hub.
/// We want to know the fees using the `XcmDryRunApi` and `XcmPaymentApi`.
#[test]
fn teleport_relay_system_para_works() {
	let destination: Location = Parachain(1000).into(); // Asset Hub.
	let beneficiary_id = AssetHubWestendReceiver::get();
	let beneficiary: Location = AccountId32 { id: beneficiary_id.clone().into(), network: None } // Test doesn't allow specifying a network here.
		.into(); // Beneficiary in Asset Hub.
	let teleport_amount = 1_000_000_000_000; // One WND (12 decimals).
	let assets: Assets = vec![(Here, teleport_amount).into()].into();

	// We get them from the Westend closure.
	let mut delivery_fees_amount = 0;
	let mut remote_message = VersionedXcm::V4(Xcm(Vec::new()));
	<Westend as TestExt>::new_ext().execute_with(|| {
		type Runtime = <Westend as Chain>::Runtime;
		type RuntimeCall = <Westend as Chain>::RuntimeCall;

		let call = RuntimeCall::XcmPallet(pallet_xcm::Call::transfer_assets {
			dest: Box::new(VersionedLocation::V4(destination.clone())),
			beneficiary: Box::new(VersionedLocation::V4(beneficiary)),
			assets: Box::new(VersionedAssets::V4(assets)),
			fee_asset_item: 0,
			weight_limit: Unlimited,
		});
		let sender = Alice; // Is the same as `WestendSender`.
		let extrinsic = construct_extrinsic_westend(sender, call);
		let result = Runtime::dry_run_extrinsic(extrinsic).unwrap();
		assert_eq!(result.forwarded_xcms.len(), 1);
		let (destination_to_query, messages_to_query) = &result.forwarded_xcms[0];
		assert_eq!(messages_to_query.len(), 1);
		remote_message = messages_to_query[0].clone();
		let delivery_fees =
			Runtime::query_delivery_fees(destination_to_query.clone(), remote_message.clone())
				.unwrap();
		delivery_fees_amount = get_amount_from_versioned_assets(delivery_fees);
	});

	// This is set in the AssetHubWestend closure.
	let mut remote_execution_fees = 0;
	<AssetHubWestend as TestExt>::execute_with(|| {
		type Runtime = <AssetHubWestend as Chain>::Runtime;

		let weight = Runtime::query_xcm_weight(remote_message.clone()).unwrap();
		remote_execution_fees =
			Runtime::query_weight_to_asset_fee(weight, VersionedAssetId::V4(Parent.into()))
				.unwrap();
	});

	let test_args = TestContext {
		sender: WestendSender::get(),             // Alice.
		receiver: AssetHubWestendReceiver::get(), // Bob in Asset Hub.
		args: TestArgs::new_relay(destination, beneficiary_id, teleport_amount),
	};
	let mut test = RelayToSystemParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;
	assert_eq!(sender_balance_before, 1_000_000_000_000_000_000);
	assert_eq!(receiver_balance_before, 4_096_000_000_000);

	test.set_dispatchable::<Westend>(transfer_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	// We now know the exact fees.
	assert_eq!(
		sender_balance_after,
		sender_balance_before - delivery_fees_amount - teleport_amount
	);
	assert_eq!(
		receiver_balance_after,
		receiver_balance_before + teleport_amount - remote_execution_fees
	);
}

/// We are able to dry-run and estimate the fees for a multi-hop XCM journey.
/// Scenario: Alice on PenpalA has some WND and wants to send them to PenpalB.
/// We want to know the fees using the `XcmDryRunApi` and `XcmPaymentApi`.
#[test]
fn multi_hop_works() {
	let destination = PenpalA::sibling_location_of(PenpalB::para_id());
	let sender = PenpalASender::get();
	let amount_to_send = 1_000_000_000_000; // One WND (12 decimals).
	let asset_owner = PenpalAssetOwner::get();
	let assets: Assets = (Parent, amount_to_send).into();
	let relay_native_asset_location = RelayLocation::get();
	let sender_as_seen_by_relay = Westend::child_location_of(PenpalA::para_id());
	let sov_of_sender_on_relay = Westend::sovereign_account_id_of(sender_as_seen_by_relay.clone());

	// fund Parachain's sender account
	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(asset_owner.clone()),
		relay_native_asset_location.clone(),
		sender.clone(),
		amount_to_send * 2,
	);

	// fund the Parachain Origin's SA on Relay Chain with the native tokens held in reserve
	Westend::fund_accounts(vec![(sov_of_sender_on_relay.clone().into(), amount_to_send * 2)]);

	// Init values for Parachain Destination
	let beneficiary_id = PenpalBReceiver::get();
	let beneficiary: Location = AccountId32 {
		id: beneficiary_id.clone().into(),
		network: None, // Test doesn't allow specifying a network here.
	}
	.into();

	// We get them from the PenpalA closure.
	let mut delivery_fees_amount = 0;
	let mut remote_message = VersionedXcm::V4(Xcm(Vec::new()));
	<PenpalA as TestExt>::execute_with(|| {
		type Runtime = <PenpalA as Chain>::Runtime;
		type RuntimeCall = <PenpalA as Chain>::RuntimeCall;

		let call = RuntimeCall::PolkadotXcm(pallet_xcm::Call::transfer_assets {
			dest: Box::new(VersionedLocation::V4(destination.clone())),
			beneficiary: Box::new(VersionedLocation::V4(beneficiary)),
			assets: Box::new(VersionedAssets::V4(assets.clone())),
			fee_asset_item: 0,
			weight_limit: Unlimited,
		});
		let sender = Alice; // Same as `PenpalASender`.
		let extrinsic = construct_extrinsic_penpal(sender, call);
		let result = Runtime::dry_run_extrinsic(extrinsic).unwrap();
		assert_eq!(result.forwarded_xcms.len(), 1);
		let (destination_to_query, messages_to_query) = &result.forwarded_xcms[0];
		assert_eq!(messages_to_query.len(), 1);
		remote_message = messages_to_query[0].clone();
		let delivery_fees =
			Runtime::query_delivery_fees(destination_to_query.clone(), remote_message.clone())
				.unwrap();
		delivery_fees_amount = get_amount_from_versioned_assets(delivery_fees);
	});

	// This is set in the Westend closure.
	let mut intermediate_execution_fees = 0;
	let mut intermediate_delivery_fees_amount = 0;
	let mut intermediate_remote_message = VersionedXcm::V4(Xcm::<()>(Vec::new()));
	<Westend as TestExt>::execute_with(|| {
		type Runtime = <Westend as Chain>::Runtime;
		type RuntimeCall = <Westend as Chain>::RuntimeCall;

		// First we get the execution fees.
		let weight = Runtime::query_xcm_weight(remote_message.clone()).unwrap();
		intermediate_execution_fees =
			Runtime::query_weight_to_asset_fee(weight, VersionedAssetId::V4(Here.into())).unwrap();

		// We have to do this to turn `VersionedXcm<()>` into `VersionedXcm<RuntimeCall>`.
		let xcm_program =
			VersionedXcm::V4(Xcm::<RuntimeCall>::from(remote_message.clone().try_into().unwrap()));

		// Now we get the delivery fees to the final destination.
		let result =
			Runtime::dry_run_xcm(sender_as_seen_by_relay.clone().into(), xcm_program).unwrap();
		let (destination_to_query, messages_to_query) = &result.forwarded_xcms[0];
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
		type Runtime = <PenpalB as Chain>::Runtime;

		let weight = Runtime::query_xcm_weight(intermediate_remote_message.clone()).unwrap();
		final_execution_fees =
			Runtime::query_weight_to_asset_fee(weight, VersionedAssetId::V4(Parent.into()))
				.unwrap();
	});

	// Dry-running is done.
	PenpalA::reset_ext();
	Westend::reset_ext();
	PenpalB::reset_ext();

	// Fund accounts again.
	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(asset_owner),
		relay_native_asset_location.clone(),
		sender.clone(),
		amount_to_send * 2,
	);
	Westend::fund_accounts(vec![(sov_of_sender_on_relay.into(), amount_to_send * 2)]);

	// Actually run the extrinsic.
	let test_args = TestContext {
		sender: PenpalASender::get(),     // Alice.
		receiver: PenpalBReceiver::get(), // Bob in PenpalB.
		args: TestArgs::new_para(
			destination,
			beneficiary_id.clone(),
			amount_to_send,
			assets,
			None,
			0,
		),
	};
	let mut test = ParaToParaThroughRelayTest::new(test_args);

	let sender_assets_before = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(relay_native_asset_location.clone(), &sender)
	});
	let receiver_assets_before = PenpalB::execute_with(|| {
		type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(relay_native_asset_location.clone(), &beneficiary_id)
	});

	test.set_dispatchable::<PenpalA>(transfer_assets_para_to_para);
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

fn get_amount_from_versioned_assets(assets: VersionedAssets) -> u128 {
	let latest_assets: Assets = assets.try_into().unwrap();
	let Fungible(amount) = latest_assets.inner()[0].fun else {
		unreachable!("asset is fungible");
	};
	amount
}

fn transfer_assets(test: RelayToSystemParaTest) -> DispatchResult {
	<Westend as WestendPallet>::XcmPallet::transfer_assets(
		test.signed_origin,
		bx!(test.args.dest.into()),
		bx!(test.args.beneficiary.into()),
		bx!(test.args.assets.into()),
		test.args.fee_asset_item,
		test.args.weight_limit,
	)
}

fn transfer_assets_para_to_para(test: ParaToParaThroughRelayTest) -> DispatchResult {
	<PenpalA as PenpalAPallet>::PolkadotXcm::transfer_assets(
		test.signed_origin,
		bx!(test.args.dest.into()),
		bx!(test.args.beneficiary.into()),
		bx!(test.args.assets.into()),
		test.args.fee_asset_item,
		test.args.weight_limit,
	)
}

// Constructs the SignedExtra component of an extrinsic for the Westend runtime.
fn construct_extrinsic_westend(
	sender: sp_keyring::AccountKeyring,
	call: westend_runtime::RuntimeCall,
) -> westend_runtime::UncheckedExtrinsic {
	type Runtime = <Westend as Chain>::Runtime;
	let account_id = <Runtime as frame_system::Config>::AccountId::from(sender.public());
	let tip = 0;
	let extra: westend_runtime::SignedExtra = (
		frame_system::CheckNonZeroSender::<Runtime>::new(),
		frame_system::CheckSpecVersion::<Runtime>::new(),
		frame_system::CheckTxVersion::<Runtime>::new(),
		frame_system::CheckGenesis::<Runtime>::new(),
		frame_system::CheckMortality::<Runtime>::from(sp_runtime::generic::Era::immortal()),
		frame_system::CheckNonce::<Runtime>::from(
			frame_system::Pallet::<Runtime>::account(&account_id).nonce,
		),
		frame_system::CheckWeight::<Runtime>::new(),
		pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(tip),
		frame_metadata_hash_extension::CheckMetadataHash::<Runtime>::new(false),
	);
	let raw_payload = westend_runtime::SignedPayload::new(call, extra).unwrap();
	let signature = raw_payload.using_encoded(|payload| sender.sign(payload));
	let (call, extra, _) = raw_payload.deconstruct();
	westend_runtime::UncheckedExtrinsic::new_signed(
		call,
		account_id.into(),
		MultiSignature::Sr25519(signature),
		extra,
	)
}

// Constructs the SignedExtra component of an extrinsic for the Westend runtime.
fn construct_extrinsic_penpal(
	sender: sp_keyring::AccountKeyring,
	call: penpal_runtime::RuntimeCall,
) -> penpal_runtime::UncheckedExtrinsic {
	type Runtime = <PenpalA as Chain>::Runtime;
	let account_id = <Runtime as frame_system::Config>::AccountId::from(sender.public());
	let tip = 0;
	let extra: penpal_runtime::SignedExtra = (
		frame_system::CheckNonZeroSender::<Runtime>::new(),
		frame_system::CheckSpecVersion::<Runtime>::new(),
		frame_system::CheckTxVersion::<Runtime>::new(),
		frame_system::CheckGenesis::<Runtime>::new(),
		frame_system::CheckEra::<Runtime>::from(generic::Era::immortal()),
		frame_system::CheckNonce::<Runtime>::from(
			frame_system::Pallet::<Runtime>::account(&account_id).nonce,
		),
		frame_system::CheckWeight::<Runtime>::new(),
		pallet_asset_tx_payment::ChargeAssetTxPayment::<Runtime>::from(tip, None),
	);
	type SignedPayload =
		generic::SignedPayload<penpal_runtime::RuntimeCall, penpal_runtime::SignedExtra>;
	let raw_payload = SignedPayload::new(call, extra).unwrap();
	let signature = raw_payload.using_encoded(|payload| sender.sign(payload));
	let (call, extra, _) = raw_payload.deconstruct();
	penpal_runtime::UncheckedExtrinsic::new_signed(
		call,
		account_id.into(),
		MultiSignature::Sr25519(signature),
		extra,
	)
}
