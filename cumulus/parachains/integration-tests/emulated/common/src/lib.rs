pub mod constants;

pub use constants::{
	accounts::{ALICE, BOB},
	asset_hub_kusama, asset_hub_polkadot, bridge_hub_kusama, bridge_hub_polkadot, collectives,
	kusama, penpal, polkadot,
};
use frame_support::{parameter_types, sp_io, sp_tracing};
pub use parachains_common::{AccountId, AssetHubPolkadotAuraId, AuraId, Balance, BlockNumber};
pub use sp_core::{sr25519, storage::Storage, Get};
use xcm::prelude::*;
use xcm_emulator::{
	decl_test_networks, decl_test_parachains, decl_test_relay_chains, Parachain, RelayChain,
	TestExt,
};
use xcm_executor::traits::Convert;

decl_test_relay_chains! {
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
	}
}

decl_test_parachains! {
	// Polkadot
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
	// Kusama
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
	pub struct BHKusama {
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
			ParachainInfo:bridge_hub_kusama_runtime::ParachainInfo,
		},
		pallets_extra = {
			PolkadotXcm: bridge_hub_kusama_runtime::PolkadotXcm,
		}
	},
	pub struct BHPolkadot {
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
			ParachainInfo:bridge_hub_polkadot_runtime::ParachainInfo,
		},
		pallets_extra = {
			PolkadotXcm: bridge_hub_polkadot_runtime::PolkadotXcm,
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
			BHPolkadot,
		],
	},
	pub struct KusamaMockNet {
		relay_chain = Kusama,
		parachains = vec![
			AssetHubKusama,
			PenpalKusama,
			BHKusama,
		],
	}
}

parameter_types! {
	// Polkadot
	pub PolkadotSender: AccountId = Polkadot::account_id_of(ALICE);
	pub PolkadotReceiver: AccountId = Polkadot::account_id_of(BOB);
	// Kusama
	pub KusamaSender: AccountId = Kusama::account_id_of(ALICE);
	pub KusamaReceiver: AccountId = Kusama::account_id_of(BOB);
	// Asset Hub Polkadot
	pub AssetHubPolkadotSender: AccountId = AssetHubPolkadot::account_id_of(ALICE);
	pub AssetHubPolkadotReceiver: AccountId = AssetHubPolkadot::account_id_of(BOB);
	// Asset Hub Kusama
	pub AssetHubKusamaSender: AccountId = AssetHubKusama::account_id_of(ALICE);
	pub AssetHubKusamaReceiver: AccountId = AssetHubKusama::account_id_of(BOB);
	// Penpal Polkadot
	pub PenpalPolkadotSender: AccountId = PenpalPolkadot::account_id_of(ALICE);
	pub PenpalPolkadotReceiver: AccountId = PenpalPolkadot::account_id_of(BOB);
	// Penpal Kusama
	pub PenpalKusamaSender: AccountId = PenpalKusama::account_id_of(ALICE);
	pub PenpalKusamaReceiver: AccountId = PenpalKusama::account_id_of(BOB);
	// Collectives
	pub CollectivesSender: AccountId = Collectives::account_id_of(ALICE);
	pub CollectivesReceiver: AccountId = Collectives::account_id_of(BOB);
	// Bridge Hub Polkadot
	pub BHPolkadotSender: AccountId = BHPolkadot::account_id_of(ALICE);
	pub BHPolkadotReceiver: AccountId = BHPolkadot::account_id_of(BOB);
	// Bridge Hub Kusama
	pub BHKusamaSender: AccountId = BHKusama::account_id_of(ALICE);
	pub BHKusamaReceiver: AccountId = BHKusama::account_id_of(BOB);
}
