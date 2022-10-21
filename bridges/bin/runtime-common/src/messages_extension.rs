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

use crate::{
	messages::{
		source::FromBridgedChainMessagesDeliveryProof, target::FromBridgedChainMessagesProof,
	},
	BridgeRuntimeFilterCall,
};
use frame_support::{dispatch::CallableCallFor, traits::IsSubType};
use pallet_bridge_messages::{Config, Pallet};
use sp_runtime::transaction_validity::TransactionValidity;

/// Validate messages in order to avoid "mining" messages delivery and delivery confirmation
/// transactions, that are delivering outdated messages/confirmations. Without this validation,
/// even honest relayers may lose their funds if there are multiple relays running and submitting
/// the same messages/confirmations.
impl<
		BridgedHeaderHash,
		SourceHeaderChain: bp_messages::target_chain::SourceHeaderChain<
			<T as Config<I>>::InboundMessageFee,
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
	> BridgeRuntimeFilterCall<Call> for Pallet<T, I>
{
	fn validate(call: &Call) -> TransactionValidity {
		match call.is_sub_type() {
			Some(pallet_bridge_messages::Call::<T, I>::receive_messages_proof {
				ref proof,
				..
			}) => {
				let inbound_lane_data =
					pallet_bridge_messages::InboundLanes::<T, I>::get(proof.lane);
				if proof.nonces_end <= inbound_lane_data.last_delivered_nonce() {
					log::trace!(
						target: pallet_bridge_messages::LOG_TARGET,
						"Rejecting obsolete messages delivery transaction: \
                            lane {:?}, bundled {:?}, best {:?}",
						proof.lane,
						proof.nonces_end,
						inbound_lane_data.last_delivered_nonce(),
					);

					return sp_runtime::transaction_validity::InvalidTransaction::Stale.into()
				}
			},
			Some(pallet_bridge_messages::Call::<T, I>::receive_messages_delivery_proof {
				ref proof,
				ref relayers_state,
				..
			}) => {
				let latest_delivered_nonce = relayers_state.last_delivered_nonce;

				let outbound_lane_data =
					pallet_bridge_messages::OutboundLanes::<T, I>::get(proof.lane);
				if latest_delivered_nonce <= outbound_lane_data.latest_received_nonce {
					log::trace!(
						target: pallet_bridge_messages::LOG_TARGET,
						"Rejecting obsolete messages confirmation transaction: \
                            lane {:?}, bundled {:?}, best {:?}",
						proof.lane,
						latest_delivered_nonce,
						outbound_lane_data.latest_received_nonce,
					);

					return sp_runtime::transaction_validity::InvalidTransaction::Stale.into()
				}
			},
			_ => {},
		}

		Ok(sp_runtime::transaction_validity::ValidTransaction::default())
	}
}

#[cfg(test)]
mod tests {
	use bp_messages::UnrewardedRelayersState;
	use millau_runtime::{
		bridge_runtime_common::{
			messages::{
				source::FromBridgedChainMessagesDeliveryProof,
				target::FromBridgedChainMessagesProof,
			},
			BridgeRuntimeFilterCall,
		},
		Runtime, RuntimeCall, WithRialtoMessagesInstance,
	};

	fn deliver_message_10() {
		pallet_bridge_messages::InboundLanes::<Runtime, WithRialtoMessagesInstance>::insert(
			[0, 0, 0, 0],
			bp_messages::InboundLaneData { relayers: Default::default(), last_confirmed_nonce: 10 },
		);
	}

	fn validate_message_delivery(
		nonces_start: bp_messages::MessageNonce,
		nonces_end: bp_messages::MessageNonce,
	) -> bool {
		pallet_bridge_messages::Pallet::<Runtime, WithRialtoMessagesInstance>::validate(
			&RuntimeCall::BridgeRialtoMessages(
				pallet_bridge_messages::Call::<Runtime, ()>::receive_messages_proof {
					relayer_id_at_bridged_chain: [0u8; 32].into(),
					messages_count: (nonces_end - nonces_start + 1) as u32,
					dispatch_weight: frame_support::weights::Weight::zero(),
					proof: FromBridgedChainMessagesProof {
						bridged_header_hash: Default::default(),
						storage_proof: vec![],
						lane: [0, 0, 0, 0],
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
		pallet_bridge_messages::OutboundLanes::<Runtime, WithRialtoMessagesInstance>::insert(
			[0, 0, 0, 0],
			bp_messages::OutboundLaneData {
				oldest_unpruned_nonce: 0,
				latest_received_nonce: 10,
				latest_generated_nonce: 10,
			},
		);
	}

	fn validate_message_confirmation(last_delivered_nonce: bp_messages::MessageNonce) -> bool {
		pallet_bridge_messages::Pallet::<Runtime, WithRialtoMessagesInstance>::validate(
			&RuntimeCall::BridgeRialtoMessages(pallet_bridge_messages::Call::<
				Runtime,
				WithRialtoMessagesInstance,
			>::receive_messages_delivery_proof {
				proof: FromBridgedChainMessagesDeliveryProof {
					bridged_header_hash: Default::default(),
					storage_proof: Vec::new(),
					lane: [0, 0, 0, 0],
				},
				relayers_state: UnrewardedRelayersState {
					last_delivered_nonce,
					..Default::default()
				},
			}),
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
