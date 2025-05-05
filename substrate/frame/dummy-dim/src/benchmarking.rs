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

//! Dummy DIM pallet benchmarking.

extern crate alloc;

use alloc::vec::Vec;

use super::*;
use crate::Pallet as DummyDim;

use frame_benchmarking::v2::{benchmarks, *};
use frame_support::{assert_ok, traits::Get};
use frame_system::RawOrigin;

type SecretOf<T> = <<T as Config>::People as AddOnlyPeopleTrait>::Secret;
type MemberOf<T> = <<T as Config>::People as AddOnlyPeopleTrait>::Member;

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

fn new_member_from<T: Config>(id: PersonalId) -> (MemberOf<T>, SecretOf<T>) {
	T::People::mock_key(id)
}

fn generate_members<T: Config>(start: u32, end: u32) -> Vec<(MemberOf<T>, SecretOf<T>)> {
	(start..end).map(|i| new_member_from::<T>(i as PersonalId)).collect::<Vec<_>>()
}

#[benchmarks]
mod benches {
	use super::*;

	#[benchmark]
	fn reserve_ids(c: Linear<1, { T::MaxPersonBatchSize::get() }>) -> Result<(), BenchmarkError> {
		#[extrinsic_call]
		_(RawOrigin::Root, c);

		assert_last_event::<T>(Event::IdsReserved { count: c }.into());
		Ok(())
	}

	#[benchmark]
	fn renew_id_reservation() -> Result<(), BenchmarkError> {
		assert_ok!(DummyDim::<T>::reserve_ids(RawOrigin::Root.into(), 1));
		assert_ok!(DummyDim::<T>::cancel_id_reservation(RawOrigin::Root.into(), 0));

		#[extrinsic_call]
		_(RawOrigin::Root, 0);

		assert_last_event::<T>(Event::IdRenewed { id: 0 }.into());
		Ok(())
	}

	#[benchmark]
	fn cancel_id_reservation() -> Result<(), BenchmarkError> {
		assert_ok!(DummyDim::<T>::reserve_ids(RawOrigin::Root.into(), 1));

		#[extrinsic_call]
		_(RawOrigin::Root, 0);

		assert_last_event::<T>(Event::IdUnreserved { id: 0 }.into());
		Ok(())
	}

	#[benchmark]
	fn recognize_personhood(
		c: Linear<1, { T::MaxPersonBatchSize::get() }>,
	) -> Result<(), BenchmarkError> {
		assert_ok!(DummyDim::<T>::reserve_ids(RawOrigin::Root.into(), c));
		let keys = generate_members::<T>(0, c);
		let keys_and_ids: Vec<_> =
			(0..c).map(|i| (i as PersonalId, keys[i as usize].0.clone())).collect();

		#[extrinsic_call]
		_(RawOrigin::Root, keys_and_ids.try_into().unwrap());

		assert_last_event::<T>(Event::PeopleRegistered { count: c }.into());
		Ok(())
	}

	#[benchmark]
	fn suspend_personhood(
		c: Linear<1, { T::MaxPersonBatchSize::get() }>,
	) -> Result<(), BenchmarkError> {
		assert_ok!(DummyDim::<T>::reserve_ids(RawOrigin::Root.into(), c));
		let keys = generate_members::<T>(0, c);
		let keys_and_ids: Vec<_> =
			(0..c).map(|i| (i as PersonalId, keys[i as usize].0.clone())).collect();
		assert_ok!(DummyDim::<T>::recognize_personhood(
			RawOrigin::Root.into(),
			keys_and_ids.try_into().unwrap()
		));
		assert_ok!(T::People::start_people_set_mutation_session());
		let ids: Vec<_> = (0..c as PersonalId).collect();

		#[extrinsic_call]
		_(RawOrigin::Root, ids.try_into().unwrap());

		assert_last_event::<T>(Event::PeopleSuspended { count: c }.into());
		Ok(())
	}

	#[benchmark]
	fn resume_personhood() -> Result<(), BenchmarkError> {
		let people_count = T::MaxPersonBatchSize::get();
		assert_ok!(DummyDim::<T>::reserve_ids(RawOrigin::Root.into(), people_count));
		let keys = generate_members::<T>(0, people_count);
		let keys_and_ids: Vec<_> = (0..people_count)
			.map(|i| (i as PersonalId, keys[i as usize].0.clone()))
			.collect();
		assert_ok!(DummyDim::<T>::recognize_personhood(
			RawOrigin::Root.into(),
			keys_and_ids.try_into().unwrap()
		));
		assert_ok!(T::People::start_people_set_mutation_session());
		let ids: Vec<_> = (0..people_count as PersonalId).collect();
		assert_ok!(DummyDim::<T>::suspend_personhood(
			RawOrigin::Root.into(),
			ids.clone().try_into().unwrap()
		));

		#[extrinsic_call]
		_(RawOrigin::Root, 0);

		assert_last_event::<T>(Event::PersonhoodResumed { id: 0 }.into());
		Ok(())
	}

	#[benchmark]
	fn start_mutation_session() -> Result<(), BenchmarkError> {
		#[extrinsic_call]
		_(RawOrigin::Root);

		assert_last_event::<T>(Event::SuspensionsStarted.into());
		Ok(())
	}

	#[benchmark]
	fn end_mutation_session() -> Result<(), BenchmarkError> {
		assert_ok!(T::People::start_people_set_mutation_session());

		#[extrinsic_call]
		_(RawOrigin::Root);

		assert_last_event::<T>(Event::SuspensionsEnded.into());
		Ok(())
	}

	// Implements a test for each benchmark. Execute with:
	// `cargo test -p pallet-dummy-dim --features runtime-benchmarks`.
	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
