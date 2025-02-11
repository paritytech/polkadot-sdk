// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use crate::{mock::*, Error};
use frame_support::{assert_noop, assert_ok};
use snowbridge_core::AssetMetadata;
use xcm::{
	latest::Location,
	prelude::{GeneralIndex, Parachain},
	VersionedLocation,
};

#[test]
fn create_agent() {
	new_test_ext().execute_with(|| {
		let origin_location = Location::new(1, [Parachain(2000)]);
		let origin = make_xcm_origin(origin_location);
		assert_ok!(EthereumSystemFrontend::create_agent(origin, 100));
	});
}

#[test]
fn register_token() {
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
		assert_ok!(EthereumSystemFrontend::register_token(origin, asset_id, asset_metadata, 100));
	});
}

#[test]
fn register_token_fail_for_owner_check() {
	new_test_ext().execute_with(|| {
		let origin_location = Location::new(1, [Parachain(2000)]);
		let origin = make_xcm_origin(origin_location);
		let asset_location: Location = Location::new(1, [Parachain(2001), GeneralIndex(1)]);
		let asset_id = Box::new(VersionedLocation::from(asset_location));
		let asset_metadata = AssetMetadata {
			name: "pal".as_bytes().to_vec().try_into().unwrap(),
			symbol: "pal".as_bytes().to_vec().try_into().unwrap(),
			decimals: 12,
		};
		assert_noop!(
			EthereumSystemFrontend::register_token(origin, asset_id, asset_metadata, 100),
			Error::<Test>::OwnerCheck
		);
	});
}
