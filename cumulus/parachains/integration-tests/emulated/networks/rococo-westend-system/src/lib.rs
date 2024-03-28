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

pub use asset_hub_rococo_emulated_chain;
pub use asset_hub_westend_emulated_chain;
pub use bridge_hub_rococo_emulated_chain;
pub use bridge_hub_westend_emulated_chain;
pub use penpal_emulated_chain;
pub use rococo_emulated_chain;
pub use westend_emulated_chain;

use asset_hub_rococo_emulated_chain::AssetHubRococo;
use asset_hub_westend_emulated_chain::AssetHubWestend;
use bridge_hub_rococo_emulated_chain::BridgeHubRococo;
use bridge_hub_westend_emulated_chain::BridgeHubWestend;
use penpal_emulated_chain::PenpalA;
use rococo_emulated_chain::Rococo;
use westend_emulated_chain::Westend;

// Cumulus
use emulated_integration_tests_common::{
	accounts::{ALICE, BOB},
	impls::{BridgeHubMessageHandler, BridgeMessagesInstance1, BridgeMessagesInstance3},
	xcm_emulator::{
		decl_test_bridges, decl_test_networks, decl_test_sender_receiver_accounts_parameter_types,
		Chain,
	},
};

decl_test_networks! {
	pub struct RococoMockNet {
		relay_chain = Rococo,
		parachains = vec![
			AssetHubRococo,
			BridgeHubRococo,
			PenpalA,
		],
		bridge = RococoWestendMockBridge

	},
	pub struct WestendMockNet {
		relay_chain = Westend,
		parachains = vec![
			AssetHubWestend,
			BridgeHubWestend,
		],
		bridge = WestendRococoMockBridge
	},
}

decl_test_bridges! {
	pub struct RococoWestendMockBridge {
		source = BridgeHubRococoPara,
		target = BridgeHubWestendPara,
		handler = RococoWestendMessageHandler
	},
	pub struct WestendRococoMockBridge {
		source = BridgeHubWestendPara,
		target = BridgeHubRococoPara,
		handler = WestendRococoMessageHandler
	}
}

type BridgeHubRococoRuntime = <BridgeHubRococoPara as Chain>::Runtime;
type BridgeHubWestendRuntime = <BridgeHubWestendPara as Chain>::Runtime;

pub type RococoWestendMessageHandler = BridgeHubMessageHandler<
	BridgeHubRococoRuntime,
	BridgeMessagesInstance3,
	BridgeHubWestendRuntime,
	BridgeMessagesInstance1,
>;
pub type WestendRococoMessageHandler = BridgeHubMessageHandler<
	BridgeHubWestendRuntime,
	BridgeMessagesInstance1,
	BridgeHubRococoRuntime,
	BridgeMessagesInstance3,
>;

decl_test_sender_receiver_accounts_parameter_types! {
	RococoRelay { sender: ALICE, receiver: BOB },
	AssetHubRococoPara { sender: ALICE, receiver: BOB },
	BridgeHubRococoPara { sender: ALICE, receiver: BOB },
	WestendRelay { sender: ALICE, receiver: BOB },
	AssetHubWestendPara { sender: ALICE, receiver: BOB },
	BridgeHubWestendPara { sender: ALICE, receiver: BOB },
	PenpalAPara { sender: ALICE, receiver: BOB }
}
