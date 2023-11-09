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
pub use frame_support::assert_ok;

// Polkadot
pub use xcm::{
	prelude::{AccountId32 as AccountId32Junction, *},
	v3::{Error, NetworkId::Rococo as RococoId},
};

// Bridges
pub use bp_messages::LaneId;

// Cumulus
pub use emulated_integration_tests_common::{
	test_parachain_is_trusted_teleporter,
	xcm_emulator::{
		assert_expected_events, bx, helpers::weight_within_threshold, Chain, Parachain as Para,
		RelayChain as Relay, Test, TestArgs, TestContext, TestExt,
	},
	xcm_helpers::{xcm_transact_paid_execution, xcm_transact_unpaid_execution},
	PROOF_SIZE_THRESHOLD, REF_TIME_THRESHOLD, XCM_V3,
};
pub use parachains_common::{AccountId, Balance};
pub use westend_system_emulated_network::{
	bridge_hub_westend_emulated_chain::{
		genesis::ED as BRIDGE_HUB_ROCOCO_ED, BridgeHubWestendParaPallet as BridgeHubWestendPallet,
	},
	westend_emulated_chain::{genesis::ED as ROCOCO_ED, WestendRelayPallet as WestendPallet},
	AssetHubWestendPara as AssetHubWestend, AssetHubWestendParaReceiver as AssetHubWestendReceiver,
	AssetHubWestendParaSender as AssetHubWestendSender, BridgeHubWestendPara as BridgeHubWestend,
	BridgeHubWestendParaReceiver as BridgeHubWestendReceiver,
	BridgeHubWestendParaSender as BridgeHubWestendSender, WestendRelay as Westend,
	WestendRelayReceiver as WestendReceiver, WestendRelaySender as WestendSender,
};

pub const ASSET_ID: u32 = 1;
pub const ASSET_MIN_BALANCE: u128 = 1000;
pub const ASSETS_PALLET_ID: u8 = 50;

#[cfg(test)]
mod tests;
