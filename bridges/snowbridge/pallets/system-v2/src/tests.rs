// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use crate::{mock::*, *};
use frame_support::{assert_noop, assert_ok};
use sp_runtime::DispatchError::BadOrigin;
#[test]
fn create_agent() {
	new_test_ext(true).execute_with(|| {
		let origin_location = Location::new(1, [Parachain(1000)]);
		let origin = make_xcm_origin(origin_location);

		let agent_origin = Location::new(1, [Parachain(2000)]);

		let agent_id = EthereumSystemV2::location_to_message_origin(&agent_origin).unwrap();

		assert!(!Agents::<Test>::contains_key(agent_id));
		assert_ok!(EthereumSystemV2::create_agent(
			origin,
			Box::new(VersionedLocation::from(agent_origin)),
			1
		));

		assert!(Agents::<Test>::contains_key(agent_id));
	});
}

#[test]
fn create_agent_bad_origin() {
	new_test_ext(true).execute_with(|| {
		assert_noop!(
			EthereumSystemV2::create_agent(
				make_xcm_origin(Location::new(1, []),),
				Box::new(Here.into()),
				1,
			),
			BadOrigin,
		);

		// None origin not allowed
		assert_noop!(
			EthereumSystemV2::create_agent(RuntimeOrigin::none(), Box::new(Here.into()), 1),
			BadOrigin
		);
	});
}

#[test]
fn register_tokens_succeeds() {
	new_test_ext(true).execute_with(|| {
		let origin = make_xcm_origin(Location::new(1, [Parachain(1000)]));
		let versioned_location: VersionedLocation = Location::parent().into();

		assert_ok!(EthereumSystemV2::register_token(
			origin,
			Box::new(versioned_location),
			Default::default(),
			1
		));
	});
}
