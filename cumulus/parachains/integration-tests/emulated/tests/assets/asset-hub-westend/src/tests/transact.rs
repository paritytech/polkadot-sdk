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

use crate::{create_pool_with_wnd_on, foreign_balance_on, imports::*};
use frame_support::traits::tokens::fungibles::Mutate;
use xcm_builder::{DescribeAllTerminal, DescribeFamily, HashedDescription};
use xcm_executor::traits::ConvertLocation;

/// PenpalA transacts on PenpalB, paying fees using USDT. XCM has to go through Asset Hub as the
/// reserve location of USDT. The original origin `PenpalA/PenpalASender` is proxied by Asset Hub.
fn transfer_and_transact_in_same_xcm(
	destination: Location,
	usdt: Asset,
	beneficiary: Location,
	call: xcm::DoubleEncoded<()>,
) {
	let signed_origin = <PenpalA as Chain>::RuntimeOrigin::signed(PenpalASender::get().into());
	let context = PenpalUniversalLocation::get();
	let asset_hub_location = PenpalA::sibling_location_of(AssetHubWestend::para_id());

	let Fungible(total_usdt) = usdt.fun else { unreachable!() };

	// TODO(https://github.com/paritytech/polkadot-sdk/issues/6197): dry-run to get local fees, for now use hardcoded value.
	let local_fees_amount = 80_000_000_000; // current exact value 69_200_786_622
	let ah_fees_amount = 90_000_000_000; // current exact value 79_948_099_299
	let usdt_to_ah_then_onward_amount = total_usdt - local_fees_amount - ah_fees_amount;

	let local_fees: Asset = (usdt.id.clone(), local_fees_amount).into();
	let fees_for_ah: Asset = (usdt.id.clone(), ah_fees_amount).into();
	let usdt_to_ah_then_onward: Asset = (usdt.id.clone(), usdt_to_ah_then_onward_amount).into();

	let require_weight_at_most = Weight::from_parts(1000000000, 200000);
	// xcm to be executed at dest
	let xcm_on_dest = Xcm(vec![
		Transact { require_weight_at_most, origin_kind: OriginKind::Xcm, call },
		ExpectTransactStatus(MaybeErrorCode::Success),
		// since this is the last hop, we don't need to further use any assets previously
		// reserved for fees (there are no further hops to cover transport fees for); we
		// RefundSurplus to get back any unspent fees
		RefundSurplus,
		DepositAsset { assets: Wild(All), beneficiary },
	]);
	let destination = destination.reanchored(&asset_hub_location, &context).unwrap();
	let xcm_on_ah = Xcm(vec![InitiateTransfer {
		destination,
		remote_fees: Some(AssetTransferFilter::ReserveDeposit(Wild(All))),
		preserve_origin: true,
		assets: vec![],
		remote_xcm: xcm_on_dest,
	}]);
	let xcm = Xcm::<()>(vec![
		WithdrawAsset(usdt.into()),
		PayFees { asset: local_fees },
		InitiateTransfer {
			destination: asset_hub_location,
			remote_fees: Some(AssetTransferFilter::ReserveWithdraw(fees_for_ah.into())),
			preserve_origin: true,
			assets: vec![AssetTransferFilter::ReserveWithdraw(usdt_to_ah_then_onward.into())],
			remote_xcm: xcm_on_ah,
		},
	]);
	<PenpalA as PenpalAPallet>::PolkadotXcm::execute(
		signed_origin,
		bx!(xcm::VersionedXcm::from(xcm.into())),
		Weight::MAX,
	)
	.unwrap();
}

/// PenpalA transacts on PenpalB, paying fees using USDT. XCM has to go through Asset Hub as the
/// reserve location of USDT. The original origin `PenpalA/PenpalASender` is proxied by Asset Hub.
#[test]
fn transact_from_para_to_para_through_asset_hub() {
	let destination = PenpalA::sibling_location_of(PenpalB::para_id());
	let sender = PenpalASender::get();
	let fee_amount_to_send: Balance = WESTEND_ED * 10000;
	let sender_chain_as_seen_by_asset_hub =
		AssetHubWestend::sibling_location_of(PenpalA::para_id());
	let sov_of_sender_on_asset_hub =
		AssetHubWestend::sovereign_account_id_of(sender_chain_as_seen_by_asset_hub);
	let receiver_as_seen_by_asset_hub = AssetHubWestend::sibling_location_of(PenpalB::para_id());
	let sov_of_receiver_on_asset_hub =
		AssetHubWestend::sovereign_account_id_of(receiver_as_seen_by_asset_hub);

	// Create SA-of-Penpal-on-AHW with ED.
	AssetHubWestend::fund_accounts(vec![
		(sov_of_sender_on_asset_hub.clone().into(), ASSET_HUB_WESTEND_ED),
		(sov_of_receiver_on_asset_hub.clone().into(), ASSET_HUB_WESTEND_ED),
	]);

	// Prefund USDT to sov account of sender.
	AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
		assert_ok!(<Assets as Mutate<_>>::mint_into(
			USDT_ID,
			&sov_of_sender_on_asset_hub.clone().into(),
			fee_amount_to_send,
		));
	});

	// We create a pool between WND and USDT in AssetHub.
	let usdt = Location::new(0, [PalletInstance(ASSETS_PALLET_ID), GeneralIndex(USDT_ID.into())]);
	create_pool_with_wnd_on!(AssetHubWestend, usdt, false, AssetHubWestendSender::get());
	// We also need a pool between WND and USDT on PenpalA.
	create_pool_with_wnd_on!(PenpalA, PenpalUsdtFromAssetHub::get(), true, PenpalAssetOwner::get());
	// We also need a pool between WND and USDT on PenpalB.
	create_pool_with_wnd_on!(PenpalB, PenpalUsdtFromAssetHub::get(), true, PenpalAssetOwner::get());

	let usdt_from_asset_hub = PenpalUsdtFromAssetHub::get();
	PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		assert_ok!(<ForeignAssets as Mutate<_>>::mint_into(
			usdt_from_asset_hub.clone(),
			&sender,
			fee_amount_to_send,
		));
	});

	// Give the sender enough Relay tokens to pay for local delivery fees.
	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(PenpalAssetOwner::get()),
		RelayLocation::get(),
		sender.clone(),
		10_000_000_000_000, // Large estimate to make sure it works.
	);

	// Init values for Parachain Destination
	let receiver = PenpalBReceiver::get();

	// Query initial balances
	let sender_assets_before = foreign_balance_on!(PenpalA, usdt_from_asset_hub.clone(), &sender);
	let receiver_assets_before =
		foreign_balance_on!(PenpalB, usdt_from_asset_hub.clone(), &receiver);

	// Now register a new asset on PenpalB from PenpalA/sender account while paying fees using USDT
	// (going through Asset Hub)

	let usdt_to_send: Asset = (usdt_from_asset_hub.clone(), fee_amount_to_send).into();
	let assets: Assets = usdt_to_send.clone().into();
	let asset_location_on_penpal_a =
		Location::new(0, [PalletInstance(ASSETS_PALLET_ID), GeneralIndex(ASSET_ID.into())]);
	let penpal_a_as_seen_by_penpal_b = PenpalB::sibling_location_of(PenpalA::para_id());
	let sender_as_seen_by_penpal_b =
		penpal_a_as_seen_by_penpal_b.clone().appended_with(sender.clone()).unwrap();
	let foreign_asset_at_penpal_b =
		penpal_a_as_seen_by_penpal_b.appended_with(asset_location_on_penpal_a).unwrap();
	// Encoded `create_asset` call to be executed in PenpalB
	let call = PenpalB::create_foreign_asset_call(
		foreign_asset_at_penpal_b.clone(),
		ASSET_MIN_BALANCE,
		receiver.clone(),
	);
	PenpalA::execute_with(|| {
		// initiate transaction
		transfer_and_transact_in_same_xcm(destination, usdt_to_send, receiver.clone().into(), call);

		// verify expected events;
		PenpalA::assert_xcm_pallet_attempted_complete(None);
	});
	AssetHubWestend::execute_with(|| {
		let sov_penpal_a_on_ah = AssetHubWestend::sovereign_account_id_of(
			AssetHubWestend::sibling_location_of(PenpalA::para_id()),
		);
		let sov_penpal_b_on_ah = AssetHubWestend::sovereign_account_id_of(
			AssetHubWestend::sibling_location_of(PenpalB::para_id()),
		);
		asset_hub_hop_assertions(&assets, sov_penpal_a_on_ah, sov_penpal_b_on_ah);
	});
	PenpalB::execute_with(|| {
		let expected_creator =
			HashedDescription::<AccountId, DescribeFamily<DescribeAllTerminal>>::convert_location(
				&sender_as_seen_by_penpal_b,
			)
			.unwrap();
		penpal_b_assertions(foreign_asset_at_penpal_b, expected_creator, receiver.clone());
	});

	// Query final balances
	let sender_assets_after = foreign_balance_on!(PenpalA, usdt_from_asset_hub.clone(), &sender);
	let receiver_assets_after = foreign_balance_on!(PenpalB, usdt_from_asset_hub, &receiver);

	// Sender's balance is reduced by amount
	assert_eq!(sender_assets_after, sender_assets_before - fee_amount_to_send);
	// Receiver's balance is increased
	assert!(receiver_assets_after > receiver_assets_before);
}

fn asset_hub_hop_assertions(assets: &Assets, sender_sa: AccountId, receiver_sa: AccountId) {
	type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
	for asset in assets.inner() {
		let amount = if let Fungible(a) = asset.fun { a } else { unreachable!() };
		assert_expected_events!(
			AssetHubWestend,
			vec![
				// Withdrawn from sender parachain SA
				RuntimeEvent::Assets(
					pallet_assets::Event::Burned { owner, balance, .. }
				) => {
					owner: *owner == sender_sa,
					balance: *balance == amount,
				},
				// Deposited to receiver parachain SA
				RuntimeEvent::Assets(
					pallet_assets::Event::Deposited { who, .. }
				) => {
					who: *who == receiver_sa,
				},
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
		);
	}
}

fn penpal_b_assertions(
	expected_asset: Location,
	expected_creator: AccountId,
	expected_owner: AccountId,
) {
	type RuntimeEvent = <PenpalB as Chain>::RuntimeEvent;
	PenpalB::assert_xcmp_queue_success(None);
	assert_expected_events!(
		PenpalB,
		vec![
			RuntimeEvent::ForeignAssets(
				pallet_assets::Event::Created { asset_id, creator, owner }
			) => {
				asset_id: *asset_id == expected_asset,
				creator: *creator == expected_creator,
				owner: *owner == expected_owner,
			},
		]
	);
}
