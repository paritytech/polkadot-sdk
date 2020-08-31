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

use bp_message_lane::{InboundLaneData, LaneId, Message, MessageKey, MessageNonce, MessageResult, OnMessageReceived};

/// Inbound lane storage.
pub trait InboundLaneStorage {
	/// Message payload.
	type Payload;

	/// Lane id.
	fn id(&self) -> LaneId;
	/// Get lane data from the storage.
	fn data(&self) -> InboundLaneData;
	/// Update lane data in the storage.
	fn set_data(&mut self, data: InboundLaneData);
	/// Returns saved inbound message payload.
	fn message(&self, nonce: &MessageNonce) -> Option<Self::Payload>;
	/// Save inbound message in the storage.
	fn save_message(&mut self, nonce: MessageNonce, payload: Self::Payload);
	/// Remove inbound message from the storage.
	fn remove_message(&mut self, nonce: &MessageNonce);
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
	pub fn receive_message(
		&mut self,
		nonce: MessageNonce,
		payload: S::Payload,
		processor: &mut impl OnMessageReceived<S::Payload>,
	) -> bool {
		let mut data = self.storage.data();
		let is_correct_message = nonce == data.latest_received_nonce + 1;
		if !is_correct_message {
			return false;
		}

		let is_process_required = is_correct_message && data.oldest_unprocessed_nonce == nonce;
		data.latest_received_nonce = nonce;
		self.storage.set_data(data);

		let payload_to_save = match is_process_required {
			true => {
				let message = Message {
					key: MessageKey {
						lane_id: self.storage.id(),
						nonce,
					},
					payload,
				};
				match processor.on_message_received(message) {
					MessageResult::Processed => None,
					MessageResult::NotProcessed(message) => Some(message.payload),
				}
			}
			false => Some(payload),
		};

		if let Some(payload_to_save) = payload_to_save {
			self.storage.save_message(nonce, payload_to_save);
		}

		true
	}

	/// Process stored lane messages.
	///
	/// Stops processing either when all messages are processed, or when processor returns
	/// MessageResult::NotProcessed.
	pub fn process_messages(&mut self, processor: &mut impl OnMessageReceived<S::Payload>) {
		let mut anything_processed = false;
		let mut data = self.storage.data();
		while data.oldest_unprocessed_nonce <= data.latest_received_nonce {
			let nonce = data.oldest_unprocessed_nonce;
			let payload = self
				.storage
				.message(&nonce)
				.expect("message is referenced by lane; referenced message is not pruned; qed");
			let message = Message {
				key: MessageKey {
					lane_id: self.storage.id(),
					nonce,
				},
				payload,
			};

			let process_result = processor.on_message_received(message);
			if let MessageResult::NotProcessed(_) = process_result {
				break;
			}

			self.storage.remove_message(&nonce);

			anything_processed = true;
			data.oldest_unprocessed_nonce += 1;
		}

		if anything_processed {
			self.storage.set_data(data);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		inbound_lane,
		mock::{
			run_test, TestMessageProcessor, TestPayload, TestRuntime, PAYLOAD_TO_QUEUE, REGULAR_PAYLOAD, TEST_LANE_ID,
		},
	};

	#[test]
	fn fails_to_receive_message_with_incorrect_nonce() {
		run_test(|| {
			let mut lane = inbound_lane::<TestRuntime, _>(TEST_LANE_ID);
			assert!(!lane.receive_message(10, REGULAR_PAYLOAD, &mut TestMessageProcessor));
			assert!(lane.storage.message(&10).is_none());
			assert_eq!(lane.storage.data().latest_received_nonce, 0);
		});
	}

	#[test]
	fn correct_message_is_queued_if_some_other_messages_are_queued() {
		run_test(|| {
			let mut lane = inbound_lane::<TestRuntime, _>(TEST_LANE_ID);
			assert!(lane.receive_message(1, PAYLOAD_TO_QUEUE, &mut TestMessageProcessor));
			assert!(lane.storage.message(&1).is_some());
			assert!(lane.receive_message(2, REGULAR_PAYLOAD, &mut TestMessageProcessor));
			assert!(lane.storage.message(&2).is_some());
			assert_eq!(lane.storage.data().latest_received_nonce, 2);
		});
	}

	#[test]
	fn correct_message_is_queued_if_processor_wants_to_queue() {
		run_test(|| {
			let mut lane = inbound_lane::<TestRuntime, _>(TEST_LANE_ID);
			assert!(lane.receive_message(1, PAYLOAD_TO_QUEUE, &mut TestMessageProcessor));
			assert!(lane.storage.message(&1).is_some());
			assert_eq!(lane.storage.data().latest_received_nonce, 1);
		});
	}

	#[test]
	fn correct_message_is_not_queued_if_processed_instantly() {
		run_test(|| {
			let mut lane = inbound_lane::<TestRuntime, _>(TEST_LANE_ID);
			assert!(lane.receive_message(1, REGULAR_PAYLOAD, &mut TestMessageProcessor));
			assert!(lane.storage.message(&1).is_none());
			assert_eq!(lane.storage.data().latest_received_nonce, 1);
		});
	}

	#[test]
	fn process_message_does_nothing_when_lane_is_empty() {
		run_test(|| {
			let mut lane = inbound_lane::<TestRuntime, _>(TEST_LANE_ID);
			assert_eq!(lane.storage.data().oldest_unprocessed_nonce, 1);
			lane.process_messages(&mut TestMessageProcessor);
			assert_eq!(lane.storage.data().oldest_unprocessed_nonce, 1);
		});
	}

	#[test]
	fn process_message_works() {
		run_test(|| {
			pub struct QueueByNonce(MessageNonce);

			impl OnMessageReceived<TestPayload> for QueueByNonce {
				fn on_message_received(&mut self, message: Message<TestPayload>) -> MessageResult<TestPayload> {
					if message.key.nonce == self.0 {
						MessageResult::NotProcessed(message)
					} else {
						MessageResult::Processed
					}
				}
			}

			let mut lane = inbound_lane::<TestRuntime, _>(TEST_LANE_ID);
			assert!(lane.receive_message(1, PAYLOAD_TO_QUEUE, &mut TestMessageProcessor));
			assert!(lane.receive_message(2, PAYLOAD_TO_QUEUE, &mut TestMessageProcessor));
			assert!(lane.receive_message(3, PAYLOAD_TO_QUEUE, &mut TestMessageProcessor));
			assert!(lane.receive_message(4, REGULAR_PAYLOAD, &mut TestMessageProcessor));

			assert!(lane.storage.message(&1).is_some());
			assert!(lane.storage.message(&2).is_some());
			assert!(lane.storage.message(&3).is_some());
			assert!(lane.storage.message(&4).is_some());
			assert_eq!(lane.storage.data().oldest_unprocessed_nonce, 1);

			lane.process_messages(&mut QueueByNonce(3));

			assert!(lane.storage.message(&1).is_none());
			assert!(lane.storage.message(&2).is_none());
			assert!(lane.storage.message(&3).is_some());
			assert!(lane.storage.message(&4).is_some());
			assert_eq!(lane.storage.data().oldest_unprocessed_nonce, 3);

			lane.process_messages(&mut QueueByNonce(10));

			assert!(lane.storage.message(&1).is_none());
			assert!(lane.storage.message(&2).is_none());
			assert!(lane.storage.message(&3).is_none());
			assert!(lane.storage.message(&4).is_none());
			assert_eq!(lane.storage.data().oldest_unprocessed_nonce, 5);
		});
	}
}
