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

//! Message delivery race delivers proof-of-messages from lane.source to lane.target.

use crate::message_lane::{MessageLane, SourceHeaderIdOf, TargetHeaderIdOf};
use crate::message_lane_loop::{
	MessageDeliveryParams, MessageProofParameters, MessageWeightsMap, SourceClient as MessageLaneSourceClient,
	SourceClientState, TargetClient as MessageLaneTargetClient, TargetClientState,
};
use crate::message_race_loop::{
	MessageRace, NoncesRange, RaceState, RaceStrategy, SourceClient, SourceClientNonces, TargetClient,
	TargetClientNonces,
};
use crate::message_race_strategy::BasicStrategy;
use crate::metrics::MessageLaneLoopMetrics;

use async_trait::async_trait;
use bp_message_lane::{MessageNonce, Weight};
use futures::stream::FusedStream;
use relay_utils::FailedClient;
use std::{collections::BTreeMap, marker::PhantomData, ops::RangeInclusive, time::Duration};

/// Run message delivery race.
pub async fn run<P: MessageLane>(
	source_client: impl MessageLaneSourceClient<P>,
	source_state_updates: impl FusedStream<Item = SourceClientState<P>>,
	target_client: impl MessageLaneTargetClient<P>,
	target_state_updates: impl FusedStream<Item = TargetClientState<P>>,
	stall_timeout: Duration,
	metrics_msg: Option<MessageLaneLoopMetrics>,
	params: MessageDeliveryParams,
) -> Result<(), FailedClient> {
	crate::message_race_loop::run(
		MessageDeliveryRaceSource {
			client: source_client,
			metrics_msg: metrics_msg.clone(),
			_phantom: Default::default(),
		},
		source_state_updates,
		MessageDeliveryRaceTarget {
			client: target_client,
			metrics_msg,
			_phantom: Default::default(),
		},
		target_state_updates,
		stall_timeout,
		MessageDeliveryStrategy::<P> {
			max_unconfirmed_nonces_at_target: params.max_unconfirmed_nonces_at_target,
			max_messages_weight_in_single_batch: params.max_messages_weight_in_single_batch,
			latest_confirmed_nonce_at_source: None,
			target_nonces: None,
			strategy: BasicStrategy::new(),
		},
	)
	.await
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
	type NoncesRange = MessageWeightsMap;
	type ProofParameters = MessageProofParameters;

	async fn nonces(
		&self,
		at_block: SourceHeaderIdOf<P>,
		prev_latest_nonce: MessageNonce,
	) -> Result<(SourceHeaderIdOf<P>, SourceClientNonces<Self::NoncesRange>), Self::Error> {
		let (at_block, latest_generated_nonce) = self.client.latest_generated_nonce(at_block).await?;
		let (at_block, latest_confirmed_nonce) = self.client.latest_confirmed_received_nonce(at_block).await?;

		if let Some(metrics_msg) = self.metrics_msg.as_ref() {
			metrics_msg.update_source_latest_generated_nonce::<P>(latest_generated_nonce);
			metrics_msg.update_source_latest_confirmed_nonce::<P>(latest_confirmed_nonce);
		}

		let new_nonces = if latest_generated_nonce > prev_latest_nonce {
			self.client
				.generated_messages_weights(at_block.clone(), prev_latest_nonce + 1..=latest_generated_nonce)
				.await?
		} else {
			MessageWeightsMap::new()
		};

		Ok((
			at_block,
			SourceClientNonces {
				new_nonces,
				confirmed_nonce: Some(latest_confirmed_nonce),
			},
		))
	}

	async fn generate_proof(
		&self,
		at_block: SourceHeaderIdOf<P>,
		nonces: RangeInclusive<MessageNonce>,
		proof_parameters: Self::ProofParameters,
	) -> Result<(SourceHeaderIdOf<P>, RangeInclusive<MessageNonce>, P::MessagesProof), Self::Error> {
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

	async fn nonces(
		&self,
		at_block: TargetHeaderIdOf<P>,
	) -> Result<(TargetHeaderIdOf<P>, TargetClientNonces), Self::Error> {
		let (at_block, latest_received_nonce) = self.client.latest_received_nonce(at_block).await?;
		let (at_block, latest_confirmed_nonce) = self.client.latest_confirmed_received_nonce(at_block).await?;

		if let Some(metrics_msg) = self.metrics_msg.as_ref() {
			metrics_msg.update_target_latest_received_nonce::<P>(latest_received_nonce);
			metrics_msg.update_target_latest_confirmed_nonce::<P>(latest_confirmed_nonce);
		}

		Ok((
			at_block,
			TargetClientNonces {
				latest_nonce: latest_received_nonce,
				confirmed_nonce: Some(latest_confirmed_nonce),
			},
		))
	}

	async fn submit_proof(
		&self,
		generated_at_block: SourceHeaderIdOf<P>,
		nonces: RangeInclusive<MessageNonce>,
		proof: P::MessagesProof,
	) -> Result<RangeInclusive<MessageNonce>, Self::Error> {
		self.client
			.submit_messages_proof(generated_at_block, nonces, proof)
			.await
	}
}

/// Messages delivery strategy.
struct MessageDeliveryStrategy<P: MessageLane> {
	/// Maximal unconfirmed nonces at target client.
	max_unconfirmed_nonces_at_target: MessageNonce,
	/// Maximal cumulative messages weight in the single delivery transaction.
	max_messages_weight_in_single_batch: Weight,
	/// Latest confirmed nonce at the source client.
	latest_confirmed_nonce_at_source: Option<MessageNonce>,
	/// Target nonces from the source client.
	target_nonces: Option<TargetClientNonces>,
	/// Basic delivery strategy.
	strategy: MessageDeliveryStrategyBase<P>,
}

type MessageDeliveryStrategyBase<P> = BasicStrategy<
	<P as MessageLane>::SourceHeaderNumber,
	<P as MessageLane>::SourceHeaderHash,
	<P as MessageLane>::TargetHeaderNumber,
	<P as MessageLane>::TargetHeaderHash,
	MessageWeightsMap,
	<P as MessageLane>::MessagesProof,
>;

impl<P: MessageLane> RaceStrategy<SourceHeaderIdOf<P>, TargetHeaderIdOf<P>, P::MessagesProof>
	for MessageDeliveryStrategy<P>
{
	type SourceNoncesRange = MessageWeightsMap;
	type ProofParameters = MessageProofParameters;

	fn is_empty(&self) -> bool {
		self.strategy.is_empty()
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
		self.latest_confirmed_nonce_at_source = nonces.confirmed_nonce;
		self.strategy.source_nonces_updated(at_block, nonces)
	}

	fn target_nonces_updated(
		&mut self,
		nonces: TargetClientNonces,
		race_state: &mut RaceState<SourceHeaderIdOf<P>, TargetHeaderIdOf<P>, P::MessagesProof>,
	) {
		self.target_nonces = Some(nonces.clone());
		self.strategy.target_nonces_updated(nonces, race_state)
	}

	fn select_nonces_to_deliver(
		&mut self,
		race_state: &RaceState<SourceHeaderIdOf<P>, TargetHeaderIdOf<P>, P::MessagesProof>,
	) -> Option<(RangeInclusive<MessageNonce>, Self::ProofParameters)> {
		const CONFIRMED_NONCE_PROOF: &str = "\
			ClientNonces are crafted by MessageDeliveryRace(Source|Target);\
			MessageDeliveryRace(Source|Target) always fills confirmed_nonce field;\
			qed";

		let latest_confirmed_nonce_at_source = self.latest_confirmed_nonce_at_source?;
		let target_nonces = self.target_nonces.as_ref()?;

		// There's additional condition in the message delivery race: target would reject messages
		// if there are too much unconfirmed messages at the inbound lane.

		// The receiving race is responsible to deliver confirmations back to the source chain. So if
		// there's a lot of unconfirmed messages, let's wait until it'll be able to do its job.
		let latest_received_nonce_at_target = target_nonces.latest_nonce;
		let confirmations_missing = latest_received_nonce_at_target.checked_sub(latest_confirmed_nonce_at_source);
		match confirmations_missing {
			Some(confirmations_missing) if confirmations_missing >= self.max_unconfirmed_nonces_at_target => {
				log::debug!(
					target: "bridge",
					"Cannot deliver any more messages from {} to {}. Too many unconfirmed nonces \
					at target: target.latest_received={:?}, source.latest_confirmed={:?}, max={:?}",
					MessageDeliveryRace::<P>::source_name(),
					MessageDeliveryRace::<P>::target_name(),
					latest_received_nonce_at_target,
					latest_confirmed_nonce_at_source,
					self.max_unconfirmed_nonces_at_target,
				);

				return None;
			}
			_ => (),
		}

		// Ok - we may have new nonces to deliver. But target may still reject new messages, because we haven't
		// notified it that (some) messages have been confirmed. So we may want to include updated
		// `source.latest_confirmed` in the proof.
		//
		// Important note: we're including outbound state lane proof whenever there are unconfirmed nonces
		// on the target chain. Other strategy is to include it only if it's absolutely necessary.
		let latest_confirmed_nonce_at_target = target_nonces.confirmed_nonce.expect(CONFIRMED_NONCE_PROOF);
		let outbound_state_proof_required = latest_confirmed_nonce_at_target < latest_confirmed_nonce_at_source;

		// If we're here, then the confirmations race did its job && sending side now knows that messages
		// have been delivered. Now let's select nonces that we want to deliver.
		//
		// We may deliver at most:
		//
		// max_unconfirmed_nonces_at_target - (latest_received_nonce_at_target - latest_confirmed_nonce_at_target)
		//
		// messages in the batch. But since we're including outbound state proof in the batch, then it
		// may be increased to:
		//
		// max_unconfirmed_nonces_at_target - (latest_received_nonce_at_target - latest_confirmed_nonce_at_source)
		let future_confirmed_nonce_at_target = if outbound_state_proof_required {
			latest_confirmed_nonce_at_source
		} else {
			latest_confirmed_nonce_at_target
		};
		let max_nonces = latest_received_nonce_at_target
			.checked_sub(future_confirmed_nonce_at_target)
			.and_then(|diff| self.max_unconfirmed_nonces_at_target.checked_sub(diff))
			.unwrap_or_default();
		let max_messages_weight_in_single_batch = self.max_messages_weight_in_single_batch;
		let mut selected_weight: Weight = 0;
		let mut selected_count: MessageNonce = 0;

		let selected_nonces = self
			.strategy
			.select_nonces_to_deliver_with_selector(race_state, |range| {
				let to_requeue = range
					.into_iter()
					.skip_while(|(_, weight)| {
						// limit messages in the batch by weight
						let new_selected_weight = match selected_weight.checked_add(*weight) {
							Some(new_selected_weight) if new_selected_weight <= max_messages_weight_in_single_batch => {
								new_selected_weight
							}
							_ => return false,
						};

						// limit number of messages in the batch
						let new_selected_count = selected_count + 1;
						if new_selected_count > max_nonces {
							return false;
						}

						selected_weight = new_selected_weight;
						selected_count = new_selected_count;
						true
					})
					.collect::<BTreeMap<_, _>>();
				if to_requeue.is_empty() {
					None
				} else {
					Some(to_requeue)
				}
			})?;

		Some((
			selected_nonces,
			MessageProofParameters {
				outbound_state_proof_required,
				dispatch_weight: selected_weight,
			},
		))
	}
}

impl NoncesRange for MessageWeightsMap {
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
	use super::*;
	use crate::message_lane_loop::tests::{
		header_id, TestMessageLane, TestMessagesProof, TestSourceHeaderId, TestTargetHeaderId,
	};

	type TestRaceState = RaceState<TestSourceHeaderId, TestTargetHeaderId, TestMessagesProof>;
	type TestStrategy = MessageDeliveryStrategy<TestMessageLane>;

	fn prepare_strategy() -> (TestRaceState, TestStrategy) {
		let mut race_state = RaceState {
			best_finalized_source_header_id_at_source: Some(header_id(1)),
			best_finalized_source_header_id_at_best_target: Some(header_id(1)),
			best_target_header_id: Some(header_id(1)),
			nonces_to_submit: None,
			nonces_submitted: None,
		};

		let mut race_strategy = TestStrategy {
			max_unconfirmed_nonces_at_target: 4,
			max_messages_weight_in_single_batch: 4,
			latest_confirmed_nonce_at_source: Some(19),
			target_nonces: Some(TargetClientNonces {
				latest_nonce: 19,
				confirmed_nonce: Some(19),
			}),
			strategy: BasicStrategy::new(),
		};

		race_strategy.strategy.source_nonces_updated(
			header_id(1),
			SourceClientNonces {
				new_nonces: vec![(20, 1), (21, 1), (22, 1), (23, 1)].into_iter().collect(),
				confirmed_nonce: Some(19),
			},
		);
		race_strategy
			.strategy
			.target_nonces_updated(race_strategy.target_nonces.clone().unwrap(), &mut race_state);

		(race_state, race_strategy)
	}

	fn proof_parameters(state_required: bool, weight: Weight) -> MessageProofParameters {
		MessageProofParameters {
			outbound_state_proof_required: state_required,
			dispatch_weight: weight,
		}
	}

	#[test]
	fn weights_map_works_as_nonces_range() {
		fn build_map(range: RangeInclusive<MessageNonce>) -> MessageWeightsMap {
			range.map(|idx| (idx, idx)).collect()
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

	#[test]
	fn message_delivery_strategy_selects_messages_to_deliver() {
		let (state, mut strategy) = prepare_strategy();

		// both sides are ready to relay new messages
		assert_eq!(
			strategy.select_nonces_to_deliver(&state),
			Some(((20..=23), proof_parameters(false, 4)))
		);
	}

	#[test]
	fn message_delivery_strategy_selects_nothing_if_too_many_confirmations_missing() {
		let (state, mut strategy) = prepare_strategy();

		// if there are already `max_unconfirmed_nonces_at_target` messages on target,
		// we need to wait until confirmations will be delivered by receiving race
		strategy.latest_confirmed_nonce_at_source =
			Some(strategy.target_nonces.as_ref().unwrap().latest_nonce - strategy.max_unconfirmed_nonces_at_target);
		assert_eq!(strategy.select_nonces_to_deliver(&state), None);
	}

	#[test]
	fn message_delivery_strategy_includes_outbound_state_proof_when_new_nonces_are_available() {
		let (state, mut strategy) = prepare_strategy();

		// if there are new confirmed nonces on source, we want to relay this information
		// to target to prune rewards queue
		let prev_confirmed_nonce_at_source = strategy.latest_confirmed_nonce_at_source.unwrap();
		strategy.target_nonces.as_mut().unwrap().confirmed_nonce = Some(prev_confirmed_nonce_at_source - 1);
		assert_eq!(
			strategy.select_nonces_to_deliver(&state),
			Some(((20..=23), proof_parameters(true, 4)))
		);
	}

	#[test]
	fn message_delivery_strategy_limits_batch_by_messages_weight() {
		let (state, mut strategy) = prepare_strategy();

		// not all queued messages may fit in the batch, because batch has max weight
		strategy.max_messages_weight_in_single_batch = 3;
		assert_eq!(
			strategy.select_nonces_to_deliver(&state),
			Some(((20..=22), proof_parameters(false, 3)))
		);
	}

	#[test]
	fn message_delivery_strategy_limits_batch_by_messages_count() {
		let (state, mut strategy) = prepare_strategy();

		// 1 delivery confirmation from target to source is still missing, so we may only
		// relay 3 new messages
		let prev_confirmed_nonce_at_source = strategy.latest_confirmed_nonce_at_source.unwrap();
		strategy.latest_confirmed_nonce_at_source = Some(prev_confirmed_nonce_at_source - 1);
		strategy.target_nonces.as_mut().unwrap().confirmed_nonce = Some(prev_confirmed_nonce_at_source - 1);
		assert_eq!(
			strategy.select_nonces_to_deliver(&state),
			Some(((20..=22), proof_parameters(false, 3)))
		);
	}
}
