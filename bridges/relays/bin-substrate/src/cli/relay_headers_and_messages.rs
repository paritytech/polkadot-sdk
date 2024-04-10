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

use codec::Encode;
use relay_substrate_client::{AccountIdOf, Chain, Client, TransactionSignScheme, UnsignedTransaction};
use relay_utils::metrics::MetricsParams;
use sp_core::{Bytes, Pair};
use substrate_relay_helper::messages_lane::{MessagesRelayParams, SubstrateMessageLane};
use substrate_relay_helper::on_demand_headers::OnDemandHeadersRelay;

use crate::cli::{relay_messages::RelayerMode, CliChain, HexLaneId, PrometheusParams};
use crate::declare_chain_options;

/// Maximal allowed conversion rate error ratio (abs(real - stored) / stored) that we allow.
///
/// If it is zero, then transaction will be submitted every time we see difference between
/// stored and real conversion rates. If it is large enough (e.g. > than 10 percents, which is 0.1),
/// then rational relayers may stop relaying messages because they were submitted using
/// lesser conversion rate.
const CONVERSION_RATE_ALLOWED_DIFFERENCE_RATIO: f64 = 0.05;

/// Start headers+messages relayer process.
#[derive(StructOpt)]
pub enum RelayHeadersAndMessages {
	MillauRialto(MillauRialtoHeadersAndMessages),
	RococoWococo(RococoWococoHeadersAndMessages),
	KusamaPolkadot(KusamaPolkadotHeadersAndMessages),
}

/// Parameters that have the same names across all bridges.
#[derive(StructOpt)]
pub struct HeadersAndMessagesSharedParams {
	/// Hex-encoded lane identifiers that should be served by the complex relay.
	#[structopt(long, default_value = "00000000")]
	lane: Vec<HexLaneId>,
	#[structopt(long, possible_values = RelayerMode::VARIANTS, case_insensitive = true, default_value = "rational")]
	relayer_mode: RelayerMode,
	/// Create relayers fund accounts on both chains, if it does not exists yet.
	#[structopt(long)]
	create_relayers_fund_accounts: bool,
	/// If passed, only mandatory headers (headers that are changing the GRANDPA authorities set) are relayed.
	#[structopt(long)]
	only_mandatory_headers: bool,
	#[structopt(flatten)]
	prometheus_params: PrometheusParams,
}

// The reason behind this macro is that 'normal' relays are using source and target chains terminology,
// which is unusable for both-way relays (if you're relaying headers from Rialto to Millau and from
// Millau to Rialto, then which chain is source?).
macro_rules! declare_bridge_options {
	($chain1:ident, $chain2:ident) => {
		paste::item! {
			#[doc = $chain1 " and " $chain2 " headers+messages relay params."]
			#[derive(StructOpt)]
			pub struct [<$chain1 $chain2 HeadersAndMessages>] {
				#[structopt(flatten)]
				shared: HeadersAndMessagesSharedParams,
				#[structopt(flatten)]
				left: [<$chain1 ConnectionParams>],
				#[structopt(flatten)]
				left_sign: [<$chain1 SigningParams>],
				#[structopt(flatten)]
				left_messages_pallet_owner: [<$chain1 MessagesPalletOwnerSigningParams>],
				#[structopt(flatten)]
				right: [<$chain2 ConnectionParams>],
				#[structopt(flatten)]
				right_sign: [<$chain2 SigningParams>],
				#[structopt(flatten)]
				right_messages_pallet_owner: [<$chain2 MessagesPalletOwnerSigningParams>],
			}

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

				type LeftToRightFinality = crate::chains::millau_headers_to_rialto::MillauFinalityToRialto;
				type RightToLeftFinality = crate::chains::rialto_headers_to_millau::RialtoFinalityToMillau;

				type LeftToRightMessages = crate::chains::millau_messages_to_rialto::MillauMessagesToRialto;
				type RightToLeftMessages = crate::chains::rialto_messages_to_millau::RialtoMessagesToMillau;

				type LeftAccountIdConverter = bp_millau::AccountIdConverter;
				type RightAccountIdConverter = bp_rialto::AccountIdConverter;

				const MAX_MISSING_LEFT_HEADERS_AT_RIGHT: bp_millau::BlockNumber = bp_millau::SESSION_LENGTH;
				const MAX_MISSING_RIGHT_HEADERS_AT_LEFT: bp_rialto::BlockNumber = bp_rialto::SESSION_LENGTH;

				use crate::chains::millau_messages_to_rialto::{
					add_standalone_metrics as add_left_to_right_standalone_metrics, run as left_to_right_messages,
					update_rialto_to_millau_conversion_rate as update_right_to_left_conversion_rate,
				};
				use crate::chains::rialto_messages_to_millau::{
					add_standalone_metrics as add_right_to_left_standalone_metrics, run as right_to_left_messages,
					update_millau_to_rialto_conversion_rate as update_left_to_right_conversion_rate,
				};

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
			}
			RelayHeadersAndMessages::RococoWococo(_) => {
				type Params = RococoWococoHeadersAndMessages;

				type Left = relay_rococo_client::Rococo;
				type Right = relay_wococo_client::Wococo;

				type LeftToRightFinality = crate::chains::rococo_headers_to_wococo::RococoFinalityToWococo;
				type RightToLeftFinality = crate::chains::wococo_headers_to_rococo::WococoFinalityToRococo;

				type LeftToRightMessages = crate::chains::rococo_messages_to_wococo::RococoMessagesToWococo;
				type RightToLeftMessages = crate::chains::wococo_messages_to_rococo::WococoMessagesToRococo;

				type LeftAccountIdConverter = bp_rococo::AccountIdConverter;
				type RightAccountIdConverter = bp_wococo::AccountIdConverter;

				const MAX_MISSING_LEFT_HEADERS_AT_RIGHT: bp_rococo::BlockNumber = bp_rococo::SESSION_LENGTH;
				const MAX_MISSING_RIGHT_HEADERS_AT_LEFT: bp_wococo::BlockNumber = bp_wococo::SESSION_LENGTH;

				use crate::chains::rococo_messages_to_wococo::{
					add_standalone_metrics as add_left_to_right_standalone_metrics, run as left_to_right_messages,
				};
				use crate::chains::wococo_messages_to_rococo::{
					add_standalone_metrics as add_right_to_left_standalone_metrics, run as right_to_left_messages,
				};

				async fn update_right_to_left_conversion_rate(
					_client: Client<Left>,
					_signer: <Left as TransactionSignScheme>::AccountKeyPair,
					_updated_rate: f64,
				) -> anyhow::Result<()> {
					Err(anyhow::format_err!("Conversion rate is not supported by this bridge"))
				}

				async fn update_left_to_right_conversion_rate(
					_client: Client<Right>,
					_signer: <Right as TransactionSignScheme>::AccountKeyPair,
					_updated_rate: f64,
				) -> anyhow::Result<()> {
					Err(anyhow::format_err!("Conversion rate is not supported by this bridge"))
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
			}
			RelayHeadersAndMessages::KusamaPolkadot(_) => {
				type Params = KusamaPolkadotHeadersAndMessages;

				type Left = relay_kusama_client::Kusama;
				type Right = relay_polkadot_client::Polkadot;

				type LeftToRightFinality = crate::chains::kusama_headers_to_polkadot::KusamaFinalityToPolkadot;
				type RightToLeftFinality = crate::chains::polkadot_headers_to_kusama::PolkadotFinalityToKusama;

				type LeftToRightMessages = crate::chains::kusama_messages_to_polkadot::KusamaMessagesToPolkadot;
				type RightToLeftMessages = crate::chains::polkadot_messages_to_kusama::PolkadotMessagesToKusama;

				type LeftAccountIdConverter = bp_kusama::AccountIdConverter;
				type RightAccountIdConverter = bp_polkadot::AccountIdConverter;

				const MAX_MISSING_LEFT_HEADERS_AT_RIGHT: bp_kusama::BlockNumber = bp_kusama::SESSION_LENGTH;
				const MAX_MISSING_RIGHT_HEADERS_AT_LEFT: bp_polkadot::BlockNumber = bp_polkadot::SESSION_LENGTH;

				use crate::chains::kusama_messages_to_polkadot::{
					add_standalone_metrics as add_left_to_right_standalone_metrics, run as left_to_right_messages,
					update_polkadot_to_kusama_conversion_rate as update_right_to_left_conversion_rate,
				};
				use crate::chains::polkadot_messages_to_kusama::{
					add_standalone_metrics as add_right_to_left_standalone_metrics, run as right_to_left_messages,
					update_kusama_to_polkadot_conversion_rate as update_left_to_right_conversion_rate,
				};

				async fn left_create_account(
					left_client: Client<Left>,
					left_sign: <Left as TransactionSignScheme>::AccountKeyPair,
					account_id: AccountIdOf<Left>,
				) -> anyhow::Result<()> {
					let left_genesis_hash = *left_client.genesis_hash();
					left_client
						.submit_signed_extrinsic(left_sign.public().into(), move |_, transaction_nonce| {
							Bytes(
								Left::sign_transaction(
									left_genesis_hash,
									&left_sign,
									relay_substrate_client::TransactionEra::immortal(),
									UnsignedTransaction::new(
										relay_kusama_client::runtime::Call::Balances(
											relay_kusama_client::runtime::BalancesCall::transfer(
												bp_kusama::AccountAddress::Id(account_id),
												bp_kusama::EXISTENTIAL_DEPOSIT.into(),
											),
										),
										transaction_nonce,
									),
								)
								.encode(),
							)
						})
						.await
						.map(drop)
						.map_err(|e| anyhow::format_err!("{}", e))
				}

				async fn right_create_account(
					right_client: Client<Right>,
					right_sign: <Right as TransactionSignScheme>::AccountKeyPair,
					account_id: AccountIdOf<Right>,
				) -> anyhow::Result<()> {
					let right_genesis_hash = *right_client.genesis_hash();
					right_client
						.submit_signed_extrinsic(right_sign.public().into(), move |_, transaction_nonce| {
							Bytes(
								Right::sign_transaction(
									right_genesis_hash,
									&right_sign,
									relay_substrate_client::TransactionEra::immortal(),
									UnsignedTransaction::new(
										relay_polkadot_client::runtime::Call::Balances(
											relay_polkadot_client::runtime::BalancesCall::transfer(
												bp_polkadot::AccountAddress::Id(account_id),
												bp_polkadot::EXISTENTIAL_DEPOSIT.into(),
											),
										),
										transaction_nonce,
									),
								)
								.encode(),
							)
						})
						.await
						.map(drop)
						.map_err(|e| anyhow::format_err!("{}", e))
				}

				$generic
			}
		}
	};
}

// All supported chains.
declare_chain_options!(Millau, millau);
declare_chain_options!(Rialto, rialto);
declare_chain_options!(Rococo, rococo);
declare_chain_options!(Wococo, wococo);
declare_chain_options!(Kusama, kusama);
declare_chain_options!(Polkadot, polkadot);
// All supported bridges.
declare_bridge_options!(Millau, Rialto);
declare_bridge_options!(Rococo, Wococo);
declare_bridge_options!(Kusama, Polkadot);

impl RelayHeadersAndMessages {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		select_bridge!(self, {
			let params: Params = self.into();

			let left_client = params.left.to_client::<Left>().await?;
			let left_transactions_mortality = params.left_sign.transactions_mortality()?;
			let left_sign = params.left_sign.to_keypair::<Left>()?;
			let left_messages_pallet_owner = params.left_messages_pallet_owner.to_keypair::<Left>()?;
			let right_client = params.right.to_client::<Right>().await?;
			let right_transactions_mortality = params.right_sign.transactions_mortality()?;
			let right_sign = params.right_sign.to_keypair::<Right>()?;
			let right_messages_pallet_owner = params.right_messages_pallet_owner.to_keypair::<Right>()?;

			let lanes = params.shared.lane;
			let relayer_mode = params.shared.relayer_mode.into();

			const METRIC_IS_SOME_PROOF: &str = "it is `None` when metric has been already registered; \
				this is the command entrypoint, so nothing has been registered yet; \
				qed";

			let metrics_params: MetricsParams = params.shared.prometheus_params.into();
			let metrics_params = relay_utils::relay_metrics(None, metrics_params).into_params();
			let (metrics_params, left_to_right_metrics) =
				add_left_to_right_standalone_metrics(None, metrics_params, left_client.clone())?;
			let (metrics_params, right_to_left_metrics) =
				add_right_to_left_standalone_metrics(None, metrics_params, right_client.clone())?;
			if let Some(left_messages_pallet_owner) = left_messages_pallet_owner {
				let left_client = left_client.clone();
				substrate_relay_helper::conversion_rate_update::run_conversion_rate_update_loop(
					left_to_right_metrics
						.target_to_source_conversion_rate
						.expect(METRIC_IS_SOME_PROOF),
					left_to_right_metrics
						.target_to_base_conversion_rate
						.clone()
						.expect(METRIC_IS_SOME_PROOF),
					left_to_right_metrics
						.source_to_base_conversion_rate
						.clone()
						.expect(METRIC_IS_SOME_PROOF),
					CONVERSION_RATE_ALLOWED_DIFFERENCE_RATIO,
					move |new_rate| {
						log::info!(
							target: "bridge",
							"Going to update {} -> {} (on {}) conversion rate to {}.",
							Right::NAME,
							Left::NAME,
							Left::NAME,
							new_rate,
						);
						update_right_to_left_conversion_rate(
							left_client.clone(),
							left_messages_pallet_owner.clone(),
							new_rate,
						)
					},
				);
			}
			if let Some(right_messages_pallet_owner) = right_messages_pallet_owner {
				let right_client = right_client.clone();
				substrate_relay_helper::conversion_rate_update::run_conversion_rate_update_loop(
					right_to_left_metrics
						.target_to_source_conversion_rate
						.expect(METRIC_IS_SOME_PROOF),
					left_to_right_metrics
						.source_to_base_conversion_rate
						.expect(METRIC_IS_SOME_PROOF),
					left_to_right_metrics
						.target_to_base_conversion_rate
						.expect(METRIC_IS_SOME_PROOF),
					CONVERSION_RATE_ALLOWED_DIFFERENCE_RATIO,
					move |new_rate| {
						log::info!(
							target: "bridge",
							"Going to update {} -> {} (on {}) conversion rate to {}.",
							Left::NAME,
							Right::NAME,
							Right::NAME,
							new_rate,
						);
						update_left_to_right_conversion_rate(
							right_client.clone(),
							right_messages_pallet_owner.clone(),
							new_rate,
						)
					},
				);
			}

			if params.shared.create_relayers_fund_accounts {
				let relayer_fund_acount_id =
					pallet_bridge_messages::relayer_fund_account_id::<AccountIdOf<Left>, LeftAccountIdConverter>();
				let relayers_fund_account_balance =
					left_client.free_native_balance(relayer_fund_acount_id.clone()).await;
				if let Err(relay_substrate_client::Error::AccountDoesNotExist) = relayers_fund_account_balance {
					log::info!(target: "bridge", "Going to create relayers fund account at {}.", Left::NAME);
					left_create_account(left_client.clone(), left_sign.clone(), relayer_fund_acount_id).await?;
				}

				let relayer_fund_acount_id =
					pallet_bridge_messages::relayer_fund_account_id::<AccountIdOf<Right>, RightAccountIdConverter>();
				let relayers_fund_account_balance =
					right_client.free_native_balance(relayer_fund_acount_id.clone()).await;
				if let Err(relay_substrate_client::Error::AccountDoesNotExist) = relayers_fund_account_balance {
					log::info!(target: "bridge", "Going to create relayers fund account at {}.", Right::NAME);
					right_create_account(right_client.clone(), right_sign.clone(), relayer_fund_acount_id).await?;
				}
			}

			let left_to_right_on_demand_headers = OnDemandHeadersRelay::new(
				left_client.clone(),
				right_client.clone(),
				right_transactions_mortality,
				LeftToRightFinality::new(right_client.clone(), right_sign.clone()),
				MAX_MISSING_LEFT_HEADERS_AT_RIGHT,
				params.shared.only_mandatory_headers,
			);
			let right_to_left_on_demand_headers = OnDemandHeadersRelay::new(
				right_client.clone(),
				left_client.clone(),
				left_transactions_mortality,
				RightToLeftFinality::new(left_client.clone(), left_sign.clone()),
				MAX_MISSING_RIGHT_HEADERS_AT_LEFT,
				params.shared.only_mandatory_headers,
			);

			// Need 2x capacity since we consider both directions for each lane
			let mut message_relays = Vec::with_capacity(lanes.len() * 2);
			for lane in lanes {
				let lane = lane.into();
				let left_to_right_messages = left_to_right_messages(MessagesRelayParams {
					source_client: left_client.clone(),
					source_sign: left_sign.clone(),
					target_client: right_client.clone(),
					target_sign: right_sign.clone(),
					source_to_target_headers_relay: Some(left_to_right_on_demand_headers.clone()),
					target_to_source_headers_relay: Some(right_to_left_on_demand_headers.clone()),
					lane_id: lane,
					relayer_mode,
					metrics_params: metrics_params.clone().disable().metrics_prefix(
						messages_relay::message_lane_loop::metrics_prefix::<
							<LeftToRightMessages as SubstrateMessageLane>::MessageLane,
						>(&lane),
					),
				})
				.map_err(|e| anyhow::format_err!("{}", e))
				.boxed();
				let right_to_left_messages = right_to_left_messages(MessagesRelayParams {
					source_client: right_client.clone(),
					source_sign: right_sign.clone(),
					target_client: left_client.clone(),
					target_sign: left_sign.clone(),
					source_to_target_headers_relay: Some(right_to_left_on_demand_headers.clone()),
					target_to_source_headers_relay: Some(left_to_right_on_demand_headers.clone()),
					lane_id: lane,
					relayer_mode,
					metrics_params: metrics_params.clone().disable().metrics_prefix(
						messages_relay::message_lane_loop::metrics_prefix::<
							<RightToLeftMessages as SubstrateMessageLane>::MessageLane,
						>(&lane),
					),
				})
				.map_err(|e| anyhow::format_err!("{}", e))
				.boxed();

				message_relays.push(left_to_right_messages);
				message_relays.push(right_to_left_messages);
			}

			relay_utils::relay_metrics(None, metrics_params)
				.expose()
				.await
				.map_err(|e| anyhow::format_err!("{}", e))?;

			futures::future::select_all(message_relays).await.0
		})
	}
}
