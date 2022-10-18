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

//! Relayer strategy

use async_trait::async_trait;
use bp_messages::{MessageNonce, Weight};
use sp_arithmetic::traits::Saturating;
use std::ops::Range;

use crate::{
	message_lane::MessageLane,
	message_lane_loop::{
		MessageDetails, MessageDetailsMap, SourceClient as MessageLaneSourceClient,
		TargetClient as MessageLaneTargetClient,
	},
	message_race_strategy::SourceRangesQueue,
	metrics::MessageLaneLoopMetrics,
};

pub(crate) use self::enforcement_strategy::*;
pub use self::{altruistic_strategy::*, mix_strategy::*, rational_strategy::*};

mod altruistic_strategy;
mod enforcement_strategy;
mod mix_strategy;
mod rational_strategy;

/// Relayer strategy trait
#[async_trait]
pub trait RelayStrategy: 'static + Clone + Send + Sync {
	/// The relayer decide how to process nonce by reference.
	/// From given set of source nonces, that are ready to be delivered, select nonces
	/// to fit into single delivery transaction.
	///
	/// The function returns last nonce that must be delivered to the target chain.
	async fn decide<
		P: MessageLane,
		SourceClient: MessageLaneSourceClient<P>,
		TargetClient: MessageLaneTargetClient<P>,
	>(
		&mut self,
		reference: &mut RelayReference<P, SourceClient, TargetClient>,
	) -> bool;

	/// Notification that the following maximal nonce has been selected for the delivery.
	fn on_final_decision<
		P: MessageLane,
		SourceClient: MessageLaneSourceClient<P>,
		TargetClient: MessageLaneTargetClient<P>,
	>(
		&self,
		reference: &RelayReference<P, SourceClient, TargetClient>,
	);
}

/// Total cost of mesage delivery and confirmation.
struct MessagesDeliveryCost<SourceChainBalance> {
	/// Cost of message delivery transaction.
	pub delivery_transaction_cost: SourceChainBalance,
	/// Cost of confirmation delivery transaction.
	pub confirmation_transaction_cost: SourceChainBalance,
}

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
	/// Current block reward summary
	pub selected_reward: P::SourceChainBalance,
	/// Current block cost summary
	pub selected_cost: P::SourceChainBalance,
	/// Messages size summary
	pub selected_size: u32,

	/// Current block reward summary
	pub total_reward: P::SourceChainBalance,
	/// All confirmations cost
	pub total_confirmations_cost: P::SourceChainBalance,
	/// Current block cost summary
	pub total_cost: P::SourceChainBalance,

	/// Hard check begin nonce
	pub hard_selected_begin_nonce: MessageNonce,
	/// Count prepaid nonces
	pub selected_prepaid_nonces: MessageNonce,
	/// Unpaid nonces weight summary
	pub selected_unpaid_weight: Weight,

	/// Index by all ready nonces
	pub index: usize,
	/// Current nonce
	pub nonce: MessageNonce,
	/// Current nonce details
	pub details: MessageDetails<P::SourceChainBalance>,
}

impl<
		P: MessageLane,
		SourceClient: MessageLaneSourceClient<P>,
		TargetClient: MessageLaneTargetClient<P>,
	> RelayReference<P, SourceClient, TargetClient>
{
	/// Returns whether the current `RelayReference` is profitable.
	pub fn is_profitable(&self) -> bool {
		self.total_reward >= self.total_cost
	}

	async fn estimate_messages_delivery_cost(
		&self,
	) -> Result<MessagesDeliveryCost<P::SourceChainBalance>, TargetClient::Error> {
		// technically, multiple confirmations will be delivered in a single transaction,
		// meaning less loses for relayer. But here we don't know the final relayer yet, so
		// we're adding a separate transaction for every message. Normally, this cost is covered
		// by the message sender. Probably reconsider this?
		let confirmation_transaction_cost =
			self.lane_source_client.estimate_confirmation_transaction().await;

		let delivery_transaction_cost = self
			.lane_target_client
			.estimate_delivery_transaction_in_source_tokens(
				self.hard_selected_begin_nonce..=
					(self.hard_selected_begin_nonce + self.index as MessageNonce),
				self.selected_prepaid_nonces,
				self.selected_unpaid_weight,
				self.selected_size,
			)
			.await?;

		Ok(MessagesDeliveryCost { confirmation_transaction_cost, delivery_transaction_cost })
	}

	async fn update_cost_and_reward(&mut self) -> Result<(), TargetClient::Error> {
		let prev_is_profitable = self.is_profitable();
		let prev_total_cost = self.total_cost;
		let prev_total_reward = self.total_reward;

		let MessagesDeliveryCost { confirmation_transaction_cost, delivery_transaction_cost } =
			self.estimate_messages_delivery_cost().await?;
		self.total_confirmations_cost =
			self.total_confirmations_cost.saturating_add(confirmation_transaction_cost);
		self.total_reward = self.total_reward.saturating_add(self.details.reward);
		self.total_cost = self.total_confirmations_cost.saturating_add(delivery_transaction_cost);

		if prev_is_profitable && !self.is_profitable() {
			// if it is the first message that makes reward less than cost, let's log it
			log::debug!(
				target: "bridge",
				"Message with nonce {} (reward = {:?}) changes total cost {:?}->{:?} and makes it larger than \
				total reward {:?}->{:?}",
				self.nonce,
				self.details.reward,
				prev_total_cost,
				self.total_cost,
				prev_total_reward,
				self.total_reward,
			);
		} else if !prev_is_profitable && self.is_profitable() {
			// if this message makes batch profitable again, let's log it
			log::debug!(
				target: "bridge",
				"Message with nonce {} (reward = {:?}) changes total cost {:?}->{:?} and makes it less than or \
				equal to the total reward {:?}->{:?} (again)",
				self.nonce,
				self.details.reward,
				prev_total_cost,
				self.total_cost,
				prev_total_reward,
				self.total_reward,
			);
		}

		Ok(())
	}
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
	/// Source queue.
	pub nonces_queue: SourceRangesQueue<
		P::SourceHeaderHash,
		P::SourceHeaderNumber,
		MessageDetailsMap<P::SourceChainBalance>,
	>,
	/// Source queue range
	pub nonces_queue_range: Range<usize>,
}
