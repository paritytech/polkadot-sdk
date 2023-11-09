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
pub use asset_hub_wococo_emulated_chain;
pub use bridge_hub_rococo_emulated_chain;
pub use bridge_hub_wococo_emulated_chain;
pub use rococo_emulated_chain;
pub use wococo_emulated_chain;

use asset_hub_rococo_emulated_chain::AssetHubRococo;
use asset_hub_wococo_emulated_chain::AssetHubWococo;
use bridge_hub_rococo_emulated_chain::BridgeHubRococo;
use bridge_hub_wococo_emulated_chain::BridgeHubWococo;
use rococo_emulated_chain::Rococo;
use wococo_emulated_chain::Wococo;

// Cumulus
use emulated_integration_tests_common::{
	accounts::{ALICE, BOB},
	impls::{BridgeHubMessageHandler, BridgeMessagesInstance2},
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
		],
		bridge = RococoWococoMockBridge

	},
	pub struct WococoMockNet {
		relay_chain = Wococo,
		parachains = vec![
			AssetHubWococo,
			BridgeHubWococo,
		],
		bridge = WococoRococoMockBridge
	},
}

decl_test_bridges! {
	pub struct RococoWococoMockBridge {
		source = BridgeHubRococoPara,
		target = BridgeHubWococoPara,
		handler = RococoWococoMessageHandler
	},
	pub struct WococoRococoMockBridge {
		source = BridgeHubWococoPara,
		target = BridgeHubRococoPara,
		handler = WococoRococoMessageHandler
	}
}

type BridgeHubRococoRuntime = <BridgeHubRococoPara as Chain>::Runtime;
type BridgeHubWococoRuntime = <BridgeHubWococoPara as Chain>::Runtime;

pub type RococoWococoMessageHandler = BridgeHubMessageHandler<
	BridgeHubRococoRuntime,
	BridgeHubWococoRuntime,
	BridgeMessagesInstance2,
>;
pub type WococoRococoMessageHandler = BridgeHubMessageHandler<
	BridgeHubWococoRuntime,
	BridgeHubRococoRuntime,
	BridgeMessagesInstance2,
>;

decl_test_sender_receiver_accounts_parameter_types! {
	RococoRelay { sender: ALICE, receiver: BOB },
	AssetHubRococoPara { sender: ALICE, receiver: BOB },
	BridgeHubRococoPara { sender: ALICE, receiver: BOB },
	WococoRelay { sender: ALICE, receiver: BOB },
	AssetHubWococoPara { sender: ALICE, receiver: BOB },
	BridgeHubWococoPara { sender: ALICE, receiver: BOB }
}
