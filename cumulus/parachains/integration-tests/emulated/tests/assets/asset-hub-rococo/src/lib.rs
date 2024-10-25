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
	pub use xcm::prelude::{AccountId32 as AccountId32Junction, *};
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
		ASSETS_PALLET_ID, RESERVABLE_ASSET_ID, XCM_V3,
	};
	pub use parachains_common::Balance;
	pub use rococo_system_emulated_network::{
		asset_hub_rococo_emulated_chain::{
			asset_hub_rococo_runtime::{
				self,
				xcm_config::{
					self as ahr_xcm_config, TokenLocation as RelayLocation,
					XcmConfig as AssetHubRococoXcmConfig,
				},
				AssetConversionOrigin as AssetHubRococoAssetConversionOrigin,
				ExistentialDeposit as AssetHubRococoExistentialDeposit,
			},
			genesis::{AssetHubRococoAssetOwner, ED as ASSET_HUB_ROCOCO_ED},
			AssetHubRococoParaPallet as AssetHubRococoPallet,
		},
		penpal_emulated_chain::{
			penpal_runtime::xcm_config::{
				CustomizableAssetFromSystemAssetHub as PenpalCustomizableAssetFromSystemAssetHub,
				LocalReservableFromAssetHub as PenpalLocalReservableFromAssetHub,
				LocalTeleportableToAssetHub as PenpalLocalTeleportableToAssetHub,
				UsdtFromAssetHub as PenpalUsdtFromAssetHub,
			},
			PenpalAParaPallet as PenpalAPallet, PenpalAssetOwner,
			PenpalBParaPallet as PenpalBPallet, ED as PENPAL_ED,
		},
		rococo_emulated_chain::{
			genesis::ED as ROCOCO_ED,
			rococo_runtime::{
				governance as rococo_governance,
				xcm_config::{
					UniversalLocation as RococoUniversalLocation, XcmConfig as RococoXcmConfig,
				},
				OriginCaller as RococoOriginCaller,
			},
			RococoRelayPallet as RococoPallet,
		},
		AssetHubRococoPara as AssetHubRococo, AssetHubRococoParaReceiver as AssetHubRococoReceiver,
		AssetHubRococoParaSender as AssetHubRococoSender, BridgeHubRococoPara as BridgeHubRococo,
		BridgeHubRococoParaReceiver as BridgeHubRococoReceiver, PenpalAPara as PenpalA,
		PenpalAParaReceiver as PenpalAReceiver, PenpalAParaSender as PenpalASender,
		PenpalBPara as PenpalB, PenpalBParaReceiver as PenpalBReceiver, RococoRelay as Rococo,
		RococoRelayReceiver as RococoReceiver, RococoRelaySender as RococoSender,
	};

	pub const ASSET_ID: u32 = 3;
	pub const ASSET_MIN_BALANCE: u128 = 1000;

	pub type RelayToParaTest = Test<Rococo, PenpalA>;
	pub type ParaToRelayTest = Test<PenpalA, Rococo>;
	pub type SystemParaToRelayTest = Test<AssetHubRococo, Rococo>;
	pub type SystemParaToParaTest = Test<AssetHubRococo, PenpalA>;
	pub type ParaToSystemParaTest = Test<PenpalA, AssetHubRococo>;
	pub type ParaToParaThroughRelayTest = Test<PenpalA, PenpalB, Rococo>;
	pub type ParaToParaThroughAHTest = Test<PenpalA, PenpalB, AssetHubRococo>;
	pub type RelayToParaThroughAHTest = Test<Rococo, PenpalA, AssetHubRococo>;
}

#[cfg(test)]
mod tests;
