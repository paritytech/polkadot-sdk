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

pub mod constants;
pub mod impls;
pub mod macros;
pub mod xcm_helpers;

use constants::{
	accounts::{ALICE, BOB},
	asset_hub_rococo, asset_hub_westend, asset_hub_wococo, bridge_hub_rococo, penpal, rococo,
	westend,
};
use impls::{RococoWococoMessageHandler, WococoRococoMessageHandler};
pub use paste;

// Substrate
use frame_support::traits::OnInitialize;
pub use pallet_balances;

// Cumulus
pub use cumulus_pallet_xcmp_queue;
pub use xcm_emulator::Chain;
use xcm_emulator::{
	decl_test_bridges, decl_test_networks, decl_test_parachains, decl_test_relay_chains,
	decl_test_sender_receiver_accounts_parameter_types, DefaultMessageProcessor,
};

// Polkadot
pub use pallet_xcm;
pub use xcm::prelude::{AccountId32, WeightLimit};

decl_test_relay_chains! {
	#[api_version(8)]
	pub struct Westend {
		genesis = westend::genesis(),
		on_init = (),
		runtime = westend_runtime,
		core = {
			MessageProcessor: DefaultMessageProcessor<Westend>,
			SovereignAccountOf: westend_runtime::xcm_config::LocationConverter, //TODO: rename to SovereignAccountOf,
		},
		pallets = {
			XcmPallet: westend_runtime::XcmPallet,
			Sudo: westend_runtime::Sudo,
			Balances: westend_runtime::Balances,
			Treasury: westend_runtime::Treasury,
			AssetRate: westend_runtime::AssetRate,
		}
	},
	#[api_version(8)]
	pub struct Rococo {
		genesis = rococo::genesis(),
		on_init = (),
		runtime = rococo_runtime,
		core = {
			MessageProcessor: DefaultMessageProcessor<Rococo>,
			SovereignAccountOf: rococo_runtime::xcm_config::LocationConverter, //TODO: rename to SovereignAccountOf,
		},
		pallets = {
			XcmPallet: rococo_runtime::XcmPallet,
			Sudo: rococo_runtime::Sudo,
			Balances: rococo_runtime::Balances,
			Hrmp: rococo_runtime::Hrmp,
		}
	},
	#[api_version(8)]
	pub struct Wococo {
		genesis = rococo::genesis(),
		on_init = (),
		runtime = rococo_runtime,
		core = {
			MessageProcessor: DefaultMessageProcessor<Wococo>,
			SovereignAccountOf: rococo_runtime::xcm_config::LocationConverter, //TODO: rename to SovereignAccountOf,
		},
		pallets = {
			XcmPallet: rococo_runtime::XcmPallet,
			Sudo: rococo_runtime::Sudo,
			Balances: rococo_runtime::Balances,
		}
	}
}

decl_test_parachains! {
	// Westend Parachains
	pub struct AssetHubWestend {
		genesis = asset_hub_westend::genesis(),
		on_init = {
			asset_hub_westend_runtime::AuraExt::on_initialize(1);
		},
		runtime = asset_hub_westend_runtime,
		core = {
			XcmpMessageHandler: asset_hub_westend_runtime::XcmpQueue,
			DmpMessageHandler: asset_hub_westend_runtime::DmpQueue,
			LocationToAccountId: asset_hub_westend_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: asset_hub_westend_runtime::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: asset_hub_westend_runtime::PolkadotXcm,
			Balances: asset_hub_westend_runtime::Balances,
			Assets: asset_hub_westend_runtime::Assets,
			ForeignAssets: asset_hub_westend_runtime::ForeignAssets,
			PoolAssets: asset_hub_westend_runtime::PoolAssets,
			AssetConversion: asset_hub_westend_runtime::AssetConversion,
		}
	},
	pub struct PenpalWestendA {
		genesis = penpal::genesis(penpal::PARA_ID_A),
		on_init = {
			penpal_runtime::AuraExt::on_initialize(1);
		},
		runtime = penpal_runtime,
		core = {
			XcmpMessageHandler: penpal_runtime::XcmpQueue,
			DmpMessageHandler: penpal_runtime::DmpQueue,
			LocationToAccountId: penpal_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: penpal_runtime::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: penpal_runtime::PolkadotXcm,
			Assets: penpal_runtime::Assets,
			Balances: penpal_runtime::Balances,
		}
	},
	// Rococo Parachains
	pub struct BridgeHubRococo {
		genesis = bridge_hub_rococo::genesis(),
		on_init = {
			bridge_hub_rococo_runtime::AuraExt::on_initialize(1);
		},
		runtime = bridge_hub_rococo_runtime,
		core = {
			XcmpMessageHandler: bridge_hub_rococo_runtime::XcmpQueue,
			DmpMessageHandler: bridge_hub_rococo_runtime::DmpQueue,
			LocationToAccountId: bridge_hub_rococo_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: bridge_hub_rococo_runtime::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: bridge_hub_rococo_runtime::PolkadotXcm,
			Balances: bridge_hub_rococo_runtime::Balances,
		}
	},
	// AssetHubRococo
	pub struct AssetHubRococo {
		genesis = asset_hub_rococo::genesis(),
		on_init = {
			asset_hub_rococo_runtime::AuraExt::on_initialize(1);
		},
		runtime = asset_hub_rococo_runtime,
		core = {
			XcmpMessageHandler: asset_hub_rococo_runtime::XcmpQueue,
			DmpMessageHandler: asset_hub_rococo_runtime::DmpQueue,
			LocationToAccountId: asset_hub_rococo_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: asset_hub_rococo_runtime::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: asset_hub_rococo_runtime::PolkadotXcm,
			Assets: asset_hub_rococo_runtime::Assets,
			ForeignAssets: asset_hub_rococo_runtime::ForeignAssets,
			PoolAssets: asset_hub_rococo_runtime::PoolAssets,
			AssetConversion: asset_hub_rococo_runtime::AssetConversion,
			Balances: asset_hub_rococo_runtime::Balances,
		}
	},
	pub struct PenpalRococoA {
		genesis = penpal::genesis(penpal::PARA_ID_A),
		on_init = {
			penpal_runtime::AuraExt::on_initialize(1);
		},
		runtime = penpal_runtime,
		core = {
			XcmpMessageHandler: penpal_runtime::XcmpQueue,
			DmpMessageHandler: penpal_runtime::DmpQueue,
			LocationToAccountId: penpal_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: penpal_runtime::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: penpal_runtime::PolkadotXcm,
			Assets: penpal_runtime::Assets,
		}
	},
	pub struct PenpalRococoB {
		genesis = penpal::genesis(penpal::PARA_ID_B),
		on_init = {
			penpal_runtime::AuraExt::on_initialize(1);
		},
		runtime = penpal_runtime,
		core = {
			XcmpMessageHandler: penpal_runtime::XcmpQueue,
			DmpMessageHandler: penpal_runtime::DmpQueue,
			LocationToAccountId: penpal_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: penpal_runtime::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: penpal_runtime::PolkadotXcm,
			Assets: penpal_runtime::Assets,
		}
	},
	// Wococo Parachains
	pub struct BridgeHubWococo {
		genesis = bridge_hub_rococo::genesis(),
		on_init = {
			bridge_hub_rococo_runtime::AuraExt::on_initialize(1);
			// TODO: manage to set_wococo_flavor with `set_storage`
		},
		runtime = bridge_hub_rococo_runtime,
		core = {
			XcmpMessageHandler: bridge_hub_rococo_runtime::XcmpQueue,
			DmpMessageHandler: bridge_hub_rococo_runtime::DmpQueue,
			LocationToAccountId: bridge_hub_rococo_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: bridge_hub_rococo_runtime::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: bridge_hub_rococo_runtime::PolkadotXcm,
		}
	},
	pub struct AssetHubWococo {
		genesis = asset_hub_wococo::genesis(),
		on_init = {
			asset_hub_rococo_runtime::AuraExt::on_initialize(1);
			// TODO: manage to set_wococo_flavor with `set_storage`
		},
		runtime = asset_hub_rococo_runtime,
		core = {
			XcmpMessageHandler: asset_hub_rococo_runtime::XcmpQueue,
			DmpMessageHandler: asset_hub_rococo_runtime::DmpQueue,
			LocationToAccountId: asset_hub_rococo_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: asset_hub_rococo_runtime::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: asset_hub_rococo_runtime::PolkadotXcm,
			Assets: asset_hub_rococo_runtime::Assets,
			ForeignAssets: asset_hub_rococo_runtime::ForeignAssets,
			PoolAssets: asset_hub_rococo_runtime::PoolAssets,
			AssetConversion: asset_hub_rococo_runtime::AssetConversion,
			Balances: asset_hub_rococo_runtime::Balances,
		}
	}
}

decl_test_networks! {
	pub struct WestendMockNet {
		relay_chain = Westend,
		parachains = vec![
			AssetHubWestend,
			PenpalWestendA,
		],
		bridge = ()
	},
	pub struct RococoMockNet {
		relay_chain = Rococo,
		parachains = vec![
			AssetHubRococo,
			BridgeHubRococo,
			PenpalRococoA,
			PenpalRococoB,
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
	}
}

decl_test_bridges! {
	pub struct RococoWococoMockBridge {
		source = BridgeHubRococo,
		target = BridgeHubWococo,
		handler = RococoWococoMessageHandler
	},
	pub struct WococoRococoMockBridge {
		source = BridgeHubWococo,
		target = BridgeHubRococo,
		handler = WococoRococoMessageHandler
	}
}

// Westend implementation
impl_accounts_helpers_for_relay_chain!(Westend);
impl_assert_events_helpers_for_relay_chain!(Westend);
impl_send_transact_helpers_for_relay_chain!(Westend);

// Rococo implementation
impl_accounts_helpers_for_relay_chain!(Rococo);
impl_assert_events_helpers_for_relay_chain!(Rococo);
impl_hrmp_channels_helpers_for_relay_chain!(Rococo);
impl_send_transact_helpers_for_relay_chain!(Rococo);

// Wococo implementation
impl_accounts_helpers_for_relay_chain!(Wococo);
impl_assert_events_helpers_for_relay_chain!(Wococo);
impl_send_transact_helpers_for_relay_chain!(Wococo);

// AssetHubWestend implementation
impl_accounts_helpers_for_parachain!(AssetHubWestend);
impl_assets_helpers_for_parachain!(AssetHubWestend, Westend);
impl_assert_events_helpers_for_parachain!(AssetHubWestend);

// AssetHubRococo implementation
impl_accounts_helpers_for_parachain!(AssetHubRococo);
impl_assets_helpers_for_parachain!(AssetHubRococo, Rococo);
impl_assert_events_helpers_for_parachain!(AssetHubRococo);

// PenpalWestendA implementation
impl_assert_events_helpers_for_parachain!(PenpalWestendA);

// BridgeHubRococo implementation
impl_accounts_helpers_for_parachain!(BridgeHubRococo);
impl_assert_events_helpers_for_parachain!(BridgeHubRococo);

// PenpalRococo implementations
impl_assert_events_helpers_for_parachain!(PenpalRococoA);
impl_assert_events_helpers_for_parachain!(PenpalRococoB);

decl_test_sender_receiver_accounts_parameter_types! {
	// Relays
	Westend { sender: ALICE, receiver: BOB },
	Rococo { sender: ALICE, receiver: BOB },
	Wococo { sender: ALICE, receiver: BOB },
	// Asset Hubs
	AssetHubWestend { sender: ALICE, receiver: BOB },
	AssetHubRococo { sender: ALICE, receiver: BOB },
	AssetHubWococo { sender: ALICE, receiver: BOB },
	// Bridged Hubs
	BridgeHubRococo { sender: ALICE, receiver: BOB },
	BridgeHubWococo { sender: ALICE, receiver: BOB },
	// Penpals
	PenpalWestendA { sender: ALICE, receiver: BOB },
	PenpalRococoA { sender: ALICE, receiver: BOB },
	PenpalRococoB { sender: ALICE, receiver: BOB }
}
