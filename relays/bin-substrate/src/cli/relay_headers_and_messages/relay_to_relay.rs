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

use async_trait::async_trait;
use std::sync::Arc;

use crate::cli::{
	bridge::{CliBridgeBase, MessagesCliBridge, RelayToRelayHeadersCliBridge},
	relay_headers_and_messages::{Full2WayBridgeBase, Full2WayBridgeCommonParams},
	CliChain,
};
use relay_substrate_client::{AccountIdOf, AccountKeyPairOf, ChainWithTransactions};
use sp_core::Pair;
use substrate_relay_helper::{
	finality::SubstrateFinalitySyncPipeline,
	on_demand::{headers::OnDemandHeadersRelay, OnDemandRelay},
	TaggedAccount, TransactionParams,
};

/// A base relay between two standalone (relay) chains.
///
/// Such relay starts 2 messages relay and 2 on-demand header relays.
pub struct RelayToRelayBridge<
	L2R: MessagesCliBridge + RelayToRelayHeadersCliBridge,
	R2L: MessagesCliBridge + RelayToRelayHeadersCliBridge,
> {
	/// Parameters that are shared by all bridge types.
	pub common:
		Full2WayBridgeCommonParams<<R2L as CliBridgeBase>::Target, <L2R as CliBridgeBase>::Target>,
	/// Override for right->left headers signer.
	pub right_to_left_transaction_params:
		TransactionParams<AccountKeyPairOf<<R2L as CliBridgeBase>::Target>>,
	/// Override for left->right headers signer.
	pub left_to_right_transaction_params:
		TransactionParams<AccountKeyPairOf<<L2R as CliBridgeBase>::Target>>,
}

macro_rules! declare_relay_to_relay_bridge_schema {
	($left_chain:ident, $right_chain:ident) => {
		bp_runtime::paste::item! {
			#[doc = $left_chain " and " $right_chain " headers+messages relay params."]
			#[derive(Debug, PartialEq, StructOpt)]
			pub struct [<$left_chain $right_chain HeadersAndMessages>] {
				#[structopt(flatten)]
				shared: HeadersAndMessagesSharedParams,
				// default signer, which is always used to sign messages relay transactions on the left chain
				#[structopt(flatten)]
				left: [<$left_chain ConnectionParams>],
				// override for right->left headers signer
				#[structopt(flatten)]
				right_headers_to_left_sign_override: [<$right_chain HeadersTo $left_chain SigningParams>],
				#[structopt(flatten)]
				left_sign: [<$left_chain SigningParams>],
				// default signer, which is always used to sign messages relay transactions on the right chain
				#[structopt(flatten)]
				right: [<$right_chain ConnectionParams>],
				// override for left->right headers signer
				#[structopt(flatten)]
				left_headers_to_right_sign_override: [<$left_chain HeadersTo $right_chain SigningParams>],
				#[structopt(flatten)]
				right_sign: [<$right_chain SigningParams>],
			}

			impl [<$left_chain $right_chain HeadersAndMessages>] {
				async fn into_bridge<
					Left: ChainWithTransactions + CliChain,
					Right: ChainWithTransactions + CliChain,
					L2R: CliBridgeBase<Source = Left, Target = Right> + MessagesCliBridge + RelayToRelayHeadersCliBridge,
					R2L: CliBridgeBase<Source = Right, Target = Left> + MessagesCliBridge + RelayToRelayHeadersCliBridge,
				>(
					self,
				) -> anyhow::Result<RelayToRelayBridge<L2R, R2L>> {
					Ok(RelayToRelayBridge {
						common: Full2WayBridgeCommonParams::new::<L2R>(
							self.shared,
							BridgeEndCommonParams {
								client: self.left.into_client::<Left>().await?,
								sign: self.left_sign.to_keypair::<Left>()?,
								transactions_mortality: self.left_sign.transactions_mortality()?,
								accounts: vec![],
							},
							BridgeEndCommonParams {
								client: self.right.into_client::<Right>().await?,
								sign: self.right_sign.to_keypair::<Right>()?,
								transactions_mortality: self.right_sign.transactions_mortality()?,
								accounts: vec![],
							},
						)?,
						right_to_left_transaction_params: self
							.right_headers_to_left_sign_override
							.transaction_params_or::<Left, _>(&self.left_sign)?,
						left_to_right_transaction_params: self
							.left_headers_to_right_sign_override
							.transaction_params_or::<Right, _>(&self.right_sign)?,
					})
				}
			}
		}
	};
}

#[async_trait]
impl<
		Left: ChainWithTransactions + CliChain,
		Right: ChainWithTransactions + CliChain,
		L2R: CliBridgeBase<Source = Left, Target = Right>
			+ MessagesCliBridge
			+ RelayToRelayHeadersCliBridge,
		R2L: CliBridgeBase<Source = Right, Target = Left>
			+ MessagesCliBridge
			+ RelayToRelayHeadersCliBridge,
	> Full2WayBridgeBase for RelayToRelayBridge<L2R, R2L>
where
	AccountIdOf<Left>: From<<AccountKeyPairOf<Left> as Pair>::Public>,
	AccountIdOf<Right>: From<<AccountKeyPairOf<Right> as Pair>::Public>,
{
	type Params = RelayToRelayBridge<L2R, R2L>;
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
		self.common.right.accounts.push(TaggedAccount::Headers {
			id: self.left_to_right_transaction_params.signer.public().into(),
			bridged_chain: Self::Left::NAME.to_string(),
		});
		self.common.left.accounts.push(TaggedAccount::Headers {
			id: self.right_to_left_transaction_params.signer.public().into(),
			bridged_chain: Self::Right::NAME.to_string(),
		});

		<L2R as RelayToRelayHeadersCliBridge>::Finality::start_relay_guards(
			&self.common.right.client,
			&self.left_to_right_transaction_params,
			self.common.right.client.can_start_version_guard(),
		)
		.await?;
		<R2L as RelayToRelayHeadersCliBridge>::Finality::start_relay_guards(
			&self.common.left.client,
			&self.right_to_left_transaction_params,
			self.common.left.client.can_start_version_guard(),
		)
		.await?;

		let left_to_right_on_demand_headers =
			OnDemandHeadersRelay::<<L2R as RelayToRelayHeadersCliBridge>::Finality>::new(
				self.common.left.client.clone(),
				self.common.right.client.clone(),
				self.left_to_right_transaction_params.clone(),
				self.common.shared.only_mandatory_headers,
				None,
			);
		let right_to_left_on_demand_headers =
			OnDemandHeadersRelay::<<R2L as RelayToRelayHeadersCliBridge>::Finality>::new(
				self.common.right.client.clone(),
				self.common.left.client.clone(),
				self.right_to_left_transaction_params.clone(),
				self.common.shared.only_mandatory_headers,
				None,
			);

		Ok((Arc::new(left_to_right_on_demand_headers), Arc::new(right_to_left_on_demand_headers)))
	}
}
