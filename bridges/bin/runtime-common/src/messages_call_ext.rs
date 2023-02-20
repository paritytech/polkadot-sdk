// Copyright 2021 Parity Technologies (UK) Ltd.
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

use crate::messages::{
	source::FromBridgedChainMessagesDeliveryProof, target::FromBridgedChainMessagesProof,
};
use bp_messages::{LaneId, MessageNonce};
use frame_support::{dispatch::CallableCallFor, traits::IsSubType, RuntimeDebug};
use pallet_bridge_messages::{Config, Pallet};
use sp_runtime::transaction_validity::TransactionValidity;

/// Info about a `ReceiveMessagesProof` call which tries to update a single lane.
#[derive(PartialEq, RuntimeDebug)]
pub struct ReceiveMessagesProofInfo {
	pub lane_id: LaneId,
	pub best_proof_nonce: MessageNonce,
	pub best_stored_nonce: MessageNonce,
}

/// Helper struct that provides methods for working with the `ReceiveMessagesProof` call.
pub struct ReceiveMessagesProofHelper<T: Config<I>, I: 'static> {
	pub _phantom_data: sp_std::marker::PhantomData<(T, I)>,
}

impl<T: Config<I>, I: 'static> ReceiveMessagesProofHelper<T, I> {
	/// Check if the `ReceiveMessagesProof` call delivered at least some of the messages that
	/// it contained.
	pub fn was_partially_successful(info: &ReceiveMessagesProofInfo) -> bool {
		let inbound_lane_data = pallet_bridge_messages::InboundLanes::<T, I>::get(info.lane_id);
		inbound_lane_data.last_delivered_nonce() > info.best_stored_nonce
	}
}

/// Trait representing a call that is a sub type of `pallet_bridge_messages::Call`.
pub trait MessagesCallSubType<T: Config<I, RuntimeCall = Self>, I: 'static>:
	IsSubType<CallableCallFor<Pallet<T, I>, T>>
{
	/// Create a new instance of `ReceiveMessagesProofInfo` from a `ReceiveMessagesProof` call.
	fn receive_messages_proof_info(&self) -> Option<ReceiveMessagesProofInfo>;

	/// Create a new instance of `ReceiveMessagesProofInfo` from a `ReceiveMessagesProof` call,
	/// if the call is for the provided lane.
	fn receive_messages_proof_info_for(&self, lane_id: LaneId) -> Option<ReceiveMessagesProofInfo>;

	/// Check that a `ReceiveMessagesProof` call is trying to deliver at least some messages that
	/// are better than the ones we know of.
	fn check_obsolete_receive_messages_proof(&self) -> TransactionValidity;

	/// Check that a `ReceiveMessagesDeliveryProof` call is trying to deliver at least some message
	/// confirmations that are better than the ones we know of.
	fn check_obsolete_receive_messages_delivery_proof(&self) -> TransactionValidity;
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
				lane_id: proof.lane,
				best_proof_nonce: proof.nonces_end,
				best_stored_nonce: inbound_lane_data.last_delivered_nonce(),
			})
		}

		None
	}

	fn receive_messages_proof_info_for(&self, lane_id: LaneId) -> Option<ReceiveMessagesProofInfo> {
		self.receive_messages_proof_info().filter(|info| info.lane_id == lane_id)
	}

	fn check_obsolete_receive_messages_proof(&self) -> TransactionValidity {
		if let Some(proof_info) = self.receive_messages_proof_info() {
			if proof_info.best_proof_nonce <= proof_info.best_stored_nonce {
				log::trace!(
					target: pallet_bridge_messages::LOG_TARGET,
					"Rejecting obsolete messages delivery transaction: \
                            lane {:?}, bundled {:?}, best {:?}",
					proof_info.lane_id,
					proof_info.best_proof_nonce,
					proof_info.best_stored_nonce,
				);

				return sp_runtime::transaction_validity::InvalidTransaction::Stale.into()
			}
		}

		Ok(sp_runtime::transaction_validity::ValidTransaction::default())
	}

	fn check_obsolete_receive_messages_delivery_proof(&self) -> TransactionValidity {
		if let Some(pallet_bridge_messages::Call::<T, I>::receive_messages_delivery_proof {
			ref proof,
			ref relayers_state,
			..
		}) = self.is_sub_type()
		{
			let outbound_lane_data = pallet_bridge_messages::OutboundLanes::<T, I>::get(proof.lane);
			if relayers_state.last_delivered_nonce <= outbound_lane_data.latest_received_nonce {
				log::trace!(
					target: pallet_bridge_messages::LOG_TARGET,
					"Rejecting obsolete messages confirmation transaction: \
                            lane {:?}, bundled {:?}, best {:?}",
					proof.lane,
					relayers_state.last_delivered_nonce,
					outbound_lane_data.latest_received_nonce,
				);

				return sp_runtime::transaction_validity::InvalidTransaction::Stale.into()
			}
		}

		Ok(sp_runtime::transaction_validity::ValidTransaction::default())
	}
}

#[cfg(test)]
mod tests {
	use crate::{
		messages::{
			source::FromBridgedChainMessagesDeliveryProof, target::FromBridgedChainMessagesProof,
		},
		mock::{TestRuntime, ThisChainRuntimeCall},
		BridgeRuntimeFilterCall,
	};
	use bp_messages::UnrewardedRelayersState;

	fn deliver_message_10() {
		pallet_bridge_messages::InboundLanes::<TestRuntime>::insert(
			bp_messages::LaneId([0, 0, 0, 0]),
			bp_messages::InboundLaneData { relayers: Default::default(), last_confirmed_nonce: 10 },
		);
	}

	fn validate_message_delivery(
		nonces_start: bp_messages::MessageNonce,
		nonces_end: bp_messages::MessageNonce,
	) -> bool {
		pallet_bridge_messages::Pallet::<TestRuntime>::validate(
			&ThisChainRuntimeCall::BridgeMessages(
				pallet_bridge_messages::Call::<TestRuntime, ()>::receive_messages_proof {
					relayer_id_at_bridged_chain: 42,
					messages_count: (nonces_end - nonces_start + 1) as u32,
					dispatch_weight: frame_support::weights::Weight::zero(),
					proof: FromBridgedChainMessagesProof {
						bridged_header_hash: Default::default(),
						storage_proof: vec![],
						lane: bp_messages::LaneId([0, 0, 0, 0]),
						nonces_start,
						nonces_end,
					},
				},
			),
		)
		.is_ok()
	}

	#[test]
	fn extension_rejects_obsolete_messages() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			// when current best delivered is message#10 and we're trying to deliver message#5 => tx
			// is rejected
			deliver_message_10();
			assert!(!validate_message_delivery(8, 9));
		});
	}

	#[test]
	fn extension_rejects_same_message() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			// when current best delivered is message#10 and we're trying to import message#10 => tx
			// is rejected
			deliver_message_10();
			assert!(!validate_message_delivery(8, 10));
		});
	}

	#[test]
	fn extension_accepts_new_message() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			// when current best delivered is message#10 and we're trying to deliver message#15 =>
			// tx is accepted
			deliver_message_10();
			assert!(validate_message_delivery(10, 15));
		});
	}

	fn confirm_message_10() {
		pallet_bridge_messages::OutboundLanes::<TestRuntime>::insert(
			bp_messages::LaneId([0, 0, 0, 0]),
			bp_messages::OutboundLaneData {
				oldest_unpruned_nonce: 0,
				latest_received_nonce: 10,
				latest_generated_nonce: 10,
			},
		);
	}

	fn validate_message_confirmation(last_delivered_nonce: bp_messages::MessageNonce) -> bool {
		pallet_bridge_messages::Pallet::<TestRuntime>::validate(
			&ThisChainRuntimeCall::BridgeMessages(
				pallet_bridge_messages::Call::<TestRuntime>::receive_messages_delivery_proof {
					proof: FromBridgedChainMessagesDeliveryProof {
						bridged_header_hash: Default::default(),
						storage_proof: Vec::new(),
						lane: bp_messages::LaneId([0, 0, 0, 0]),
					},
					relayers_state: UnrewardedRelayersState {
						last_delivered_nonce,
						..Default::default()
					},
				},
			),
		)
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
	fn extension_accepts_new_confirmation() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			// when current best confirmed is message#10 and we're trying to confirm message#15 =>
			// tx is accepted
			confirm_message_10();
			assert!(validate_message_confirmation(15));
		});
	}
}
