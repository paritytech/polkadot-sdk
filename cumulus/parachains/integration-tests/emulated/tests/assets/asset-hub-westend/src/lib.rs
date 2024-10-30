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
		latest::AssetTransferFilter,
		prelude::{AccountId32 as AccountId32Junction, *},
	};
	pub use xcm_executor::traits::TransferType;

	// Cumulus
	pub use asset_test_utils::xcm_helpers;
	pub use emulated_integration_tests_common::{
		accounts::DUMMY_EMPTY,
		test_parachain_is_trusted_teleporter, test_parachain_is_trusted_teleporter_for_relay,
		test_relay_is_trusted_teleporter, test_xcm_fee_querying_apis_work_for_asset_hub,
		xcm_emulator::{
			assert_expected_events, bx, Chain, Parachain as Para, RelayChain as Relay, Test,
			TestArgs, TestContext, TestExt,
		},
		xcm_helpers::{
			get_amount_from_versioned_assets, non_fee_asset, xcm_transact_paid_execution,
		},
		ASSETS_PALLET_ID, RESERVABLE_ASSET_ID, USDT_ID, XCM_V3,
	};
	pub use parachains_common::{AccountId, Balance};
	pub use westend_system_emulated_network::{
		asset_hub_westend_emulated_chain::{
			asset_hub_westend_runtime::{
				self,
				xcm_config::{
					self as ahw_xcm_config, WestendLocation as RelayLocation,
					XcmConfig as AssetHubWestendXcmConfig,
				},
				AssetConversionOrigin as AssetHubWestendAssetConversionOrigin,
				ExistentialDeposit as AssetHubWestendExistentialDeposit,
			},
			genesis::{AssetHubWestendAssetOwner, ED as ASSET_HUB_WESTEND_ED},
			AssetHubWestendParaPallet as AssetHubWestendPallet,
		},
		bridge_hub_westend_emulated_chain::bridge_hub_westend_runtime::xcm_config::{
			self as bhw_xcm_config,
		},
		collectives_westend_emulated_chain::CollectivesWestendParaPallet as CollectivesWestendPallet,
		penpal_emulated_chain::{
			penpal_runtime::xcm_config::{
				CustomizableAssetFromSystemAssetHub as PenpalCustomizableAssetFromSystemAssetHub,
				LocalReservableFromAssetHub as PenpalLocalReservableFromAssetHub,
				LocalTeleportableToAssetHub as PenpalLocalTeleportableToAssetHub,
				UniversalLocation as PenpalUniversalLocation,
				UsdtFromAssetHub as PenpalUsdtFromAssetHub,
			},
			PenpalAParaPallet as PenpalAPallet, PenpalAssetOwner,
			PenpalBParaPallet as PenpalBPallet,
		},
		westend_emulated_chain::{
			genesis::ED as WESTEND_ED,
			westend_runtime::xcm_config::{
				UniversalLocation as WestendUniversalLocation, XcmConfig as WestendXcmConfig,
			},
			WestendRelayPallet as WestendPallet,
		},
		AssetHubWestendPara as AssetHubWestend,
		AssetHubWestendParaReceiver as AssetHubWestendReceiver,
		AssetHubWestendParaSender as AssetHubWestendSender,
		BridgeHubWestendPara as BridgeHubWestend,
		BridgeHubWestendParaReceiver as BridgeHubWestendReceiver,
		CollectivesWestendPara as CollectivesWestend, PenpalAPara as PenpalA,
		PenpalAParaReceiver as PenpalAReceiver, PenpalAParaSender as PenpalASender,
		PenpalBPara as PenpalB, PenpalBParaReceiver as PenpalBReceiver, WestendRelay as Westend,
		WestendRelayReceiver as WestendReceiver, WestendRelaySender as WestendSender,
	};

	pub const ASSET_ID: u32 = 3;
	pub const ASSET_MIN_BALANCE: u128 = 1000;

	pub type RelayToParaTest = Test<Westend, PenpalA>;
	pub type ParaToRelayTest = Test<PenpalA, Westend>;
	pub type SystemParaToRelayTest = Test<AssetHubWestend, Westend>;
	pub type SystemParaToParaTest = Test<AssetHubWestend, PenpalA>;
	pub type ParaToSystemParaTest = Test<PenpalA, AssetHubWestend>;
	pub type ParaToParaThroughRelayTest = Test<PenpalA, PenpalB, Westend>;
	pub type ParaToParaThroughAHTest = Test<PenpalA, PenpalB, AssetHubWestend>;
	pub type RelayToParaThroughAHTest = Test<Westend, PenpalA, AssetHubWestend>;
}

#[cfg(test)]
mod tests;
