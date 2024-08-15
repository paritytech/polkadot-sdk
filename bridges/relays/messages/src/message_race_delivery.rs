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

//! Message delivery race delivers proof-of-messages from "lane.source" to "lane.target".

use std::{collections::VecDeque, marker::PhantomData, ops::RangeInclusive};

use async_trait::async_trait;
use futures::stream::FusedStream;

use bp_messages::{MessageNonce, UnrewardedRelayersState, Weight};
use relay_utils::{FailedClient, TrackedTransactionStatus, TransactionTracker};

use crate::{
	message_lane::{MessageLane, SourceHeaderIdOf, TargetHeaderIdOf},
	message_lane_loop::{
		MessageDeliveryParams, MessageDetailsMap, MessageProofParameters, NoncesSubmitArtifacts,
		SourceClient as MessageLaneSourceClient, SourceClientState,
		TargetClient as MessageLaneTargetClient, TargetClientState,
	},
	message_race_limits::{MessageRaceLimits, RelayMessagesBatchReference},
	message_race_loop::{
		MessageRace, NoncesRange, RaceState, RaceStrategy, SourceClient, SourceClientNonces,
		TargetClient, TargetClientNonces,
	},
	message_race_strategy::BasicStrategy,
	metrics::MessageLaneLoopMetrics,
};

/// Run message delivery race.
pub async fn run<P: MessageLane>(
	source_client: impl MessageLaneSourceClient<P>,
	source_state_updates: impl FusedStream<Item = SourceClientState<P>>,
	target_client: impl MessageLaneTargetClient<P>,
	target_state_updates: impl FusedStream<Item = TargetClientState<P>>,
	metrics_msg: Option<MessageLaneLoopMetrics>,
	params: MessageDeliveryParams,
) -> Result<(), FailedClient> {
	crate::message_race_loop::run(
		MessageDeliveryRaceSource {
			client: source_client.clone(),
			metrics_msg: metrics_msg.clone(),
			_phantom: Default::default(),
		},
		source_state_updates,
		MessageDeliveryRaceTarget {
			client: target_client.clone(),
			metrics_msg: metrics_msg.clone(),
			_phantom: Default::default(),
		},
		target_state_updates,
		MessageDeliveryStrategy::<P, _, _> {
			lane_source_client: source_client,
			lane_target_client: target_client,
			max_unrewarded_relayer_entries_at_target: params
				.max_unrewarded_relayer_entries_at_target,
			max_unconfirmed_nonces_at_target: params.max_unconfirmed_nonces_at_target,
			max_messages_in_single_batch: params.max_messages_in_single_batch,
			max_messages_weight_in_single_batch: params.max_messages_weight_in_single_batch,
			max_messages_size_in_single_batch: params.max_messages_size_in_single_batch,
			latest_confirmed_nonces_at_source: VecDeque::new(),
			target_nonces: None,
			strategy: BasicStrategy::new(),
			metrics_msg,
		},
	)
	.await
}

/// Relay range of messages.
pub async fn relay_messages_range<P: MessageLane>(
	source_client: impl MessageLaneSourceClient<P>,
	target_client: impl MessageLaneTargetClient<P>,
	at: SourceHeaderIdOf<P>,
	range: RangeInclusive<MessageNonce>,
	outbound_state_proof_required: bool,
) -> Result<(), ()> {
	// compute cumulative dispatch weight of all messages in given range
	let dispatch_weight = source_client
		.generated_message_details(at.clone(), range.clone())
		.await
		.map_err(|e| {
			log::error!(
				target: "bridge",
				"Failed to get generated message details at {:?} for messages {:?}: {:?}",
				at,
				range,
				e,
			);
		})?
		.values()
		.fold(Weight::zero(), |total, details| total.saturating_add(details.dispatch_weight));
	// prepare messages proof
	let (at, range, proof) = source_client
		.prove_messages(
			at.clone(),
			range.clone(),
			MessageProofParameters { outbound_state_proof_required, dispatch_weight },
		)
		.await
		.map_err(|e| {
			log::error!(
				target: "bridge",
				"Failed to generate messages proof at {:?} for messages {:?}: {:?}",
				at,
				range,
				e,
			);
		})?;
	// submit messages proof to the target node
	let tx_tracker = target_client
		.submit_messages_proof(None, at, range.clone(), proof)
		.await
		.map_err(|e| {
			log::error!(
				target: "bridge",
				"Failed to submit messages proof for messages {:?}: {:?}",
				range,
				e,
			);
		})?
		.tx_tracker;

	match tx_tracker.wait().await {
		TrackedTransactionStatus::Finalized(_) => Ok(()),
		TrackedTransactionStatus::Lost => {
			log::error!("Transaction with messages {:?} is considered lost", range,);
			Err(())
		},
	}
}

/// Message delivery race.
struct MessageDeliveryRace<P>(std::marker::PhantomData<P>);

impl<P: MessageLane> MessageRace for MessageDeliveryRace<P> {
	type SourceHeaderId = SourceHeaderIdOf<P>;
	type TargetHeaderId = TargetHeaderIdOf<P>;

	type MessageNonce = MessageNonce;
	type Proof = P::MessagesProof;

	fn source_name() -> String {
		format!("{}::MessagesDelivery", P::SOURCE_NAME)
	}

	fn target_name() -> String {
		format!("{}::MessagesDelivery", P::TARGET_NAME)
	}
}

/// Message delivery race source, which is a source of the lane.
struct MessageDeliveryRaceSource<P: MessageLane, C> {
	client: C,
	metrics_msg: Option<MessageLaneLoopMetrics>,
	_phantom: PhantomData<P>,
}

#[async_trait]
impl<P, C> SourceClient<MessageDeliveryRace<P>> for MessageDeliveryRaceSource<P, C>
where
	P: MessageLane,
	C: MessageLaneSourceClient<P>,
{
	type Error = C::Error;
	type NoncesRange = MessageDetailsMap<P::SourceChainBalance>;
	type ProofParameters = MessageProofParameters;

	async fn nonces(
		&self,
		at_block: SourceHeaderIdOf<P>,
		prev_latest_nonce: MessageNonce,
	) -> Result<(SourceHeaderIdOf<P>, SourceClientNonces<Self::NoncesRange>), Self::Error> {
		let (at_block, latest_generated_nonce) =
			self.client.latest_generated_nonce(at_block).await?;
		let (at_block, latest_confirmed_nonce) =
			self.client.latest_confirmed_received_nonce(at_block).await?;

		if let Some(metrics_msg) = self.metrics_msg.as_ref() {
			metrics_msg.update_source_latest_generated_nonce(latest_generated_nonce);
			metrics_msg.update_source_latest_confirmed_nonce(latest_confirmed_nonce);
		}

		let new_nonces = if latest_generated_nonce > prev_latest_nonce {
			self.client
				.generated_message_details(
					at_block.clone(),
					prev_latest_nonce + 1..=latest_generated_nonce,
				)
				.await?
		} else {
			MessageDetailsMap::new()
		};

		Ok((
			at_block,
			SourceClientNonces { new_nonces, confirmed_nonce: Some(latest_confirmed_nonce) },
		))
	}

	async fn generate_proof(
		&self,
		at_block: SourceHeaderIdOf<P>,
		nonces: RangeInclusive<MessageNonce>,
		proof_parameters: Self::ProofParameters,
	) -> Result<(SourceHeaderIdOf<P>, RangeInclusive<MessageNonce>, P::MessagesProof), Self::Error>
	{
		self.client.prove_messages(at_block, nonces, proof_parameters).await
	}
}

/// Message delivery race target, which is a target of the lane.
struct MessageDeliveryRaceTarget<P: MessageLane, C> {
	client: C,
	metrics_msg: Option<MessageLaneLoopMetrics>,
	_phantom: PhantomData<P>,
}

#[async_trait]
impl<P, C> TargetClient<MessageDeliveryRace<P>> for MessageDeliveryRaceTarget<P, C>
where
	P: MessageLane,
	C: MessageLaneTargetClient<P>,
{
	type Error = C::Error;
	type TargetNoncesData = DeliveryRaceTargetNoncesData;
	type BatchTransaction = C::BatchTransaction;
	type TransactionTracker = C::TransactionTracker;

	async fn require_source_header(
		&self,
		id: SourceHeaderIdOf<P>,
	) -> Result<Option<C::BatchTransaction>, Self::Error> {
		self.client.require_source_header_on_target(id).await
	}

	async fn nonces(
		&self,
		at_block: TargetHeaderIdOf<P>,
		update_metrics: bool,
	) -> Result<(TargetHeaderIdOf<P>, TargetClientNonces<DeliveryRaceTargetNoncesData>), Self::Error>
	{
		let (at_block, latest_received_nonce) = self.client.latest_received_nonce(at_block).await?;
		let (at_block, latest_confirmed_nonce) =
			self.client.latest_confirmed_received_nonce(at_block).await?;
		let (at_block, unrewarded_relayers) =
			self.client.unrewarded_relayers_state(at_block).await?;

		if update_metrics {
			if let Some(metrics_msg) = self.metrics_msg.as_ref() {
				metrics_msg.update_target_latest_received_nonce(latest_received_nonce);
				metrics_msg.update_target_latest_confirmed_nonce(latest_confirmed_nonce);
			}
		}

		Ok((
			at_block,
			TargetClientNonces {
				latest_nonce: latest_received_nonce,
				nonces_data: DeliveryRaceTargetNoncesData {
					confirmed_nonce: latest_confirmed_nonce,
					unrewarded_relayers,
				},
			},
		))
	}

	async fn submit_proof(
		&self,
		maybe_batch_tx: Option<Self::BatchTransaction>,
		generated_at_block: SourceHeaderIdOf<P>,
		nonces: RangeInclusive<MessageNonce>,
		proof: P::MessagesProof,
	) -> Result<NoncesSubmitArtifacts<Self::TransactionTracker>, Self::Error> {
		self.client
			.submit_messages_proof(maybe_batch_tx, generated_at_block, nonces, proof)
			.await
	}
}

/// Additional nonces data from the target client used by message delivery race.
#[derive(Debug, Clone)]
struct DeliveryRaceTargetNoncesData {
	/// The latest nonce that we know: (1) has been delivered to us (2) has been confirmed
	/// back to the source node (by confirmations race) and (3) relayer has received
	/// reward for (and this has been confirmed by the message delivery race).
	confirmed_nonce: MessageNonce,
	/// State of the unrewarded relayers set at the target node.
	unrewarded_relayers: UnrewardedRelayersState,
}

/// Messages delivery strategy.
struct MessageDeliveryStrategy<P: MessageLane, SC, TC> {
	/// The client that is connected to the message lane source node.
	lane_source_client: SC,
	/// The client that is connected to the message lane target node.
	lane_target_client: TC,
	/// Maximal unrewarded relayer entries at target client.
	max_unrewarded_relayer_entries_at_target: MessageNonce,
	/// Maximal unconfirmed nonces at target client.
	max_unconfirmed_nonces_at_target: MessageNonce,
	/// Maximal number of messages in the single delivery transaction.
	max_messages_in_single_batch: MessageNonce,
	/// Maximal cumulative messages weight in the single delivery transaction.
	max_messages_weight_in_single_batch: Weight,
	/// Maximal messages size in the single delivery transaction.
	max_messages_size_in_single_batch: u32,
	/// Latest confirmed nonces at the source client + the header id where we have first met this
	/// nonce.
	latest_confirmed_nonces_at_source: VecDeque<(SourceHeaderIdOf<P>, MessageNonce)>,
	/// Target nonces available at the **best** block of the target chain.
	target_nonces: Option<TargetClientNonces<DeliveryRaceTargetNoncesData>>,
	/// Basic delivery strategy.
	strategy: MessageDeliveryStrategyBase<P>,
	/// Message lane metrics.
	metrics_msg: Option<MessageLaneLoopMetrics>,
}

type MessageDeliveryStrategyBase<P> = BasicStrategy<
	<P as MessageLane>::SourceHeaderNumber,
	<P as MessageLane>::SourceHeaderHash,
	<P as MessageLane>::TargetHeaderNumber,
	<P as MessageLane>::TargetHeaderHash,
	MessageDetailsMap<<P as MessageLane>::SourceChainBalance>,
	<P as MessageLane>::MessagesProof,
>;

impl<P: MessageLane, SC, TC> std::fmt::Debug for MessageDeliveryStrategy<P, SC, TC> {
	fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
		fmt.debug_struct("MessageDeliveryStrategy")
			.field(
				"max_unrewarded_relayer_entries_at_target",
				&self.max_unrewarded_relayer_entries_at_target,
			)
			.field("max_unconfirmed_nonces_at_target", &self.max_unconfirmed_nonces_at_target)
			.field("max_messages_in_single_batch", &self.max_messages_in_single_batch)
			.field("max_messages_weight_in_single_batch", &self.max_messages_weight_in_single_batch)
			.field("max_messages_size_in_single_batch", &self.max_messages_size_in_single_batch)
			.field("latest_confirmed_nonces_at_source", &self.latest_confirmed_nonces_at_source)
			.field("target_nonces", &self.target_nonces)
			.field("strategy", &self.strategy)
			.finish()
	}
}

impl<P: MessageLane, SC, TC> MessageDeliveryStrategy<P, SC, TC>
where
	P: MessageLane,
	SC: MessageLaneSourceClient<P>,
	TC: MessageLaneTargetClient<P>,
{
	/// Returns true if some race action can be selected (with `select_race_action`) at given
	/// `best_finalized_source_header_id_at_best_target` source header at target.
	async fn can_submit_transaction_with<
		RS: RaceState<SourceHeaderIdOf<P>, TargetHeaderIdOf<P>>,
	>(
		&self,
		mut race_state: RS,
		maybe_best_finalized_source_header_id_at_best_target: Option<SourceHeaderIdOf<P>>,
	) -> bool {
		if let Some(best_finalized_source_header_id_at_best_target) =
			maybe_best_finalized_source_header_id_at_best_target
		{
			race_state.set_best_finalized_source_header_id_at_best_target(
				best_finalized_source_header_id_at_best_target,
			);

			return self.select_race_action(race_state).await.is_some()
		}

		false
	}

	async fn select_race_action<RS: RaceState<SourceHeaderIdOf<P>, TargetHeaderIdOf<P>>>(
		&self,
		race_state: RS,
	) -> Option<(RangeInclusive<MessageNonce>, MessageProofParameters)> {
		// if we have already selected nonces that we want to submit, do nothing
		if race_state.nonces_to_submit().is_some() {
			return None
		}

		// if we already submitted some nonces, do nothing
		if race_state.nonces_submitted().is_some() {
			return None
		}

		let best_target_nonce = self.strategy.best_at_target()?;
		let best_finalized_source_header_id_at_best_target =
			race_state.best_finalized_source_header_id_at_best_target()?;
		let target_nonces = self.target_nonces.as_ref()?;
		let latest_confirmed_nonce_at_source = self
			.latest_confirmed_nonce_at_source(&best_finalized_source_header_id_at_best_target)
			.unwrap_or(target_nonces.nonces_data.confirmed_nonce);

		// There's additional condition in the message delivery race: target would reject messages
		// if there are too much unconfirmed messages at the inbound lane.

		// Ok - we may have new nonces to deliver. But target may still reject new messages, because
		// we haven't notified it that (some) messages have been confirmed. So we may want to
		// include updated `source.latest_confirmed` in the proof.
		//
		// Important note: we're including outbound state lane proof whenever there are unconfirmed
		// nonces on the target chain. Other strategy is to include it only if it's absolutely
		// necessary.
		let latest_received_nonce_at_target = target_nonces.latest_nonce;
		let latest_confirmed_nonce_at_target = target_nonces.nonces_data.confirmed_nonce;
		let outbound_state_proof_required =
			latest_confirmed_nonce_at_target < latest_confirmed_nonce_at_source;

		// The target node would also reject messages if there are too many entries in the
		// "unrewarded relayers" set. If we are unable to prove new rewards to the target node, then
		// we should wait for confirmations race.
		let unrewarded_limit_reached =
			target_nonces.nonces_data.unrewarded_relayers.unrewarded_relayer_entries >=
				self.max_unrewarded_relayer_entries_at_target ||
				target_nonces.nonces_data.unrewarded_relayers.total_messages >=
					self.max_unconfirmed_nonces_at_target;
		if unrewarded_limit_reached {
			// so there are already too many unrewarded relayer entries in the set
			//
			// => check if we can prove enough rewards. If not, we should wait for more rewards to
			// be paid
			let number_of_rewards_being_proved =
				latest_confirmed_nonce_at_source.saturating_sub(latest_confirmed_nonce_at_target);
			let enough_rewards_being_proved = number_of_rewards_being_proved >=
				target_nonces.nonces_data.unrewarded_relayers.messages_in_oldest_entry;
			if !enough_rewards_being_proved {
				return None
			}
		}

		// If we're here, then the confirmations race did its job && sending side now knows that
		// messages have been delivered. Now let's select nonces that we want to deliver.
		//
		// We may deliver at most:
		//
		// max_unconfirmed_nonces_at_target - (latest_received_nonce_at_target -
		// latest_confirmed_nonce_at_target)
		//
		// messages in the batch. But since we're including outbound state proof in the batch, then
		// it may be increased to:
		//
		// max_unconfirmed_nonces_at_target - (latest_received_nonce_at_target -
		// latest_confirmed_nonce_at_source)
		let future_confirmed_nonce_at_target = if outbound_state_proof_required {
			latest_confirmed_nonce_at_source
		} else {
			latest_confirmed_nonce_at_target
		};
		let max_nonces = latest_received_nonce_at_target
			.checked_sub(future_confirmed_nonce_at_target)
			.and_then(|diff| self.max_unconfirmed_nonces_at_target.checked_sub(diff))
			.unwrap_or_default();
		let max_nonces = std::cmp::min(max_nonces, self.max_messages_in_single_batch);
		let max_messages_weight_in_single_batch = self.max_messages_weight_in_single_batch;
		let max_messages_size_in_single_batch = self.max_messages_size_in_single_batch;
		let lane_source_client = self.lane_source_client.clone();
		let lane_target_client = self.lane_target_client.clone();

		// select nonces from nonces, available for delivery
		let selected_nonces = match self.strategy.available_source_queue_indices(race_state) {
			Some(available_source_queue_indices) => {
				let source_queue = self.strategy.source_queue();
				let reference = RelayMessagesBatchReference {
					max_messages_in_this_batch: max_nonces,
					max_messages_weight_in_single_batch,
					max_messages_size_in_single_batch,
					lane_source_client: lane_source_client.clone(),
					lane_target_client: lane_target_client.clone(),
					best_target_nonce,
					nonces_queue: source_queue.clone(),
					nonces_queue_range: available_source_queue_indices,
					metrics: self.metrics_msg.clone(),
				};

				MessageRaceLimits::decide(reference).await
			},
			None => {
				// we still may need to submit delivery transaction with zero messages to
				// unblock the lane. But it'll only be accepted if the lane is blocked
				// (i.e. when `unrewarded_limit_reached` is `true`)
				None
			},
		};

		// check if we need unblocking transaction and we may submit it
		#[allow(clippy::reversed_empty_ranges)]
		let selected_nonces = match selected_nonces {
			Some(selected_nonces) => selected_nonces,
			None if unrewarded_limit_reached && outbound_state_proof_required => 1..=0,
			_ => return None,
		};

		let dispatch_weight = self.dispatch_weight_for_range(&selected_nonces);
		Some((
			selected_nonces,
			MessageProofParameters { outbound_state_proof_required, dispatch_weight },
		))
	}

	/// Returns latest confirmed message at source chain, given source block.
	fn latest_confirmed_nonce_at_source(&self, at: &SourceHeaderIdOf<P>) -> Option<MessageNonce> {
		self.latest_confirmed_nonces_at_source
			.iter()
			.take_while(|(id, _)| id.0 <= at.0)
			.last()
			.map(|(_, nonce)| *nonce)
	}

	/// Returns total weight of all undelivered messages.
	fn dispatch_weight_for_range(&self, range: &RangeInclusive<MessageNonce>) -> Weight {
		self.strategy
			.source_queue()
			.iter()
			.flat_map(|(_, subrange)| {
				subrange
					.iter()
					.filter(|(nonce, _)| range.contains(nonce))
					.map(|(_, details)| details.dispatch_weight)
			})
			.fold(Weight::zero(), |total, weight| total.saturating_add(weight))
	}
}

#[async_trait]
impl<P, SC, TC> RaceStrategy<SourceHeaderIdOf<P>, TargetHeaderIdOf<P>, P::MessagesProof>
	for MessageDeliveryStrategy<P, SC, TC>
where
	P: MessageLane,
	SC: MessageLaneSourceClient<P>,
	TC: MessageLaneTargetClient<P>,
{
	type SourceNoncesRange = MessageDetailsMap<P::SourceChainBalance>;
	type ProofParameters = MessageProofParameters;
	type TargetNoncesData = DeliveryRaceTargetNoncesData;

	fn is_empty(&self) -> bool {
		self.strategy.is_empty()
	}

	async fn required_source_header_at_target<
		RS: RaceState<SourceHeaderIdOf<P>, TargetHeaderIdOf<P>>,
	>(
		&self,
		race_state: RS,
	) -> Option<SourceHeaderIdOf<P>> {
		// we have already submitted something - let's wait until it is mined
		if race_state.nonces_submitted().is_some() {
			return None
		}

		// if we can deliver something using current race state, go on
		let selected_nonces = self.select_race_action(race_state.clone()).await;
		if selected_nonces.is_some() {
			return None
		}

		// check if we may deliver some messages if we'll relay require source header
		// to target first
		let maybe_source_header_for_delivery =
			self.strategy.source_queue().back().map(|(id, _)| id.clone());
		if self
			.can_submit_transaction_with(
				race_state.clone(),
				maybe_source_header_for_delivery.clone(),
			)
			.await
		{
			return maybe_source_header_for_delivery
		}

		// ok, we can't delivery anything even if we relay some source blocks first. But maybe
		// the lane is blocked and we need to submit unblock transaction?
		let maybe_source_header_for_reward_confirmation =
			self.latest_confirmed_nonces_at_source.back().map(|(id, _)| id.clone());
		if self
			.can_submit_transaction_with(
				race_state.clone(),
				maybe_source_header_for_reward_confirmation.clone(),
			)
			.await
		{
			return maybe_source_header_for_reward_confirmation
		}

		None
	}

	fn best_at_source(&self) -> Option<MessageNonce> {
		self.strategy.best_at_source()
	}

	fn best_at_target(&self) -> Option<MessageNonce> {
		self.strategy.best_at_target()
	}

	fn source_nonces_updated(
		&mut self,
		at_block: SourceHeaderIdOf<P>,
		nonces: SourceClientNonces<Self::SourceNoncesRange>,
	) {
		if let Some(confirmed_nonce) = nonces.confirmed_nonce {
			let is_confirmed_nonce_updated = self
				.latest_confirmed_nonces_at_source
				.back()
				.map(|(_, prev_nonce)| *prev_nonce != confirmed_nonce)
				.unwrap_or(true);
			if is_confirmed_nonce_updated {
				self.latest_confirmed_nonces_at_source
					.push_back((at_block.clone(), confirmed_nonce));
			}
		}
		self.strategy.source_nonces_updated(at_block, nonces)
	}

	fn reset_best_target_nonces(&mut self) {
		self.target_nonces = None;
		self.strategy.reset_best_target_nonces();
	}

	fn best_target_nonces_updated<RS: RaceState<SourceHeaderIdOf<P>, TargetHeaderIdOf<P>>>(
		&mut self,
		nonces: TargetClientNonces<DeliveryRaceTargetNoncesData>,
		race_state: &mut RS,
	) {
		// best target nonces must always be ge than finalized target nonces
		let latest_nonce = nonces.latest_nonce;
		self.target_nonces = Some(nonces);

		self.strategy.best_target_nonces_updated(
			TargetClientNonces { latest_nonce, nonces_data: () },
			race_state,
		)
	}

	fn finalized_target_nonces_updated<RS: RaceState<SourceHeaderIdOf<P>, TargetHeaderIdOf<P>>>(
		&mut self,
		nonces: TargetClientNonces<DeliveryRaceTargetNoncesData>,
		race_state: &mut RS,
	) {
		if let Some(ref best_finalized_source_header_id_at_best_target) =
			race_state.best_finalized_source_header_id_at_best_target()
		{
			let oldest_header_number_to_keep = best_finalized_source_header_id_at_best_target.0;
			while self
				.latest_confirmed_nonces_at_source
				.front()
				.map(|(id, _)| id.0 < oldest_header_number_to_keep)
				.unwrap_or(false)
			{
				self.latest_confirmed_nonces_at_source.pop_front();
			}
		}

		if let Some(ref mut target_nonces) = self.target_nonces {
			target_nonces.latest_nonce =
				std::cmp::max(target_nonces.latest_nonce, nonces.latest_nonce);
		}

		self.strategy.finalized_target_nonces_updated(
			TargetClientNonces { latest_nonce: nonces.latest_nonce, nonces_data: () },
			race_state,
		)
	}

	async fn select_nonces_to_deliver<RS: RaceState<SourceHeaderIdOf<P>, TargetHeaderIdOf<P>>>(
		&self,
		race_state: RS,
	) -> Option<(RangeInclusive<MessageNonce>, Self::ProofParameters)> {
		self.select_race_action(race_state).await
	}
}

impl<SourceChainBalance: std::fmt::Debug> NoncesRange for MessageDetailsMap<SourceChainBalance> {
	fn begin(&self) -> MessageNonce {
		self.keys().next().cloned().unwrap_or_default()
	}

	fn end(&self) -> MessageNonce {
		self.keys().next_back().cloned().unwrap_or_default()
	}

	fn greater_than(mut self, nonce: MessageNonce) -> Option<Self> {
		let gte = self.split_off(&(nonce + 1));
		if gte.is_empty() {
			None
		} else {
			Some(gte)
		}
	}
}

#[cfg(test)]
mod tests {
	use crate::{
		message_lane_loop::{
			tests::{
				header_id, TestMessageLane, TestMessagesBatchTransaction, TestMessagesProof,
				TestSourceChainBalance, TestSourceClient, TestSourceHeaderId, TestTargetClient,
				TestTargetHeaderId,
			},
			MessageDetails,
		},
		message_race_loop::RaceStateImpl,
	};

	use super::*;

	const DEFAULT_DISPATCH_WEIGHT: Weight = Weight::from_parts(1, 0);
	const DEFAULT_SIZE: u32 = 1;

	type TestRaceState = RaceStateImpl<
		TestSourceHeaderId,
		TestTargetHeaderId,
		TestMessagesProof,
		TestMessagesBatchTransaction,
	>;
	type TestStrategy =
		MessageDeliveryStrategy<TestMessageLane, TestSourceClient, TestTargetClient>;

	fn source_nonces(
		new_nonces: RangeInclusive<MessageNonce>,
		confirmed_nonce: MessageNonce,
		reward: TestSourceChainBalance,
	) -> SourceClientNonces<MessageDetailsMap<TestSourceChainBalance>> {
		SourceClientNonces {
			new_nonces: new_nonces
				.into_iter()
				.map(|nonce| {
					(
						nonce,
						MessageDetails {
							dispatch_weight: DEFAULT_DISPATCH_WEIGHT,
							size: DEFAULT_SIZE,
							reward,
						},
					)
				})
				.collect(),
			confirmed_nonce: Some(confirmed_nonce),
		}
	}

	fn prepare_strategy() -> (TestRaceState, TestStrategy) {
		let mut race_state = RaceStateImpl {
			best_finalized_source_header_id_at_source: Some(header_id(1)),
			best_finalized_source_header_id_at_best_target: Some(header_id(1)),
			best_target_header_id: Some(header_id(1)),
			best_finalized_target_header_id: Some(header_id(1)),
			nonces_to_submit: None,
			nonces_to_submit_batch: None,
			nonces_submitted: None,
		};

		let mut race_strategy = TestStrategy {
			max_unrewarded_relayer_entries_at_target: 4,
			max_unconfirmed_nonces_at_target: 4,
			max_messages_in_single_batch: 4,
			max_messages_weight_in_single_batch: Weight::from_parts(4, 0),
			max_messages_size_in_single_batch: 4,
			latest_confirmed_nonces_at_source: vec![(header_id(1), 19)].into_iter().collect(),
			lane_source_client: TestSourceClient::default(),
			lane_target_client: TestTargetClient::default(),
			metrics_msg: None,
			target_nonces: Some(TargetClientNonces {
				latest_nonce: 19,
				nonces_data: DeliveryRaceTargetNoncesData {
					confirmed_nonce: 19,
					unrewarded_relayers: UnrewardedRelayersState {
						unrewarded_relayer_entries: 0,
						messages_in_oldest_entry: 0,
						total_messages: 0,
						last_delivered_nonce: 0,
					},
				},
			}),
			strategy: BasicStrategy::new(),
		};

		race_strategy
			.strategy
			.source_nonces_updated(header_id(1), source_nonces(20..=23, 19, 0));

		let target_nonces = TargetClientNonces { latest_nonce: 19, nonces_data: () };
		race_strategy
			.strategy
			.best_target_nonces_updated(target_nonces.clone(), &mut race_state);
		race_strategy
			.strategy
			.finalized_target_nonces_updated(target_nonces, &mut race_state);

		(race_state, race_strategy)
	}

	fn proof_parameters(state_required: bool, weight: u32) -> MessageProofParameters {
		MessageProofParameters {
			outbound_state_proof_required: state_required,
			dispatch_weight: Weight::from_parts(weight as u64, 0),
		}
	}

	#[test]
	fn weights_map_works_as_nonces_range() {
		fn build_map(
			range: RangeInclusive<MessageNonce>,
		) -> MessageDetailsMap<TestSourceChainBalance> {
			range
				.map(|idx| {
					(
						idx,
						MessageDetails {
							dispatch_weight: Weight::from_parts(idx, 0),
							size: idx as _,
							reward: idx as _,
						},
					)
				})
				.collect()
		}

		let map = build_map(20..=30);

		assert_eq!(map.begin(), 20);
		assert_eq!(map.end(), 30);
		assert_eq!(map.clone().greater_than(10), Some(build_map(20..=30)));
		assert_eq!(map.clone().greater_than(19), Some(build_map(20..=30)));
		assert_eq!(map.clone().greater_than(20), Some(build_map(21..=30)));
		assert_eq!(map.clone().greater_than(25), Some(build_map(26..=30)));
		assert_eq!(map.clone().greater_than(29), Some(build_map(30..=30)));
		assert_eq!(map.greater_than(30), None);
	}

	#[async_std::test]
	async fn message_delivery_strategy_selects_messages_to_deliver() {
		let (state, strategy) = prepare_strategy();

		// both sides are ready to relay new messages
		assert_eq!(
			strategy.select_nonces_to_deliver(state).await,
			Some(((20..=23), proof_parameters(false, 4)))
		);
	}

	#[async_std::test]
	async fn message_delivery_strategy_includes_outbound_state_proof_when_new_nonces_are_available()
	{
		let (state, mut strategy) = prepare_strategy();

		// if there are new confirmed nonces on source, we want to relay this information
		// to target to prune rewards queue
		let prev_confirmed_nonce_at_source =
			strategy.latest_confirmed_nonces_at_source.back().unwrap().1;
		strategy.target_nonces.as_mut().unwrap().nonces_data.confirmed_nonce =
			prev_confirmed_nonce_at_source - 1;
		assert_eq!(
			strategy.select_nonces_to_deliver(state).await,
			Some(((20..=23), proof_parameters(true, 4)))
		);
	}

	#[async_std::test]
	async fn message_delivery_strategy_selects_nothing_if_there_are_too_many_unrewarded_relayers() {
		let (state, mut strategy) = prepare_strategy();

		// if there are already `max_unrewarded_relayer_entries_at_target` entries at target,
		// we need to wait until rewards will be paid
		{
			let unrewarded_relayers =
				&mut strategy.target_nonces.as_mut().unwrap().nonces_data.unrewarded_relayers;
			unrewarded_relayers.unrewarded_relayer_entries =
				strategy.max_unrewarded_relayer_entries_at_target;
			unrewarded_relayers.messages_in_oldest_entry = 4;
		}
		assert_eq!(strategy.select_nonces_to_deliver(state).await, None);
	}

	#[async_std::test]
	async fn message_delivery_strategy_selects_nothing_if_proved_rewards_is_not_enough_to_remove_oldest_unrewarded_entry(
	) {
		let (state, mut strategy) = prepare_strategy();

		// if there are already `max_unrewarded_relayer_entries_at_target` entries at target,
		// we need to prove at least `messages_in_oldest_entry` rewards
		let prev_confirmed_nonce_at_source =
			strategy.latest_confirmed_nonces_at_source.back().unwrap().1;
		{
			let nonces_data = &mut strategy.target_nonces.as_mut().unwrap().nonces_data;
			nonces_data.confirmed_nonce = prev_confirmed_nonce_at_source - 1;
			let unrewarded_relayers = &mut nonces_data.unrewarded_relayers;
			unrewarded_relayers.unrewarded_relayer_entries =
				strategy.max_unrewarded_relayer_entries_at_target;
			unrewarded_relayers.messages_in_oldest_entry = 4;
		}
		assert_eq!(strategy.select_nonces_to_deliver(state).await, None);
	}

	#[async_std::test]
	async fn message_delivery_strategy_includes_outbound_state_proof_if_proved_rewards_is_enough() {
		let (state, mut strategy) = prepare_strategy();

		// if there are already `max_unrewarded_relayer_entries_at_target` entries at target,
		// we need to prove at least `messages_in_oldest_entry` rewards
		let prev_confirmed_nonce_at_source =
			strategy.latest_confirmed_nonces_at_source.back().unwrap().1;
		{
			let nonces_data = &mut strategy.target_nonces.as_mut().unwrap().nonces_data;
			nonces_data.confirmed_nonce = prev_confirmed_nonce_at_source - 3;
			let unrewarded_relayers = &mut nonces_data.unrewarded_relayers;
			unrewarded_relayers.unrewarded_relayer_entries =
				strategy.max_unrewarded_relayer_entries_at_target;
			unrewarded_relayers.messages_in_oldest_entry = 3;
		}
		assert_eq!(
			strategy.select_nonces_to_deliver(state).await,
			Some(((20..=23), proof_parameters(true, 4)))
		);
	}

	#[async_std::test]
	async fn message_delivery_strategy_limits_batch_by_messages_weight() {
		let (state, mut strategy) = prepare_strategy();

		// not all queued messages may fit in the batch, because batch has max weight
		strategy.max_messages_weight_in_single_batch = Weight::from_parts(3, 0);
		assert_eq!(
			strategy.select_nonces_to_deliver(state).await,
			Some(((20..=22), proof_parameters(false, 3)))
		);
	}

	#[async_std::test]
	async fn message_delivery_strategy_accepts_single_message_even_if_its_weight_overflows_maximal_weight(
	) {
		let (state, mut strategy) = prepare_strategy();

		// first message doesn't fit in the batch, because it has weight (10) that overflows max
		// weight (4)
		strategy.strategy.source_queue_mut()[0].1.get_mut(&20).unwrap().dispatch_weight =
			Weight::from_parts(10, 0);
		assert_eq!(
			strategy.select_nonces_to_deliver(state).await,
			Some(((20..=20), proof_parameters(false, 10)))
		);
	}

	#[async_std::test]
	async fn message_delivery_strategy_limits_batch_by_messages_size() {
		let (state, mut strategy) = prepare_strategy();

		// not all queued messages may fit in the batch, because batch has max weight
		strategy.max_messages_size_in_single_batch = 3;
		assert_eq!(
			strategy.select_nonces_to_deliver(state).await,
			Some(((20..=22), proof_parameters(false, 3)))
		);
	}

	#[async_std::test]
	async fn message_delivery_strategy_accepts_single_message_even_if_its_weight_overflows_maximal_size(
	) {
		let (state, mut strategy) = prepare_strategy();

		// first message doesn't fit in the batch, because it has weight (10) that overflows max
		// weight (4)
		strategy.strategy.source_queue_mut()[0].1.get_mut(&20).unwrap().size = 10;
		assert_eq!(
			strategy.select_nonces_to_deliver(state).await,
			Some(((20..=20), proof_parameters(false, 1)))
		);
	}

	#[async_std::test]
	async fn message_delivery_strategy_limits_batch_by_messages_count_when_there_is_upper_limit() {
		let (state, mut strategy) = prepare_strategy();

		// not all queued messages may fit in the batch, because batch has max number of messages
		// limit
		strategy.max_messages_in_single_batch = 3;
		assert_eq!(
			strategy.select_nonces_to_deliver(state).await,
			Some(((20..=22), proof_parameters(false, 3)))
		);
	}

	#[async_std::test]
	async fn message_delivery_strategy_limits_batch_by_messages_count_when_there_are_unconfirmed_nonces(
	) {
		let (state, mut strategy) = prepare_strategy();

		// 1 delivery confirmation from target to source is still missing, so we may only
		// relay 3 new messages
		let prev_confirmed_nonce_at_source =
			strategy.latest_confirmed_nonces_at_source.back().unwrap().1;
		strategy.latest_confirmed_nonces_at_source =
			vec![(header_id(1), prev_confirmed_nonce_at_source - 1)].into_iter().collect();
		strategy.target_nonces.as_mut().unwrap().nonces_data.confirmed_nonce =
			prev_confirmed_nonce_at_source - 1;
		assert_eq!(
			strategy.select_nonces_to_deliver(state).await,
			Some(((20..=22), proof_parameters(false, 3)))
		);
	}

	#[async_std::test]
	async fn message_delivery_strategy_waits_for_confirmed_nonce_header_to_appear_on_target() {
		// 1 delivery confirmation from target to source is still missing, so we may deliver
		// reward confirmation with our message delivery transaction. But the problem is that
		// the reward has been paid at header 2 && this header is still unknown to target node.
		//
		// => so we can't deliver more than 3 messages
		let (mut state, mut strategy) = prepare_strategy();
		let prev_confirmed_nonce_at_source =
			strategy.latest_confirmed_nonces_at_source.back().unwrap().1;
		strategy.latest_confirmed_nonces_at_source = vec![
			(header_id(1), prev_confirmed_nonce_at_source - 1),
			(header_id(2), prev_confirmed_nonce_at_source),
		]
		.into_iter()
		.collect();
		strategy.target_nonces.as_mut().unwrap().nonces_data.confirmed_nonce =
			prev_confirmed_nonce_at_source - 1;
		state.best_finalized_source_header_id_at_best_target = Some(header_id(1));
		assert_eq!(
			strategy.select_nonces_to_deliver(state).await,
			Some(((20..=22), proof_parameters(false, 3)))
		);

		// the same situation, but the header 2 is known to the target node, so we may deliver
		// reward confirmation
		let (mut state, mut strategy) = prepare_strategy();
		let prev_confirmed_nonce_at_source =
			strategy.latest_confirmed_nonces_at_source.back().unwrap().1;
		strategy.latest_confirmed_nonces_at_source = vec![
			(header_id(1), prev_confirmed_nonce_at_source - 1),
			(header_id(2), prev_confirmed_nonce_at_source),
		]
		.into_iter()
		.collect();
		strategy.target_nonces.as_mut().unwrap().nonces_data.confirmed_nonce =
			prev_confirmed_nonce_at_source - 1;
		state.best_finalized_source_header_id_at_source = Some(header_id(2));
		state.best_finalized_source_header_id_at_best_target = Some(header_id(2));
		assert_eq!(
			strategy.select_nonces_to_deliver(state).await,
			Some(((20..=23), proof_parameters(true, 4)))
		);
	}

	#[async_std::test]
	async fn source_header_is_required_when_confirmations_are_required() {
		// let's prepare situation when:
		// - all messages [20; 23] have been generated at source block#1;
		let (mut state, mut strategy) = prepare_strategy();
		//
		// - messages [20; 23] have been delivered
		assert_eq!(
			strategy.select_nonces_to_deliver(state.clone()).await,
			Some(((20..=23), proof_parameters(false, 4)))
		);
		strategy.finalized_target_nonces_updated(
			TargetClientNonces {
				latest_nonce: 23,
				nonces_data: DeliveryRaceTargetNoncesData {
					confirmed_nonce: 19,
					unrewarded_relayers: UnrewardedRelayersState {
						unrewarded_relayer_entries: 1,
						messages_in_oldest_entry: 4,
						total_messages: 4,
						last_delivered_nonce: 23,
					},
				},
			},
			&mut state,
		);
		// nothing needs to be delivered now and we don't need any new headers
		assert_eq!(strategy.select_nonces_to_deliver(state.clone()).await, None);
		assert_eq!(strategy.required_source_header_at_target(state.clone()).await, None);

		// block#2 is generated
		state.best_finalized_source_header_id_at_source = Some(header_id(2));
		state.best_finalized_source_header_id_at_best_target = Some(header_id(2));
		state.best_target_header_id = Some(header_id(2));
		state.best_finalized_target_header_id = Some(header_id(2));

		// now let's generate two more nonces [24; 25] at the source;
		strategy.source_nonces_updated(header_id(2), source_nonces(24..=25, 19, 0));
		//
		// we don't need to relay more headers to target, because messages [20; 23] have
		// not confirmed to source yet
		assert_eq!(strategy.select_nonces_to_deliver(state.clone()).await, None);
		assert_eq!(strategy.required_source_header_at_target(state.clone()).await, None);

		// let's relay source block#3
		state.best_finalized_source_header_id_at_source = Some(header_id(3));
		state.best_finalized_source_header_id_at_best_target = Some(header_id(3));
		state.best_target_header_id = Some(header_id(3));
		state.best_finalized_target_header_id = Some(header_id(3));

		// and ask strategy again => still nothing to deliver, because parallel confirmations
		// race need to be pushed further
		assert_eq!(strategy.select_nonces_to_deliver(state.clone()).await, None);
		assert_eq!(strategy.required_source_header_at_target(state.clone()).await, None);

		// let's relay source block#3
		state.best_finalized_source_header_id_at_source = Some(header_id(4));
		state.best_finalized_source_header_id_at_best_target = Some(header_id(4));
		state.best_target_header_id = Some(header_id(4));
		state.best_finalized_target_header_id = Some(header_id(4));

		// let's confirm messages [20; 23]
		strategy.source_nonces_updated(header_id(4), source_nonces(24..=25, 23, 0));

		// and ask strategy again => now we have everything required to deliver remaining
		// [24; 25] nonces and proof of [20; 23] confirmation
		assert_eq!(
			strategy.select_nonces_to_deliver(state.clone()).await,
			Some(((24..=25), proof_parameters(true, 2))),
		);
		assert_eq!(strategy.required_source_header_at_target(state).await, None);
	}

	#[async_std::test]
	async fn relayer_uses_flattened_view_of_the_source_queue_to_select_nonces() {
		// Real scenario that has happened on test deployments:
		// 1) relayer witnessed M1 at block 1 => it has separate entry in the `source_queue`
		// 2) relayer witnessed M2 at block 2 => it has separate entry in the `source_queue`
		// 3) if block 2 is known to the target node, then both M1 and M2 are selected for single
		// delivery,    even though weight(M1+M2) > larger than largest allowed weight
		//
		// This was happening because selector (`select_nonces_for_delivery_transaction`) has been
		// called for every `source_queue` entry separately without preserving any context.
		let (mut state, mut strategy) = prepare_strategy();
		let nonces = source_nonces(24..=25, 19, 0);
		strategy.strategy.source_nonces_updated(header_id(2), nonces);
		strategy.max_unrewarded_relayer_entries_at_target = 100;
		strategy.max_unconfirmed_nonces_at_target = 100;
		strategy.max_messages_in_single_batch = 5;
		strategy.max_messages_weight_in_single_batch = Weight::from_parts(100, 0);
		strategy.max_messages_size_in_single_batch = 100;
		state.best_finalized_source_header_id_at_best_target = Some(header_id(2));

		assert_eq!(
			strategy.select_nonces_to_deliver(state).await,
			Some(((20..=24), proof_parameters(false, 5)))
		);
	}

	#[async_std::test]
	#[allow(clippy::reversed_empty_ranges)]
	async fn no_source_headers_required_at_target_if_lanes_are_empty() {
		let (state, _) = prepare_strategy();
		let mut strategy = TestStrategy {
			max_unrewarded_relayer_entries_at_target: 4,
			max_unconfirmed_nonces_at_target: 4,
			max_messages_in_single_batch: 4,
			max_messages_weight_in_single_batch: Weight::from_parts(4, 0),
			max_messages_size_in_single_batch: 4,
			latest_confirmed_nonces_at_source: VecDeque::new(),
			lane_source_client: TestSourceClient::default(),
			lane_target_client: TestTargetClient::default(),
			metrics_msg: None,
			target_nonces: None,
			strategy: BasicStrategy::new(),
		};

		let source_header_id = header_id(10);
		strategy.source_nonces_updated(
			source_header_id,
			// MessageDeliveryRaceSource::nonces returns Some(0), because that's how it is
			// represented in memory (there's no Options in OutboundLaneState)
			source_nonces(1u64..=0u64, 0, 0),
		);

		// even though `latest_confirmed_nonces_at_source` is not empty, new headers are not
		// requested
		assert_eq!(
			strategy.latest_confirmed_nonces_at_source,
			VecDeque::from([(source_header_id, 0)])
		);
		assert_eq!(strategy.required_source_header_at_target(state).await, None);
	}

	#[async_std::test]
	async fn previous_nonces_are_selected_if_reorg_happens_at_target_chain() {
		// this is the copy of the similar test in the `mesage_race_strategy.rs`, but it also tests
		// that the `MessageDeliveryStrategy` acts properly in the similar scenario

		// tune parameters to allow 5 nonces per delivery transaction
		let (mut state, mut strategy) = prepare_strategy();
		strategy.max_unrewarded_relayer_entries_at_target = 5;
		strategy.max_unconfirmed_nonces_at_target = 5;
		strategy.max_messages_in_single_batch = 5;
		strategy.max_messages_weight_in_single_batch = Weight::from_parts(5, 0);
		strategy.max_messages_size_in_single_batch = 5;

		// in this state we have 4 available nonces for delivery
		assert_eq!(
			strategy.select_nonces_to_deliver(state.clone()).await,
			Some((
				20..=23,
				MessageProofParameters {
					outbound_state_proof_required: false,
					dispatch_weight: Weight::from_parts(4, 0),
				}
			)),
		);

		// let's say we have submitted 20..=23
		state.nonces_submitted = Some(20..=23);

		// then new nonce 24 appear at the source block 2
		let new_nonce_24 = vec![(
			24,
			MessageDetails { dispatch_weight: Weight::from_parts(1, 0), size: 0, reward: 0 },
		)]
		.into_iter()
		.collect();
		let source_header_2 = header_id(2);
		state.best_finalized_source_header_id_at_source = Some(source_header_2);
		strategy.source_nonces_updated(
			source_header_2,
			SourceClientNonces { new_nonces: new_nonce_24, confirmed_nonce: None },
		);
		// and nonce 23 appear at the best block of the target node (best finalized still has 0
		// nonces)
		let target_nonces_data = DeliveryRaceTargetNoncesData {
			confirmed_nonce: 19,
			unrewarded_relayers: UnrewardedRelayersState::default(),
		};
		let target_header_2 = header_id(2);
		state.best_target_header_id = Some(target_header_2);
		strategy.best_target_nonces_updated(
			TargetClientNonces { latest_nonce: 23, nonces_data: target_nonces_data.clone() },
			&mut state,
		);

		// then best target header is retracted
		strategy.best_target_nonces_updated(
			TargetClientNonces { latest_nonce: 19, nonces_data: target_nonces_data.clone() },
			&mut state,
		);

		// ... and some fork with 19 delivered nonces is finalized
		let target_header_2_fork = header_id(2_1);
		state.best_finalized_source_header_id_at_source = Some(source_header_2);
		state.best_finalized_source_header_id_at_best_target = Some(source_header_2);
		state.best_target_header_id = Some(target_header_2_fork);
		state.best_finalized_target_header_id = Some(target_header_2_fork);
		strategy.finalized_target_nonces_updated(
			TargetClientNonces { latest_nonce: 19, nonces_data: target_nonces_data.clone() },
			&mut state,
		);

		// now we have to select nonces 20..=23 for delivery again
		assert_eq!(
			strategy.select_nonces_to_deliver(state.clone()).await,
			Some((
				20..=24,
				MessageProofParameters {
					outbound_state_proof_required: false,
					dispatch_weight: Weight::from_parts(5, 0),
				}
			)),
		);
	}

	#[async_std::test]
	#[allow(clippy::reversed_empty_ranges)]
	async fn delivery_race_is_able_to_unblock_lane() {
		// step 1: messages 20..=23 are delivered from source to target at target block 2
		fn at_target_block_2_deliver_messages(
			strategy: &mut TestStrategy,
			state: &mut TestRaceState,
			occupied_relayer_slots: MessageNonce,
			occupied_message_slots: MessageNonce,
		) {
			let nonces_at_target = TargetClientNonces {
				latest_nonce: 23,
				nonces_data: DeliveryRaceTargetNoncesData {
					confirmed_nonce: 19,
					unrewarded_relayers: UnrewardedRelayersState {
						unrewarded_relayer_entries: occupied_relayer_slots,
						total_messages: occupied_message_slots,
						..Default::default()
					},
				},
			};

			state.best_target_header_id = Some(header_id(2));
			state.best_finalized_target_header_id = Some(header_id(2));

			strategy.best_target_nonces_updated(nonces_at_target.clone(), state);
			strategy.finalized_target_nonces_updated(nonces_at_target, state);
		}

		// step 2: delivery of messages 20..=23 is confirmed to the source node at source block 2
		fn at_source_block_2_deliver_confirmations(
			strategy: &mut TestStrategy,
			state: &mut TestRaceState,
		) {
			state.best_finalized_source_header_id_at_source = Some(header_id(2));

			strategy.source_nonces_updated(
				header_id(2),
				SourceClientNonces { new_nonces: Default::default(), confirmed_nonce: Some(23) },
			);
		}

		// step 3: finalize source block 2 at target block 3 and select nonces to deliver
		async fn at_target_block_3_select_nonces_to_deliver(
			strategy: &TestStrategy,
			mut state: TestRaceState,
		) -> Option<(RangeInclusive<MessageNonce>, MessageProofParameters)> {
			state.best_finalized_source_header_id_at_best_target = Some(header_id(2));
			state.best_target_header_id = Some(header_id(3));
			state.best_finalized_target_header_id = Some(header_id(3));

			strategy.select_nonces_to_deliver(state).await
		}

		let max_unrewarded_relayer_entries_at_target = 4;
		let max_unconfirmed_nonces_at_target = 4;
		let expected_rewards_proof = Some((
			1..=0,
			MessageProofParameters {
				outbound_state_proof_required: true,
				dispatch_weight: Weight::zero(),
			},
		));

		// when lane is NOT blocked
		let (mut state, mut strategy) = prepare_strategy();
		at_target_block_2_deliver_messages(
			&mut strategy,
			&mut state,
			max_unrewarded_relayer_entries_at_target - 1,
			max_unconfirmed_nonces_at_target - 1,
		);
		at_source_block_2_deliver_confirmations(&mut strategy, &mut state);
		assert_eq!(strategy.required_source_header_at_target(state.clone()).await, None);
		assert_eq!(at_target_block_3_select_nonces_to_deliver(&strategy, state).await, None);

		// when lane is blocked by no-relayer-slots in unrewarded relayers vector
		let (mut state, mut strategy) = prepare_strategy();
		at_target_block_2_deliver_messages(
			&mut strategy,
			&mut state,
			max_unrewarded_relayer_entries_at_target,
			max_unconfirmed_nonces_at_target - 1,
		);
		at_source_block_2_deliver_confirmations(&mut strategy, &mut state);
		assert_eq!(
			strategy.required_source_header_at_target(state.clone()).await,
			Some(header_id(2))
		);
		assert_eq!(
			at_target_block_3_select_nonces_to_deliver(&strategy, state).await,
			expected_rewards_proof
		);

		// when lane is blocked by no-message-slots in unrewarded relayers vector
		let (mut state, mut strategy) = prepare_strategy();
		at_target_block_2_deliver_messages(
			&mut strategy,
			&mut state,
			max_unrewarded_relayer_entries_at_target - 1,
			max_unconfirmed_nonces_at_target,
		);
		at_source_block_2_deliver_confirmations(&mut strategy, &mut state);
		assert_eq!(
			strategy.required_source_header_at_target(state.clone()).await,
			Some(header_id(2))
		);
		assert_eq!(
			at_target_block_3_select_nonces_to_deliver(&strategy, state).await,
			expected_rewards_proof
		);

		// when lane is blocked by no-message-slots and no-message-slots in unrewarded relayers
		// vector
		let (mut state, mut strategy) = prepare_strategy();
		at_target_block_2_deliver_messages(
			&mut strategy,
			&mut state,
			max_unrewarded_relayer_entries_at_target - 1,
			max_unconfirmed_nonces_at_target,
		);
		at_source_block_2_deliver_confirmations(&mut strategy, &mut state);
		assert_eq!(
			strategy.required_source_header_at_target(state.clone()).await,
			Some(header_id(2))
		);
		assert_eq!(
			at_target_block_3_select_nonces_to_deliver(&strategy, state).await,
			expected_rewards_proof
		);

		// when we have already selected some nonces to deliver, we don't need to select anything
		let (mut state, mut strategy) = prepare_strategy();
		at_target_block_2_deliver_messages(
			&mut strategy,
			&mut state,
			max_unrewarded_relayer_entries_at_target - 1,
			max_unconfirmed_nonces_at_target,
		);
		at_source_block_2_deliver_confirmations(&mut strategy, &mut state);
		state.nonces_to_submit = Some((header_id(2), 1..=0, (1..=0, None)));
		assert_eq!(strategy.required_source_header_at_target(state.clone()).await, None);
		assert_eq!(at_target_block_3_select_nonces_to_deliver(&strategy, state).await, None);

		// when we have already submitted some nonces, we don't need to select anything
		let (mut state, mut strategy) = prepare_strategy();
		at_target_block_2_deliver_messages(
			&mut strategy,
			&mut state,
			max_unrewarded_relayer_entries_at_target - 1,
			max_unconfirmed_nonces_at_target,
		);
		at_source_block_2_deliver_confirmations(&mut strategy, &mut state);
		state.nonces_submitted = Some(1..=0);
		assert_eq!(strategy.required_source_header_at_target(state.clone()).await, None);
		assert_eq!(at_target_block_3_select_nonces_to_deliver(&strategy, state).await, None);
	}

	#[async_std::test]
	async fn outbound_state_proof_is_not_required_when_we_have_no_new_confirmations() {
		let (mut state, mut strategy) = prepare_strategy();

		// pretend that we haven't seen any confirmations yet (or they're at the future target chain
		// blocks)
		strategy.latest_confirmed_nonces_at_source.clear();

		// emulate delivery of some nonces (20..=23 are generated, but we only deliver 20..=21)
		let nonces_at_target = TargetClientNonces {
			latest_nonce: 21,
			nonces_data: DeliveryRaceTargetNoncesData {
				confirmed_nonce: 19,
				unrewarded_relayers: UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					total_messages: 2,
					..Default::default()
				},
			},
		};
		state.best_target_header_id = Some(header_id(2));
		state.best_finalized_target_header_id = Some(header_id(2));
		strategy.best_target_nonces_updated(nonces_at_target.clone(), &mut state);
		strategy.finalized_target_nonces_updated(nonces_at_target, &mut state);

		// we won't include outbound lane state proof into 22..=23 delivery transaction
		// because it brings no new reward confirmations
		assert_eq!(
			strategy.select_nonces_to_deliver(state).await,
			Some(((22..=23), proof_parameters(false, 2)))
		);
	}
}
