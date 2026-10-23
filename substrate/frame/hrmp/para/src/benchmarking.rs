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

//! Benchmarks for `pallet-hrmp-para`.

use super::*;
use frame_benchmarking::v2::*;
use frame_support::traits::fungible::{Inspect, Mutate};
use frame_system::RawOrigin;

const CHANNEL: ChannelId = ChannelId { sender: 1, recipient: 2 };

/// Endows both paras of [`CHANNEL`] and returns a deposit each can cover.
fn endow<T: Config>() -> Balance
where
	T::Currency: Mutate<T::AccountId>,
{
	let ed = T::Currency::minimum_balance().max(1);
	for side in [DepositSide::Sender, DepositSide::Recipient] {
		T::Currency::set_balance(&Pallet::<T>::payer(&CHANNEL, side), ed.saturating_mul(100));
	}
	ed.saturating_mul(10)
}

#[benchmarks(where T::Currency: Mutate<T::AccountId>)]
mod benchmarks {
	use super::*;

	/// Both sides held from nothing, and the answer sent.
	#[benchmark]
	fn receive_set_deposits() {
		T::BenchmarkHelper::ensure_send_to_relay();
		let deposit = endow::<T>();
		let message = MessageToPara::V1(MessageToParaV1::SetDeposits {
			id: 0,
			channel: CHANNEL,
			sender: Some(deposit),
			recipient: Some(deposit),
		});

		#[extrinsic_call]
		receive(RawOrigin::Root, message);

		assert_eq!(Deposits::<T>::get((CHANNEL, DepositSide::Recipient)), Some(deposit));
	}

	/// Both sides released.
	#[benchmark]
	fn receive_release() {
		let deposit = endow::<T>();
		for side in [DepositSide::Sender, DepositSide::Recipient] {
			Pallet::<T>::set_side(CHANNEL, side, deposit).expect("endowed; qed");
		}
		let message = MessageToPara::V1(MessageToParaV1::Release {
			channel: CHANNEL,
			sender: true,
			recipient: true,
		});

		#[extrinsic_call]
		receive(RawOrigin::Root, message);

		assert_eq!(Deposits::<T>::iter().count(), 0);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::build_ext(), crate::mock::Test);
}
