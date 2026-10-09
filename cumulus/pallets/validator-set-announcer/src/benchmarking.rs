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
		pallet_validator_collators::Pallet::<T>::receive_validator_set(0, stored)?;
		let validators = (0..n).map(|i| account("validator", i, 0)).collect::<Vec<T::AccountId>>();

		#[block]
		{
			pallet_validator_collators::Pallet::<T>::receive_validator_set(
				1,
				validators.iter().cloned(),
			)?;
		}

		assert_eq!(ValidatorSet::<T>::get().map(|set| set.era), Some(1));
		Ok(())
	}

	// Measures one send to the first entry of `Destinations`. `announce_weight` charges this
	// weight once for every destination.
	#[benchmark]
	fn send_announcement(n: Linear<1, { T::MaxValidators::get() }>) -> Result<(), BenchmarkError> {
		let validators = (0..n).map(|i| account("validator", i, 0)).collect::<Vec<T::AccountId>>();
		let destination = T::Destinations::get()
			.into_iter()
			.next()
			.ok_or(BenchmarkError::Stop("no destination is configured"))?;
		T::Sender::ensure_successful_send(&destination);
		// Leave a set queued for the destination, the worst case for a transport that appends
		// to queued messages.
		T::Sender::send(&destination, 0, &validators)
			.map_err(|_| BenchmarkError::Stop("the sender rejected the set"))?;

		#[block]
		{
			Pallet::<T>::send_to(destination.clone(), 1, &validators);
		}

		frame_system::Pallet::<T>::assert_last_event(
			<T as frame_system::Config>::RuntimeEvent::from(Event::<T>::AnnouncementSent {
				destination,
				era: 1,
			}),
		);
		Ok(())
	}

	impl_benchmark_test_suite!(
		ValidatorSetAnnouncer,
		crate::mock::new_test_ext(),
		crate::mock::Test
	);
}
