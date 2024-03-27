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

//! Parachain to parachain relayer CLI primitives.

use async_trait::async_trait;
use std::sync::Arc;

use crate::{
	cli::{
		bridge::{CliBridgeBase, MessagesCliBridge, ParachainToRelayHeadersCliBridge},
		relay_headers_and_messages::{Full2WayBridgeBase, Full2WayBridgeCommonParams},
	},
	finality::SubstrateFinalitySyncPipeline,
	on_demand::{
		headers::OnDemandHeadersRelay, parachains::OnDemandParachainsRelay, OnDemandRelay,
	},
};
use bp_polkadot_core::parachains::ParaHash;
use pallet_bridge_parachains::{RelayBlockHash, RelayBlockHasher, RelayBlockNumber};
use relay_substrate_client::{
	AccountIdOf, AccountKeyPairOf, Chain, ChainWithRuntimeVersion, ChainWithTransactions, Client,
	Parachain,
};
use sp_core::Pair;

/// A base relay between two parachain from different consensus systems.
///
/// Such relay starts 2 messages relay. It also starts 2 on-demand header relays and 2 on-demand
/// parachain heads relay.
pub struct ParachainToParachainBridge<
	L2R: MessagesCliBridge + ParachainToRelayHeadersCliBridge,
	R2L: MessagesCliBridge + ParachainToRelayHeadersCliBridge,
> where
	<L2R as CliBridgeBase>::Source: Parachain,
	<R2L as CliBridgeBase>::Source: Parachain,
{
	/// Parameters that are shared by all bridge types.
	pub common:
		Full2WayBridgeCommonParams<<R2L as CliBridgeBase>::Target, <L2R as CliBridgeBase>::Target>,
	/// Client of the left relay chain.
	pub left_relay: Client<<L2R as ParachainToRelayHeadersCliBridge>::SourceRelay>,
	/// Client of the right relay chain.
	pub right_relay: Client<<R2L as ParachainToRelayHeadersCliBridge>::SourceRelay>,
}

/// Create set of configuration objects specific to parachain-to-parachain relayer.
#[macro_export]
macro_rules! declare_parachain_to_parachain_bridge_schema {
	// left-parachain, relay-chain-of-left-parachain, right-parachain, relay-chain-of-right-parachain
	($left_parachain:ident, $left_chain:ident, $right_parachain:ident, $right_chain:ident) => {
		bp_runtime::paste::item! {
			#[doc = $left_parachain ", " $left_chain ", " $right_parachain " and " $right_chain " headers+parachains+messages relay params."]
			#[derive(Debug, PartialEq, StructOpt)]
			pub struct [<$left_parachain $right_parachain HeadersAndMessages>] {
				// shared parameters
				#[structopt(flatten)]
				shared: HeadersAndMessagesSharedParams,

				#[structopt(flatten)]
				left: [<$left_parachain ConnectionParams>],
				// default signer, which is always used to sign messages relay transactions on the left chain
				#[structopt(flatten)]
				left_sign: [<$left_parachain SigningParams>],

				#[structopt(flatten)]
				left_relay: [<$left_chain ConnectionParams>],

				#[structopt(flatten)]
				right: [<$right_parachain ConnectionParams>],
				// default signer, which is always used to sign messages relay transactions on the right chain
				#[structopt(flatten)]
				right_sign: [<$right_parachain SigningParams>],

				#[structopt(flatten)]
				right_relay: [<$right_chain ConnectionParams>],
			}

			impl [<$left_parachain $right_parachain HeadersAndMessages>] {
				async fn into_bridge<
					Left: ChainWithTransactions + ChainWithRuntimeVersion + Parachain,
					LeftRelay: ChainWithRuntimeVersion,
					Right: ChainWithTransactions + ChainWithRuntimeVersion + Parachain,
					RightRelay: ChainWithRuntimeVersion,
					L2R: $crate::cli::bridge::CliBridgeBase<Source = Left, Target = Right>
						+ MessagesCliBridge
						+ $crate::cli::bridge::ParachainToRelayHeadersCliBridge<SourceRelay = LeftRelay>,
					R2L: $crate::cli::bridge::CliBridgeBase<Source = Right, Target = Left>
						+ MessagesCliBridge
						+ $crate::cli::bridge::ParachainToRelayHeadersCliBridge<SourceRelay = RightRelay>,
				>(
					self,
				) -> anyhow::Result<$crate::cli::relay_headers_and_messages::parachain_to_parachain::ParachainToParachainBridge<L2R, R2L>> {
					Ok($crate::cli::relay_headers_and_messages::parachain_to_parachain::ParachainToParachainBridge {
						common: Full2WayBridgeCommonParams::new::<L2R>(
							self.shared,
							BridgeEndCommonParams {
								client: self.left.into_client::<Left>().await?,
								tx_params: self.left_sign.transaction_params::<Left>()?,
								accounts: vec![],
							},
							BridgeEndCommonParams {
								client: self.right.into_client::<Right>().await?,
								tx_params: self.right_sign.transaction_params::<Right>()?,
								accounts: vec![],
							},
						)?,
						left_relay: self.left_relay.into_client::<LeftRelay>().await?,
						right_relay: self.right_relay.into_client::<RightRelay>().await?,
					})
				}
			}
		}
	};
}

#[async_trait]
impl<
		Left: Chain<Hash = ParaHash> + ChainWithTransactions + ChainWithRuntimeVersion + Parachain,
		Right: Chain<Hash = ParaHash> + ChainWithTransactions + ChainWithRuntimeVersion + Parachain,
		LeftRelay: Chain<BlockNumber = RelayBlockNumber, Hash = RelayBlockHash, Hasher = RelayBlockHasher>
			+ ChainWithRuntimeVersion,
		RightRelay: Chain<BlockNumber = RelayBlockNumber, Hash = RelayBlockHash, Hasher = RelayBlockHasher>
			+ ChainWithRuntimeVersion,
		L2R: CliBridgeBase<Source = Left, Target = Right>
			+ MessagesCliBridge
			+ ParachainToRelayHeadersCliBridge<SourceRelay = LeftRelay>,
		R2L: CliBridgeBase<Source = Right, Target = Left>
			+ MessagesCliBridge
			+ ParachainToRelayHeadersCliBridge<SourceRelay = RightRelay>,
	> Full2WayBridgeBase for ParachainToParachainBridge<L2R, R2L>
where
	AccountIdOf<Left>: From<<AccountKeyPairOf<Left> as Pair>::Public>,
	AccountIdOf<Right>: From<<AccountKeyPairOf<Right> as Pair>::Public>,
{
	type Params = ParachainToParachainBridge<L2R, R2L>;
	type Left = Left;
	type Right = Right;

	fn common(&self) -> &Full2WayBridgeCommonParams<Left, Right> {
		&self.common
	}

	fn mut_common(&mut self) -> &mut Full2WayBridgeCommonParams<Self::Left, Self::Right> {
		&mut self.common
	}

	async fn start_on_demand_headers_relayers(
		&mut self,
	) -> anyhow::Result<(
		Arc<dyn OnDemandRelay<Self::Left, Self::Right>>,
		Arc<dyn OnDemandRelay<Self::Right, Self::Left>>,
	)> {
		<L2R as ParachainToRelayHeadersCliBridge>::RelayFinality::start_relay_guards(
			&self.common.right.client,
			self.common.right.client.can_start_version_guard(),
		)
		.await?;
		<R2L as ParachainToRelayHeadersCliBridge>::RelayFinality::start_relay_guards(
			&self.common.left.client,
			self.common.left.client.can_start_version_guard(),
		)
		.await?;

		let left_relay_to_right_on_demand_headers =
			OnDemandHeadersRelay::<<L2R as ParachainToRelayHeadersCliBridge>::RelayFinality>::new(
				self.left_relay.clone(),
				self.common.right.client.clone(),
				self.common.right.tx_params.clone(),
				self.common.shared.only_mandatory_headers,
				Some(self.common.metrics_params.clone()),
			);
		let right_relay_to_left_on_demand_headers =
			OnDemandHeadersRelay::<<R2L as ParachainToRelayHeadersCliBridge>::RelayFinality>::new(
				self.right_relay.clone(),
				self.common.left.client.clone(),
				self.common.left.tx_params.clone(),
				self.common.shared.only_mandatory_headers,
				Some(self.common.metrics_params.clone()),
			);

		let left_to_right_on_demand_parachains = OnDemandParachainsRelay::<
			<L2R as ParachainToRelayHeadersCliBridge>::ParachainFinality,
		>::new(
			self.left_relay.clone(),
			self.common.right.client.clone(),
			self.common.right.tx_params.clone(),
			Arc::new(left_relay_to_right_on_demand_headers),
		);
		let right_to_left_on_demand_parachains = OnDemandParachainsRelay::<
			<R2L as ParachainToRelayHeadersCliBridge>::ParachainFinality,
		>::new(
			self.right_relay.clone(),
			self.common.left.client.clone(),
			self.common.left.tx_params.clone(),
			Arc::new(right_relay_to_left_on_demand_headers),
		);

		Ok((
			Arc::new(left_to_right_on_demand_parachains),
			Arc::new(right_to_left_on_demand_parachains),
		))
	}
}
