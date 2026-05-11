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

//! Tests mock for `pallet-linked-list`.

#![cfg(test)]

use crate as pallet_linked_list;
use alloc::collections::BTreeMap;
use frame::testing_prelude::*;

pub type ListId = u32;
pub type ItemId = u64;
pub type Priority = u32;

type Block = MockBlock<Test>;

#[frame_construct_runtime]
mod runtime {
	#[runtime::runtime]
	#[runtime::derive(RuntimeCall, RuntimeEvent, RuntimeError, RuntimeOrigin, RuntimeTask)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system;

	#[runtime::pallet_index(1)]
	pub type LinkedList = pallet_linked_list;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

parameter_types! {
	pub static StaticPriorities: BTreeMap<(ListId, ItemId), Priority> = BTreeMap::new();
	pub const MaxHintRepairSteps: u32 = 4;
}

pub struct StaticPriorityProvider;
impl pallet_linked_list::PriorityProvider<ListId, ItemId> for StaticPriorityProvider {
	type Priority = Priority;
	fn priority(list_id: &ListId, item: &ItemId) -> Option<Priority> {
		StaticPriorities::get().get(&(*list_id, *item)).copied()
	}
}

#[cfg(feature = "runtime-benchmarks")]
pub struct LinkedListBenchHelper;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_linked_list::BenchmarkHelper<ListId, ItemId, Priority> for LinkedListBenchHelper {
	fn set_priority(list_id: &ListId, item: &ItemId, priority: Priority) {
		StaticPriorities::mutate(|m| {
			m.insert((*list_id, *item), priority);
		});
	}
}

impl pallet_linked_list::Config for Test {
	type WeightInfo = ();
	type ListId = ListId;
	type ItemId = ItemId;
	type Priority = Priority;
	type MaxHintRepairSteps = MaxHintRepairSteps;
	type PriorityProvider = StaticPriorityProvider;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = LinkedListBenchHelper;
}

pub(crate) fn new_test_ext() -> TestState {
	let mut ext: TestState =
		frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into();
	ext.execute_with(|| System::set_block_number(1));
	ext
}

/// Run `test` against a fresh externality and unconditionally re-check the
/// pallet's invariants afterwards under `try-runtime`.
pub(crate) fn build_and_execute(test: impl FnOnce()) {
	new_test_ext().execute_with(|| {
		test();
		#[cfg(feature = "try-runtime")]
		LinkedList::do_try_state().expect("invariants hold post-test");
	});
}

/// Like [`build_and_execute`], but skips the post-test invariant check. Use
/// only for tests that deliberately leave storage corrupt to exercise
/// `do_try_state` directly.
pub(crate) fn build_and_execute_no_post_check(test: impl FnOnce()) {
	new_test_ext().execute_with(test);
}

/// Set the authoritative priority for `(list_id, item)` so [`StaticPriorityProvider`]
/// reports it. Used in `dispatchables` tests for the `reprioritize` flow.
pub(crate) fn set_real_priority(list_id: ListId, item: ItemId, priority: Priority) {
	StaticPriorities::mutate(|m| {
		m.insert((list_id, item), priority);
	});
}

/// Convenience: insert via the `SortedListInterface` with the hint fetched
/// from `find_position`. Returns the `repair_steps` count.
pub(crate) fn insert(list_id: ListId, item: ItemId, priority: Priority) -> u32 {
	use pallet_linked_list::SortedListInterface;
	let hint =
		<LinkedList as SortedListInterface<ListId, ItemId>>::find_position(&list_id, priority);
	LinkedList::insert(list_id, item, priority, hint).expect("insert succeeds in tests")
}

/// Items in `list_id` head-to-tail.
pub(crate) fn dump(list_id: ListId) -> alloc::vec::Vec<(ItemId, Priority)> {
	let count = pallet_linked_list::ListSizes::<Test>::get(list_id);
	let mut out = alloc::vec::Vec::with_capacity(count as usize);
	let mut cursor = pallet_linked_list::ListHeads::<Test>::get(list_id);
	while let Some(item) = cursor {
		let node = pallet_linked_list::ListNodes::<Test>::get(list_id, item)
			.expect("listed items are stored; qed");
		out.push((item, node.priority));
		cursor = node.next;
	}
	out
}
