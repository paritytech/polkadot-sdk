// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

use crate::bridges::{
	kusama_polkadot::{
		kusama_headers_to_bridge_hub_polkadot::KusamaToBridgeHubPolkadotCliBridge,
		polkadot_headers_to_bridge_hub_kusama::PolkadotToBridgeHubKusamaCliBridge,
	},
	polkadot_bulletin::{
		polkadot_bulletin_headers_to_bridge_hub_polkadot::PolkadotBulletinToBridgeHubPolkadotCliBridge,
		polkadot_headers_to_polkadot_bulletin::PolkadotToPolkadotBulletinCliBridge,
	},
	rococo_bulletin::{
		rococo_bulletin_headers_to_bridge_hub_rococo::RococoBulletinToBridgeHubRococoCliBridge,
		rococo_headers_to_rococo_bulletin::RococoToRococoBulletinCliBridge,
	},
	rococo_westend::{
		rococo_headers_to_bridge_hub_westend::RococoToBridgeHubWestendCliBridge,
		westend_headers_to_bridge_hub_rococo::WestendToBridgeHubRococoCliBridge,
	},
};
use relay_substrate_client::Chain;
use structopt::StructOpt;
use strum::{EnumString, VariantNames};
use substrate_relay_helper::{
	cli::init_bridge::{BridgeInitializer, InitBridgeParams},
	finality_base::engine::{Engine, Grandpa as GrandpaFinalityEngine},
};

impl BridgeInitializer for RococoToBridgeHubWestendCliBridge {
	type Engine = GrandpaFinalityEngine<Self::Source>;

	fn encode_init_bridge(
		init_data: <Self::Engine as Engine<Self::Source>>::InitializationData,
	) -> <Self::Target as Chain>::Call {
		relay_bridge_hub_westend_client::RuntimeCall::BridgeRococoGrandpa(
			relay_bridge_hub_westend_client::BridgeGrandpaCall::initialize { init_data },
		)
	}
}

impl BridgeInitializer for WestendToBridgeHubRococoCliBridge {
	type Engine = GrandpaFinalityEngine<Self::Source>;

	fn encode_init_bridge(
		init_data: <Self::Engine as Engine<Self::Source>>::InitializationData,
	) -> <Self::Target as Chain>::Call {
		relay_bridge_hub_rococo_client::RuntimeCall::BridgeWestendGrandpa(
			relay_bridge_hub_rococo_client::BridgeGrandpaCall::initialize { init_data },
		)
	}
}

impl BridgeInitializer for KusamaToBridgeHubPolkadotCliBridge {
	type Engine = GrandpaFinalityEngine<Self::Source>;

	fn encode_init_bridge(
		init_data: <Self::Engine as Engine<Self::Source>>::InitializationData,
	) -> <Self::Target as Chain>::Call {
		relay_bridge_hub_polkadot_client::RuntimeCall::BridgeKusamaGrandpa(
			relay_bridge_hub_polkadot_client::BridgeKusamaGrandpaCall::initialize { init_data },
		)
	}
}

impl BridgeInitializer for PolkadotToBridgeHubKusamaCliBridge {
	type Engine = GrandpaFinalityEngine<Self::Source>;

	fn encode_init_bridge(
		init_data: <Self::Engine as Engine<Self::Source>>::InitializationData,
	) -> <Self::Target as Chain>::Call {
		relay_bridge_hub_kusama_client::RuntimeCall::BridgePolkadotGrandpa(
			relay_bridge_hub_kusama_client::BridgeGrandpaCall::initialize { init_data },
		)
	}
}

impl BridgeInitializer for PolkadotToPolkadotBulletinCliBridge {
	type Engine = GrandpaFinalityEngine<Self::Source>;

	fn encode_init_bridge(
		init_data: <Self::Engine as Engine<Self::Source>>::InitializationData,
	) -> <Self::Target as Chain>::Call {
		type RuntimeCall = relay_polkadot_bulletin_client::RuntimeCall;
		type BridgePolkadotGrandpaCall = relay_polkadot_bulletin_client::BridgePolkadotGrandpaCall;
		type SudoCall = relay_polkadot_bulletin_client::SudoCall;

		let initialize_call =
			RuntimeCall::BridgePolkadotGrandpa(BridgePolkadotGrandpaCall::initialize { init_data });

		RuntimeCall::Sudo(SudoCall::sudo { call: Box::new(initialize_call) })
	}
}

impl BridgeInitializer for PolkadotBulletinToBridgeHubPolkadotCliBridge {
	type Engine = GrandpaFinalityEngine<Self::Source>;

	fn encode_init_bridge(
		init_data: <Self::Engine as Engine<Self::Source>>::InitializationData,
	) -> <Self::Target as Chain>::Call {
		// TODO: https://github.com/paritytech/parity-bridges-common/issues/2547 - use BridgePolkadotBulletinGrandpa
		relay_bridge_hub_polkadot_client::RuntimeCall::BridgeKusamaGrandpa(
			relay_bridge_hub_polkadot_client::BridgePolkadotBulletinGrandpaCall::initialize {
				init_data,
			},
		)
	}
}

impl BridgeInitializer for RococoToRococoBulletinCliBridge {
	type Engine = GrandpaFinalityEngine<Self::Source>;

	fn encode_init_bridge(
		init_data: <Self::Engine as Engine<Self::Source>>::InitializationData,
	) -> <Self::Target as Chain>::Call {
		type RuntimeCall = relay_polkadot_bulletin_client::RuntimeCall;
		type BridgePolkadotGrandpaCall = relay_polkadot_bulletin_client::BridgePolkadotGrandpaCall;
		type SudoCall = relay_polkadot_bulletin_client::SudoCall;

		let initialize_call =
			RuntimeCall::BridgePolkadotGrandpa(BridgePolkadotGrandpaCall::initialize { init_data });

		RuntimeCall::Sudo(SudoCall::sudo { call: Box::new(initialize_call) })
	}
}

impl BridgeInitializer for RococoBulletinToBridgeHubRococoCliBridge {
	type Engine = GrandpaFinalityEngine<Self::Source>;

	fn encode_init_bridge(
		init_data: <Self::Engine as Engine<Self::Source>>::InitializationData,
	) -> <Self::Target as Chain>::Call {
		relay_bridge_hub_rococo_client::RuntimeCall::BridgePolkadotBulletinGrandpa(
			relay_bridge_hub_rococo_client::BridgeBulletinGrandpaCall::initialize { init_data },
		)
	}
}

/// Initialize bridge pallet.
#[derive(StructOpt)]
pub struct InitBridge {
	/// A bridge instance to initialize.
	#[structopt(possible_values = InitBridgeName::VARIANTS, case_insensitive = true)]
	bridge: InitBridgeName,
	#[structopt(flatten)]
	params: InitBridgeParams,
}

#[derive(Debug, EnumString, VariantNames)]
#[strum(serialize_all = "kebab_case")]
/// Bridge to initialize.
pub enum InitBridgeName {
	KusamaToBridgeHubPolkadot,
	PolkadotToBridgeHubKusama,
	PolkadotToPolkadotBulletin,
	PolkadotBulletinToBridgeHubPolkadot,
	RococoToRococoBulletin,
	RococoBulletinToBridgeHubRococo,
	RococoToBridgeHubWestend,
	WestendToBridgeHubRococo,
}

impl InitBridge {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		match self.bridge {
			InitBridgeName::KusamaToBridgeHubPolkadot =>
				KusamaToBridgeHubPolkadotCliBridge::init_bridge(self.params),
			InitBridgeName::PolkadotToBridgeHubKusama =>
				PolkadotToBridgeHubKusamaCliBridge::init_bridge(self.params),
			InitBridgeName::PolkadotToPolkadotBulletin =>
				PolkadotToPolkadotBulletinCliBridge::init_bridge(self.params),
			InitBridgeName::PolkadotBulletinToBridgeHubPolkadot =>
				PolkadotBulletinToBridgeHubPolkadotCliBridge::init_bridge(self.params),
			InitBridgeName::RococoToRococoBulletin =>
				RococoToRococoBulletinCliBridge::init_bridge(self.params),
			InitBridgeName::RococoBulletinToBridgeHubRococo =>
				RococoBulletinToBridgeHubRococoCliBridge::init_bridge(self.params),
			InitBridgeName::RococoToBridgeHubWestend =>
				RococoToBridgeHubWestendCliBridge::init_bridge(self.params),
			InitBridgeName::WestendToBridgeHubRococo =>
				WestendToBridgeHubRococoCliBridge::init_bridge(self.params),
		}
		.await
	}
}
