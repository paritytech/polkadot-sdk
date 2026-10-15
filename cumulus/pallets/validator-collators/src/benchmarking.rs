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
use alloc::{
	collections::{BTreeMap, BTreeSet},
	vec::Vec,
};
use cumulus_pallet_parachain_system::OnSystemEvent;
use frame_benchmarking::{account, v2::*, BenchmarkError};
use frame_support::{
	traits::{EnsureOrigin, Get, Hooks},
	BoundedBTreeMap, BoundedBTreeSet, BoundedVec,
};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_session::ShouldEndSession;
use sp_runtime::traits::{Saturating, Zero};

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

	/// Worst case: removing the cap with a stored set and drawn collators.
	#[benchmark]
	fn set_max_collators() -> Result<(), BenchmarkError> {
		let origin =
			T::UpdateOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let max = T::MaxValidators::get();
		let stored = BoundedBTreeSet::try_from(
			(0..max).map(|i| account("stored", i, 0)).collect::<BTreeSet<_>>(),
		)
		.map_err(|_| BenchmarkError::Stop("the set exceeds MaxValidators"))?;
		let drawn = BoundedVec::truncate_from(stored.iter().cloned().collect());
		Pallet::<T>::do_receive_validator_set(0, stored)?;
		Collators::<T>::put(DrawnCollators { era: 0, validators: drawn });

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, None);

		assert_eq!(MaxCollators::<T>::get(), None);
		assert!(!Collators::<T>::exists());
		assert_eq!(PendingRotation::<T>::get(), RotationState::AwaitingQueue);
		Ok(())
	}

	/// The relay chain epoch randomness changes, so the cache is written.
	#[benchmark]
	fn on_relay_state_proof() {
		EpochRandomness::<T>::put([1; 32]);
		let proof = crate::relay_proof::with_epoch_randomness([2; 32]);

		#[block]
		{
			<Pallet<T> as OnSystemEvent>::on_relay_state_proof(&proof);
		}

		assert_eq!(EpochRandomness::<T>::get(), Some([2; 32]));
	}

	#[benchmark]
	fn set_report_period() -> Result<(), BenchmarkError> {
		let origin =
			T::UpdateOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let period = T::DefaultReportPeriod::get().saturating_add(1u32.into());

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, period);

		assert_eq!(ReportPeriod::<T>::get(), period);
		Ok(())
	}

	#[benchmark]
	fn set_expected_era_duration() -> Result<(), BenchmarkError> {
		let origin =
			T::UpdateOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let duration = T::DefaultExpectedEraDuration::get().saturating_add(1);

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, duration);

		assert_eq!(ExpectedEraDuration::<T>::get(), duration);
		Ok(())
	}

	#[benchmark]
	fn set_report_margin() -> Result<(), BenchmarkError> {
		let origin =
			T::UpdateOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let margin = T::MaxReportMargin::get();

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, margin);

		assert_eq!(ReportMargin::<T>::get(), margin);
		Ok(())
	}

	/// The first block of an author, which inserts its entry and bumps the counter, by an active
	/// collator of this pallet, whose activity record is refreshed.
	#[benchmark]
	fn note_author() {
		let author: T::AccountId = account("author", 0, 0);
		LastActivity::<T>::insert(
			&author,
			Activity::Active { since: Zero::zero(), authorities: T::MaxValidators::get() },
		);
		frame_system::Pallet::<T>::set_block_number(1u32.into());

		#[block]
		{
			<Pallet<T> as pallet_authorship::EventHandler<_, _>>::note_author(author.clone());
		}

		assert_eq!(AuthoredBlocks::<T>::get(&author), 1);
		assert_eq!(
			LastActivity::<T>::get(&author),
			Some(Activity::Active { since: 1u32.into(), authorities: 0 })
		);
	}

	/// A block at a multiple of `ReportPeriod` that makes the early report of the stored set with
	/// no counts, and the `PendingRotation` read of `should_end_session`.
	#[benchmark]
	fn on_initialize() -> Result<(), BenchmarkError> {
		let validators = BoundedBTreeSet::try_from(BTreeSet::from([account("validator", 0, 0)]))
			.map_err(|_| BenchmarkError::Stop("the set exceeds MaxValidators"))?;
		Pallet::<T>::do_receive_validator_set(0, validators)?;
		// The early report is due at any time with a zero expected era duration.
		ExpectedEraDuration::<T>::put(0);
		let now = ReportPeriod::<T>::get();

		#[block]
		{
			<Pallet<T> as Hooks<BlockNumberFor<T>>>::on_initialize(now);
			<Pallet<T> as ShouldEndSession<_>>::should_end_session(now);
		}

		assert_eq!(SetReceivedAt::<T>::get(), None);
		Ok(())
	}

	/// A stored set of `MaxValidators` validators, `n` of them with counts, every count handed
	/// to the runtime's reporter.
	#[benchmark]
	fn report_blocks(n: Linear<1, { T::MaxValidators::get() }>) -> Result<(), BenchmarkError> {
		let validators = (0..T::MaxValidators::get())
			.map(|i| account("validator", i, 0))
			.collect::<Vec<T::AccountId>>();
		Pallet::<T>::receive_validator_set(0, validators.iter().cloned())?;
		let blocks = validators.into_iter().take(n as usize).map(|v| (v, 1)).collect::<Vec<_>>();
		for (who, count) in &blocks {
			AuthoredBlocks::<T>::insert(who, count);
		}
		T::BlockReporter::ensure_successful_report(&blocks);

		#[block]
		{
			Pallet::<T>::report_blocks();
		}

		assert_eq!(AuthoredBlocks::<T>::count(), 0);
		frame_system::Pallet::<T>::assert_last_event(
			<T as frame_system::Config>::RuntimeEvent::from(Event::<T>::BlocksReported {
				authors: n,
				reported: n,
			}),
		);
		Ok(())
	}

	/// Worst case: removing the rule with `MaxValidators` drop records.
	#[benchmark]
	fn set_drop_rule() -> Result<(), BenchmarkError> {
		let origin =
			T::UpdateOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		CurrentDropRule::<T>::put(DropRule { rounds: 2, blocks: 1u32.into(), min_collators: 0 });
		let records = BoundedBTreeMap::try_from(
			(0..T::MaxValidators::get())
				.map(|i| (account("dropped", i, 0), Some(Zero::zero())))
				.collect::<BTreeMap<_, _>>(),
		)
		.map_err(|_| BenchmarkError::Stop("the records exceed MaxValidators"))?;
		Dropped::<T>::put(records);

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, None);

		assert!(!CurrentDropRule::<T>::exists());
		assert!(!Dropped::<T>::exists());
		Ok(())
	}

	impl_benchmark_test_suite!(ValidatorCollators, crate::mock::new_test_ext(), crate::mock::Test);
}
