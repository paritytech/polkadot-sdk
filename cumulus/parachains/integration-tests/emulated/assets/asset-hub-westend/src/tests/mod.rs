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

mod reserve_transfer;
mod send;
mod set_xcm_versions;
mod swap;
mod teleport;

/// Relay Chain sends `Transact` instruction with `force_create_asset` to System Parachain.
pub fn do_force_create_asset_from_relay_to_system_para(origin_kind: OriginKind) {
	let asset_owner: AccountId = AssetHubWestendSender::get().into();

	Westend::send_transact_to_parachain(
		origin_kind,
		AssetHubWestend::para_id(),
		AssetHubWestend::force_create_asset_call(ASSET_ID, asset_owner.clone(), true, 1000),
	);

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
