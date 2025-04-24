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
	pub use codec::Encode;
	pub use frame_support::{assert_err, assert_ok, pallet_prelude::DispatchResult, BoundedVec};
	pub use sp_runtime::DispatchError;

	// Polkadot
	pub use xcm::{
		latest::{ParentThen, ROCOCO_GENESIS_HASH, WESTEND_GENESIS_HASH},
		prelude::{AccountId32 as AccountId32Junction, *},
		v5,
	};
	pub use xcm_executor::traits::TransferType;

	// Cumulus
	pub use emulated_integration_tests_common::{
		accounts::ALICE,
		create_pool_with_native_on,
		impls::Inspect,
		test_dry_run_transfer_across_pk_bridge, test_parachain_is_trusted_teleporter,
		test_parachain_is_trusted_teleporter_for_relay, test_relay_is_trusted_teleporter,
		xcm_emulator::{
			assert_expected_events, bx, Chain, Parachain as Para, RelayChain as Relay, TestExt,
		},
		xcm_helpers::xcm_transact_paid_execution,
		ASSETS_PALLET_ID, USDT_ID,
	};
	pub use parachains_common::AccountId;
	pub use rococo_westend_system_emulated_network::{
		asset_hub_rococo_emulated_chain::{
			asset_hub_rococo_runtime::xcm_config::TreasuryAccount,
			genesis::ED as ASSET_HUB_ROCOCO_ED, AssetHubRococoParaPallet as AssetHubRococoPallet,
		},
		asset_hub_westend_emulated_chain::{
			genesis::{AssetHubWestendAssetOwner, ED as ASSET_HUB_WESTEND_ED},
			AssetHubWestendParaPallet as AssetHubWestendPallet,
		},
		bridge_hub_westend_emulated_chain::{
			bridge_hub_westend_runtime, genesis::ED as BRIDGE_HUB_WESTEND_ED,
			BridgeHubWestendExistentialDeposit,
			BridgeHubWestendParaPallet as BridgeHubWestendPallet, BridgeHubWestendRuntimeOrigin,
			BridgeHubWestendXcmConfig,
		},
		penpal_emulated_chain::{
			self,
			penpal_runtime::xcm_config::{
				CustomizableAssetFromSystemAssetHub as PenpalCustomizableAssetFromSystemAssetHub,
				LocalTeleportableToAssetHub as PenpalLocalTeleportableToAssetHub,
				UniversalLocation as PenpalUniversalLocation,
			},
			PenpalAParaPallet as PenpalAPallet, PenpalAssetOwner,
			PenpalBParaPallet as PenpalBPallet,
		},
		rococo_emulated_chain::RococoRelayPallet as RococoPallet,
		westend_emulated_chain::{
			genesis::ED as WESTEND_ED, westend_runtime::xcm_config::XcmConfig as WestendXcmConfig,
			WestendRelayPallet as WestendPallet,
		},
		AssetHubRococoPara as AssetHubRococo, AssetHubRococoParaReceiver as AssetHubRococoReceiver,
		AssetHubRococoParaSender as AssetHubRococoSender, AssetHubWestendPara as AssetHubWestend,
		AssetHubWestendParaReceiver as AssetHubWestendReceiver,
		AssetHubWestendParaSender as AssetHubWestendSender, BridgeHubRococoPara as BridgeHubRococo,
		BridgeHubWestendPara as BridgeHubWestend,
		BridgeHubWestendParaReceiver as BridgeHubWestendReceiver,
		BridgeHubWestendParaSender as BridgeHubWestendSender, PenpalAPara as PenpalA,
		PenpalAParaReceiver as PenpalAReceiver, PenpalBPara as PenpalB,
		PenpalBParaReceiver as PenpalBReceiver, PenpalBParaSender as PenpalBSender,
		RococoRelay as Rococo, RococoRelayReceiver as RococoReceiver, WestendRelay as Westend,
		WestendRelayReceiver as WestendReceiver, WestendRelaySender as WestendSender,
	};

	pub const ASSET_ID: u32 = 1;
	pub const ASSET_MIN_BALANCE: u128 = 1000;
}

#[cfg(test)]
mod tests;
