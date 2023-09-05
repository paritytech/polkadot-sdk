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

/// Relay Chain should be able to execute `Transact` instructions in System Parachain
/// when `OriginKind::Superuser` and signer is `sudo`
#[test]
fn send_transact_sudo_from_relay_to_system_para_works() {
	// Init tests variables
	let root_origin = <Kusama as Chain>::RuntimeOrigin::root();
	let system_para_destination = Kusama::child_location_of(AssetHubKusama::para_id()).into();
	let asset_owner: AccountId = AssetHubKusamaSender::get().into();
	let xcm = AssetHubKusama::force_create_asset_xcm(
		OriginKind::Superuser,
		ASSET_ID,
		asset_owner.clone(),
		true,
		1000,
	);
	// Send XCM message from Relay Chain
	Kusama::execute_with(|| {
		assert_ok!(<Kusama as KusamaPallet>::XcmPallet::send(
			root_origin,
			bx!(system_para_destination),
			bx!(xcm),
		));

		Kusama::assert_xcm_pallet_sent();
	});

	// Receive XCM message in Assets Parachain
	AssetHubKusama::execute_with(|| {
		type RuntimeEvent = <AssetHubKusama as Chain>::RuntimeEvent;

		AssetHubKusama::assert_dmp_queue_complete(Some(Weight::from_parts(1_019_445_000, 200_000)));

		assert_expected_events!(
			AssetHubKusama,
			vec![
				RuntimeEvent::Assets(pallet_assets::Event::ForceCreated { asset_id, owner }) => {
					asset_id: *asset_id == ASSET_ID,
					owner: *owner == asset_owner,
				},
			]
		);

		assert!(<AssetHubKusama as AssetHubKusamaPallet>::Assets::asset_exists(ASSET_ID));
	});
}

/// Relay Chain shouldn't be able to execute `Transact` instructions in System Parachain
/// when `OriginKind::Native`
#[test]
fn send_transact_native_from_relay_to_system_para_fails() {
	// Init tests variables
	let signed_origin = <Kusama as Chain>::RuntimeOrigin::signed(KusamaSender::get().into());
	let system_para_destination = Kusama::child_location_of(AssetHubKusama::para_id()).into();
	let asset_owner = AssetHubKusamaSender::get().into();
	let xcm = AssetHubKusama::force_create_asset_xcm(
		OriginKind::Native,
		ASSET_ID,
		asset_owner,
		true,
		1000,
	);

	// Send XCM message from Relay Chain
	Kusama::execute_with(|| {
		assert_err!(
			<Kusama as KusamaPallet>::XcmPallet::send(
				signed_origin,
				bx!(system_para_destination),
				bx!(xcm)
			),
			DispatchError::BadOrigin
		);
	});
}

/// System Parachain shouldn't be able to execute `Transact` instructions in Relay Chain
/// when `OriginKind::Native`
#[test]
fn send_transact_native_from_system_para_to_relay_fails() {
	// Init tests variables
	let signed_origin =
		<AssetHubKusama as Chain>::RuntimeOrigin::signed(AssetHubKusamaSender::get().into());
	let relay_destination = AssetHubKusama::parent_location().into();
	let call = <Kusama as Chain>::RuntimeCall::System(frame_system::Call::<
		<Kusama as Chain>::Runtime,
	>::remark_with_event {
		remark: vec![0, 1, 2, 3],
	})
	.encode()
	.into();
	let origin_kind = OriginKind::Native;

	let xcm = xcm_transact_unpaid_execution(call, origin_kind);

	// Send XCM message from Relay Chain
	AssetHubKusama::execute_with(|| {
		assert_err!(
			<AssetHubKusama as AssetHubKusamaPallet>::PolkadotXcm::send(
				signed_origin,
				bx!(relay_destination),
				bx!(xcm)
			),
			DispatchError::BadOrigin
		);
	});
}

/// Parachain should be able to send XCM paying its fee with sufficient asset
/// in the System Parachain
#[test]
fn send_xcm_from_para_to_system_para_paying_fee_with_assets_works() {
	let para_sovereign_account = AssetHubKusama::sovereign_account_id_of(
		AssetHubKusama::sibling_location_of(PenpalKusamaA::para_id()),
	);

	// Force create and mint assets for Parachain's sovereign account
	AssetHubKusama::force_create_and_mint_asset(
		ASSET_ID,
		ASSET_MIN_BALANCE,
		true,
		para_sovereign_account.clone(),
		ASSET_MIN_BALANCE * 1000000000,
	);

	// We just need a call that can pass the `SafeCallFilter`
	// Call values are not relevant
	let call = AssetHubKusama::force_create_asset_call(
		ASSET_ID,
		para_sovereign_account.clone(),
		true,
		ASSET_MIN_BALANCE,
	);

	let origin_kind = OriginKind::SovereignAccount;
	let fee_amount = ASSET_MIN_BALANCE * 1000000;
	let native_asset =
		(X2(PalletInstance(ASSETS_PALLET_ID), GeneralIndex(ASSET_ID.into())), fee_amount).into();

	let root_origin = <PenpalKusamaA as Chain>::RuntimeOrigin::root();
	let system_para_destination =
		PenpalKusamaA::sibling_location_of(AssetHubKusama::para_id()).into();
	let xcm = xcm_transact_paid_execution(
		call,
		origin_kind,
		native_asset,
		para_sovereign_account.clone(),
	);

	PenpalKusamaA::execute_with(|| {
		assert_ok!(<PenpalKusamaA as PenpalKusamaAPallet>::PolkadotXcm::send(
			root_origin,
			bx!(system_para_destination),
			bx!(xcm),
		));

		PenpalKusamaA::assert_xcm_pallet_sent();
	});

	AssetHubKusama::execute_with(|| {
		type RuntimeEvent = <AssetHubKusama as Chain>::RuntimeEvent;

		AssetHubKusama::assert_xcmp_queue_success(Some(Weight::from_parts(2_176_414_000, 203_593)));

		assert_expected_events!(
			AssetHubKusama,
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
