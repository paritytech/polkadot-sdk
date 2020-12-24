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

//! Weight-related utilities.

use crate::weights::WeightInfo;

use bp_message_lane::MessageNonce;
use frame_support::weights::Weight;

/// Ensure that weights from `WeightInfoExt` implementation are looking correct.
pub fn ensure_weights_are_correct<W: WeightInfoExt>() {
	assert_ne!(W::send_message_worst_case(), 0);

	assert_ne!(W::receive_messages_proof_overhead(), 0);
	assert_ne!(W::receive_messages_proof_messages_overhead(1), 0);
	assert_ne!(W::receive_messages_proof_outbound_lane_state_overhead(), 0);

	assert_ne!(W::receive_messages_delivery_proof_overhead(), 0);
	assert_ne!(W::receive_messages_delivery_proof_messages_overhead(1), 0);
	assert_ne!(W::receive_messages_delivery_proof_relayers_overhead(1), 0);
}

/// Extended weight info.
pub trait WeightInfoExt: WeightInfo {
	/// Returns weight overhead of message delivery transaction (`receive_messages_proof`).
	fn receive_messages_proof_overhead() -> Weight {
		let weight_of_two_messages_and_two_tx_overheads = Self::receive_single_message_proof().saturating_mul(2);
		let weight_of_two_messages_and_single_tx_overhead = Self::receive_two_messages_proof();
		weight_of_two_messages_and_two_tx_overheads.saturating_sub(weight_of_two_messages_and_single_tx_overhead)
	}

	/// Returns weight that needs to be accounted when receiving given number of messages with message
	/// delivery transaction (`receive_messages_proof`).
	fn receive_messages_proof_messages_overhead(messages: MessageNonce) -> Weight {
		let weight_of_two_messages_and_single_tx_overhead = Self::receive_two_messages_proof();
		let weight_of_single_message_and_single_tx_overhead = Self::receive_single_message_proof();
		weight_of_two_messages_and_single_tx_overhead
			.saturating_sub(weight_of_single_message_and_single_tx_overhead)
			.saturating_mul(messages as Weight)
	}

	/// Returns weight that needs to be accounted when message delivery transaction (`receive_messages_proof`)
	/// is carrying outbound lane state proof.
	fn receive_messages_proof_outbound_lane_state_overhead() -> Weight {
		let weight_of_single_message_and_lane_state = Self::receive_single_message_proof_with_outbound_lane_state();
		let weight_of_single_message = Self::receive_single_message_proof();
		weight_of_single_message_and_lane_state.saturating_sub(weight_of_single_message)
	}

	/// Returns weight overhead of delivery confirmation transaction (`receive_messages_delivery_proof`).
	fn receive_messages_delivery_proof_overhead() -> Weight {
		let weight_of_two_messages_and_two_tx_overheads =
			Self::receive_delivery_proof_for_single_message().saturating_mul(2);
		let weight_of_two_messages_and_single_tx_overhead =
			Self::receive_delivery_proof_for_two_messages_by_single_relayer();
		weight_of_two_messages_and_two_tx_overheads.saturating_sub(weight_of_two_messages_and_single_tx_overhead)
	}

	/// Returns weight that needs to be accounted when receiving confirmations for given number of
	/// messages with delivery confirmation transaction (`receive_messages_delivery_proof`).
	fn receive_messages_delivery_proof_messages_overhead(messages: MessageNonce) -> Weight {
		let weight_of_two_messages = Self::receive_delivery_proof_for_two_messages_by_single_relayer();
		let weight_of_single_message = Self::receive_delivery_proof_for_single_message();
		weight_of_two_messages
			.saturating_sub(weight_of_single_message)
			.saturating_mul(messages as Weight)
	}

	/// Returns weight that needs to be accounted when receiving confirmations for given number of
	/// relayers entries with delivery confirmation transaction (`receive_messages_delivery_proof`).
	fn receive_messages_delivery_proof_relayers_overhead(relayers: MessageNonce) -> Weight {
		let weight_of_two_messages_by_two_relayers = Self::receive_delivery_proof_for_two_messages_by_two_relayers();
		let weight_of_two_messages_by_single_relayer =
			Self::receive_delivery_proof_for_two_messages_by_single_relayer();
		weight_of_two_messages_by_two_relayers
			.saturating_sub(weight_of_two_messages_by_single_relayer)
			.saturating_mul(relayers as Weight)
	}
}

impl<T: WeightInfo> WeightInfoExt for T {}
