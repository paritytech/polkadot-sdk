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

#[macro_use]
pub mod parachain_to_parachain;
#[macro_use]
pub mod relay_to_relay;
#[macro_use]
pub mod relay_to_parachain;

use async_trait::async_trait;
use std::{marker::PhantomData, sync::Arc};
use structopt::StructOpt;

use futures::{FutureExt, TryFutureExt};

use crate::{
	cli::{bridge::MessagesCliBridge, HexLaneId, PrometheusParams},
	messages_lane::{MessagesRelayLimits, MessagesRelayParams},
	on_demand::OnDemandRelay,
	TaggedAccount, TransactionParams,
};
use bp_messages::LaneId;
use bp_runtime::BalanceOf;
use relay_substrate_client::{
	AccountIdOf, AccountKeyPairOf, Chain, ChainWithBalances, ChainWithMessages,
	ChainWithRuntimeVersion, ChainWithTransactions, Client,
};
use relay_utils::metrics::MetricsParams;
use sp_core::Pair;

/// Parameters that have the same names across all bridges.
#[derive(Debug, PartialEq, StructOpt)]
pub struct HeadersAndMessagesSharedParams {
	/// Hex-encoded lane identifiers that should be served by the complex relay.
	#[structopt(long, default_value = "00000000")]
	pub lane: Vec<HexLaneId>,
	/// If passed, only mandatory headers (headers that are changing the GRANDPA authorities set)
	/// are relayed.
	#[structopt(long)]
	pub only_mandatory_headers: bool,
	#[structopt(flatten)]
	/// Prometheus metrics params.
	pub prometheus_params: PrometheusParams,
}

/// Bridge parameters, shared by all bridge types.
pub struct Full2WayBridgeCommonParams<
	Left: ChainWithTransactions + ChainWithRuntimeVersion,
	Right: ChainWithTransactions + ChainWithRuntimeVersion,
> {
	/// Shared parameters.
	pub shared: HeadersAndMessagesSharedParams,
	/// Parameters of the left chain.
	pub left: BridgeEndCommonParams<Left>,
	/// Parameters of the right chain.
	pub right: BridgeEndCommonParams<Right>,

	/// Common metric parameters.
	pub metrics_params: MetricsParams,
}

impl<
		Left: ChainWithTransactions + ChainWithRuntimeVersion,
		Right: ChainWithTransactions + ChainWithRuntimeVersion,
	> Full2WayBridgeCommonParams<Left, Right>
{
	/// Creates new bridge parameters from its components.
	pub fn new<L2R: MessagesCliBridge<Source = Left, Target = Right>>(
		shared: HeadersAndMessagesSharedParams,
		left: BridgeEndCommonParams<Left>,
		right: BridgeEndCommonParams<Right>,
	) -> anyhow::Result<Self> {
		// Create metrics registry.
		let metrics_params = shared.prometheus_params.clone().into_metrics_params()?;
		let metrics_params = relay_utils::relay_metrics(metrics_params).into_params();

		Ok(Self { shared, left, right, metrics_params })
	}
}

/// Parameters that are associated with one side of the bridge.
pub struct BridgeEndCommonParams<Chain: ChainWithTransactions + ChainWithRuntimeVersion> {
	/// Chain client.
	pub client: Client<Chain>,
	/// Params used for sending transactions to the chain.
	pub tx_params: TransactionParams<AccountKeyPairOf<Chain>>,
	/// Accounts, which balances are exposed as metrics by the relay process.
	pub accounts: Vec<TaggedAccount<AccountIdOf<Chain>>>,
}

/// All data of the bidirectional complex relay.
pub struct FullBridge<
	'a,
	Source: ChainWithTransactions + ChainWithRuntimeVersion,
	Target: ChainWithTransactions + ChainWithRuntimeVersion,
	Bridge: MessagesCliBridge<Source = Source, Target = Target>,
> {
	source: &'a mut BridgeEndCommonParams<Source>,
	target: &'a mut BridgeEndCommonParams<Target>,
	metrics_params: &'a MetricsParams,
	_phantom_data: PhantomData<Bridge>,
}

impl<
		'a,
		Source: ChainWithTransactions + ChainWithRuntimeVersion,
		Target: ChainWithTransactions + ChainWithRuntimeVersion,
		Bridge: MessagesCliBridge<Source = Source, Target = Target>,
	> FullBridge<'a, Source, Target, Bridge>
where
	AccountIdOf<Source>: From<<AccountKeyPairOf<Source> as Pair>::Public>,
	AccountIdOf<Target>: From<<AccountKeyPairOf<Target> as Pair>::Public>,
	BalanceOf<Source>: TryFrom<BalanceOf<Target>> + Into<u128>,
{
	/// Construct complex relay given it components.
	fn new(
		source: &'a mut BridgeEndCommonParams<Source>,
		target: &'a mut BridgeEndCommonParams<Target>,
		metrics_params: &'a MetricsParams,
	) -> Self {
		Self { source, target, metrics_params, _phantom_data: Default::default() }
	}

	/// Returns message relay parameters.
	fn messages_relay_params(
		&self,
		source_to_target_headers_relay: Arc<dyn OnDemandRelay<Source, Target>>,
		target_to_source_headers_relay: Arc<dyn OnDemandRelay<Target, Source>>,
		lane_id: LaneId,
		maybe_limits: Option<MessagesRelayLimits>,
	) -> MessagesRelayParams<Bridge::MessagesLane> {
		MessagesRelayParams {
			source_client: self.source.client.clone(),
			source_transaction_params: self.source.tx_params.clone(),
			target_client: self.target.client.clone(),
			target_transaction_params: self.target.tx_params.clone(),
			source_to_target_headers_relay: Some(source_to_target_headers_relay),
			target_to_source_headers_relay: Some(target_to_source_headers_relay),
			lane_id,
			limits: maybe_limits,
			metrics_params: self.metrics_params.clone().disable(),
		}
	}
}

/// Base portion of the bidirectional complex relay.
///
/// This main purpose of extracting this trait is that in different relays the implementation
/// of `start_on_demand_headers_relayers` method will be different. But the number of
/// implementations is limited to relay <> relay, parachain <> relay and parachain <> parachain.
/// This trait allows us to reuse these implementations in different bridges.
#[async_trait]
pub trait Full2WayBridgeBase: Sized + Send + Sync {
	/// The CLI params for the bridge.
	type Params;
	/// The left relay chain.
	type Left: ChainWithTransactions + ChainWithRuntimeVersion;
	/// The right destination chain (it can be a relay or a parachain).
	type Right: ChainWithTransactions + ChainWithRuntimeVersion;

	/// Reference to common relay parameters.
	fn common(&self) -> &Full2WayBridgeCommonParams<Self::Left, Self::Right>;

	/// Mutable reference to common relay parameters.
	fn mut_common(&mut self) -> &mut Full2WayBridgeCommonParams<Self::Left, Self::Right>;

	/// Start on-demand headers relays.
	async fn start_on_demand_headers_relayers(
		&mut self,
	) -> anyhow::Result<(
		Arc<dyn OnDemandRelay<Self::Left, Self::Right>>,
		Arc<dyn OnDemandRelay<Self::Right, Self::Left>>,
	)>;
}

/// Bidirectional complex relay.
#[async_trait]
pub trait Full2WayBridge: Sized + Sync
where
	AccountIdOf<Self::Left>: From<<AccountKeyPairOf<Self::Left> as Pair>::Public>,
	AccountIdOf<Self::Right>: From<<AccountKeyPairOf<Self::Right> as Pair>::Public>,
	BalanceOf<Self::Left>: TryFrom<BalanceOf<Self::Right>> + Into<u128>,
	BalanceOf<Self::Right>: TryFrom<BalanceOf<Self::Left>> + Into<u128>,
{
	/// Base portion of the bidirectional complex relay.
	type Base: Full2WayBridgeBase<Left = Self::Left, Right = Self::Right>;

	/// The left relay chain.
	type Left: ChainWithTransactions
		+ ChainWithBalances
		+ ChainWithMessages
		+ ChainWithRuntimeVersion;
	/// The right relay chain.
	type Right: ChainWithTransactions
		+ ChainWithBalances
		+ ChainWithMessages
		+ ChainWithRuntimeVersion;

	/// Left to Right bridge.
	type L2R: MessagesCliBridge<Source = Self::Left, Target = Self::Right>;
	/// Right to Left bridge
	type R2L: MessagesCliBridge<Source = Self::Right, Target = Self::Left>;

	/// Construct new bridge.
	fn new(params: <Self::Base as Full2WayBridgeBase>::Params) -> anyhow::Result<Self>;

	/// Reference to the base relay portion.
	fn base(&self) -> &Self::Base;

	/// Mutable reference to the base relay portion.
	fn mut_base(&mut self) -> &mut Self::Base;

	/// Creates and returns Left to Right complex relay.
	fn left_to_right(&mut self) -> FullBridge<Self::Left, Self::Right, Self::L2R> {
		let common = self.mut_base().mut_common();
		FullBridge::<_, _, Self::L2R>::new(
			&mut common.left,
			&mut common.right,
			&common.metrics_params,
		)
	}

	/// Creates and returns Right to Left complex relay.
	fn right_to_left(&mut self) -> FullBridge<Self::Right, Self::Left, Self::R2L> {
		let common = self.mut_base().mut_common();
		FullBridge::<_, _, Self::R2L>::new(
			&mut common.right,
			&mut common.left,
			&common.metrics_params,
		)
	}

	/// Start complex relay.
	async fn run(&mut self) -> anyhow::Result<()> {
		// Register standalone metrics.
		{
			let common = self.mut_base().mut_common();
			common.left.accounts.push(TaggedAccount::Messages {
				id: common.left.tx_params.signer.public().into(),
				bridged_chain: Self::Right::NAME.to_string(),
			});
			common.right.accounts.push(TaggedAccount::Messages {
				id: common.right.tx_params.signer.public().into(),
				bridged_chain: Self::Left::NAME.to_string(),
			});
		}

		// start on-demand header relays
		let (left_to_right_on_demand_headers, right_to_left_on_demand_headers) =
			self.mut_base().start_on_demand_headers_relayers().await?;

		// add balance-related metrics
		let lanes = self
			.base()
			.common()
			.shared
			.lane
			.iter()
			.cloned()
			.map(Into::into)
			.collect::<Vec<_>>();
		{
			let common = self.mut_base().mut_common();
			crate::messages_metrics::add_relay_balances_metrics::<_, Self::Right>(
				common.left.client.clone(),
				&common.metrics_params,
				&common.left.accounts,
				&lanes,
			)
			.await?;
			crate::messages_metrics::add_relay_balances_metrics::<_, Self::Left>(
				common.right.client.clone(),
				&common.metrics_params,
				&common.right.accounts,
				&lanes,
			)
			.await?;
		}

		// Need 2x capacity since we consider both directions for each lane
		let mut message_relays = Vec::with_capacity(lanes.len() * 2);
		for lane in lanes {
			let left_to_right_messages = crate::messages_lane::run::<
				<Self::L2R as MessagesCliBridge>::MessagesLane,
			>(self.left_to_right().messages_relay_params(
				left_to_right_on_demand_headers.clone(),
				right_to_left_on_demand_headers.clone(),
				lane,
				Self::L2R::maybe_messages_limits(),
			))
			.map_err(|e| anyhow::format_err!("{}", e))
			.boxed();
			message_relays.push(left_to_right_messages);

			let right_to_left_messages = crate::messages_lane::run::<
				<Self::R2L as MessagesCliBridge>::MessagesLane,
			>(self.right_to_left().messages_relay_params(
				right_to_left_on_demand_headers.clone(),
				left_to_right_on_demand_headers.clone(),
				lane,
				Self::R2L::maybe_messages_limits(),
			))
			.map_err(|e| anyhow::format_err!("{}", e))
			.boxed();
			message_relays.push(right_to_left_messages);
		}

		relay_utils::relay_metrics(self.base().common().metrics_params.clone())
			.expose()
			.await
			.map_err(|e| anyhow::format_err!("{}", e))?;

		futures::future::select_all(message_relays).await.0
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{cli::chain_schema::RuntimeVersionType, declare_chain_cli_schema};

	use relay_substrate_client::{ChainRuntimeVersion, Parachain, SimpleRuntimeVersion};

	#[test]
	// We need `#[allow(dead_code)]` because some of the methods generated by the macros
	// are not used.
	#[allow(dead_code)]
	fn should_parse_parachain_to_parachain_options() {
		// Chains.
		declare_chain_cli_schema!(Kusama, kusama);
		declare_chain_cli_schema!(BridgeHubKusama, bridge_hub_kusama);
		declare_chain_cli_schema!(Polkadot, polkadot);
		declare_chain_cli_schema!(BridgeHubPolkadot, bridge_hub_polkadot);
		// Means to override signers of different layer transactions.
		declare_chain_cli_schema!(
			KusamaHeadersToBridgeHubPolkadot,
			kusama_headers_to_bridge_hub_polkadot
		);
		declare_chain_cli_schema!(
			KusamaParachainsToBridgeHubPolkadot,
			kusama_parachains_to_bridge_hub_polkadot
		);
		declare_chain_cli_schema!(
			PolkadotHeadersToBridgeHubKusama,
			polkadot_headers_to_bridge_hub_kusama
		);
		declare_chain_cli_schema!(
			PolkadotParachainsToBridgeHubKusama,
			polkadot_parachains_to_bridge_hub_kusama
		);
		// Bridges.
		declare_parachain_to_parachain_bridge_schema!(
			BridgeHubKusama,
			Kusama,
			BridgeHubPolkadot,
			Polkadot
		);

		let res = BridgeHubKusamaBridgeHubPolkadotHeadersAndMessages::from_iter(vec![
			"bridge-hub-kusama-bridge-hub-polkadot-headers-and-messages",
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
				left: BridgeHubKusamaConnectionParams {
					bridge_hub_kusama_uri: None,
					bridge_hub_kusama_host: "bridge-hub-kusama-node-collator1".into(),
					bridge_hub_kusama_port: 9944,
					bridge_hub_kusama_path: None,
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
				left_relay: KusamaConnectionParams {
					kusama_uri: None,
					kusama_host: "kusama-alice".into(),
					kusama_port: 9944,
					kusama_path: None,
					kusama_secure: false,
					kusama_runtime_version: KusamaRuntimeVersionParams {
						kusama_version_mode: RuntimeVersionType::Bundle,
						kusama_spec_version: None,
						kusama_transaction_version: None,
					},
				},
				right: BridgeHubPolkadotConnectionParams {
					bridge_hub_polkadot_uri: None,
					bridge_hub_polkadot_host: "bridge-hub-polkadot-collator1".into(),
					bridge_hub_polkadot_port: 9944,
					bridge_hub_polkadot_path: None,
					bridge_hub_polkadot_secure: false,
					bridge_hub_polkadot_runtime_version: BridgeHubPolkadotRuntimeVersionParams {
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
					polkadot_uri: None,
					polkadot_host: "polkadot-alice".into(),
					polkadot_port: 9944,
					polkadot_path: None,
					polkadot_secure: false,
					polkadot_runtime_version: PolkadotRuntimeVersionParams {
						polkadot_version_mode: RuntimeVersionType::Bundle,
						polkadot_spec_version: None,
						polkadot_transaction_version: None,
					},
				},
			}
		);
	}
}
