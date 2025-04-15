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
use emulated_integration_tests_common::{
	test_parachain_is_trusted_teleporter_for_relay, test_relay_is_trusted_teleporter,
};
use frame_support::assert_ok;

#[test]
fn teleport_from_and_to_relay() {
	let amount = WESTEND_ED * 10;
	let native_asset: Assets = (Here, amount).into();

	test_relay_is_trusted_teleporter!(
		Westend,                  // Origin
		WestendXcmConfig,         // XCM Configuration
		vec![CollectivesWestend], // Destinations
		(native_asset, amount)
	);

	test_parachain_is_trusted_teleporter_for_relay!(
		CollectivesWestend,          // Origin
		CollectivesWestendXcmConfig, // XCM Configuration
		Westend,                     // Destination
		amount
	);
}

#[test]
fn teleport_from_collectives_to_asset_hub() {
	let amount = ASSET_HUB_WESTEND_ED * 100;
	let native_asset: Assets = (Parent, amount).into();

	test_parachain_is_trusted_teleporter!(
		CollectivesWestend,          // Origin
		CollectivesWestendXcmConfig, // XCM Configuration
		vec![AssetHubWestend],       // Destinations
		(native_asset, amount)
	);
}

#[test]
fn teleport_from_asset_hub_to_collectives() {
	let amount = COLLECTIVES_WESTEND_ED * 100;
	let native_asset: Assets = (Parent, amount).into();

	test_parachain_is_trusted_teleporter!(
		AssetHubWestend,          // Origin
		AssetHubWestendXcmConfig, // XCM Configuration
		vec![CollectivesWestend], // Destinations
		(native_asset, amount)
	);
}
