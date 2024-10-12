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

//! Messages pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use crate::{
	active_outbound_lane, weights_ext::EXPECTED_DEFAULT_MESSAGE_LENGTH, BridgedChainOf, Call,
	InboundLanes, OutboundLanes,
};

use bp_messages::{
	source_chain::FromBridgedChainMessagesDeliveryProof,
	target_chain::FromBridgedChainMessagesProof, ChainWithMessages, DeliveredMessages,
	InboundLaneData, LaneState, MessageNonce, OutboundLaneData, UnrewardedRelayer,
	UnrewardedRelayersState,
};
use bp_runtime::{AccountIdOf, HashOf, UnverifiedStorageProofParams};
use codec::Decode;
use frame_benchmarking::{account, v2::*};
use frame_support::weights::Weight;
use frame_system::RawOrigin;
use sp_runtime::{traits::TrailingZeroInput, BoundedVec};
use sp_std::{ops::RangeInclusive, prelude::*};

const SEED: u32 = 0;

/// Pallet we're benchmarking here.
pub struct Pallet<T: Config<I>, I: 'static = ()>(crate::Pallet<T, I>);

/// Benchmark-specific message proof parameters.
#[derive(Debug)]
pub struct MessageProofParams<LaneId> {
	/// Id of the lane.
	pub lane: LaneId,
	/// Range of messages to include in the proof.
	pub message_nonces: RangeInclusive<MessageNonce>,
	/// If `Some`, the proof needs to include this outbound lane data.
	pub outbound_lane_data: Option<OutboundLaneData>,
	/// If `true`, the caller expects that the proof will contain correct messages that will
	/// be successfully dispatched. This is only called from the "optional"
	/// `receive_single_message_proof_with_dispatch` benchmark. If you don't need it, just
	/// return `true` from the `is_message_successfully_dispatched`.
	pub is_successful_dispatch_expected: bool,
	/// Proof size requirements.
	pub proof_params: UnverifiedStorageProofParams,
}

/// Benchmark-specific message delivery proof parameters.
#[derive(Debug)]
pub struct MessageDeliveryProofParams<ThisChainAccountId, LaneId> {
	/// Id of the lane.
	pub lane: LaneId,
	/// The proof needs to include this inbound lane data.
	pub inbound_lane_data: InboundLaneData<ThisChainAccountId>,
	/// Proof size requirements.
	pub proof_params: UnverifiedStorageProofParams,
}

/// Trait that must be implemented by runtime.
pub trait Config<I: 'static>: crate::Config<I> {
	/// Lane id to use in benchmarks.
	fn bench_lane_id() -> Self::LaneId {
		Self::LaneId::default()
	}

	/// Return id of relayer account at the bridged chain.
	///
	/// By default, zero account is returned.
	fn bridged_relayer_id() -> AccountIdOf<BridgedChainOf<Self, I>> {
		Decode::decode(&mut TrailingZeroInput::zeroes()).unwrap()
	}

	/// Create given account and give it enough balance for test purposes. Used to create
	/// relayer account at the target chain. Is strictly necessary when your rewards scheme
	/// assumes that the relayer account must exist.
	///
	/// Does nothing by default.
	fn endow_account(_account: &Self::AccountId) {}

	/// Prepare messages proof to receive by the module.
	fn prepare_message_proof(
		params: MessageProofParams<Self::LaneId>,
	) -> (FromBridgedChainMessagesProof<HashOf<BridgedChainOf<Self, I>>, Self::LaneId>, Weight);
	/// Prepare messages delivery proof to receive by the module.
	fn prepare_message_delivery_proof(
		params: MessageDeliveryProofParams<Self::AccountId, Self::LaneId>,
	) -> FromBridgedChainMessagesDeliveryProof<HashOf<BridgedChainOf<Self, I>>, Self::LaneId>;

	/// Returns true if message has been successfully dispatched or not.
	fn is_message_successfully_dispatched(_nonce: MessageNonce) -> bool {
		true
	}

	/// Returns true if given relayer has been rewarded for some of its actions.
	fn is_relayer_rewarded(relayer: &Self::AccountId) -> bool;
}

fn send_regular_message<T: Config<I>, I: 'static>() {
	OutboundLanes::<T, I>::insert(
		T::bench_lane_id(),
		OutboundLaneData {
			state: LaneState::Opened,
			latest_generated_nonce: 1,
			..Default::default()
		},
	);

	let mut outbound_lane = active_outbound_lane::<T, I>(T::bench_lane_id()).unwrap();
	outbound_lane.send_message(BoundedVec::try_from(vec![]).expect("We craft valid messages"));
}

fn receive_messages<T: Config<I>, I: 'static>(nonce: MessageNonce) {
	InboundLanes::<T, I>::insert(
		T::bench_lane_id(),
		InboundLaneData {
			state: LaneState::Opened,
			relayers: vec![UnrewardedRelayer {
				relayer: T::bridged_relayer_id(),
				messages: DeliveredMessages::new(nonce),
			}]
			.into(),
			last_confirmed_nonce: 0,
		},
	);
}

struct ReceiveMessagesProofSetup<T: Config<I>, I: 'static> {
	relayer_id_on_src: AccountIdOf<BridgedChainOf<T, I>>,
	relayer_id_on_tgt: T::AccountId,
	msgs_count: u32,
	_phantom_data: sp_std::marker::PhantomData<I>,
}

impl<T: Config<I>, I: 'static> ReceiveMessagesProofSetup<T, I> {
	const LATEST_RECEIVED_NONCE: MessageNonce = 20;

	fn new(msgs_count: u32) -> Self {
		let setup = Self {
			relayer_id_on_src: T::bridged_relayer_id(),
			relayer_id_on_tgt: account("relayer", 0, SEED),
			msgs_count,
			_phantom_data: Default::default(),
		};
		T::endow_account(&setup.relayer_id_on_tgt);
		// mark messages 1..=latest_recvd_nonce as delivered
		receive_messages::<T, I>(Self::LATEST_RECEIVED_NONCE);

		setup
	}

	fn relayer_id_on_src(&self) -> AccountIdOf<BridgedChainOf<T, I>> {
		self.relayer_id_on_src.clone()
	}

	fn relayer_id_on_tgt(&self) -> T::AccountId {
		self.relayer_id_on_tgt.clone()
	}

	fn last_nonce(&self) -> MessageNonce {
		Self::LATEST_RECEIVED_NONCE + self.msgs_count as u64
	}

	fn nonces(&self) -> RangeInclusive<MessageNonce> {
		(Self::LATEST_RECEIVED_NONCE + 1)..=self.last_nonce()
	}

	fn check_last_nonce(&self) {
		assert_eq!(
			crate::InboundLanes::<T, I>::get(&T::bench_lane_id()).map(|d| d.last_delivered_nonce()),
			Some(self.last_nonce()),
		);
	}
}

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	//
	// Benchmarks that are used directly by the runtime calls weight formulae.
	//

	fn max_msgs<T: Config<I>, I: 'static>() -> u32 {
		T::BridgedChain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX as u32 -
			ReceiveMessagesProofSetup::<T, I>::LATEST_RECEIVED_NONCE as u32
	}

	// Benchmark `receive_messages_proof` extrinsic with single minimal-weight message and following
	// conditions:
	// * proof does not include outbound lane state proof;
	// * inbound lane already has state, so it needs to be read and decoded;
	// * message is dispatched (reminder: dispatch weight should be minimal);
	// * message requires all heavy checks done by dispatcher.
	#[benchmark]
	fn receive_single_message_proof() {
		// setup code
		let setup = ReceiveMessagesProofSetup::<T, I>::new(1);
		let (proof, dispatch_weight) = T::prepare_message_proof(MessageProofParams {
			lane: T::bench_lane_id(),
			message_nonces: setup.nonces(),
			outbound_lane_data: None,
			is_successful_dispatch_expected: false,
			proof_params: UnverifiedStorageProofParams::from_db_size(
				EXPECTED_DEFAULT_MESSAGE_LENGTH,
			),
		});

		#[extrinsic_call]
		receive_messages_proof(
			RawOrigin::Signed(setup.relayer_id_on_tgt()),
			setup.relayer_id_on_src(),
			Box::new(proof),
			setup.msgs_count,
			dispatch_weight,
		);

		// verification code
		setup.check_last_nonce();
	}

	// Benchmark `receive_messages_proof` extrinsic with `n` minimal-weight messages and following
	// conditions:
	// * proof does not include outbound lane state proof;
	// * inbound lane already has state, so it needs to be read and decoded;
	// * message is dispatched (reminder: dispatch weight should be minimal);
	// * message requires all heavy checks done by dispatcher.
	#[benchmark]
	fn receive_n_messages_proof(n: Linear<1, { max_msgs::<T, I>() }>) {
		// setup code
		let setup = ReceiveMessagesProofSetup::<T, I>::new(n);
		let (proof, dispatch_weight) = T::prepare_message_proof(MessageProofParams {
			lane: T::bench_lane_id(),
			message_nonces: setup.nonces(),
			outbound_lane_data: None,
			is_successful_dispatch_expected: false,
			proof_params: UnverifiedStorageProofParams::from_db_size(
				EXPECTED_DEFAULT_MESSAGE_LENGTH,
			),
		});

		#[extrinsic_call]
		receive_messages_proof(
			RawOrigin::Signed(setup.relayer_id_on_tgt()),
			setup.relayer_id_on_src(),
			Box::new(proof),
			setup.msgs_count,
			dispatch_weight,
		);

		// verification code
		setup.check_last_nonce();
	}

	// Benchmark `receive_messages_proof` extrinsic with single minimal-weight message and following
	// conditions:
	// * proof includes outbound lane state proof;
	// * inbound lane already has state, so it needs to be read and decoded;
	// * message is successfully dispatched (reminder: dispatch weight should be minimal);
	// * message requires all heavy checks done by dispatcher.
	//
	// The weight of outbound lane state delivery would be
	// `weight(receive_single_message_proof_with_outbound_lane_state) -
	// weight(receive_single_message_proof)`. This won't be super-accurate if message has non-zero
	// dispatch weight, but estimation should be close enough to real weight.
	#[benchmark]
	fn receive_single_message_proof_with_outbound_lane_state() {
		// setup code
		let setup = ReceiveMessagesProofSetup::<T, I>::new(1);
		let (proof, dispatch_weight) = T::prepare_message_proof(MessageProofParams {
			lane: T::bench_lane_id(),
			message_nonces: setup.nonces(),
			outbound_lane_data: Some(OutboundLaneData {
				state: LaneState::Opened,
				oldest_unpruned_nonce: setup.last_nonce(),
				latest_received_nonce: ReceiveMessagesProofSetup::<T, I>::LATEST_RECEIVED_NONCE,
				latest_generated_nonce: setup.last_nonce(),
			}),
			is_successful_dispatch_expected: false,
			proof_params: UnverifiedStorageProofParams::from_db_size(
				EXPECTED_DEFAULT_MESSAGE_LENGTH,
			),
		});

		#[extrinsic_call]
		receive_messages_proof(
			RawOrigin::Signed(setup.relayer_id_on_tgt()),
			setup.relayer_id_on_src(),
			Box::new(proof),
			setup.msgs_count,
			dispatch_weight,
		);

		// verification code
		setup.check_last_nonce();
	}

	// Benchmark `receive_messages_proof` extrinsic with single minimal-weight message and following
	// conditions:
	// * the proof has large leaf with total size ranging between 1KB and 16KB;
	// * proof does not include outbound lane state proof;
	// * inbound lane already has state, so it needs to be read and decoded;
	// * message is dispatched (reminder: dispatch weight should be minimal);
	// * message requires all heavy checks done by dispatcher.
	#[benchmark]
	fn receive_single_n_bytes_message_proof(
		/// Proof size in KB
		n: Linear<1, { 16 * 1024 }>,
	) {
		// setup code
		let setup = ReceiveMessagesProofSetup::<T, I>::new(1);
		let (proof, dispatch_weight) = T::prepare_message_proof(MessageProofParams {
			lane: T::bench_lane_id(),
			message_nonces: setup.nonces(),
			outbound_lane_data: None,
			is_successful_dispatch_expected: false,
			proof_params: UnverifiedStorageProofParams::from_db_size(n),
		});

		#[extrinsic_call]
		receive_messages_proof(
			RawOrigin::Signed(setup.relayer_id_on_tgt()),
			setup.relayer_id_on_src(),
			Box::new(proof),
			setup.msgs_count,
			dispatch_weight,
		);

		// verification code
		setup.check_last_nonce();
	}

	// Benchmark `receive_messages_delivery_proof` extrinsic with following conditions:
	// * single relayer is rewarded for relaying single message;
	// * relayer account does not exist (in practice it needs to exist in production environment).
	//
	// This is base benchmark for all other confirmations delivery benchmarks.
	#[benchmark]
	fn receive_delivery_proof_for_single_message() {
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
				state: LaneState::Opened,
				relayers: vec![UnrewardedRelayer {
					relayer: relayer_id.clone(),
					messages: DeliveredMessages::new(1),
				}]
				.into_iter()
				.collect(),
				last_confirmed_nonce: 0,
			},
			proof_params: UnverifiedStorageProofParams::default(),
		});

		#[extrinsic_call]
		receive_messages_delivery_proof(
			RawOrigin::Signed(relayer_id.clone()),
			proof,
			relayers_state,
		);

		assert_eq!(
			OutboundLanes::<T, I>::get(T::bench_lane_id()).map(|s| s.latest_received_nonce),
			Some(1)
		);
		assert!(T::is_relayer_rewarded(&relayer_id));
	}

	// Benchmark `receive_messages_delivery_proof` extrinsic with following conditions:
	// * single relayer is rewarded for relaying two messages;
	// * relayer account does not exist (in practice it needs to exist in production environment).
	//
	// Additional weight for paying single-message reward to the same relayer could be computed
	// as `weight(receive_delivery_proof_for_two_messages_by_single_relayer)
	//   - weight(receive_delivery_proof_for_single_message)`.
	#[benchmark]
	fn receive_delivery_proof_for_two_messages_by_single_relayer() {
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
				state: LaneState::Opened,
				relayers: vec![UnrewardedRelayer {
					relayer: relayer_id.clone(),
					messages: delivered_messages,
				}]
				.into_iter()
				.collect(),
				last_confirmed_nonce: 0,
			},
			proof_params: UnverifiedStorageProofParams::default(),
		});

		#[extrinsic_call]
		receive_messages_delivery_proof(
			RawOrigin::Signed(relayer_id.clone()),
			proof,
			relayers_state,
		);

		assert_eq!(
			OutboundLanes::<T, I>::get(T::bench_lane_id()).map(|s| s.latest_received_nonce),
			Some(2)
		);
		assert!(T::is_relayer_rewarded(&relayer_id));
	}

	// Benchmark `receive_messages_delivery_proof` extrinsic with following conditions:
	// * two relayers are rewarded for relaying single message each;
	// * relayer account does not exist (in practice it needs to exist in production environment).
	//
	// Additional weight for paying reward to the next relayer could be computed
	// as `weight(receive_delivery_proof_for_two_messages_by_two_relayers)
	//   - weight(receive_delivery_proof_for_two_messages_by_single_relayer)`.
	#[benchmark]
	fn receive_delivery_proof_for_two_messages_by_two_relayers() {
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
				state: LaneState::Opened,
				relayers: vec![
					UnrewardedRelayer {
						relayer: relayer1_id.clone(),
						messages: DeliveredMessages::new(1),
					},
					UnrewardedRelayer {
						relayer: relayer2_id.clone(),
						messages: DeliveredMessages::new(2),
					},
				]
				.into_iter()
				.collect(),
				last_confirmed_nonce: 0,
			},
			proof_params: UnverifiedStorageProofParams::default(),
		});

		#[extrinsic_call]
		receive_messages_delivery_proof(
			RawOrigin::Signed(relayer1_id.clone()),
			proof,
			relayers_state,
		);

		assert_eq!(
			OutboundLanes::<T, I>::get(T::bench_lane_id()).map(|s| s.latest_received_nonce),
			Some(2)
		);
		assert!(T::is_relayer_rewarded(&relayer1_id));
		assert!(T::is_relayer_rewarded(&relayer2_id));
	}

	//
	// Benchmarks that the runtime developers may use for proper pallet configuration.
	//

	// This benchmark is optional and may be used when runtime developer need a way to compute
	// message dispatch weight. In this case, he needs to provide messages that can go the whole
	// dispatch
	//
	// Benchmark `receive_messages_proof` extrinsic with single message and following conditions:
	//
	// * proof does not include outbound lane state proof;
	// * inbound lane already has state, so it needs to be read and decoded;
	// * message is **SUCCESSFULLY** dispatched;
	// * message requires all heavy checks done by dispatcher.
	#[benchmark]
	fn receive_single_n_bytes_message_proof_with_dispatch(
		/// Proof size in KB
		n: Linear<1, { 16 * 1024 }>,
	) {
		// setup code
		let setup = ReceiveMessagesProofSetup::<T, I>::new(1);
		let (proof, dispatch_weight) = T::prepare_message_proof(MessageProofParams {
			lane: T::bench_lane_id(),
			message_nonces: setup.nonces(),
			outbound_lane_data: None,
			is_successful_dispatch_expected: true,
			proof_params: UnverifiedStorageProofParams::from_db_size(n),
		});

		#[extrinsic_call]
		receive_messages_proof(
			RawOrigin::Signed(setup.relayer_id_on_tgt()),
			setup.relayer_id_on_src(),
			Box::new(proof),
			setup.msgs_count,
			dispatch_weight,
		);

		// verification code
		setup.check_last_nonce();
		assert!(T::is_message_successfully_dispatched(setup.last_nonce()));
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::tests::mock::new_test_ext(),
		crate::tests::mock::TestRuntime
	);
}
