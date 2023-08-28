// Copyright Parity Technologies (UK) Ltd.
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
pub use codec::Encode;
pub use frame_support::{
	assert_err, assert_ok,
	instances::Instance1,
	pallet_prelude::Weight,
	sp_runtime::{AccountId32, DispatchError, DispatchResult, MultiAddress},
	traits::{fungibles::Inspect, OriginTrait},
};
pub use integration_tests_common::{
	constants::{
		accounts::{ALICE, BOB},
		asset_hub_kusama::ED as ASSET_HUB_ROCOCO_ED,
		kusama::ED as ROCOCO_ED,
		PROOF_SIZE_THRESHOLD, REF_TIME_THRESHOLD, XCM_V3,
	},
	lazy_static::lazy_static,
	xcm_transact_paid_execution, xcm_transact_unpaid_execution, AssetHubRococo,
	AssetHubRococoPallet, AssetHubRococoReceiver, AssetHubRococoSender, AssetHubWococo,
	AssetHubWococoPallet, AssetHubWococoReceiver, AssetHubWococoSender, BridgeHubRococo,
	BridgeHubRococoPallet, BridgeHubRococoReceiver, BridgeHubRococoSender, BridgeHubWococo,
	BridgeHubWococoPallet, BridgeHubWococoReceiver, BridgeHubWococoSender, Collectives,
	CollectivesPallet, CollectivesReceiver, CollectivesSender, PenpalRococoA, PenpalRococoAPallet,
	PenpalRococoAReceiver, PenpalRococoASender, Rococo, RococoMockNet, RococoPallet,
	RococoReceiver, RococoSender, Wococo, WococoMockNet, WococoPallet, WococoReceiver,
	WococoSender,
};
pub use parachains_common::{AccountId, Balance};
pub use polkadot_core_primitives::InboundDownwardMessage;
pub use polkadot_parachain::primitives::{HrmpChannelId, Id};
pub use polkadot_runtime_parachains::inclusion::{AggregateMessageOrigin, UmpQueueId};
pub use xcm::{
	prelude::*,
	v3::{
		Error,
		NetworkId::{Rococo as RococoId, Wococo as WococoId},
	},
	DoubleEncoded,
};
pub use xcm_emulator::{
	assert_expected_events, bx, cumulus_pallet_dmp_queue, helpers::weight_within_threshold,
	AccountId32Junction, Chain, ParaId, Parachain as Para, RelayChain as Relay, Test, TestArgs,
	TestContext, TestExt, TestExternalities,
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
