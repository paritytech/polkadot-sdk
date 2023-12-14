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

pub use asset_hub_westend_emulated_chain;
pub use bridge_hub_westend_emulated_chain;
pub use collectives_westend_emulated_chain;
pub use penpal_emulated_chain;
pub use people_westend_emulated_chain;
pub use westend_emulated_chain;

use asset_hub_westend_emulated_chain::AssetHubWestend;
use bridge_hub_westend_emulated_chain::BridgeHubWestend;
use collectives_westend_emulated_chain::CollectivesWestend;
use penpal_emulated_chain::{PenpalA, PenpalB};
use people_westend_emulated_chain::PeopleWestend;
use westend_emulated_chain::Westend;

// Cumulus
use emulated_integration_tests_common::{
	accounts::{ALICE, BOB},
	xcm_emulator::{decl_test_networks, decl_test_sender_receiver_accounts_parameter_types},
};

decl_test_networks! {
	pub struct WestendMockNet {
		relay_chain = Westend,
		parachains = vec![
			AssetHubWestend,
			BridgeHubWestend,
			CollectivesWestend,
			PeopleWestend,
			PenpalA,
			PenpalB,
		],
		bridge = ()
	},
}

decl_test_sender_receiver_accounts_parameter_types! {
	WestendRelay { sender: ALICE, receiver: BOB },
	AssetHubWestendPara { sender: ALICE, receiver: BOB },
	BridgeHubWestendPara { sender: ALICE, receiver: BOB },
	CollectivesWestendPara { sender: ALICE, receiver: BOB },
	PeopleWestendPara { sender: ALICE, receiver: BOB },
	PenpalAPara { sender: ALICE, receiver: BOB },
	PenpalBPara { sender: ALICE, receiver: BOB }
}
