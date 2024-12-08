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
use frame_support::{
	dispatch::RawOrigin,
	sp_runtime::traits::Dispatchable,
	traits::{
		fungible::Inspect,
		fungibles::{Inspect as FungiblesInspect, Mutate},
	},
};
use parachains_common::AccountId;
use polkadot_runtime_common::impls::VersionedLocatableAsset;
use rococo_runtime_constants::currency::GRAND;
use xcm_executor::traits::ConvertLocation;

// Fund Treasury account on Asset Hub from Treasury account on Relay Chain with ROCs.
#[test]
fn spend_roc_on_asset_hub() {
	// initial treasury balance on Asset Hub in ROCs.
	let treasury_balance = 9_000 * GRAND;
	// the balance spend on Asset Hub.
	let treasury_spend_balance = 1_000 * GRAND;

	let init_alice_balance = AssetHubRococo::execute_with(|| {
		<<AssetHubRococo as AssetHubRococoPallet>::Balances as Inspect<_>>::balance(
			&AssetHubRococo::account_id_of(ALICE),
		)
	});

	Rococo::execute_with(|| {
		type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;
		type RuntimeCall = <Rococo as Chain>::RuntimeCall;
		type Runtime = <Rococo as Chain>::Runtime;
		type Balances = <Rococo as RococoPallet>::Balances;
		type Treasury = <Rococo as RococoPallet>::Treasury;

		// Fund Treasury account on Asset Hub with ROCs.

		let root = <Rococo as Chain>::RuntimeOrigin::root();
		let treasury_account = Treasury::account_id();

		// Mint assets to Treasury account on Relay Chain.
		assert_ok!(Balances::force_set_balance(
			root.clone(),
			treasury_account.clone().into(),
			treasury_balance * 2,
		));

		let native_asset = Location::here();
		let asset_hub_location: Location = [Parachain(1000)].into();
		let treasury_location: Location = (Parent, PalletInstance(18)).into();

		let teleport_call = RuntimeCall::Utility(pallet_utility::Call::<Runtime>::dispatch_as {
			as_origin: bx!(RococoOriginCaller::system(RawOrigin::Signed(treasury_account))),
			call: bx!(RuntimeCall::XcmPallet(pallet_xcm::Call::<Runtime>::teleport_assets {
				dest: bx!(VersionedLocation::from(asset_hub_location.clone())),
				beneficiary: bx!(VersionedLocation::from(treasury_location)),
				assets: bx!(VersionedAssets::from(Assets::from(Asset {
					id: native_asset.clone().into(),
					fun: treasury_balance.into()
				}))),
				fee_asset_item: 0,
			})),
		});

		// Dispatched from Root to `despatch_as` `Signed(treasury_account)`.
		assert_ok!(teleport_call.dispatch(root));

		assert_expected_events!(
			Rococo,
			vec![
				RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
			]
		);
	});

	Rococo::execute_with(|| {
		type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;
		type RuntimeCall = <Rococo as Chain>::RuntimeCall;
		type RuntimeOrigin = <Rococo as Chain>::RuntimeOrigin;
		type Runtime = <Rococo as Chain>::Runtime;
		type Treasury = <Rococo as RococoPallet>::Treasury;

		// Fund Alice account from Rococo Treasury account on Asset Hub.

		let treasury_origin: RuntimeOrigin =
			rococo_governance::pallet_custom_origins::Origin::Treasurer.into();

		let alice_location: Location =
			[Junction::AccountId32 { network: None, id: Rococo::account_id_of(ALICE).into() }]
				.into();
		let asset_hub_location: Location = [Parachain(1000)].into();
		let native_asset = Location::parent();

		let treasury_spend_call = RuntimeCall::Treasury(pallet_treasury::Call::<Runtime>::spend {
			asset_kind: bx!(VersionedLocatableAsset::from((
				asset_hub_location.clone(),
				native_asset.into()
			))),
			amount: treasury_spend_balance,
			beneficiary: bx!(VersionedLocation::from(alice_location)),
			valid_from: None,
		});

		assert_ok!(treasury_spend_call.dispatch(treasury_origin));

		// Claim the spend.

		let bob_signed = RuntimeOrigin::signed(Rococo::account_id_of(BOB));
		assert_ok!(Treasury::payout(bob_signed.clone(), 0));

		assert_expected_events!(
			Rococo,
			vec![
				RuntimeEvent::Treasury(pallet_treasury::Event::AssetSpendApproved { .. }) => {},
				RuntimeEvent::Treasury(pallet_treasury::Event::Paid { .. }) => {},
			]
		);
	});

	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
		type Balances = <AssetHubRococo as AssetHubRococoPallet>::Balances;

		// Ensure that the funds deposited to Alice account.

		let alice_account = AssetHubRococo::account_id_of(ALICE);
		assert_eq!(
			<Balances as Inspect<_>>::balance(&alice_account),
			treasury_spend_balance + init_alice_balance
		);

		// Assert events triggered by xcm pay program:
		// 1. treasury asset transferred to spend beneficiary;
		// 2. response to Relay Chain Treasury pallet instance sent back;
		// 3. XCM program completed;
		assert_expected_events!(
			AssetHubRococo,
			vec![
				RuntimeEvent::Balances(pallet_balances::Event::Transfer { .. }) => {},
				RuntimeEvent::ParachainSystem(cumulus_pallet_parachain_system::Event::UpwardMessageSent { .. }) => {},
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { success: true ,.. }) => {},
			]
		);
	});
}

#[test]
fn create_and_claim_treasury_spend_in_usdt() {
	const SPEND_AMOUNT: u128 = 10_000_000;
	// treasury location from a sibling parachain.
	let treasury_location: Location = Location::new(1, PalletInstance(18));
	// treasury account on a sibling parachain.
	let treasury_account =
		ahr_xcm_config::LocationToAccountId::convert_location(&treasury_location).unwrap();
	let asset_hub_location = Location::new(0, Parachain(AssetHubRococo::para_id().into()));
	let root = <Rococo as Chain>::RuntimeOrigin::root();
	// asset kind to be spent from the treasury.
	let asset_kind: VersionedLocatableAsset =
		(asset_hub_location, AssetId((PalletInstance(50), GeneralIndex(USDT_ID.into())).into()))
			.into();
	// treasury spend beneficiary.
	let alice: AccountId = Rococo::account_id_of(ALICE);
	let bob: AccountId = Rococo::account_id_of(BOB);
	let bob_signed = <Rococo as Chain>::RuntimeOrigin::signed(bob.clone());

	AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::Assets;

		// USDT created at genesis, mint some assets to the treasury account.
		assert_ok!(<Assets as Mutate<_>>::mint_into(USDT_ID, &treasury_account, SPEND_AMOUNT * 4));
		// beneficiary has zero balance.
		assert_eq!(<Assets as FungiblesInspect<_>>::balance(USDT_ID, &alice,), 0u128,);
	});

	Rococo::execute_with(|| {
		type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;
		type Treasury = <Rococo as RococoPallet>::Treasury;
		type AssetRate = <Rococo as RococoPallet>::AssetRate;

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
			Rococo,
			vec![
				RuntimeEvent::Treasury(pallet_treasury::Event::Paid { .. }) => {},
			]
		);
	});

	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::Assets;

		// assert events triggered by xcm pay program
		// 1. treasury asset transferred to spend beneficiary
		// 2. response to Relay Chain treasury pallet instance sent back
		// 3. XCM program completed
		assert_expected_events!(
			AssetHubRococo,
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
		assert_eq!(<Assets as FungiblesInspect<_>>::balance(USDT_ID, &alice,), SPEND_AMOUNT,);
	});

	Rococo::execute_with(|| {
		type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;
		type Treasury = <Rococo as RococoPallet>::Treasury;

		// check the payment status to ensure the response from the AssetHub was received.
		assert_ok!(Treasury::check_status(bob_signed, 0));
		assert_expected_events!(
			Rococo,
			vec![
				RuntimeEvent::Treasury(pallet_treasury::Event::SpendProcessed { .. }) => {},
			]
		);
	});
}
