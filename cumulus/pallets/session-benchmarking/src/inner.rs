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

//! Benchmarking setup for pallet-session.
#![cfg(feature = "runtime-benchmarks")]

use alloc::{vec, vec::Vec};

use codec::Decode;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use pallet_session::*;
pub struct Pallet<T: Config>(pallet_session::Pallet<T>);
pub trait Config: pallet_session::Config {}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_keys() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		frame_system::Pallet::<T>::inc_providers(&caller);
		let keys = T::Keys::decode(&mut sp_runtime::traits::TrailingZeroInput::zeroes()).unwrap();
		let proof: Vec<u8> = vec![0, 1, 2, 3];
		<pallet_session::Pallet<T>>::ensure_can_pay_key_deposit(&caller).unwrap();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), keys, proof);

		Ok(())
	}

	#[benchmark]
	fn purge_keys() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		frame_system::Pallet::<T>::inc_providers(&caller);
		let keys = T::Keys::decode(&mut sp_runtime::traits::TrailingZeroInput::zeroes()).unwrap();
		let proof: Vec<u8> = vec![0, 1, 2, 3];
		<pallet_session::Pallet<T>>::ensure_can_pay_key_deposit(&caller).unwrap();
		let _t = pallet_session::Pallet::<T>::set_keys(
			RawOrigin::Signed(caller.clone()).into(),
			keys,
			proof,
		);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller));

		Ok(())
	}
}
