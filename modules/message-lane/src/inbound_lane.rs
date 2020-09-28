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

//! Everything about incoming messages receival.

use bp_message_lane::{
	target_chain::{DispatchMessage, DispatchMessageData, MessageDispatch},
	InboundLaneData, LaneId, MessageKey, MessageNonce,
};

/// Inbound lane storage.
pub trait InboundLaneStorage {
	/// Delivery and dispatch fee type on source chain.
	type MessageFee;

	/// Lane id.
	fn id(&self) -> LaneId;
	/// Get lane data from the storage.
	fn data(&self) -> InboundLaneData;
	/// Update lane data in the storage.
	fn set_data(&mut self, data: InboundLaneData);
}

/// Inbound messages lane.
pub struct InboundLane<S> {
	storage: S,
}

impl<S: InboundLaneStorage> InboundLane<S> {
	/// Create new inbound lane backed by given storage.
	pub fn new(storage: S) -> Self {
		InboundLane { storage }
	}

	/// Receive new message.
	pub fn receive_message<P: MessageDispatch<S::MessageFee>>(
		&mut self,
		nonce: MessageNonce,
		message_data: DispatchMessageData<P::DispatchPayload, S::MessageFee>,
	) -> bool {
		let mut data = self.storage.data();
		let is_correct_message = nonce == data.latest_received_nonce + 1;
		if !is_correct_message {
			return false;
		}

		data.latest_received_nonce = nonce;
		self.storage.set_data(data);

		P::dispatch(DispatchMessage {
			key: MessageKey {
				lane_id: self.storage.id(),
				nonce,
			},
			data: message_data,
		});

		true
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		inbound_lane,
		mock::{message_data, run_test, TestMessageDispatch, TestRuntime, REGULAR_PAYLOAD, TEST_LANE_ID},
	};

	#[test]
	fn fails_to_receive_message_with_incorrect_nonce() {
		run_test(|| {
			let mut lane = inbound_lane::<TestRuntime, _>(TEST_LANE_ID);
			assert!(!lane.receive_message::<TestMessageDispatch>(10, message_data(REGULAR_PAYLOAD).into()));
			assert_eq!(lane.storage.data().latest_received_nonce, 0);
		});
	}

	#[test]
	fn correct_message_is_processed_instantly() {
		run_test(|| {
			let mut lane = inbound_lane::<TestRuntime, _>(TEST_LANE_ID);
			assert!(lane.receive_message::<TestMessageDispatch>(1, message_data(REGULAR_PAYLOAD).into()));
			assert_eq!(lane.storage.data().latest_received_nonce, 1);
		});
	}
}
