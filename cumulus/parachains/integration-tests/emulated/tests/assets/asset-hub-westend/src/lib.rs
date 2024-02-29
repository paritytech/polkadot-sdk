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
		XCM_V3, RESERVABLE_ASSET_ID, ASSETS_PALLET_ID
	};
	pub use parachains_common::{Balance, AccountId};
	pub use westend_system_emulated_network::{
		asset_hub_westend_emulated_chain::{
			genesis::{ED as ASSET_HUB_WESTEND_ED,  AssetHubWestendAssetOwner}, AssetHubWestendParaPallet as AssetHubWestendPallet,
		},
		collectives_westend_emulated_chain::{
			CollectivesWestendParaPallet as CollectivesWestendPallet,
		},
		penpal_emulated_chain::{PenpalAParaPallet as PenpalAPallet, PenpalBParaPallet as PenpalBPallet, PenpalAssetOwner},
		westend_emulated_chain::{genesis::ED as WESTEND_ED, WestendRelayPallet as WestendPallet},
		AssetHubWestendPara as AssetHubWestend, AssetHubWestendParaReceiver as AssetHubWestendReceiver,
		AssetHubWestendParaSender as AssetHubWestendSender, BridgeHubWestendPara as BridgeHubWestend,
		BridgeHubWestendParaReceiver as BridgeHubWestendReceiver, PenpalAPara as PenpalA,
		CollectivesWestendPara as CollectivesWestend,
		PenpalAParaReceiver as PenpalAReceiver, PenpalAParaSender as PenpalASender,
		PenpalBPara as PenpalB, PenpalBParaReceiver as PenpalBReceiver, WestendRelay as Westend,
		WestendRelayReceiver as WestendReceiver, WestendRelaySender as WestendSender,
	};

	// Runtimes
	pub use westend_runtime::xcm_config::{XcmConfig as WestendXcmConfig, UniversalLocation as WestendUniversalLocation};
	pub use asset_hub_westend_runtime::xcm_config::{
		XcmConfig as AssetHubWestendXcmConfig, UniversalLocation as AssetHubWestendUniversalLocation,
		WestendLocationV3 as RelayLocationV3,
	};
	pub use penpal_runtime::xcm_config::{
		LocalTeleportableToAssetHubV3 as PenpalLocalTeleportableToAssetHubV3,
		UniversalLocation as PenpalUniversalLocation, XcmConfig as PenpalWestendXcmConfig,
		LocalReservableFromAssetHubV3 as PenpalLocalReservableFromAssetHubV3,
	};

	pub const ASSET_ID: u32 = 3;
	pub const ASSET_MIN_BALANCE: u128 = 1000;

	pub type RelayToSystemParaTest = Test<Westend, AssetHubWestend>;
	pub type RelayToParaTest = Test<Westend, PenpalA>;
	pub type ParaToRelayTest = Test<PenpalA, Westend>;
	pub type SystemParaToRelayTest = Test<AssetHubWestend, Westend>;
	pub type SystemParaToParaTest = Test<AssetHubWestend, PenpalA>;
	pub type ParaToSystemParaTest = Test<PenpalA, AssetHubWestend>;
	pub type ParaToParaThroughRelayTest = Test<PenpalA, PenpalB, Westend>;
}

#[cfg(test)]
mod tests;
