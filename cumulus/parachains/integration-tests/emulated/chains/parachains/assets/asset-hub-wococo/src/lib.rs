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

// Substrate
use frame_support::traits::OnInitialize;

// Cumulus
use emulated_integration_tests_common::{
	impl_accounts_helpers_for_parachain, impl_assert_events_helpers_for_parachain,
	impl_assets_helpers_for_system_parachain, xcm_emulator::decl_test_parachains,
};
use wococo_emulated_chain::Wococo;

// AssetHubWococo Parachain declaration
decl_test_parachains! {
	pub struct AssetHubWococo {
		genesis = asset_hub_rococo_emulated_chain::genesis::genesis(),
		on_init = {
			asset_hub_rococo_runtime::AuraExt::on_initialize(1);
		},
		runtime = asset_hub_rococo_runtime,
		core = {
			XcmpMessageHandler: asset_hub_rococo_runtime::XcmpQueue,
			LocationToAccountId: asset_hub_rococo_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: asset_hub_rococo_runtime::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: asset_hub_rococo_runtime::PolkadotXcm,
			Assets: asset_hub_rococo_runtime::Assets,
			ForeignAssets: asset_hub_rococo_runtime::ForeignAssets,
			PoolAssets: asset_hub_rococo_runtime::PoolAssets,
			AssetConversion: asset_hub_rococo_runtime::AssetConversion,
			Balances: asset_hub_rococo_runtime::Balances,
		}
	},
}

// AssetHubWococo implementation
impl_accounts_helpers_for_parachain!(AssetHubWococo);
impl_assert_events_helpers_for_parachain!(AssetHubWococo);
impl_assets_helpers_for_system_parachain!(AssetHubWococo, Wococo);
