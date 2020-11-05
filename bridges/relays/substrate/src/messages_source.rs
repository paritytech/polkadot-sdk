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

//! Substrate client as Substrate messages source. The chain we connect to should have
//! runtime that implements `<BridgedChainName>HeaderApi` to allow bridging with
//! <BridgedName> chain.

use async_trait::async_trait;
use bp_message_lane::{LaneId, MessageNonce};
use bp_runtime::InstanceId;
use codec::{Decode, Encode};
use frame_support::weights::Weight;
use messages_relay::{
	message_lane::{MessageLane, SourceHeaderIdOf, TargetHeaderIdOf},
	message_lane_loop::{ClientState, SourceClient, SourceClientState},
};
use relay_substrate_client::{Chain, Client, Error as SubstrateError, HashOf, HeaderIdOf};
use relay_utils::HeaderId;
use sp_core::Bytes;
use sp_runtime::{traits::Header as HeaderT, DeserializeOwned};
use sp_trie::StorageProof;
use std::{marker::PhantomData, ops::RangeInclusive};

/// Intermediate message proof returned by the source Substrate node. Includes everything
/// required to submit to the target node: cumulative dispatch weight of bundled messages and
/// the proof itself.
pub type SubstrateMessagesProof<C> = (Weight, (HashOf<C>, StorageProof, LaneId, MessageNonce, MessageNonce));

/// Substrate client as Substrate messages source.
pub struct SubstrateMessagesSource<C: Chain, P, M> {
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

	/// Make messages receiving proof transaction.
	async fn make_messages_receiving_proof_transaction(
		&self,
		generated_at_block: TargetHeaderIdOf<P>,
		proof: P::MessagesReceivingProof,
	) -> Result<Self::SignedTransaction, SubstrateError>;
}

impl<C: Chain, P, M> SubstrateMessagesSource<C, P, M> {
	/// Create new Substrate headers source.
	pub fn new(client: Client<C>, tx_maker: M, lane: LaneId, instance: InstanceId) -> Self {
		SubstrateMessagesSource {
			client,
			tx_maker,
			lane,
			instance,
			_marker: Default::default(),
		}
	}
}

impl<C: Chain, P, M: Clone> Clone for SubstrateMessagesSource<C, P, M> {
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
impl<C, P, M> SourceClient<P> for SubstrateMessagesSource<C, P, M>
where
	C: Chain,
	C::Header: DeserializeOwned,
	C::Index: DeserializeOwned,
	<C::Header as HeaderT>::Number: Into<u64>,
	P: MessageLane<
		MessageNonce = MessageNonce,
		MessagesProof = SubstrateMessagesProof<C>,
		SourceHeaderNumber = <C::Header as HeaderT>::Number,
		SourceHeaderHash = <C::Header as HeaderT>::Hash,
	>,
	P::TargetHeaderNumber: Decode,
	P::TargetHeaderHash: Decode,
	M: SubstrateTransactionMaker<C, P>,
{
	type Error = SubstrateError;

	async fn reconnect(mut self) -> Result<Self, Self::Error> {
		let new_client = self.client.clone().reconnect().await?;
		self.client = new_client;
		Ok(self)
	}

	async fn state(&self) -> Result<SourceClientState<P>, Self::Error> {
		read_client_state::<_, P::TargetHeaderHash, P::TargetHeaderNumber>(&self.client, P::TARGET_NAME).await
	}

	async fn latest_generated_nonce(
		&self,
		id: SourceHeaderIdOf<P>,
	) -> Result<(SourceHeaderIdOf<P>, P::MessageNonce), Self::Error> {
		let encoded_response = self
			.client
			.state_call(
				// TODO: https://github.com/paritytech/parity-bridges-common/issues/457
				"OutboundLaneApi_latest_generated_nonce".into(),
				Bytes(self.lane.encode()),
				Some(id.1),
			)
			.await?;
		let latest_generated_nonce: P::MessageNonce =
			Decode::decode(&mut &encoded_response.0[..]).map_err(SubstrateError::ResponseParseFailed)?;
		Ok((id, latest_generated_nonce))
	}

	async fn latest_confirmed_received_nonce(
		&self,
		id: SourceHeaderIdOf<P>,
	) -> Result<(SourceHeaderIdOf<P>, P::MessageNonce), Self::Error> {
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

	async fn prove_messages(
		&self,
		id: SourceHeaderIdOf<P>,
		nonces: RangeInclusive<P::MessageNonce>,
		include_outbound_lane_state: bool,
	) -> Result<(SourceHeaderIdOf<P>, RangeInclusive<P::MessageNonce>, P::MessagesProof), Self::Error> {
		let (weight, proof) = self
			.client
			.prove_messages(
				self.instance,
				self.lane,
				nonces.clone(),
				include_outbound_lane_state,
				id.1,
			)
			.await?;
		let proof = (id.1, proof, self.lane, *nonces.start(), *nonces.end());
		Ok((id, nonces, (weight, proof)))
	}

	async fn submit_messages_receiving_proof(
		&self,
		generated_at_block: TargetHeaderIdOf<P>,
		proof: P::MessagesReceivingProof,
	) -> Result<(), Self::Error> {
		let tx = self
			.tx_maker
			.make_messages_receiving_proof_transaction(generated_at_block, proof)
			.await?;
		self.client.submit_extrinsic(Bytes(tx.encode())).await?;
		Ok(())
	}
}

pub async fn read_client_state<SelfChain, BridgedHeaderHash, BridgedHeaderNumber>(
	self_client: &Client<SelfChain>,
	bridged_chain_name: &str,
) -> Result<ClientState<HeaderIdOf<SelfChain>, HeaderId<BridgedHeaderHash, BridgedHeaderNumber>>, SubstrateError>
where
	SelfChain: Chain,
	SelfChain::Header: DeserializeOwned,
	SelfChain::Index: DeserializeOwned,
	BridgedHeaderHash: Decode,
	BridgedHeaderNumber: Decode,
{
	// let's read our state first: we need best finalized header hash on **this** chain
	let self_best_finalized_header_hash = self_client.best_finalized_header_hash().await?;
	let self_best_finalized_header = self_client.header_by_hash(self_best_finalized_header_hash).await?;
	let self_best_finalized_id = HeaderId(*self_best_finalized_header.number(), self_best_finalized_header_hash);

	// now let's read id of best finalized peer header at our best finalized block
	let best_finalized_peer_on_self_method = format!("{}HeaderApi_finalized_block", bridged_chain_name);
	let encoded_best_finalized_peer_on_self = self_client
		.state_call(
			best_finalized_peer_on_self_method,
			Bytes(Vec::new()),
			Some(self_best_finalized_header_hash),
		)
		.await?;
	let decoded_best_finalized_peer_on_self: (BridgedHeaderNumber, BridgedHeaderHash) =
		Decode::decode(&mut &encoded_best_finalized_peer_on_self.0[..]).map_err(SubstrateError::ResponseParseFailed)?;
	let peer_on_self_best_finalized_id = HeaderId(
		decoded_best_finalized_peer_on_self.0,
		decoded_best_finalized_peer_on_self.1,
	);

	Ok(ClientState {
		best_self: self_best_finalized_id,
		best_peer: peer_on_self_best_finalized_id,
	})
}
