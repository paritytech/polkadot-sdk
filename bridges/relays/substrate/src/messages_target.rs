// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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
//! <BridgedName> chain.

use crate::messages_source::read_client_state;

use async_trait::async_trait;
use bp_message_lane::{LaneId, MessageNonce};
use bp_runtime::InstanceId;
use codec::{Decode, Encode};
use messages_relay::{
	message_lane::{MessageLane, SourceHeaderIdOf, TargetHeaderIdOf},
	message_lane_loop::{TargetClient, TargetClientState},
};
use relay_substrate_client::{Chain, Client, Error as SubstrateError, HashOf};
use sp_core::Bytes;
use sp_runtime::{traits::Header as HeaderT, DeserializeOwned};
use sp_trie::StorageProof;
use std::{marker::PhantomData, ops::RangeInclusive};

/// Substrate client as Substrate messages target.
pub struct SubstrateMessagesTarget<C: Chain, P, M> {
	client: Client<C>,
	tx_maker: M,
	lane: LaneId,
	instance: InstanceId,
	_marker: PhantomData<P>,
}

/// Substrate transactions maker.
#[async_trait]
pub trait SubstrateTransactionMaker<C: Chain, P: MessageLane>: Clone + Send + Sync {
	/// Signed transaction type.
	type SignedTransaction: Send + Sync + Encode;

	/// Make messages delivery transaction.
	async fn make_messages_delivery_transaction(
		&self,
		generated_at_header: SourceHeaderIdOf<P>,
		nonces: RangeInclusive<P::MessageNonce>,
		proof: P::MessagesProof,
	) -> Result<Self::SignedTransaction, SubstrateError>;
}

impl<C: Chain, P, M> SubstrateMessagesTarget<C, P, M> {
	/// Create new Substrate headers target.
	pub fn new(client: Client<C>, tx_maker: M, lane: LaneId, instance: InstanceId) -> Self {
		SubstrateMessagesTarget {
			client,
			tx_maker,
			lane,
			instance,
			_marker: Default::default(),
		}
	}
}

impl<C: Chain, P, M: Clone> Clone for SubstrateMessagesTarget<C, P, M> {
	fn clone(&self) -> Self {
		Self {
			client: self.client.clone(),
			tx_maker: self.tx_maker.clone(),
			lane: self.lane,
			instance: self.instance,
			_marker: Default::default(),
		}
	}
}

#[async_trait]
impl<C, P, M> TargetClient<P> for SubstrateMessagesTarget<C, P, M>
where
	C: Chain,
	C::Header: DeserializeOwned,
	C::Index: DeserializeOwned,
	<C::Header as HeaderT>::Number: Into<u64>,
	P: MessageLane<
		MessageNonce = MessageNonce,
		MessagesReceivingProof = (HashOf<C>, StorageProof, LaneId),
		TargetHeaderNumber = <C::Header as HeaderT>::Number,
		TargetHeaderHash = <C::Header as HeaderT>::Hash,
	>,
	P::SourceHeaderNumber: Decode,
	P::SourceHeaderHash: Decode,
	M: SubstrateTransactionMaker<C, P>,
{
	type Error = SubstrateError;

	async fn reconnect(mut self) -> Result<Self, Self::Error> {
		let new_client = self.client.clone().reconnect().await?;
		self.client = new_client;
		Ok(self)
	}

	async fn state(&self) -> Result<TargetClientState<P>, Self::Error> {
		read_client_state::<_, P::SourceHeaderHash, P::SourceHeaderNumber>(&self.client, P::SOURCE_NAME).await
	}

	async fn latest_received_nonce(
		&self,
		id: TargetHeaderIdOf<P>,
	) -> Result<(TargetHeaderIdOf<P>, P::MessageNonce), Self::Error> {
		let encoded_response = self
			.client
			.state_call(
				// TODO: https://github.com/paritytech/parity-bridges-common/issues/457
				"InboundLaneApi_latest_received_nonce".into(),
				Bytes(self.lane.encode()),
				Some(id.1),
			)
			.await?;
		let latest_received_nonce: P::MessageNonce =
			Decode::decode(&mut &encoded_response.0[..]).map_err(SubstrateError::ResponseParseFailed)?;
		Ok((id, latest_received_nonce))
	}

	async fn latest_confirmed_received_nonce(
		&self,
		id: TargetHeaderIdOf<P>,
	) -> Result<(TargetHeaderIdOf<P>, P::MessageNonce), Self::Error> {
		let encoded_response = self
			.client
			.state_call(
				// TODO: https://github.com/paritytech/parity-bridges-common/issues/457
				"OutboundLaneApi_latest_received_nonce".into(),
				Bytes(self.lane.encode()),
				Some(id.1),
			)
			.await?;
		let latest_received_nonce: P::MessageNonce =
			Decode::decode(&mut &encoded_response.0[..]).map_err(SubstrateError::ResponseParseFailed)?;
		Ok((id, latest_received_nonce))
	}

	async fn prove_messages_receiving(
		&self,
		id: TargetHeaderIdOf<P>,
	) -> Result<(TargetHeaderIdOf<P>, P::MessagesReceivingProof), Self::Error> {
		let proof = self
			.client
			.prove_messages_delivery(self.instance, self.lane, id.1)
			.await?;
		let proof = (id.1, proof, self.lane);
		Ok((id, proof))
	}

	async fn submit_messages_proof(
		&self,
		generated_at_header: SourceHeaderIdOf<P>,
		nonces: RangeInclusive<P::MessageNonce>,
		proof: P::MessagesProof,
	) -> Result<RangeInclusive<P::MessageNonce>, Self::Error> {
		let tx = self
			.tx_maker
			.make_messages_delivery_transaction(generated_at_header, nonces.clone(), proof)
			.await?;
		self.client.submit_extrinsic(Bytes(tx.encode())).await?;
		Ok(nonces)
	}
}
