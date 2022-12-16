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
	bridge::{CliBridgeBase, MessagesCliBridge, ParachainToRelayHeadersCliBridge},
	relay_headers_and_messages::{Full2WayBridgeBase, Full2WayBridgeCommonParams},
	CliChain,
};
use bp_polkadot_core::parachains::ParaHash;
use pallet_bridge_parachains::{RelayBlockHash, RelayBlockHasher, RelayBlockNumber};
use relay_substrate_client::{
	AccountIdOf, AccountKeyPairOf, Chain, ChainWithTransactions, Client, Parachain,
};
use sp_core::Pair;
use substrate_relay_helper::{
	finality::SubstrateFinalitySyncPipeline,
	on_demand::{
		headers::OnDemandHeadersRelay, parachains::OnDemandParachainsRelay, OnDemandRelay,
	},
	TaggedAccount, TransactionParams,
};

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

	/// Override for right_relay->left headers signer.
	pub right_headers_to_left_transaction_params:
		TransactionParams<AccountKeyPairOf<<R2L as CliBridgeBase>::Target>>,
	/// Override for left_relay->right headers signer.
	pub left_headers_to_right_transaction_params:
		TransactionParams<AccountKeyPairOf<<L2R as CliBridgeBase>::Target>>,

	/// Override for right->left parachains signer.
	pub right_parachains_to_left_transaction_params:
		TransactionParams<AccountKeyPairOf<<R2L as CliBridgeBase>::Target>>,
	/// Override for left->right parachains signer.
	pub left_parachains_to_right_transaction_params:
		TransactionParams<AccountKeyPairOf<<L2R as CliBridgeBase>::Target>>,
}

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
				#[structopt(flatten)]
				left_relay: [<$left_chain ConnectionParams>],

				// default signer, which is always used to sign messages relay transactions on the left chain
				#[structopt(flatten)]
				left_sign: [<$left_parachain SigningParams>],
				// signer used to sign parameter update transactions at the left parachain
				#[structopt(flatten)]
				left_messages_pallet_owner: [<$left_parachain MessagesPalletOwnerSigningParams>],

				#[structopt(flatten)]
				right: [<$right_parachain ConnectionParams>],
				#[structopt(flatten)]
				right_relay: [<$right_chain ConnectionParams>],

				// default signer, which is always used to sign messages relay transactions on the right chain
				#[structopt(flatten)]
				right_sign: [<$right_parachain SigningParams>],
				// signer used to sign parameter update transactions at the right parachain
				#[structopt(flatten)]
				right_messages_pallet_owner: [<$right_parachain MessagesPalletOwnerSigningParams>],

				// override for right_relay->left-parachain headers signer
				#[structopt(flatten)]
				right_relay_headers_to_left_sign_override: [<$right_chain HeadersTo $left_parachain SigningParams>],
				// override for left_relay->right-parachain headers signer
				#[structopt(flatten)]
				left_relay_headers_to_right_sign_override: [<$left_chain HeadersTo $right_parachain SigningParams>],

				// override for right->left parachains signer
				#[structopt(flatten)]
				right_parachains_to_left_sign_override: [<$right_chain ParachainsTo $left_parachain SigningParams>],
				// override for left->right parachains signer
				#[structopt(flatten)]
				left_parachains_to_right_sign_override: [<$left_chain ParachainsTo $right_parachain SigningParams>],
			}

			impl [<$left_parachain $right_parachain HeadersAndMessages>] {
				async fn into_bridge<
					Left: ChainWithTransactions + CliChain<KeyPair = AccountKeyPairOf<Left>> + Parachain,
					LeftRelay: CliChain,
					Right: ChainWithTransactions + CliChain<KeyPair = AccountKeyPairOf<Right>> + Parachain,
					RightRelay: CliChain,
					L2R: CliBridgeBase<Source = Left, Target = Right>
						+ MessagesCliBridge
						+ ParachainToRelayHeadersCliBridge<SourceRelay = LeftRelay>,
					R2L: CliBridgeBase<Source = Right, Target = Left>
						+ MessagesCliBridge
						+ ParachainToRelayHeadersCliBridge<SourceRelay = RightRelay>,
				>(
					self,
				) -> anyhow::Result<ParachainToParachainBridge<L2R, R2L>> {
					Ok(ParachainToParachainBridge {
						common: Full2WayBridgeCommonParams::new::<L2R>(
							self.shared,
							BridgeEndCommonParams {
								client: self.left.into_client::<Left>().await?,
								sign: self.left_sign.to_keypair::<Left>()?,
								transactions_mortality: self.left_sign.transactions_mortality()?,
								messages_pallet_owner: self.left_messages_pallet_owner.to_keypair::<Left>()?,
								accounts: vec![],
							},
							BridgeEndCommonParams {
								client: self.right.into_client::<Right>().await?,
								sign: self.right_sign.to_keypair::<Right>()?,
								transactions_mortality: self.right_sign.transactions_mortality()?,
								messages_pallet_owner: self.right_messages_pallet_owner.to_keypair::<Right>()?,
								accounts: vec![],
							},
						)?,
						left_relay: self.left_relay.into_client::<LeftRelay>().await?,
						right_relay: self.right_relay.into_client::<RightRelay>().await?,
						right_headers_to_left_transaction_params: self
							.right_relay_headers_to_left_sign_override
							.transaction_params_or::<Left, _>(&self.left_sign)?,
						left_headers_to_right_transaction_params: self
							.left_relay_headers_to_right_sign_override
							.transaction_params_or::<Right, _>(&self.right_sign)?,
						right_parachains_to_left_transaction_params: self
							.right_parachains_to_left_sign_override
							.transaction_params_or::<Left, _>(&self.left_sign)?,
						left_parachains_to_right_transaction_params: self
							.left_parachains_to_right_sign_override
							.transaction_params_or::<Right, _>(&self.right_sign)?,
					})
				}
			}
		}
	};
}

#[async_trait]
impl<
		Left: Chain<Hash = ParaHash>
			+ ChainWithTransactions
			+ CliChain<KeyPair = AccountKeyPairOf<Left>>
			+ Parachain,
		Right: Chain<Hash = ParaHash>
			+ ChainWithTransactions
			+ CliChain<KeyPair = AccountKeyPairOf<Right>>
			+ Parachain,
		LeftRelay: Chain<BlockNumber = RelayBlockNumber, Hash = RelayBlockHash, Hasher = RelayBlockHasher>
			+ CliChain,
		RightRelay: Chain<BlockNumber = RelayBlockNumber, Hash = RelayBlockHash, Hasher = RelayBlockHasher>
			+ CliChain,
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
		self.common.left.accounts.push(TaggedAccount::Headers {
			id: self.right_headers_to_left_transaction_params.signer.public().into(),
			bridged_chain: RightRelay::NAME.to_string(),
		});
		self.common.left.accounts.push(TaggedAccount::Parachains {
			id: self.right_parachains_to_left_transaction_params.signer.public().into(),
			bridged_chain: RightRelay::NAME.to_string(),
		});
		self.common.right.accounts.push(TaggedAccount::Headers {
			id: self.left_headers_to_right_transaction_params.signer.public().into(),
			bridged_chain: Left::NAME.to_string(),
		});
		self.common.right.accounts.push(TaggedAccount::Parachains {
			id: self.left_parachains_to_right_transaction_params.signer.public().into(),
			bridged_chain: LeftRelay::NAME.to_string(),
		});

		<L2R as ParachainToRelayHeadersCliBridge>::RelayFinality::start_relay_guards(
			&self.common.right.client,
			&self.left_headers_to_right_transaction_params,
			self.common.right.client.can_start_version_guard(),
		)
		.await?;
		<R2L as ParachainToRelayHeadersCliBridge>::RelayFinality::start_relay_guards(
			&self.common.left.client,
			&self.right_headers_to_left_transaction_params,
			self.common.left.client.can_start_version_guard(),
		)
		.await?;

		let left_relay_to_right_on_demand_headers =
			OnDemandHeadersRelay::<<L2R as ParachainToRelayHeadersCliBridge>::RelayFinality>::new(
				self.left_relay.clone(),
				self.common.right.client.clone(),
				self.left_headers_to_right_transaction_params.clone(),
				self.common.shared.only_mandatory_headers,
			);
		let right_relay_to_left_on_demand_headers =
			OnDemandHeadersRelay::<<R2L as ParachainToRelayHeadersCliBridge>::RelayFinality>::new(
				self.right_relay.clone(),
				self.common.left.client.clone(),
				self.right_headers_to_left_transaction_params.clone(),
				self.common.shared.only_mandatory_headers,
			);

		let left_to_right_on_demand_parachains = OnDemandParachainsRelay::<
			<L2R as ParachainToRelayHeadersCliBridge>::ParachainFinality,
		>::new(
			self.left_relay.clone(),
			self.common.right.client.clone(),
			self.left_parachains_to_right_transaction_params.clone(),
			Arc::new(left_relay_to_right_on_demand_headers),
		);
		let right_to_left_on_demand_parachains = OnDemandParachainsRelay::<
			<R2L as ParachainToRelayHeadersCliBridge>::ParachainFinality,
		>::new(
			self.right_relay.clone(),
			self.common.left.client.clone(),
			self.right_parachains_to_left_transaction_params.clone(),
			Arc::new(right_relay_to_left_on_demand_headers),
		);

		Ok((
			Arc::new(left_to_right_on_demand_parachains),
			Arc::new(right_to_left_on_demand_parachains),
		))
	}
}
