// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

pub use bp_messages::LaneId;
pub use frame_support::assert_ok;
pub use integration_tests_common::{
	constants::{
		asset_hub_kusama::ED as ASSET_HUB_ROCOCO_ED, kusama::ED as ROCOCO_ED, PROOF_SIZE_THRESHOLD,
		REF_TIME_THRESHOLD, XCM_V3,
	},
	xcm_helpers::{xcm_transact_paid_execution, xcm_transact_unpaid_execution},
	AssetHubRococo, AssetHubRococoReceiver, AssetHubWococo, BridgeHubRococo, BridgeHubWococo,
	PenpalRococoA, Rococo, RococoPallet,
};
pub use parachains_common::{AccountId, Balance};
pub use xcm::{
	prelude::{AccountId32 as AccountId32Junction, *},
	v3::{
		Error,
		NetworkId::{Rococo as RococoId, Wococo as WococoId},
	},
};
pub use xcm_emulator::{
	assert_expected_events, bx, helpers::weight_within_threshold, Chain, Parachain as Para,
	RelayChain as Relay, Test, TestArgs, TestContext, TestExt,
};

pub const ASSET_ID: u32 = 1;
pub const ASSET_MIN_BALANCE: u128 = 1000;
pub const ASSETS_PALLET_ID: u8 = 50;

pub type RelayToSystemParaTest = Test<Rococo, AssetHubRococo>;
pub type SystemParaToRelayTest = Test<AssetHubRococo, Rococo>;
pub type SystemParaToParaTest = Test<AssetHubRococo, PenpalRococoA>;

/// Returns a `TestArgs` instance to de used for the Relay Chain accross integraton tests
pub fn relay_test_args(amount: Balance) -> TestArgs {
	TestArgs {
		dest: Rococo::child_location_of(AssetHubRococo::para_id()),
		beneficiary: AccountId32Junction {
			network: None,
			id: AssetHubRococoReceiver::get().into(),
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
