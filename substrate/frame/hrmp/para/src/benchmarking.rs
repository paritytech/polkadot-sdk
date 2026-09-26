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
use frame_system::RawOrigin;
use hrmp_primitives::{ChannelId, DepositSide};

const AMOUNT: Balance = 1_000_000_000_000;

fn key() -> DepositKey {
	DepositKey { channel: ChannelId { sender: 4_242, recipient: 4_243 }, side: DepositSide::Sender }
}

fn fund<T: Config>() {
	let who = T::SovereignAccountOf::convert(key().para());
	let _ = T::Currency::set_balance(&who, AMOUNT * 10);
}

fn hold<T: Config>() -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::Hold { key: key(), amount: AMOUNT })
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn receive_hold() {
		fund::<T>();

		#[extrinsic_call]
		receive(RawOrigin::Root, hold::<T>());

		assert_eq!(Deposits::<T>::get(key()), Some(AMOUNT));
	}

	#[benchmark]
	fn receive_release() -> Result<(), BenchmarkError> {
		fund::<T>();
		Pallet::<T>::receive(RawOrigin::Root.into(), hold::<T>())?;

		#[extrinsic_call]
		receive(
			RawOrigin::Root,
			MessageToPara::V1(MessageToParaV1::Release { key: key(), amount: None }),
		);

		assert!(Deposits::<T>::get(key()).is_none());
		Ok(())
	}

	#[benchmark]
	fn force_release() -> Result<(), BenchmarkError> {
		fund::<T>();
		Pallet::<T>::receive(RawOrigin::Root.into(), hold::<T>())?;

		#[extrinsic_call]
		_(RawOrigin::Root, key());

		assert!(Deposits::<T>::get(key()).is_none());
		Ok(())
	}

	#[benchmark]
	fn poke_channel_deposits() {
		let caller: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), key().channel.sender, key().channel.recipient);
	}

	#[benchmark]
	fn establish_system_channel() {
		let caller: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), 1_000, 1_001);
	}

	#[benchmark]
	fn force_answer() {
		#[extrinsic_call]
		_(RawOrigin::Root, key(), true);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
