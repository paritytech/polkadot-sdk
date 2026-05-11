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

use crate::{mock::*, SortedListInterface};

#[test]
fn iter_from_tail_returns_lowest_first() {
	build_and_execute(|| {
		insert(1, 1, 90);
		insert(1, 2, 50);
		insert(1, 3, 10);
		assert_eq!(<LinkedList as SortedListInterface<_, _>>::iter_from_tail(&1, 5), vec![3, 2, 1]);
	});
}

#[test]
fn iter_from_tail_n_zero_returns_empty() {
	build_and_execute(|| {
		insert(1, 1, 90);
		insert(1, 2, 50);
		assert!(<LinkedList as SortedListInterface<_, _>>::iter_from_tail(&1, 0).is_empty());
	});
}

#[test]
fn iter_from_tail_n_greater_than_size_returns_all() {
	build_and_execute(|| {
		insert(1, 1, 90);
		insert(1, 2, 50);
		assert_eq!(<LinkedList as SortedListInterface<_, _>>::iter_from_tail(&1, 99), vec![2, 1]);
	});
}

#[test]
fn iter_from_tail_respects_same_priority_lifo() {
	build_and_execute(|| {
		// Three same-priority items; iter from tail returns LIFO.
		insert(1, 10, 50);
		insert(1, 20, 50);
		insert(1, 30, 50);
		assert_eq!(
			<LinkedList as SortedListInterface<_, _>>::iter_from_tail(&1, 3),
			vec![30, 20, 10]
		);
	});
}

#[test]
fn iter_from_tail_empty_list_returns_empty() {
	build_and_execute(|| {
		assert!(<LinkedList as SortedListInterface<_, _>>::iter_from_tail(&1, 5).is_empty());
	});
}
