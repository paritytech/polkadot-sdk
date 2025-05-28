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

#[test]
fn teleport_via_limited_teleport_assets_to_other_system_parachains_works() {
	let amount = BRIDGE_HUB_WESTEND_ED * 100;
	let native_asset: Assets = (Parent, amount).into();

	test_parachain_is_trusted_teleporter!(
		BridgeHubWestend,      // Origin
		vec![AssetHubWestend], // Destinations
		(native_asset, amount),
		limited_teleport_assets
	);
}

#[test]
fn teleport_via_transfer_assets_to_other_system_parachains_works() {
	let amount = BRIDGE_HUB_WESTEND_ED * 100;
	let native_asset: Assets = (Parent, amount).into();

	test_parachain_is_trusted_teleporter!(
		BridgeHubWestend,      // Origin
		vec![AssetHubWestend], // Destinations
		(native_asset, amount),
		transfer_assets
	);
}

#[test]
fn teleport_via_limited_teleport_assets_from_and_to_relay() {
	let amount = WESTEND_ED * 100;
	let native_asset: Assets = (Here, amount).into();

	test_relay_is_trusted_teleporter!(
		Westend,
		vec![BridgeHubWestend],
		(native_asset, amount),
		limited_teleport_assets
	);

	test_parachain_is_trusted_teleporter_for_relay!(
		BridgeHubWestend,
		Westend,
		amount,
		limited_teleport_assets
	);
}

#[test]
fn teleport_via_transfer_assets_from_and_to_relay() {
	let amount = WESTEND_ED * 100;
	let native_asset: Assets = (Here, amount).into();

	test_relay_is_trusted_teleporter!(
		Westend,
		vec![BridgeHubWestend],
		(native_asset, amount),
		transfer_assets
	);

	test_parachain_is_trusted_teleporter_for_relay!(
		BridgeHubWestend,
		Westend,
		amount,
		transfer_assets
	);
}
