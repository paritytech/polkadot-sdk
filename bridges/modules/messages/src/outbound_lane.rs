// Copyright (C) Parity Technologies (UK) Ltd.
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

use crate::{Config, LOG_TARGET};

use bp_messages::{
	ChainWithMessages, DeliveredMessages, LaneState, MessageNonce, OutboundLaneData,
	UnrewardedRelayer,
};
use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::{traits::Get, BoundedVec, PalletError};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;
use sp_std::{collections::vec_deque::VecDeque, marker::PhantomData, ops::RangeInclusive};

/// Outbound lane storage.
pub trait OutboundLaneStorage {
	/// Stored message payload type.
	type StoredMessagePayload;
	/// Lane identifier type.
	type LaneId: Encode;

	/// Lane id.
	fn id(&self) -> Self::LaneId;
	/// Get lane data from the storage.
	fn data(&self) -> OutboundLaneData;
	/// Update lane data in the storage.
	fn set_data(&mut self, data: OutboundLaneData);
	/// Returns saved outbound message payload.
	#[cfg(test)]
	fn message(&self, nonce: &MessageNonce) -> Option<Self::StoredMessagePayload>;
	/// Save outbound message in the storage.
	fn save_message(&mut self, nonce: MessageNonce, message_payload: Self::StoredMessagePayload);
	/// Remove outbound message from the storage.
	fn remove_message(&mut self, nonce: &MessageNonce);
	/// Purge lane data from the storage.
	fn purge(self);
}

/// Limit for the `StoredMessagePayload` vector.
pub struct StoredMessagePayloadLimit<T, I>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> Get<u32> for StoredMessagePayloadLimit<T, I> {
	fn get() -> u32 {
		T::BridgedChain::maximal_incoming_message_size()
	}
}

/// Outbound message data wrapper that implements `MaxEncodedLen`.
pub type StoredMessagePayload<T, I> = BoundedVec<u8, StoredMessagePayloadLimit<T, I>>;

/// Result of messages receival confirmation.
#[derive(
	Encode, Decode, DecodeWithMemTracking, RuntimeDebug, PartialEq, Eq, PalletError, TypeInfo,
)]
pub enum ReceptionConfirmationError {
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
	TryingToConfirmMoreMessagesThanExpected,
}

/// Outbound messages lane.
#[derive(Debug, PartialEq, Eq)]
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

	/// Get lane state.
	pub fn state(&self) -> LaneState {
		self.storage.data().state
	}

	/// Set lane state.
	pub fn set_state(&mut self, state: LaneState) {
		let mut data = self.storage.data();
		data.state = state;
		self.storage.set_data(data);
	}

	/// Return nonces of all currently queued messages.
	pub fn queued_messages(&self) -> RangeInclusive<MessageNonce> {
		let data = self.storage.data();
		data.oldest_unpruned_nonce..=data.latest_generated_nonce
	}

	/// Send message over lane.
	///
	/// Returns new message nonce.
	pub fn send_message(&mut self, message_payload: S::StoredMessagePayload) -> MessageNonce {
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
	) -> Result<Option<DeliveredMessages>, ReceptionConfirmationError> {
		let mut data = self.storage.data();
		let confirmed_messages = DeliveredMessages {
			begin: data.latest_received_nonce.saturating_add(1),
			end: latest_delivered_nonce,
		};
		if confirmed_messages.total_messages() == 0 {
			return Ok(None)
		}
		if confirmed_messages.end > data.latest_generated_nonce {
			return Err(ReceptionConfirmationError::FailedToConfirmFutureMessages)
		}
		if confirmed_messages.total_messages() > max_allowed_messages {
			// that the relayer has declared correct number of messages that the proof contains (it
			// is checked outside of the function). But it may happen (but only if this/bridged
			// chain storage is corrupted, though) that the actual number of confirmed messages if
			// larger than declared. This would mean that 'reward loop' will take more time than the
			// weight formula accounts, so we can't allow that.
			log::trace!(
				target: LOG_TARGET,
				"Messages delivery proof contains too many messages to confirm: {} vs declared {}",
				confirmed_messages.total_messages(),
				max_allowed_messages,
			);
			return Err(ReceptionConfirmationError::TryingToConfirmMoreMessagesThanExpected)
		}

		ensure_unrewarded_relayers_are_correct(confirmed_messages.end, relayers)?;

		// prune all confirmed messages
		for nonce in confirmed_messages.begin..=confirmed_messages.end {
			self.storage.remove_message(&nonce);
		}

		data.latest_received_nonce = confirmed_messages.end;
		data.oldest_unpruned_nonce = data.latest_received_nonce.saturating_add(1);
		self.storage.set_data(data);

		Ok(Some(confirmed_messages))
	}

	/// Remove message from the storage. Doesn't perform any checks.
	pub fn remove_oldest_unpruned_message(&mut self) {
		let mut data = self.storage.data();
		self.storage.remove_message(&data.oldest_unpruned_nonce);
		data.oldest_unpruned_nonce += 1;
		self.storage.set_data(data);
	}

	/// Purge lane state from the storage.
	pub fn purge(self) {
		self.storage.purge()
	}
}

/// Verifies unrewarded relayers vec.
///
/// Returns `Err(_)` if unrewarded relayers vec contains invalid data, meaning that the bridged
/// chain has invalid runtime storage.
fn ensure_unrewarded_relayers_are_correct<RelayerId>(
	latest_received_nonce: MessageNonce,
	relayers: &VecDeque<UnrewardedRelayer<RelayerId>>,
) -> Result<(), ReceptionConfirmationError> {
	let mut expected_entry_begin = relayers.front().map(|entry| entry.messages.begin);
	for entry in relayers {
		// unrewarded relayer entry must have at least 1 unconfirmed message
		// (guaranteed by the `InboundLane::receive_message()`)
		if entry.messages.end < entry.messages.begin {
			return Err(ReceptionConfirmationError::EmptyUnrewardedRelayerEntry)
		}
		// every entry must confirm range of messages that follows previous entry range
		// (guaranteed by the `InboundLane::receive_message()`)
		if expected_entry_begin != Some(entry.messages.begin) {
			return Err(ReceptionConfirmationError::NonConsecutiveUnrewardedRelayerEntries)
		}
		expected_entry_begin = entry.messages.end.checked_add(1);
		// entry can't confirm messages larger than `inbound_lane_data.latest_received_nonce()`
		// (guaranteed by the `InboundLane::receive_message()`)
		if entry.messages.end > latest_received_nonce {
			return Err(ReceptionConfirmationError::FailedToConfirmFutureMessages)
		}
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		active_outbound_lane,
		tests::mock::{
			outbound_message_data, run_test, test_lane_id, unrewarded_relayer, TestRelayer,
			TestRuntime, REGULAR_PAYLOAD,
		},
	};
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
	) -> Result<Option<DeliveredMessages>, ReceptionConfirmationError> {
		run_test(|| {
			let mut lane = active_outbound_lane::<TestRuntime, _>(test_lane_id()).unwrap();
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
			let mut lane = active_outbound_lane::<TestRuntime, _>(test_lane_id()).unwrap();
			assert_eq!(lane.storage.data().latest_generated_nonce, 0);
			assert_eq!(lane.send_message(outbound_message_data(REGULAR_PAYLOAD)), 1);
			assert!(lane.storage.message(&1).is_some());
			assert_eq!(lane.storage.data().latest_generated_nonce, 1);
		});
	}

	#[test]
	fn confirm_delivery_works() {
		run_test(|| {
			let mut lane = active_outbound_lane::<TestRuntime, _>(test_lane_id()).unwrap();
			assert_eq!(lane.send_message(outbound_message_data(REGULAR_PAYLOAD)), 1);
			assert_eq!(lane.send_message(outbound_message_data(REGULAR_PAYLOAD)), 2);
			assert_eq!(lane.send_message(outbound_message_data(REGULAR_PAYLOAD)), 3);
			assert_eq!(lane.storage.data().latest_generated_nonce, 3);
			assert_eq!(lane.storage.data().latest_received_nonce, 0);
			assert_eq!(lane.storage.data().oldest_unpruned_nonce, 1);
			assert_eq!(
				lane.confirm_delivery(3, 3, &unrewarded_relayers(1..=3)),
				Ok(Some(delivered_messages(1..=3))),
			);
			assert_eq!(lane.storage.data().latest_generated_nonce, 3);
			assert_eq!(lane.storage.data().latest_received_nonce, 3);
			assert_eq!(lane.storage.data().oldest_unpruned_nonce, 4);
		});
	}

	#[test]
	fn confirm_partial_delivery_works() {
		run_test(|| {
			let mut lane = active_outbound_lane::<TestRuntime, _>(test_lane_id()).unwrap();
			assert_eq!(lane.send_message(outbound_message_data(REGULAR_PAYLOAD)), 1);
			assert_eq!(lane.send_message(outbound_message_data(REGULAR_PAYLOAD)), 2);
			assert_eq!(lane.send_message(outbound_message_data(REGULAR_PAYLOAD)), 3);
			assert_eq!(lane.storage.data().latest_generated_nonce, 3);
			assert_eq!(lane.storage.data().latest_received_nonce, 0);
			assert_eq!(lane.storage.data().oldest_unpruned_nonce, 1);

			assert_eq!(
				lane.confirm_delivery(3, 2, &unrewarded_relayers(1..=2)),
				Ok(Some(delivered_messages(1..=2))),
			);
			assert_eq!(lane.storage.data().latest_generated_nonce, 3);
			assert_eq!(lane.storage.data().latest_received_nonce, 2);
			assert_eq!(lane.storage.data().oldest_unpruned_nonce, 3);

			assert_eq!(
				lane.confirm_delivery(3, 3, &unrewarded_relayers(3..=3)),
				Ok(Some(delivered_messages(3..=3))),
			);
			assert_eq!(lane.storage.data().latest_generated_nonce, 3);
			assert_eq!(lane.storage.data().latest_received_nonce, 3);
			assert_eq!(lane.storage.data().oldest_unpruned_nonce, 4);
		});
	}

	#[test]
	fn confirm_delivery_rejects_nonce_lesser_than_latest_received() {
		run_test(|| {
			let mut lane = active_outbound_lane::<TestRuntime, _>(test_lane_id()).unwrap();
			lane.send_message(outbound_message_data(REGULAR_PAYLOAD));
			lane.send_message(outbound_message_data(REGULAR_PAYLOAD));
			lane.send_message(outbound_message_data(REGULAR_PAYLOAD));
			assert_eq!(lane.storage.data().latest_generated_nonce, 3);
			assert_eq!(lane.storage.data().latest_received_nonce, 0);
			assert_eq!(lane.storage.data().oldest_unpruned_nonce, 1);
			assert_eq!(
				lane.confirm_delivery(3, 3, &unrewarded_relayers(1..=3)),
				Ok(Some(delivered_messages(1..=3))),
			);
			assert_eq!(lane.confirm_delivery(3, 3, &unrewarded_relayers(1..=3)), Ok(None),);
			assert_eq!(lane.storage.data().latest_generated_nonce, 3);
			assert_eq!(lane.storage.data().latest_received_nonce, 3);
			assert_eq!(lane.storage.data().oldest_unpruned_nonce, 4);

			assert_eq!(lane.confirm_delivery(1, 2, &unrewarded_relayers(1..=1)), Ok(None),);
			assert_eq!(lane.storage.data().latest_generated_nonce, 3);
			assert_eq!(lane.storage.data().latest_received_nonce, 3);
			assert_eq!(lane.storage.data().oldest_unpruned_nonce, 4);
		});
	}

	#[test]
	fn confirm_delivery_rejects_nonce_larger_than_last_generated() {
		assert_eq!(
			assert_3_messages_confirmation_fails(10, &unrewarded_relayers(1..=10),),
			Err(ReceptionConfirmationError::FailedToConfirmFutureMessages),
		);
	}

	#[test]
	fn confirm_delivery_fails_if_entry_confirms_future_messages() {
		assert_eq!(
			assert_3_messages_confirmation_fails(
				3,
				&unrewarded_relayers(1..=1)
					.into_iter()
					.chain(unrewarded_relayers(2..=30))
					.chain(unrewarded_relayers(3..=3))
					.collect(),
			),
			Err(ReceptionConfirmationError::FailedToConfirmFutureMessages),
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
					.chain(unrewarded_relayers(2..=1))
					.chain(unrewarded_relayers(2..=3))
					.collect(),
			),
			Err(ReceptionConfirmationError::EmptyUnrewardedRelayerEntry),
		);
	}

	#[test]
	fn confirm_delivery_fails_if_entries_are_non_consecutive() {
		assert_eq!(
			assert_3_messages_confirmation_fails(
				3,
				&unrewarded_relayers(1..=1)
					.into_iter()
					.chain(unrewarded_relayers(3..=3))
					.chain(unrewarded_relayers(2..=2))
					.collect(),
			),
			Err(ReceptionConfirmationError::NonConsecutiveUnrewardedRelayerEntries),
		);
	}

	#[test]
	fn confirm_delivery_detects_when_more_than_expected_messages_are_confirmed() {
		run_test(|| {
			let mut lane = active_outbound_lane::<TestRuntime, _>(test_lane_id()).unwrap();
			lane.send_message(outbound_message_data(REGULAR_PAYLOAD));
			lane.send_message(outbound_message_data(REGULAR_PAYLOAD));
			lane.send_message(outbound_message_data(REGULAR_PAYLOAD));
			assert_eq!(
				lane.confirm_delivery(0, 3, &unrewarded_relayers(1..=3)),
				Err(ReceptionConfirmationError::TryingToConfirmMoreMessagesThanExpected),
			);
			assert_eq!(
				lane.confirm_delivery(2, 3, &unrewarded_relayers(1..=3)),
				Err(ReceptionConfirmationError::TryingToConfirmMoreMessagesThanExpected),
			);
			assert_eq!(
				lane.confirm_delivery(3, 3, &unrewarded_relayers(1..=3)),
				Ok(Some(delivered_messages(1..=3))),
			);
		});
	}
}
