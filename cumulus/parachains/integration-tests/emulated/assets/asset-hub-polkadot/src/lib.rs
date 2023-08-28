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
		asset_hub_polkadot::ED as ASSET_HUB_POLKADOT_ED,
		polkadot::ED as POLKADOT_ED,
		PROOF_SIZE_THRESHOLD, REF_TIME_THRESHOLD, XCM_V3,
	},
	lazy_static::lazy_static,
	xcm_transact_paid_execution, xcm_transact_unpaid_execution, AssetHubPolkadot,
	AssetHubPolkadotPallet, AssetHubPolkadotReceiver, AssetHubPolkadotSender, BridgeHubPolkadot,
	BridgeHubPolkadotPallet, BridgeHubPolkadotReceiver, BridgeHubPolkadotSender, Collectives,
	CollectivesPallet, CollectivesReceiver, CollectivesSender, PenpalPolkadotA,
	PenpalPolkadotAPallet, PenpalPolkadotAReceiver, PenpalPolkadotASender, PenpalPolkadotB,
	PenpalPolkadotBPallet, PenpalPolkadotBReceiver, PenpalPolkadotBSender, Polkadot,
	PolkadotMockNet, PolkadotPallet, PolkadotReceiver, PolkadotSender,
};
pub use parachains_common::{AccountId, Balance};
pub use polkadot_core_primitives::InboundDownwardMessage;
pub use polkadot_parachain::primitives::{HrmpChannelId, Id};
pub use polkadot_runtime_parachains::inclusion::{AggregateMessageOrigin, UmpQueueId};
pub use xcm::{
	prelude::*,
	v3::{Error, NetworkId::Polkadot as PolkadotId},
	DoubleEncoded,
};
pub use xcm_emulator::{
	assert_expected_events, bx, cumulus_pallet_dmp_queue, helpers::weight_within_threshold,
	AccountId32Junction, Chain, ParaId, Parachain as Para, RelayChain as Relay, Test, TestArgs,
	TestContext, TestExt, TestExternalities,
};

pub const ASSET_ID: u32 = 1;
pub const ASSET_MIN_BALANCE: u128 = 1000;
// `Assets` pallet index
pub const ASSETS_PALLET_ID: u8 = 50;

pub type RelayToSystemParaTest = Test<Polkadot, AssetHubPolkadot>;
pub type SystemParaToRelayTest = Test<AssetHubPolkadot, Polkadot>;
pub type SystemParaToParaTest = Test<AssetHubPolkadot, PenpalPolkadotA>;

/// Returns a `TestArgs` instance to de used for the Relay Chain accross integraton tests
pub fn relay_test_args(amount: Balance) -> TestArgs {
	TestArgs {
		dest: Polkadot::child_location_of(AssetHubPolkadot::para_id()),
		beneficiary: AccountId32Junction {
			network: None,
			id: AssetHubPolkadotReceiver::get().into(),
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
mod tests;
