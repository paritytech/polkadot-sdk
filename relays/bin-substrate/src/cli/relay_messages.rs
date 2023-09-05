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

use async_trait::async_trait;
use sp_core::Pair;
use structopt::StructOpt;
use strum::VariantNames;

use crate::bridges::{
	kusama_polkadot::{
		bridge_hub_kusama_messages_to_bridge_hub_polkadot::BridgeHubKusamaToBridgeHubPolkadotMessagesCliBridge,
		bridge_hub_polkadot_messages_to_bridge_hub_kusama::BridgeHubPolkadotToBridgeHubKusamaMessagesCliBridge,
	},
	rialto_millau::{
		millau_headers_to_rialto::MillauToRialtoCliBridge,
		rialto_headers_to_millau::RialtoToMillauCliBridge,
	},
	rialto_parachain_millau::{
		millau_headers_to_rialto_parachain::MillauToRialtoParachainCliBridge,
		rialto_parachains_to_millau::RialtoParachainToMillauCliBridge,
	},
	rococo_wococo::{
		bridge_hub_rococo_messages_to_bridge_hub_wococo::BridgeHubRococoToBridgeHubWococoMessagesCliBridge,
		bridge_hub_wococo_messages_to_bridge_hub_rococo::BridgeHubWococoToBridgeHubRococoMessagesCliBridge,
	},
};
use relay_substrate_client::{AccountIdOf, AccountKeyPairOf, BalanceOf, ChainWithTransactions};
use substrate_relay_helper::{messages_lane::MessagesRelayParams, TransactionParams};

use crate::cli::{bridge::*, chain_schema::*, CliChain, HexLaneId, PrometheusParams};

/// Start messages relayer process.
#[derive(StructOpt)]
pub struct RelayMessages {
	/// A bridge instance to relay messages for.
	#[structopt(possible_values = FullBridge::VARIANTS, case_insensitive = true)]
	bridge: FullBridge,
	/// Hex-encoded lane id that should be served by the relay. Defaults to `00000000`.
	#[structopt(long, default_value = "00000000")]
	lane: HexLaneId,
	#[structopt(flatten)]
	source: SourceConnectionParams,
	#[structopt(flatten)]
	source_sign: SourceSigningParams,
	#[structopt(flatten)]
	target: TargetConnectionParams,
	#[structopt(flatten)]
	target_sign: TargetSigningParams,
	#[structopt(flatten)]
	prometheus_params: PrometheusParams,
}

#[async_trait]
trait MessagesRelayer: MessagesCliBridge
where
	Self::Source: ChainWithTransactions + CliChain,
	AccountIdOf<Self::Source>: From<<AccountKeyPairOf<Self::Source> as Pair>::Public>,
	AccountIdOf<Self::Target>: From<<AccountKeyPairOf<Self::Target> as Pair>::Public>,
	BalanceOf<Self::Source>: TryFrom<BalanceOf<Self::Target>>,
{
	async fn relay_messages(data: RelayMessages) -> anyhow::Result<()> {
		let source_client = data.source.into_client::<Self::Source>().await?;
		let source_sign = data.source_sign.to_keypair::<Self::Source>()?;
		let source_transactions_mortality = data.source_sign.transactions_mortality()?;
		let target_client = data.target.into_client::<Self::Target>().await?;
		let target_sign = data.target_sign.to_keypair::<Self::Target>()?;
		let target_transactions_mortality = data.target_sign.transactions_mortality()?;

		substrate_relay_helper::messages_lane::run::<Self::MessagesLane>(MessagesRelayParams {
			source_client,
			source_transaction_params: TransactionParams {
				signer: source_sign,
				mortality: source_transactions_mortality,
			},
			target_client,
			target_transaction_params: TransactionParams {
				signer: target_sign,
				mortality: target_transactions_mortality,
			},
			source_to_target_headers_relay: None,
			target_to_source_headers_relay: None,
			lane_id: data.lane.into(),
			metrics_params: data.prometheus_params.into_metrics_params()?,
		})
		.await
		.map_err(|e| anyhow::format_err!("{}", e))
	}
}

impl MessagesRelayer for MillauToRialtoCliBridge {}
impl MessagesRelayer for RialtoToMillauCliBridge {}
impl MessagesRelayer for MillauToRialtoParachainCliBridge {}
impl MessagesRelayer for RialtoParachainToMillauCliBridge {}
impl MessagesRelayer for BridgeHubRococoToBridgeHubWococoMessagesCliBridge {}
impl MessagesRelayer for BridgeHubWococoToBridgeHubRococoMessagesCliBridge {}
impl MessagesRelayer for BridgeHubKusamaToBridgeHubPolkadotMessagesCliBridge {}
impl MessagesRelayer for BridgeHubPolkadotToBridgeHubKusamaMessagesCliBridge {}

impl RelayMessages {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		match self.bridge {
			FullBridge::MillauToRialto => MillauToRialtoCliBridge::relay_messages(self),
			FullBridge::RialtoToMillau => RialtoToMillauCliBridge::relay_messages(self),
			FullBridge::MillauToRialtoParachain =>
				MillauToRialtoParachainCliBridge::relay_messages(self),
			FullBridge::RialtoParachainToMillau =>
				RialtoParachainToMillauCliBridge::relay_messages(self),
			FullBridge::BridgeHubRococoToBridgeHubWococo =>
				BridgeHubRococoToBridgeHubWococoMessagesCliBridge::relay_messages(self),
			FullBridge::BridgeHubWococoToBridgeHubRococo =>
				BridgeHubWococoToBridgeHubRococoMessagesCliBridge::relay_messages(self),
			FullBridge::BridgeHubKusamaToBridgeHubPolkadot =>
				BridgeHubKusamaToBridgeHubPolkadotMessagesCliBridge::relay_messages(self),
			FullBridge::BridgeHubPolkadotToBridgeHubKusama =>
				BridgeHubPolkadotToBridgeHubKusamaMessagesCliBridge::relay_messages(self),
		}
		.await
	}
}
