// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use crate::{mock::*, DispatchError::Other, Error};
use frame_support::{assert_err, assert_noop, assert_ok};
use frame_system::RawOrigin;
use snowbridge_core::{reward::MessageId, AssetMetadata, BasicOperatingMode};
use snowbridge_test_utils::mock_swap_executor::TRIGGER_SWAP_ERROR_AMOUNT;
use sp_keyring::sr25519::Keyring;
use xcm::{
	latest::{Assets, Error as XcmError, Location},
	opaque::latest::{Asset, AssetId, AssetInstance, Fungibility},
	prelude::{GeneralIndex, Parachain, SendError},
	VersionedLocation,
};

#[test]
fn register_token() {
	new_test_ext().execute_with(|| {
		let origin_location = Location::new(1, [Parachain(2000)]);
		let origin = make_xcm_origin(origin_location.clone());
		let asset_location: Location = Location::new(1, [Parachain(2000), GeneralIndex(1)]);
		let asset_id = Box::new(VersionedLocation::from(asset_location));
		let asset_metadata = AssetMetadata {
			name: "pal".as_bytes().to_vec().try_into().unwrap(),
			symbol: "pal".as_bytes().to_vec().try_into().unwrap(),
			decimals: 12,
		};

		assert_ok!(EthereumSystemFrontend::register_token(
			origin.clone(),
			asset_id.clone(),
			asset_metadata.clone()
		));
	});
}

#[test]
fn register_token_fails_delivery_fees_not_met() {
	new_test_ext().execute_with(|| {
		let origin_location = Location::new(1, [Parachain(2000)]);
		let origin = make_xcm_origin(origin_location);
		let asset_location: Location = Location::new(1, [Parachain(2000), GeneralIndex(1)]);
		let asset_id = Box::new(VersionedLocation::from(asset_location));
		let asset_metadata = AssetMetadata {
			name: "pal".as_bytes().to_vec().try_into().unwrap(),
			symbol: "pal".as_bytes().to_vec().try_into().unwrap(),
			decimals: 12,
		};

		set_charge_fees_override(|_, _| Err(XcmError::FeesNotMet));

		assert_err!(
			EthereumSystemFrontend::register_token(origin, asset_id, asset_metadata),
			Error::<Test>::FeesNotMet,
		);
	});
}

#[test]
fn register_token_fails_unroutable() {
	new_test_ext().execute_with(|| {
		let origin_location = Location::new(1, [Parachain(2000)]);
		let origin = make_xcm_origin(origin_location);
		let asset_location: Location = Location::new(1, [Parachain(2000), GeneralIndex(1)]);
		let asset_id = Box::new(VersionedLocation::from(asset_location));
		let asset_metadata = AssetMetadata {
			name: "pal".as_bytes().to_vec().try_into().unwrap(),
			symbol: "pal".as_bytes().to_vec().try_into().unwrap(),
			decimals: 12,
		};

		// Send XCM with overrides for `SendXcm` behavior to return `Unroutable` error on
		// validate
		set_sender_override(
			|_, _| Err(SendError::Unroutable),
			|_| Err(SendError::Transport("not allowed to call here")),
		);
		assert_err!(
			EthereumSystemFrontend::register_token(
				origin.clone(),
				asset_id.clone(),
				asset_metadata.clone()
			),
			Error::<Test>::SendFailure
		);

		// Send XCM with overrides for `SendXcm` behavior to return `Unroutable` error on
		// deliver
		set_sender_override(
			|_, y| Ok((y.take().unwrap(), Assets::default())),
			|_| Err(SendError::Unroutable),
		);

		assert_err!(
			EthereumSystemFrontend::register_token(origin, asset_id, asset_metadata),
			Error::<Test>::SendFailure
		);
	});
}

#[test]
fn test_switch_operating_mode() {
	new_test_ext().execute_with(|| {
		assert_ok!(EthereumSystemFrontend::set_operating_mode(
			RawOrigin::Root.into(),
			BasicOperatingMode::Halted,
		));
		let origin_location = Location::new(1, [Parachain(2000)]);
		let origin = make_xcm_origin(origin_location);
		let asset_location: Location = Location::new(1, [Parachain(2000), GeneralIndex(1)]);
		let asset_id = Box::new(VersionedLocation::from(asset_location));
		let asset_metadata = AssetMetadata {
			name: "pal".as_bytes().to_vec().try_into().unwrap(),
			symbol: "pal".as_bytes().to_vec().try_into().unwrap(),
			decimals: 12,
		};
		assert_noop!(
			EthereumSystemFrontend::register_token(
				origin.clone(),
				asset_id.clone(),
				asset_metadata.clone()
			),
			crate::Error::<Test>::Halted
		);
		assert_ok!(EthereumSystemFrontend::set_operating_mode(
			RawOrigin::Root.into(),
			BasicOperatingMode::Normal,
		));
		assert_ok!(EthereumSystemFrontend::register_token(origin, asset_id, asset_metadata),);
	});
}

#[test]
fn add_tip_ether_asset_succeeds() {
	new_test_ext().execute_with(|| {
		let who: AccountId = Keyring::Alice.into();
		let message_id = MessageId::Inbound(1);
		let ether_location = Ether::get();
		let tip_amount = 1000;
		let asset = Asset::from((ether_location.clone(), tip_amount));

		assert_ok!(EthereumSystemFrontend::add_tip(
			RuntimeOrigin::signed(who.clone()),
			message_id.clone(),
			asset.clone()
		));

		let events = System::events();
		let event_record = events.last().expect("Expected at least one event").event.clone();

		if !matches!(
			event_record,
			RuntimeEvent::EthereumSystemFrontend(crate::Event::MessageSent { .. })
		) {
			panic!("Expected MessageSent event, got: {:?}", event_record);
		}
	});
}

#[test]
fn add_tip_non_ether_asset_succeeds() {
	new_test_ext().execute_with(|| {
		let who: AccountId = Keyring::Alice.into();
		let message_id = MessageId::Outbound(2);
		let non_ether_location = Location::new(1, [Parachain(3000)]);
		let tip_amount = 2000;
		let asset = Asset::from((non_ether_location.clone(), tip_amount));

		assert_ok!(EthereumSystemFrontend::add_tip(
			RuntimeOrigin::signed(who.clone()),
			message_id.clone(),
			asset.clone()
		));

		let events = System::events();
		let event_record = events.last().expect("Expected at least one event").event.clone();

		if !matches!(
			event_record,
			RuntimeEvent::EthereumSystemFrontend(crate::Event::MessageSent { .. })
		) {
			panic!("Expected MessageSent event, got: {:?}", event_record);
		}
	});
}

#[test]
fn add_tip_unsupported_asset_fails() {
	new_test_ext().execute_with(|| {
		let who: AccountId = Keyring::Alice.into();
		let message_id = MessageId::Inbound(1);
		let asset = Asset {
			id: AssetId(Location::new(1, [Parachain(4000)])),
			fun: Fungibility::NonFungible(AssetInstance::Array4([0u8; 4])),
		};
		assert_noop!(
			EthereumSystemFrontend::add_tip(RuntimeOrigin::signed(who), message_id, asset),
			Error::<Test>::UnsupportedAsset
		);
	});
}

#[test]
fn add_tip_send_xcm_failure() {
	new_test_ext().execute_with(|| {
		set_sender_override(
			|_, _| Ok((Default::default(), Default::default())),
			|_| Err(SendError::Unroutable),
		);
		let who: AccountId = Keyring::Alice.into();
		let message_id = MessageId::Outbound(4);
		let ether_location = Ether::get();
		let tip_amount = 3000;
		let asset = Asset::from((ether_location.clone(), tip_amount));
		assert_noop!(
			EthereumSystemFrontend::add_tip(RuntimeOrigin::signed(who), message_id, asset),
			Error::<Test>::SendFailure
		);
	});
}

#[test]
fn add_tip_origin_not_signed_fails() {
	new_test_ext().execute_with(|| {
		let message_id = MessageId::Inbound(5);
		let ether_location = Ether::get();
		let tip_amount = 1500;
		let asset = Asset::from((ether_location, tip_amount));
		assert_noop!(
			EthereumSystemFrontend::add_tip(RuntimeOrigin::root(), message_id, asset),
			sp_runtime::DispatchError::BadOrigin
		);
	});
}

#[test]
fn tip_fails_due_to_swap_error() {
	new_test_ext().execute_with(|| {
		let who: AccountId = Keyring::Alice.into();
		let message_id = MessageId::Inbound(6);
		let non_ether_location = Location::new(1, [Parachain(3000)]);
		// Use the special amount 12345 that will trigger a swap error in mock_swap_executor
		let tip_amount = TRIGGER_SWAP_ERROR_AMOUNT;
		let asset = Asset::from((non_ether_location.clone(), tip_amount));

		assert_noop!(
			EthereumSystemFrontend::add_tip(RuntimeOrigin::signed(who), message_id, asset),
			Other("Swap failed for test")
		);
	});
}
