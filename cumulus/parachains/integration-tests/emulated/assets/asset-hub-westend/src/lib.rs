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

pub use codec::Encode;
pub use frame_support::{
	assert_err, assert_ok,
	instances::Instance2,
	pallet_prelude::Weight,
	sp_runtime::{AccountId32, DispatchError, DispatchResult, ModuleError},
	traits::fungibles::Inspect,
	BoundedVec,
};
pub use integration_tests_common::{
	constants::{
		asset_hub_westend::ED as ASSET_HUB_WESTEND_ED, westend::ED as WESTEND_ED,
		PROOF_SIZE_THRESHOLD, REF_TIME_THRESHOLD, XCM_V3,
	},
	xcm_helpers::{xcm_transact_paid_execution, xcm_transact_unpaid_execution},
	AssetHubWestend, AssetHubWestendPallet, AssetHubWestendReceiver, AssetHubWestendSender,
	PenpalWestendA, PenpalWestendAPallet, PenpalWestendAReceiver, PenpalWestendASender, Westend,
	WestendPallet, WestendReceiver, WestendSender,
};
pub use parachains_common::{AccountId, Balance};
pub use xcm::{
	prelude::{AccountId32 as AccountId32Junction, *},
	v3::{Error, NetworkId::Westend as WestendId},
};
pub use xcm_emulator::{
	assert_expected_events, bx, helpers::weight_within_threshold, Chain, Parachain as Para,
	RelayChain as Relay, Test, TestArgs, TestContext, TestExt,
};

pub const ASSET_ID: u32 = 1;
pub const ASSET_MIN_BALANCE: u128 = 1000;
// `Assets` pallet index
pub const ASSETS_PALLET_ID: u8 = 50;

pub type RelayToSystemParaTest = Test<Westend, AssetHubWestend>;
pub type SystemParaToRelayTest = Test<AssetHubWestend, Westend>;
pub type SystemParaToParaTest = Test<AssetHubWestend, PenpalWestendA>;

/// Returns a `TestArgs` instance to de used for the Relay Chain accross integraton tests
pub fn relay_test_args(amount: Balance) -> TestArgs {
	TestArgs {
		dest: Westend::child_location_of(AssetHubWestend::para_id()),
		beneficiary: AccountId32Junction {
			network: None,
			id: AssetHubWestendReceiver::get().into(),
		}
		.into(),
		amount,
		assets: (Here, amount).into(),
		asset_id: None,
		fee_asset_item: 0,
		weight_limit: WeightLimit::Unlimited,
	}
}

/// Returns a `TestArgs` instance to de used for the System Parachain accross integraton tests
pub fn system_para_test_args(
	dest: MultiLocation,
	beneficiary_id: AccountId32,
	amount: Balance,
	assets: MultiAssets,
	asset_id: Option<u32>,
) -> TestArgs {
	TestArgs {
		dest,
		beneficiary: AccountId32Junction { network: None, id: beneficiary_id.into() }.into(),
		amount,
		assets,
		asset_id,
		fee_asset_item: 0,
		weight_limit: WeightLimit::Unlimited,
	}
}

#[cfg(test)]
#[cfg(not(feature = "runtime-benchmarks"))]
mod tests;
