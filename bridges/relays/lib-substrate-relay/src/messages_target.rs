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

//! Substrate client as Substrate messages target. The chain we connect to should have
//! runtime that implements `<BridgedChainName>HeaderApi` to allow bridging with
//! `<BridgedName>` chain.

use crate::{
	messages_lane::{
		BatchProofTransaction, MessageLaneAdapter, ReceiveMessagesProofCallBuilder,
		SubstrateMessageLane,
	},
	messages_source::{
		ensure_messages_pallet_active, read_client_state_from_both_chains, SubstrateMessagesProof,
	},
	on_demand::OnDemandRelay,
	TransactionParams,
};

use async_std::sync::Arc;
use async_trait::async_trait;
use bp_messages::{
	storage_keys::inbound_lane_data_key, ChainWithMessages as _, InboundLaneData, LaneId,
	MessageNonce, UnrewardedRelayersState,
};
use bridge_runtime_common::messages::source::FromBridgedChainMessagesDeliveryProof;
use messages_relay::{
	message_lane::{MessageLane, SourceHeaderIdOf, TargetHeaderIdOf},
	message_lane_loop::{NoncesSubmitArtifacts, TargetClient, TargetClientState},
};
use relay_substrate_client::{
	AccountIdOf, AccountKeyPairOf, BalanceOf, CallOf, Chain, Client, Error as SubstrateError,
	HashOf, TransactionEra, TransactionTracker, UnsignedTransaction,
};
use relay_utils::relay_loop::Client as RelayClient;
use sp_core::Pair;
use std::ops::RangeInclusive;

/// Message receiving proof returned by the target Substrate node.
pub type SubstrateMessagesDeliveryProof<C> =
	(UnrewardedRelayersState, FromBridgedChainMessagesDeliveryProof<HashOf<C>>);

/// Substrate client as Substrate messages target.
pub struct SubstrateMessagesTarget<P: SubstrateMessageLane, SourceClnt, TargetClnt> {
	target_client: TargetClnt,
	source_client: SourceClnt,
	lane_id: LaneId,
	relayer_id_at_source: AccountIdOf<P::SourceChain>,
	transaction_params: Option<TransactionParams<AccountKeyPairOf<P::TargetChain>>>,
	source_to_target_headers_relay: Option<Arc<dyn OnDemandRelay<P::SourceChain, P::TargetChain>>>,
}

impl<P, SourceClnt, TargetClnt> SubstrateMessagesTarget<P, SourceClnt, TargetClnt>
where
	P: SubstrateMessageLane,
	TargetClnt: Client<P::TargetChain>,
{
	/// Create new Substrate headers target.
	pub fn new(
		target_client: TargetClnt,
		source_client: SourceClnt,
		lane_id: LaneId,
		relayer_id_at_source: AccountIdOf<P::SourceChain>,
		transaction_params: Option<TransactionParams<AccountKeyPairOf<P::TargetChain>>>,
		source_to_target_headers_relay: Option<
			Arc<dyn OnDemandRelay<P::SourceChain, P::TargetChain>>,
		>,
	) -> Self {
		SubstrateMessagesTarget {
			target_client,
			source_client,
			lane_id,
			relayer_id_at_source,
			transaction_params,
			source_to_target_headers_relay,
		}
	}

	/// Read inbound lane state from the on-chain storage at given block.
	async fn inbound_lane_data(
		&self,
		id: TargetHeaderIdOf<MessageLaneAdapter<P>>,
	) -> Result<Option<InboundLaneData<AccountIdOf<P::SourceChain>>>, SubstrateError> {
		self.target_client
			.storage_value(
				id.hash(),
				inbound_lane_data_key(
					P::SourceChain::WITH_CHAIN_MESSAGES_PALLET_NAME,
					&self.lane_id,
				),
			)
			.await
	}

	/// Ensure that the messages pallet at target chain is active.
	async fn ensure_pallet_active(&self) -> Result<(), SubstrateError> {
		ensure_messages_pallet_active::<P::TargetChain, P::SourceChain, _>(&self.target_client)
			.await
	}
}

impl<P: SubstrateMessageLane, SourceClnt: Clone, TargetClnt: Clone> Clone
	for SubstrateMessagesTarget<P, SourceClnt, TargetClnt>
{
	fn clone(&self) -> Self {
		Self {
			target_client: self.target_client.clone(),
			source_client: self.source_client.clone(),
			lane_id: self.lane_id,
			relayer_id_at_source: self.relayer_id_at_source.clone(),
			transaction_params: self.transaction_params.clone(),
			source_to_target_headers_relay: self.source_to_target_headers_relay.clone(),
		}
	}
}

#[async_trait]
impl<
		P: SubstrateMessageLane,
		SourceClnt: Client<P::SourceChain>,
		TargetClnt: Client<P::TargetChain>,
	> RelayClient for SubstrateMessagesTarget<P, SourceClnt, TargetClnt>
{
	type Error = SubstrateError;

	async fn reconnect(&mut self) -> Result<(), SubstrateError> {
		// since the client calls RPC methods on both sides, we need to reconnect both
		self.target_client.reconnect().await?;
		self.source_client.reconnect().await?;

		// call reconnect on on-demand headers relay, because we may use different chains there
		// and the error that has lead to reconnect may have came from those other chains
		// (see `require_source_header_on_target`)
		//
		// this may lead to multiple reconnects to the same node during the same call and it
		// needs to be addressed in the future
		// TODO: https://github.com/paritytech/parity-bridges-common/issues/1928
		if let Some(ref mut source_to_target_headers_relay) = self.source_to_target_headers_relay {
			source_to_target_headers_relay.reconnect().await?;
		}

		Ok(())
	}
}

#[async_trait]
impl<
		P: SubstrateMessageLane,
		SourceClnt: Client<P::SourceChain>,
		TargetClnt: Client<P::TargetChain>,
	> TargetClient<MessageLaneAdapter<P>> for SubstrateMessagesTarget<P, SourceClnt, TargetClnt>
where
	AccountIdOf<P::TargetChain>: From<<AccountKeyPairOf<P::TargetChain> as Pair>::Public>,
	BalanceOf<P::SourceChain>: TryFrom<BalanceOf<P::TargetChain>>,
{
	type BatchTransaction =
		BatchProofTransaction<P::TargetChain, P::SourceChain, P::TargetBatchCallBuilder>;
	type TransactionTracker = TransactionTracker<P::TargetChain, TargetClnt>;

	async fn state(&self) -> Result<TargetClientState<MessageLaneAdapter<P>>, SubstrateError> {
		// we can't continue to deliver confirmations if source node is out of sync, because
		// it may have already received confirmations that we're going to deliver
		//
		// we can't continue to deliver messages if target node is out of sync, because
		// it may have already received (some of) messages that we're going to deliver
		self.source_client.ensure_synced().await?;
		self.target_client.ensure_synced().await?;
		// we can't relay messages if messages pallet at target chain is halted
		self.ensure_pallet_active().await?;

		read_client_state_from_both_chains(&self.target_client, &self.source_client).await
	}

	async fn latest_received_nonce(
		&self,
		id: TargetHeaderIdOf<MessageLaneAdapter<P>>,
	) -> Result<(TargetHeaderIdOf<MessageLaneAdapter<P>>, MessageNonce), SubstrateError> {
		// lane data missing from the storage is fine until first message is received
		let latest_received_nonce = self
			.inbound_lane_data(id)
			.await?
			.map(|data| data.last_delivered_nonce())
			.unwrap_or(0);
		Ok((id, latest_received_nonce))
	}

	async fn latest_confirmed_received_nonce(
		&self,
		id: TargetHeaderIdOf<MessageLaneAdapter<P>>,
	) -> Result<(TargetHeaderIdOf<MessageLaneAdapter<P>>, MessageNonce), SubstrateError> {
		// lane data missing from the storage is fine until first message is received
		let last_confirmed_nonce = self
			.inbound_lane_data(id)
			.await?
			.map(|data| data.last_confirmed_nonce)
			.unwrap_or(0);
		Ok((id, last_confirmed_nonce))
	}

	async fn unrewarded_relayers_state(
		&self,
		id: TargetHeaderIdOf<MessageLaneAdapter<P>>,
	) -> Result<(TargetHeaderIdOf<MessageLaneAdapter<P>>, UnrewardedRelayersState), SubstrateError>
	{
		let inbound_lane_data =
			self.inbound_lane_data(id).await?.unwrap_or(InboundLaneData::default());
		Ok((id, (&inbound_lane_data).into()))
	}

	async fn prove_messages_receiving(
		&self,
		id: TargetHeaderIdOf<MessageLaneAdapter<P>>,
	) -> Result<
		(
			TargetHeaderIdOf<MessageLaneAdapter<P>>,
			<MessageLaneAdapter<P> as MessageLane>::MessagesReceivingProof,
		),
		SubstrateError,
	> {
		let (id, relayers_state) = self.unrewarded_relayers_state(id).await?;
		let inbound_data_key = bp_messages::storage_keys::inbound_lane_data_key(
			P::SourceChain::WITH_CHAIN_MESSAGES_PALLET_NAME,
			&self.lane_id,
		);
		let proof = self
			.target_client
			.prove_storage(id.hash(), vec![inbound_data_key])
			.await?
			.into_iter_nodes()
			.collect();
		let proof = FromBridgedChainMessagesDeliveryProof {
			bridged_header_hash: id.1,
			storage_proof: proof,
			lane: self.lane_id,
		};
		Ok((id, (relayers_state, proof)))
	}

	async fn submit_messages_proof(
		&self,
		maybe_batch_tx: Option<Self::BatchTransaction>,
		_generated_at_header: SourceHeaderIdOf<MessageLaneAdapter<P>>,
		nonces: RangeInclusive<MessageNonce>,
		proof: <MessageLaneAdapter<P> as MessageLane>::MessagesProof,
	) -> Result<NoncesSubmitArtifacts<Self::TransactionTracker>, SubstrateError> {
		let messages_proof_call = make_messages_delivery_call::<P>(
			self.relayer_id_at_source.clone(),
			proof.1.nonces_start..=proof.1.nonces_end,
			proof,
			maybe_batch_tx.is_none(),
		);
		let final_call = match maybe_batch_tx {
			Some(batch_tx) => batch_tx.append_call_and_build(messages_proof_call),
			None => messages_proof_call,
		};

		let transaction_params = self.transaction_params.clone().map(Ok).unwrap_or_else(|| {
			// this error shall never happen in practice, so it not deserves
			// a separate error variant
			Err(SubstrateError::Custom(format!(
				"Cannot sign transaction of {} chain",
				P::TargetChain::NAME,
			)))
		})?;
		let tx_tracker = self
			.target_client
			.submit_and_watch_signed_extrinsic(
				&transaction_params.signer,
				move |best_block_id, transaction_nonce| {
					Ok(UnsignedTransaction::new(final_call.into(), transaction_nonce)
						.era(TransactionEra::new(best_block_id, transaction_params.mortality)))
				},
			)
			.await?;
		Ok(NoncesSubmitArtifacts { nonces, tx_tracker })
	}

	async fn require_source_header_on_target(
		&self,
		id: SourceHeaderIdOf<MessageLaneAdapter<P>>,
	) -> Result<Option<Self::BatchTransaction>, SubstrateError> {
		if let Some(ref source_to_target_headers_relay) = self.source_to_target_headers_relay {
			if let Some(batch_tx) =
				BatchProofTransaction::new(source_to_target_headers_relay.clone(), id.0).await?
			{
				return Ok(Some(batch_tx))
			}

			source_to_target_headers_relay.require_more_headers(id.0).await;
		}

		Ok(None)
	}
}

/// Make messages delivery call from given proof.
fn make_messages_delivery_call<P: SubstrateMessageLane>(
	relayer_id_at_source: AccountIdOf<P::SourceChain>,
	nonces: RangeInclusive<MessageNonce>,
	proof: SubstrateMessagesProof<P::SourceChain>,
	trace_call: bool,
) -> CallOf<P::TargetChain> {
	let messages_count = nonces.end() - nonces.start() + 1;
	let dispatch_weight = proof.0;
	P::ReceiveMessagesProofCallBuilder::build_receive_messages_proof_call(
		relayer_id_at_source,
		proof,
		messages_count as _,
		dispatch_weight,
		trace_call,
	)
}
