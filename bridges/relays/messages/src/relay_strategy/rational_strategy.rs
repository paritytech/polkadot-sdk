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

//! Rational relay strategy

use async_trait::async_trait;

use crate::{
	message_lane::MessageLane,
	message_lane_loop::{
		SourceClient as MessageLaneSourceClient, TargetClient as MessageLaneTargetClient,
	},
	relay_strategy::{RelayReference, RelayStrategy},
};

/// The relayer will deliver all messages and confirmations as long as he's not losing any
/// funds.
#[derive(Clone)]
pub struct RationalStrategy;

#[async_trait]
impl RelayStrategy for RationalStrategy {
	async fn decide<
		P: MessageLane,
		SourceClient: MessageLaneSourceClient<P>,
		TargetClient: MessageLaneTargetClient<P>,
	>(
		&mut self,
		reference: &mut RelayReference<P, SourceClient, TargetClient>,
	) -> bool {
		if let Err(e) = reference.update_cost_and_reward().await {
			log::debug!(
				target: "bridge",
				"Failed to update transaction cost and reward: {:?}. No nonces selected for delivery",
				e,
			);

			return false
		}

		// Rational relayer never wants to lose his funds.
		if reference.is_profitable() {
			reference.selected_reward = reference.total_reward;
			reference.selected_cost = reference.total_cost;
			return true
		}

		false
	}

	fn on_final_decision<
		P: MessageLane,
		SourceClient: MessageLaneSourceClient<P>,
		TargetClient: MessageLaneTargetClient<P>,
	>(
		&self,
		_reference: &RelayReference<P, SourceClient, TargetClient>,
	) {
		// rational relayer would never submit unprofitable transactions, so we don't need to do
		// anything here
	}
}
