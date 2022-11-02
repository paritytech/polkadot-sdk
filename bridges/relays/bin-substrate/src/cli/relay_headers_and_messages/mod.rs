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
mod relay_to_relay;
#[macro_use]
mod relay_to_parachain;

use async_trait::async_trait;
use std::{marker::PhantomData, sync::Arc};
use structopt::StructOpt;
use strum::VariantNames;

use futures::{FutureExt, TryFutureExt};
use relay_to_parachain::*;
use relay_to_relay::*;

use crate::{
	chains::{
		millau_headers_to_rialto::MillauToRialtoCliBridge,
		millau_headers_to_rialto_parachain::MillauToRialtoParachainCliBridge,
		rialto_headers_to_millau::RialtoToMillauCliBridge,
		rialto_parachains_to_millau::RialtoParachainToMillauCliBridge,
	},
	cli::{
		bridge::{
			CliBridgeBase, MessagesCliBridge, ParachainToRelayHeadersCliBridge,
			RelayToRelayHeadersCliBridge,
		},
		chain_schema::*,
		relay_messages::RelayerMode,
		CliChain, HexLaneId, PrometheusParams,
	},
	declare_chain_cli_schema,
};
use bp_messages::LaneId;
use bp_runtime::{BalanceOf, BlockNumberOf};
use messages_relay::relay_strategy::MixStrategy;
use relay_substrate_client::{
	AccountIdOf, AccountKeyPairOf, Chain, ChainWithBalances, ChainWithTransactions, Client,
};
use relay_utils::metrics::MetricsParams;
use sp_core::Pair;
use substrate_relay_helper::{
	messages_lane::MessagesRelayParams, messages_metrics::StandaloneMessagesMetrics,
	on_demand::OnDemandRelay, TaggedAccount, TransactionParams,
};

/// Maximal allowed conversion rate error ratio (abs(real - stored) / stored) that we allow.
///
/// If it is zero, then transaction will be submitted every time we see difference between
/// stored and real conversion rates. If it is large enough (e.g. > than 10 percents, which is 0.1),
/// then rational relayers may stop relaying messages because they were submitted using
/// lesser conversion rate.
pub(crate) const CONVERSION_RATE_ALLOWED_DIFFERENCE_RATIO: f64 = 0.05;

/// Parameters that have the same names across all bridges.
#[derive(Debug, PartialEq, StructOpt)]
pub struct HeadersAndMessagesSharedParams {
	/// Hex-encoded lane identifiers that should be served by the complex relay.
	#[structopt(long, default_value = "00000000")]
	pub lane: Vec<HexLaneId>,
	#[structopt(long, possible_values = RelayerMode::VARIANTS, case_insensitive = true, default_value = "rational")]
	pub relayer_mode: RelayerMode,
	/// If passed, only mandatory headers (headers that are changing the GRANDPA authorities set)
	/// are relayed.
	#[structopt(long)]
	pub only_mandatory_headers: bool,
	#[structopt(flatten)]
	pub prometheus_params: PrometheusParams,
}

pub struct Full2WayBridgeCommonParams<
	Left: ChainWithTransactions + CliChain,
	Right: ChainWithTransactions + CliChain,
> {
	pub shared: HeadersAndMessagesSharedParams,
	pub left: BridgeEndCommonParams<Left>,
	pub right: BridgeEndCommonParams<Right>,

	pub metrics_params: MetricsParams,
	pub left_to_right_metrics: StandaloneMessagesMetrics<Left, Right>,
	pub right_to_left_metrics: StandaloneMessagesMetrics<Right, Left>,
}

impl<Left: ChainWithTransactions + CliChain, Right: ChainWithTransactions + CliChain>
	Full2WayBridgeCommonParams<Left, Right>
{
	pub fn new<L2R: MessagesCliBridge<Source = Left, Target = Right>>(
		shared: HeadersAndMessagesSharedParams,
		left: BridgeEndCommonParams<Left>,
		right: BridgeEndCommonParams<Right>,
	) -> anyhow::Result<Self> {
		// Create metrics registry.
		let metrics_params = shared.prometheus_params.clone().into();
		let metrics_params = relay_utils::relay_metrics(metrics_params).into_params();
		let left_to_right_metrics = substrate_relay_helper::messages_metrics::standalone_metrics::<
			L2R::MessagesLane,
		>(left.client.clone(), right.client.clone())?;
		let right_to_left_metrics = left_to_right_metrics.clone().reverse();

		Ok(Self {
			shared,
			left,
			right,
			metrics_params,
			left_to_right_metrics,
			right_to_left_metrics,
		})
	}
}

pub struct BridgeEndCommonParams<Chain: ChainWithTransactions + CliChain> {
	pub client: Client<Chain>,
	pub sign: AccountKeyPairOf<Chain>,
	pub transactions_mortality: Option<u32>,
	pub messages_pallet_owner: Option<AccountKeyPairOf<Chain>>,
	pub accounts: Vec<TaggedAccount<AccountIdOf<Chain>>>,
}

struct FullBridge<
	'a,
	Source: ChainWithTransactions + CliChain,
	Target: ChainWithTransactions + CliChain,
	Bridge: MessagesCliBridge<Source = Source, Target = Target>,
> {
	shared: &'a HeadersAndMessagesSharedParams,
	source: &'a mut BridgeEndCommonParams<Source>,
	target: &'a mut BridgeEndCommonParams<Target>,
	metrics_params: &'a MetricsParams,
	metrics: &'a StandaloneMessagesMetrics<Source, Target>,
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
	fn new(
		shared: &'a HeadersAndMessagesSharedParams,
		source: &'a mut BridgeEndCommonParams<Source>,
		target: &'a mut BridgeEndCommonParams<Target>,
		metrics_params: &'a MetricsParams,
		metrics: &'a StandaloneMessagesMetrics<Source, Target>,
	) -> Self {
		Self { shared, source, target, metrics_params, metrics, _phantom_data: Default::default() }
	}

	fn start_conversion_rate_update_loop(&mut self) -> anyhow::Result<()> {
		if let Some(ref messages_pallet_owner) = self.source.messages_pallet_owner {
			let format_err = || {
				anyhow::format_err!(
					"Cannon run conversion rate updater: {} -> {}",
					Target::NAME,
					Source::NAME
				)
			};
			substrate_relay_helper::conversion_rate_update::run_conversion_rate_update_loop::<
				Bridge::MessagesLane,
			>(
				self.source.client.clone(),
				TransactionParams {
					signer: messages_pallet_owner.clone(),
					mortality: self.source.transactions_mortality,
				},
				self.metrics
					.target_to_source_conversion_rate
					.as_ref()
					.ok_or_else(format_err)?
					.shared_value_ref(),
				self.metrics
					.target_to_base_conversion_rate
					.as_ref()
					.ok_or_else(format_err)?
					.shared_value_ref(),
				self.metrics
					.source_to_base_conversion_rate
					.as_ref()
					.ok_or_else(format_err)?
					.shared_value_ref(),
				CONVERSION_RATE_ALLOWED_DIFFERENCE_RATIO,
			);
			self.source.accounts.push(TaggedAccount::MessagesPalletOwner {
				id: messages_pallet_owner.public().into(),
				bridged_chain: Target::NAME.to_string(),
			});
		}
		Ok(())
	}

	fn messages_relay_params(
		&self,
		source_to_target_headers_relay: Arc<dyn OnDemandRelay<BlockNumberOf<Source>>>,
		target_to_source_headers_relay: Arc<dyn OnDemandRelay<BlockNumberOf<Target>>>,
		lane_id: LaneId,
	) -> MessagesRelayParams<Bridge::MessagesLane> {
		let relayer_mode = self.shared.relayer_mode.into();
		let relay_strategy = MixStrategy::new(relayer_mode);

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
			standalone_metrics: Some(self.metrics.clone()),
			relay_strategy,
		}
	}
}

// All supported chains.
declare_chain_cli_schema!(Millau, millau);
declare_chain_cli_schema!(Rialto, rialto);
declare_chain_cli_schema!(RialtoParachain, rialto_parachain);
// Means to override signers of different layer transactions.
declare_chain_cli_schema!(MillauHeadersToRialto, millau_headers_to_rialto);
declare_chain_cli_schema!(MillauHeadersToRialtoParachain, millau_headers_to_rialto_parachain);
declare_chain_cli_schema!(RialtoHeadersToMillau, rialto_headers_to_millau);
declare_chain_cli_schema!(RialtoParachainsToMillau, rialto_parachains_to_millau);
// All supported bridges.
declare_relay_to_relay_bridge_schema!(Millau, Rialto);
declare_relay_to_parachain_bridge_schema!(Millau, RialtoParachain, Rialto);

#[async_trait]
trait Full2WayBridgeBase: Sized + Send + Sync {
	/// The CLI params for the bridge.
	type Params;
	/// The left relay chain.
	type Left: ChainWithTransactions + CliChain<KeyPair = AccountKeyPairOf<Self::Left>>;
	/// The right destination chain (it can be a relay or a parachain).
	type Right: ChainWithTransactions + CliChain<KeyPair = AccountKeyPairOf<Self::Right>>;

	fn common(&self) -> &Full2WayBridgeCommonParams<Self::Left, Self::Right>;

	fn mut_common(&mut self) -> &mut Full2WayBridgeCommonParams<Self::Left, Self::Right>;

	async fn start_on_demand_headers_relayers(
		&mut self,
	) -> anyhow::Result<(
		Arc<dyn OnDemandRelay<BlockNumberOf<Self::Left>>>,
		Arc<dyn OnDemandRelay<BlockNumberOf<Self::Right>>>,
	)>;
}

#[async_trait]
trait Full2WayBridge: Sized + Sync
where
	AccountIdOf<Self::Left>: From<<AccountKeyPairOf<Self::Left> as Pair>::Public>,
	AccountIdOf<Self::Right>: From<<AccountKeyPairOf<Self::Right> as Pair>::Public>,
	BalanceOf<Self::Left>: TryFrom<BalanceOf<Self::Right>> + Into<u128>,
	BalanceOf<Self::Right>: TryFrom<BalanceOf<Self::Left>> + Into<u128>,
{
	type Base: Full2WayBridgeBase<Left = Self::Left, Right = Self::Right>;

	/// The left relay chain.
	type Left: ChainWithTransactions
		+ ChainWithBalances
		+ CliChain<KeyPair = AccountKeyPairOf<Self::Left>>;
	/// The right relay chain.
	type Right: ChainWithTransactions
		+ ChainWithBalances
		+ CliChain<KeyPair = AccountKeyPairOf<Self::Right>>;

	// Left to Right bridge
	type L2R: MessagesCliBridge<Source = Self::Left, Target = Self::Right>;
	// Right to Left bridge
	type R2L: MessagesCliBridge<Source = Self::Right, Target = Self::Left>;

	fn new(params: <Self::Base as Full2WayBridgeBase>::Params) -> anyhow::Result<Self>;

	fn base(&self) -> &Self::Base;

	fn mut_base(&mut self) -> &mut Self::Base;

	fn left_to_right(&mut self) -> FullBridge<Self::Left, Self::Right, Self::L2R> {
		let common = self.mut_base().mut_common();
		FullBridge::<_, _, Self::L2R>::new(
			&common.shared,
			&mut common.left,
			&mut common.right,
			&common.metrics_params,
			&common.left_to_right_metrics,
		)
	}

	fn right_to_left(&mut self) -> FullBridge<Self::Right, Self::Left, Self::R2L> {
		let common = self.mut_base().mut_common();
		FullBridge::<_, _, Self::R2L>::new(
			&common.shared,
			&mut common.right,
			&mut common.left,
			&common.metrics_params,
			&common.right_to_left_metrics,
		)
	}

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

		// start conversion rate update loops for left/right chains
		self.left_to_right().start_conversion_rate_update_loop()?;
		self.right_to_left().start_conversion_rate_update_loop()?;

		// start on-demand header relays
		let (left_to_right_on_demand_headers, right_to_left_on_demand_headers) =
			self.mut_base().start_on_demand_headers_relayers().await?;

		// add balance-related metrics
		{
			let common = self.mut_base().mut_common();
			substrate_relay_helper::messages_metrics::add_relay_balances_metrics(
				common.left.client.clone(),
				&mut common.metrics_params,
				&common.left.accounts,
			)
			.await?;
			substrate_relay_helper::messages_metrics::add_relay_balances_metrics(
				common.right.client.clone(),
				&mut common.metrics_params,
				&common.right.accounts,
			)
			.await?;
		}

		let lanes = self.base().common().shared.lane.clone();
		// Need 2x capacity since we consider both directions for each lane
		let mut message_relays = Vec::with_capacity(lanes.len() * 2);
		for lane in lanes {
			let lane = lane.into();

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

/// Start headers+messages relayer process.
#[derive(Debug, PartialEq, StructOpt)]
pub enum RelayHeadersAndMessages {
	MillauRialto(MillauRialtoHeadersAndMessages),
	MillauRialtoParachain(MillauRialtoParachainHeadersAndMessages),
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
			"--millau-messages-pallet-owner",
			"//RialtoMessagesOwner",
			"--millau-transactions-mortality",
			"64",
			"--rialto-host",
			"rialto-node-alice",
			"--rialto-port",
			"9944",
			"--rialto-signer",
			"//Charlie",
			"--rialto-messages-pallet-owner",
			"//MillauMessagesOwner",
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
					relayer_mode: RelayerMode::Rational,
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
				left_messages_pallet_owner: MillauMessagesPalletOwnerSigningParams {
					millau_messages_pallet_owner: Some("//RialtoMessagesOwner".into()),
					millau_messages_pallet_owner_password: None,
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
				right_messages_pallet_owner: RialtoMessagesPalletOwnerSigningParams {
					rialto_messages_pallet_owner: Some("//MillauMessagesOwner".into()),
					rialto_messages_pallet_owner_password: None,
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
			"--millau-messages-pallet-owner",
			"//RialtoParachainMessagesOwner",
			"--millau-transactions-mortality",
			"64",
			"--rialto-parachain-host",
			"rialto-parachain-collator-charlie",
			"--rialto-parachain-port",
			"9944",
			"--rialto-parachain-signer",
			"//George",
			"--rialto-parachain-messages-pallet-owner",
			"//MillauMessagesOwner",
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
						relayer_mode: RelayerMode::Rational,
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
					left_messages_pallet_owner: MillauMessagesPalletOwnerSigningParams {
						millau_messages_pallet_owner: Some("//RialtoParachainMessagesOwner".into()),
						millau_messages_pallet_owner_password: None,
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
					right_messages_pallet_owner: RialtoParachainMessagesPalletOwnerSigningParams {
						rialto_parachain_messages_pallet_owner: Some(
							"//MillauMessagesOwner".into()
						),
						rialto_parachain_messages_pallet_owner_password: None,
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
