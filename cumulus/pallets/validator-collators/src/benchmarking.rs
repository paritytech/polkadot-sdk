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
	vec,
	vec::Vec,
};
use codec::Encode;
use cumulus_pallet_parachain_system::OnSystemEvent;
use frame_benchmarking::{account, v2::*, BenchmarkError};
use frame_support::{
	traits::{EnsureOrigin, Get, Hooks},
	BoundedBTreeMap, BoundedBTreeSet, BoundedVec,
};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_session::ShouldEndSession;
use sp_consensus_aura::{Slot, AURA_ENGINE_ID};
use sp_runtime::{traits::Saturating, DigestItem};

/// The slot of the current block, with an Aura digest for slot `slot` added if it has none.
fn current_slot<T: Config>(slot: u64) -> Slot {
	Pallet::<T>::current_slot().unwrap_or_else(|| {
		let slot = Slot::from(slot);
		frame_system::Pallet::<T>::deposit_log(DigestItem::PreRuntime(
			AURA_ENGINE_ID,
			slot.encode(),
		));
		slot
	})
}

/// Put [`CountedSlots`] at the slot of the current block with a zero count, so an advance
/// changes nothing.
fn slots_counted<T: Config>() {
	CountedSlots::<T>::put(SlotCount { slot: current_slot::<T>(1), count: 0 });
}

/// Prepare the next advance of the counted slots to add `g` missed turns, the worst case for
/// `g`: [`Config::MaxTrackedAuthorities`] session validators, a missed turn at every position,
/// and [`CountedSlots`] at a zero count `g + 1` slots before the slot of the current block.
fn slots_to_count<T: Config>(g: u32) -> Result<(), BenchmarkError> {
	let authorities = T::MaxTrackedAuthorities::get();
	if authorities == 0 {
		return Err(BenchmarkError::Stop("MaxTrackedAuthorities is zero"));
	}
	let validators = (0..authorities)
		.map(|i| account("validator", i, 0))
		.collect::<Vec<T::AccountId>>();
	pallet_session::Validators::<T>::put(validators);
	let gap = u64::from(g) + 1;
	let slot = current_slot::<T>(gap);
	if *slot < gap {
		return Err(BenchmarkError::Stop("the block is in a slot before the gap"));
	}
	CountedSlots::<T>::put(SlotCount { slot: (*slot - gap).into(), count: 0 });
	MissedTurns::<T>::put(BoundedVec::truncate_from(vec![1; authorities as usize]));
	Ok(())
}

/// The missed turns [`slots_to_count`] leaves after the advance to the slot of the current block.
fn expected_missed_turns<T: Config>(g: u32) -> Vec<u16> {
	let authorities = u64::from(T::MaxTrackedAuthorities::get());
	let slot = Pallet::<T>::current_slot().map_or(0, |slot| *slot);
	let empty = slot - u64::from(g)..slot;
	(0..authorities)
		.map(|position| {
			if position == slot % authorities {
				0
			} else if empty.clone().any(|s| s % authorities == position) {
				2
			} else {
				1
			}
		})
		.collect()
}

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
	/// collator of this pallet, whose activity record is refreshed, after an earlier call of the
	/// block advanced the counted slots.
	#[benchmark]
	fn note_author() -> Result<(), BenchmarkError> {
		let author: T::AccountId = account("author", 0, 0);
		LastActivity::<T>::insert(&author, Activity::Active { missed: u32::MAX });
		slots_counted::<T>();

		#[block]
		{
			<Pallet<T> as pallet_authorship::EventHandler<_, _>>::note_author(author.clone());
		}

		assert_eq!(AuthoredBlocks::<T>::get(&author), 1);
		assert_eq!(LastActivity::<T>::get(&author), Some(Activity::Active { missed: 0 }));
		Ok(())
	}

	/// A block at a multiple of `ReportPeriod` that makes the early report of the stored set with
	/// no counts, after an earlier call of the block advanced the counted slots, and the
	/// `PendingRotation` read of `should_end_session`.
	#[benchmark]
	fn on_initialize() -> Result<(), BenchmarkError> {
		let validators = BoundedBTreeSet::try_from(BTreeSet::from([account("validator", 0, 0)]))
			.map_err(|_| BenchmarkError::Stop("the set exceeds MaxValidators"))?;
		Pallet::<T>::do_receive_validator_set(0, validators)?;
		// The early report is due at any time with a zero expected era duration.
		ExpectedEraDuration::<T>::put(0);
		let now = ReportPeriod::<T>::get();
		slots_counted::<T>();

		#[block]
		{
			<Pallet<T> as Hooks<BlockNumberFor<T>>>::on_initialize(now);
			<Pallet<T> as ShouldEndSession<_>>::should_end_session(now);
		}

		assert_eq!(SetReceivedAt::<T>::get(), None);
		assert_eq!(CountedSlots::<T>::get().map(|counted| counted.count), Some(0));
		Ok(())
	}

	/// The first advance of the counted slots in a block, which loops over `g` empty slots and
	/// adds a missed turn for each.
	#[benchmark]
	fn advance_slots(
		g: Linear<0, { T::MaxTrackedAuthorities::get().saturating_sub(1) }>,
	) -> Result<(), BenchmarkError> {
		slots_to_count::<T>(g)?;

		#[block]
		{
			Pallet::<T>::advance_slots(Pallet::<T>::current_slot());
		}

		assert_eq!(CountedSlots::<T>::get().map(|counted| counted.count), Some(u64::from(g) + 1));
		assert_eq!(MissedTurns::<T>::get().into_inner(), expected_missed_turns::<T>(g));
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
		CurrentDropRule::<T>::put(DropRule { idle_turns: 2, drop_slots: 1, min_collators: 0 });
		let records = BoundedBTreeMap::try_from(
			(0..T::MaxValidators::get())
				.map(|i| (account("dropped", i, 0), Some(0)))
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
