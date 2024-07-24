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

pub use xcm::{prelude::*, v3};

pub use emulated_integration_tests_common::{
	accounts::ALICE,
	test_parachain_is_trusted_teleporter,
	xcm_emulator::{assert_expected_events, bx, Chain, Parachain, RelayChain as Relay, TestExt},
};
pub use westend_system_emulated_network::{
	asset_hub_westend_emulated_chain::{
		asset_hub_westend_runtime::xcm_config::{
			LocationToAccountId as AssetHubLocationToAccountId,
			XcmConfig as AssetHubWestendXcmConfig,
		},
		genesis::ED as ASSET_HUB_WESTEND_ED,
		AssetHubWestendParaPallet as AssetHubWestendPallet,
	},
	collectives_westend_emulated_chain::{
		collectives_westend_runtime::{
			fellowship as collectives_fellowship,
			xcm_config::XcmConfig as CollectivesWestendXcmConfig,
		},
		genesis::ED as COLLECTIVES_WESTEND_ED,
		CollectivesWestendParaPallet as CollectivesWestendPallet,
	},
	westend_emulated_chain::{
		genesis::ED as WESTEND_ED,
		westend_runtime::{
			governance as westend_governance, xcm_config::XcmConfig as WestendXcmConfig,
			OriginCaller as WestendOriginCaller,
		},
		WestendRelayPallet as WestendPallet,
	},
	AssetHubWestendPara as AssetHubWestend, AssetHubWestendParaReceiver as AssetHubWestendReceiver,
	AssetHubWestendParaSender as AssetHubWestendSender,
	CollectivesWestendPara as CollectivesWestend,
	CollectivesWestendParaReceiver as CollectivesWestendReceiver,
	CollectivesWestendParaSender as CollectivesWestendSender, WestendRelay as Westend,
	WestendRelayReceiver as WestendReceiver, WestendRelaySender as WestendSender,
};

#[cfg(test)]
mod tests;
