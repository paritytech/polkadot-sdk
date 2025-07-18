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

//! Benchmarking setup for pallet-staking-async-ah-client

use super::*;
use frame_benchmarking::v2::*;
use frame_support::traits::Get;

use sp_staking::SessionIndex;

const SEED: u32 = 0;

fn create_offenders<T: Config>(n: u32) -> Vec<T::AccountId> {
	(0..n).map(|i| frame_benchmarking::account("offender", i, SEED)).collect()
}

fn create_buffered_offences<T: Config>(
	_session: SessionIndex,
	offenders: &[T::AccountId],
) -> BTreeMap<T::AccountId, BufferedOffence<T::AccountId>> {
	offenders
		.iter()
		.enumerate()
		.map(|(i, offender)| {
			let slash_fraction = sp_runtime::Perbill::from_percent(10 + (i % 90) as u32);
			(offender.clone(), BufferedOffence { reporter: Some(offender.clone()), slash_fraction })
		})
		.collect()
}

fn setup_buffered_offences<T: Config>(n: u32) -> SessionIndex {
	// Set the pallet to Buffered mode
	Mode::<T>::put(OperatingMode::Buffered);

	// Create offenders
	let offenders = create_offenders::<T>(n);

	// Use a specific session for testing
	let session: SessionIndex = 42;

	// Create buffered offences
	let offences_map = create_buffered_offences::<T>(session, &offenders);

	// Store the buffered offences
	BufferedOffences::<T>::mutate(|buffered| {
		buffered.insert(session, offences_map);
	});

	session
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn process_buffered_offences(n: Linear<1, { T::MaxOffenceBatchSize::get() }>) {
		// Setup: Create buffered offences and put pallet in Active mode
		let session = setup_buffered_offences::<T>(n);

		// Transition to Active mode to trigger processing
		Mode::<T>::put(OperatingMode::Active);

		// Verify offences exist before processing
		assert!(BufferedOffences::<T>::get().contains_key(&session));

		#[block]
		{
			Pallet::<T>::process_buffered_offences();
		}

		// Verify some offences were processed
		// In a real scenario, either the session is gone or has fewer offences
		let remaining_offences =
			BufferedOffences::<T>::get().get(&session).map(|m| m.len()).unwrap_or(0);
		let expected_remaining = if n > T::MaxOffenceBatchSize::get() {
			(n - T::MaxOffenceBatchSize::get()) as usize
		} else {
			0
		};
		assert_eq!(remaining_offences, expected_remaining);
	}

	#[cfg(test)]
	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
