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

/// Declares a runtime-specific `BridgeRejectObsoleteMessages` and
/// `BridgeRejectObsoleteMessageConfirmations` signed extensions.
///
/// ## Example
///
/// ```nocompile
/// bridge_runtime_common::declare_bridge_reject_obsolete_messages!{
///     Runtime,
///     Call::BridgeRialtoMessages => WithRialtoMessagesInstance,
///     Call::BridgeRialtoParachainMessages => WithRialtoParachainMessagesInstance,
/// }
/// ```
///
/// The goal of this extension is to avoid "mining" messages delivery and delivery confirmation
/// transactions, that are delivering outdated messages/confirmations. Without that extension,
/// even honest relayers may lose their funds if there are multiple relays running and submitting
/// the same messages/confirmations.
#[macro_export]
macro_rules! declare_bridge_reject_obsolete_messages {
	($runtime:ident, $($call:path => $instance:ty),*) => {
		/// Transaction-with-obsolete-messages check that will reject transaction if
		/// it submits obsolete messages/confirmations.
		#[derive(Clone, codec::Decode, codec::Encode, Eq, PartialEq, frame_support::RuntimeDebug, scale_info::TypeInfo)]
		pub struct BridgeRejectObsoleteMessages;

		impl sp_runtime::traits::SignedExtension for BridgeRejectObsoleteMessages {
			const IDENTIFIER: &'static str = "BridgeRejectObsoleteMessages";
			type AccountId = <$runtime as frame_system::Config>::AccountId;
			type Call = <$runtime as frame_system::Config>::Call;
			type AdditionalSigned = ();
			type Pre = ();

			fn additional_signed(&self) -> sp_std::result::Result<
				(),
				sp_runtime::transaction_validity::TransactionValidityError,
			> {
				Ok(())
			}

			fn validate(
				&self,
				_who: &Self::AccountId,
				call: &Self::Call,
				_info: &sp_runtime::traits::DispatchInfoOf<Self::Call>,
				_len: usize,
			) -> sp_runtime::transaction_validity::TransactionValidity {
				match *call {
					$(
						$call(pallet_bridge_messages::Call::<$runtime, $instance>::receive_messages_proof {
							ref proof,
							..
						}) => {
							let nonces_end = proof.nonces_end;

							let inbound_lane_data = pallet_bridge_messages::InboundLanes::<$runtime, $instance>::get(&proof.lane);
							if proof.nonces_end <= inbound_lane_data.last_delivered_nonce() {
								log::trace!(
									target: pallet_bridge_messages::LOG_TARGET,
									"Rejecting obsolete messages delivery transaction: lane {:?}, bundled {:?}, best {:?}",
									proof.lane,
									proof.nonces_end,
									inbound_lane_data.last_delivered_nonce(),
								);

								return sp_runtime::transaction_validity::InvalidTransaction::Stale.into();
							}

							Ok(sp_runtime::transaction_validity::ValidTransaction::default())
						},
						$call(pallet_bridge_messages::Call::<$runtime, $instance>::receive_messages_delivery_proof {
							ref proof,
							ref relayers_state,
							..
						}) => {
							let latest_delivered_nonce = relayers_state.last_delivered_nonce;

							let outbound_lane_data = pallet_bridge_messages::OutboundLanes::<$runtime, $instance>::get(&proof.lane);
							if latest_delivered_nonce <= outbound_lane_data.latest_received_nonce {
								log::trace!(
									target: pallet_bridge_messages::LOG_TARGET,
									"Rejecting obsolete messages confirmation transaction: lane {:?}, bundled {:?}, best {:?}",
									proof.lane,
									latest_delivered_nonce,
									outbound_lane_data.latest_received_nonce,
								);

								return sp_runtime::transaction_validity::InvalidTransaction::Stale.into();
							}

							Ok(sp_runtime::transaction_validity::ValidTransaction::default())
						}
					)*
					_ => Ok(sp_runtime::transaction_validity::ValidTransaction::default()),
				}
			}

			fn pre_dispatch(
				self,
				who: &Self::AccountId,
				call: &Self::Call,
				info: &sp_runtime::traits::DispatchInfoOf<Self::Call>,
				len: usize,
			) -> Result<Self::Pre, sp_runtime::transaction_validity::TransactionValidityError> {
				self.validate(who, call, info, len).map(drop)
			}

			fn post_dispatch(
				_maybe_pre: Option<Self::Pre>,
				_info: &sp_runtime::traits::DispatchInfoOf<Self::Call>,
				_post_info: &sp_runtime::traits::PostDispatchInfoOf<Self::Call>,
				_len: usize,
				_result: &sp_runtime::DispatchResult,
			) -> Result<(), sp_runtime::transaction_validity::TransactionValidityError> {
				Ok(())
			}
		}
	};
}

#[cfg(test)]
mod tests {
	use bp_messages::UnrewardedRelayersState;
	use frame_support::weights::{DispatchClass, DispatchInfo, Pays};
	use millau_runtime::{
		bridge_runtime_common::messages::{
			source::FromBridgedChainMessagesDeliveryProof, target::FromBridgedChainMessagesProof,
		},
		BridgeRejectObsoleteMessages, Call, Runtime, WithRialtoMessagesInstance,
	};
	use sp_runtime::traits::SignedExtension;

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
		BridgeRejectObsoleteMessages
			.validate(
				&[0u8; 32].into(),
				&Call::BridgeRialtoMessages(pallet_bridge_messages::Call::<
					Runtime,
					WithRialtoMessagesInstance,
				>::receive_messages_proof {
					relayer_id_at_bridged_chain: [0u8; 32].into(),
					messages_count: (nonces_end - nonces_start + 1) as u32,
					dispatch_weight: 0,
					proof: FromBridgedChainMessagesProof {
						bridged_header_hash: Default::default(),
						storage_proof: vec![],
						lane: [0, 0, 0, 0],
						nonces_start,
						nonces_end,
					},
				}),
				&DispatchInfo { weight: 0, class: DispatchClass::Operational, pays_fee: Pays::Yes },
				0,
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
		BridgeRejectObsoleteMessages
			.validate(
				&[0u8; 32].into(),
				&Call::BridgeRialtoMessages(pallet_bridge_messages::Call::<
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
				&DispatchInfo { weight: 0, class: DispatchClass::Operational, pays_fee: Pays::Yes },
				0,
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
