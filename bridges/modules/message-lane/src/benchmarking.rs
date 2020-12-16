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

use crate::{Call, Instance};

use bp_message_lane::{LaneId, MessageData, MessageNonce};
use frame_benchmarking::{account, benchmarks_instance};
use frame_support::traits::Get;
use frame_system::RawOrigin;
use num_traits::Zero;
use sp_std::prelude::*;

/// Message crafted with this size factor should be the largest possible message.
pub const WORST_MESSAGE_SIZE_FACTOR: u32 = 1000;

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

/// Trait that must be implemented by runtime.
pub trait Config<I: Instance>: crate::Config<I> {
	/// Create given account and give it enough balance for test purposes.
	fn endow_account(account: &Self::AccountId);
	/// Prepare message to send over lane.
	fn prepare_message(params: MessageParams<Self::AccountId>) -> (Self::OutboundPayload, Self::OutboundMessageFee);
}

benchmarks_instance! {
	_ { }

	// Benchmark `send_message` extrinsic with the worst possible conditions:
	// * outbound lane already has state, so it needs to be read and decoded;
	// * relayers fund account does not exists (in practice it needs to exist in production environment);
	// * maximal number of messages is being pruned during the call;
	// * message size is maximal for the target chain.
	send_message_worst_case {
		let i in 1..100;

		let lane_id = bench_lane_id();
		let sender = account("sender", i, SEED);
		T::endow_account(&sender);

		// 'send' messages that are to be pruned when our message is sent
		for _nonce in 1..=T::MaxMessagesToPruneAtOnce::get() {
			send_regular_message::<T, I>();
		}
		confirm_message_delivery::<T, I>(T::MaxMessagesToPruneAtOnce::get());

		let (payload, fee) = T::prepare_message(MessageParams {
			size_factor: WORST_MESSAGE_SIZE_FACTOR,
			sender_account: sender.clone(),
		});
	}: send_message(RawOrigin::Signed(sender), lane_id, payload, fee)
}

fn bench_lane_id() -> LaneId {
	*b"test"
}

fn send_regular_message<T: Config<I>, I: Instance>() {
	let mut outbound_lane = crate::outbound_lane::<T, I>(bench_lane_id());
	outbound_lane.send_message(MessageData {
		payload: vec![],
		fee: Zero::zero(),
	});
}

fn confirm_message_delivery<T: Config<I>, I: Instance>(nonce: MessageNonce) {
	let mut outbound_lane = crate::outbound_lane::<T, I>(bench_lane_id());
	assert!(outbound_lane.confirm_delivery(nonce).is_some());
}
