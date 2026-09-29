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

//! Benchmarking setup for pallet-validator-set-announcer.

#![cfg(feature = "runtime-benchmarks")]

use super::*;

#[allow(unused)]
use crate::Pallet as ValidatorSetAnnouncer;
use alloc::vec::Vec;
use frame_benchmarking::{account, v2::*, BenchmarkError};
use frame_support::traits::Get;
use pallet_validator_collators::ValidatorSet;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn announce(n: Linear<1, { T::MaxValidators::get() }>) -> Result<(), BenchmarkError> {
		let stored = (0..T::MaxValidators::get())
			.map(|i| account("stored", i, 0))
			.collect::<Vec<_>>();
		Pallet::<T>::announce(0, &stored)?;
		let validators = (0..n).map(|i| account("validator", i, 0)).collect::<Vec<T::AccountId>>();

		#[block]
		{
			Pallet::<T>::announce(1, &validators)?;
		}

		assert_eq!(ValidatorSet::<T>::get().map(|set| set.era), Some(1));
		assert_eq!(OutgoingAnnouncements::<T>::iter_keys().count(), T::Destinations::get().len());
		Ok(())
	}

	#[benchmark]
	fn send_announcements(n: Linear<1, { T::MaxValidators::get() }>) -> Result<(), BenchmarkError> {
		let validators = (0..n).map(|i| account("validator", i, 0)).collect::<Vec<T::AccountId>>();
		Pallet::<T>::announce(1, &validators)?;
		T::Destinations::get().iter().for_each(T::Sender::ensure_successful_send);

		#[block]
		{
			Pallet::<T>::send_announcements();
		}

		assert_eq!(OutgoingAnnouncements::<T>::iter_keys().count(), 0);
		Ok(())
	}

	impl_benchmark_test_suite!(
		ValidatorSetAnnouncer,
		crate::mock::new_test_ext(),
		crate::mock::Test
	);
}
