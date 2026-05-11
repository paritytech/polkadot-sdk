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

use crate::{mock::*, ListHeads, ListSizes, ListTails, SortedListInterface};
use frame::testing_prelude::assert_ok;

#[test]
fn lists_are_independent_size_head_tail() {
	build_and_execute(|| {
		insert(1, 100, 50);
		insert(2, 100, 70);
		insert(2, 200, 30);

		assert_eq!(ListSizes::<Test>::get(1), 1);
		assert_eq!(ListSizes::<Test>::get(2), 2);
		assert_eq!(ListHeads::<Test>::get(1), Some(100));
		assert_eq!(ListHeads::<Test>::get(2), Some(100));
		assert_eq!(ListTails::<Test>::get(2), Some(200));
	});
}

#[test]
fn same_item_id_in_two_lists_does_not_collide() {
	build_and_execute(|| {
		insert(1, 100, 50);
		insert(2, 100, 999); // same ItemId, different list.
		assert_eq!(dump(1), vec![(100, 50)]);
		assert_eq!(dump(2), vec![(100, 999)]);
	});
}

#[test]
fn removing_from_one_list_leaves_other_intact() {
	build_and_execute(|| {
		insert(1, 100, 50);
		insert(2, 100, 70);
		assert_ok!(<LinkedList as SortedListInterface<_, _>>::remove(&1, &100));
		assert_eq!(dump(1), vec![]);
		assert_eq!(dump(2), vec![(100, 70)]);
	});
}
