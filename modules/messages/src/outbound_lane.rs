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

//! Everything about outgoing messages sending.

use crate::Config;

use bp_messages::{
	DeliveredMessages, LaneId, MessageNonce, MessagePayload, OutboundLaneData, UnrewardedRelayer,
};
use frame_support::{
	weights::{RuntimeDbWeight, Weight},
	BoundedVec, RuntimeDebug,
};
use num_traits::Zero;
use sp_std::collections::vec_deque::VecDeque;

/// Outbound lane storage.
pub trait OutboundLaneStorage {
	/// Lane id.
	fn id(&self) -> LaneId;
	/// Get lane data from the storage.
	fn data(&self) -> OutboundLaneData;
	/// Update lane data in the storage.
	fn set_data(&mut self, data: OutboundLaneData);
	/// Returns saved outbound message payload.
	#[cfg(test)]
	fn message(&self, nonce: &MessageNonce) -> Option<MessagePayload>;
	/// Save outbound message in the storage.
	fn save_message(&mut self, nonce: MessageNonce, message_payload: MessagePayload);
	/// Remove outbound message from the storage.
	fn remove_message(&mut self, nonce: &MessageNonce);
}

/// Outbound message data wrapper that implements `MaxEncodedLen`.
pub type StoredMessagePayload<T, I> = BoundedVec<u8, <T as Config<I>>::MaximalOutboundPayloadSize>;

/// Result of messages receival confirmation.
#[derive(RuntimeDebug, PartialEq, Eq)]
pub enum ReceivalConfirmationResult {
	/// New messages have been confirmed by the confirmation transaction.
	ConfirmedMessages(DeliveredMessages),
	/// Confirmation transaction brings no new confirmation. This may be a result of relayer
	/// error or several relayers running.
	NoNewConfirmations,
	/// Bridged chain is trying to confirm more messages than we have generated. May be a result
	/// of invalid bridged chain storage.
	FailedToConfirmFutureMessages,
	/// The unrewarded relayers vec contains an empty entry. May be a result of invalid bridged
	/// chain storage.
	EmptyUnrewardedRelayerEntry,
	/// The unrewarded relayers vec contains non-consecutive entries. May be a result of invalid
	/// bridged chain storage.
	NonConsecutiveUnrewardedRelayerEntries,
	/// The chain has more messages that need to be confirmed than there is in the proof.
	TryingToConfirmMoreMessagesThanExpected(MessageNonce),
}

/// Outbound messages lane.
pub struct OutboundLane<S> {
	storage: S,
}

impl<S: OutboundLaneStorage> OutboundLane<S> {
	/// Create new outbound lane backed by given storage.
	pub fn new(storage: S) -> Self {
		OutboundLane { storage }
	}

	/// Get this lane data.
	pub fn data(&self) -> OutboundLaneData {
		self.storage.data()
	}

	/// Send message over lane.
	///
	/// Returns new message nonce.
	pub fn send_message(&mut self, message_payload: MessagePayload) -> MessageNonce {
		let mut data = self.storage.data();
		let nonce = data.latest_generated_nonce + 1;
		data.latest_generated_nonce = nonce;

		self.storage.save_message(nonce, message_payload);
		self.storage.set_data(data);

		nonce
	}

	/// Confirm messages delivery.
	pub fn confirm_delivery<RelayerId>(
		&mut self,
		max_allowed_messages: MessageNonce,
		latest_delivered_nonce: MessageNonce,
		relayers: &VecDeque<UnrewardedRelayer<RelayerId>>,
	) -> ReceivalConfirmationResult {
		let mut data = self.storage.data();
		if latest_delivered_nonce <= data.latest_received_nonce {
			return ReceivalConfirmationResult::NoNewConfirmations
		}
		if latest_delivered_nonce > data.latest_generated_nonce {
			return ReceivalConfirmationResult::FailedToConfirmFutureMessages
		}
		if latest_delivered_nonce - data.latest_received_nonce > max_allowed_messages {
			// that the relayer has declared correct number of messages that the proof contains (it
			// is checked outside of the function). But it may happen (but only if this/bridged
			// chain storage is corrupted, though) that the actual number of confirmed messages if
			// larger than declared. This would mean that 'reward loop' will take more time than the
			// weight formula accounts, so we can't allow that.
			return ReceivalConfirmationResult::TryingToConfirmMoreMessagesThanExpected(
				latest_delivered_nonce - data.latest_received_nonce,
			)
		}

		if let Err(e) = ensure_unrewarded_relayers_are_correct(latest_delivered_nonce, relayers) {
			return e
		}

		let prev_latest_received_nonce = data.latest_received_nonce;
		data.latest_received_nonce = latest_delivered_nonce;
		self.storage.set_data(data);

		ReceivalConfirmationResult::ConfirmedMessages(DeliveredMessages {
			begin: prev_latest_received_nonce + 1,
			end: latest_delivered_nonce,
		})
	}

	/// Prune at most `max_messages_to_prune` already received messages.
	///
	/// Returns weight, consumed by messages pruning and lane state update.
	pub fn prune_messages(
		&mut self,
		db_weight: RuntimeDbWeight,
		mut remaining_weight: Weight,
	) -> Weight {
		let write_weight = db_weight.writes(1);
		let two_writes_weight = write_weight + write_weight;
		let mut spent_weight = Weight::zero();
		let mut data = self.storage.data();
		while remaining_weight.all_gte(two_writes_weight) &&
			data.oldest_unpruned_nonce <= data.latest_received_nonce
		{
			self.storage.remove_message(&data.oldest_unpruned_nonce);

			spent_weight += write_weight;
			remaining_weight -= write_weight;
			data.oldest_unpruned_nonce += 1;
		}

		if !spent_weight.is_zero() {
			spent_weight += write_weight;
			self.storage.set_data(data);
		}

		spent_weight
	}
}

/// Verifies unrewarded relayers vec.
///
/// Returns `Err(_)` if unrewarded relayers vec contains invalid data, meaning that the bridged
/// chain has invalid runtime storage.
fn ensure_unrewarded_relayers_are_correct<RelayerId>(
	latest_received_nonce: MessageNonce,
	relayers: &VecDeque<UnrewardedRelayer<RelayerId>>,
) -> Result<(), ReceivalConfirmationResult> {
	let mut last_entry_end: Option<MessageNonce> = None;
	for entry in relayers {
		// unrewarded relayer entry must have at least 1 unconfirmed message
		// (guaranteed by the `InboundLane::receive_message()`)
		if entry.messages.end < entry.messages.begin {
			return Err(ReceivalConfirmationResult::EmptyUnrewardedRelayerEntry)
		}
		// every entry must confirm range of messages that follows previous entry range
		// (guaranteed by the `InboundLane::receive_message()`)
		if let Some(last_entry_end) = last_entry_end {
			let expected_entry_begin = last_entry_end.checked_add(1);
			if expected_entry_begin != Some(entry.messages.begin) {
				return Err(ReceivalConfirmationResult::NonConsecutiveUnrewardedRelayerEntries)
			}
		}
		last_entry_end = Some(entry.messages.end);
		// entry can't confirm messages larger than `inbound_lane_data.latest_received_nonce()`
		// (guaranteed by the `InboundLane::receive_message()`)
		if entry.messages.end > latest_received_nonce {
			return Err(ReceivalConfirmationResult::FailedToConfirmFutureMessages)
		}
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		mock::{
			outbound_message_data, run_test, unrewarded_relayer, TestRelayer, TestRuntime,
			REGULAR_PAYLOAD, TEST_LANE_ID,
		},
		outbound_lane,
	};
	use frame_support::weights::constants::RocksDbWeight;
	use sp_std::ops::RangeInclusive;

	fn unrewarded_relayers(
		nonces: RangeInclusive<MessageNonce>,
	) -> VecDeque<UnrewardedRelayer<TestRelayer>> {
		vec![unrewarded_relayer(*nonces.start(), *nonces.end(), 0)]
			.into_iter()
			.collect()
	}

	fn delivered_messages(nonces: RangeInclusive<MessageNonce>) -> DeliveredMessages {
		DeliveredMessages { begin: *nonces.start(), end: *nonces.end() }
	}

	fn assert_3_messages_confirmation_fails(
		latest_received_nonce: MessageNonce,
		relayers: &VecDeque<UnrewardedRelayer<TestRelayer>>,
	) -> ReceivalConfirmationResult {
		run_test(|| {
			let mut lane = outbound_lane::<TestRuntime, _>(TEST_LANE_ID);
			lane.send_message(outbound_message_data(REGULAR_PAYLOAD));
			lane.send_message(outbound_message_data(REGULAR_PAYLOAD));
			lane.send_message(outbound_message_data(REGULAR_PAYLOAD));
			assert_eq!(lane.storage.data().latest_generated_nonce, 3);
			assert_eq!(lane.storage.data().latest_received_nonce, 0);
			let result = lane.confirm_delivery(3, latest_received_nonce, relayers);
			assert_eq!(lane.storage.data().latest_generated_nonce, 3);
			assert_eq!(lane.storage.data().latest_received_nonce, 0);
			result
		})
	}

	#[test]
	fn send_message_works() {
		run_test(|| {
			let mut lane = outbound_lane::<TestRuntime, _>(TEST_LANE_ID);
			assert_eq!(lane.storage.data().latest_generated_nonce, 0);
			assert_eq!(lane.send_message(outbound_message_data(REGULAR_PAYLOAD)), 1);
			assert!(lane.storage.message(&1).is_some());
			assert_eq!(lane.storage.data().latest_generated_nonce, 1);
		});
	}

	#[test]
	fn confirm_delivery_works() {
		run_test(|| {
			let mut lane = outbound_lane::<TestRuntime, _>(TEST_LANE_ID);
			assert_eq!(lane.send_message(outbound_message_data(REGULAR_PAYLOAD)), 1);
			assert_eq!(lane.send_message(outbound_message_data(REGULAR_PAYLOAD)), 2);
			assert_eq!(lane.send_message(outbound_message_data(REGULAR_PAYLOAD)), 3);
			assert_eq!(lane.storage.data().latest_generated_nonce, 3);
			assert_eq!(lane.storage.data().latest_received_nonce, 0);
			assert_eq!(
				lane.confirm_delivery(3, 3, &unrewarded_relayers(1..=3)),
				ReceivalConfirmationResult::ConfirmedMessages(delivered_messages(1..=3)),
			);
			assert_eq!(lane.storage.data().latest_generated_nonce, 3);
			assert_eq!(lane.storage.data().latest_received_nonce, 3);
		});
	}

	#[test]
	fn confirm_delivery_rejects_nonce_lesser_than_latest_received() {
		run_test(|| {
			let mut lane = outbound_lane::<TestRuntime, _>(TEST_LANE_ID);
			lane.send_message(outbound_message_data(REGULAR_PAYLOAD));
			lane.send_message(outbound_message_data(REGULAR_PAYLOAD));
			lane.send_message(outbound_message_data(REGULAR_PAYLOAD));
			assert_eq!(lane.storage.data().latest_generated_nonce, 3);
			assert_eq!(lane.storage.data().latest_received_nonce, 0);
			assert_eq!(
				lane.confirm_delivery(3, 3, &unrewarded_relayers(1..=3)),
				ReceivalConfirmationResult::ConfirmedMessages(delivered_messages(1..=3)),
			);
			assert_eq!(
				lane.confirm_delivery(3, 3, &unrewarded_relayers(1..=3)),
				ReceivalConfirmationResult::NoNewConfirmations,
			);
			assert_eq!(lane.storage.data().latest_generated_nonce, 3);
			assert_eq!(lane.storage.data().latest_received_nonce, 3);

			assert_eq!(
				lane.confirm_delivery(1, 2, &unrewarded_relayers(1..=1)),
				ReceivalConfirmationResult::NoNewConfirmations,
			);
			assert_eq!(lane.storage.data().latest_generated_nonce, 3);
			assert_eq!(lane.storage.data().latest_received_nonce, 3);
		});
	}

	#[test]
	fn confirm_delivery_rejects_nonce_larger_than_last_generated() {
		assert_eq!(
			assert_3_messages_confirmation_fails(10, &unrewarded_relayers(1..=10),),
			ReceivalConfirmationResult::FailedToConfirmFutureMessages,
		);
	}

	#[test]
	fn confirm_delivery_fails_if_entry_confirms_future_messages() {
		assert_eq!(
			assert_3_messages_confirmation_fails(
				3,
				&unrewarded_relayers(1..=1)
					.into_iter()
					.chain(unrewarded_relayers(2..=30).into_iter())
					.chain(unrewarded_relayers(3..=3).into_iter())
					.collect(),
			),
			ReceivalConfirmationResult::FailedToConfirmFutureMessages,
		);
	}

	#[test]
	#[allow(clippy::reversed_empty_ranges)]
	fn confirm_delivery_fails_if_entry_is_empty() {
		assert_eq!(
			assert_3_messages_confirmation_fails(
				3,
				&unrewarded_relayers(1..=1)
					.into_iter()
					.chain(unrewarded_relayers(2..=1).into_iter())
					.chain(unrewarded_relayers(2..=3).into_iter())
					.collect(),
			),
			ReceivalConfirmationResult::EmptyUnrewardedRelayerEntry,
		);
	}

	#[test]
	fn confirm_delivery_fails_if_entries_are_non_consecutive() {
		assert_eq!(
			assert_3_messages_confirmation_fails(
				3,
				&unrewarded_relayers(1..=1)
					.into_iter()
					.chain(unrewarded_relayers(3..=3).into_iter())
					.chain(unrewarded_relayers(2..=2).into_iter())
					.collect(),
			),
			ReceivalConfirmationResult::NonConsecutiveUnrewardedRelayerEntries,
		);
	}

	#[test]
	fn prune_messages_works() {
		run_test(|| {
			let mut lane = outbound_lane::<TestRuntime, _>(TEST_LANE_ID);
			// when lane is empty, nothing is pruned
			assert_eq!(
				lane.prune_messages(RocksDbWeight::get(), RocksDbWeight::get().writes(101)),
				Weight::zero()
			);
			assert_eq!(lane.storage.data().oldest_unpruned_nonce, 1);
			// when nothing is confirmed, nothing is pruned
			lane.send_message(outbound_message_data(REGULAR_PAYLOAD));
			lane.send_message(outbound_message_data(REGULAR_PAYLOAD));
			lane.send_message(outbound_message_data(REGULAR_PAYLOAD));
			assert!(lane.storage.message(&1).is_some());
			assert!(lane.storage.message(&2).is_some());
			assert!(lane.storage.message(&3).is_some());
			assert_eq!(
				lane.prune_messages(RocksDbWeight::get(), RocksDbWeight::get().writes(101)),
				Weight::zero()
			);
			assert_eq!(lane.storage.data().oldest_unpruned_nonce, 1);
			// after confirmation, some messages are received
			assert_eq!(
				lane.confirm_delivery(2, 2, &unrewarded_relayers(1..=2)),
				ReceivalConfirmationResult::ConfirmedMessages(delivered_messages(1..=2)),
			);
			assert_eq!(
				lane.prune_messages(RocksDbWeight::get(), RocksDbWeight::get().writes(101)),
				RocksDbWeight::get().writes(3),
			);
			assert!(lane.storage.message(&1).is_none());
			assert!(lane.storage.message(&2).is_none());
			assert!(lane.storage.message(&3).is_some());
			assert_eq!(lane.storage.data().oldest_unpruned_nonce, 3);
			// after last message is confirmed, everything is pruned
			assert_eq!(
				lane.confirm_delivery(1, 3, &unrewarded_relayers(3..=3)),
				ReceivalConfirmationResult::ConfirmedMessages(delivered_messages(3..=3)),
			);
			assert_eq!(
				lane.prune_messages(RocksDbWeight::get(), RocksDbWeight::get().writes(101)),
				RocksDbWeight::get().writes(2),
			);
			assert!(lane.storage.message(&1).is_none());
			assert!(lane.storage.message(&2).is_none());
			assert!(lane.storage.message(&3).is_none());
			assert_eq!(lane.storage.data().oldest_unpruned_nonce, 4);
		});
	}

	#[test]
	fn confirm_delivery_detects_when_more_than_expected_messages_are_confirmed() {
		run_test(|| {
			let mut lane = outbound_lane::<TestRuntime, _>(TEST_LANE_ID);
			lane.send_message(outbound_message_data(REGULAR_PAYLOAD));
			lane.send_message(outbound_message_data(REGULAR_PAYLOAD));
			lane.send_message(outbound_message_data(REGULAR_PAYLOAD));
			assert_eq!(
				lane.confirm_delivery(0, 3, &unrewarded_relayers(1..=3)),
				ReceivalConfirmationResult::TryingToConfirmMoreMessagesThanExpected(3),
			);
			assert_eq!(
				lane.confirm_delivery(2, 3, &unrewarded_relayers(1..=3)),
				ReceivalConfirmationResult::TryingToConfirmMoreMessagesThanExpected(3),
			);
			assert_eq!(
				lane.confirm_delivery(3, 3, &unrewarded_relayers(1..=3)),
				ReceivalConfirmationResult::ConfirmedMessages(delivered_messages(1..=3)),
			);
		});
	}
}
