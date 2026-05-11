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
		let hint = <Pallet<T> as SortedListInterface<T::ListId, T::ItemId>>::find_position(
			list_id, priority,
		);
		Pallet::<T>::insert(list_id.clone(), item.clone(), priority, hint)
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

	/// `insert` parametric over the hint-repair walk length `s`.
	///
	/// Setup: seed `s + 2` items at strictly descending priorities and insert a
	/// new item with priority above the head. The hint
	/// `(seeded[s - 1], seeded[s])` (or `(None, seeded[0])` for `s = 0`) sits
	/// exactly `s` head-ward steps from the correct slot, so the walk runs for
	/// exactly `s` steps. `s = 0` exercises the immediate-valid-hint path.
	#[benchmark]
	fn insert(s: Linear<0, { T::MaxHintRepairSteps::get() }>) -> Result<(), BenchmarkError> {
		let list_id: T::ListId = 0u32.into();
		let s_idx = s as usize;
		let seeded = seed_chain::<T>(&list_id, s + 2);
		let new_item: T::ItemId = u32::MAX.into();
		let new_priority: T::Priority = 1_000_000u32.into(); // above every seed priority
		let hint = Position {
			prev: if s_idx == 0 { None } else { Some(seeded[s_idx - 1].clone()) },
			next: Some(seeded[s_idx].clone()),
		};

		#[block]
		{
			Pallet::<T>::insert(list_id.clone(), new_item.clone(), new_priority, hint).unwrap();
		}

		assert_eq!(ListHeads::<T>::get(&list_id), Some(new_item));
		Ok(())
	}

	/// `remove` a middle node.
	#[benchmark]
	fn remove() {
		let list_id: T::ListId = 0u32.into();
		let seeded = seed_chain::<T>(&list_id, 4);
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
		let list_id: T::ListId = 0u32.into();
		let seeded = seed_chain::<T>(&list_id, 5);
		let middle = seeded[2].clone();
		// `seed_chain` priorities middle/neighbors at 130/(140, 120); 125 stays between.
		let new_priority: T::Priority = 125u32.into();

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
		let list_id: T::ListId = 0u32.into();
		let s_idx = s as usize;
		let seeded = seed_chain::<T>(&list_id, s + 2);
		let target = seeded[s_idx + 1].clone();
		let new_priority: T::Priority = 1_000_000u32.into(); // above every seed priority
		let hint = Position {
			prev: if s_idx == 0 { None } else { Some(seeded[s_idx - 1].clone()) },
			next: Some(seeded[s_idx].clone()),
		};

		#[block]
		{
			Pallet::<T>::re_insert(list_id.clone(), target.clone(), new_priority, hint).unwrap();
		}

		assert_eq!(ListHeads::<T>::get(&list_id), Some(target));
		Ok(())
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
		let hint = Position {
			prev: if s_idx == 0 { None } else { Some(seeded[s_idx - 1].clone()) },
			next: Some(seeded[s_idx + 1].clone()),
		};

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), list_id.clone(), target.clone(), hint);

		assert_eq!(ListHeads::<T>::get(&list_id), Some(target));
		Ok(())
	}

	impl_benchmark_test_suite! {
		Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Test
	}
}
