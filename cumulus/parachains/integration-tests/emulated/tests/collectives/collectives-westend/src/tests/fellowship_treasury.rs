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

use crate::*;
use frame_support::{
	assert_ok, dispatch::RawOrigin, instances::Instance1, sp_runtime::traits::Dispatchable,
	traits::fungible::Inspect,
};
use polkadot_runtime_common::impls::VersionedLocatableAsset;
use westend_runtime_constants::currency::UNITS;
use xcm_executor::traits::ConvertLocation;

// Fund Fellowship Treasury from Westend Treasury and spend from Fellowship Treasury.
#[test]
fn fellowship_treasury_spend() {
	// initial treasury balance on Asset Hub in WNDs.
	let treasury_balance = 20_000_000 * UNITS;
	// target fellowship balance on Asset Hub in WNDs.
	let fellowship_treasury_balance = 1_000_000 * UNITS;
	// fellowship first spend balance in WNDs.
	let fellowship_spend_balance = 10_000 * UNITS;

	let init_alice_balance = AssetHubWestend::execute_with(|| {
		<<AssetHubWestend as AssetHubWestendPallet>::Balances as Inspect<_>>::balance(
			&AssetHubWestend::account_id_of(ALICE),
		)
	});

	Westend::execute_with(|| {
		type RuntimeEvent = <Westend as Chain>::RuntimeEvent;
		type RuntimeCall = <Westend as Chain>::RuntimeCall;
		type Runtime = <Westend as Chain>::Runtime;
		type Balances = <Westend as WestendPallet>::Balances;
		type Treasury = <Westend as WestendPallet>::Treasury;

		// Fund Treasury account on Asset Hub with WNDs.

		let root = <Westend as Chain>::RuntimeOrigin::root();
		let treasury_account = Treasury::account_id();

		// Mist assets to Treasury account on Relay Chain.
		assert_ok!(Balances::force_set_balance(
			root.clone(),
			treasury_account.clone().into(),
			treasury_balance * 2,
		));

		let native_asset = Location::here();
		let asset_hub_location: Location = [Parachain(1000)].into();
		let treasury_location: Location = (Parent, PalletInstance(37)).into();

		let teleport_call = RuntimeCall::Utility(pallet_utility::Call::<Runtime>::dispatch_as {
			as_origin: bx!(WestendOriginCaller::system(RawOrigin::Signed(treasury_account))),
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

		// Dispatched from Root to `dispatch_as` `Signed(treasury_account)`.
		assert_ok!(teleport_call.dispatch(root));

		assert_expected_events!(
			Westend,
			vec![
				RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
			]
		);
	});

	Westend::execute_with(|| {
		type RuntimeEvent = <Westend as Chain>::RuntimeEvent;
		type RuntimeCall = <Westend as Chain>::RuntimeCall;
		type RuntimeOrigin = <Westend as Chain>::RuntimeOrigin;
		type Runtime = <Westend as Chain>::Runtime;
		type Treasury = <Westend as WestendPallet>::Treasury;

		// Fund Fellowship Treasury from Westend Treasury.

		let treasury_origin: RuntimeOrigin =
			westend_governance::pallet_custom_origins::Origin::Treasurer.into();
		let fellowship_treasury_location: Location =
			Location::new(1, [Parachain(1001), PalletInstance(65)]);
		let asset_hub_location: Location = [Parachain(1000)].into();
		let native_asset = Location::parent();

		let treasury_spend_call = RuntimeCall::Treasury(pallet_treasury::Call::<Runtime>::spend {
			asset_kind: bx!(VersionedLocatableAsset::from((
				asset_hub_location.clone(),
				native_asset.into()
			))),
			amount: fellowship_treasury_balance,
			beneficiary: bx!(VersionedLocation::from(fellowship_treasury_location)),
			valid_from: None,
		});

		assert_ok!(treasury_spend_call.dispatch(treasury_origin));

		// Claim the spend.

		let alice_signed = RuntimeOrigin::signed(Westend::account_id_of(ALICE));
		assert_ok!(Treasury::payout(alice_signed.clone(), 0));

		assert_expected_events!(
			Westend,
			vec![
				RuntimeEvent::Treasury(pallet_treasury::Event::AssetSpendApproved { .. }) => {},
				RuntimeEvent::Treasury(pallet_treasury::Event::Paid { .. }) => {},
			]
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		type Balances = <AssetHubWestend as AssetHubWestendPallet>::Balances;

		// Ensure that the funds deposited to the Fellowship Treasury account.

		let fellowship_treasury_location: Location =
			Location::new(1, [Parachain(1001), PalletInstance(65)]);
		let fellowship_treasury_account =
			AssetHubLocationToAccountId::convert_location(&fellowship_treasury_location).unwrap();

		assert_eq!(
			<Balances as Inspect<_>>::balance(&fellowship_treasury_account),
			fellowship_treasury_balance
		);

		// Assert events triggered by xcm pay program:
		// 1. treasury asset transferred to spend beneficiary;
		// 2. response to Relay Chain Treasury pallet instance sent back;
		// 3. XCM program completed;
		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::Balances(pallet_balances::Event::Transfer { .. }) => {},
				RuntimeEvent::ParachainSystem(cumulus_pallet_parachain_system::Event::UpwardMessageSent { .. }) => {},
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { success: true ,.. }) => {},
			]
		);
	});

	CollectivesWestend::execute_with(|| {
		type RuntimeEvent = <CollectivesWestend as Chain>::RuntimeEvent;
		type RuntimeCall = <CollectivesWestend as Chain>::RuntimeCall;
		type RuntimeOrigin = <CollectivesWestend as Chain>::RuntimeOrigin;
		type Runtime = <CollectivesWestend as Chain>::Runtime;
		type FellowshipTreasury =
			<CollectivesWestend as CollectivesWestendPallet>::FellowshipTreasury;

		// Fund Alice account from Fellowship Treasury.

		let fellows_origin: RuntimeOrigin =
			collectives_fellowship::pallet_fellowship_origins::Origin::Fellows.into();
		let asset_hub_location: Location = (Parent, Parachain(1000)).into();
		let native_asset = Location::parent();

		let alice_location: Location = [Junction::AccountId32 {
			network: None,
			id: CollectivesWestend::account_id_of(ALICE).into(),
		}]
		.into();

		let fellowship_treasury_spend_call =
			RuntimeCall::FellowshipTreasury(pallet_treasury::Call::<Runtime, Instance1>::spend {
				asset_kind: bx!(VersionedLocatableAsset::from((
					asset_hub_location,
					native_asset.into()
				))),
				amount: fellowship_spend_balance,
				beneficiary: bx!(VersionedLocation::from(alice_location)),
				valid_from: None,
			});

		assert_ok!(fellowship_treasury_spend_call.dispatch(fellows_origin));

		// Claim the spend.

		let alice_signed = RuntimeOrigin::signed(CollectivesWestend::account_id_of(ALICE));
		assert_ok!(FellowshipTreasury::payout(alice_signed.clone(), 0));

		assert_expected_events!(
			CollectivesWestend,
			vec![
				RuntimeEvent::FellowshipTreasury(pallet_treasury::Event::AssetSpendApproved { .. }) => {},
				RuntimeEvent::FellowshipTreasury(pallet_treasury::Event::Paid { .. }) => {},
			]
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		type Balances = <AssetHubWestend as AssetHubWestendPallet>::Balances;

		// Ensure that the funds deposited to Alice account.

		let alice_account = AssetHubWestend::account_id_of(ALICE);
		assert_eq!(
			<Balances as Inspect<_>>::balance(&alice_account),
			fellowship_spend_balance + init_alice_balance
		);

		// Assert events triggered by xcm pay program:
		// 1. treasury asset transferred to spend beneficiary;
		// 2. response to Relay Chain Treasury pallet instance sent back;
		// 3. XCM program completed;
		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::Balances(pallet_balances::Event::Transfer { .. }) => {},
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { success: true ,.. }) => {},
			]
		);
	});
}
