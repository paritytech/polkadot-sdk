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

use crate::{mock::*, Error, Event, ListHeads, ListSizes, ListTails, SortedListInterface};
use frame::testing_prelude::{assert_ok, assert_storage_noop, hypothetically};

#[test]
fn remove_only_item_clears_head_tail_size() {
	build_and_execute(|| {
		insert(1, 100, 50);
		assert_ok!(LinkedList::remove(&1, &100));
		assert!(ListHeads::<Test>::get(1).is_none());
		assert!(ListTails::<Test>::get(1).is_none());
		assert_eq!(ListSizes::<Test>::get(1), 0);
		assert!(!ListSizes::<Test>::contains_key(1));
		System::assert_last_event(Event::ItemRemoved { list_id: 1, item: 100 }.into());
	});
}

#[test]
fn remove_head_promotes_next() {
	build_and_execute(|| {
		insert(1, 100, 90);
		insert(1, 200, 50);
		assert_ok!(LinkedList::remove(&1, &100));
		assert_eq!(ListHeads::<Test>::get(1), Some(200));
		assert_eq!(dump(1), vec![(200, 50)]);
	});
}

#[test]
fn remove_tail_promotes_prev() {
	build_and_execute(|| {
		insert(1, 100, 90);
		insert(1, 200, 50);
		assert_ok!(LinkedList::remove(&1, &200));
		assert_eq!(ListTails::<Test>::get(1), Some(100));
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
		assert_storage_noop!(assert!(matches!(
			LinkedList::remove(&1, &100),
			Err(Error::<Test>::ItemNotFound)
		)));
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
		System::assert_last_event(Event::ItemRemoved { list_id: 1, item: 300 }.into());

		// Continuing to drain leaves the list empty and tears down all metadata.
		hypothetically!({
			assert_eq!(LinkedList::pop_tail(&1).unwrap(), Some((200, 50)));
			assert_eq!(LinkedList::pop_tail(&1).unwrap(), Some((100, 90)));
			assert!(ListHeads::<Test>::get(1).is_none());
			assert!(ListTails::<Test>::get(1).is_none());
			assert!(!ListSizes::<Test>::contains_key(1));
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
