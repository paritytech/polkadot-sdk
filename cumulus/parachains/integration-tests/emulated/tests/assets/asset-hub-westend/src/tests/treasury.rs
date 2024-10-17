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

use crate::imports::*;
use emulated_integration_tests_common::{
	accounts::{ALICE, BOB},
	USDT_ID,
};
use frame_support::traits::fungibles::{Inspect, Mutate};
use polkadot_runtime_common::impls::VersionedLocatableAsset;
use xcm_executor::traits::ConvertLocation;

#[test]
fn create_and_claim_treasury_spend() {
	const SPEND_AMOUNT: u128 = 1_000_000_000;
	// treasury location from a sibling parachain.
	let treasury_location: Location = Location::new(1, PalletInstance(37));
	// treasury account on a sibling parachain.
	let treasury_account =
		ahw_xcm_config::LocationToAccountId::convert_location(&treasury_location).unwrap();
	let asset_hub_location = Location::new(0, Parachain(AssetHubWestend::para_id().into()));
	let root = <Westend as Chain>::RuntimeOrigin::root();
	// asset kind to be spent from the treasury.
	let asset_kind: VersionedLocatableAsset =
		(asset_hub_location, AssetId([PalletInstance(50), GeneralIndex(USDT_ID.into())].into()))
			.into();
	// treasury spend beneficiary.
	let alice: AccountId = Westend::account_id_of(ALICE);
	let bob: AccountId = Westend::account_id_of(BOB);
	let bob_signed = <Westend as Chain>::RuntimeOrigin::signed(bob.clone());

	AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;

		// USDT created at genesis, mint some assets to the treasury account.
		assert_ok!(<Assets as Mutate<_>>::mint_into(USDT_ID, &treasury_account, SPEND_AMOUNT * 4));
		// beneficiary has zero balance.
		assert_eq!(<Assets as Inspect<_>>::balance(USDT_ID, &alice,), 0u128,);
	});

	Westend::execute_with(|| {
		type RuntimeEvent = <Westend as Chain>::RuntimeEvent;
		type Treasury = <Westend as WestendPallet>::Treasury;
		type AssetRate = <Westend as WestendPallet>::AssetRate;

		// create a conversion rate from `asset_kind` to the native currency.
		assert_ok!(AssetRate::create(root.clone(), Box::new(asset_kind.clone()), 2.into()));

		// create and approve a treasury spend.
		assert_ok!(Treasury::spend(
			root,
			Box::new(asset_kind),
			SPEND_AMOUNT,
			Box::new(Location::new(0, Into::<[u8; 32]>::into(alice.clone())).into()),
			None,
		));
		// claim the spend.
		assert_ok!(Treasury::payout(bob_signed.clone(), 0));

		assert_expected_events!(
			Westend,
			vec![
				RuntimeEvent::Treasury(pallet_treasury::Event::Paid { .. }) => {},
			]
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;

		// assert events triggered by xcm pay program
		// 1. treasury asset transferred to spend beneficiary
		// 2. response to Relay Chain treasury pallet instance sent back
		// 3. XCM program completed
		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::Assets(pallet_assets::Event::Transferred { asset_id: id, from, to, amount }) => {
					id: id == &USDT_ID,
					from: from == &treasury_account,
					to: to == &alice,
					amount: amount == &SPEND_AMOUNT,
				},
				RuntimeEvent::ParachainSystem(cumulus_pallet_parachain_system::Event::UpwardMessageSent { .. }) => {},
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { success: true ,.. }) => {},
			]
		);
		// beneficiary received the assets from the treasury.
		assert_eq!(<Assets as Inspect<_>>::balance(USDT_ID, &alice,), SPEND_AMOUNT,);
	});

	Westend::execute_with(|| {
		type RuntimeEvent = <Westend as Chain>::RuntimeEvent;
		type Treasury = <Westend as WestendPallet>::Treasury;

		// check the payment status to ensure the response from the AssetHub was received.
		assert_ok!(Treasury::check_status(bob_signed, 0));
		assert_expected_events!(
			Westend,
			vec![
				RuntimeEvent::Treasury(pallet_treasury::Event::SpendProcessed { .. }) => {},
			]
		);
	});
}
