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

use crate::{mock::*, Event, ListError, ListMetas, ListNodes, SortedListInterface};
use frame::testing_prelude::{assert_ok, assert_storage_noop, hypothetically};

#[test]
fn remove_only_item_clears_head_tail_size() {
	build_and_execute(|| {
		insert(1, 100, 50);
		assert_ok!(LinkedList::remove(&1, &100));
		assert!(LinkedList::head(1).is_none());
		assert!(LinkedList::tail(1).is_none());
		assert_eq!(LinkedList::count(1), 0);
		assert!(!ListMetas::<Test>::contains_key(1));
		System::assert_has_event(Event::ItemRemoved { list_id: 1, item: 100, priority: 50 }.into());
		System::assert_last_event(Event::ListRemoved { list_id: 1 }.into());
	});
}

#[test]
fn pop_tail_emptying_list_emits_list_removed() {
	build_and_execute(|| {
		insert(1, 100, 50);
		// Popping the only item empties the list and tears down its metadata.
		assert_eq!(LinkedList::pop_tail(&1).unwrap(), Some((100, 50)));
		System::assert_has_event(Event::ItemRemoved { list_id: 1, item: 100, priority: 50 }.into());
		System::assert_last_event(Event::ListRemoved { list_id: 1 }.into());
	});
}

#[test]
fn remove_head_promotes_next() {
	build_and_execute(|| {
		insert(1, 100, 90);
		insert(1, 200, 50);
		assert_ok!(LinkedList::remove(&1, &100));
		assert_eq!(LinkedList::head(1), Some(200));
		assert_eq!(dump(1), vec![(200, 50)]);
	});
}

#[test]
fn remove_tail_promotes_prev() {
	build_and_execute(|| {
		insert(1, 100, 90);
		insert(1, 200, 50);
		assert_ok!(LinkedList::remove(&1, &200));
		assert_eq!(LinkedList::tail(1), Some(100));
		assert_eq!(dump(1), vec![(100, 90)]);
	});
}

#[test]
fn remove_middle_splices() {
	build_and_execute(|| {
		insert(1, 100, 90);
		insert(1, 150, 70);
		insert(1, 200, 50);
		assert_ok!(LinkedList::remove(&1, &150));
		assert_eq!(dump(1), vec![(100, 90), (200, 50)]);
	});
}

#[test]
fn remove_unknown_errors() {
	build_and_execute(|| {
		assert_storage_noop!(assert_eq!(
			LinkedList::remove(&1, &100),
			Err(ListError::ItemNotFound)
		));
	});
}

#[test]
fn pop_tail_empty_list_returns_none() {
	build_and_execute(|| {
		assert_eq!(LinkedList::pop_tail(&1).unwrap(), None);
	});
}

#[test]
fn pop_tail_removes_lowest_priority_tail() {
	build_and_execute(|| {
		insert(1, 100, 90);
		insert(1, 200, 50);
		insert(1, 300, 10);

		assert_eq!(LinkedList::pop_tail(&1).unwrap(), Some((300, 10)));
		assert_eq!(dump(1), vec![(100, 90), (200, 50)]);
		System::assert_last_event(
			Event::ItemRemoved { list_id: 1, item: 300, priority: 10 }.into(),
		);

		// Continuing to drain leaves the list empty and tears down all metadata.
		hypothetically!({
			assert_eq!(LinkedList::pop_tail(&1).unwrap(), Some((200, 50)));
			assert_eq!(LinkedList::pop_tail(&1).unwrap(), Some((100, 90)));
			assert!(LinkedList::head(1).is_none());
			assert!(LinkedList::tail(1).is_none());
			assert!(!ListMetas::<Test>::contains_key(1));
		});
	});
}

#[test]
fn pop_tail_is_lifo_for_same_priority_cluster() {
	build_and_execute(|| {
		insert(1, 10, 50);
		insert(1, 20, 50);
		insert(1, 30, 50);

		assert_eq!(LinkedList::pop_tail(&1).unwrap(), Some((30, 50)));
		assert_eq!(LinkedList::pop_tail(&1).unwrap(), Some((20, 50)));
		assert_eq!(dump(1), vec![(10, 50)]);
	});
}

#[test]
#[should_panic = "Defensive failure has been triggered"]
fn remove_missing_neighbor_is_defensive() {
	build_and_execute_no_post_check(|| {
		insert(1, 100, 90);
		insert(1, 200, 50);
		// Drop 100, orphaning 200's prev link.
		ListNodes::<Test>::remove(1, 100);
		let _ = LinkedList::remove(&1, &200);
	});
}

#[test]
#[should_panic = "head pointer disagrees with removed head node"]
fn remove_at_node_with_none_prev_but_not_head_is_defensive() {
	build_and_execute_no_post_check(|| {
		insert(1, 100, 90);
		insert(1, 200, 50);
		// Corrupt node 200's `prev` to `None` so it falsely claims to be the head
		// (the actual head is 100). The endpoint cross-check trips defensively.
		ListNodes::<Test>::mutate(1, 200, |maybe| {
			if let Some(n) = maybe {
				n.prev = None;
			}
		});
		let _ = LinkedList::remove(&1, &200);
	});
}

#[test]
#[should_panic = "tail pointer disagrees with removed tail node"]
fn remove_at_node_with_none_next_but_not_tail_is_defensive() {
	build_and_execute_no_post_check(|| {
		insert(1, 100, 90);
		insert(1, 200, 50);
		// Corrupt node 100's `next` to `None` so it falsely claims to be the tail
		// (the actual tail is 200). The endpoint cross-check trips defensively.
		ListNodes::<Test>::mutate(1, 100, |maybe| {
			if let Some(n) = maybe {
				n.next = None;
			}
		});
		let _ = LinkedList::remove(&1, &100);
	});
}

#[test]
#[should_panic = "node linked against itself"]
fn remove_at_self_loop_is_defensive() {
	build_and_execute_no_post_check(|| {
		insert(1, 100, 50);
		// Corrupt 100 so its `prev` names itself, forming a self-loop. The
		// anti-cycle guard trips defensively; production logs and returns
		// `CorruptList` instead.
		ListNodes::<Test>::mutate(1, 100, |maybe| {
			if let Some(node) = maybe {
				node.prev = Some(100);
			}
		});
		let _ = LinkedList::remove(&1, &100);
	});
}

#[test]
#[should_panic = "Defensive failure has been triggered"]
fn remove_at_missing_meta_row_is_defensive() {
	build_and_execute_no_post_check(|| {
		insert(1, 100, 50);
		// Drop the metadata row while the node persists: a present node without a
		// meta row violates the all-or-nothing invariant.
		ListMetas::<Test>::remove(1);
		let _ = LinkedList::remove(&1, &100);
	});
}

#[test]
#[should_panic = "Defensive failure has been triggered"]
fn remove_at_len_underflow_is_defensive() {
	build_and_execute_no_post_check(|| {
		insert(1, 100, 50);
		// Corrupt the length counter to 0 while the node and head/tail persist, so
		// the decrement underflows.
		ListMetas::<Test>::mutate(1, |maybe| {
			if let Some(meta) = maybe {
				meta.len = 0;
			}
		});
		let _ = LinkedList::remove(&1, &100);
	});
}

#[test]
#[should_panic = "Defensive failure has been triggered"]
fn pop_tail_with_missing_tail_node_is_defensive() {
	build_and_execute_no_post_check(|| {
		insert(1, 100, 50);
		// Remove the node but leave `ListMetas.tail` pointing at it.
		ListNodes::<Test>::remove(1, 100);
		let _ = LinkedList::pop_tail(&1);
	});
}
