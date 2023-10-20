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
use bridge_hub_rococo_runtime::xcm_config::XcmConfig;

#[test]
fn teleport_to_other_system_parachains_works() {
	let amount = BRIDGE_HUB_ROCOCO_ED * 100;
	let native_asset: MultiAssets = (Parent, amount).into();

	test_parachain_is_trusted_teleporter!(
		BridgeHubRococo,      // Origin
		XcmConfig,            // XCM configuration
		vec![AssetHubRococo], // Destinations
		(native_asset, amount)
	);
}
