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
mod parachain_to_parachain;
#[macro_use]
mod relay_to_relay;
#[macro_use]
mod relay_to_parachain;

use async_trait::async_trait;
use std::{marker::PhantomData, sync::Arc};
use structopt::StructOpt;

use futures::{FutureExt, TryFutureExt};
use relay_to_parachain::*;
use relay_to_relay::*;

use crate::{
	bridges::{
		kusama_polkadot::{
			kusama_parachains_to_bridge_hub_polkadot::BridgeHubKusamaToBridgeHubPolkadotCliBridge,
			polkadot_parachains_to_bridge_hub_kusama::BridgeHubPolkadotToBridgeHubKusamaCliBridge,
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
			rococo_parachains_to_bridge_hub_wococo::BridgeHubRococoToBridgeHubWococoCliBridge,
			wococo_parachains_to_bridge_hub_rococo::BridgeHubWococoToBridgeHubRococoCliBridge,
		},
	},
	cli::{
		bridge::{
			CliBridgeBase, MessagesCliBridge, ParachainToRelayHeadersCliBridge,
			RelayToRelayHeadersCliBridge,
		},
		chain_schema::*,
		relay_headers_and_messages::parachain_to_parachain::ParachainToParachainBridge,
		CliChain, HexLaneId, PrometheusParams,
	},
	declare_chain_cli_schema,
};
use bp_messages::LaneId;
use bp_runtime::BalanceOf;
use relay_substrate_client::{
	AccountIdOf, AccountKeyPairOf, Chain, ChainWithBalances, ChainWithMessages,
	ChainWithTransactions, Client, Parachain,
};
use relay_utils::metrics::MetricsParams;
use sp_core::Pair;
use substrate_relay_helper::{
	messages_lane::MessagesRelayParams, on_demand::OnDemandRelay, TaggedAccount, TransactionParams,
};

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
	pub prometheus_params: PrometheusParams,
}

/// Bridge parameters, shared by all bridge types.
pub struct Full2WayBridgeCommonParams<
	Left: ChainWithTransactions + CliChain,
	Right: ChainWithTransactions + CliChain,
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

impl<Left: ChainWithTransactions + CliChain, Right: ChainWithTransactions + CliChain>
	Full2WayBridgeCommonParams<Left, Right>
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
pub struct BridgeEndCommonParams<Chain: ChainWithTransactions + CliChain> {
	/// Chain client.
	pub client: Client<Chain>,
	/// Transactions signer.
	pub sign: AccountKeyPairOf<Chain>,
	/// Transactions mortality.
	pub transactions_mortality: Option<u32>,
	/// Accounts, which balances are exposed as metrics by the relay process.
	pub accounts: Vec<TaggedAccount<AccountIdOf<Chain>>>,
}

/// All data of the bidirectional complex relay.
struct FullBridge<
	'a,
	Source: ChainWithTransactions + CliChain,
	Target: ChainWithTransactions + CliChain,
	Bridge: MessagesCliBridge<Source = Source, Target = Target>,
> {
	source: &'a mut BridgeEndCommonParams<Source>,
	target: &'a mut BridgeEndCommonParams<Target>,
	metrics_params: &'a MetricsParams,
	_phantom_data: PhantomData<Bridge>,
}

impl<
		'a,
		Source: ChainWithTransactions + CliChain,
		Target: ChainWithTransactions + CliChain,
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
	) -> MessagesRelayParams<Bridge::MessagesLane> {
		MessagesRelayParams {
			source_client: self.source.client.clone(),
			source_transaction_params: TransactionParams {
				signer: self.source.sign.clone(),
				mortality: self.source.transactions_mortality,
			},
			target_client: self.target.client.clone(),
			target_transaction_params: TransactionParams {
				signer: self.target.sign.clone(),
				mortality: self.target.transactions_mortality,
			},
			source_to_target_headers_relay: Some(source_to_target_headers_relay),
			target_to_source_headers_relay: Some(target_to_source_headers_relay),
			lane_id,
			metrics_params: self.metrics_params.clone().disable(),
		}
	}
}

// All supported chains.
declare_chain_cli_schema!(Millau, millau);
declare_chain_cli_schema!(Rialto, rialto);
declare_chain_cli_schema!(RialtoParachain, rialto_parachain);
declare_chain_cli_schema!(Rococo, rococo);
declare_chain_cli_schema!(BridgeHubRococo, bridge_hub_rococo);
declare_chain_cli_schema!(Wococo, wococo);
declare_chain_cli_schema!(BridgeHubWococo, bridge_hub_wococo);
declare_chain_cli_schema!(Kusama, kusama);
declare_chain_cli_schema!(BridgeHubKusama, bridge_hub_kusama);
declare_chain_cli_schema!(Polkadot, polkadot);
declare_chain_cli_schema!(BridgeHubPolkadot, bridge_hub_polkadot);
// Means to override signers of different layer transactions.
declare_chain_cli_schema!(MillauHeadersToRialto, millau_headers_to_rialto);
declare_chain_cli_schema!(MillauHeadersToRialtoParachain, millau_headers_to_rialto_parachain);
declare_chain_cli_schema!(RialtoHeadersToMillau, rialto_headers_to_millau);
declare_chain_cli_schema!(RialtoParachainsToMillau, rialto_parachains_to_millau);
declare_chain_cli_schema!(RococoHeadersToBridgeHubWococo, rococo_headers_to_bridge_hub_wococo);
declare_chain_cli_schema!(
	RococoParachainsToBridgeHubWococo,
	rococo_parachains_to_bridge_hub_wococo
);
declare_chain_cli_schema!(WococoHeadersToBridgeHubRococo, wococo_headers_to_bridge_hub_rococo);
declare_chain_cli_schema!(
	WococoParachainsToBridgeHubRococo,
	wococo_parachains_to_bridge_hub_rococo
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
// All supported bridges.
declare_relay_to_relay_bridge_schema!(Millau, Rialto);
declare_relay_to_parachain_bridge_schema!(Millau, RialtoParachain, Rialto);
declare_parachain_to_parachain_bridge_schema!(BridgeHubRococo, Rococo, BridgeHubWococo, Wococo);
declare_parachain_to_parachain_bridge_schema!(BridgeHubKusama, Kusama, BridgeHubPolkadot, Polkadot);

/// Base portion of the bidirectional complex relay.
///
/// This main purpose of extracting this trait is that in different relays the implementation
/// of `start_on_demand_headers_relayers` method will be different. But the number of
/// implementations is limited to relay <> relay, parachain <> relay and parachain <> parachain.
/// This trait allows us to reuse these implementations in different bridges.
#[async_trait]
trait Full2WayBridgeBase: Sized + Send + Sync {
	/// The CLI params for the bridge.
	type Params;
	/// The left relay chain.
	type Left: ChainWithTransactions + CliChain;
	/// The right destination chain (it can be a relay or a parachain).
	type Right: ChainWithTransactions + CliChain;

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
trait Full2WayBridge: Sized + Sync
where
	AccountIdOf<Self::Left>: From<<AccountKeyPairOf<Self::Left> as Pair>::Public>,
	AccountIdOf<Self::Right>: From<<AccountKeyPairOf<Self::Right> as Pair>::Public>,
	BalanceOf<Self::Left>: TryFrom<BalanceOf<Self::Right>> + Into<u128>,
	BalanceOf<Self::Right>: TryFrom<BalanceOf<Self::Left>> + Into<u128>,
{
	/// Base portion of the bidirectional complex relay.
	type Base: Full2WayBridgeBase<Left = Self::Left, Right = Self::Right>;

	/// The left relay chain.
	type Left: ChainWithTransactions + ChainWithBalances + ChainWithMessages + CliChain;
	/// The right relay chain.
	type Right: ChainWithTransactions + ChainWithBalances + ChainWithMessages + CliChain;

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
				id: common.left.sign.public().into(),
				bridged_chain: Self::Right::NAME.to_string(),
			});
			common.right.accounts.push(TaggedAccount::Messages {
				id: common.right.sign.public().into(),
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
			substrate_relay_helper::messages_metrics::add_relay_balances_metrics::<_, Self::Right>(
				common.left.client.clone(),
				&mut common.metrics_params,
				&common.left.accounts,
				&lanes,
			)
			.await?;
			substrate_relay_helper::messages_metrics::add_relay_balances_metrics::<_, Self::Left>(
				common.right.client.clone(),
				&mut common.metrics_params,
				&common.right.accounts,
				&lanes,
			)
			.await?;
		}

		// Need 2x capacity since we consider both directions for each lane
		let mut message_relays = Vec::with_capacity(lanes.len() * 2);
		for lane in lanes {
			let left_to_right_messages = substrate_relay_helper::messages_lane::run::<
				<Self::L2R as MessagesCliBridge>::MessagesLane,
			>(self.left_to_right().messages_relay_params(
				left_to_right_on_demand_headers.clone(),
				right_to_left_on_demand_headers.clone(),
				lane,
			))
			.map_err(|e| anyhow::format_err!("{}", e))
			.boxed();
			message_relays.push(left_to_right_messages);

			let right_to_left_messages = substrate_relay_helper::messages_lane::run::<
				<Self::R2L as MessagesCliBridge>::MessagesLane,
			>(self.right_to_left().messages_relay_params(
				right_to_left_on_demand_headers.clone(),
				left_to_right_on_demand_headers.clone(),
				lane,
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

/// Millau <> Rialto complex relay.
pub struct MillauRialtoFull2WayBridge {
	base: <Self as Full2WayBridge>::Base,
}

#[async_trait]
impl Full2WayBridge for MillauRialtoFull2WayBridge {
	type Base = RelayToRelayBridge<Self::L2R, Self::R2L>;
	type Left = relay_millau_client::Millau;
	type Right = relay_rialto_client::Rialto;
	type L2R = MillauToRialtoCliBridge;
	type R2L = RialtoToMillauCliBridge;

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

/// Millau <> RialtoParachain complex relay.
pub struct MillauRialtoParachainFull2WayBridge {
	base: <Self as Full2WayBridge>::Base,
}

#[async_trait]
impl Full2WayBridge for MillauRialtoParachainFull2WayBridge {
	type Base = RelayToParachainBridge<Self::L2R, Self::R2L>;
	type Left = relay_millau_client::Millau;
	type Right = relay_rialto_parachain_client::RialtoParachain;
	type L2R = MillauToRialtoParachainCliBridge;
	type R2L = RialtoParachainToMillauCliBridge;

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

/// BridgeHubRococo <> BridgeHubWococo complex relay.
pub struct BridgeHubRococoBridgeHubWococoFull2WayBridge {
	base: <Self as Full2WayBridge>::Base,
}

#[async_trait]
impl Full2WayBridge for BridgeHubRococoBridgeHubWococoFull2WayBridge {
	type Base = ParachainToParachainBridge<Self::L2R, Self::R2L>;
	type Left = relay_bridge_hub_rococo_client::BridgeHubRococo;
	type Right = relay_bridge_hub_wococo_client::BridgeHubWococo;
	type L2R = BridgeHubRococoToBridgeHubWococoCliBridge;
	type R2L = BridgeHubWococoToBridgeHubRococoCliBridge;

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

/// Complex headers+messages relay.
#[derive(Debug, PartialEq, StructOpt)]
pub enum RelayHeadersAndMessages {
	/// Millau <> Rialto relay.
	MillauRialto(MillauRialtoHeadersAndMessages),
	/// Millau <> RialtoParachain relay.
	MillauRialtoParachain(MillauRialtoParachainHeadersAndMessages),
	/// BridgeHubRococo <> BridgeHubWococo relay.
	BridgeHubRococoBridgeHubWococo(BridgeHubRococoBridgeHubWococoHeadersAndMessages),
	/// BridgeHubKusama <> BridgeHubPolkadot relay.
	BridgeHubKusamaBridgeHubPolkadot(BridgeHubKusamaBridgeHubPolkadotHeadersAndMessages),
}

impl RelayHeadersAndMessages {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		match self {
			RelayHeadersAndMessages::MillauRialto(params) =>
				MillauRialtoFull2WayBridge::new(params.into_bridge().await?)?.run().await,
			RelayHeadersAndMessages::MillauRialtoParachain(params) =>
				MillauRialtoParachainFull2WayBridge::new(params.into_bridge().await?)?
					.run()
					.await,
			RelayHeadersAndMessages::BridgeHubRococoBridgeHubWococo(params) =>
				BridgeHubRococoBridgeHubWococoFull2WayBridge::new(params.into_bridge().await?)?
					.run()
					.await,
			RelayHeadersAndMessages::BridgeHubKusamaBridgeHubPolkadot(params) =>
				BridgeHubKusamaBridgeHubPolkadotFull2WayBridge::new(params.into_bridge().await?)?
					.run()
					.await,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn should_parse_relay_to_relay_options() {
		// when
		let res = RelayHeadersAndMessages::from_iter(vec![
			"relay-headers-and-messages",
			"millau-rialto",
			"--millau-host",
			"millau-node-alice",
			"--millau-port",
			"9944",
			"--millau-signer",
			"//Charlie",
			"--millau-transactions-mortality",
			"64",
			"--rialto-host",
			"rialto-node-alice",
			"--rialto-port",
			"9944",
			"--rialto-signer",
			"//Charlie",
			"--rialto-transactions-mortality",
			"64",
			"--lane",
			"00000000",
			"--lane",
			"73776170",
			"--prometheus-host",
			"0.0.0.0",
		]);

		// then
		assert_eq!(
			res,
			RelayHeadersAndMessages::MillauRialto(MillauRialtoHeadersAndMessages {
				shared: HeadersAndMessagesSharedParams {
					lane: vec![
						HexLaneId([0x00, 0x00, 0x00, 0x00]),
						HexLaneId([0x73, 0x77, 0x61, 0x70])
					],
					only_mandatory_headers: false,
					prometheus_params: PrometheusParams {
						no_prometheus: false,
						prometheus_host: "0.0.0.0".into(),
						prometheus_port: 9616,
					},
				},
				left: MillauConnectionParams {
					millau_host: "millau-node-alice".into(),
					millau_port: 9944,
					millau_secure: false,
					millau_runtime_version: MillauRuntimeVersionParams {
						millau_version_mode: RuntimeVersionType::Bundle,
						millau_spec_version: None,
						millau_transaction_version: None,
					},
				},
				left_sign: MillauSigningParams {
					millau_signer: Some("//Charlie".into()),
					millau_signer_password: None,
					millau_signer_file: None,
					millau_signer_password_file: None,
					millau_transactions_mortality: Some(64),
				},
				left_headers_to_right_sign_override: MillauHeadersToRialtoSigningParams {
					millau_headers_to_rialto_signer: None,
					millau_headers_to_rialto_signer_password: None,
					millau_headers_to_rialto_signer_file: None,
					millau_headers_to_rialto_signer_password_file: None,
					millau_headers_to_rialto_transactions_mortality: None,
				},
				right: RialtoConnectionParams {
					rialto_host: "rialto-node-alice".into(),
					rialto_port: 9944,
					rialto_secure: false,
					rialto_runtime_version: RialtoRuntimeVersionParams {
						rialto_version_mode: RuntimeVersionType::Bundle,
						rialto_spec_version: None,
						rialto_transaction_version: None,
					},
				},
				right_sign: RialtoSigningParams {
					rialto_signer: Some("//Charlie".into()),
					rialto_signer_password: None,
					rialto_signer_file: None,
					rialto_signer_password_file: None,
					rialto_transactions_mortality: Some(64),
				},
				right_headers_to_left_sign_override: RialtoHeadersToMillauSigningParams {
					rialto_headers_to_millau_signer: None,
					rialto_headers_to_millau_signer_password: None,
					rialto_headers_to_millau_signer_file: None,
					rialto_headers_to_millau_signer_password_file: None,
					rialto_headers_to_millau_transactions_mortality: None,
				},
			}),
		);
	}

	#[test]
	fn should_parse_relay_to_parachain_options() {
		// when
		let res = RelayHeadersAndMessages::from_iter(vec![
			"relay-headers-and-messages",
			"millau-rialto-parachain",
			"--millau-host",
			"millau-node-alice",
			"--millau-port",
			"9944",
			"--millau-signer",
			"//Iden",
			"--rialto-headers-to-millau-signer",
			"//Ken",
			"--millau-transactions-mortality",
			"64",
			"--rialto-parachain-host",
			"rialto-parachain-collator-charlie",
			"--rialto-parachain-port",
			"9944",
			"--rialto-parachain-signer",
			"//George",
			"--rialto-parachain-transactions-mortality",
			"64",
			"--rialto-host",
			"rialto-node-alice",
			"--rialto-port",
			"9944",
			"--lane",
			"00000000",
			"--prometheus-host",
			"0.0.0.0",
		]);

		// then
		assert_eq!(
			res,
			RelayHeadersAndMessages::MillauRialtoParachain(
				MillauRialtoParachainHeadersAndMessages {
					shared: HeadersAndMessagesSharedParams {
						lane: vec![HexLaneId([0x00, 0x00, 0x00, 0x00])],
						only_mandatory_headers: false,
						prometheus_params: PrometheusParams {
							no_prometheus: false,
							prometheus_host: "0.0.0.0".into(),
							prometheus_port: 9616,
						},
					},
					left: MillauConnectionParams {
						millau_host: "millau-node-alice".into(),
						millau_port: 9944,
						millau_secure: false,
						millau_runtime_version: MillauRuntimeVersionParams {
							millau_version_mode: RuntimeVersionType::Bundle,
							millau_spec_version: None,
							millau_transaction_version: None,
						},
					},
					left_sign: MillauSigningParams {
						millau_signer: Some("//Iden".into()),
						millau_signer_password: None,
						millau_signer_file: None,
						millau_signer_password_file: None,
						millau_transactions_mortality: Some(64),
					},
					left_headers_to_right_sign_override:
						MillauHeadersToRialtoParachainSigningParams {
							millau_headers_to_rialto_parachain_signer: None,
							millau_headers_to_rialto_parachain_signer_password: None,
							millau_headers_to_rialto_parachain_signer_file: None,
							millau_headers_to_rialto_parachain_signer_password_file: None,
							millau_headers_to_rialto_parachain_transactions_mortality: None,
						},
					right: RialtoParachainConnectionParams {
						rialto_parachain_host: "rialto-parachain-collator-charlie".into(),
						rialto_parachain_port: 9944,
						rialto_parachain_secure: false,
						rialto_parachain_runtime_version: RialtoParachainRuntimeVersionParams {
							rialto_parachain_version_mode: RuntimeVersionType::Bundle,
							rialto_parachain_spec_version: None,
							rialto_parachain_transaction_version: None,
						},
					},
					right_sign: RialtoParachainSigningParams {
						rialto_parachain_signer: Some("//George".into()),
						rialto_parachain_signer_password: None,
						rialto_parachain_signer_file: None,
						rialto_parachain_signer_password_file: None,
						rialto_parachain_transactions_mortality: Some(64),
					},
					right_relay_headers_to_left_sign_override: RialtoHeadersToMillauSigningParams {
						rialto_headers_to_millau_signer: Some("//Ken".into()),
						rialto_headers_to_millau_signer_password: None,
						rialto_headers_to_millau_signer_file: None,
						rialto_headers_to_millau_signer_password_file: None,
						rialto_headers_to_millau_transactions_mortality: None,
					},
					right_parachains_to_left_sign_override: RialtoParachainsToMillauSigningParams {
						rialto_parachains_to_millau_signer: None,
						rialto_parachains_to_millau_signer_password: None,
						rialto_parachains_to_millau_signer_file: None,
						rialto_parachains_to_millau_signer_password_file: None,
						rialto_parachains_to_millau_transactions_mortality: None,
					},
					right_relay: RialtoConnectionParams {
						rialto_host: "rialto-node-alice".into(),
						rialto_port: 9944,
						rialto_secure: false,
						rialto_runtime_version: RialtoRuntimeVersionParams {
							rialto_version_mode: RuntimeVersionType::Bundle,
							rialto_spec_version: None,
							rialto_transaction_version: None,
						},
					},
				}
			),
		);
	}
}
