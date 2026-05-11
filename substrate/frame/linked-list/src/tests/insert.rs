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

use crate::{
	list, mock::*, Error, Event, ListHeads, ListNodes, ListSizes, ListTails, Position,
	SortedListInterface,
};
use frame::testing_prelude::{assert_ok, assert_storage_noop, hypothetically};

#[test]
fn insert_into_empty_list_sets_head_tail_size() {
	build_and_execute(|| {
		let steps = LinkedList::insert(1, 100, 50, Position::endpoints_only()).unwrap();
		assert_eq!(steps, 0);
		assert_eq!(ListHeads::<Test>::get(1), Some(100));
		assert_eq!(ListTails::<Test>::get(1), Some(100));
		assert_eq!(ListSizes::<Test>::get(1), 1);
		assert_eq!(dump(1), vec![(100, 50)]);
		System::assert_last_event(
			Event::ItemInserted { list_id: 1, item: 100, priority: 50 }.into(),
		);

		// Re-inserting the same `(list_id, item)` rejects without touching state.
		hypothetically!({
			assert_storage_noop!(assert!(matches!(
				LinkedList::insert(1, 100, 50, Position::endpoints_only()),
				Err(Error::<Test>::ItemAlreadyExists)
			)));
		});
	});
}

#[test]
fn insert_with_valid_hints_o1() {
	build_and_execute(|| {
		insert(1, 100, 90); // head
		insert(1, 200, 50); // tail

		let steps = LinkedList::insert(1, 150, 70, Position::between(100, 200)).unwrap();
		assert_eq!(steps, 0);
		assert_eq!(dump(1), vec![(100, 90), (150, 70), (200, 50)]);
	});
}

#[test]
fn insert_at_head() {
	build_and_execute(|| {
		insert(1, 100, 50);
		assert_ok!(LinkedList::insert(1, 200, 90, Position::at_head(100)));
		assert_eq!(ListHeads::<Test>::get(1), Some(200));
		assert_eq!(dump(1), vec![(200, 90), (100, 50)]);
	});
}

#[test]
fn insert_at_tail() {
	build_and_execute(|| {
		insert(1, 100, 90);
		assert_ok!(LinkedList::insert(1, 200, 10, Position::at_tail(100)));
		assert_eq!(ListTails::<Test>::get(1), Some(200));
		assert_eq!(dump(1), vec![(100, 90), (200, 10)]);
	});
}

#[test]
fn insert_same_priority_lands_at_tail_side_of_cluster() {
	build_and_execute(|| {
		insert(1, 1, 50);
		insert(1, 2, 50);
		insert(1, 3, 50);
		assert_eq!(dump(1), vec![(1, 50), (2, 50), (3, 50)]);
		assert_eq!(<LinkedList as SortedListInterface<_, _>>::iter_from_tail(&1, 3), vec![3, 2, 1]);
	});
}

#[test]
fn insert_existing_item_errors() {
	build_and_execute(|| {
		insert(1, 100, 50);
		assert_storage_noop!(assert!(matches!(
			LinkedList::insert(1, 100, 50, Position::endpoints_only()),
			Err(Error::<Test>::ItemAlreadyExists)
		)));
	});
}

#[test]
fn insert_existing_item_errors_before_hint_repair() {
	build_and_execute(|| {
		// Chain length must exceed `MaxHintRepairSteps` so the hint walk would
		// otherwise be exercised; the duplicate check has to fire first.
		let chain_len = MaxHintRepairSteps::get() + 4;
		for i in 1..=chain_len {
			insert(1, u64::from(i), 100 - 10 * i + 10);
		}
		assert_storage_noop!(assert!(matches!(
			LinkedList::insert(1, 1, 5, Position::at_head(1)),
			Err(Error::<Test>::ItemAlreadyExists)
		)));
	});
}

#[test]
fn insert_does_not_saturate_size_counter() {
	// Manually corrupts `ListSizes` to exercise the saturation guard, so we
	// skip the post-test invariant check.
	build_and_execute_no_post_check(|| {
		ListSizes::<Test>::insert(1, u32::MAX);
		assert_storage_noop!(assert!(matches!(
			LinkedList::insert(1, 100, 50, Position::endpoints_only()),
			Err(Error::<Test>::ListTooLong)
		)));
		assert!(!ListNodes::<Test>::contains_key(1, 100));
		assert!(ListHeads::<Test>::get(1).is_none());
		assert!(ListTails::<Test>::get(1).is_none());
	});
}

#[test]
fn insert_at_missing_neighbor_returns_corrupt_list() {
	build_and_execute_no_post_check(|| {
		assert_storage_noop!(assert!(matches!(
			list::insert_at::<Test>(&1, 200, 50, Position::at_tail(100)),
			Err(Error::<Test>::CorruptList)
		)));
		assert!(!ListNodes::<Test>::contains_key(1, 200));
	});
}
