// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Benchmarks for `pallet-hrmp-relay`.

use super::*;
use frame_benchmarking::v2::*;
use polkadot_runtime_parachains::hrmp::deposits::DepositChange;

/// A request that changes both sides of a channel.
fn both_sides_request() -> DepositChangeRequest {
	let change = DepositChange { current: 0, target: 1_000 };
	DepositChangeRequest {
		channel_id: HrmpChannelId { sender: 1.into(), recipient: 2.into() },
		sender: Some(change),
		recipient: Some(change),
		action: DepositAction::InitAndAcceptOpenChannel {
			max_capacity: u32::MAX,
			max_message_size: u32::MAX,
			max_total_size: u32::MAX,
		},
	}
}

#[benchmarks(where <T as frame_system::Config>::RuntimeOrigin: From<ParachainOrigin>)]
mod benchmarks {
	use super::*;

	/// The answer to a request for both sides. The answer is a refusal, so that
	/// `OnDepositSet` does the least: its own weight is charged on top.
	#[benchmark]
	fn receive() {
		T::BenchmarkHelper::ensure_send_to_para();
		let response = Pallet::<T>::try_set_deposit(both_sides_request());
		assert_eq!(response, Ok(DepositChangeResponse::Pending));
		let id = NextRequestId::<T>::get().wrapping_sub(1);
		let origin = ParachainOrigin::Parachain(T::HolderParaId::get());
		let message = MessageToRelay::V1(MessageToRelayV1::DepositsSet {
			id,
			result: Err(WireDepositError::Other),
		});

		#[extrinsic_call]
		_(origin, message);

		assert!(PendingRequests::<T>::get(id).is_none());
	}

	/// A request for both sides. A release reads and writes less.
	#[benchmark]
	fn try_set_deposit() {
		T::BenchmarkHelper::ensure_send_to_para();
		let request = both_sides_request();
		let response;

		#[block]
		{
			response = Pallet::<T>::try_set_deposit(request);
		}

		assert_eq!(response, Ok(DepositChangeResponse::Pending));
	}

	/// `n` releases waiting in the outbox, all sent. The outbox is not bounded: the cost is linear
	/// in `n`, so the range only needs to be wide enough to fit it.
	#[benchmark]
	fn flush_outbox(n: Linear<0, 100>) {
		T::BenchmarkHelper::ensure_send_to_para();
		for i in 0..n {
			let channel = ChannelId { sender: i, recipient: i + 1 };
			let message = MessageToPara::V1(MessageToParaV1::Release {
				channel,
				sender: true,
				recipient: true,
			});
			Outbox::<T>::insert(u64::from(i), message);
		}
		OutboxBounds::<T>::put(OutboxWindow { head: 0, tail: n.into() });

		#[block]
		{
			Pallet::<T>::flush_outbox(&mut WeightMeter::new());
		}

		assert!(OutboxBounds::<T>::get().is_empty());
		assert_eq!(Outbox::<T>::iter_keys().count(), 0);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::build_ext(), crate::mock::Test);
}
