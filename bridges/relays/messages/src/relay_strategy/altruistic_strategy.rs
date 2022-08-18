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

//! Altruistic relay strategy

use async_trait::async_trait;

use crate::{
	message_lane::MessageLane,
	message_lane_loop::{
		SourceClient as MessageLaneSourceClient, TargetClient as MessageLaneTargetClient,
	},
	relay_strategy::{RelayReference, RelayStrategy},
};

/// The relayer doesn't care about rewards.
#[derive(Clone)]
pub struct AltruisticStrategy;

#[async_trait]
impl RelayStrategy for AltruisticStrategy {
	async fn decide<
		P: MessageLane,
		SourceClient: MessageLaneSourceClient<P>,
		TargetClient: MessageLaneTargetClient<P>,
	>(
		&mut self,
		reference: &mut RelayReference<P, SourceClient, TargetClient>,
	) -> bool {
		// We don't care about costs and rewards, but we want to report unprofitable transactions.
		if let Err(e) = reference.update_cost_and_reward().await {
			log::debug!(
				target: "bridge",
				"Failed to update transaction cost and reward: {:?}. \
				The `unprofitable_delivery_transactions` metric will be inaccurate",
				e,
			);
		}

		true
	}

	fn on_final_decision<
		P: MessageLane,
		SourceClient: MessageLaneSourceClient<P>,
		TargetClient: MessageLaneTargetClient<P>,
	>(
		&self,
		reference: &RelayReference<P, SourceClient, TargetClient>,
	) {
		if let Some(ref metrics) = reference.metrics {
			if !reference.is_profitable() {
				log::debug!(
					target: "bridge",
					"The relayer has submitted unprofitable {} -> {} message delivery transaction \
					with {} messages: total cost = {:?}, total reward = {:?}",
					P::SOURCE_NAME,
					P::TARGET_NAME,
					reference.index + 1,
					reference.total_cost,
					reference.total_reward,
				);

				metrics.note_unprofitable_delivery_transactions();
			}
		}
	}
}
