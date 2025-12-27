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

//! Helpers for easier manipulation of call processing with signed extensions.

use crate::{BridgedChainOf, Config, InboundLanes, OutboundLanes, Pallet, LOG_TARGET};

use bp_messages::{
	target_chain::MessageDispatch, BaseMessagesProofInfo, ChainWithMessages, InboundLaneData,
	MessageNonce, MessagesCallInfo, ReceiveMessagesDeliveryProofInfo, ReceiveMessagesProofInfo,
	UnrewardedRelayerOccupation,
};
use bp_runtime::{AccountIdOf, OwnedBridgeModule};
use frame_support::{dispatch::CallableCallFor, traits::IsSubType};
use sp_runtime::transaction_validity::TransactionValidity;

/// Helper struct that provides methods for working with a call supported by `MessagesCallInfo`.
pub struct CallHelper<T: Config<I>, I: 'static> {
	_phantom_data: sp_std::marker::PhantomData<(T, I)>,
}

impl<T: Config<I>, I: 'static> CallHelper<T, I> {
	/// Returns true if:
	///
	/// - call is `receive_messages_proof` and all messages have been delivered;
	///
	/// - call is `receive_messages_delivery_proof` and all messages confirmations have been
	///   received.
	pub fn was_successful(info: &MessagesCallInfo<T::LaneId>) -> bool {
		match info {
			MessagesCallInfo::ReceiveMessagesProof(info) => {
				let inbound_lane_data = match InboundLanes::<T, I>::get(info.base.lane_id) {
					Some(inbound_lane_data) => inbound_lane_data,
					None => return false,
				};
				if info.base.bundled_range.is_empty() {
					let post_occupation =
						unrewarded_relayers_occupation::<T, I>(&inbound_lane_data);
					// we don't care about `free_relayer_slots` here - it is checked in
					// `is_obsolete` and every relayer has delivered at least one message,
					// so if relayer slots are released, then message slots are also
					// released
					return post_occupation.free_message_slots >
						info.unrewarded_relayers.free_message_slots
				}

				inbound_lane_data.last_delivered_nonce() == *info.base.bundled_range.end()
			},
			MessagesCallInfo::ReceiveMessagesDeliveryProof(info) => {
				let outbound_lane_data = match OutboundLanes::<T, I>::get(info.0.lane_id) {
					Some(outbound_lane_data) => outbound_lane_data,
					None => return false,
				};
				outbound_lane_data.latest_received_nonce == *info.0.bundled_range.end()
			},
		}
	}
}

/// Trait representing a call that is a sub type of `pallet_bridge_messages::Call`.
pub trait CallSubType<T: Config<I, RuntimeCall = Self>, I: 'static>:
	IsSubType<CallableCallFor<Pallet<T, I>, T>>
{
	/// Create a new instance of `ReceiveMessagesProofInfo` from a `ReceiveMessagesProof` call.
	fn receive_messages_proof_info(&self) -> Option<ReceiveMessagesProofInfo<T::LaneId>>;

	/// Create a new instance of `ReceiveMessagesDeliveryProofInfo` from
	/// a `ReceiveMessagesDeliveryProof` call.
	fn receive_messages_delivery_proof_info(
		&self,
	) -> Option<ReceiveMessagesDeliveryProofInfo<T::LaneId>>;

	/// Create a new instance of `MessagesCallInfo` from a `ReceiveMessagesProof`
	/// or a `ReceiveMessagesDeliveryProof` call.
	fn call_info(&self) -> Option<MessagesCallInfo<T::LaneId>>;

	/// Create a new instance of `MessagesCallInfo` from a `ReceiveMessagesProof`
	/// or a `ReceiveMessagesDeliveryProof` call, if the call is for the provided lane.
	fn call_info_for(&self, lane_id: T::LaneId) -> Option<MessagesCallInfo<T::LaneId>>;

	/// Ensures that a `ReceiveMessagesProof` or a `ReceiveMessagesDeliveryProof` call:
	///
	/// - does not deliver already delivered messages. We require all messages in the
	///   `ReceiveMessagesProof` call to be undelivered;
	///
	/// - does not submit empty `ReceiveMessagesProof` call with zero messages, unless the lane
	///   needs to be unblocked by providing relayer rewards proof;
	///
	/// - brings no new delivery confirmations in a `ReceiveMessagesDeliveryProof` call. We require
	///   at least one new delivery confirmation in the unrewarded relayers set;
	///
	/// - does not violate some basic (easy verifiable) messages pallet rules obsolete (like
	///   submitting a call when a pallet is halted or delivering messages when a dispatcher is
	///   inactive).
	///
	/// If one of above rules is violated, the transaction is treated as invalid.
	fn check_obsolete_call(&self) -> TransactionValidity;
}

impl<
		Call: IsSubType<CallableCallFor<Pallet<T, I>, T>>,
		T: frame_system::Config<RuntimeCall = Call> + Config<I>,
		I: 'static,
	> CallSubType<T, I> for T::RuntimeCall
{
	fn receive_messages_proof_info(&self) -> Option<ReceiveMessagesProofInfo<T::LaneId>> {
		if let Some(crate::Call::<T, I>::receive_messages_proof { ref proof, .. }) =
			self.is_sub_type()
		{
			let inbound_lane_data = InboundLanes::<T, I>::get(proof.lane)?;

			return Some(ReceiveMessagesProofInfo {
				base: BaseMessagesProofInfo {
					lane_id: proof.lane,
					// we want all messages in this range to be new for us. Otherwise transaction
					// will be considered obsolete.
					bundled_range: proof.nonces_start..=proof.nonces_end,
					best_stored_nonce: inbound_lane_data.last_delivered_nonce(),
				},
				unrewarded_relayers: unrewarded_relayers_occupation::<T, I>(&inbound_lane_data),
			})
		}

		None
	}

	fn receive_messages_delivery_proof_info(
		&self,
	) -> Option<ReceiveMessagesDeliveryProofInfo<T::LaneId>> {
		if let Some(crate::Call::<T, I>::receive_messages_delivery_proof {
			ref proof,
			ref relayers_state,
			..
		}) = self.is_sub_type()
		{
			let outbound_lane_data = OutboundLanes::<T, I>::get(proof.lane)?;

			return Some(ReceiveMessagesDeliveryProofInfo(BaseMessagesProofInfo {
				lane_id: proof.lane,
				// there's a time frame between message delivery, message confirmation and reward
				// confirmation. Because of that, we can't assume that our state has been confirmed
				// to the bridged chain. So we are accepting any proof that brings new
				// confirmations.
				bundled_range: outbound_lane_data.latest_received_nonce + 1..=
					relayers_state.last_delivered_nonce,
				best_stored_nonce: outbound_lane_data.latest_received_nonce,
			}))
		}

		None
	}

	fn call_info(&self) -> Option<MessagesCallInfo<T::LaneId>> {
		if let Some(info) = self.receive_messages_proof_info() {
			return Some(MessagesCallInfo::ReceiveMessagesProof(info))
		}

		if let Some(info) = self.receive_messages_delivery_proof_info() {
			return Some(MessagesCallInfo::ReceiveMessagesDeliveryProof(info))
		}

		None
	}

	fn call_info_for(&self, lane_id: T::LaneId) -> Option<MessagesCallInfo<T::LaneId>> {
		self.call_info().filter(|info| {
			let actual_lane_id = match info {
				MessagesCallInfo::ReceiveMessagesProof(info) => info.base.lane_id,
				MessagesCallInfo::ReceiveMessagesDeliveryProof(info) => info.0.lane_id,
			};
			actual_lane_id == lane_id
		})
	}

	fn check_obsolete_call(&self) -> TransactionValidity {
		let is_pallet_halted = Pallet::<T, I>::ensure_not_halted().is_err();
		match self.call_info() {
			Some(proof_info) if is_pallet_halted => {
				tracing::trace!(
					target: LOG_TARGET,
					?proof_info,
					"Rejecting messages transaction on halted pallet"
				);

				return sp_runtime::transaction_validity::InvalidTransaction::Call.into()
			},
			Some(MessagesCallInfo::ReceiveMessagesProof(proof_info))
				if proof_info
					.is_obsolete(T::MessageDispatch::is_active(proof_info.base.lane_id)) =>
			{
				tracing::trace!(
					target: LOG_TARGET,
					?proof_info,
					"Rejecting obsolete messages delivery transaction"
				);

				return sp_runtime::transaction_validity::InvalidTransaction::Stale.into()
			},
			Some(MessagesCallInfo::ReceiveMessagesDeliveryProof(proof_info))
				if proof_info.is_obsolete() =>
			{
				tracing::trace!(
					target: LOG_TARGET,
					?proof_info,
					"Rejecting obsolete messages confirmation transaction"
				);

				return sp_runtime::transaction_validity::InvalidTransaction::Stale.into()
			},
			_ => {},
		}

		Ok(sp_runtime::transaction_validity::ValidTransaction::default())
	}
}

/// Returns occupation state of unrewarded relayers vector.
fn unrewarded_relayers_occupation<T: Config<I>, I: 'static>(
	inbound_lane_data: &InboundLaneData<AccountIdOf<BridgedChainOf<T, I>>>,
) -> UnrewardedRelayerOccupation {
	UnrewardedRelayerOccupation {
		free_relayer_slots: T::BridgedChain::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX
			.saturating_sub(inbound_lane_data.relayers.len() as MessageNonce),
		free_message_slots: {
			let unconfirmed_messages = inbound_lane_data
				.last_delivered_nonce()
				.saturating_sub(inbound_lane_data.last_confirmed_nonce);
			T::BridgedChain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX
				.saturating_sub(unconfirmed_messages)
		},
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::tests::mock::*;
	use bp_messages::{
		source_chain::FromBridgedChainMessagesDeliveryProof,
		target_chain::FromBridgedChainMessagesProof, DeliveredMessages, InboundLaneData, LaneState,
		OutboundLaneData, UnrewardedRelayer, UnrewardedRelayersState,
	};
	use sp_std::ops::RangeInclusive;

	fn fill_unrewarded_relayers() {
		let mut inbound_lane_state = InboundLanes::<TestRuntime>::get(test_lane_id()).unwrap();
		for n in 0..BridgedChain::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX {
			inbound_lane_state.relayers.push_back(UnrewardedRelayer {
				relayer: Default::default(),
				messages: DeliveredMessages { begin: n + 1, end: n + 1 },
			});
		}
		InboundLanes::<TestRuntime>::insert(test_lane_id(), inbound_lane_state);
	}

	fn fill_unrewarded_messages() {
		let mut inbound_lane_state = InboundLanes::<TestRuntime>::get(test_lane_id()).unwrap();
		inbound_lane_state.relayers.push_back(UnrewardedRelayer {
			relayer: Default::default(),
			messages: DeliveredMessages {
				begin: 1,
				end: BridgedChain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
			},
		});
		InboundLanes::<TestRuntime>::insert(test_lane_id(), inbound_lane_state);
	}

	fn deliver_message_10() {
		InboundLanes::<TestRuntime>::insert(
			test_lane_id(),
			bp_messages::InboundLaneData {
				state: LaneState::Opened,
				relayers: Default::default(),
				last_confirmed_nonce: 10,
			},
		);
	}

	fn validate_message_delivery(
		nonces_start: bp_messages::MessageNonce,
		nonces_end: bp_messages::MessageNonce,
	) -> bool {
		RuntimeCall::Messages(crate::Call::<TestRuntime, ()>::receive_messages_proof {
			relayer_id_at_bridged_chain: 42,
			messages_count: nonces_end.checked_sub(nonces_start).map(|x| x + 1).unwrap_or(0) as u32,
			dispatch_weight: frame_support::weights::Weight::zero(),
			proof: Box::new(FromBridgedChainMessagesProof {
				bridged_header_hash: Default::default(),
				storage_proof: Default::default(),
				lane: test_lane_id(),
				nonces_start,
				nonces_end,
			}),
		})
		.check_obsolete_call()
		.is_ok()
	}

	fn run_test<T>(test: impl Fn() -> T) -> T {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			InboundLanes::<TestRuntime>::insert(test_lane_id(), InboundLaneData::opened());
			OutboundLanes::<TestRuntime>::insert(test_lane_id(), OutboundLaneData::opened());
			test()
		})
	}

	#[test]
	fn extension_rejects_obsolete_messages() {
		run_test(|| {
			// when current best delivered is message#10 and we're trying to deliver messages 8..=9
			// => tx is rejected
			deliver_message_10();
			assert!(!validate_message_delivery(8, 9));
		});
	}

	#[test]
	fn extension_rejects_same_message() {
		run_test(|| {
			// when current best delivered is message#10 and we're trying to import messages 10..=10
			// => tx is rejected
			deliver_message_10();
			assert!(!validate_message_delivery(8, 10));
		});
	}

	#[test]
	fn extension_rejects_call_with_some_obsolete_messages() {
		run_test(|| {
			// when current best delivered is message#10 and we're trying to deliver messages
			// 10..=15 => tx is rejected
			deliver_message_10();
			assert!(!validate_message_delivery(10, 15));
		});
	}

	#[test]
	fn extension_rejects_call_with_future_messages() {
		run_test(|| {
			// when current best delivered is message#10 and we're trying to deliver messages
			// 13..=15 => tx is rejected
			deliver_message_10();
			assert!(!validate_message_delivery(13, 15));
		});
	}

	#[test]
	fn extension_reject_call_when_dispatcher_is_inactive() {
		run_test(|| {
			// when current best delivered is message#10 and we're trying to deliver message 11..=15
			// => tx is accepted, but we have inactive dispatcher, so...
			deliver_message_10();

			TestMessageDispatch::deactivate(test_lane_id());
			assert!(!validate_message_delivery(11, 15));
		});
	}

	#[test]
	fn extension_rejects_empty_delivery_with_rewards_confirmations_if_there_are_free_relayer_and_message_slots(
	) {
		run_test(|| {
			deliver_message_10();
			assert!(!validate_message_delivery(10, 9));
		});
	}

	#[test]
	fn extension_accepts_empty_delivery_with_rewards_confirmations_if_there_are_no_free_relayer_slots(
	) {
		run_test(|| {
			deliver_message_10();
			fill_unrewarded_relayers();
			assert!(validate_message_delivery(10, 9));
		});
	}

	#[test]
	fn extension_accepts_empty_delivery_with_rewards_confirmations_if_there_are_no_free_message_slots(
	) {
		run_test(|| {
			fill_unrewarded_messages();
			assert!(validate_message_delivery(
				BridgedChain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
				BridgedChain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX - 1
			));
		});
	}

	#[test]
	fn extension_accepts_new_messages() {
		run_test(|| {
			// when current best delivered is message#10 and we're trying to deliver message 11..=15
			// => tx is accepted
			deliver_message_10();
			assert!(validate_message_delivery(11, 15));
		});
	}

	fn confirm_message_10() {
		OutboundLanes::<TestRuntime>::insert(
			test_lane_id(),
			bp_messages::OutboundLaneData {
				state: LaneState::Opened,
				oldest_unpruned_nonce: 0,
				latest_received_nonce: 10,
				latest_generated_nonce: 10,
			},
		);
	}

	fn validate_message_confirmation(last_delivered_nonce: bp_messages::MessageNonce) -> bool {
		RuntimeCall::Messages(crate::Call::<TestRuntime>::receive_messages_delivery_proof {
			proof: FromBridgedChainMessagesDeliveryProof {
				bridged_header_hash: Default::default(),
				storage_proof: Default::default(),
				lane: test_lane_id(),
			},
			relayers_state: UnrewardedRelayersState { last_delivered_nonce, ..Default::default() },
		})
		.check_obsolete_call()
		.is_ok()
	}

	#[test]
	fn extension_rejects_obsolete_confirmations() {
		run_test(|| {
			// when current best confirmed is message#10 and we're trying to confirm message#5 => tx
			// is rejected
			confirm_message_10();
			assert!(!validate_message_confirmation(5));
		});
	}

	#[test]
	fn extension_rejects_same_confirmation() {
		run_test(|| {
			// when current best confirmed is message#10 and we're trying to confirm message#10 =>
			// tx is rejected
			confirm_message_10();
			assert!(!validate_message_confirmation(10));
		});
	}

	#[test]
	fn extension_rejects_empty_confirmation_even_if_there_are_no_free_unrewarded_entries() {
		run_test(|| {
			confirm_message_10();
			fill_unrewarded_relayers();
			assert!(!validate_message_confirmation(10));
		});
	}

	#[test]
	fn extension_accepts_new_confirmation() {
		run_test(|| {
			// when current best confirmed is message#10 and we're trying to confirm message#15 =>
			// tx is accepted
			confirm_message_10();
			assert!(validate_message_confirmation(15));
		});
	}

	fn was_message_delivery_successful(
		bundled_range: RangeInclusive<MessageNonce>,
		is_empty: bool,
	) -> bool {
		CallHelper::<TestRuntime, ()>::was_successful(&MessagesCallInfo::ReceiveMessagesProof(
			ReceiveMessagesProofInfo {
				base: BaseMessagesProofInfo {
					lane_id: test_lane_id(),
					bundled_range,
					best_stored_nonce: 0, // doesn't matter for `was_successful`
				},
				unrewarded_relayers: UnrewardedRelayerOccupation {
					free_relayer_slots: 0, // doesn't matter for `was_successful`
					free_message_slots: if is_empty {
						0
					} else {
						BridgedChain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX
					},
				},
			},
		))
	}

	#[test]
	#[allow(clippy::reversed_empty_ranges)]
	fn was_successful_returns_false_for_failed_reward_confirmation_transaction() {
		run_test(|| {
			fill_unrewarded_messages();
			assert!(!was_message_delivery_successful(10..=9, true));
		});
	}

	#[test]
	#[allow(clippy::reversed_empty_ranges)]
	fn was_successful_returns_true_for_successful_reward_confirmation_transaction() {
		run_test(|| {
			assert!(was_message_delivery_successful(10..=9, true));
		});
	}

	#[test]
	fn was_successful_returns_false_for_failed_delivery() {
		run_test(|| {
			deliver_message_10();
			assert!(!was_message_delivery_successful(10..=12, false));
		});
	}

	#[test]
	fn was_successful_returns_false_for_partially_successful_delivery() {
		run_test(|| {
			deliver_message_10();
			assert!(!was_message_delivery_successful(9..=12, false));
		});
	}

	#[test]
	fn was_successful_returns_true_for_successful_delivery() {
		run_test(|| {
			deliver_message_10();
			assert!(was_message_delivery_successful(9..=10, false));
		});
	}

	fn was_message_confirmation_successful(bundled_range: RangeInclusive<MessageNonce>) -> bool {
		CallHelper::<TestRuntime, ()>::was_successful(
			&MessagesCallInfo::ReceiveMessagesDeliveryProof(ReceiveMessagesDeliveryProofInfo(
				BaseMessagesProofInfo {
					lane_id: test_lane_id(),
					bundled_range,
					best_stored_nonce: 0, // doesn't matter for `was_successful`
				},
			)),
		)
	}

	#[test]
	fn was_successful_returns_false_for_failed_confirmation() {
		run_test(|| {
			confirm_message_10();
			assert!(!was_message_confirmation_successful(10..=12));
		});
	}

	#[test]
	fn was_successful_returns_false_for_partially_successful_confirmation() {
		run_test(|| {
			confirm_message_10();
			assert!(!was_message_confirmation_successful(9..=12));
		});
	}

	#[test]
	fn was_successful_returns_true_for_successful_confirmation() {
		run_test(|| {
			confirm_message_10();
			assert!(was_message_confirmation_successful(9..=10));
		});
	}
}
