// This file is part of Substrate.

// Copyright (C) Amforc AG.
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

//! Benchmarks for `pallet-linked-list`.
//!
//! Covers the trait surface (`insert`, `remove`, `re_insert`) on both the fast
//! and worst-case paths, plus the `reprioritize` dispatchable parametric over
//! hint-repair walk length.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame::benchmarking::prelude::*;

/// Seed `list_id` with `count` items at strictly descending priorities
/// `(count - i) * 10 + 100`. Returns the items in head→tail order.
fn seed_chain<T: Config>(list_id: &T::ListId, count: u32) -> Vec<T::ItemId>
where
	T::ItemId: From<u32>,
	T::Priority: From<u32>,
{
	let mut items = Vec::with_capacity(count as usize);
	for i in 0..count {
		let item: T::ItemId = i.into();
		let priority: T::Priority = ((count - i) * 10 + 100).into();
		let (prev, next) = <Pallet<T> as SortedListInterface<T::ListId, T::ItemId>>::find_position(
			list_id, priority,
		);
		<Pallet<T> as SortedListInterface<T::ListId, T::ItemId>>::insert(
			list_id.clone(),
			item.clone(),
			priority,
			prev,
			next,
		)
		.expect("benchmark seed insert");
		items.push(item);
	}
	items
}

#[benchmarks(
	where
		T::ListId: From<u32>,
		T::ItemId: From<u32>,
		T::Priority: From<u32>,
)]
mod benchmarks {
	use super::*;

	/// `insert` at the head with a valid hint: pure splice, no repair walk.
	#[benchmark]
	fn insert_terminal() {
		let list_id: T::ListId = 0u32.into();
		let _seed = seed_chain::<T>(&list_id, 4);
		let head = ListHeads::<T>::get(&list_id);
		let new_item: T::ItemId = 99u32.into();
		let new_priority: T::Priority = 10_000u32.into(); // higher than every seed priority

		#[block]
		{
			<Pallet<T> as SortedListInterface<T::ListId, T::ItemId>>::insert(
				list_id.clone(),
				new_item.clone(),
				new_priority,
				None,
				head,
			)
			.unwrap();
		}

		assert_eq!(ListHeads::<T>::get(&list_id), Some(new_item));
	}

	/// `insert` with a hint that requires a full `MaxHintRepairSteps` walk.
	#[benchmark]
	fn insert_worst_case() {
		let list_id: T::ListId = 0u32.into();
		let budget = T::MaxHintRepairSteps::get();
		// Position the real insert slot exactly `budget` steps from the hint.
		let seeded = seed_chain::<T>(&list_id, budget + 2);
		let new_item: T::ItemId = budget.saturating_add(2).into();
		let between_priority: T::Priority = (2 * 10 + 100 - 5).into();
		let hint_prev = Some(seeded[0].clone());
		let hint_next = Some(seeded[1].clone());

		#[block]
		{
			<Pallet<T> as SortedListInterface<T::ListId, T::ItemId>>::insert(
				list_id.clone(),
				new_item.clone(),
				between_priority,
				hint_prev,
				hint_next,
			)
			.unwrap();
		}

		assert!(ListNodes::<T>::contains_key(&list_id, &new_item));
	}

	/// `remove` a middle node.
	#[benchmark]
	fn remove() {
		let list_id: T::ListId = 0u32.into();
		let seeded = seed_chain::<T>(&list_id, 4);
		let middle = seeded[1].clone();

		#[block]
		{
			<Pallet<T> as SortedListInterface<T::ListId, T::ItemId>>::remove(&list_id, &middle)
				.unwrap();
		}

		assert!(!ListNodes::<T>::contains_key(&list_id, &middle));
	}

	/// `re_insert` fast path: new priority still fits between the existing
	/// neighbors, so only the node's `priority` field is mutated.
	#[benchmark]
	fn re_insert_in_place() {
		let list_id: T::ListId = 0u32.into();
		let seeded = seed_chain::<T>(&list_id, 5);
		let middle = seeded[2].clone();
		// `seed_chain` priorities middle/neighbors at 130/(140, 120); 125 stays between.
		let new_priority: T::Priority = 125u32.into();

		#[block]
		{
			<Pallet<T> as SortedListInterface<T::ListId, T::ItemId>>::re_insert(
				list_id.clone(),
				middle.clone(),
				new_priority,
				None,
				None,
			)
			.unwrap();
		}

		assert_eq!(ListNodes::<T>::get(&list_id, &middle).map(|n| n.priority), Some(new_priority),);
	}

	/// `re_insert` slow path: priority change forces splice + repair + insert.
	#[benchmark]
	fn re_insert_relocate() {
		let list_id: T::ListId = 0u32.into();
		let seeded = seed_chain::<T>(&list_id, 5);
		let middle = seeded[2].clone();
		// Push the priority above the head; hint comes from `find_re_insert_position`.
		let new_priority: T::Priority = 10_000u32.into();
		let (prev, next) =
			<Pallet<T> as SortedListInterface<T::ListId, T::ItemId>>::find_re_insert_position(
				&list_id,
				&middle,
				new_priority,
			)
			.unwrap();

		#[block]
		{
			<Pallet<T> as SortedListInterface<T::ListId, T::ItemId>>::re_insert(
				list_id.clone(),
				middle.clone(),
				new_priority,
				prev,
				next,
			)
			.unwrap();
		}

		assert_eq!(ListHeads::<T>::get(&list_id), Some(middle));
	}

	/// `reprioritize` parametric over the hint-repair walk length `s`.
	///
	/// Setup: seed `s + 2` items at strictly descending priorities, drift the
	/// item at index `s` above the head, and supply a hint that, after the
	/// internal splice, sits exactly `s` head-ward steps away from the new
	/// position. `s = 0` exercises the splice + immediate-valid-hint path.
	/// Drift is set up via [`crate::BenchmarkHelper::set_priority`].
	#[benchmark]
	fn reprioritize(s: Linear<0, { T::MaxHintRepairSteps::get() }>) -> Result<(), BenchmarkError> {
		let list_id: T::ListId = 0u32.into();
		let s_idx = s as usize;
		let seeded = seed_chain::<T>(&list_id, s + 2);
		let target = seeded[s_idx].clone();
		T::BenchmarkHelper::set_priority(&list_id, &target, 10_000u32.into());
		let caller: T::AccountId = whitelisted_caller();
		let hint_prev = if s_idx == 0 { None } else { Some(seeded[s_idx - 1].clone()) };
		let hint_next = Some(seeded[s_idx + 1].clone());

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), list_id.clone(), target.clone(), hint_prev, hint_next);

		assert_eq!(ListHeads::<T>::get(&list_id), Some(target));
		Ok(())
	}

	impl_benchmark_test_suite! {
		Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Test
	}
}
