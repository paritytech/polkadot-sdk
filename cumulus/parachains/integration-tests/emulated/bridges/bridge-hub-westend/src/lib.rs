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

pub use bp_messages::LaneId;
pub use frame_support::assert_ok;
pub use integration_tests_common::{
	constants::{
		bridge_hub_westend::ED as BRIDGE_HUB_WESTEND_ED, westend::ED as WESTEND_ED,
		PROOF_SIZE_THRESHOLD, REF_TIME_THRESHOLD, XCM_V3,
	},
	test_parachain_is_trusted_teleporter,
	xcm_helpers::{xcm_transact_paid_execution, xcm_transact_unpaid_execution},
	AssetHubRococo, AssetHubWestend, AssetHubWestendReceiver, BridgeHubRococo, BridgeHubWestend,
	BridgeHubWestendPallet, BridgeHubWestendSender, PenpalWestendA, Westend, WestendPallet,
};
pub use parachains_common::{AccountId, Balance};
pub use xcm::{
	prelude::{AccountId32 as AccountId32Junction, *},
	v3::{
		Error,
		NetworkId::{Rococo as RococoId, Westend as WestendId},
	},
};
pub use xcm_emulator::{
	assert_expected_events, bx, helpers::weight_within_threshold, Chain, Parachain as Para,
	RelayChain as Relay, Test, TestArgs, TestContext, TestExt,
};

pub const ASSET_ID: u32 = 1;
pub const ASSET_MIN_BALANCE: u128 = 1000;
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

#[cfg(test)]
mod tests;
