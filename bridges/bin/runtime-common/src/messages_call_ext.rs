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

/// Generic info about a messages delivery/confirmation proof.
#[derive(PartialEq, RuntimeDebug)]
pub struct BaseMessagesProofInfo {
	pub lane_id: LaneId,
	pub best_bundled_nonce: MessageNonce,
	pub best_stored_nonce: MessageNonce,
}

impl BaseMessagesProofInfo {
	fn is_obsolete(&self) -> bool {
		self.best_bundled_nonce <= self.best_stored_nonce
	}
}

/// Info about a `ReceiveMessagesProof` call which tries to update a single lane.
#[derive(PartialEq, RuntimeDebug)]
pub struct ReceiveMessagesProofInfo(pub BaseMessagesProofInfo);

/// Info about a `ReceiveMessagesDeliveryProof` call which tries to update a single lane.
#[derive(PartialEq, RuntimeDebug)]
pub struct ReceiveMessagesDeliveryProofInfo(pub BaseMessagesProofInfo);

/// Info about a `ReceiveMessagesProof` or a `ReceiveMessagesDeliveryProof` call
/// which tries to update a single lane.
#[derive(PartialEq, RuntimeDebug)]
pub enum CallInfo {
	ReceiveMessagesProof(ReceiveMessagesProofInfo),
	ReceiveMessagesDeliveryProof(ReceiveMessagesDeliveryProofInfo),
}

/// Helper struct that provides methods for working with a call supported by `CallInfo`.
pub struct CallHelper<T: Config<I>, I: 'static> {
	pub _phantom_data: sp_std::marker::PhantomData<(T, I)>,
}

impl<T: Config<I>, I: 'static> CallHelper<T, I> {
	/// Check if a call delivered proof/confirmation for at least some of the messages that it
	/// contained.
	pub fn was_partially_successful(info: &CallInfo) -> bool {
		match info {
			CallInfo::ReceiveMessagesProof(info) => {
				let inbound_lane_data =
					pallet_bridge_messages::InboundLanes::<T, I>::get(info.0.lane_id);
				inbound_lane_data.last_delivered_nonce() > info.0.best_stored_nonce
			},
			CallInfo::ReceiveMessagesDeliveryProof(info) => {
				let outbound_lane_data =
					pallet_bridge_messages::OutboundLanes::<T, I>::get(info.0.lane_id);
				outbound_lane_data.latest_received_nonce > info.0.best_stored_nonce
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

	/// Check that a `ReceiveMessagesProof` or a `ReceiveMessagesDeliveryProof` call is trying
	/// to deliver/confirm at least some messages that are better than the ones we know of.
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

			return Some(ReceiveMessagesProofInfo(BaseMessagesProofInfo {
				lane_id: proof.lane,
				best_bundled_nonce: proof.nonces_end,
				best_stored_nonce: inbound_lane_data.last_delivered_nonce(),
			}))
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
				best_bundled_nonce: relayers_state.last_delivered_nonce,
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
				CallInfo::ReceiveMessagesProof(info) => info.0.lane_id,
				CallInfo::ReceiveMessagesDeliveryProof(info) => info.0.lane_id,
			};
			actual_lane_id == lane_id
		})
	}

	fn check_obsolete_call(&self) -> TransactionValidity {
		match self.call_info() {
			Some(CallInfo::ReceiveMessagesProof(proof_info)) if proof_info.0.is_obsolete() => {
				log::trace!(
					target: pallet_bridge_messages::LOG_TARGET,
					"Rejecting obsolete messages delivery transaction: {:?}",
					proof_info
				);

				return sp_runtime::transaction_validity::InvalidTransaction::Stale.into()
			},
			Some(CallInfo::ReceiveMessagesDeliveryProof(proof_info))
				if proof_info.0.is_obsolete() =>
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

#[cfg(test)]
mod tests {
	use crate::{
		messages::{
			source::FromBridgedChainMessagesDeliveryProof, target::FromBridgedChainMessagesProof,
		},
		messages_call_ext::MessagesCallSubType,
		mock::{TestRuntime, ThisChainRuntimeCall},
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
		ThisChainRuntimeCall::BridgeMessages(
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
		)
		.check_obsolete_call()
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
		ThisChainRuntimeCall::BridgeMessages(
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
	fn extension_accepts_new_confirmation() {
		sp_io::TestExternalities::new(Default::default()).execute_with(|| {
			// when current best confirmed is message#10 and we're trying to confirm message#15 =>
			// tx is accepted
			confirm_message_10();
			assert!(validate_message_confirmation(15));
		});
	}
}
