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
//! Covers the trait surface (`insert`, `remove`, `re_insert`) — the path with a
//! hint-repair walk is parametric over the walk length so consumers can refund
//! unused weight directly from the linear formula — plus the `reprioritize`
//! dispatchable, parametric on the same parameter.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use alloc::vec::Vec;
use frame::benchmarking::prelude::*;

/// Seed `list_id` with `count` items at strictly descending priorities, leaving
/// a gap of two units between adjacent items so that `target_priority - one()`
/// is a valid in-place re-insert value. Returns the items and their priorities
/// in head→tail (priority-descending) order.
fn seed_chain<T: Config>(list_id: &T::ListId, count: u32) -> (Vec<T::ItemId>, Vec<T::Priority>)
where
	T::ItemId: Decode,
	T::Priority: One + Saturating,
{
	let one = T::Priority::one();
	let two = one.saturating_add(one);

	let mut priorities = Vec::with_capacity(count as usize);
	let mut current = two;
	for _ in 0..count {
		priorities.push(current);
		current = current.saturating_add(two);
	}
	priorities.reverse();

	let mut items = Vec::with_capacity(count as usize);
	for (i, priority) in priorities.iter().enumerate() {
		let item: T::ItemId = account("seed", i as u32, 0);
		let hint = <Pallet<T> as SortedListInterface<T::ListId, T::ItemId>>::find_position(
			list_id, *priority,
		);
		Pallet::<T>::insert(list_id.clone(), item.clone(), *priority, hint)
			.expect("benchmark seed insert");
		items.push(item);
	}

	(items, priorities)
}

#[benchmarks(
	where
		T::ListId: Default,
		T::ItemId: Decode,
		T::Priority: One + Bounded + Saturating,
)]
mod benchmarks {
	use super::*;

	/// `insert` parametric over the hint-repair walk length `s`.
	///
	/// Setup: seed `s + 2` items at strictly descending priorities and insert a
	/// new item with priority above the head. The hint
	/// `(seeded[s - 1], seeded[s])` (or `(None, seeded[0])` for `s = 0`) sits
	/// exactly `s` head-ward steps from the correct slot, so the walk runs for
	/// exactly `s` steps. `s = 0` exercises the immediate-valid-hint path.
	#[benchmark]
	fn insert(s: Linear<0, { T::MaxHintRepairSteps::get() }>) -> Result<(), BenchmarkError> {
		let list_id = T::ListId::default();
		let s_idx = s as usize;
		let (seeded, _) = seed_chain::<T>(&list_id, s + 2);
		let new_item: T::ItemId = account("new", 0, 0);
		let new_priority = T::Priority::max_value();
		let hint = Position {
			prev: if s_idx == 0 { None } else { Some(seeded[s_idx - 1].clone()) },
			next: Some(seeded[s_idx].clone()),
		};

		#[block]
		{
			Pallet::<T>::insert(list_id.clone(), new_item.clone(), new_priority, hint).unwrap();
		}

		assert_eq!(Pallet::<T>::head(list_id), Some(new_item));
		Ok(())
	}

	/// `remove` a middle node.
	#[benchmark]
	fn remove() {
		let list_id = T::ListId::default();
		let (seeded, _) = seed_chain::<T>(&list_id, 4);
		let middle = seeded[1].clone();

		#[block]
		{
			Pallet::<T>::remove(&list_id, &middle).unwrap();
		}

		assert!(!ListNodes::<T>::contains_key(&list_id, &middle));
	}

	/// `re_insert` fast path: new priority still fits between the existing
	/// neighbors, so only the node's `priority` field is mutated.
	#[benchmark]
	fn re_insert_in_place() {
		let list_id = T::ListId::default();
		let (seeded, priorities) = seed_chain::<T>(&list_id, 5);
		let middle = seeded[2].clone();
		// `seed_chain` gives gap-of-two priorities; `target - one()` stays
		// strictly between the two neighbors.
		let new_priority = priorities[2].saturating_sub(T::Priority::one());

		#[block]
		{
			Pallet::<T>::re_insert(
				list_id.clone(),
				middle.clone(),
				new_priority,
				Position::endpoints_only(),
			)
			.unwrap();
		}

		assert_eq!(ListNodes::<T>::get(&list_id, &middle).map(|n| n.priority), Some(new_priority),);
	}

	/// `re_insert` slow path parametric over the hint-repair walk length `s`.
	///
	/// Setup: seed `s + 2` items at strictly descending priorities, target the
	/// tail (`seeded[s + 1]`) so its current neighbors cannot admit the new
	/// priority (forcing the slow path), and supply a hint that, after the
	/// internal splice, sits exactly `s` head-ward steps from the new position.
	/// `s = 0` exercises the splice + immediate-valid-hint path.
	#[benchmark]
	fn re_insert_relocate(
		s: Linear<0, { T::MaxHintRepairSteps::get() }>,
	) -> Result<(), BenchmarkError> {
		let list_id = T::ListId::default();
		let s_idx = s as usize;
		let (seeded, _) = seed_chain::<T>(&list_id, s + 2);
		let target = seeded[s_idx + 1].clone();
		let new_priority = T::Priority::max_value();
		let hint = Position {
			prev: if s_idx == 0 { None } else { Some(seeded[s_idx - 1].clone()) },
			next: Some(seeded[s_idx].clone()),
		};

		#[block]
		{
			Pallet::<T>::re_insert(list_id.clone(), target.clone(), new_priority, hint).unwrap();
		}

		assert_eq!(Pallet::<T>::head(list_id), Some(target));
		Ok(())
	}

	/// `reprioritize` when the stored priority already matches the authoritative
	/// priority: one `ListNodes` read and an early return.
	#[benchmark]
	fn reprioritize_no_op() -> Result<(), BenchmarkError> {
		let list_id = T::ListId::default();
		let (seeded, priorities) = seed_chain::<T>(&list_id, 3);
		let target = seeded[1].clone();
		// Pin authoritative priority to the stored value so the drift check
		// returns equal.
		T::PriorityProvider::set_priority(&list_id, &target, priorities[1]);
		let caller: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		reprioritize(
			RawOrigin::Signed(caller),
			list_id.clone(),
			target.clone(),
			Position::endpoints_only(),
		);

		assert_eq!(ListNodes::<T>::get(&list_id, &target).map(|n| n.priority), Some(priorities[1]));
		Ok(())
	}

	/// `reprioritize` on the in-place fast path: the authoritative priority
	/// differs from stored but still fits between the current neighbors, so
	/// `re_insert` mutates the node without moving it.
	#[benchmark]
	fn reprioritize_in_place() -> Result<(), BenchmarkError> {
		let list_id = T::ListId::default();
		let (seeded, priorities) = seed_chain::<T>(&list_id, 3);
		let target = seeded[1].clone();
		// `target - one()` keeps the value strictly between the two neighbors
		// (gap-of-two seeds), so `neighbor_priorities_admit` succeeds and
		// `re_insert` takes the in-place fast path.
		let new_priority = priorities[1].saturating_sub(T::Priority::one());
		T::PriorityProvider::set_priority(&list_id, &target, new_priority);
		let caller: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		reprioritize(
			RawOrigin::Signed(caller),
			list_id.clone(),
			target.clone(),
			Position::endpoints_only(),
		);

		assert_eq!(ListNodes::<T>::get(&list_id, &target).map(|n| n.priority), Some(new_priority));
		assert_eq!(Pallet::<T>::head(list_id.clone()), Some(seeded[0].clone()));
		assert_eq!(Pallet::<T>::tail(list_id), Some(seeded[2].clone()));
		Ok(())
	}

	/// `reprioritize` on the slow splice path, parametric over the hint-repair
	/// walk length `s`.
	///
	/// Setup: seed `s + 2` items at strictly descending priorities, target the
	/// tail (`seeded[s + 1]`) so its current neighbors cannot admit the new
	/// priority (forcing the splice path), drift the authoritative priority
	/// above the head, and supply a hint that, after the internal splice, sits
	/// exactly `s` head-ward steps from the new position. `s = 0` exercises the
	/// splice + immediate-valid-hint path.
	#[benchmark]
	fn reprioritize_relocate(
		s: Linear<0, { T::MaxHintRepairSteps::get() }>,
	) -> Result<(), BenchmarkError> {
		let list_id = T::ListId::default();
		let s_idx = s as usize;
		let (seeded, _) = seed_chain::<T>(&list_id, s + 2);
		let target = seeded[s_idx + 1].clone();
		T::PriorityProvider::set_priority(&list_id, &target, T::Priority::max_value());
		let caller: T::AccountId = whitelisted_caller();
		let hint = Position {
			prev: if s_idx == 0 { None } else { Some(seeded[s_idx - 1].clone()) },
			next: Some(seeded[s_idx].clone()),
		};

		#[extrinsic_call]
		reprioritize(RawOrigin::Signed(caller), list_id.clone(), target.clone(), hint);

		assert_eq!(Pallet::<T>::head(list_id), Some(target));
		Ok(())
	}

	/// `reprioritize` when [`crate::PriorityProvider::priority`] returns `None`:
	/// the item is removed from the list. No `set_priority` call is made, so
	/// the static provider has no entry for `target`.
	#[benchmark]
	fn reprioritize_priority_removed() -> Result<(), BenchmarkError> {
		let list_id = T::ListId::default();
		let (seeded, _) = seed_chain::<T>(&list_id, 3);
		let target = seeded[1].clone();
		let caller: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		reprioritize(
			RawOrigin::Signed(caller),
			list_id.clone(),
			target.clone(),
			Position::endpoints_only(),
		);

		assert!(!ListNodes::<T>::contains_key(&list_id, &target));
		Ok(())
	}

	impl_benchmark_test_suite! {
		Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Test
	}
}
