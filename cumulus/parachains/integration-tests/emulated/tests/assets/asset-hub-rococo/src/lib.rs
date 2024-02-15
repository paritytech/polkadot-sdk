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
	pub use codec::Encode;

	// Substrate
	pub use frame_support::{
		assert_err, assert_ok,
		pallet_prelude::Weight,
		sp_runtime::{DispatchError, DispatchResult, ModuleError},
		traits::fungibles::Inspect,
	};

	// Polkadot
	pub use xcm::{
		prelude::{AccountId32 as AccountId32Junction, *},
		v3,
	};

	// Cumulus
	pub use asset_test_utils::xcm_helpers;
	pub use emulated_integration_tests_common::{
		test_parachain_is_trusted_teleporter,
		xcm_emulator::{
			assert_expected_events, bx, Chain, Parachain as Para,
			RelayChain as Relay, Test, TestArgs, TestContext, TestExt,
		},
		xcm_helpers::{xcm_transact_paid_execution, non_fee_asset},
		XCM_V3,
	};
	pub use parachains_common::Balance;
	pub use rococo_system_emulated_network::{
		asset_hub_rococo_emulated_chain::{
			genesis::{ED as ASSET_HUB_ROCOCO_ED, PARA_ID as ASSETHUB_PARA_ID}, AssetHubRococoParaPallet as AssetHubRococoPallet,
		},
		penpal_emulated_chain::{PenpalAParaPallet as PenpalAPallet, PenpalBParaPallet as PenpalBPallet, asset_owner as penpal_asset_owner},
		rococo_emulated_chain::{genesis::ED as ROCOCO_ED, RococoRelayPallet as RococoPallet},
		AssetHubRococoPara as AssetHubRococo, AssetHubRococoParaReceiver as AssetHubRococoReceiver,
		AssetHubRococoParaSender as AssetHubRococoSender, BridgeHubRococoPara as BridgeHubRococo,
		BridgeHubRococoParaReceiver as BridgeHubRococoReceiver, PenpalAPara as PenpalA,
		PenpalAParaReceiver as PenpalAReceiver, PenpalAParaSender as PenpalASender,
		PenpalBPara as PenpalB, PenpalBParaReceiver as PenpalBReceiver, RococoRelay as Rococo,
		RococoRelayReceiver as RococoReceiver, RococoRelaySender as RococoSender,
	};

	// Runtimes
	pub use rococo_runtime::xcm_config::{XcmConfig as RococoXcmConfig, UniversalLocation as RococoUniversalLocation};
	pub use asset_hub_rococo_runtime::xcm_config::{
		XcmConfig as AssetHubRococoXcmConfig, UniversalLocation as AssetHubRococoUniversalLocation,
		TokenLocationV3 as RelayLocationV3,
	};
	pub use penpal_runtime::xcm_config::{
		LocalTeleportableToAssetHubV3 as PenpalLocalTeleportableToAssetHubV3,
		SystemAssetHubLocationV3,
		UniversalLocation as PenpalUniversalLocation, XcmConfig as PenpalRococoXcmConfig
	};

	pub const ASSET_ID: u32 = 1;
	pub const ASSET_MIN_BALANCE: u128 = 1000;
	// `Assets` pallet index
	pub const ASSETS_PALLET_ID: u8 = 50;

	pub type RelayToSystemParaTest = Test<Rococo, AssetHubRococo>;
	pub type RelayToParaTest = Test<Rococo, PenpalA>;
	pub type SystemParaToRelayTest = Test<AssetHubRococo, Rococo>;
	pub type SystemParaToParaTest = Test<AssetHubRococo, PenpalA>;
	pub type ParaToSystemParaTest = Test<PenpalA, AssetHubRococo>;
	pub type ParaToParaThroughRelayTest = Test<PenpalA, PenpalB, Rococo>;
	pub type ParaToParaThroughSystemParaTest = Test<PenpalA, PenpalB, AssetHubRococo>;

	emulated_integration_tests_common::include_penpal_create_foreign_asset_on_asset_hub!(
		PenpalA,
		AssetHubRococo,
		ROCOCO_ED,
		testnet_parachains_constants::rococo::fee::WeightToFee
	);
}

#[cfg(test)]
mod tests;
