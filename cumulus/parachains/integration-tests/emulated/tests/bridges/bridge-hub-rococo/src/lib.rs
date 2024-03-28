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

#[cfg(test)]
mod imports {
	// Substrate
	pub use frame_support::{assert_err, assert_ok, pallet_prelude::DispatchResult};
	pub use sp_runtime::DispatchError;

	// Polkadot
	pub use xcm::{
		latest::ParentThen,
		prelude::{AccountId32 as AccountId32Junction, *},
		v3::{self, NetworkId::Westend as WestendId},
	};

	// Cumulus
	pub use emulated_integration_tests_common::{
		accounts::ALICE,
		impls::Inspect,
		test_parachain_is_trusted_teleporter,
		xcm_emulator::{
			assert_expected_events, bx, Chain, Parachain as Para, RelayChain as Relay, TestExt,
		},
	};
	pub use parachains_common::AccountId;
	pub use rococo_westend_system_emulated_network::{
		asset_hub_rococo_emulated_chain::{
			genesis::ED as ASSET_HUB_ROCOCO_ED, AssetHubRococoParaPallet as AssetHubRococoPallet,
		},
		asset_hub_westend_emulated_chain::{
			genesis::ED as ASSET_HUB_WESTEND_ED, AssetHubWestendParaPallet as AssetHubWestendPallet,
		},
		bridge_hub_rococo_emulated_chain::{
			genesis::ED as BRIDGE_HUB_ROCOCO_ED, BridgeHubRococoParaPallet as BridgeHubRococoPallet,
		},
		penpal_emulated_chain::PenpalAParaPallet as PenpalAPallet,
		rococo_emulated_chain::{genesis::ED as ROCOCO_ED, RococoRelayPallet as RococoPallet},
		AssetHubRococoPara as AssetHubRococo, AssetHubRococoParaReceiver as AssetHubRococoReceiver,
		AssetHubRococoParaSender as AssetHubRococoSender, AssetHubWestendPara as AssetHubWestend,
		AssetHubWestendParaReceiver as AssetHubWestendReceiver,
		AssetHubWestendParaSender as AssetHubWestendSender, BridgeHubRococoPara as BridgeHubRococo,
		BridgeHubRococoParaSender as BridgeHubRococoSender,
		BridgeHubWestendPara as BridgeHubWestend, PenpalAPara as PenpalA,
		PenpalAParaReceiver as PenpalAReceiver, PenpalAParaSender as PenpalASender,
		RococoRelay as Rococo,
	};

	pub const ASSET_MIN_BALANCE: u128 = 1000;
}

#[cfg(test)]
mod tests;
