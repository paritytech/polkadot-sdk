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
use strum::{EnumString, EnumVariantNames, VariantNames};

use crate::chains::{
	bridge_hub_rococo_messages_to_bridge_hub_wococo::BridgeHubRococoToBridgeHubWococoMessagesCliBridge,
	bridge_hub_wococo_messages_to_bridge_hub_rococo::BridgeHubWococoToBridgeHubRococoMessagesCliBridge,
	millau_headers_to_rialto::MillauToRialtoCliBridge,
	millau_headers_to_rialto_parachain::MillauToRialtoParachainCliBridge,
	rialto_headers_to_millau::RialtoToMillauCliBridge,
	rialto_parachains_to_millau::RialtoParachainToMillauCliBridge,
};
use messages_relay::relay_strategy::MixStrategy;
use relay_substrate_client::{AccountIdOf, AccountKeyPairOf, BalanceOf, ChainWithTransactions};
use substrate_relay_helper::{messages_lane::MessagesRelayParams, TransactionParams};

use crate::cli::{bridge::*, chain_schema::*, CliChain, HexLaneId, PrometheusParams};

/// Relayer operating mode.
#[derive(Debug, EnumString, EnumVariantNames, Clone, Copy, PartialEq, Eq)]
#[strum(serialize_all = "kebab_case")]
pub enum RelayerMode {
	/// The relayer doesn't care about rewards.
	Altruistic,
	/// The relayer will deliver all messages and confirmations as long as he's not losing any
	/// funds.
	Rational,
}

impl From<RelayerMode> for messages_relay::message_lane_loop::RelayerMode {
	fn from(mode: RelayerMode) -> Self {
		match mode {
			RelayerMode::Altruistic => Self::Altruistic,
			RelayerMode::Rational => Self::Rational,
		}
	}
}

/// Start messages relayer process.
#[derive(StructOpt)]
pub struct RelayMessages {
	/// A bridge instance to relay messages for.
	#[structopt(possible_values = FullBridge::VARIANTS, case_insensitive = true)]
	bridge: FullBridge,
	/// Hex-encoded lane id that should be served by the relay. Defaults to `00000000`.
	#[structopt(long, default_value = "00000000")]
	lane: HexLaneId,
	#[structopt(long, possible_values = RelayerMode::VARIANTS, case_insensitive = true, default_value = "rational")]
	relayer_mode: RelayerMode,
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
	Self::Source: ChainWithTransactions + CliChain<KeyPair = AccountKeyPairOf<Self::Source>>,
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
		let relayer_mode = data.relayer_mode.into();
		let relay_strategy = MixStrategy::new(relayer_mode);

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
			metrics_params: data.prometheus_params.into(),
			standalone_metrics: None,
			relay_strategy,
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
		}
		.await
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn should_use_rational_relayer_mode_by_default() {
		assert_eq!(
			RelayMessages::from_iter(vec![
				"relay-messages",
				"rialto-to-millau",
				"--source-port=0",
				"--source-signer=//Alice",
				"--target-port=0",
				"--target-signer=//Alice",
				"--lane=00000000",
			])
			.relayer_mode,
			RelayerMode::Rational,
		);
	}

	#[test]
	fn should_accept_altruistic_relayer_mode() {
		assert_eq!(
			RelayMessages::from_iter(vec![
				"relay-messages",
				"rialto-to-millau",
				"--source-port=0",
				"--source-signer=//Alice",
				"--target-port=0",
				"--target-signer=//Alice",
				"--lane=00000000",
				"--relayer-mode=altruistic",
			])
			.relayer_mode,
			RelayerMode::Altruistic,
		);
	}
}
