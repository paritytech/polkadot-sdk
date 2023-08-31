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

pub mod constants;
pub mod impls;
pub mod xcm_helpers;

use constants::{
	accounts::{ALICE, BOB},
	asset_hub_kusama, asset_hub_polkadot, asset_hub_westend, bridge_hub_kusama,
	bridge_hub_polkadot, bridge_hub_rococo, collectives, kusama, penpal, polkadot, rococo, westend,
};
use impls::{RococoWococoMessageHandler, WococoRococoMessageHandler};

// Substrate
use frame_support::traits::OnInitialize;

// Cumulus
use xcm_emulator::{
	decl_test_bridges, decl_test_networks, decl_test_parachains, decl_test_relay_chains,
	decl_test_sender_receiver_accounts_parameter_types, DefaultMessageProcessor,
};

decl_test_relay_chains! {
	#[api_version(5)]
	pub struct Polkadot {
		genesis = polkadot::genesis(),
		on_init = (),
		runtime = polkadot_runtime,
		core = {
			MessageProcessor: DefaultMessageProcessor<Polkadot>,
			SovereignAccountOf: polkadot_runtime::xcm_config::SovereignAccountOf,
		},
		pallets = {
			XcmPallet: polkadot_runtime::XcmPallet,
			Balances: polkadot_runtime::Balances,
			Hrmp: polkadot_runtime::Hrmp,
		}
	},
	#[api_version(5)]
	pub struct Kusama {
		genesis = kusama::genesis(),
		on_init = (),
		runtime = kusama_runtime,
		core = {
			MessageProcessor: DefaultMessageProcessor<Kusama>,
			SovereignAccountOf: kusama_runtime::xcm_config::SovereignAccountOf,
		},
		pallets = {
			XcmPallet: kusama_runtime::XcmPallet,
			Balances: kusama_runtime::Balances,
			Hrmp: kusama_runtime::Hrmp,
		}
	},
	#[api_version(6)]
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
		}
	},
	#[api_version(5)]
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
		}
	},
	#[api_version(5)]
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
	// Polkadot Parachains
	pub struct AssetHubPolkadot {
		genesis = asset_hub_polkadot::genesis(),
		on_init = {
			asset_hub_polkadot_runtime::AuraExt::on_initialize(1);
		},
		runtime = asset_hub_polkadot_runtime,
		core = {
			XcmpMessageHandler: asset_hub_polkadot_runtime::XcmpQueue,
			DmpMessageHandler: asset_hub_polkadot_runtime::DmpQueue,
			LocationToAccountId: asset_hub_polkadot_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: asset_hub_polkadot_runtime::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: asset_hub_polkadot_runtime::PolkadotXcm,
			Assets: asset_hub_polkadot_runtime::Assets,
			Balances: asset_hub_polkadot_runtime::Balances,
		}
	},
	pub struct Collectives {
		genesis = collectives::genesis(),
		on_init = {
			collectives_polkadot_runtime::AuraExt::on_initialize(1);
		},
		runtime = collectives_polkadot_runtime,
		core = {
			XcmpMessageHandler: collectives_polkadot_runtime::XcmpQueue,
			DmpMessageHandler: collectives_polkadot_runtime::DmpQueue,
			LocationToAccountId: collectives_polkadot_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: collectives_polkadot_runtime::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: collectives_polkadot_runtime::PolkadotXcm,
			Balances: collectives_polkadot_runtime::Balances,
		}
	},
	pub struct BridgeHubPolkadot {
		genesis = bridge_hub_polkadot::genesis(),
		on_init = {
			bridge_hub_polkadot_runtime::AuraExt::on_initialize(1);
		},
		runtime = bridge_hub_polkadot_runtime,
		core = {
			XcmpMessageHandler: bridge_hub_polkadot_runtime::XcmpQueue,
			DmpMessageHandler: bridge_hub_polkadot_runtime::DmpQueue,
			LocationToAccountId: bridge_hub_polkadot_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: bridge_hub_polkadot_runtime::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: bridge_hub_polkadot_runtime::PolkadotXcm,
		}
	},
	pub struct PenpalPolkadotA {
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
	pub struct PenpalPolkadotB {
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
	// Kusama Parachains
	pub struct AssetHubKusama {
		genesis = asset_hub_kusama::genesis(),
		on_init = {
			asset_hub_kusama_runtime::AuraExt::on_initialize(1);
		},
		runtime = asset_hub_kusama_runtime,
		core = {
			XcmpMessageHandler: asset_hub_kusama_runtime::XcmpQueue,
			DmpMessageHandler: asset_hub_kusama_runtime::DmpQueue,
			LocationToAccountId: asset_hub_kusama_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: asset_hub_kusama_runtime::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: asset_hub_kusama_runtime::PolkadotXcm,
			Assets: asset_hub_kusama_runtime::Assets,
			ForeignAssets: asset_hub_kusama_runtime::ForeignAssets,
			PoolAssets: asset_hub_kusama_runtime::PoolAssets,
			AssetConversion: asset_hub_kusama_runtime::AssetConversion,
			Balances: asset_hub_kusama_runtime::Balances,
		}
	},
	pub struct BridgeHubKusama {
		genesis = bridge_hub_kusama::genesis(),
		on_init = {
			bridge_hub_kusama_runtime::AuraExt::on_initialize(1);
		},
		runtime = bridge_hub_kusama_runtime,
		core = {
			XcmpMessageHandler: bridge_hub_kusama_runtime::XcmpQueue,
			DmpMessageHandler: bridge_hub_kusama_runtime::DmpQueue,
			LocationToAccountId: bridge_hub_kusama_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: bridge_hub_kusama_runtime::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: bridge_hub_kusama_runtime::PolkadotXcm,
		}
	},
	pub struct PenpalKusamaA {
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
	pub struct PenpalKusamaB {
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
	// AssetHubRococo (aka Rockmine/Rockmine2) mirrors AssetHubKusama
	pub struct AssetHubRococo {
		genesis = asset_hub_kusama::genesis(),
		on_init = {
			asset_hub_polkadot_runtime::AuraExt::on_initialize(1);
		},
		runtime = asset_hub_kusama_runtime,
		core = {
			XcmpMessageHandler: asset_hub_kusama_runtime::XcmpQueue,
			DmpMessageHandler: asset_hub_kusama_runtime::DmpQueue,
			LocationToAccountId: asset_hub_kusama_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: asset_hub_kusama_runtime::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: asset_hub_kusama_runtime::PolkadotXcm,
			Assets: asset_hub_kusama_runtime::Assets,
		}
	},
	// Wococo Parachains
	pub struct BridgeHubWococo {
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
		}
	},
	pub struct AssetHubWococo {
		genesis = asset_hub_polkadot::genesis(),
		on_init = {
			asset_hub_polkadot_runtime::AuraExt::on_initialize(1);
		},
		runtime = asset_hub_polkadot_runtime,
		core = {
			XcmpMessageHandler: asset_hub_polkadot_runtime::XcmpQueue,
			DmpMessageHandler: asset_hub_polkadot_runtime::DmpQueue,
			LocationToAccountId: asset_hub_polkadot_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: asset_hub_polkadot_runtime::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: asset_hub_polkadot_runtime::PolkadotXcm,
			Assets: asset_hub_polkadot_runtime::Assets,
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
	}
}

decl_test_networks! {
	pub struct PolkadotMockNet {
		relay_chain = Polkadot,
		parachains = vec![
			AssetHubPolkadot,
			Collectives,
			BridgeHubPolkadot,
			PenpalPolkadotA,
			PenpalPolkadotB,
		],
		// TODO: uncomment when https://github.com/paritytech/cumulus/pull/2528 is merged
		// bridge = PolkadotKusamaMockBridge
		bridge = ()
	},
	pub struct KusamaMockNet {
		relay_chain = Kusama,
		parachains = vec![
			AssetHubKusama,
			PenpalKusamaA,
			BridgeHubKusama,
			PenpalKusamaB,
		],
		// TODO: uncomment when https://github.com/paritytech/cumulus/pull/2528 is merged
		// bridge = KusamaPolkadotMockBridge
		bridge = ()
	},
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
	// TODO: uncomment when https://github.com/paritytech/cumulus/pull/2528 is merged
	// pub struct PolkadotKusamaMockBridge {
	// 	source = BridgeHubPolkadot,
	// 	target = BridgeHubKusama,
	//  handler = PolkadotKusamaMessageHandler
	// },
	// pub struct KusamaPolkadotMockBridge {
	// 	source = BridgeHubKusama,
	// 	target = BridgeHubPolkadot,
	// 	handler = KusamaPolkadotMessageHandler
	// }
}

// Polkadot implementation
impl_accounts_helpers_for_relay_chain!(Polkadot);
impl_assert_events_helpers_for_relay_chain!(Polkadot);
impl_hrmp_channels_helpers_for_relay_chain!(Polkadot);

// Kusama implementation
impl_accounts_helpers_for_relay_chain!(Kusama);
impl_assert_events_helpers_for_relay_chain!(Kusama);
impl_hrmp_channels_helpers_for_relay_chain!(Kusama);

// Westend implementation
impl_accounts_helpers_for_relay_chain!(Westend);
impl_assert_events_helpers_for_relay_chain!(Westend);

// Rococo implementation
impl_accounts_helpers_for_relay_chain!(Rococo);
impl_assert_events_helpers_for_relay_chain!(Rococo);

// Wococo implementation
impl_accounts_helpers_for_relay_chain!(Wococo);
impl_assert_events_helpers_for_relay_chain!(Wococo);

// AssetHubPolkadot implementation
impl_accounts_helpers_for_parachain!(AssetHubPolkadot);
impl_assets_helpers_for_parachain!(AssetHubPolkadot, Polkadot);
impl_assert_events_helpers_for_parachain!(AssetHubPolkadot);

// AssetHubKusama implementation
impl_accounts_helpers_for_parachain!(AssetHubKusama);
impl_assets_helpers_for_parachain!(AssetHubKusama, Kusama);
impl_assert_events_helpers_for_parachain!(AssetHubKusama);

// AssetHubWestend implementation
impl_accounts_helpers_for_parachain!(AssetHubWestend);
impl_assets_helpers_for_parachain!(AssetHubWestend, Westend);
impl_assert_events_helpers_for_parachain!(AssetHubWestend);

// PenpalPolkadot implementations
impl_assert_events_helpers_for_parachain!(PenpalPolkadotA);
impl_assert_events_helpers_for_parachain!(PenpalPolkadotB);

// PenpalKusama implementations
impl_assert_events_helpers_for_parachain!(PenpalKusamaA);
impl_assert_events_helpers_for_parachain!(PenpalKusamaB);

// PenpalWestendA implementation
impl_assert_events_helpers_for_parachain!(PenpalWestendA);

// Collectives implementation
impl_accounts_helpers_for_parachain!(Collectives);
impl_assert_events_helpers_for_parachain!(Collectives);

// BridgeHubRococo implementation
impl_accounts_helpers_for_parachain!(BridgeHubRococo);
impl_assert_events_helpers_for_parachain!(BridgeHubRococo);

decl_test_sender_receiver_accounts_parameter_types! {
	// Relays
	Polkadot { sender: ALICE, receiver: BOB },
	Kusama { sender: ALICE, receiver: BOB },
	Westend { sender: ALICE, receiver: BOB },
	Rococo { sender: ALICE, receiver: BOB },
	Wococo { sender: ALICE, receiver: BOB },
	// Asset Hubs
	AssetHubPolkadot { sender: ALICE, receiver: BOB },
	AssetHubKusama { sender: ALICE, receiver: BOB },
	AssetHubWestend { sender: ALICE, receiver: BOB },
	AssetHubRococo { sender: ALICE, receiver: BOB },
	AssetHubWococo { sender: ALICE, receiver: BOB },
	// Collectives
	Collectives { sender: ALICE, receiver: BOB },
	// Bridged Hubs
	BridgeHubPolkadot { sender: ALICE, receiver: BOB },
	BridgeHubKusama { sender: ALICE, receiver: BOB },
	BridgeHubRococo { sender: ALICE, receiver: BOB },
	BridgeHubWococo { sender: ALICE, receiver: BOB },
	// Penpals
	PenpalPolkadotA { sender: ALICE, receiver: BOB },
	PenpalPolkadotB { sender: ALICE, receiver: BOB },
	PenpalKusamaA { sender: ALICE, receiver: BOB },
	PenpalKusamaB { sender: ALICE, receiver: BOB },
	PenpalWestendA { sender: ALICE, receiver: BOB },
	PenpalRococoA { sender: ALICE, receiver: BOB }
}
