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

//! Substrate client as Substrate messages source. The chain we connect to should have
//! runtime that implements `<BridgedChainName>HeaderApi` to allow bridging with
//! `<BridgedName>` chain.

use crate::{
	finality_base::best_synced_header_id,
	messages_lane::{
		BatchProofTransaction, MessageLaneAdapter, ReceiveMessagesDeliveryProofCallBuilder,
		SubstrateMessageLane,
	},
	on_demand::OnDemandRelay,
	TransactionParams,
};

use async_std::sync::Arc;
use async_trait::async_trait;
use bp_messages::{
	storage_keys::{operating_mode_key, outbound_lane_data_key},
	ChainWithMessages as _, InboundMessageDetails, LaneId, MessageNonce, MessagePayload,
	MessagesOperatingMode, OutboundLaneData, OutboundMessageDetails,
};
use bp_runtime::{BasicOperatingMode, HeaderIdProvider};
use bridge_runtime_common::messages::target::FromBridgedChainMessagesProof;
use codec::Encode;
use frame_support::weights::Weight;
use messages_relay::{
	message_lane::{MessageLane, SourceHeaderIdOf, TargetHeaderIdOf},
	message_lane_loop::{
		ClientState, MessageDetails, MessageDetailsMap, MessageProofParameters, SourceClient,
		SourceClientState,
	},
};
use num_traits::Zero;
use relay_substrate_client::{
	AccountIdOf, AccountKeyPairOf, BalanceOf, Chain, ChainWithMessages, Client,
	Error as SubstrateError, HashOf, HeaderIdOf, TransactionEra, TransactionTracker,
	UnsignedTransaction,
};
use relay_utils::relay_loop::Client as RelayClient;
use sp_core::Pair;
use std::ops::RangeInclusive;

/// Intermediate message proof returned by the source Substrate node. Includes everything
/// required to submit to the target node: cumulative dispatch weight of bundled messages and
/// the proof itself.
pub type SubstrateMessagesProof<C> = (Weight, FromBridgedChainMessagesProof<HashOf<C>>);
type MessagesToRefine<'a> = Vec<(MessagePayload, &'a mut OutboundMessageDetails)>;

/// Substrate client as Substrate messages source.
pub struct SubstrateMessagesSource<P: SubstrateMessageLane> {
	source_client: Client<P::SourceChain>,
	target_client: Client<P::TargetChain>,
	lane_id: LaneId,
	transaction_params: TransactionParams<AccountKeyPairOf<P::SourceChain>>,
	target_to_source_headers_relay: Option<Arc<dyn OnDemandRelay<P::TargetChain, P::SourceChain>>>,
}

impl<P: SubstrateMessageLane> SubstrateMessagesSource<P> {
	/// Create new Substrate headers source.
	pub fn new(
		source_client: Client<P::SourceChain>,
		target_client: Client<P::TargetChain>,
		lane_id: LaneId,
		transaction_params: TransactionParams<AccountKeyPairOf<P::SourceChain>>,
		target_to_source_headers_relay: Option<
			Arc<dyn OnDemandRelay<P::TargetChain, P::SourceChain>>,
		>,
	) -> Self {
		SubstrateMessagesSource {
			source_client,
			target_client,
			lane_id,
			transaction_params,
			target_to_source_headers_relay,
		}
	}

	/// Read outbound lane state from the on-chain storage at given block.
	async fn outbound_lane_data(
		&self,
		id: SourceHeaderIdOf<MessageLaneAdapter<P>>,
	) -> Result<Option<OutboundLaneData>, SubstrateError> {
		self.source_client
			.storage_value(
				outbound_lane_data_key(
					P::TargetChain::WITH_CHAIN_MESSAGES_PALLET_NAME,
					&self.lane_id,
				),
				Some(id.1),
			)
			.await
	}

	/// Ensure that the messages pallet at source chain is active.
	async fn ensure_pallet_active(&self) -> Result<(), SubstrateError> {
		ensure_messages_pallet_active::<P::SourceChain, P::TargetChain>(&self.source_client).await
	}
}

impl<P: SubstrateMessageLane> Clone for SubstrateMessagesSource<P> {
	fn clone(&self) -> Self {
		Self {
			source_client: self.source_client.clone(),
			target_client: self.target_client.clone(),
			lane_id: self.lane_id,
			transaction_params: self.transaction_params.clone(),
			target_to_source_headers_relay: self.target_to_source_headers_relay.clone(),
		}
	}
}

#[async_trait]
impl<P: SubstrateMessageLane> RelayClient for SubstrateMessagesSource<P> {
	type Error = SubstrateError;

	async fn reconnect(&mut self) -> Result<(), SubstrateError> {
		// since the client calls RPC methods on both sides, we need to reconnect both
		self.source_client.reconnect().await?;
		self.target_client.reconnect().await?;

		// call reconnect on on-demand headers relay, because we may use different chains there
		// and the error that has lead to reconnect may have came from those other chains
		// (see `require_target_header_on_source`)
		//
		// this may lead to multiple reconnects to the same node during the same call and it
		// needs to be addressed in the future
		// TODO: https://github.com/paritytech/parity-bridges-common/issues/1928
		if let Some(ref mut target_to_source_headers_relay) = self.target_to_source_headers_relay {
			target_to_source_headers_relay.reconnect().await?;
		}

		Ok(())
	}
}

#[async_trait]
impl<P: SubstrateMessageLane> SourceClient<MessageLaneAdapter<P>> for SubstrateMessagesSource<P>
where
	AccountIdOf<P::SourceChain>: From<<AccountKeyPairOf<P::SourceChain> as Pair>::Public>,
{
	type BatchTransaction =
		BatchProofTransaction<P::SourceChain, P::TargetChain, P::SourceBatchCallBuilder>;
	type TransactionTracker = TransactionTracker<P::SourceChain, Client<P::SourceChain>>;

	async fn state(&self) -> Result<SourceClientState<MessageLaneAdapter<P>>, SubstrateError> {
		// we can't continue to deliver confirmations if source node is out of sync, because
		// it may have already received confirmations that we're going to deliver
		//
		// we can't continue to deliver messages if target node is out of sync, because
		// it may have already received (some of) messages that we're going to deliver
		self.source_client.ensure_synced().await?;
		self.target_client.ensure_synced().await?;
		// we can't relay confirmations if messages pallet at source chain is halted
		self.ensure_pallet_active().await?;

		read_client_state(&self.source_client, Some(&self.target_client)).await
	}

	async fn latest_generated_nonce(
		&self,
		id: SourceHeaderIdOf<MessageLaneAdapter<P>>,
	) -> Result<(SourceHeaderIdOf<MessageLaneAdapter<P>>, MessageNonce), SubstrateError> {
		// lane data missing from the storage is fine until first message is sent
		let latest_generated_nonce = self
			.outbound_lane_data(id)
			.await?
			.map(|data| data.latest_generated_nonce)
			.unwrap_or(0);
		Ok((id, latest_generated_nonce))
	}

	async fn latest_confirmed_received_nonce(
		&self,
		id: SourceHeaderIdOf<MessageLaneAdapter<P>>,
	) -> Result<(SourceHeaderIdOf<MessageLaneAdapter<P>>, MessageNonce), SubstrateError> {
		// lane data missing from the storage is fine until first message is sent
		let latest_received_nonce = self
			.outbound_lane_data(id)
			.await?
			.map(|data| data.latest_received_nonce)
			.unwrap_or(0);
		Ok((id, latest_received_nonce))
	}

	async fn generated_message_details(
		&self,
		id: SourceHeaderIdOf<MessageLaneAdapter<P>>,
		nonces: RangeInclusive<MessageNonce>,
	) -> Result<MessageDetailsMap<BalanceOf<P::SourceChain>>, SubstrateError> {
		let mut out_msgs_details = self
			.source_client
			.typed_state_call::<_, Vec<_>>(
				P::TargetChain::TO_CHAIN_MESSAGE_DETAILS_METHOD.into(),
				(self.lane_id, *nonces.start(), *nonces.end()),
				Some(id.1),
			)
			.await?;
		validate_out_msgs_details::<P::SourceChain>(&out_msgs_details, nonces)?;

		// prepare arguments of the inbound message details call (if we need it)
		let mut msgs_to_refine = vec![];
		for out_msg_details in out_msgs_details.iter_mut() {
			// in our current strategy all messages are supposed to be paid at the target chain

			// for pay-at-target messages we may want to ask target chain for
			// refined dispatch weight
			let msg_key = bp_messages::storage_keys::message_key(
				P::TargetChain::WITH_CHAIN_MESSAGES_PALLET_NAME,
				&self.lane_id,
				out_msg_details.nonce,
			);
			let msg_payload: MessagePayload =
				self.source_client.storage_value(msg_key, Some(id.1)).await?.ok_or_else(|| {
					SubstrateError::Custom(format!(
						"Message to {} {:?}/{} is missing from runtime the storage of {} at {:?}",
						P::TargetChain::NAME,
						self.lane_id,
						out_msg_details.nonce,
						P::SourceChain::NAME,
						id,
					))
				})?;

			msgs_to_refine.push((msg_payload, out_msg_details));
		}

		for mut msgs_to_refine_batch in
			split_msgs_to_refine::<P::SourceChain, P::TargetChain>(self.lane_id, msgs_to_refine)?
		{
			let in_msgs_details = self
				.target_client
				.typed_state_call::<_, Vec<InboundMessageDetails>>(
					P::SourceChain::FROM_CHAIN_MESSAGE_DETAILS_METHOD.into(),
					(self.lane_id, &msgs_to_refine_batch),
					None,
				)
				.await?;
			if in_msgs_details.len() != msgs_to_refine_batch.len() {
				return Err(SubstrateError::Custom(format!(
					"Call of {} at {} has returned {} entries instead of expected {}",
					P::SourceChain::FROM_CHAIN_MESSAGE_DETAILS_METHOD,
					P::TargetChain::NAME,
					in_msgs_details.len(),
					msgs_to_refine_batch.len(),
				)))
			}
			for ((_, out_msg_details), in_msg_details) in
				msgs_to_refine_batch.iter_mut().zip(in_msgs_details)
			{
				log::trace!(
					target: "bridge",
					"Refined weight of {}->{} message {:?}/{}: at-source: {}, at-target: {}",
					P::SourceChain::NAME,
					P::TargetChain::NAME,
					self.lane_id,
					out_msg_details.nonce,
					out_msg_details.dispatch_weight,
					in_msg_details.dispatch_weight,
				);
				out_msg_details.dispatch_weight = in_msg_details.dispatch_weight;
			}
		}

		let mut msgs_details_map = MessageDetailsMap::new();
		for out_msg_details in out_msgs_details {
			msgs_details_map.insert(
				out_msg_details.nonce,
				MessageDetails {
					dispatch_weight: out_msg_details.dispatch_weight,
					size: out_msg_details.size as _,
					reward: Zero::zero(),
				},
			);
		}

		Ok(msgs_details_map)
	}

	async fn prove_messages(
		&self,
		id: SourceHeaderIdOf<MessageLaneAdapter<P>>,
		nonces: RangeInclusive<MessageNonce>,
		proof_parameters: MessageProofParameters,
	) -> Result<
		(
			SourceHeaderIdOf<MessageLaneAdapter<P>>,
			RangeInclusive<MessageNonce>,
			<MessageLaneAdapter<P> as MessageLane>::MessagesProof,
		),
		SubstrateError,
	> {
		let mut storage_keys =
			Vec::with_capacity(nonces.end().saturating_sub(*nonces.start()) as usize + 1);
		let mut message_nonce = *nonces.start();
		while message_nonce <= *nonces.end() {
			let message_key = bp_messages::storage_keys::message_key(
				P::TargetChain::WITH_CHAIN_MESSAGES_PALLET_NAME,
				&self.lane_id,
				message_nonce,
			);
			storage_keys.push(message_key);
			message_nonce += 1;
		}
		if proof_parameters.outbound_state_proof_required {
			storage_keys.push(bp_messages::storage_keys::outbound_lane_data_key(
				P::TargetChain::WITH_CHAIN_MESSAGES_PALLET_NAME,
				&self.lane_id,
			));
		}

		let proof = self
			.source_client
			.prove_storage(storage_keys, id.1)
			.await?
			.into_iter_nodes()
			.collect();
		let proof = FromBridgedChainMessagesProof {
			bridged_header_hash: id.1,
			storage_proof: proof,
			lane: self.lane_id,
			nonces_start: *nonces.start(),
			nonces_end: *nonces.end(),
		};
		Ok((id, nonces, (proof_parameters.dispatch_weight, proof)))
	}

	async fn submit_messages_receiving_proof(
		&self,
		maybe_batch_tx: Option<Self::BatchTransaction>,
		_generated_at_block: TargetHeaderIdOf<MessageLaneAdapter<P>>,
		proof: <MessageLaneAdapter<P> as MessageLane>::MessagesReceivingProof,
	) -> Result<Self::TransactionTracker, SubstrateError> {
		let messages_proof_call =
			P::ReceiveMessagesDeliveryProofCallBuilder::build_receive_messages_delivery_proof_call(
				proof,
				maybe_batch_tx.is_none(),
			);
		let final_call = match maybe_batch_tx {
			Some(batch_tx) => batch_tx.append_call_and_build(messages_proof_call),
			None => messages_proof_call,
		};

		let transaction_params = self.transaction_params.clone();
		self.source_client
			.submit_and_watch_signed_extrinsic(
				&self.transaction_params.signer,
				move |best_block_id, transaction_nonce| {
					Ok(UnsignedTransaction::new(final_call.into(), transaction_nonce)
						.era(TransactionEra::new(best_block_id, transaction_params.mortality)))
				},
			)
			.await
	}

	async fn require_target_header_on_source(
		&self,
		id: TargetHeaderIdOf<MessageLaneAdapter<P>>,
	) -> Result<Option<Self::BatchTransaction>, SubstrateError> {
		if let Some(ref target_to_source_headers_relay) = self.target_to_source_headers_relay {
			if let Some(batch_tx) =
				BatchProofTransaction::new(target_to_source_headers_relay.clone(), id.0).await?
			{
				return Ok(Some(batch_tx))
			}

			target_to_source_headers_relay.require_more_headers(id.0).await;
		}

		Ok(None)
	}
}

/// Ensure that the messages pallet at source chain is active.
pub(crate) async fn ensure_messages_pallet_active<AtChain, WithChain>(
	client: &Client<AtChain>,
) -> Result<(), SubstrateError>
where
	AtChain: ChainWithMessages,
	WithChain: ChainWithMessages,
{
	let operating_mode = client
		.storage_value(operating_mode_key(WithChain::WITH_CHAIN_MESSAGES_PALLET_NAME), None)
		.await?;
	let is_halted =
		operating_mode == Some(MessagesOperatingMode::Basic(BasicOperatingMode::Halted));
	if is_halted {
		Err(SubstrateError::BridgePalletIsHalted)
	} else {
		Ok(())
	}
}

/// Read best blocks from given client.
///
/// This function assumes that the chain that is followed by the `self_client` has
/// bridge GRANDPA pallet deployed and it provides `best_finalized_header_id_method_name`
/// runtime API to read the best finalized Bridged chain header.
///
/// If `peer_client` is `None`, the value of `actual_best_finalized_peer_at_best_self` will
/// always match the `best_finalized_peer_at_best_self`.
pub async fn read_client_state<SelfChain, PeerChain>(
	self_client: &Client<SelfChain>,
	peer_client: Option<&Client<PeerChain>>,
) -> Result<ClientState<HeaderIdOf<SelfChain>, HeaderIdOf<PeerChain>>, SubstrateError>
where
	SelfChain: Chain,
	PeerChain: Chain,
{
	// let's read our state first: we need best finalized header hash on **this** chain
	let self_best_finalized_id = self_client.best_finalized_header().await?.id();
	// now let's read our best header on **this** chain
	let self_best_id = self_client.best_header().await?.id();

	// now let's read id of best finalized peer header at our best finalized block
	let peer_on_self_best_finalized_id =
		best_synced_header_id::<PeerChain, SelfChain>(self_client, self_best_id.hash()).await?;

	// read actual header, matching the `peer_on_self_best_finalized_id` from the peer chain
	let actual_peer_on_self_best_finalized_id =
		match (peer_client, peer_on_self_best_finalized_id.as_ref()) {
			(Some(peer_client), Some(peer_on_self_best_finalized_id)) => {
				let actual_peer_on_self_best_finalized =
					peer_client.header_by_number(peer_on_self_best_finalized_id.number()).await?;
				Some(actual_peer_on_self_best_finalized.id())
			},
			_ => peer_on_self_best_finalized_id,
		};

	Ok(ClientState {
		best_self: self_best_id,
		best_finalized_self: self_best_finalized_id,
		best_finalized_peer_at_best_self: peer_on_self_best_finalized_id,
		actual_best_finalized_peer_at_best_self: actual_peer_on_self_best_finalized_id,
	})
}

/// Reads best `PeerChain` header known to the `SelfChain` using provided runtime API method.
///
/// Method is supposed to be the `<PeerChain>FinalityApi::best_finalized()` method.
pub async fn best_finalized_peer_header_at_self<SelfChain, PeerChain>(
	self_client: &Client<SelfChain>,
	at_self_hash: HashOf<SelfChain>,
) -> Result<Option<HeaderIdOf<PeerChain>>, SubstrateError>
where
	SelfChain: Chain,
	PeerChain: Chain,
{
	// now let's read id of best finalized peer header at our best finalized block
	self_client
		.typed_state_call::<_, Option<_>>(
			PeerChain::BEST_FINALIZED_HEADER_ID_METHOD.into(),
			(),
			Some(at_self_hash),
		)
		.await
}

fn validate_out_msgs_details<C: Chain>(
	out_msgs_details: &[OutboundMessageDetails],
	nonces: RangeInclusive<MessageNonce>,
) -> Result<(), SubstrateError> {
	let make_missing_nonce_error = |expected_nonce| {
		Err(SubstrateError::Custom(format!(
			"Missing nonce {expected_nonce} in message_details call result. Expected all nonces from {nonces:?}",
		)))
	};

	if out_msgs_details.len() > nonces.clone().count() {
		return Err(SubstrateError::Custom(
			"More messages than requested returned by the message_details call.".into(),
		))
	}

	// Check if last nonce is missing. The loop below is not checking this.
	if out_msgs_details.is_empty() && !nonces.is_empty() {
		return make_missing_nonce_error(*nonces.end())
	}

	let mut nonces_iter = nonces.clone().rev().peekable();
	let mut out_msgs_details_iter = out_msgs_details.iter().rev();
	while let Some((out_msg_details, &nonce)) = out_msgs_details_iter.next().zip(nonces_iter.peek())
	{
		nonces_iter.next();
		if out_msg_details.nonce != nonce {
			// Some nonces are missing from the middle/tail of the range. This is critical error.
			return make_missing_nonce_error(nonce)
		}
	}

	// Check if some nonces from the beginning of the range are missing. This may happen if
	// some messages were already pruned from the source node. This is not a critical error
	// and will be auto-resolved by messages lane (and target node).
	if nonces_iter.peek().is_some() {
		log::info!(
			target: "bridge",
			"Some messages are missing from the {} node: {:?}. Target node may be out of sync?",
			C::NAME,
			nonces_iter.rev().collect::<Vec<_>>(),
		);
	}

	Ok(())
}

fn split_msgs_to_refine<Source: Chain + ChainWithMessages, Target: Chain>(
	lane_id: LaneId,
	msgs_to_refine: MessagesToRefine,
) -> Result<Vec<MessagesToRefine>, SubstrateError> {
	let max_batch_size = Target::max_extrinsic_size() as usize;
	let mut batches = vec![];

	let mut current_msgs_batch = msgs_to_refine;
	while !current_msgs_batch.is_empty() {
		let mut next_msgs_batch = vec![];
		while (lane_id, &current_msgs_batch).encoded_size() > max_batch_size {
			if current_msgs_batch.len() <= 1 {
				return Err(SubstrateError::Custom(format!(
					"Call of {} at {} can't be executed even if only one message is supplied. \
						max_extrinsic_size(): {}",
					Source::FROM_CHAIN_MESSAGE_DETAILS_METHOD,
					Target::NAME,
					Target::max_extrinsic_size(),
				)))
			}

			if let Some(msg) = current_msgs_batch.pop() {
				next_msgs_batch.insert(0, msg);
			}
		}

		batches.push(current_msgs_batch);
		current_msgs_batch = next_msgs_batch;
	}

	Ok(batches)
}

#[cfg(test)]
mod tests {
	use super::*;
	use relay_substrate_client::test_chain::TestChain;

	fn message_details_from_rpc(
		nonces: RangeInclusive<MessageNonce>,
	) -> Vec<OutboundMessageDetails> {
		nonces
			.into_iter()
			.map(|nonce| bp_messages::OutboundMessageDetails {
				nonce,
				dispatch_weight: Weight::zero(),
				size: 0,
			})
			.collect()
	}

	#[test]
	fn validate_out_msgs_details_succeeds_if_no_messages_are_missing() {
		assert!(validate_out_msgs_details::<TestChain>(&message_details_from_rpc(1..=3), 1..=3,)
			.is_ok());
	}

	#[test]
	fn validate_out_msgs_details_succeeds_if_head_messages_are_missing() {
		assert!(validate_out_msgs_details::<TestChain>(&message_details_from_rpc(2..=3), 1..=3,)
			.is_ok())
	}

	#[test]
	fn validate_out_msgs_details_fails_if_mid_messages_are_missing() {
		let mut message_details_from_rpc = message_details_from_rpc(1..=3);
		message_details_from_rpc.remove(1);
		assert!(matches!(
			validate_out_msgs_details::<TestChain>(&message_details_from_rpc, 1..=3,),
			Err(SubstrateError::Custom(_))
		));
	}

	#[test]
	fn validate_out_msgs_details_map_fails_if_tail_messages_are_missing() {
		assert!(matches!(
			validate_out_msgs_details::<TestChain>(&message_details_from_rpc(1..=2), 1..=3,),
			Err(SubstrateError::Custom(_))
		));
	}

	#[test]
	fn validate_out_msgs_details_fails_if_all_messages_are_missing() {
		assert!(matches!(
			validate_out_msgs_details::<TestChain>(&[], 1..=3),
			Err(SubstrateError::Custom(_))
		));
	}

	#[test]
	fn validate_out_msgs_details_fails_if_more_messages_than_nonces() {
		assert!(matches!(
			validate_out_msgs_details::<TestChain>(&message_details_from_rpc(1..=5), 2..=5,),
			Err(SubstrateError::Custom(_))
		));
	}

	fn check_split_msgs_to_refine(
		payload_sizes: Vec<usize>,
		expected_batches: Result<Vec<usize>, ()>,
	) {
		let mut out_msgs_details = vec![];
		for (idx, _) in payload_sizes.iter().enumerate() {
			out_msgs_details.push(OutboundMessageDetails {
				nonce: idx as MessageNonce,
				dispatch_weight: Weight::zero(),
				size: 0,
			});
		}

		let mut msgs_to_refine = vec![];
		for (&payload_size, out_msg_details) in
			payload_sizes.iter().zip(out_msgs_details.iter_mut())
		{
			let payload = vec![1u8; payload_size];
			msgs_to_refine.push((payload, out_msg_details));
		}

		let maybe_batches =
			split_msgs_to_refine::<TestChain, TestChain>(Default::default(), msgs_to_refine);
		match expected_batches {
			Ok(expected_batches) => {
				let batches = maybe_batches.unwrap();
				let mut idx = 0;
				assert_eq!(batches.len(), expected_batches.len());
				for (batch, &expected_batch_size) in batches.iter().zip(expected_batches.iter()) {
					assert_eq!(batch.len(), expected_batch_size);
					for msg_to_refine in batch {
						assert_eq!(msg_to_refine.0.len(), payload_sizes[idx]);
						idx += 1;
					}
				}
			},
			Err(_) => {
				matches!(maybe_batches, Err(SubstrateError::Custom(_)));
			},
		}
	}

	#[test]
	fn test_split_msgs_to_refine() {
		let max_extrinsic_size = 100000;

		// Check that an error is returned when one of the messages is too big.
		check_split_msgs_to_refine(vec![max_extrinsic_size], Err(()));
		check_split_msgs_to_refine(vec![50, 100, max_extrinsic_size, 200], Err(()));

		// Otherwise check that the split is valid.
		check_split_msgs_to_refine(vec![100, 200, 300, 400], Ok(vec![4]));
		check_split_msgs_to_refine(
			vec![
				50,
				100,
				max_extrinsic_size - 500,
				500,
				1000,
				1500,
				max_extrinsic_size - 3500,
				5000,
				10000,
			],
			Ok(vec![3, 4, 2]),
		);
		check_split_msgs_to_refine(
			vec![
				50,
				100,
				max_extrinsic_size - 150,
				500,
				1000,
				1500,
				max_extrinsic_size - 3000,
				5000,
				10000,
			],
			Ok(vec![2, 1, 3, 1, 2]),
		);
		check_split_msgs_to_refine(
			vec![
				5000,
				10000,
				max_extrinsic_size - 3500,
				500,
				1000,
				1500,
				max_extrinsic_size - 500,
				50,
				100,
			],
			Ok(vec![2, 4, 3]),
		);
	}
}
