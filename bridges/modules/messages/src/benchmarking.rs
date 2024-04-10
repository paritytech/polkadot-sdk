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

//! Messages pallet benchmarking.

use crate::{
	inbound_lane::InboundLaneStorage, inbound_lane_storage, outbound_lane,
	weights_ext::EXPECTED_DEFAULT_MESSAGE_LENGTH, Call, OutboundLanes,
};

use bp_messages::{
	source_chain::TargetHeaderChain, target_chain::SourceHeaderChain, DeliveredMessages,
	InboundLaneData, LaneId, MessageNonce, OutboundLaneData, UnrewardedRelayer,
	UnrewardedRelayersState,
};
use bp_runtime::StorageProofSize;
use frame_benchmarking::{account, benchmarks_instance_pallet};
use frame_support::weights::Weight;
use frame_system::RawOrigin;
use sp_std::{ops::RangeInclusive, prelude::*};

const SEED: u32 = 0;

/// Pallet we're benchmarking here.
pub struct Pallet<T: Config<I>, I: 'static>(crate::Pallet<T, I>);

/// Benchmark-specific message proof parameters.
#[derive(Debug)]
pub struct MessageProofParams {
	/// Id of the lane.
	pub lane: LaneId,
	/// Range of messages to include in the proof.
	pub message_nonces: RangeInclusive<MessageNonce>,
	/// If `Some`, the proof needs to include this outbound lane data.
	pub outbound_lane_data: Option<OutboundLaneData>,
	/// Proof size requirements.
	pub size: StorageProofSize,
}

/// Benchmark-specific message delivery proof parameters.
#[derive(Debug)]
pub struct MessageDeliveryProofParams<ThisChainAccountId> {
	/// Id of the lane.
	pub lane: LaneId,
	/// The proof needs to include this inbound lane data.
	pub inbound_lane_data: InboundLaneData<ThisChainAccountId>,
	/// Proof size requirements.
	pub size: StorageProofSize,
}

/// Trait that must be implemented by runtime.
pub trait Config<I: 'static>: crate::Config<I> {
	/// Lane id to use in benchmarks.
	fn bench_lane_id() -> LaneId {
		Default::default()
	}
	/// Return id of relayer account at the bridged chain.
	fn bridged_relayer_id() -> Self::InboundRelayer;
	/// Create given account and give it enough balance for test purposes.
	fn endow_account(account: &Self::AccountId);
	/// Prepare messages proof to receive by the module.
	fn prepare_message_proof(
		params: MessageProofParams,
	) -> (<Self::SourceHeaderChain as SourceHeaderChain>::MessagesProof, Weight);
	/// Prepare messages delivery proof to receive by the module.
	fn prepare_message_delivery_proof(
		params: MessageDeliveryProofParams<Self::AccountId>,
	) -> <Self::TargetHeaderChain as TargetHeaderChain<Self::OutboundPayload, Self::AccountId>>::MessagesDeliveryProof;
	/// Returns true if message has been dispatched (either successfully or not).
	fn is_message_dispatched(nonce: MessageNonce) -> bool;
}

benchmarks_instance_pallet! {
	//
	// Benchmarks that are used directly by the runtime.
	//

	// Benchmark `receive_messages_proof` extrinsic with single minimal-weight message and following conditions:
	// * proof does not include outbound lane state proof;
	// * inbound lane already has state, so it needs to be read and decoded;
	// * message is successfully dispatched;
	// * message requires all heavy checks done by dispatcher;
	// * message dispatch fee is paid at target (this) chain.
	//
	// This is base benchmark for all other message delivery benchmarks.
	receive_single_message_proof {
		let relayer_id_on_source = T::bridged_relayer_id();
		let relayer_id_on_target = account("relayer", 0, SEED);
		T::endow_account(&relayer_id_on_target);

		// mark messages 1..=20 as delivered
		receive_messages::<T, I>(20);

		let (proof, dispatch_weight) = T::prepare_message_proof(MessageProofParams {
			lane: T::bench_lane_id(),
			message_nonces: 21..=21,
			outbound_lane_data: None,
			size: StorageProofSize::Minimal(EXPECTED_DEFAULT_MESSAGE_LENGTH),
		});
	}: receive_messages_proof(RawOrigin::Signed(relayer_id_on_target), relayer_id_on_source, proof, 1, dispatch_weight)
	verify {
		assert_eq!(
			crate::InboundLanes::<T, I>::get(&T::bench_lane_id()).last_delivered_nonce(),
			21,
		);
		assert!(T::is_message_dispatched(21));
	}

	// Benchmark `receive_messages_proof` extrinsic with two minimal-weight messages and following conditions:
	// * proof does not include outbound lane state proof;
	// * inbound lane already has state, so it needs to be read and decoded;
	// * message is successfully dispatched;
	// * message requires all heavy checks done by dispatcher;
	// * message dispatch fee is paid at target (this) chain.
	//
	// The weight of single message delivery could be approximated as
	// `weight(receive_two_messages_proof) - weight(receive_single_message_proof)`.
	// This won't be super-accurate if message has non-zero dispatch weight, but estimation should
	// be close enough to real weight.
	receive_two_messages_proof {
		let relayer_id_on_source = T::bridged_relayer_id();
		let relayer_id_on_target = account("relayer", 0, SEED);
		T::endow_account(&relayer_id_on_target);

		// mark messages 1..=20 as delivered
		receive_messages::<T, I>(20);

		let (proof, dispatch_weight) = T::prepare_message_proof(MessageProofParams {
			lane: T::bench_lane_id(),
			message_nonces: 21..=22,
			outbound_lane_data: None,
			size: StorageProofSize::Minimal(EXPECTED_DEFAULT_MESSAGE_LENGTH),
		});
	}: receive_messages_proof(RawOrigin::Signed(relayer_id_on_target), relayer_id_on_source, proof, 2, dispatch_weight)
	verify {
		assert_eq!(
			crate::InboundLanes::<T, I>::get(&T::bench_lane_id()).last_delivered_nonce(),
			22,
		);
		assert!(T::is_message_dispatched(22));
	}

	// Benchmark `receive_messages_proof` extrinsic with single minimal-weight message and following conditions:
	// * proof includes outbound lane state proof;
	// * inbound lane already has state, so it needs to be read and decoded;
	// * message is successfully dispatched;
	// * message requires all heavy checks done by dispatcher;
	// * message dispatch fee is paid at target (this) chain.
	//
	// The weight of outbound lane state delivery would be
	// `weight(receive_single_message_proof_with_outbound_lane_state) - weight(receive_single_message_proof)`.
	// This won't be super-accurate if message has non-zero dispatch weight, but estimation should
	// be close enough to real weight.
	receive_single_message_proof_with_outbound_lane_state {
		let relayer_id_on_source = T::bridged_relayer_id();
		let relayer_id_on_target = account("relayer", 0, SEED);
		T::endow_account(&relayer_id_on_target);

		// mark messages 1..=20 as delivered
		receive_messages::<T, I>(20);

		let (proof, dispatch_weight) = T::prepare_message_proof(MessageProofParams {
			lane: T::bench_lane_id(),
			message_nonces: 21..=21,
			outbound_lane_data: Some(OutboundLaneData {
				oldest_unpruned_nonce: 21,
				latest_received_nonce: 20,
				latest_generated_nonce: 21,
			}),
			size: StorageProofSize::Minimal(EXPECTED_DEFAULT_MESSAGE_LENGTH),
		});
	}: receive_messages_proof(RawOrigin::Signed(relayer_id_on_target), relayer_id_on_source, proof, 1, dispatch_weight)
	verify {
		let lane_state = crate::InboundLanes::<T, I>::get(&T::bench_lane_id());
		assert_eq!(lane_state.last_delivered_nonce(), 21);
		assert_eq!(lane_state.last_confirmed_nonce, 20);
		assert!(T::is_message_dispatched(21));
	}

	// Benchmark `receive_messages_proof` extrinsic with single minimal-weight message and following conditions:
	// * the proof has many redundand trie nodes with total size of approximately 1KB;
	// * proof does not include outbound lane state proof;
	// * inbound lane already has state, so it needs to be read and decoded;
	// * message is successfully dispatched;
	// * message requires all heavy checks done by dispatcher.
	//
	// With single KB of messages proof, the weight of the call is increased (roughly) by
	// `(receive_single_message_proof_16KB - receive_single_message_proof_1_kb) / 15`.
	receive_single_message_proof_1_kb {
		let relayer_id_on_source = T::bridged_relayer_id();
		let relayer_id_on_target = account("relayer", 0, SEED);
		T::endow_account(&relayer_id_on_target);

		// mark messages 1..=20 as delivered
		receive_messages::<T, I>(20);

		let (proof, dispatch_weight) = T::prepare_message_proof(MessageProofParams {
			lane: T::bench_lane_id(),
			message_nonces: 21..=21,
			outbound_lane_data: None,
			size: StorageProofSize::HasExtraNodes(1024),
		});
	}: receive_messages_proof(RawOrigin::Signed(relayer_id_on_target), relayer_id_on_source, proof, 1, dispatch_weight)
	verify {
		assert_eq!(
			crate::InboundLanes::<T, I>::get(&T::bench_lane_id()).last_delivered_nonce(),
			21,
		);
		assert!(T::is_message_dispatched(21));
	}

	// Benchmark `receive_messages_proof` extrinsic with single minimal-weight message and following conditions:
	// * the proof has many redundand trie nodes with total size of approximately 16KB;
	// * proof does not include outbound lane state proof;
	// * inbound lane already has state, so it needs to be read and decoded;
	// * message is successfully dispatched;
	// * message requires all heavy checks done by dispatcher.
	//
	// Size of proof grows because it contains extra trie nodes in it.
	//
	// With single KB of messages proof, the weight of the call is increased (roughly) by
	// `(receive_single_message_proof_16KB - receive_single_message_proof) / 15`.
	receive_single_message_proof_16_kb {
		let relayer_id_on_source = T::bridged_relayer_id();
		let relayer_id_on_target = account("relayer", 0, SEED);
		T::endow_account(&relayer_id_on_target);

		// mark messages 1..=20 as delivered
		receive_messages::<T, I>(20);

		let (proof, dispatch_weight) = T::prepare_message_proof(MessageProofParams {
			lane: T::bench_lane_id(),
			message_nonces: 21..=21,
			outbound_lane_data: None,
			size: StorageProofSize::HasExtraNodes(16 * 1024),
		});
	}: receive_messages_proof(RawOrigin::Signed(relayer_id_on_target), relayer_id_on_source, proof, 1, dispatch_weight)
	verify {
		assert_eq!(
			crate::InboundLanes::<T, I>::get(&T::bench_lane_id()).last_delivered_nonce(),
			21,
		);
		assert!(T::is_message_dispatched(21));
	}

	// Benchmark `receive_messages_delivery_proof` extrinsic with following conditions:
	// * single relayer is rewarded for relaying single message;
	// * relayer account does not exist (in practice it needs to exist in production environment).
	//
	// This is base benchmark for all other confirmations delivery benchmarks.
	receive_delivery_proof_for_single_message {
		let relayer_id: T::AccountId = account("relayer", 0, SEED);

		// send message that we're going to confirm
		send_regular_message::<T, I>();

		let relayers_state = UnrewardedRelayersState {
			unrewarded_relayer_entries: 1,
			messages_in_oldest_entry: 1,
			total_messages: 1,
			last_delivered_nonce: 1,
		};
		let proof = T::prepare_message_delivery_proof(MessageDeliveryProofParams {
			lane: T::bench_lane_id(),
			inbound_lane_data: InboundLaneData {
				relayers: vec![UnrewardedRelayer {
					relayer: relayer_id.clone(),
					messages: DeliveredMessages::new(1),
				}].into_iter().collect(),
				last_confirmed_nonce: 0,
			},
			size: StorageProofSize::Minimal(0),
		});
	}: receive_messages_delivery_proof(RawOrigin::Signed(relayer_id.clone()), proof, relayers_state)
	verify {
		assert_eq!(OutboundLanes::<T, I>::get(T::bench_lane_id()).latest_received_nonce, 1);
	}

	// Benchmark `receive_messages_delivery_proof` extrinsic with following conditions:
	// * single relayer is rewarded for relaying two messages;
	// * relayer account does not exist (in practice it needs to exist in production environment).
	//
	// Additional weight for paying single-message reward to the same relayer could be computed
	// as `weight(receive_delivery_proof_for_two_messages_by_single_relayer)
	//   - weight(receive_delivery_proof_for_single_message)`.
	receive_delivery_proof_for_two_messages_by_single_relayer {
		let relayer_id: T::AccountId = account("relayer", 0, SEED);

		// send message that we're going to confirm
		send_regular_message::<T, I>();
		send_regular_message::<T, I>();

		let relayers_state = UnrewardedRelayersState {
			unrewarded_relayer_entries: 1,
			messages_in_oldest_entry: 2,
			total_messages: 2,
			last_delivered_nonce: 2,
		};
		let mut delivered_messages = DeliveredMessages::new(1);
		delivered_messages.note_dispatched_message();
		let proof = T::prepare_message_delivery_proof(MessageDeliveryProofParams {
			lane: T::bench_lane_id(),
			inbound_lane_data: InboundLaneData {
				relayers: vec![UnrewardedRelayer {
					relayer: relayer_id.clone(),
					messages: delivered_messages,
				}].into_iter().collect(),
				last_confirmed_nonce: 0,
			},
			size: StorageProofSize::Minimal(0),
		});
	}: receive_messages_delivery_proof(RawOrigin::Signed(relayer_id.clone()), proof, relayers_state)
	verify {
		assert_eq!(OutboundLanes::<T, I>::get(T::bench_lane_id()).latest_received_nonce, 2);
	}

	// Benchmark `receive_messages_delivery_proof` extrinsic with following conditions:
	// * two relayers are rewarded for relaying single message each;
	// * relayer account does not exist (in practice it needs to exist in production environment).
	//
	// Additional weight for paying reward to the next relayer could be computed
	// as `weight(receive_delivery_proof_for_two_messages_by_two_relayers)
	//   - weight(receive_delivery_proof_for_two_messages_by_single_relayer)`.
	receive_delivery_proof_for_two_messages_by_two_relayers {
		let relayer1_id: T::AccountId = account("relayer1", 1, SEED);
		let relayer2_id: T::AccountId = account("relayer2", 2, SEED);

		// send message that we're going to confirm
		send_regular_message::<T, I>();
		send_regular_message::<T, I>();

		let relayers_state = UnrewardedRelayersState {
			unrewarded_relayer_entries: 2,
			messages_in_oldest_entry: 1,
			total_messages: 2,
			last_delivered_nonce: 2,
		};
		let proof = T::prepare_message_delivery_proof(MessageDeliveryProofParams {
			lane: T::bench_lane_id(),
			inbound_lane_data: InboundLaneData {
				relayers: vec![
					UnrewardedRelayer {
						relayer: relayer1_id.clone(),
						messages: DeliveredMessages::new(1),
					},
					UnrewardedRelayer {
						relayer: relayer2_id.clone(),
						messages: DeliveredMessages::new(2),
					},
				].into_iter().collect(),
				last_confirmed_nonce: 0,
			},
			size: StorageProofSize::Minimal(0),
		});
	}: receive_messages_delivery_proof(RawOrigin::Signed(relayer1_id.clone()), proof, relayers_state)
	verify {
		assert_eq!(OutboundLanes::<T, I>::get(T::bench_lane_id()).latest_received_nonce, 2);
	}
}

fn send_regular_message<T: Config<I>, I: 'static>() {
	let mut outbound_lane = outbound_lane::<T, I>(T::bench_lane_id());
	outbound_lane.send_message(vec![]);
}

fn receive_messages<T: Config<I>, I: 'static>(nonce: MessageNonce) {
	let mut inbound_lane_storage = inbound_lane_storage::<T, I>(T::bench_lane_id());
	inbound_lane_storage.set_data(InboundLaneData {
		relayers: vec![UnrewardedRelayer {
			relayer: T::bridged_relayer_id(),
			messages: DeliveredMessages::new(nonce),
		}]
		.into_iter()
		.collect(),
		last_confirmed_nonce: 0,
	});
}
