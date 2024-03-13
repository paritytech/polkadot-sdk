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

//! Signed extension for the `pallet-bridge-messages` that is able to reject obsolete
//! (and some other invalid) transactions.

use crate::messages::{
	source::FromBridgedChainMessagesDeliveryProof, target::FromBridgedChainMessagesProof,
};
use bp_messages::{target_chain::MessageDispatch, InboundLaneData, LaneId, MessageNonce};
use bp_runtime::OwnedBridgeModule;
use frame_support::{
	dispatch::CallableCallFor,
	traits::{Get, IsSubType},
};
use pallet_bridge_messages::{Config, Pallet};
use sp_runtime::{transaction_validity::TransactionValidity, RuntimeDebug};
use sp_std::ops::RangeInclusive;

/// Generic info about a messages delivery/confirmation proof.
#[derive(PartialEq, RuntimeDebug)]
pub struct BaseMessagesProofInfo {
	/// Message lane, used by the call.
	pub lane_id: LaneId,
	/// Nonces of messages, included in the call.
	///
	/// For delivery transaction, it is nonces of bundled messages. For confirmation
	/// transaction, it is nonces that are to be confirmed during the call.
	pub bundled_range: RangeInclusive<MessageNonce>,
	/// Nonce of the best message, stored by this chain before the call is dispatched.
	///
	/// For delivery transaction, it is the nonce of best delivered message before the call.
	/// For confirmation transaction, it is the nonce of best confirmed message before the call.
	pub best_stored_nonce: MessageNonce,
}

impl BaseMessagesProofInfo {
	/// Returns true if `bundled_range` continues the `0..=best_stored_nonce` range.
	fn appends_to_stored_nonce(&self) -> bool {
		Some(*self.bundled_range.start()) == self.best_stored_nonce.checked_add(1)
	}
}

/// Occupation state of the unrewarded relayers vector.
#[derive(PartialEq, RuntimeDebug)]
#[cfg_attr(test, derive(Default))]
pub struct UnrewardedRelayerOccupation {
	/// The number of remaining unoccupied entries for new relayers.
	pub free_relayer_slots: MessageNonce,
	/// The number of messages that we are ready to accept.
	pub free_message_slots: MessageNonce,
}

/// Info about a `ReceiveMessagesProof` call which tries to update a single lane.
#[derive(PartialEq, RuntimeDebug)]
pub struct ReceiveMessagesProofInfo {
	/// Base messages proof info
	pub base: BaseMessagesProofInfo,
	/// State of unrewarded relayers vector.
	pub unrewarded_relayers: UnrewardedRelayerOccupation,
}

impl ReceiveMessagesProofInfo {
	/// Returns true if:
	///
	/// - either inbound lane is ready to accept bundled messages;
	///
	/// - or there are no bundled messages, but the inbound lane is blocked by too many unconfirmed
	///   messages and/or unrewarded relayers.
	fn is_obsolete(&self, is_dispatcher_active: bool) -> bool {
		// if dispatcher is inactive, we don't accept any delivery transactions
		if !is_dispatcher_active {
			return true
		}

		// transactions with zero bundled nonces are not allowed, unless they're message
		// delivery transactions, which brings reward confirmations required to unblock
		// the lane
		if self.base.bundled_range.is_empty() {
			let empty_transactions_allowed =
				// we allow empty transactions when we can't accept delivery from new relayers
				self.unrewarded_relayers.free_relayer_slots == 0 ||
				// or if we can't accept new messages at all
				self.unrewarded_relayers.free_message_slots == 0;

			return !empty_transactions_allowed
		}

		// otherwise we require bundled messages to continue stored range
		!self.base.appends_to_stored_nonce()
	}
}

/// Info about a `ReceiveMessagesDeliveryProof` call which tries to update a single lane.
#[derive(PartialEq, RuntimeDebug)]
pub struct ReceiveMessagesDeliveryProofInfo(pub BaseMessagesProofInfo);

impl ReceiveMessagesDeliveryProofInfo {
	/// Returns true if outbound lane is ready to accept confirmations of bundled messages.
	fn is_obsolete(&self) -> bool {
		self.0.bundled_range.is_empty() || !self.0.appends_to_stored_nonce()
	}
}

/// Info about a `ReceiveMessagesProof` or a `ReceiveMessagesDeliveryProof` call
/// which tries to update a single lane.
#[derive(PartialEq, RuntimeDebug)]
pub enum CallInfo {
	/// Messages delivery call info.
	ReceiveMessagesProof(ReceiveMessagesProofInfo),
	/// Messages delivery confirmation call info.
	ReceiveMessagesDeliveryProof(ReceiveMessagesDeliveryProofInfo),
}

impl CallInfo {
	/// Returns range of messages, bundled with the call.
	pub fn bundled_messages(&self) -> RangeInclusive<MessageNonce> {
		match *self {
			Self::ReceiveMessagesProof(ref info) => info.base.bundled_range.clone(),
			Self::ReceiveMessagesDeliveryProof(ref info) => info.0.bundled_range.clone(),
		}
	}
}

/// Helper struct that provides methods for working with a call supported by `CallInfo`.
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
	pub fn was_successful(info: &CallInfo) -> bool {
		match info {
			CallInfo::ReceiveMessagesProof(info) => {
				let inbound_lane_data =
					pallet_bridge_messages::InboundLanes::<T, I>::get(info.base.lane_id);
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
			CallInfo::ReceiveMessagesDeliveryProof(info) => {
				let outbound_lane_data =
					pallet_bridge_messages::OutboundLanes::<T, I>::get(info.0.lane_id);
				outbound_lane_data.latest_received_nonce == *info.0.bundled_range.end()
			},
		}
	}
}

/// Trait representing a call that is a sub type of `pallet_bridge_messages::Call`.
pub trait MessagesCallSubType<T: Config<I, RuntimeCall = Self>, I: 'static>:
	IsSubType<CallableCallFor<Pallet<T, I>, T>>
{
	/// Create a new instance of `ReceiveMessagesProofInfo` from a `ReceiveMessagesProof` call.
	fn receive_messages_proof_info(&self) -> Option<ReceiveMessagesProofInfo>;

	/// Create a new instance of `ReceiveMessagesDeliveryProofInfo` from
	/// a `ReceiveMessagesDeliveryProof` call.
	fn receive_messages_delivery_proof_info(&self) -> Option<ReceiveMessagesDeliveryProofInfo>;

	/// Create a new instance of `CallInfo` from a `ReceiveMessagesProof`
	/// or a `ReceiveMessagesDeliveryProof` call.
	fn call_info(&self) -> Option<CallInfo>;

	/// Create a new instance of `CallInfo` from a `ReceiveMessagesProof`
	/// or a `ReceiveMessagesDeliveryProof` call, if the call is for the provided lane.
	fn call_info_for(&self, lane_id: LaneId) -> Option<CallInfo>;

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
		BridgedHeaderHash,
		SourceHeaderChain: bp_messages::target_chain::SourceHeaderChain<
			MessagesProof = FromBridgedChainMessagesProof<BridgedHeaderHash>,
		>,
		TargetHeaderChain: bp_messages::source_chain::TargetHeaderChain<
			<T as Config<I>>::OutboundPayload,
			<T as frame_system::Config>::AccountId,
			MessagesDeliveryProof = FromBridgedChainMessagesDeliveryProof<BridgedHeaderHash>,
		>,
		Call: IsSubType<CallableCallFor<Pallet<T, I>, T>>,
		T: frame_system::Config<RuntimeCall = Call>
			+ Config<I, SourceHeaderChain = SourceHeaderChain, TargetHeaderChain = TargetHeaderChain>,
		I: 'static,
	> MessagesCallSubType<T, I> for T::RuntimeCall
{
	fn receive_messages_proof_info(&self) -> Option<ReceiveMessagesProofInfo> {
		if let Some(pallet_bridge_messages::Call::<T, I>::receive_messages_proof {
			ref proof,
			..
		}) = self.is_sub_type()
		{
			let inbound_lane_data = pallet_bridge_messages::InboundLanes::<T, I>::get(proof.lane);

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

	fn receive_messages_delivery_proof_info(&self) -> Option<ReceiveMessagesDeliveryProofInfo> {
		if let Some(pallet_bridge_messages::Call::<T, I>::receive_messages_delivery_proof {
			ref proof,
			ref relayers_state,
			..
		}) = self.is_sub_type()
		{
			let outbound_lane_data = pallet_bridge_messages::OutboundLanes::<T, I>::get(proof.lane);

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

	fn call_info(&self) -> Option<CallInfo> {
		if let Some(info) = self.receive_messages_proof_info() {
			return Some(CallInfo::ReceiveMessagesProof(info))
		}

		if let Some(info) = self.receive_messages_delivery_proof_info() {
			return Some(CallInfo::ReceiveMessagesDeliveryProof(info))
		}

		None
	}

	fn call_info_for(&self, lane_id: LaneId) -> Option<CallInfo> {
		self.call_info().filter(|info| {
			let actual_lane_id = match info {
				CallInfo::ReceiveMessagesProof(info) => info.base.lane_id,
				CallInfo::ReceiveMessagesDeliveryProof(info) => info.0.lane_id,
			};
			actual_lane_id == lane_id
		})
	}

	fn check_obsolete_call(&self) -> TransactionValidity {
		let is_pallet_halted = Pallet::<T, I>::ensure_not_halted().is_err();
		match self.call_info() {
			Some(proof_info) if is_pallet_halted => {
				log::trace!(
					target: pallet_bridge_messages::LOG_TARGET,
					"Rejecting messages transaction on halted pallet: {:?}",
					proof_info
				);

				return sp_runtime::transaction_validity::InvalidTransaction::Call.into()
			},
			Some(CallInfo::ReceiveMessagesProof(proof_info))
				if proof_info.is_obsolete(T::MessageDispatch::is_active()) =>
			{
				log::trace!(
					target: pallet_bridge_messages::LOG_TARGET,
					"Rejecting obsolete messages delivery transaction: {:?}",
					proof_info
				);

				return sp_runtime::transaction_validity::InvalidTransaction::Stale.into()
			},
			Some(CallInfo::ReceiveMessagesDeliveryProof(proof_info))
				if proof_info.is_obsolete() =>
			{
				log::trace!(
					target: pallet_bridge_messages::LOG_TARGET,
					"Rejecting obsolete messages confirmation transaction: {:?}",
					proof_info,
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
	inbound_lane_data: &InboundLaneData<T::InboundRelayer>,
) -> UnrewardedRelayerOccupation {
	UnrewardedRelayerOccupation {
		free_relayer_slots: T::MaxUnrewardedRelayerEntriesAtInboundLane::get()
			.saturating_sub(inbound_lane_data.relayers.len() as MessageNonce),
		free_message_slots: {
			let unconfirmed_messages = inbound_lane_data
				.last_delivered_nonce()
				.saturating_sub(inbound_lane_data.last_confirmed_nonce);
			T::MaxUnconfirmedMessagesAtInboundLane::get().saturating_sub(unconfirmed_messages)
		},
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		messages::{
			source::FromBridgedChainMessagesDeliveryProof, target::FromBridgedChainMessagesProof,
		},
		messages_call_ext::MessagesCallSubType,
		mock::{
			DummyMessageDispatch, MaxUnconfirmedMessagesAtInboundLane,
			MaxUnrewardedRelayerEntriesAtInboundLane, TestRuntime, ThisChainRuntimeCall,
		},
	};
	use bp_messages::{DeliveredMessages, UnrewardedRelayer, UnrewardedRelayersState};
	use sp_std::ops::RangeInclusive;

	fn fill_unrewarded_relayers() {
		let mut inbound_lane_state =
			pallet_bridge_messages::InboundLanes::<TestRuntime>::get(LaneId([0, 0, 0, 0]));
		for n in 0..MaxUnrewardedRelayerEntriesAtInboundLane::get() {
			inbound_lane_state.relayers.push_back(UnrewardedRelayer {
				relayer: Default::default(),
				messages: DeliveredMessages { begin: n + 1, end: n + 1 },
			});
		}
		pallet_bridge_messages::InboundLanes::<TestRuntime>::insert(
			LaneId([0, 0, 0, 0]),
			inbound_lane_state,
		);
	}

	fn fill_unrewarded_messages() {
		let mut inbound_lane_state =
			pallet_bridge_messages::InboundLanes::<TestRuntime>::get(LaneId([0, 0, 0, 0]));
		inbound_lane_state.relayers.push_back(UnrewardedRelayer {
			relayer: Default::default(),
			messages: DeliveredMessages {
				begin: 1,
				end: MaxUnconfirmedMessagesAtInboundLane::get(),
			},
		});
		pallet_bridge_messages::InboundLanes::<TestRuntime>::insert(
			LaneId([0, 0, 0, 0]),
			inbound_lane_state,
		);
	}

	fn deliver_message_10() {
		pallet_bridge_messages::InboundLanes::<TestRuntime>::insert(
			LaneId([0, 0, 0, 0]),
			bp_messages::InboundLaneData { relayers: Default::default(), last_confirmed_nonce: 10 },
		);
	}

	fn validate_message_delivery(
		nonces_start: bp_messages::MessageNonce,
		nonces_end: bp_messages::MessageNonce,
	) -> bool {
		ThisChainRuntimeCall::BridgeMessages(
			pallet_bridge_messages::Call::<TestRuntime, ()>::receive_messages_proof {
				relayer_id_at_bridged_chain: 42,
				messages_count: nonces_end.checked_sub(nonces_start).map(|x| x + 1).unwrap_or(0)
					as u32,
				dispatch_weight: frame_support::weights::Weight::zero(),
				proof: FromBridgedChainMessagesProof {
					bridged_header_hash: Default::default(),
					storage_proof: vec![],
					lane: LaneId([0, 0, 0, 0]),
					nonces_start,
					nonces_end,
				},
			},
		)
		.check_obsolete_call()
		.is_ok()
	}

	#[test]
	fn extension_rejects_obsolete_messages() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			// when current best delivered is message#10 and we're trying to deliver messages 8..=9
			// => tx is rejected
			deliver_message_10();
			assert!(!validate_message_delivery(8, 9));
		});
	}

	#[test]
	fn extension_rejects_same_message() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			// when current best delivered is message#10 and we're trying to import messages 10..=10
			// => tx is rejected
			deliver_message_10();
			assert!(!validate_message_delivery(8, 10));
		});
	}

	#[test]
	fn extension_rejects_call_with_some_obsolete_messages() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			// when current best delivered is message#10 and we're trying to deliver messages
			// 10..=15 => tx is rejected
			deliver_message_10();
			assert!(!validate_message_delivery(10, 15));
		});
	}

	#[test]
	fn extension_rejects_call_with_future_messages() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			// when current best delivered is message#10 and we're trying to deliver messages
			// 13..=15 => tx is rejected
			deliver_message_10();
			assert!(!validate_message_delivery(13, 15));
		});
	}

	#[test]
	fn extension_reject_call_when_dispatcher_is_inactive() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			// when current best delivered is message#10 and we're trying to deliver message 11..=15
			// => tx is accepted, but we have inactive dispatcher, so...
			deliver_message_10();

			DummyMessageDispatch::deactivate();
			assert!(!validate_message_delivery(11, 15));
		});
	}

	#[test]
	fn extension_rejects_empty_delivery_with_rewards_confirmations_if_there_are_free_relayer_and_message_slots(
	) {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			deliver_message_10();
			assert!(!validate_message_delivery(10, 9));
		});
	}

	#[test]
	fn extension_accepts_empty_delivery_with_rewards_confirmations_if_there_are_no_free_relayer_slots(
	) {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			deliver_message_10();
			fill_unrewarded_relayers();
			assert!(validate_message_delivery(10, 9));
		});
	}

	#[test]
	fn extension_accepts_empty_delivery_with_rewards_confirmations_if_there_are_no_free_message_slots(
	) {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			fill_unrewarded_messages();
			assert!(validate_message_delivery(
				MaxUnconfirmedMessagesAtInboundLane::get(),
				MaxUnconfirmedMessagesAtInboundLane::get() - 1
			));
		});
	}

	#[test]
	fn extension_accepts_new_messages() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			// when current best delivered is message#10 and we're trying to deliver message 11..=15
			// => tx is accepted
			deliver_message_10();
			assert!(validate_message_delivery(11, 15));
		});
	}

	fn confirm_message_10() {
		pallet_bridge_messages::OutboundLanes::<TestRuntime>::insert(
			LaneId([0, 0, 0, 0]),
			bp_messages::OutboundLaneData {
				oldest_unpruned_nonce: 0,
				latest_received_nonce: 10,
				latest_generated_nonce: 10,
			},
		);
	}

	fn validate_message_confirmation(last_delivered_nonce: bp_messages::MessageNonce) -> bool {
		ThisChainRuntimeCall::BridgeMessages(
			pallet_bridge_messages::Call::<TestRuntime>::receive_messages_delivery_proof {
				proof: FromBridgedChainMessagesDeliveryProof {
					bridged_header_hash: Default::default(),
					storage_proof: Vec::new(),
					lane: LaneId([0, 0, 0, 0]),
				},
				relayers_state: UnrewardedRelayersState {
					last_delivered_nonce,
					..Default::default()
				},
			},
		)
		.check_obsolete_call()
		.is_ok()
	}

	#[test]
	fn extension_rejects_obsolete_confirmations() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			// when current best confirmed is message#10 and we're trying to confirm message#5 => tx
			// is rejected
			confirm_message_10();
			assert!(!validate_message_confirmation(5));
		});
	}

	#[test]
	fn extension_rejects_same_confirmation() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			// when current best confirmed is message#10 and we're trying to confirm message#10 =>
			// tx is rejected
			confirm_message_10();
			assert!(!validate_message_confirmation(10));
		});
	}

	#[test]
	fn extension_rejects_empty_confirmation_even_if_there_are_no_free_unrewarded_entries() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			confirm_message_10();
			fill_unrewarded_relayers();
			assert!(!validate_message_confirmation(10));
		});
	}

	#[test]
	fn extension_accepts_new_confirmation() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
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
		CallHelper::<TestRuntime, ()>::was_successful(&CallInfo::ReceiveMessagesProof(
			ReceiveMessagesProofInfo {
				base: BaseMessagesProofInfo {
					lane_id: LaneId([0, 0, 0, 0]),
					bundled_range,
					best_stored_nonce: 0, // doesn't matter for `was_successful`
				},
				unrewarded_relayers: UnrewardedRelayerOccupation {
					free_relayer_slots: 0, // doesn't matter for `was_successful`
					free_message_slots: if is_empty {
						0
					} else {
						MaxUnconfirmedMessagesAtInboundLane::get()
					},
				},
			},
		))
	}

	#[test]
	#[allow(clippy::reversed_empty_ranges)]
	fn was_successful_returns_false_for_failed_reward_confirmation_transaction() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			fill_unrewarded_messages();
			assert!(!was_message_delivery_successful(10..=9, true));
		});
	}

	#[test]
	#[allow(clippy::reversed_empty_ranges)]
	fn was_successful_returns_true_for_successful_reward_confirmation_transaction() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			assert!(was_message_delivery_successful(10..=9, true));
		});
	}

	#[test]
	fn was_successful_returns_false_for_failed_delivery() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			deliver_message_10();
			assert!(!was_message_delivery_successful(10..=12, false));
		});
	}

	#[test]
	fn was_successful_returns_false_for_partially_successful_delivery() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			deliver_message_10();
			assert!(!was_message_delivery_successful(9..=12, false));
		});
	}

	#[test]
	fn was_successful_returns_true_for_successful_delivery() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			deliver_message_10();
			assert!(was_message_delivery_successful(9..=10, false));
		});
	}

	fn was_message_confirmation_successful(bundled_range: RangeInclusive<MessageNonce>) -> bool {
		CallHelper::<TestRuntime, ()>::was_successful(&CallInfo::ReceiveMessagesDeliveryProof(
			ReceiveMessagesDeliveryProofInfo(BaseMessagesProofInfo {
				lane_id: LaneId([0, 0, 0, 0]),
				bundled_range,
				best_stored_nonce: 0, // doesn't matter for `was_successful`
			}),
		))
	}

	#[test]
	fn was_successful_returns_false_for_failed_confirmation() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			confirm_message_10();
			assert!(!was_message_confirmation_successful(10..=12));
		});
	}

	#[test]
	fn was_successful_returns_false_for_partially_successful_confirmation() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			confirm_message_10();
			assert!(!was_message_confirmation_successful(9..=12));
		});
	}

	#[test]
	fn was_successful_returns_true_for_successful_confirmation() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			confirm_message_10();
			assert!(was_message_confirmation_successful(9..=10));
		});
	}
}
