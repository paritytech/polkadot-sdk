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
	assets_balance_on, create_pool_with_wnd_on, foreign_balance_on,
	imports::{
		asset_hub_westend_runtime::{ExistentialDeposit, Runtime},
		*,
	},
};
use asset_hub_westend_runtime::{
	xcm_config::WestendLocation, Balances, ForeignAssets, PolkadotXcm, RuntimeOrigin,
};
use emulated_integration_tests_common::{accounts::ALICE, xcm_emulator::TestExt};
use frame_support::{
	assert_err_ignore_postinfo, assert_ok,
	traits::fungible::{Inspect, Mutate},
};
use parachains_common::{AccountId, Balance};
use sp_tracing::capture_test_logs;
use std::convert::Into;
use xcm::latest::{Assets, Error as XcmError, Location, Xcm};

const UNITS: Balance = 1_000_000_000;

#[test]
fn exchange_asset_success() {
	test_exchange_asset(true, 500 * UNITS, 665 * UNITS, None);
}

#[test]
fn exchange_asset_insufficient_liquidity() {
	let log_capture = capture_test_logs!({
		test_exchange_asset(
			true,
			1_000 * UNITS,
			2_000 * UNITS,
			Some(InstructionError { index: 1, error: XcmError::NoDeal }),
		);
	});
	assert!(log_capture.contains("NoDeal"));
}

#[test]
fn exchange_asset_insufficient_balance() {
	let log_capture = capture_test_logs!({
		test_exchange_asset(
			true,
			5_000 * UNITS,
			1_665 * UNITS,
			Some(InstructionError { index: 0, error: XcmError::FailedToTransactAsset("") }),
		);
	});
	assert!(log_capture.contains("Funds are unavailable"));
}

#[test]
fn exchange_asset_pool_not_created() {
	test_exchange_asset(
		false,
		500 * UNITS,
		665 * UNITS,
		Some(InstructionError { index: 1, error: XcmError::NoDeal }),
	);
}

fn test_exchange_asset(
	create_pool: bool,
	give_amount: Balance,
	want_amount: Balance,
	expected_error: Option<InstructionError>,
) {
	let alice: AccountId = Westend::account_id_of(ALICE);
	let native_asset_location = WestendLocation::get();
	let native_asset_id = AssetId(native_asset_location.clone());
	let origin = RuntimeOrigin::signed(alice.clone());
	let asset_location = Location::new(1, [Parachain(2001)]);
	let asset_id = AssetId(asset_location.clone());

	// Setup initial state
	AssetHubWestend::execute_with(|| {
		assert_ok!(<Balances as Mutate<_>>::mint_into(
			&alice,
			ExistentialDeposit::get() + (1_000 * UNITS)
		));

		assert_ok!(ForeignAssets::force_create(
			RuntimeOrigin::root(),
			asset_location.clone().into(),
			alice.clone().into(),
			true,
			1
		));
	});

	if create_pool {
		create_pool_with_wnd_on!(AssetHubWestend, asset_location.clone(), true, alice.clone());
	}

	// Execute and verify swap
	AssetHubWestend::execute_with(|| {
		let foreign_balance_before = ForeignAssets::balance(asset_location.clone(), &alice);
		let wnd_balance_before = Balances::total_balance(&alice);

		let give: Assets = (native_asset_id, give_amount).into();
		let want: Assets = (asset_id, want_amount).into();
		let xcm = Xcm(vec![
			WithdrawAsset(give.clone().into()),
			ExchangeAsset { give: give.into(), want: want.into(), maximal: true },
			DepositAsset { assets: Wild(All), beneficiary: alice.clone().into() },
		]);

		let result = PolkadotXcm::execute(origin, bx!(xcm::VersionedXcm::from(xcm)), Weight::MAX);

		let foreign_balance_after = ForeignAssets::balance(asset_location, &alice);
		let wnd_balance_after = Balances::total_balance(&alice);

		if let Some(InstructionError { index, error }) = expected_error {
			assert_err_ignore_postinfo!(
				result,
				pallet_xcm::Error::<Runtime>::LocalExecutionIncompleteWithError {
					index,
					error: error.into()
				}
			);
			assert_eq!(
				foreign_balance_after, foreign_balance_before,
				"Foreign balance changed unexpectedly: got {foreign_balance_after}, expected {foreign_balance_before}"
			);
			assert_eq!(
				wnd_balance_after, wnd_balance_before,
				"WND balance changed unexpectedly: got {wnd_balance_after}, expected {wnd_balance_before}"
			);
		} else {
			assert_ok!(result);
			assert!(
				foreign_balance_after >= foreign_balance_before + want_amount,
				"Expected foreign balance to increase by at least {want_amount} units, got {foreign_balance_after} from {foreign_balance_before}"
			);
			assert_eq!(
				wnd_balance_after, wnd_balance_before - give_amount,
				"Expected WND balance to decrease by {give_amount} units, got {wnd_balance_after} from {wnd_balance_before}"
			);
		}
	});
}

#[test]
fn exchange_asset_from_penpal_via_asset_hub_back_to_penpal() {
	let sender = PenpalASender::get();
	let sov_of_penpal_on_asset_hub = AssetHubWestend::sovereign_account_id_of(
		AssetHubWestend::sibling_location_of(PenpalA::para_id()),
	);
	let wnd_from_parachain_pov: Location = RelayLocation::get();
	let usdt_asset_hub_pov =
		Location::new(0, [PalletInstance(ASSETS_PALLET_ID), GeneralIndex(USDT_ID.into())]);
	let usdt_penpal_pov = PenpalUsdtFromAssetHub::get();
	let amount_of_wnd_to_transfer_to_ah = WESTEND_ED * 1_000_000_000;
	let amount_of_usdt_we_want_from_exchange = 1_000_000_000;

	let mut topic_id_tracker = TopicIdTracker::new();

	// SA-of-Penpal-on-AHW should contain WND amount equal at least the amount that will be
	// transferred-in to AH Since AH is the reserve for WND
	AssetHubWestend::fund_accounts(vec![(
		sov_of_penpal_on_asset_hub.clone().into(),
		ASSET_HUB_WESTEND_ED + amount_of_wnd_to_transfer_to_ah,
	)]);
	// Give the sender enough WND
	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(PenpalAssetOwner::get()),
		wnd_from_parachain_pov.clone(),
		sender.clone(),
		amount_of_wnd_to_transfer_to_ah,
	);

	// We create a pool between WND and USDT in AssetHub so we can do the exchange
	create_pool_with_wnd_on!(
		AssetHubWestend,
		usdt_asset_hub_pov.clone(),
		false,
		AssetHubWestendSender::get(),
		1_000_000_000_000,
		20_000_000_000
	);

	// Query initial balances
	let sender_usdt_on_penpal_before =
		foreign_balance_on!(PenpalA, usdt_penpal_pov.clone(), &sender);
	let sender_usdt_on_ah_before = assets_balance_on!(AssetHubWestend, USDT_ID, &sender);

	let asset_hub_location_penpal_pov = PenpalA::sibling_location_of(AssetHubWestend::para_id());
	let penpal_location_ah_pov = AssetHubWestend::sibling_location_of(PenpalA::para_id());

	PenpalA::execute_with(|| {
		let sender_signed_origin = <PenpalA as Chain>::RuntimeOrigin::signed(sender.clone());

		let local_fees_amount = 80_000_000_000_000u128;
		let remote_fees_amount = 200_000_000_000_000u128;

		let penpal_local_fees: Asset = (wnd_from_parachain_pov.clone(), local_fees_amount).into();
		let ah_remote_fees: Asset = (wnd_from_parachain_pov.clone(), remote_fees_amount).into();
		let penpal_remote_fees: Asset = (wnd_from_parachain_pov.clone(), remote_fees_amount).into();
		let wnd_to_withdraw: Asset =
			(wnd_from_parachain_pov.clone(), amount_of_wnd_to_transfer_to_ah).into();

		// xcm to be executed by penpal, sent by ah
		let xcm_back_on_penpal = Xcm(vec![
			RefundSurplus,
			DepositAsset { assets: Wild(All), beneficiary: sender.clone().into() },
		]);
		// xcm to be executed by ah, sent by penpal
		let xcm_on_ah = Xcm(vec![
			ExchangeAsset {
				give: Definite((wnd_from_parachain_pov.clone(), 100_000_000_000u128).into()),
				want: (usdt_asset_hub_pov.clone(), amount_of_usdt_we_want_from_exchange).into(),
				maximal: false,
			},
			InitiateTransfer {
				destination: penpal_location_ah_pov,
				remote_fees: Some(AssetTransferFilter::ReserveDeposit(
					penpal_remote_fees.clone().into(),
				)),
				preserve_origin: false,
				assets: BoundedVec::truncate_from(vec![AssetTransferFilter::ReserveDeposit(Wild(
					All,
				))]),
				remote_xcm: xcm_back_on_penpal,
			},
			RefundSurplus,
			DepositAsset { assets: Wild(All), beneficiary: sender.clone().into() },
		]);
		// xcm to be executed locally on penpal as starting point
		let xcm = Xcm::<()>(vec![
			WithdrawAsset(wnd_to_withdraw.into()),
			PayFees { asset: penpal_local_fees },
			InitiateTransfer {
				destination: asset_hub_location_penpal_pov,
				remote_fees: Some(AssetTransferFilter::ReserveWithdraw(
					ah_remote_fees.clone().into(),
				)),
				preserve_origin: false,
				assets: BoundedVec::truncate_from(vec![AssetTransferFilter::ReserveWithdraw(
					Wild(All),
				)]),
				remote_xcm: xcm_on_ah,
			},
			RefundSurplus,
			DepositAsset { assets: Wild(All), beneficiary: sender.clone().into() },
		]);
		// initiate transaction
		<PenpalA as PenpalAPallet>::PolkadotXcm::execute(
			sender_signed_origin,
			bx!(xcm::VersionedXcm::from(xcm.into())),
			Weight::MAX,
		)
		.unwrap();

		// verify expected events;
		PenpalA::assert_xcm_pallet_attempted_complete(None);

		let msg_sent_id = find_xcm_sent_message_id::<PenpalA>().expect("Missing Sent Event");
		topic_id_tracker.insert("PenpalA_sent", msg_sent_id.into());
	});
	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
				RuntimeEvent::AssetConversion(
					pallet_asset_conversion::Event::SwapCreditExecuted { amount_out, ..}
				) => { amount_out: *amount_out == amount_of_usdt_we_want_from_exchange, },
			]
		);

		let mq_prc_id = find_mq_processed_id::<AssetHubWestend>().expect("Missing Processed Event");
		topic_id_tracker.insert("AssetHubWestend_received", mq_prc_id);
		let msg_sent_id =
			find_xcm_sent_message_id::<AssetHubWestend>().expect("Missing Sent Event");
		topic_id_tracker.insert("AssetHubWestend_sent", msg_sent_id.into());
	});

	PenpalA::execute_with(|| {
		type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
		assert_expected_events!(
			PenpalA,
			vec![
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
		);

		let mq_prc_id = find_mq_processed_id::<PenpalA>().expect("Missing Processed Event");
		topic_id_tracker.insert("PenpalA_received", mq_prc_id);
	});

	topic_id_tracker.assert_unique();

	// Query final balances
	let sender_usdt_on_ah_after = assets_balance_on!(AssetHubWestend, USDT_ID, &sender);
	let sender_usdt_on_penpal_after =
		foreign_balance_on!(PenpalA, usdt_penpal_pov.clone(), &sender);

	// Receiver's balance is increased by usdt amount we got from exchange
	assert_eq!(
		sender_usdt_on_penpal_after,
		sender_usdt_on_penpal_before + amount_of_usdt_we_want_from_exchange
	);
	// Usdt amount on senders account AH side should stay the same i.e. all usdt came from exchange
	// not free balance
	assert_eq!(sender_usdt_on_ah_before, sender_usdt_on_ah_after);
}
