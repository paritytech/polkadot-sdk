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
	list, mock::*, Error, Event, ListMeta, ListMetas, ListNodes, Position, SortedListInterface,
};
use frame::testing_prelude::{assert_ok, assert_storage_noop, hypothetically};

#[test]
fn insert_into_empty_list_sets_head_tail_size() {
	build_and_execute(|| {
		let steps = LinkedList::insert(1, 100, 50, Position::endpoints_only()).unwrap();
		assert_eq!(steps, 0);
		assert_eq!(LinkedList::head(1), Some(100));
		assert_eq!(LinkedList::tail(1), Some(100));
		assert_eq!(LinkedList::count(1), 1);
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
fn insert_creating_list_emits_list_created() {
	build_and_execute(|| {
		// The first item creates the list: `ListCreated` then `ItemInserted`.
		insert(1, 100, 50);
		System::assert_has_event(Event::ListCreated { list_id: 1 }.into());
		System::assert_last_event(
			Event::ItemInserted { list_id: 1, item: 100, priority: 50 }.into(),
		);

		// A second item into the same list must not re-emit `ListCreated`.
		System::reset_events();
		insert(1, 200, 40);
		assert_eq!(System::events().len(), 1);
		System::assert_last_event(
			Event::ItemInserted { list_id: 1, item: 200, priority: 40 }.into(),
		);
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
		assert_eq!(LinkedList::head(1), Some(200));
		assert_eq!(dump(1), vec![(200, 90), (100, 50)]);
	});
}

#[test]
fn insert_at_tail() {
	build_and_execute(|| {
		insert(1, 100, 90);
		assert_ok!(LinkedList::insert(1, 200, 10, Position::at_tail(100)));
		assert_eq!(LinkedList::tail(1), Some(200));
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
	// Manually corrupts `ListMetas.len` to exercise the saturation guard, so we
	// skip the post-test invariant check.
	build_and_execute_no_post_check(|| {
		ListMetas::<Test>::insert(1, ListMeta { len: u32::MAX, ..Default::default() });
		assert_storage_noop!(assert!(matches!(
			LinkedList::insert(1, 100, 50, Position::endpoints_only()),
			Err(Error::<Test>::ListTooLong)
		)));
		assert!(!ListNodes::<Test>::contains_key(1, 100));
		assert!(LinkedList::head(1).is_none());
		assert!(LinkedList::tail(1).is_none());
	});
}

#[test]
#[should_panic = "Defensive failure has been triggered"]
fn insert_at_missing_neighbor_is_defensive() {
	build_and_execute_no_post_check(|| {
		// The hint names a prev neighbor (100) that does not exist.
		let _ = list::insert_at::<Test>(&1, &200, 50, Position::at_tail(100));
	});
}

#[test]
#[should_panic = "head pointer disagrees with head-side insert"]
fn insert_at_endpoint_mismatch_is_defensive() {
	build_and_execute_no_post_check(|| {
		insert(1, 100, 90);
		insert(1, 200, 50);
		// `Position::endpoints_only()` on a non-empty list: `prev = next = None`
		// would rewrite both head and tail. The endpoint cross-check trips
		// defensively. In production it logs and returns `CorruptList` instead.
		let _ = list::insert_at::<Test>(&1, &300, 70, Position::endpoints_only());
	});
}

#[test]
#[should_panic = "prev neighbor rejects the resolved position"]
fn insert_at_priority_above_prev_is_defensive() {
	build_and_execute_no_post_check(|| {
		insert(1, 100, 90);
		insert(1, 200, 50);
		// Priority 100 violates `prev.priority (90) >= priority (100)`; the
		// adjacency check trips defensively.
		let _ = list::insert_at::<Test>(&1, &300, 100, Position::between(100, 200));
	});
}

#[test]
#[should_panic = "next neighbor rejects the resolved position"]
fn insert_at_priority_not_above_next_is_defensive() {
	build_and_execute_no_post_check(|| {
		insert(1, 100, 90);
		insert(1, 200, 50);
		// Priority 50 violates `priority (50) > next.priority (50)`; the
		// adjacency check trips defensively.
		let _ = list::insert_at::<Test>(&1, &300, 50, Position::between(100, 200));
	});
}

#[test]
#[should_panic = "item linked against itself"]
fn insert_at_self_link_is_defensive() {
	build_and_execute_no_post_check(|| {
		// The hint claims the new item 200 is its own predecessor.
		let _ = list::insert_at::<Test>(&1, &200, 40, Position::at_tail(200));
	});
}

#[test]
#[should_panic = "tail pointer disagrees with tail-side insert"]
fn insert_at_tail_endpoint_mismatch_is_defensive() {
	build_and_execute_no_post_check(|| {
		insert(1, 100, 90);
		insert(1, 200, 50);
		// Corrupt the tail pointer so a tail-side insert (`next = None`) past the
		// real tail (200) disagrees with `ListMetas.tail`. Twin of the head-side
		// `insert_at_endpoint_mismatch_is_defensive` case.
		ListMetas::<Test>::mutate(1, |maybe| {
			if let Some(meta) = maybe {
				meta.tail = Some(100);
			}
		});
		let _ = list::insert_at::<Test>(&1, &300, 40, Position::at_tail(200));
	});
}
