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

//! enforcement strategy

use num_traits::Zero;
use std::ops::RangeInclusive;

use bp_messages::{MessageNonce, Weight};

use crate::{
	message_lane::MessageLane,
	message_lane_loop::{
		MessageDetails, MessageDetailsMap, SourceClient as MessageLaneSourceClient,
		TargetClient as MessageLaneTargetClient,
	},
	message_race_loop::NoncesRange,
	message_race_strategy::SourceRangesQueue,
	metrics::MessageLaneLoopMetrics,
};

/// Reference data for participating in relay
pub struct RelayReference<
	P: MessageLane,
	SourceClient: MessageLaneSourceClient<P>,
	TargetClient: MessageLaneTargetClient<P>,
> {
	/// The client that is connected to the message lane source node.
	pub lane_source_client: SourceClient,
	/// The client that is connected to the message lane target node.
	pub lane_target_client: TargetClient,
	/// Metrics reference.
	pub metrics: Option<MessageLaneLoopMetrics>,
	/// Messages size summary
	pub selected_size: u32,

	/// Hard check begin nonce
	pub hard_selected_begin_nonce: MessageNonce,

	/// Index by all ready nonces
	pub index: usize,
	/// Current nonce
	pub nonce: MessageNonce,
	/// Current nonce details
	pub details: MessageDetails<P::SourceChainBalance>,
}

/// Relay reference data
pub struct RelayMessagesBatchReference<
	P: MessageLane,
	SourceClient: MessageLaneSourceClient<P>,
	TargetClient: MessageLaneTargetClient<P>,
> {
	/// Maximal number of relayed messages in single delivery transaction.
	pub max_messages_in_this_batch: MessageNonce,
	/// Maximal cumulative dispatch weight of relayed messages in single delivery transaction.
	pub max_messages_weight_in_single_batch: Weight,
	/// Maximal cumulative size of relayed messages in single delivery transaction.
	pub max_messages_size_in_single_batch: u32,
	/// The client that is connected to the message lane source node.
	pub lane_source_client: SourceClient,
	/// The client that is connected to the message lane target node.
	pub lane_target_client: TargetClient,
	/// Metrics reference.
	pub metrics: Option<MessageLaneLoopMetrics>,
	/// Best available nonce at the **best** target block. We do not want to deliver nonces
	/// less than this nonce, even though the block may be retracted.
	pub best_target_nonce: MessageNonce,
	/// Source queue.
	pub nonces_queue: SourceRangesQueue<
		P::SourceHeaderHash,
		P::SourceHeaderNumber,
		MessageDetailsMap<P::SourceChainBalance>,
	>,
	/// Range of indices within the `nonces_queue` that are available for selection.
	pub nonces_queue_range: RangeInclusive<usize>,
}

/// Limits of the message race transactions.
#[derive(Clone)]
pub struct MessageRaceLimits;

impl MessageRaceLimits {
	pub async fn decide<
		P: MessageLane,
		SourceClient: MessageLaneSourceClient<P>,
		TargetClient: MessageLaneTargetClient<P>,
	>(
		reference: RelayMessagesBatchReference<P, SourceClient, TargetClient>,
	) -> Option<RangeInclusive<MessageNonce>> {
		let mut hard_selected_count = 0;

		let mut selected_weight = Weight::zero();
		let mut selected_count: MessageNonce = 0;

		let hard_selected_begin_nonce = std::cmp::max(
			reference.best_target_nonce + 1,
			reference.nonces_queue[*reference.nonces_queue_range.start()].1.begin(),
		);

		// relay reference
		let mut relay_reference = RelayReference {
			lane_source_client: reference.lane_source_client.clone(),
			lane_target_client: reference.lane_target_client.clone(),
			metrics: reference.metrics.clone(),

			selected_size: 0,

			hard_selected_begin_nonce,

			index: 0,
			nonce: 0,
			details: MessageDetails {
				dispatch_weight: Weight::zero(),
				size: 0,
				reward: P::SourceChainBalance::zero(),
			},
		};

		let all_ready_nonces = reference
			.nonces_queue
			.range(reference.nonces_queue_range.clone())
			.flat_map(|(_, ready_nonces)| ready_nonces.iter())
			.filter(|(nonce, _)| **nonce >= hard_selected_begin_nonce)
			.enumerate();
		for (index, (nonce, details)) in all_ready_nonces {
			relay_reference.index = index;
			relay_reference.nonce = *nonce;
			relay_reference.details = *details;

			// Since we (hopefully) have some reserves in `max_messages_weight_in_single_batch`
			// and `max_messages_size_in_single_batch`, we may still try to submit transaction
			// with single message if message overflows these limits. The worst case would be if
			// transaction will be rejected by the target runtime, but at least we have tried.

			// limit messages in the batch by weight
			let new_selected_weight = match selected_weight.checked_add(&details.dispatch_weight) {
				Some(new_selected_weight)
					if new_selected_weight
						.all_lte(reference.max_messages_weight_in_single_batch) =>
					new_selected_weight,
				new_selected_weight if selected_count == 0 => {
					log::warn!(
						target: "bridge",
						"Going to submit message delivery transaction with declared dispatch \
						weight {:?} that overflows maximal configured weight {}",
						new_selected_weight,
						reference.max_messages_weight_in_single_batch,
					);
					new_selected_weight.unwrap_or(Weight::MAX)
				},
				_ => break,
			};

			// limit messages in the batch by size
			let new_selected_size = match relay_reference.selected_size.checked_add(details.size) {
				Some(new_selected_size)
					if new_selected_size <= reference.max_messages_size_in_single_batch =>
					new_selected_size,
				new_selected_size if selected_count == 0 => {
					log::warn!(
						target: "bridge",
						"Going to submit message delivery transaction with message \
						size {:?} that overflows maximal configured size {}",
						new_selected_size,
						reference.max_messages_size_in_single_batch,
					);
					new_selected_size.unwrap_or(u32::MAX)
				},
				_ => break,
			};

			// limit number of messages in the batch
			let new_selected_count = selected_count + 1;
			if new_selected_count > reference.max_messages_in_this_batch {
				break
			}
			relay_reference.selected_size = new_selected_size;

			hard_selected_count = index + 1;
			selected_weight = new_selected_weight;
			selected_count = new_selected_count;
		}

		if hard_selected_count != 0 {
			let selected_max_nonce =
				hard_selected_begin_nonce + hard_selected_count as MessageNonce - 1;
			Some(hard_selected_begin_nonce..=selected_max_nonce)
		} else {
			None
		}
	}
}
