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
	assert_ok, instances::Instance1, pallet_prelude::Weight, traits::fungibles::Inspect,
};
pub use integration_tests_common::{
	constants::{
		accounts::{ALICE, BOB},
		kusama::ED as KUSAMA_ED,
		PROOF_SIZE_THRESHOLD, REF_TIME_THRESHOLD, XCM_V3,
	},
	AccountId, AssetHubKusama, AssetHubKusamaPallet, AssetHubKusamaReceiver, AssetHubKusamaSender,
	BridgeHubKusama, BridgeHubKusamaPallet, BridgeHubKusamaReceiver, BridgeHubKusamaSender,
	BridgeHubPolkadot, BridgeHubPolkadotPallet, BridgeHubPolkadotReceiver, BridgeHubPolkadotSender,
	Collectives, CollectivesPallet, CollectivesReceiver, CollectivesSender, Kusama, KusamaMockNet,
	KusamaPallet, KusamaReceiver, KusamaSender, PenpalKusama, PenpalKusamaReceiver,
	PenpalKusamaSender, PenpalPolkadot, PenpalPolkadotReceiver, PenpalPolkadotSender, Polkadot,
	PolkadotMockNet, PolkadotPallet, PolkadotReceiver, PolkadotSender,
};
pub use polkadot_core_primitives::InboundDownwardMessage;
pub use xcm::{
	prelude::*,
	v3::{Error, NetworkId::Kusama as KusamaId},
};
pub use xcm_emulator::{
	assert_expected_events, bx, cumulus_pallet_dmp_queue, helpers::weight_within_threshold,
	Parachain as Para, RelayChain as Relay, TestExt,
};

#[cfg(test)]
mod tests;
