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

//! Benchmarking setup for pallet-validator-collators.

#![cfg(feature = "runtime-benchmarks")]

use super::*;

#[allow(unused)]
use crate::Pallet as ValidatorCollators;
use alloc::collections::BTreeSet;
use frame_benchmarking::{account, v2::*, BenchmarkError};
use frame_support::{
	traits::{EnsureOrigin, Get},
	BoundedBTreeSet,
};

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_validators(n: Linear<1, { T::MaxValidators::get() }>) -> Result<(), BenchmarkError> {
		let set = |name: &'static str, count: u32| {
			BoundedBTreeSet::try_from(
				(0..count).map(|i| account(name, i, 0)).collect::<BTreeSet<_>>(),
			)
			.map_err(|_| BenchmarkError::Stop("the set exceeds MaxValidators"))
		};
		Pallet::<T>::do_receive_validator_set(0, set("stored", T::MaxValidators::get())?)?;
		let validators = set("validator", n)?;

		#[block]
		{
			Pallet::<T>::do_receive_validator_set(1, validators)?;
		}

		assert_eq!(
			ValidatorSet::<T>::get().map(|set| (set.era, set.validators.len() as u32)),
			Some((1, n))
		);
		assert_eq!(PendingRotation::<T>::get(), RotationState::AwaitingQueue);
		Ok(())
	}

	#[benchmark]
	fn set_max_collators() -> Result<(), BenchmarkError> {
		let origin =
			T::UpdateOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, Some(1));

		assert_eq!(MaxCollators::<T>::get(), Some(1));
		Ok(())
	}

	impl_benchmark_test_suite!(ValidatorCollators, crate::mock::new_test_ext(), crate::mock::Test);
}
