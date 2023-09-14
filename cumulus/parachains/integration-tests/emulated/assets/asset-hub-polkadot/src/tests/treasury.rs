// Copyright Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use crate::*;
use frame_support::traits::fungibles::{Create, Inspect, Mutate};
use integration_tests_common::constants::accounts::{ALICE, BOB};
use polkadot_runtime_common::impls::VersionedLocatableAsset;
use xcm_executor::traits::ConvertLocation;

#[test]
fn create_and_claim_treasury_spend() {
	const ASSET_ID: u32 = 1984;
	const SPEND_AMOUNT: u128 = 1_000_000;
	// treasury location from a sibling parachain.
	let treasury_location: MultiLocation = MultiLocation::new(1, PalletInstance(19));
	// treasury account on a sibling parachain.
	let treasury_account =
		asset_hub_polkadot_runtime::xcm_config::LocationToAccountId::convert_location(
			&treasury_location,
		)
		.unwrap();
	let asset_hub_location = MultiLocation::new(0, Parachain(AssetHubPolkadot::para_id().into()));
	let root = <Polkadot as Chain>::RuntimeOrigin::root();
	// asset kind to be spent from the treasury.
	let asset_kind = VersionedLocatableAsset::V3 {
		location: asset_hub_location,
		asset_id: AssetId::Concrete((PalletInstance(50), GeneralIndex(ASSET_ID.into())).into()),
	};
	// treasury spend beneficiary.
	let alice: AccountId = Polkadot::account_id_of(ALICE);
	let bob: AccountId = Polkadot::account_id_of(BOB);
	let bob_signed = <Polkadot as Chain>::RuntimeOrigin::signed(bob.clone());

	AssetHubPolkadot::execute_with(|| {
		type Assets = <AssetHubPolkadot as AssetHubPolkadotPallet>::Assets;

		// create an asset class and mint some assets to the treasury account.
		assert_ok!(<Assets as Create<_>>::create(
			ASSET_ID,
			treasury_account.clone(),
			true,
			SPEND_AMOUNT / 2
		));
		assert_ok!(<Assets as Mutate<_>>::mint_into(ASSET_ID, &treasury_account, SPEND_AMOUNT * 4));
		// beneficiary has zero balance.
		assert_eq!(<Assets as Inspect<_>>::balance(ASSET_ID, &alice,), 0u128,);
	});

	Polkadot::execute_with(|| {
		type RuntimeEvent = <Polkadot as Chain>::RuntimeEvent;
		type Treasury = <Polkadot as PolkadotPallet>::Treasury;
		type AssetRate = <Polkadot as PolkadotPallet>::AssetRate;

		// create a conversion rate from `asset_kind` to the native currency.
		assert_ok!(AssetRate::create(root.clone(), Box::new(asset_kind.clone()), 2.into()));

		// create and approve a treasury spend.
		assert_ok!(Treasury::spend(
			root,
			Box::new(asset_kind),
			SPEND_AMOUNT,
			Box::new(MultiLocation::new(0, Into::<[u8; 32]>::into(alice.clone())).into()),
			None,
		));
		// claim the spend.
		assert_ok!(Treasury::payout(bob_signed.clone(), 0));

		assert_expected_events!(
			Polkadot,
			vec![
				RuntimeEvent::Treasury(pallet_treasury::Event::Paid { .. }) => {},
			]
		);
	});

	AssetHubPolkadot::execute_with(|| {
		type RuntimeEvent = <AssetHubPolkadot as Chain>::RuntimeEvent;
		type Assets = <AssetHubPolkadot as AssetHubPolkadotPallet>::Assets;

		// assets transferred, response sent back via UMP.
		assert_expected_events!(
			AssetHubPolkadot,
			vec![
				RuntimeEvent::Assets(pallet_assets::Event::Transferred { asset_id: id, from, to, amount }) => {
					id: id == &ASSET_ID,
					from: from == &treasury_account,
					to: to == &alice,
					amount: amount == &SPEND_AMOUNT,
				},
				RuntimeEvent::ParachainSystem(cumulus_pallet_parachain_system::Event::UpwardMessageSent { .. }) => {},
				RuntimeEvent::DmpQueue(cumulus_pallet_dmp_queue::Event::ExecutedDownward { outcome: Outcome::Complete(..) ,.. }) => {},
			]
		);
		// beneficiary received the assets from the treasury.
		assert_eq!(<Assets as Inspect<_>>::balance(ASSET_ID, &alice,), SPEND_AMOUNT,);
	});

	Polkadot::execute_with(|| {
		type RuntimeEvent = <Polkadot as Chain>::RuntimeEvent;
		type Treasury = <Polkadot as PolkadotPallet>::Treasury;

		// check the payment status to ensure the response from the AssetHub was received.
		assert_ok!(Treasury::check_status(bob_signed, 0));
		assert_expected_events!(
			Polkadot,
			vec![
				RuntimeEvent::Treasury(pallet_treasury::Event::SpendProcessed { .. }) => {},
			]
		);
	});
}
