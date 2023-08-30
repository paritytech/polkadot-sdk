// Copyright (C) 2021 Parity Technologies (UK) Ltd.
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

//! Benchmarking setup for pallet-collator-selection

#![cfg(feature = "runtime-benchmarks")]

use super::*;

#[allow(unused)]
use crate::Pallet as Fixed;
use codec::Decode;
use frame_benchmarking::{account, impl_benchmark_test_suite, v2::*, BenchmarkError};
use frame_support::traits::{EnsureOrigin, Get};
use frame_system::{pallet_prelude::BlockNumberFor, EventRecord};
use pallet_authorship::EventHandler;
use pallet_session::{self as session, SessionManager};
use sp_std::prelude::*;

const SEED: u32 = 0;

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	let events = frame_system::Pallet::<T>::events();
	let system_event: <T as frame_system::Config>::RuntimeEvent = generic_event.into();
	// compare to the last event record
	let EventRecord { event, .. } = &events[events.len() - 1];
	assert_eq!(event, &system_event);
}

fn keys<T: Config + session::Config>(c: u32) -> <T as session::Config>::Keys {
	use rand::{RngCore, SeedableRng};

	let keys = {
		let mut keys = [0u8; 128];

		if c > 0 {
			let mut rng = rand::rngs::StdRng::seed_from_u64(c as u64);
			rng.fill_bytes(&mut keys);
		}

		keys
	};

	Decode::decode(&mut &keys[..]).unwrap()
}

fn validator<T: Config + session::Config>(c: u32) -> (T::AccountId, <T as session::Config>::Keys) {
	(account("candidate", c, 1000), keys::<T>(c))
}

fn register_validators<T: Config + session::Config>(count: u32) -> Vec<T::AccountId> {
	let validators = (0..count).map(|c| validator::<T>(c)).collect::<Vec<_>>();

	validators.into_iter().map(|(who, _)| who).collect()
}

#[benchmarks(where T: pallet_authorship::Config + session::Config)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn add_collator(c: Linear<1, { T::MaxCollators::get() - 1 }>) -> Result<(), BenchmarkError> {
		let origin =
			T::UpdateOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		let mut collators = register_validators::<T>(c);
		collators.sort();
		let collators: frame_support::BoundedVec<_, T::MaxCollators> =
			frame_support::BoundedVec::try_from(collators).unwrap();
		<Collators<T>>::put(collators);

		let (to_add, _) = validator::<T>(c);

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, to_add.clone());

		assert_last_event::<T>(Event::CollatorAdded { account_id: to_add }.into());
		Ok(())
	}

	#[benchmark]
	fn remove_collator(c: Linear<1, { T::MaxCollators::get() }>) -> Result<(), BenchmarkError> {
		let origin =
			T::UpdateOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let mut collators = register_validators::<T>(c);
		collators.sort();
		let collators: frame_support::BoundedVec<_, T::MaxCollators> =
			frame_support::BoundedVec::try_from(collators).unwrap();
		<Collators<T>>::put(collators);
		let to_remove = <Collators<T>>::get().first().unwrap().clone();

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, to_remove.clone());

		assert_last_event::<T>(Event::CollatorRemoved { account_id: to_remove }.into());
		Ok(())
	}

	// worse case is paying a non-existing candidate account.
	#[benchmark]
	fn note_author() {
		let author: T::AccountId = account("author", 0, SEED);
		let new_block: BlockNumberFor<T> = 10u32.into();

		frame_system::Pallet::<T>::set_block_number(new_block);

		#[block]
		{
			<Fixed<T> as EventHandler<_, _>>::note_author(author.clone())
		}

		assert_eq!(frame_system::Pallet::<T>::block_number(), new_block);
	}

	// worst case for new session.
	#[benchmark]
	fn new_session(c: Linear<1, { T::MaxCollators::get() }>) {
		frame_system::Pallet::<T>::set_block_number(0u32.into());

		register_validators::<T>(c);

		let collators = <Collators<T>>::get();
		let pre_length = collators.len();

		#[block]
		{
			let actual_collators = <Fixed<T> as SessionManager<_>>::new_session(0);
			let expected_collators: Vec<T::AccountId> = collators.iter().cloned().collect();
			assert_eq!(actual_collators.unwrap(), expected_collators);
		}

		assert!(<Collators<T>>::get().len() == pre_length);
	}

	impl_benchmark_test_suite!(Fixed, crate::mock::new_test_ext(), crate::mock::Test,);
}
