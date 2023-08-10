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

/// Relay Chain should be able to execute `Transact` instructions in System Parachain
/// when `OriginKind::Superuser` and signer is `sudo`
#[test]
fn send_transact_sudo_from_relay_to_system_para_works() {
	// Init tests variables
	let root_origin = <Westend as Chain>::RuntimeOrigin::root();
	let system_para_destination = Westend::child_location_of(AssetHubWestend::para_id()).into();
	let asset_owner: AccountId = AssetHubWestendSender::get().into();
	let xcm = AssetHubWestend::force_create_asset_xcm(
		OriginKind::Superuser,
		ASSET_ID,
		asset_owner.clone(),
		true,
		1000,
	);
	// Send XCM message from Relay Chain
	Westend::execute_with(|| {
		assert_ok!(<Westend as WestendPallet>::XcmPallet::send(
			root_origin,
			bx!(system_para_destination),
			bx!(xcm),
		));

		Westend::assert_xcm_pallet_sent();
	});

	// Receive XCM message in Assets Parachain
	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		AssetHubWestend::assert_dmp_queue_complete(Some(Weight::from_parts(
			1_019_445_000,
			200_000,
		)));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::Assets(pallet_assets::Event::ForceCreated { asset_id, owner }) => {
					asset_id: *asset_id == ASSET_ID,
					owner: *owner == asset_owner,
				},
			]
		);

		assert!(<AssetHubWestend as AssetHubWestendPallet>::Assets::asset_exists(ASSET_ID));
	});
}

/// Parachain should be able to send XCM paying its fee with sufficient asset
/// in the System Parachain
#[test]
fn send_xcm_from_para_to_system_para_paying_fee_with_assets_works() {
	let para_sovereign_account = AssetHubWestend::sovereign_account_id_of(
		AssetHubWestend::sibling_location_of(PenpalWestendA::para_id()),
	);

	// Force create and mint assets for Parachain's sovereign account
	AssetHubWestend::force_create_and_mint_asset(
		ASSET_ID,
		ASSET_MIN_BALANCE,
		true,
		para_sovereign_account.clone(),
		ASSET_MIN_BALANCE * 1000000000,
	);

	// We just need a call that can pass the `SafeCallFilter`
	// Call values are not relevant
	let call = AssetHubWestend::force_create_asset_call(
		ASSET_ID,
		para_sovereign_account.clone(),
		true,
		ASSET_MIN_BALANCE,
	);

	let origin_kind = OriginKind::SovereignAccount;
	let fee_amount = ASSET_MIN_BALANCE * 1000000;
	let native_asset =
		(X2(PalletInstance(ASSETS_PALLET_ID), GeneralIndex(ASSET_ID.into())), fee_amount).into();

	let root_origin = <PenpalWestendA as Chain>::RuntimeOrigin::root();
	let system_para_destination =
		PenpalWestendA::sibling_location_of(AssetHubWestend::para_id()).into();
	let xcm = xcm_transact_paid_execution(
		call,
		origin_kind,
		native_asset,
		para_sovereign_account.clone(),
	);

	PenpalWestendA::execute_with(|| {
		assert_ok!(<PenpalWestendA as PenpalWestendAPallet>::PolkadotXcm::send(
			root_origin,
			bx!(system_para_destination),
			bx!(xcm),
		));

		AssetHubWestend::assert_xcm_pallet_sent();
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		AssetHubWestend::assert_xcmp_queue_success(Some(Weight::from_parts(
			2_176_414_000,
			203_593,
		)));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::Assets(pallet_assets::Event::Burned { asset_id, owner, balance }) => {
					asset_id: *asset_id == ASSET_ID,
					owner: *owner == para_sovereign_account,
					balance: *balance == fee_amount,
				},
				RuntimeEvent::Assets(pallet_assets::Event::Issued { asset_id, .. }) => {
					asset_id: *asset_id == ASSET_ID,
				},
			]
		);
	});
}
