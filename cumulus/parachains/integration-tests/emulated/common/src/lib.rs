pub mod constants;
pub mod impls;

pub use constants::{
	accounts::{ALICE, BOB},
	asset_hub_kusama, asset_hub_polkadot, asset_hub_westend, bridge_hub_kusama,
	bridge_hub_polkadot, bridge_hub_rococo, collectives, kusama, penpal, polkadot, rococo, westend,
};
pub use impls::{RococoWococoMessageHandler, WococoRococoMessageHandler};

use frame_support::{parameter_types, sp_io, sp_tracing};
pub use parachains_common::{AccountId, AssetHubPolkadotAuraId, AuraId, Balance, BlockNumber};
pub use sp_core::{sr25519, storage::Storage, Get};
use xcm::prelude::*;
use xcm_emulator::{
	decl_test_bridges, decl_test_networks, decl_test_parachains, decl_test_relay_chains,
	decl_test_sender_receiver_accounts_parameter_types, BridgeMessageHandler, Parachain,
	RelayChain, TestExt,
};
use xcm_executor::traits::ConvertLocation;

decl_test_relay_chains! {
	#[api_version(5)]
	pub struct Polkadot {
		genesis = polkadot::genesis(),
		on_init = (),
		runtime = {
			Runtime: polkadot_runtime::Runtime,
			RuntimeOrigin: polkadot_runtime::RuntimeOrigin,
			RuntimeCall: polkadot_runtime::RuntimeCall,
			RuntimeEvent: polkadot_runtime::RuntimeEvent,
			MessageQueue: polkadot_runtime::MessageQueue,
			XcmConfig: polkadot_runtime::xcm_config::XcmConfig,
			SovereignAccountOf: polkadot_runtime::xcm_config::SovereignAccountOf,
			System: polkadot_runtime::System,
			Balances: polkadot_runtime::Balances,
		},
		pallets_extra = {
			XcmPallet: polkadot_runtime::XcmPallet,
		}
	},
	#[api_version(5)]
	pub struct Kusama {
		genesis = kusama::genesis(),
		on_init = (),
		runtime = {
			Runtime: kusama_runtime::Runtime,
			RuntimeOrigin: kusama_runtime::RuntimeOrigin,
			RuntimeCall: kusama_runtime::RuntimeCall,
			RuntimeEvent: kusama_runtime::RuntimeEvent,
			MessageQueue: kusama_runtime::MessageQueue,
			XcmConfig: kusama_runtime::xcm_config::XcmConfig,
			SovereignAccountOf: kusama_runtime::xcm_config::SovereignAccountOf,
			System: kusama_runtime::System,
			Balances: kusama_runtime::Balances,
		},
		pallets_extra = {
			XcmPallet: kusama_runtime::XcmPallet,
		}
	},
	#[api_version(5)]
	pub struct Westend {
		genesis = westend::genesis(),
		on_init = (),
		runtime = {
			Runtime: westend_runtime::Runtime,
			RuntimeOrigin: westend_runtime::RuntimeOrigin,
			RuntimeCall: westend_runtime::RuntimeCall,
			RuntimeEvent: westend_runtime::RuntimeEvent,
			MessageQueue: westend_runtime::MessageQueue,
			XcmConfig: westend_runtime::xcm_config::XcmConfig,
			SovereignAccountOf: westend_runtime::xcm_config::LocationConverter, //TODO: rename to SovereignAccountOf,
			System: westend_runtime::System,
			Balances: westend_runtime::Balances,
		},
		pallets_extra = {
			XcmPallet: westend_runtime::XcmPallet,
			Sudo: westend_runtime::Sudo,
		}
	},
	#[api_version(5)]
	pub struct Rococo {
		genesis = rococo::genesis(),
		on_init = (),
		runtime = {
			Runtime: rococo_runtime::Runtime,
			RuntimeOrigin: rococo_runtime::RuntimeOrigin,
			RuntimeCall: rococo_runtime::RuntimeCall,
			RuntimeEvent: rococo_runtime::RuntimeEvent,
			MessageQueue: rococo_runtime::MessageQueue,
			XcmConfig: rococo_runtime::xcm_config::XcmConfig,
			SovereignAccountOf: rococo_runtime::xcm_config::LocationConverter, //TODO: rename to SovereignAccountOf,
			System: rococo_runtime::System,
			Balances: rococo_runtime::Balances,
		},
		pallets_extra = {
			XcmPallet: rococo_runtime::XcmPallet,
			Sudo: rococo_runtime::Sudo,
		}
	},
	#[api_version(5)]
	pub struct Wococo {
		genesis = rococo::genesis(),
		on_init = (),
		runtime = {
			Runtime: rococo_runtime::Runtime,
			RuntimeOrigin: rococo_runtime::RuntimeOrigin,
			RuntimeCall: rococo_runtime::RuntimeCall,
			RuntimeEvent: rococo_runtime::RuntimeEvent,
			MessageQueue: rococo_runtime::MessageQueue,
			XcmConfig: rococo_runtime::xcm_config::XcmConfig,
			SovereignAccountOf: rococo_runtime::xcm_config::LocationConverter, //TODO: rename to SovereignAccountOf,
			System: rococo_runtime::System,
			Balances: rococo_runtime::Balances,
		},
		pallets_extra = {
			XcmPallet: rococo_runtime::XcmPallet,
			Sudo: rococo_runtime::Sudo,
		}
	}
}

decl_test_parachains! {
	// Polkadot Parachains
	pub struct AssetHubPolkadot {
		genesis = asset_hub_polkadot::genesis(),
		on_init = (),
		runtime = {
			Runtime: asset_hub_polkadot_runtime::Runtime,
			RuntimeOrigin: asset_hub_polkadot_runtime::RuntimeOrigin,
			RuntimeCall: asset_hub_polkadot_runtime::RuntimeCall,
			RuntimeEvent: asset_hub_polkadot_runtime::RuntimeEvent,
			XcmpMessageHandler: asset_hub_polkadot_runtime::XcmpQueue,
			DmpMessageHandler: asset_hub_polkadot_runtime::DmpQueue,
			LocationToAccountId: asset_hub_polkadot_runtime::xcm_config::LocationToAccountId,
			System: asset_hub_polkadot_runtime::System,
			Balances: asset_hub_polkadot_runtime::Balances,
			ParachainSystem: asset_hub_polkadot_runtime::ParachainSystem,
			ParachainInfo: asset_hub_polkadot_runtime::ParachainInfo,
		},
		pallets_extra = {
			PolkadotXcm: asset_hub_polkadot_runtime::PolkadotXcm,
			Assets: asset_hub_polkadot_runtime::Assets,
		}
	},
	pub struct Collectives {
		genesis = collectives::genesis(),
		on_init = (),
		runtime = {
			Runtime: collectives_polkadot_runtime::Runtime,
			RuntimeOrigin: collectives_polkadot_runtime::RuntimeOrigin,
			RuntimeCall: collectives_polkadot_runtime::RuntimeCall,
			RuntimeEvent: collectives_polkadot_runtime::RuntimeEvent,
			XcmpMessageHandler: collectives_polkadot_runtime::XcmpQueue,
			DmpMessageHandler: collectives_polkadot_runtime::DmpQueue,
			LocationToAccountId: collectives_polkadot_runtime::xcm_config::LocationToAccountId,
			System: collectives_polkadot_runtime::System,
			Balances: collectives_polkadot_runtime::Balances,
			ParachainSystem: collectives_polkadot_runtime::ParachainSystem,
			ParachainInfo: collectives_polkadot_runtime::ParachainInfo,
		},
		pallets_extra = {
			PolkadotXcm: collectives_polkadot_runtime::PolkadotXcm,
		}
	},
	pub struct BridgeHubPolkadot {
		genesis = bridge_hub_polkadot::genesis(),
		on_init = (),
		runtime = {
			Runtime: bridge_hub_polkadot_runtime::Runtime,
			RuntimeOrigin: bridge_hub_polkadot_runtime::RuntimeOrigin,
			RuntimeCall: bridge_hub_polkadot_runtime::RuntimeCall,
			RuntimeEvent: bridge_hub_polkadot_runtime::RuntimeEvent,
			XcmpMessageHandler: bridge_hub_polkadot_runtime::XcmpQueue,
			DmpMessageHandler: bridge_hub_polkadot_runtime::DmpQueue,
			LocationToAccountId: bridge_hub_polkadot_runtime::xcm_config::LocationToAccountId,
			System: bridge_hub_polkadot_runtime::System,
			Balances: bridge_hub_polkadot_runtime::Balances,
			ParachainSystem: bridge_hub_polkadot_runtime::ParachainSystem,
			ParachainInfo: bridge_hub_polkadot_runtime::ParachainInfo,
		},
		pallets_extra = {
			PolkadotXcm: bridge_hub_polkadot_runtime::PolkadotXcm,
		}
	},
	pub struct PenpalPolkadot {
		genesis = penpal::genesis(penpal::PARA_ID),
		on_init = (),
		runtime = {
			Runtime: penpal_runtime::Runtime,
			RuntimeOrigin: penpal_runtime::RuntimeOrigin,
			RuntimeCall: penpal_runtime::RuntimeCall,
			RuntimeEvent: penpal_runtime::RuntimeEvent,
			XcmpMessageHandler: penpal_runtime::XcmpQueue,
			DmpMessageHandler: penpal_runtime::DmpQueue,
			LocationToAccountId: penpal_runtime::xcm_config::LocationToAccountId,
			System: penpal_runtime::System,
			Balances: penpal_runtime::Balances,
			ParachainSystem: penpal_runtime::ParachainSystem,
			ParachainInfo: penpal_runtime::ParachainInfo,
		},
		pallets_extra = {
			PolkadotXcm: penpal_runtime::PolkadotXcm,
			Assets: penpal_runtime::Assets,
		}
	},
	// Kusama Parachains
	pub struct AssetHubKusama {
		genesis = asset_hub_kusama::genesis(),
		on_init = (),
		runtime = {
			Runtime: asset_hub_kusama_runtime::Runtime,
			RuntimeOrigin: asset_hub_kusama_runtime::RuntimeOrigin,
			RuntimeCall: asset_hub_kusama_runtime::RuntimeCall,
			RuntimeEvent: asset_hub_kusama_runtime::RuntimeEvent,
			XcmpMessageHandler: asset_hub_kusama_runtime::XcmpQueue,
			DmpMessageHandler: asset_hub_kusama_runtime::DmpQueue,
			LocationToAccountId: asset_hub_kusama_runtime::xcm_config::LocationToAccountId,
			System: asset_hub_kusama_runtime::System,
			Balances: asset_hub_kusama_runtime::Balances,
			ParachainSystem: asset_hub_kusama_runtime::ParachainSystem,
			ParachainInfo: asset_hub_kusama_runtime::ParachainInfo,
		},
		pallets_extra = {
			PolkadotXcm: asset_hub_kusama_runtime::PolkadotXcm,
			Assets: asset_hub_kusama_runtime::Assets,
			ForeignAssets: asset_hub_kusama_runtime::Assets,
		}
	},
	pub struct BridgeHubKusama {
		genesis = bridge_hub_kusama::genesis(),
		on_init = (),
		runtime = {
			Runtime: bridge_hub_kusama_runtime::Runtime,
			RuntimeOrigin: bridge_hub_kusama_runtime::RuntimeOrigin,
			RuntimeCall: bridge_hub_kusama_runtime::RuntimeCall,
			RuntimeEvent: bridge_hub_kusama_runtime::RuntimeEvent,
			XcmpMessageHandler: bridge_hub_kusama_runtime::XcmpQueue,
			DmpMessageHandler: bridge_hub_kusama_runtime::DmpQueue,
			LocationToAccountId: bridge_hub_kusama_runtime::xcm_config::LocationToAccountId,
			System: bridge_hub_kusama_runtime::System,
			Balances: bridge_hub_kusama_runtime::Balances,
			ParachainSystem: bridge_hub_kusama_runtime::ParachainSystem,
			ParachainInfo: bridge_hub_kusama_runtime::ParachainInfo,
		},
		pallets_extra = {
			PolkadotXcm: bridge_hub_kusama_runtime::PolkadotXcm,
		}
	},
	pub struct PenpalKusama {
		genesis = penpal::genesis(penpal::PARA_ID),
		on_init = (),
		runtime = {
			Runtime: penpal_runtime::Runtime,
			RuntimeOrigin: penpal_runtime::RuntimeOrigin,
			RuntimeCall: penpal_runtime::RuntimeCall,
			RuntimeEvent: penpal_runtime::RuntimeEvent,
			XcmpMessageHandler: penpal_runtime::XcmpQueue,
			DmpMessageHandler: penpal_runtime::DmpQueue,
			LocationToAccountId: penpal_runtime::xcm_config::LocationToAccountId,
			System: penpal_runtime::System,
			Balances: penpal_runtime::Balances,
			ParachainSystem: penpal_runtime::ParachainSystem,
			ParachainInfo: penpal_runtime::ParachainInfo,
		},
		pallets_extra = {
			PolkadotXcm: penpal_runtime::PolkadotXcm,
			Assets: penpal_runtime::Assets,
		}
	},
	// Westend Parachains
	pub struct AssetHubWestend {
		genesis = asset_hub_westend::genesis(),
		on_init = (),
		runtime = {
			Runtime: asset_hub_westend_runtime::Runtime,
			RuntimeOrigin: asset_hub_westend_runtime::RuntimeOrigin,
			RuntimeCall: asset_hub_westend_runtime::RuntimeCall,
			RuntimeEvent: asset_hub_westend_runtime::RuntimeEvent,
			XcmpMessageHandler: asset_hub_westend_runtime::XcmpQueue,
			DmpMessageHandler: asset_hub_westend_runtime::DmpQueue,
			LocationToAccountId: asset_hub_westend_runtime::xcm_config::LocationToAccountId,
			System: asset_hub_westend_runtime::System,
			Balances: asset_hub_westend_runtime::Balances,
			ParachainSystem: asset_hub_westend_runtime::ParachainSystem,
			ParachainInfo: asset_hub_westend_runtime::ParachainInfo,
		},
		pallets_extra = {
			PolkadotXcm: asset_hub_westend_runtime::PolkadotXcm,
			Assets: asset_hub_westend_runtime::Assets,
			ForeignAssets: asset_hub_westend_runtime::ForeignAssets,
			PoolAssets: asset_hub_westend_runtime::PoolAssets,
			AssetConversion: asset_hub_westend_runtime::AssetConversion,
		}
	},
	pub struct PenpalWestend {
		genesis = penpal::genesis(penpal::PARA_ID),
		on_init = (),
		runtime = {
			Runtime: penpal_runtime::Runtime,
			RuntimeOrigin: penpal_runtime::RuntimeOrigin,
			RuntimeCall: penpal_runtime::RuntimeCall,
			RuntimeEvent: penpal_runtime::RuntimeEvent,
			XcmpMessageHandler: penpal_runtime::XcmpQueue,
			DmpMessageHandler: penpal_runtime::DmpQueue,
			LocationToAccountId: penpal_runtime::xcm_config::LocationToAccountId,
			System: penpal_runtime::System,
			Balances: penpal_runtime::Balances,
			ParachainSystem: penpal_runtime::ParachainSystem,
			ParachainInfo: penpal_runtime::ParachainInfo,
		},
		pallets_extra = {
			PolkadotXcm: penpal_runtime::PolkadotXcm,
			Assets: penpal_runtime::Assets,
		}
	},
	// Rococo Parachains
	pub struct BridgeHubRococo {
		genesis = bridge_hub_rococo::genesis(),
		on_init = (),
		runtime = {
			Runtime: bridge_hub_rococo_runtime::Runtime,
			RuntimeOrigin: bridge_hub_rococo_runtime::RuntimeOrigin,
			RuntimeCall: bridge_hub_rococo_runtime::RuntimeCall,
			RuntimeEvent: bridge_hub_rococo_runtime::RuntimeEvent,
			XcmpMessageHandler: bridge_hub_rococo_runtime::XcmpQueue,
			DmpMessageHandler: bridge_hub_rococo_runtime::DmpQueue,
			LocationToAccountId: bridge_hub_rococo_runtime::xcm_config::LocationToAccountId,
			System: bridge_hub_rococo_runtime::System,
			Balances: bridge_hub_rococo_runtime::Balances,
			ParachainSystem: bridge_hub_rococo_runtime::ParachainSystem,
			ParachainInfo: bridge_hub_rococo_runtime::ParachainInfo,
		},
		pallets_extra = {
			PolkadotXcm: bridge_hub_rococo_runtime::PolkadotXcm,
		}
	},
	pub struct AssetHubRococo {
		genesis = asset_hub_polkadot::genesis(),
		on_init = (),
		runtime = {
			Runtime: asset_hub_polkadot_runtime::Runtime,
			RuntimeOrigin: asset_hub_polkadot_runtime::RuntimeOrigin,
			RuntimeCall: asset_hub_polkadot_runtime::RuntimeCall,
			RuntimeEvent: asset_hub_polkadot_runtime::RuntimeEvent,
			XcmpMessageHandler: asset_hub_polkadot_runtime::XcmpQueue,
			DmpMessageHandler: asset_hub_polkadot_runtime::DmpQueue,
			LocationToAccountId: asset_hub_polkadot_runtime::xcm_config::LocationToAccountId,
			System: asset_hub_polkadot_runtime::System,
			Balances: asset_hub_polkadot_runtime::Balances,
			ParachainSystem: asset_hub_polkadot_runtime::ParachainSystem,
			ParachainInfo: asset_hub_polkadot_runtime::ParachainInfo,
		},
		pallets_extra = {
			PolkadotXcm: asset_hub_polkadot_runtime::PolkadotXcm,
			Assets: asset_hub_polkadot_runtime::Assets,
		}
	},
	// Wococo Parachains
	pub struct BridgeHubWococo {
		genesis = bridge_hub_rococo::genesis(),
		on_init = (),
		runtime = {
			Runtime: bridge_hub_rococo_runtime::Runtime,
			RuntimeOrigin: bridge_hub_rococo_runtime::RuntimeOrigin,
			RuntimeCall: bridge_hub_rococo_runtime::RuntimeCall,
			RuntimeEvent: bridge_hub_rococo_runtime::RuntimeEvent,
			XcmpMessageHandler: bridge_hub_rococo_runtime::XcmpQueue,
			DmpMessageHandler: bridge_hub_rococo_runtime::DmpQueue,
			LocationToAccountId: bridge_hub_rococo_runtime::xcm_config::LocationToAccountId,
			System: bridge_hub_rococo_runtime::System,
			Balances: bridge_hub_rococo_runtime::Balances,
			ParachainSystem: bridge_hub_rococo_runtime::ParachainSystem,
			ParachainInfo: bridge_hub_rococo_runtime::ParachainInfo,
		},
		pallets_extra = {
			PolkadotXcm: bridge_hub_rococo_runtime::PolkadotXcm,
		}
	},
	pub struct AssetHubWococo {
		genesis = asset_hub_polkadot::genesis(),
		on_init = (),
		runtime = {
			Runtime: asset_hub_polkadot_runtime::Runtime,
			RuntimeOrigin: asset_hub_polkadot_runtime::RuntimeOrigin,
			RuntimeCall: asset_hub_polkadot_runtime::RuntimeCall,
			RuntimeEvent: asset_hub_polkadot_runtime::RuntimeEvent,
			XcmpMessageHandler: asset_hub_polkadot_runtime::XcmpQueue,
			DmpMessageHandler: asset_hub_polkadot_runtime::DmpQueue,
			LocationToAccountId: asset_hub_polkadot_runtime::xcm_config::LocationToAccountId,
			System: asset_hub_polkadot_runtime::System,
			Balances: asset_hub_polkadot_runtime::Balances,
			ParachainSystem: asset_hub_polkadot_runtime::ParachainSystem,
			ParachainInfo: asset_hub_polkadot_runtime::ParachainInfo,
		},
		pallets_extra = {
			PolkadotXcm: asset_hub_polkadot_runtime::PolkadotXcm,
			Assets: asset_hub_polkadot_runtime::Assets,
		}
	}
}

decl_test_networks! {
	pub struct PolkadotMockNet {
		relay_chain = Polkadot,
		parachains = vec![
			AssetHubPolkadot,
			PenpalPolkadot,
			Collectives,
			BridgeHubPolkadot,
		],
		// TODO: uncomment when https://github.com/paritytech/cumulus/pull/2528 is merged
		// bridge = PolkadotKusamaMockBridge
		bridge = ()
	},
	pub struct KusamaMockNet {
		relay_chain = Kusama,
		parachains = vec![
			AssetHubKusama,
			PenpalKusama,
			BridgeHubKusama,
		],
		// TODO: uncomment when https://github.com/paritytech/cumulus/pull/2528 is merged
		// bridge = KusamaPolkadotMockBridge
		bridge = ()
	},
	pub struct WestendMockNet {
		relay_chain = Westend,
		parachains = vec![
			AssetHubWestend,
			PenpalWestend,
		],
		bridge = ()
	},
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
	PenpalPolkadot { sender: ALICE, receiver: BOB },
	PenpalKusama { sender: ALICE, receiver: BOB },
	PenpalWestend { sender: ALICE, receiver: BOB }
}
