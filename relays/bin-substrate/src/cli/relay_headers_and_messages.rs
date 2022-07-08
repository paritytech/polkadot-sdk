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

//! Complex headers+messages relays support.
//!
//! To add new complex relay between `ChainA` and `ChainB`, you must:
//!
//! 1) ensure that there's a `declare_chain_options!(...)` for both chains;
//! 2) add `declare_bridge_options!(...)` for the bridge;
//! 3) add bridge support to the `select_bridge! { ... }` macro.

use futures::{FutureExt, TryFutureExt};
use structopt::StructOpt;
use strum::VariantNames;

use async_std::sync::Arc;
use bp_polkadot_core::parachains::ParaHash;
use messages_relay::relay_strategy::MixStrategy;
use pallet_bridge_parachains::{RelayBlockHash, RelayBlockHasher, RelayBlockNumber};
use relay_substrate_client::{
	AccountIdOf, AccountKeyPairOf, BlockNumberOf, Chain, ChainRuntimeVersion, Client,
	TransactionSignScheme,
};
use relay_utils::metrics::MetricsParams;
use sp_core::Pair;
use substrate_relay_helper::{
	finality::SubstrateFinalitySyncPipeline,
	messages_lane::MessagesRelayParams,
	on_demand::{
		headers::OnDemandHeadersRelay, parachains::OnDemandParachainsRelay, OnDemandRelay,
	},
	parachains::SubstrateParachainsPipeline,
	TaggedAccount, TransactionParams,
};

use crate::{
	cli::{
		relay_messages::RelayerMode, CliChain, HexLaneId, PrometheusParams, RuntimeVersionType,
		TransactionParamsProvider,
	},
	declare_chain_options,
};

/// Maximal allowed conversion rate error ratio (abs(real - stored) / stored) that we allow.
///
/// If it is zero, then transaction will be submitted every time we see difference between
/// stored and real conversion rates. If it is large enough (e.g. > than 10 percents, which is 0.1),
/// then rational relayers may stop relaying messages because they were submitted using
/// lesser conversion rate.
pub(crate) const CONVERSION_RATE_ALLOWED_DIFFERENCE_RATIO: f64 = 0.05;

/// Start headers+messages relayer process.
#[derive(Debug, PartialEq, StructOpt)]
pub enum RelayHeadersAndMessages {
	MillauRialto(MillauRialtoHeadersAndMessages),
	MillauRialtoParachain(MillauRialtoParachainHeadersAndMessages),
}

/// Parameters that have the same names across all bridges.
#[derive(Debug, PartialEq, StructOpt)]
pub struct HeadersAndMessagesSharedParams {
	/// Hex-encoded lane identifiers that should be served by the complex relay.
	#[structopt(long, default_value = "00000000")]
	lane: Vec<HexLaneId>,
	#[structopt(long, possible_values = RelayerMode::VARIANTS, case_insensitive = true, default_value = "rational")]
	relayer_mode: RelayerMode,
	/// Create relayers fund accounts on both chains, if it does not exists yet.
	#[structopt(long)]
	create_relayers_fund_accounts: bool,
	/// If passed, only mandatory headers (headers that are changing the GRANDPA authorities set)
	/// are relayed.
	#[structopt(long)]
	only_mandatory_headers: bool,
	#[structopt(flatten)]
	prometheus_params: PrometheusParams,
}

// The reason behind this macro is that 'normal' relays are using source and target chains
// terminology, which is unusable for both-way relays (if you're relaying headers from Rialto to
// Millau and from Millau to Rialto, then which chain is source?).
macro_rules! declare_bridge_options {
	// chain, parachain, relay-chain-of-parachain
	($chain1:ident, $chain2:ident, $chain3:ident) => {
		paste::item! {
			#[doc = $chain1 ", " $chain2 " and " $chain3 " headers+parachains+messages relay params."]
			#[derive(Debug, PartialEq, StructOpt)]
			pub struct [<$chain1 $chain2 HeadersAndMessages>] {
				#[structopt(flatten)]
				shared: HeadersAndMessagesSharedParams,
				#[structopt(flatten)]
				left: [<$chain1 ConnectionParams>],
				// default signer, which is always used to sign messages relay transactions on the left chain
				#[structopt(flatten)]
				left_sign: [<$chain1 SigningParams>],
				// override for right_relay->left headers signer
				#[structopt(flatten)]
				right_relay_headers_to_left_sign_override: [<$chain3 HeadersTo $chain1 SigningParams>],
				// override for right->left parachains signer
				#[structopt(flatten)]
				right_parachains_to_left_sign_override: [<$chain3 ParachainsTo $chain1 SigningParams>],
				#[structopt(flatten)]
				left_messages_pallet_owner: [<$chain1 MessagesPalletOwnerSigningParams>],
				#[structopt(flatten)]
				right: [<$chain2 ConnectionParams>],
				// default signer, which is always used to sign messages relay transactions on the right chain
				#[structopt(flatten)]
				right_sign: [<$chain2 SigningParams>],
				// override for left->right headers signer
				#[structopt(flatten)]
				left_headers_to_right_sign_override: [<$chain1 HeadersTo $chain2 SigningParams>],
				#[structopt(flatten)]
				right_messages_pallet_owner: [<$chain2 MessagesPalletOwnerSigningParams>],
				#[structopt(flatten)]
				right_relay: [<$chain3 ConnectionParams>],
			}
		}

		declare_bridge_options!({ implement }, $chain1, $chain2);
	};
	($chain1:ident, $chain2:ident) => {
		paste::item! {
			#[doc = $chain1 " and " $chain2 " headers+messages relay params."]
			#[derive(Debug, PartialEq, StructOpt)]
			pub struct [<$chain1 $chain2 HeadersAndMessages>] {
				#[structopt(flatten)]
				shared: HeadersAndMessagesSharedParams,
				// default signer, which is always used to sign messages relay transactions on the left chain
				#[structopt(flatten)]
				left: [<$chain1 ConnectionParams>],
				// override for right->left headers signer
				#[structopt(flatten)]
				right_headers_to_left_sign_override: [<$chain2 HeadersTo $chain1 SigningParams>],
				#[structopt(flatten)]
				left_sign: [<$chain1 SigningParams>],
				#[structopt(flatten)]
				left_messages_pallet_owner: [<$chain1 MessagesPalletOwnerSigningParams>],
				// default signer, which is always used to sign messages relay transactions on the right chain
				#[structopt(flatten)]
				right: [<$chain2 ConnectionParams>],
				// override for left->right headers signer
				#[structopt(flatten)]
				left_headers_to_right_sign_override: [<$chain1 HeadersTo $chain2 SigningParams>],
				#[structopt(flatten)]
				right_sign: [<$chain2 SigningParams>],
				#[structopt(flatten)]
				right_messages_pallet_owner: [<$chain2 MessagesPalletOwnerSigningParams>],
			}
		}

		declare_bridge_options!({ implement }, $chain1, $chain2);
	};
	({ implement }, $chain1:ident, $chain2:ident) => {
		paste::item! {
			impl From<RelayHeadersAndMessages> for [<$chain1 $chain2 HeadersAndMessages>] {
				fn from(relay_params: RelayHeadersAndMessages) -> [<$chain1 $chain2 HeadersAndMessages>] {
					match relay_params {
						RelayHeadersAndMessages::[<$chain1 $chain2>](params) => params,
						_ => unreachable!(),
					}
				}
			}
		}
	};
}

macro_rules! select_bridge {
	($bridge: expr, $generic: tt) => {
		match $bridge {
			RelayHeadersAndMessages::MillauRialto(_) => {
				type Params = MillauRialtoHeadersAndMessages;

				type Left = relay_millau_client::Millau;
				type Right = relay_rialto_client::Rialto;

				type LeftAccountIdConverter = bp_millau::AccountIdConverter;
				type RightAccountIdConverter = bp_rialto::AccountIdConverter;

				use crate::chains::{
					millau_messages_to_rialto::MillauMessagesToRialto as LeftToRightMessageLane,
					rialto_messages_to_millau::RialtoMessagesToMillau as RightToLeftMessageLane,
				};

				async fn start_on_demand_relays(
					params: &Params,
					left_client: Client<Left>,
					right_client: Client<Right>,
					at_left_relay_accounts: &mut Vec<TaggedAccount<AccountIdOf<Left>>>,
					at_right_relay_accounts: &mut Vec<TaggedAccount<AccountIdOf<Right>>>,
				) -> anyhow::Result<(
					Arc<dyn OnDemandRelay<BlockNumberOf<Left>>>,
					Arc<dyn OnDemandRelay<BlockNumberOf<Right>>>,
				)> {
					start_on_demand_relay_to_relay::<
						Left,
						Right,
						crate::chains::millau_headers_to_rialto::MillauFinalityToRialto,
						crate::chains::rialto_headers_to_millau::RialtoFinalityToMillau,
					>(
						left_client,
						right_client,
						params.left_headers_to_right_sign_override.transaction_params_or::<Right, _>(&params.right_sign)?,
						params.right_headers_to_left_sign_override.transaction_params_or::<Left, _>(&params.left_sign)?,
						params.shared.only_mandatory_headers,
						params.shared.only_mandatory_headers,
						params.left.can_start_version_guard(),
						params.right.can_start_version_guard(),
						at_left_relay_accounts,
						at_right_relay_accounts,
					).await
				}

				async fn left_create_account(
					_left_client: Client<Left>,
					_left_sign: <Left as TransactionSignScheme>::AccountKeyPair,
					_account_id: AccountIdOf<Left>,
				) -> anyhow::Result<()> {
					Err(anyhow::format_err!("Account creation is not supported by this bridge"))
				}

				async fn right_create_account(
					_right_client: Client<Right>,
					_right_sign: <Right as TransactionSignScheme>::AccountKeyPair,
					_account_id: AccountIdOf<Right>,
				) -> anyhow::Result<()> {
					Err(anyhow::format_err!("Account creation is not supported by this bridge"))
				}

				$generic
			},
			RelayHeadersAndMessages::MillauRialtoParachain(_) => {
				type Params = MillauRialtoParachainHeadersAndMessages;

				type Left = relay_millau_client::Millau;
				type Right = relay_rialto_parachain_client::RialtoParachain;

				type LeftAccountIdConverter = bp_millau::AccountIdConverter;
				type RightAccountIdConverter = bp_rialto_parachain::AccountIdConverter;

				use crate::chains::{
					millau_messages_to_rialto_parachain::MillauMessagesToRialtoParachain as LeftToRightMessageLane,
					rialto_parachain_messages_to_millau::RialtoParachainMessagesToMillau as RightToLeftMessageLane,
				};

				async fn start_on_demand_relays(
					params: &Params,
					left_client: Client<Left>,
					right_client: Client<Right>,
					at_left_relay_accounts: &mut Vec<TaggedAccount<AccountIdOf<Left>>>,
					at_right_relay_accounts: &mut Vec<TaggedAccount<AccountIdOf<Right>>>,
				) -> anyhow::Result<(
					Arc<dyn OnDemandRelay<BlockNumberOf<Left>>>,
					Arc<dyn OnDemandRelay<BlockNumberOf<Right>>>,
				)> {
					type RightRelayChain = relay_rialto_client::Rialto;
					let rialto_relay_chain_client = params.right_relay.to_client::<RightRelayChain>().await?;

					start_on_demand_relay_to_parachain::<
						Left,
						Right,
						RightRelayChain,
						crate::chains::millau_headers_to_rialto_parachain::MillauFinalityToRialtoParachain,
						crate::chains::rialto_headers_to_millau::RialtoFinalityToMillau,
						crate::chains::rialto_parachains_to_millau::RialtoParachainsToMillau,
					>(
						left_client,
						right_client,
						rialto_relay_chain_client,
						params.left_headers_to_right_sign_override.transaction_params_or::<Right, _>(&params.right_sign)?,
						params.right_relay_headers_to_left_sign_override.transaction_params_or::<Left, _>(&params.left_sign)?,
						params.right_parachains_to_left_sign_override.transaction_params_or::<Left, _>(&params.left_sign)?,
						params.shared.only_mandatory_headers,
						params.shared.only_mandatory_headers,
						params.left.can_start_version_guard(),
						params.right.can_start_version_guard(),
						at_left_relay_accounts,
						at_right_relay_accounts,
					).await
				}

				async fn left_create_account(
					_left_client: Client<Left>,
					_left_sign: <Left as TransactionSignScheme>::AccountKeyPair,
					_account_id: AccountIdOf<Left>,
				) -> anyhow::Result<()> {
					Err(anyhow::format_err!("Account creation is not supported by this bridge"))
				}

				async fn right_create_account(
					_right_client: Client<Right>,
					_right_sign: <Right as TransactionSignScheme>::AccountKeyPair,
					_account_id: AccountIdOf<Right>,
				) -> anyhow::Result<()> {
					Err(anyhow::format_err!("Account creation is not supported by this bridge"))
				}

				$generic
			},
		}
	};
}

// All supported chains.
declare_chain_options!(Millau, millau);
declare_chain_options!(Rialto, rialto);
declare_chain_options!(RialtoParachain, rialto_parachain);
// Means to override signers of different layer transactions.
declare_chain_options!(MillauHeadersToRialto, millau_headers_to_rialto);
declare_chain_options!(MillauHeadersToRialtoParachain, millau_headers_to_rialto_parachain);
declare_chain_options!(RialtoHeadersToMillau, rialto_headers_to_millau);
declare_chain_options!(RialtoParachainsToMillau, rialto_parachains_to_millau);
// All supported bridges.
declare_bridge_options!(Millau, Rialto);
declare_bridge_options!(Millau, RialtoParachain, Rialto);

impl RelayHeadersAndMessages {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		select_bridge!(self, {
			let params: Params = self.into();

			let left_client = params.left.to_client::<Left>().await?;
			let left_transactions_mortality = params.left_sign.transactions_mortality()?;
			let left_sign = params.left_sign.to_keypair::<Left>()?;
			let left_messages_pallet_owner =
				params.left_messages_pallet_owner.to_keypair::<Left>()?;
			let right_client = params.right.to_client::<Right>().await?;
			let right_transactions_mortality = params.right_sign.transactions_mortality()?;
			let right_sign = params.right_sign.to_keypair::<Right>()?;
			let right_messages_pallet_owner =
				params.right_messages_pallet_owner.to_keypair::<Right>()?;

			let lanes = params.shared.lane.clone();
			let relayer_mode = params.shared.relayer_mode.into();
			let relay_strategy = MixStrategy::new(relayer_mode);

			// create metrics registry and register standalone metrics
			let metrics_params: MetricsParams = params.shared.prometheus_params.clone().into();
			let metrics_params = relay_utils::relay_metrics(metrics_params).into_params();
			let left_to_right_metrics =
				substrate_relay_helper::messages_metrics::standalone_metrics::<
					LeftToRightMessageLane,
				>(left_client.clone(), right_client.clone())?;
			let right_to_left_metrics = left_to_right_metrics.clone().reverse();
			let mut at_left_relay_accounts = vec![TaggedAccount::Messages {
				id: left_sign.public().into(),
				bridged_chain: Right::NAME.to_string(),
			}];
			let mut at_right_relay_accounts = vec![TaggedAccount::Messages {
				id: right_sign.public().into(),
				bridged_chain: Left::NAME.to_string(),
			}];

			// start conversion rate update loops for left/right chains
			if let Some(left_messages_pallet_owner) = left_messages_pallet_owner.clone() {
				let left_client = left_client.clone();
				let format_err = || {
					anyhow::format_err!(
						"Cannon run conversion rate updater: {} -> {}",
						Right::NAME,
						Left::NAME
					)
				};
				substrate_relay_helper::conversion_rate_update::run_conversion_rate_update_loop::<
					LeftToRightMessageLane,
					Left,
				>(
					left_client,
					TransactionParams {
						signer: left_messages_pallet_owner.clone(),
						mortality: left_transactions_mortality,
					},
					left_to_right_metrics
						.target_to_source_conversion_rate
						.as_ref()
						.ok_or_else(format_err)?
						.shared_value_ref(),
					left_to_right_metrics
						.target_to_base_conversion_rate
						.as_ref()
						.ok_or_else(format_err)?
						.shared_value_ref(),
					left_to_right_metrics
						.source_to_base_conversion_rate
						.as_ref()
						.ok_or_else(format_err)?
						.shared_value_ref(),
					CONVERSION_RATE_ALLOWED_DIFFERENCE_RATIO,
				);
				at_left_relay_accounts.push(TaggedAccount::MessagesPalletOwner {
					id: left_messages_pallet_owner.public().into(),
					bridged_chain: Right::NAME.to_string(),
				});
			}
			if let Some(right_messages_pallet_owner) = right_messages_pallet_owner.clone() {
				let right_client = right_client.clone();
				let format_err = || {
					anyhow::format_err!(
						"Cannon run conversion rate updater: {} -> {}",
						Left::NAME,
						Right::NAME
					)
				};
				substrate_relay_helper::conversion_rate_update::run_conversion_rate_update_loop::<
					RightToLeftMessageLane,
					Right,
				>(
					right_client,
					TransactionParams {
						signer: right_messages_pallet_owner.clone(),
						mortality: right_transactions_mortality,
					},
					right_to_left_metrics
						.target_to_source_conversion_rate
						.as_ref()
						.ok_or_else(format_err)?
						.shared_value_ref(),
					right_to_left_metrics
						.target_to_base_conversion_rate
						.as_ref()
						.ok_or_else(format_err)?
						.shared_value_ref(),
					right_to_left_metrics
						.source_to_base_conversion_rate
						.as_ref()
						.ok_or_else(format_err)?
						.shared_value_ref(),
					CONVERSION_RATE_ALLOWED_DIFFERENCE_RATIO,
				);
				at_right_relay_accounts.push(TaggedAccount::MessagesPalletOwner {
					id: right_messages_pallet_owner.public().into(),
					bridged_chain: Left::NAME.to_string(),
				});
			}

			// optionally, create relayers fund account
			if params.shared.create_relayers_fund_accounts {
				let relayer_fund_acount_id = pallet_bridge_messages::relayer_fund_account_id::<
					AccountIdOf<Left>,
					LeftAccountIdConverter,
				>();
				let relayers_fund_account_balance =
					left_client.free_native_balance(relayer_fund_acount_id.clone()).await;
				if let Err(relay_substrate_client::Error::AccountDoesNotExist) =
					relayers_fund_account_balance
				{
					log::info!(target: "bridge", "Going to create relayers fund account at {}.", Left::NAME);
					left_create_account(
						left_client.clone(),
						left_sign.clone(),
						relayer_fund_acount_id,
					)
					.await?;
				}

				let relayer_fund_acount_id = pallet_bridge_messages::relayer_fund_account_id::<
					AccountIdOf<Right>,
					RightAccountIdConverter,
				>();
				let relayers_fund_account_balance =
					right_client.free_native_balance(relayer_fund_acount_id.clone()).await;
				if let Err(relay_substrate_client::Error::AccountDoesNotExist) =
					relayers_fund_account_balance
				{
					log::info!(target: "bridge", "Going to create relayers fund account at {}.", Right::NAME);
					right_create_account(
						right_client.clone(),
						right_sign.clone(),
						relayer_fund_acount_id,
					)
					.await?;
				}
			}

			// start on-demand header relays
			let (left_to_right_on_demand_headers, right_to_left_on_demand_headers) =
				start_on_demand_relays(
					&params,
					left_client.clone(),
					right_client.clone(),
					&mut at_left_relay_accounts,
					&mut at_right_relay_accounts,
				)
				.await?;

			// add balance-related metrics
			let metrics_params =
				substrate_relay_helper::messages_metrics::add_relay_balances_metrics(
					left_client.clone(),
					metrics_params,
					at_left_relay_accounts,
				)
				.await?;
			let metrics_params =
				substrate_relay_helper::messages_metrics::add_relay_balances_metrics(
					right_client.clone(),
					metrics_params,
					at_right_relay_accounts,
				)
				.await?;

			// Need 2x capacity since we consider both directions for each lane
			let mut message_relays = Vec::with_capacity(lanes.len() * 2);
			for lane in lanes {
				let lane = lane.into();
				let left_to_right_messages = substrate_relay_helper::messages_lane::run::<
					LeftToRightMessageLane,
				>(MessagesRelayParams {
					source_client: left_client.clone(),
					source_transaction_params: TransactionParams {
						signer: left_sign.clone(),
						mortality: left_transactions_mortality,
					},
					target_client: right_client.clone(),
					target_transaction_params: TransactionParams {
						signer: right_sign.clone(),
						mortality: right_transactions_mortality,
					},
					source_to_target_headers_relay: Some(left_to_right_on_demand_headers.clone()),
					target_to_source_headers_relay: Some(right_to_left_on_demand_headers.clone()),
					lane_id: lane,
					metrics_params: metrics_params.clone().disable(),
					standalone_metrics: Some(left_to_right_metrics.clone()),
					relay_strategy: relay_strategy.clone(),
				})
				.map_err(|e| anyhow::format_err!("{}", e))
				.boxed();
				let right_to_left_messages = substrate_relay_helper::messages_lane::run::<
					RightToLeftMessageLane,
				>(MessagesRelayParams {
					source_client: right_client.clone(),
					source_transaction_params: TransactionParams {
						signer: right_sign.clone(),
						mortality: right_transactions_mortality,
					},
					target_client: left_client.clone(),
					target_transaction_params: TransactionParams {
						signer: left_sign.clone(),
						mortality: left_transactions_mortality,
					},
					source_to_target_headers_relay: Some(right_to_left_on_demand_headers.clone()),
					target_to_source_headers_relay: Some(left_to_right_on_demand_headers.clone()),
					lane_id: lane,
					metrics_params: metrics_params.clone().disable(),
					standalone_metrics: Some(right_to_left_metrics.clone()),
					relay_strategy: relay_strategy.clone(),
				})
				.map_err(|e| anyhow::format_err!("{}", e))
				.boxed();

				message_relays.push(left_to_right_messages);
				message_relays.push(right_to_left_messages);
			}

			relay_utils::relay_metrics(metrics_params)
				.expose()
				.await
				.map_err(|e| anyhow::format_err!("{}", e))?;

			futures::future::select_all(message_relays).await.0
		})
	}
}

/// Start bidirectional on-demand headers <> headers relay.
#[allow(clippy::too_many_arguments)] // TODO: https://github.com/paritytech/parity-bridges-common/issues/1415
async fn start_on_demand_relay_to_relay<LC, RC, LR, RL>(
	left_client: Client<LC>,
	right_client: Client<RC>,
	left_to_right_transaction_params: TransactionParams<AccountKeyPairOf<RC>>,
	right_to_left_transaction_params: TransactionParams<AccountKeyPairOf<LC>>,
	left_to_right_only_mandatory_headers: bool,
	right_to_left_only_mandatory_headers: bool,
	left_can_start_version_guard: bool,
	right_can_start_version_guard: bool,
	at_left_relay_accounts: &mut Vec<TaggedAccount<AccountIdOf<LC>>>,
	at_right_relay_accounts: &mut Vec<TaggedAccount<AccountIdOf<RC>>>,
) -> anyhow::Result<(
	Arc<dyn OnDemandRelay<BlockNumberOf<LC>>>,
	Arc<dyn OnDemandRelay<BlockNumberOf<RC>>>,
)>
where
	LC: Chain + TransactionSignScheme<Chain = LC> + CliChain<KeyPair = AccountKeyPairOf<LC>>,
	RC: Chain + TransactionSignScheme<Chain = RC> + CliChain<KeyPair = AccountKeyPairOf<RC>>,
	LR: SubstrateFinalitySyncPipeline<
		SourceChain = LC,
		TargetChain = RC,
		TransactionSignScheme = RC,
	>,
	RL: SubstrateFinalitySyncPipeline<
		SourceChain = RC,
		TargetChain = LC,
		TransactionSignScheme = LC,
	>,
	AccountIdOf<LC>: From<<<LC as TransactionSignScheme>::AccountKeyPair as Pair>::Public>,
	AccountIdOf<RC>: From<<<RC as TransactionSignScheme>::AccountKeyPair as Pair>::Public>,
{
	at_left_relay_accounts.push(TaggedAccount::Headers {
		id: right_to_left_transaction_params.signer.public().into(),
		bridged_chain: RC::NAME.to_string(),
	});
	at_right_relay_accounts.push(TaggedAccount::Headers {
		id: left_to_right_transaction_params.signer.public().into(),
		bridged_chain: LC::NAME.to_string(),
	});

	LR::start_relay_guards(
		&right_client,
		&left_to_right_transaction_params,
		right_can_start_version_guard,
	)
	.await?;
	RL::start_relay_guards(
		&left_client,
		&right_to_left_transaction_params,
		left_can_start_version_guard,
	)
	.await?;
	let left_to_right_on_demand_headers = OnDemandHeadersRelay::new::<LR>(
		left_client.clone(),
		right_client.clone(),
		left_to_right_transaction_params,
		left_to_right_only_mandatory_headers,
	);
	let right_to_left_on_demand_headers = OnDemandHeadersRelay::new::<RL>(
		right_client.clone(),
		left_client.clone(),
		right_to_left_transaction_params,
		right_to_left_only_mandatory_headers,
	);

	Ok((Arc::new(left_to_right_on_demand_headers), Arc::new(right_to_left_on_demand_headers)))
}

/// Start bidirectional on-demand headers <> parachains relay.
#[allow(clippy::too_many_arguments)] // TODO: https://github.com/paritytech/parity-bridges-common/issues/1415
async fn start_on_demand_relay_to_parachain<LC, RC, RRC, LR, RRF, RL>(
	left_client: Client<LC>,
	right_client: Client<RC>,
	right_relay_client: Client<RRC>,
	left_headers_to_right_transaction_params: TransactionParams<AccountKeyPairOf<RC>>,
	right_headers_to_left_transaction_params: TransactionParams<AccountKeyPairOf<LC>>,
	right_parachains_to_left_transaction_params: TransactionParams<AccountKeyPairOf<LC>>,
	left_to_right_only_mandatory_headers: bool,
	right_to_left_only_mandatory_headers: bool,
	left_can_start_version_guard: bool,
	right_can_start_version_guard: bool,
	at_left_relay_accounts: &mut Vec<TaggedAccount<AccountIdOf<LC>>>,
	at_right_relay_accounts: &mut Vec<TaggedAccount<AccountIdOf<RC>>>,
) -> anyhow::Result<(
	Arc<dyn OnDemandRelay<BlockNumberOf<LC>>>,
	Arc<dyn OnDemandRelay<BlockNumberOf<RC>>>,
)>
where
	LC: Chain + TransactionSignScheme<Chain = LC> + CliChain<KeyPair = AccountKeyPairOf<LC>>,
	RC: Chain<Hash = ParaHash>
		+ TransactionSignScheme<Chain = RC>
		+ CliChain<KeyPair = AccountKeyPairOf<RC>>,
	RRC: Chain<BlockNumber = RelayBlockNumber, Hash = RelayBlockHash, Hasher = RelayBlockHasher>
		+ TransactionSignScheme<Chain = RRC>
		+ CliChain<KeyPair = AccountKeyPairOf<RRC>>,
	LR: SubstrateFinalitySyncPipeline<
		SourceChain = LC,
		TargetChain = RC,
		TransactionSignScheme = RC,
	>,
	RRF: SubstrateFinalitySyncPipeline<
		SourceChain = RRC,
		TargetChain = LC,
		TransactionSignScheme = LC,
	>,
	RL: SubstrateParachainsPipeline<
		SourceRelayChain = RRC,
		SourceParachain = RC,
		TargetChain = LC,
		TransactionSignScheme = LC,
	>,
	AccountIdOf<LC>: From<<<LC as TransactionSignScheme>::AccountKeyPair as Pair>::Public>,
	AccountIdOf<RC>: From<<<RC as TransactionSignScheme>::AccountKeyPair as Pair>::Public>,
{
	at_left_relay_accounts.push(TaggedAccount::Headers {
		id: right_headers_to_left_transaction_params.signer.public().into(),
		bridged_chain: RRC::NAME.to_string(),
	});
	at_left_relay_accounts.push(TaggedAccount::Parachains {
		id: right_parachains_to_left_transaction_params.signer.public().into(),
		bridged_chain: RRC::NAME.to_string(),
	});
	at_right_relay_accounts.push(TaggedAccount::Headers {
		id: left_headers_to_right_transaction_params.signer.public().into(),
		bridged_chain: LC::NAME.to_string(),
	});

	LR::start_relay_guards(
		&right_client,
		&left_headers_to_right_transaction_params,
		right_can_start_version_guard,
	)
	.await?;
	RRF::start_relay_guards(
		&left_client,
		&right_headers_to_left_transaction_params,
		left_can_start_version_guard,
	)
	.await?;
	let left_to_right_on_demand_headers = OnDemandHeadersRelay::new::<LR>(
		left_client.clone(),
		right_client,
		left_headers_to_right_transaction_params,
		left_to_right_only_mandatory_headers,
	);
	let right_relay_to_left_on_demand_headers = OnDemandHeadersRelay::new::<RRF>(
		right_relay_client.clone(),
		left_client.clone(),
		right_headers_to_left_transaction_params,
		right_to_left_only_mandatory_headers,
	);
	let right_to_left_on_demand_parachains = OnDemandParachainsRelay::new::<RL>(
		right_relay_client,
		left_client,
		right_parachains_to_left_transaction_params,
		Arc::new(right_relay_to_left_on_demand_headers),
	);

	Ok((Arc::new(left_to_right_on_demand_headers), Arc::new(right_to_left_on_demand_parachains)))
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
			"//Rialto.MessagesOwner",
			"--millau-transactions-mortality",
			"64",
			"--rialto-host",
			"rialto-node-alice",
			"--rialto-port",
			"9944",
			"--rialto-signer",
			"//Charlie",
			"--rialto-messages-pallet-owner",
			"//Millau.MessagesOwner",
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
					create_relayers_fund_accounts: false,
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
					millau_messages_pallet_owner: Some("//Rialto.MessagesOwner".into()),
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
					rialto_messages_pallet_owner: Some("//Millau.MessagesOwner".into()),
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
			"//RialtoParachain.MessagesOwner",
			"--millau-transactions-mortality",
			"64",
			"--rialto-parachain-host",
			"rialto-parachain-collator-charlie",
			"--rialto-parachain-port",
			"9944",
			"--rialto-parachain-signer",
			"//George",
			"--rialto-parachain-messages-pallet-owner",
			"//Millau.MessagesOwner",
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
						create_relayers_fund_accounts: false,
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
						millau_messages_pallet_owner: Some(
							"//RialtoParachain.MessagesOwner".into()
						),
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
							"//Millau.MessagesOwner".into()
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
