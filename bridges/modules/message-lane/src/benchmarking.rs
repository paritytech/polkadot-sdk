// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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

//! Message lane pallet benchmarking.

use crate::{
	inbound_lane::InboundLaneStorage, inbound_lane_storage, outbound_lane, relayer_fund_account_id, Call, Instance,
};

use bp_message_lane::{
	source_chain::TargetHeaderChain, target_chain::SourceHeaderChain, InboundLaneData, LaneId, MessageData,
	MessageNonce, OutboundLaneData, UnrewardedRelayersState,
};
use frame_benchmarking::{account, benchmarks_instance};
use frame_support::{traits::Get, weights::Weight};
use frame_system::RawOrigin;
use sp_std::{collections::btree_map::BTreeMap, convert::TryInto, ops::RangeInclusive, prelude::*};

/// Message crafted with this size factor should be the largest possible message.
pub const WORST_MESSAGE_SIZE_FACTOR: u32 = 1000;
/// Fee paid by submitter for single message delivery.
const MESSAGE_FEE: u32 = 1_000_000;

const SEED: u32 = 0;

/// Module we're benchmarking here.
pub struct Module<T: Config<I>, I: crate::Instance>(crate::Module<T, I>);

/// Benchmark-specific message parameters.
pub struct MessageParams<ThisAccountId> {
	/// Size factor of the message payload. Message payload grows with every factor
	/// increment. Zero is the smallest possible message and the `WORST_MESSAGE_SIZE_FACTOR` is
	/// largest possible message.
	pub size_factor: u32,
	/// Message sender account.
	pub sender_account: ThisAccountId,
}

/// Benchmark-specific message proof parameters.
pub struct MessageProofParams {
	/// Id of the lane.
	pub lane: LaneId,
	/// Range of messages to include in the proof.
	pub message_nonces: RangeInclusive<MessageNonce>,
	/// If `Some`, the proof needs to include this outbound lane data.
	pub outbound_lane_data: Option<OutboundLaneData>,
}

/// Benchmark-specific message delivery proof parameters.
pub struct MessageDeliveryProofParams<ThisChainAccountId> {
	/// Id of the lane.
	pub lane: LaneId,
	/// The proof needs to include this inbound lane data.
	pub inbound_lane_data: InboundLaneData<ThisChainAccountId>,
}

/// Trait that must be implemented by runtime.
pub trait Config<I: Instance>: crate::Config<I> {
	/// Return id of relayer account at the bridged chain.
	fn bridged_relayer_id() -> Self::InboundRelayer;
	/// Return balance of given account.
	fn account_balance(account: &Self::AccountId) -> Self::OutboundMessageFee;
	/// Create given account and give it enough balance for test purposes.
	fn endow_account(account: &Self::AccountId);
	/// Prepare message to send over lane.
	fn prepare_outbound_message(
		params: MessageParams<Self::AccountId>,
	) -> (Self::OutboundPayload, Self::OutboundMessageFee);
	/// Prepare messages proof to receive by the module.
	fn prepare_message_proof(
		params: MessageProofParams,
	) -> (
		<Self::SourceHeaderChain as SourceHeaderChain<Self::InboundMessageFee>>::MessagesProof,
		Weight,
	);
	/// Prepare messages delivery proof to receive by the module.
	fn prepare_message_delivery_proof(
		params: MessageDeliveryProofParams<Self::AccountId>,
	) -> <Self::TargetHeaderChain as TargetHeaderChain<Self::OutboundPayload, Self::AccountId>>::MessagesDeliveryProof;
}

benchmarks_instance! {
	_ { }

	//
	// Benchmarks that are used directly by the runtime.
	//

	// Benchmark `send_message` extrinsic with the worst possible conditions:
	// * outbound lane already has state, so it needs to be read and decoded;
	// * relayers fund account does not exists (in practice it needs to exist in production environment);
	// * maximal number of messages is being pruned during the call;
	// * message size is maximal for the target chain.
	//
	// Results of this benchmark may be directly used in the `send_message`.
	send_message_worst_case {
		let lane_id = bench_lane_id();
		let sender = account("sender", 0, SEED);
		T::endow_account(&sender);

		// 'send' messages that are to be pruned when our message is sent
		for _nonce in 1..=T::MaxMessagesToPruneAtOnce::get() {
			send_regular_message::<T, I>();
		}
		confirm_message_delivery::<T, I>(T::MaxMessagesToPruneAtOnce::get());

		let (payload, fee) = T::prepare_outbound_message(MessageParams {
			size_factor: WORST_MESSAGE_SIZE_FACTOR,
			sender_account: sender.clone(),
		});
	}: send_message(RawOrigin::Signed(sender), lane_id, payload, fee)
	verify {
		assert_eq!(
			crate::Module::<T, I>::outbound_latest_generated_nonce(bench_lane_id()),
			T::MaxMessagesToPruneAtOnce::get() + 1,
		);
	}

	// Benchmark `receive_messages_proof` extrinsic with single minimal-weight message and following conditions:
	// * proof does not include outbound lane state proof;
	// * inbound lane already has state, so it needs to be read and decoded;
	// * message is successfully dispatched;
	// * message requires all heavy checks done by dispatcher.
	//
	// This is base benchmark for all other message delivery benchmarks.
	receive_single_message_proof {
		let relayer_id_on_source = T::bridged_relayer_id();
		let relayer_id_on_target = account("relayer", 0, SEED);

		let (proof, dispatch_weight) = T::prepare_message_proof(MessageProofParams {
			lane: bench_lane_id(),
			message_nonces: 1..=1,
			outbound_lane_data: None,
		});
	}: receive_messages_proof(RawOrigin::Signed(relayer_id_on_target), relayer_id_on_source, proof, 1, dispatch_weight)
	verify {
		assert_eq!(
			crate::Module::<T, I>::inbound_latest_received_nonce(bench_lane_id()),
			1,
		);
	}

	// Benchmark `receive_messages_proof` extrinsic with two minimal-weight messages and following conditions:
	// * proof does not include outbound lane state proof;
	// * inbound lane already has state, so it needs to be read and decoded;
	// * message is successfully dispatched;
	// * message requires all heavy checks done by dispatcher.
	//
	// The weight of single message delivery could be approximated as
	// `weight(receive_two_messages_proof) - weight(receive_single_message_proof)`.
	// This won't be super-accurate if message has non-zero dispatch weight, but estimation should
	// be close enough to real weight.
	receive_two_messages_proof {
		let relayer_id_on_source = T::bridged_relayer_id();
		let relayer_id_on_target = account("relayer", 0, SEED);

		let (proof, dispatch_weight) = T::prepare_message_proof(MessageProofParams {
			lane: bench_lane_id(),
			message_nonces: 1..=2,
			outbound_lane_data: None,
		});
	}: receive_messages_proof(RawOrigin::Signed(relayer_id_on_target), relayer_id_on_source, proof, 2, dispatch_weight)
	verify {
		assert_eq!(
			crate::Module::<T, I>::inbound_latest_received_nonce(bench_lane_id()),
			2,
		);
	}

	// Benchmark `receive_messages_proof` extrinsic with single minimal-weight message and following conditions:
	// * proof includes outbound lane state proof;
	// * inbound lane already has state, so it needs to be read and decoded;
	// * message is successfully dispatched;
	// * message requires all heavy checks done by dispatcher.
	//
	// The weight of outbound lane state delivery would be
	// `weight(receive_single_message_proof_with_outbound_lane_state) - weight(receive_single_message_proof)`.
	// This won't be super-accurate if message has non-zero dispatch weight, but estimation should
	// be close enough to real weight.
	receive_single_message_proof_with_outbound_lane_state {
		let relayer_id_on_source = T::bridged_relayer_id();
		let relayer_id_on_target = account("relayer", 0, SEED);

		// mark messages 1..=20 as delivered
		receive_messages::<T, I>(20);

		let (proof, dispatch_weight) = T::prepare_message_proof(MessageProofParams {
			lane: bench_lane_id(),
			message_nonces: 21..=21,
			outbound_lane_data: Some(OutboundLaneData {
				oldest_unpruned_nonce: 21,
				latest_received_nonce: 20,
				latest_generated_nonce: 21,
			}),
		});
	}: receive_messages_proof(RawOrigin::Signed(relayer_id_on_target), relayer_id_on_source, proof, 1, dispatch_weight)
	verify {
		assert_eq!(
			crate::Module::<T, I>::inbound_latest_received_nonce(bench_lane_id()),
			21,
		);
		assert_eq!(
			crate::Module::<T, I>::inbound_latest_confirmed_nonce(bench_lane_id()),
			20,
		);
	}

	// Benchmark `receive_messages_delivery_proof` extrinsic with following conditions:
	// * single relayer is rewarded for relaying single message;
	// * relayer account does not exist (in practice it needs to exist in production environment).
	//
	// This is base benchmark for all other confirmations delivery benchmarks.
	receive_delivery_proof_for_single_message {
		let relayers_fund_id = relayer_fund_account_id::<T, I>();
		let relayer_id: T::AccountId = account("relayer", 0, SEED);
		let relayer_balance = T::account_balance(&relayer_id);
		T::endow_account(&relayers_fund_id);

		// send message that we're going to confirm
		send_regular_message::<T, I>();

		let relayers_state = UnrewardedRelayersState {
			unrewarded_relayer_entries: 1,
			messages_in_oldest_entry: 1,
			total_messages: 1,
		};
		let proof = T::prepare_message_delivery_proof(MessageDeliveryProofParams {
			lane: bench_lane_id(),
			inbound_lane_data: InboundLaneData {
				relayers: vec![(1, 1, relayer_id.clone())].into_iter().collect(),
				latest_received_nonce: 1,
				latest_confirmed_nonce: 0,
			}
		});
	}: receive_messages_delivery_proof(RawOrigin::Signed(relayer_id.clone()), proof, relayers_state)
	verify {
		assert_eq!(
			T::account_balance(&relayer_id),
			relayer_balance + MESSAGE_FEE.into(),
		);
	}

	// Benchmark `receive_messages_delivery_proof` extrinsic with following conditions:
	// * single relayer is rewarded for relaying two messages;
	// * relayer account does not exist (in practice it needs to exist in production environment).
	//
	// Additional weight for paying single-message reward to the same relayer could be computed
	// as `weight(receive_delivery_proof_for_two_messages_by_single_relayer)
	//   - weight(receive_delivery_proof_for_single_message)`.
	receive_delivery_proof_for_two_messages_by_single_relayer {
		let relayers_fund_id = relayer_fund_account_id::<T, I>();
		let relayer_id: T::AccountId = account("relayer", 0, SEED);
		let relayer_balance = T::account_balance(&relayer_id);
		T::endow_account(&relayers_fund_id);

		// send message that we're going to confirm
		send_regular_message::<T, I>();
		send_regular_message::<T, I>();

		let relayers_state = UnrewardedRelayersState {
			unrewarded_relayer_entries: 1,
			messages_in_oldest_entry: 2,
			total_messages: 2,
		};
		let proof = T::prepare_message_delivery_proof(MessageDeliveryProofParams {
			lane: bench_lane_id(),
			inbound_lane_data: InboundLaneData {
				relayers: vec![(1, 2, relayer_id.clone())].into_iter().collect(),
				latest_received_nonce: 2,
				latest_confirmed_nonce: 0,
			}
		});
	}: receive_messages_delivery_proof(RawOrigin::Signed(relayer_id.clone()), proof, relayers_state)
	verify {
		assert_eq!(
			T::account_balance(&relayer_id),
			relayer_balance + (MESSAGE_FEE * 2).into(),
		);
	}

	// Benchmark `receive_messages_delivery_proof` extrinsic with following conditions:
	// * two relayers are rewarded for relaying single message each;
	// * relayer account does not exist (in practice it needs to exist in production environment).
	//
	// Additional weight for paying reward to the next relayer could be computed
	// as `weight(receive_delivery_proof_for_two_messages_by_two_relayers)
	//   - weight(receive_delivery_proof_for_two_messages_by_single_relayer)`.
	receive_delivery_proof_for_two_messages_by_two_relayers {
		let relayers_fund_id = relayer_fund_account_id::<T, I>();
		let relayer1_id: T::AccountId = account("relayer1", 1, SEED);
		let relayer1_balance = T::account_balance(&relayer1_id);
		let relayer2_id: T::AccountId = account("relayer2", 2, SEED);
		let relayer2_balance = T::account_balance(&relayer2_id);
		T::endow_account(&relayers_fund_id);

		// send message that we're going to confirm
		send_regular_message::<T, I>();
		send_regular_message::<T, I>();

		let relayers_state = UnrewardedRelayersState {
			unrewarded_relayer_entries: 2,
			messages_in_oldest_entry: 1,
			total_messages: 2,
		};
		let proof = T::prepare_message_delivery_proof(MessageDeliveryProofParams {
			lane: bench_lane_id(),
			inbound_lane_data: InboundLaneData {
				relayers: vec![
					(1, 1, relayer1_id.clone()),
					(2, 2, relayer2_id.clone()),
				].into_iter().collect(),
				latest_received_nonce: 2,
				latest_confirmed_nonce: 0,
			}
		});
	}: receive_messages_delivery_proof(RawOrigin::Signed(relayer1_id.clone()), proof, relayers_state)
	verify {
		assert_eq!(
			T::account_balance(&relayer1_id),
			relayer1_balance + MESSAGE_FEE.into(),
		);
		assert_eq!(
			T::account_balance(&relayer2_id),
			relayer2_balance + MESSAGE_FEE.into(),
		);
	}

	//
	// Benchmarks for manual checks.
	//

	// Benchmark `receive_messages_proof` extrinsic with multiple minimal-weight messages and following conditions:
	// * proof does not include outbound lane state proof;
	// * inbound lane already has state, so it needs to be read and decoded;
	// * message is successfully dispatched;
	// * message requires all heavy checks done by dispatcher.
	//
	// This benchmarks gives us an approximation of single message delivery weight. It is similar to the
	// `weight(receive_two_messages_proof) - weight(receive_single_message_proof)`. So it may be used
	// to verify that the other approximation is correct.
	receive_multiple_messages_proof {
		let i in 1..128;

		let relayer_id_on_source = T::bridged_relayer_id();
		let relayer_id_on_target = account("relayer", 0, SEED);
		let messages_count = i as _;

		let (proof, dispatch_weight) = T::prepare_message_proof(MessageProofParams {
			lane: bench_lane_id(),
			message_nonces: 1..=i as _,
			outbound_lane_data: None,
		});
	}: receive_messages_proof(
		RawOrigin::Signed(relayer_id_on_target),
		relayer_id_on_source,
		proof,
		messages_count,
		dispatch_weight
	)
	verify {
		assert_eq!(
			crate::Module::<T, I>::inbound_latest_received_nonce(bench_lane_id()),
			i as MessageNonce,
		);
	}

	// Benchmark `receive_messages_proof` extrinsic with multiple minimal-weight messages and following conditions:
	// * proof includes outbound lane state proof;
	// * inbound lane already has state, so it needs to be read and decoded;
	// * message is successfully dispatched;
	// * message requires all heavy checks done by dispatcher.
	//
	// This benchmarks gives us an approximation of outbound lane state delivery weight. It is similar to the
	// `weight(receive_single_message_proof_with_outbound_lane_state) - weight(receive_single_message_proof)`.
	// So it may be used to verify that the other approximation is correct.
	receive_multiple_messages_proof_with_outbound_lane_state {
		let i in 1..128;

		let relayer_id_on_source = T::bridged_relayer_id();
		let relayer_id_on_target = account("relayer", 0, SEED);
		let messages_count = i as _;

		// mark messages 1..=20 as delivered
		receive_messages::<T, I>(20);

		let (proof, dispatch_weight) = T::prepare_message_proof(MessageProofParams {
			lane: bench_lane_id(),
			message_nonces: 21..=20 + i as MessageNonce,
			outbound_lane_data: Some(OutboundLaneData {
				oldest_unpruned_nonce: 21,
				latest_received_nonce: 20,
				latest_generated_nonce: 21,
			}),
		});
	}: receive_messages_proof(
		RawOrigin::Signed(relayer_id_on_target),
		relayer_id_on_source,
		proof,
		messages_count,
		dispatch_weight
	)
	verify {
		assert_eq!(
			crate::Module::<T, I>::inbound_latest_received_nonce(bench_lane_id()),
			20 + i as MessageNonce,
		);
		assert_eq!(
			crate::Module::<T, I>::inbound_latest_confirmed_nonce(bench_lane_id()),
			20,
		);
	}

	// Benchmark `receive_messages_delivery_proof` extrinsic where single relayer delivers multiple messages.
	receive_delivery_proof_for_multiple_messages_by_single_relayer {
		// there actually should be used value of `MaxUnrewardedRelayerEntriesAtInboundLane` from the bridged
		// chain, but we're more interested in additional weight/message than in max weight
		let i in 1..T::MaxUnrewardedRelayerEntriesAtInboundLane::get()
			.try_into()
			.expect("Value of MaxUnrewardedRelayerEntriesAtInboundLane is too large");

		let relayers_fund_id = relayer_fund_account_id::<T, I>();
		let relayer_id: T::AccountId = account("relayer", 0, SEED);
		let relayer_balance = T::account_balance(&relayer_id);
		T::endow_account(&relayers_fund_id);

		// send messages that we're going to confirm
		for _ in 1..=i {
			send_regular_message::<T, I>();
		}

		let relayers_state = UnrewardedRelayersState {
			unrewarded_relayer_entries: 1,
			messages_in_oldest_entry: 1,
			total_messages: i as MessageNonce,
		};
		let proof = T::prepare_message_delivery_proof(MessageDeliveryProofParams {
			lane: bench_lane_id(),
			inbound_lane_data: InboundLaneData {
				relayers: vec![(1, i as MessageNonce, relayer_id.clone())].into_iter().collect(),
				latest_received_nonce: i as MessageNonce,
				latest_confirmed_nonce: 0,
			}
		});
	}: receive_messages_delivery_proof(RawOrigin::Signed(relayer_id.clone()), proof, relayers_state)
	verify {
		assert_eq!(
			T::account_balance(&relayer_id),
			relayer_balance + (MESSAGE_FEE * i).into(),
		);
	}

	// Benchmark `receive_messages_delivery_proof` extrinsic where every relayer delivers single messages.
	receive_delivery_proof_for_multiple_messages_by_multiple_relayers {
		// there actually should be used value of `MaxUnconfirmedMessagesAtInboundLane` from the bridged
		// chain, but we're more interested in additional weight/message than in max weight
		let i in 1..T::MaxUnconfirmedMessagesAtInboundLane::get()
			.try_into()
			.expect("Value of MaxUnconfirmedMessagesAtInboundLane is too large ");

		let relayers_fund_id = relayer_fund_account_id::<T, I>();
		let confirmation_relayer_id = account("relayer", 0, SEED);
		let relayers: BTreeMap<T::AccountId, T::OutboundMessageFee> = (1..=i)
			.map(|j| {
				let relayer_id = account("relayer", j + 1, SEED);
				let relayer_balance = T::account_balance(&relayer_id);
				(relayer_id, relayer_balance)
			})
			.collect();
		T::endow_account(&relayers_fund_id);

		// send messages that we're going to confirm
		for _ in 1..=i {
			send_regular_message::<T, I>();
		}

		let relayers_state = UnrewardedRelayersState {
			unrewarded_relayer_entries: i as MessageNonce,
			messages_in_oldest_entry: 1,
			total_messages: i as MessageNonce,
		};
		let proof = T::prepare_message_delivery_proof(MessageDeliveryProofParams {
			lane: bench_lane_id(),
			inbound_lane_data: InboundLaneData {
				relayers: relayers
					.keys()
					.enumerate()
					.map(|(j, relayer_id)| (j as MessageNonce + 1, j as MessageNonce + 1, relayer_id.clone()))
					.collect(),
				latest_received_nonce: i as MessageNonce,
				latest_confirmed_nonce: 0,
			}
		});
	}: receive_messages_delivery_proof(RawOrigin::Signed(confirmation_relayer_id), proof, relayers_state)
	verify {
		for (relayer_id, prev_balance) in relayers {
			assert_eq!(
				T::account_balance(&relayer_id),
				prev_balance + MESSAGE_FEE.into(),
			);
		}
	}
}

fn bench_lane_id() -> LaneId {
	*b"test"
}

fn send_regular_message<T: Config<I>, I: Instance>() {
	let mut outbound_lane = outbound_lane::<T, I>(bench_lane_id());
	outbound_lane.send_message(MessageData {
		payload: vec![],
		fee: MESSAGE_FEE.into(),
	});
}

fn confirm_message_delivery<T: Config<I>, I: Instance>(nonce: MessageNonce) {
	let mut outbound_lane = outbound_lane::<T, I>(bench_lane_id());
	assert!(outbound_lane.confirm_delivery(nonce).is_some());
}

fn receive_messages<T: Config<I>, I: Instance>(nonce: MessageNonce) {
	let mut inbound_lane_storage = inbound_lane_storage::<T, I>(bench_lane_id());
	inbound_lane_storage.set_data(InboundLaneData {
		relayers: vec![(1, nonce, T::bridged_relayer_id())].into_iter().collect(),
		latest_received_nonce: nonce,
		latest_confirmed_nonce: 0,
	});
}
