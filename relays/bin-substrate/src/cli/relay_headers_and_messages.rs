// Copyright 2019-2022 Parity Technologies (UK) Ltd.
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

//! Complex 2-ways headers+messages relays support.
//!
//! To add new complex relay between `ChainA` and `ChainB`, you must:
//!
//! 1) ensure that there's a `declare_chain_cli_schema!(...)` for both chains.
//! 2) add `declare_chain_to_chain_bridge_schema!(...)` or
//!    `declare_chain_to_parachain_bridge_schema` for the bridge.
//! 3) declare a new struct for the added bridge and implement the `Full2WayBridge` trait for it.

use async_trait::async_trait;
use structopt::StructOpt;

use crate::bridges::{
	kusama_polkadot::{
		kusama_parachains_to_bridge_hub_polkadot::BridgeHubKusamaToBridgeHubPolkadotCliBridge,
		polkadot_parachains_to_bridge_hub_kusama::BridgeHubPolkadotToBridgeHubKusamaCliBridge,
	},
	polkadot_bulletin::{
		polkadot_bulletin_headers_to_bridge_hub_polkadot::PolkadotBulletinToBridgeHubPolkadotCliBridge,
		polkadot_parachains_to_polkadot_bulletin::PolkadotToPolkadotBulletinCliBridge,
	},
	rococo_bulletin::{
		rococo_bulletin_headers_to_bridge_hub_rococo::RococoBulletinToBridgeHubRococoCliBridge,
		rococo_parachains_to_rococo_bulletin::RococoToRococoBulletinCliBridge,
		BridgeHubRococoAsBridgeHubPolkadot,
	},
	rococo_westend::{
		rococo_parachains_to_bridge_hub_westend::BridgeHubRococoToBridgeHubWestendCliBridge,
		westend_parachains_to_bridge_hub_rococo::BridgeHubWestendToBridgeHubRococoCliBridge,
	},
};
use relay_substrate_client::{
	AccountKeyPairOf, ChainRuntimeVersion, ChainWithRuntimeVersion, ChainWithTransactions,
	Parachain, SimpleRuntimeVersion,
};
use substrate_relay_helper::{
	cli::{
		bridge::{
			CliBridgeBase, MessagesCliBridge, ParachainToRelayHeadersCliBridge,
			RelayToRelayHeadersCliBridge,
		},
		chain_schema::*,
		relay_headers_and_messages::{
			parachain_to_parachain::ParachainToParachainBridge, relay_to_parachain::*,
			BridgeEndCommonParams, Full2WayBridge, Full2WayBridgeCommonParams,
			HeadersAndMessagesSharedParams,
		},
	},
	declare_chain_cli_schema, declare_parachain_to_parachain_bridge_schema,
	declare_relay_to_parachain_bridge_schema, TransactionParams,
};

// All supported chains.
declare_chain_cli_schema!(Rococo, rococo);
declare_chain_cli_schema!(BridgeHubRococo, bridge_hub_rococo);
declare_chain_cli_schema!(Westend, westend);
declare_chain_cli_schema!(BridgeHubWestend, bridge_hub_westend);
declare_chain_cli_schema!(Kusama, kusama);
declare_chain_cli_schema!(BridgeHubKusama, bridge_hub_kusama);
declare_chain_cli_schema!(Polkadot, polkadot);
declare_chain_cli_schema!(BridgeHubPolkadot, bridge_hub_polkadot);
declare_chain_cli_schema!(PolkadotBulletin, polkadot_bulletin);
declare_chain_cli_schema!(RococoBulletin, rococo_bulletin);
// Means to override signers of different layer transactions.
declare_chain_cli_schema!(RococoHeadersToBridgeHubWestend, rococo_headers_to_bridge_hub_westend);
declare_chain_cli_schema!(
	RococoParachainsToBridgeHubWestend,
	rococo_parachains_to_bridge_hub_westend
);
declare_chain_cli_schema!(WestendHeadersToBridgeHubRococo, westend_headers_to_bridge_hub_rococo);
declare_chain_cli_schema!(
	WestendParachainsToBridgeHubRococo,
	westend_parachains_to_bridge_hub_rococo
);
declare_chain_cli_schema!(KusamaHeadersToBridgeHubPolkadot, kusama_headers_to_bridge_hub_polkadot);
declare_chain_cli_schema!(
	KusamaParachainsToBridgeHubPolkadot,
	kusama_parachains_to_bridge_hub_polkadot
);
declare_chain_cli_schema!(PolkadotHeadersToBridgeHubKusama, polkadot_headers_to_bridge_hub_kusama);
declare_chain_cli_schema!(
	PolkadotParachainsToBridgeHubKusama,
	polkadot_parachains_to_bridge_hub_kusama
);
declare_chain_cli_schema!(
	PolkadotBulletinHeadersToBridgeHubPolkadot,
	polkadot_bulletin_headers_to_bridge_hub_polkadot
);
declare_chain_cli_schema!(
	RococoBulletinHeadersToBridgeHubRococo,
	rococo_bulletin_headers_to_bridge_hub_rococo
);
declare_chain_cli_schema!(PolkadotHeadersToPolkadotBulletin, polkadot_headers_to_polkadot_bulletin);
declare_chain_cli_schema!(RococoHeadersToRococoBulletin, rococo_headers_to_rococo_bulletin);
declare_chain_cli_schema!(
	PolkadotParachainsToPolkadotBulletin,
	polkadot_parachains_to_polkadot_bulletin
);
declare_chain_cli_schema!(RococoParachainsToRococoBulletin, rococo_parachains_to_rococo_bulletin);
// All supported bridges.
declare_parachain_to_parachain_bridge_schema!(BridgeHubRococo, Rococo, BridgeHubWestend, Westend);
declare_parachain_to_parachain_bridge_schema!(BridgeHubKusama, Kusama, BridgeHubPolkadot, Polkadot);
declare_relay_to_parachain_bridge_schema!(PolkadotBulletin, BridgeHubPolkadot, Polkadot);
declare_relay_to_parachain_bridge_schema!(RococoBulletin, BridgeHubRococo, Rococo);

/// BridgeHubRococo <> BridgeHubWestend complex relay.
pub struct BridgeHubRococoBridgeHubWestendFull2WayBridge {
	base: <Self as Full2WayBridge>::Base,
}

#[async_trait]
impl Full2WayBridge for BridgeHubRococoBridgeHubWestendFull2WayBridge {
	type Base = ParachainToParachainBridge<Self::L2R, Self::R2L>;
	type Left = relay_bridge_hub_rococo_client::BridgeHubRococo;
	type Right = relay_bridge_hub_westend_client::BridgeHubWestend;
	type L2R = BridgeHubRococoToBridgeHubWestendCliBridge;
	type R2L = BridgeHubWestendToBridgeHubRococoCliBridge;

	fn new(base: Self::Base) -> anyhow::Result<Self> {
		Ok(Self { base })
	}

	fn base(&self) -> &Self::Base {
		&self.base
	}

	fn mut_base(&mut self) -> &mut Self::Base {
		&mut self.base
	}
}

/// BridgeHubKusama <> BridgeHubPolkadot complex relay.
pub struct BridgeHubKusamaBridgeHubPolkadotFull2WayBridge {
	base: <Self as Full2WayBridge>::Base,
}

#[async_trait]
impl Full2WayBridge for BridgeHubKusamaBridgeHubPolkadotFull2WayBridge {
	type Base = ParachainToParachainBridge<Self::L2R, Self::R2L>;
	type Left = relay_bridge_hub_kusama_client::BridgeHubKusama;
	type Right = relay_bridge_hub_polkadot_client::BridgeHubPolkadot;
	type L2R = BridgeHubKusamaToBridgeHubPolkadotCliBridge;
	type R2L = BridgeHubPolkadotToBridgeHubKusamaCliBridge;

	fn new(base: Self::Base) -> anyhow::Result<Self> {
		Ok(Self { base })
	}

	fn base(&self) -> &Self::Base {
		&self.base
	}

	fn mut_base(&mut self) -> &mut Self::Base {
		&mut self.base
	}
}

/// `PolkadotBulletin` <> `BridgeHubPolkadot` complex relay.
pub struct PolkadotBulletinBridgeHubPolkadotFull2WayBridge {
	base: <Self as Full2WayBridge>::Base,
}

#[async_trait]
impl Full2WayBridge for PolkadotBulletinBridgeHubPolkadotFull2WayBridge {
	type Base = RelayToParachainBridge<Self::L2R, Self::R2L>;
	type Left = relay_polkadot_bulletin_client::PolkadotBulletin;
	type Right = relay_bridge_hub_polkadot_client::BridgeHubPolkadot;
	type L2R = PolkadotBulletinToBridgeHubPolkadotCliBridge;
	type R2L = PolkadotToPolkadotBulletinCliBridge;

	fn new(base: Self::Base) -> anyhow::Result<Self> {
		Ok(Self { base })
	}

	fn base(&self) -> &Self::Base {
		&self.base
	}

	fn mut_base(&mut self) -> &mut Self::Base {
		&mut self.base
	}
}

/// `RococoBulletin` <> `BridgeHubRococo` complex relay.
pub struct RococoBulletinBridgeHubRococoFull2WayBridge {
	base: <Self as Full2WayBridge>::Base,
}

#[async_trait]
impl Full2WayBridge for RococoBulletinBridgeHubRococoFull2WayBridge {
	type Base = RelayToParachainBridge<Self::L2R, Self::R2L>;
	type Left = relay_polkadot_bulletin_client::PolkadotBulletin;
	type Right = BridgeHubRococoAsBridgeHubPolkadot;
	type L2R = RococoBulletinToBridgeHubRococoCliBridge;
	type R2L = RococoToRococoBulletinCliBridge;

	fn new(base: Self::Base) -> anyhow::Result<Self> {
		Ok(Self { base })
	}

	fn base(&self) -> &Self::Base {
		&self.base
	}

	fn mut_base(&mut self) -> &mut Self::Base {
		&mut self.base
	}
}

/// Complex headers+messages relay.
#[derive(Debug, PartialEq, StructOpt)]
pub enum RelayHeadersAndMessages {
	/// BridgeHubKusama <> BridgeHubPolkadot relay.
	BridgeHubKusamaBridgeHubPolkadot(BridgeHubKusamaBridgeHubPolkadotHeadersAndMessages),
	/// `PolkadotBulletin` <> `BridgeHubPolkadot` relay.
	PolkadotBulletinBridgeHubPolkadot(PolkadotBulletinBridgeHubPolkadotHeadersAndMessages),
	/// `RococoBulletin` <> `BridgeHubRococo` relay.
	RococoBulletinBridgeHubRococo(RococoBulletinBridgeHubRococoHeadersAndMessages),
	/// BridgeHubRococo <> BridgeHubWestend relay.
	BridgeHubRococoBridgeHubWestend(BridgeHubRococoBridgeHubWestendHeadersAndMessages),
}

impl RelayHeadersAndMessages {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		match self {
			RelayHeadersAndMessages::BridgeHubRococoBridgeHubWestend(params) =>
				BridgeHubRococoBridgeHubWestendFull2WayBridge::new(params.into_bridge().await?)?
					.run()
					.await,
			RelayHeadersAndMessages::BridgeHubKusamaBridgeHubPolkadot(params) =>
				BridgeHubKusamaBridgeHubPolkadotFull2WayBridge::new(params.into_bridge().await?)?
					.run()
					.await,
			RelayHeadersAndMessages::PolkadotBulletinBridgeHubPolkadot(params) =>
				PolkadotBulletinBridgeHubPolkadotFull2WayBridge::new(params.into_bridge().await?)?
					.run()
					.await,
			RelayHeadersAndMessages::RococoBulletinBridgeHubRococo(params) =>
				RococoBulletinBridgeHubRococoFull2WayBridge::new(params.into_bridge().await?)?
					.run()
					.await,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use substrate_relay_helper::cli::{HexLaneId, PrometheusParams};

	#[test]
	fn should_parse_parachain_to_parachain_options() {
		// when
		let res = RelayHeadersAndMessages::from_iter(vec![
			"relay-headers-and-messages",
			"bridge-hub-kusama-bridge-hub-polkadot",
			"--bridge-hub-kusama-host",
			"bridge-hub-kusama-node-collator1",
			"--bridge-hub-kusama-port",
			"9944",
			"--bridge-hub-kusama-signer",
			"//Iden",
			"--bridge-hub-kusama-transactions-mortality",
			"64",
			"--kusama-host",
			"kusama-alice",
			"--kusama-port",
			"9944",
			"--bridge-hub-polkadot-host",
			"bridge-hub-polkadot-collator1",
			"--bridge-hub-polkadot-port",
			"9944",
			"--bridge-hub-polkadot-signer",
			"//George",
			"--bridge-hub-polkadot-transactions-mortality",
			"64",
			"--polkadot-host",
			"polkadot-alice",
			"--polkadot-port",
			"9944",
			"--lane",
			"00000000",
			"--prometheus-host",
			"0.0.0.0",
		]);

		// then
		assert_eq!(
			res,
			RelayHeadersAndMessages::BridgeHubKusamaBridgeHubPolkadot(
				BridgeHubKusamaBridgeHubPolkadotHeadersAndMessages {
					shared: HeadersAndMessagesSharedParams {
						lane: vec![HexLaneId([0x00, 0x00, 0x00, 0x00])],
						only_mandatory_headers: false,
						prometheus_params: PrometheusParams {
							no_prometheus: false,
							prometheus_host: "0.0.0.0".into(),
							prometheus_port: 9616,
						},
					},
					left_relay: KusamaConnectionParams {
						kusama_host: "kusama-alice".into(),
						kusama_port: 9944,
						kusama_secure: false,
						kusama_runtime_version: KusamaRuntimeVersionParams {
							kusama_version_mode: RuntimeVersionType::Bundle,
							kusama_spec_version: None,
							kusama_transaction_version: None,
						},
					},
					left: BridgeHubKusamaConnectionParams {
						bridge_hub_kusama_host: "bridge-hub-kusama-node-collator1".into(),
						bridge_hub_kusama_port: 9944,
						bridge_hub_kusama_secure: false,
						bridge_hub_kusama_runtime_version: BridgeHubKusamaRuntimeVersionParams {
							bridge_hub_kusama_version_mode: RuntimeVersionType::Bundle,
							bridge_hub_kusama_spec_version: None,
							bridge_hub_kusama_transaction_version: None,
						},
					},
					left_sign: BridgeHubKusamaSigningParams {
						bridge_hub_kusama_signer: Some("//Iden".into()),
						bridge_hub_kusama_signer_password: None,
						bridge_hub_kusama_signer_file: None,
						bridge_hub_kusama_signer_password_file: None,
						bridge_hub_kusama_transactions_mortality: Some(64),
					},
					right: BridgeHubPolkadotConnectionParams {
						bridge_hub_polkadot_host: "bridge-hub-polkadot-collator1".into(),
						bridge_hub_polkadot_port: 9944,
						bridge_hub_polkadot_secure: false,
						bridge_hub_polkadot_runtime_version:
							BridgeHubPolkadotRuntimeVersionParams {
								bridge_hub_polkadot_version_mode: RuntimeVersionType::Bundle,
								bridge_hub_polkadot_spec_version: None,
								bridge_hub_polkadot_transaction_version: None,
							},
					},
					right_sign: BridgeHubPolkadotSigningParams {
						bridge_hub_polkadot_signer: Some("//George".into()),
						bridge_hub_polkadot_signer_password: None,
						bridge_hub_polkadot_signer_file: None,
						bridge_hub_polkadot_signer_password_file: None,
						bridge_hub_polkadot_transactions_mortality: Some(64),
					},
					right_relay: PolkadotConnectionParams {
						polkadot_host: "polkadot-alice".into(),
						polkadot_port: 9944,
						polkadot_secure: false,
						polkadot_runtime_version: PolkadotRuntimeVersionParams {
							polkadot_version_mode: RuntimeVersionType::Bundle,
							polkadot_spec_version: None,
							polkadot_transaction_version: None,
						},
					},
				}
			),
		);
	}
}
