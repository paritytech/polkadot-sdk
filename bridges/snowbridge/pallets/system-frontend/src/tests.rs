// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use crate::mock::*;
use crate::Error;
use frame_support::{assert_ok, assert_err};
use snowbridge_core::AssetMetadata;
use xcm::{
	latest::{Location, Assets, Error as XcmError},
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

		assert_ok!(EthereumSystemFrontend::register_token(origin.clone(), asset_id.clone(), asset_metadata.clone()));
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

		set_charge_fees_override(
			|_,_| Err(XcmError::FeesNotMet)
		);

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
			|_, _,| Err(SendError::Unroutable),
			|_| Err(SendError::Transport("not allowed to call here")),
		);
		assert_err!(
			EthereumSystemFrontend::register_token(origin.clone(), asset_id.clone(), asset_metadata.clone()),
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
