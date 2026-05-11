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

use crate::{mock::*, Error, Event, Position, SortedListInterface};
use frame::testing_prelude::{assert_ok, assert_storage_noop};

#[test]
fn re_insert_unchanged_priority_no_op() {
	build_and_execute(|| {
		insert(1, 100, 50);
		let steps = LinkedList::re_insert(1, 100, 50, Position::endpoints_only()).unwrap();
		assert_eq!(steps, 0);
		assert_eq!(dump(1), vec![(100, 50)]);
	});
}

#[test]
fn re_insert_in_place_when_position_still_valid() {
	build_and_execute(|| {
		insert(1, 100, 90);
		insert(1, 200, 50);
		insert(1, 300, 10);
		// Drop 200 from 50 → 30: still strictly less than 100 (90) and strictly
		// greater than 300 (10). Position-validity check passes; in-place update.
		let steps = LinkedList::re_insert(1, 200, 30, Position::endpoints_only()).unwrap();
		assert_eq!(steps, 0);
		assert_eq!(dump(1), vec![(100, 90), (200, 30), (300, 10)]);
		System::assert_has_event(
			Event::ItemReinserted { list_id: 1, item: 200, old_priority: 50, new_priority: 30 }
				.into(),
		);
	});
}

#[test]
fn re_insert_priority_increase_moves_toward_head() {
	build_and_execute(|| {
		insert(1, 1, 90);
		insert(1, 2, 50);
		insert(1, 3, 10);
		let hint =
			<LinkedList as SortedListInterface<_, _>>::find_re_insert_position(&1, &3, 95).unwrap();
		assert_ok!(LinkedList::re_insert(1, 3, 95, hint));
		assert_eq!(dump(1), vec![(3, 95), (1, 90), (2, 50)]);
	});
}

#[test]
fn re_insert_priority_decrease_moves_toward_tail() {
	build_and_execute(|| {
		insert(1, 1, 90);
		insert(1, 2, 50);
		insert(1, 3, 10);
		let hint =
			<LinkedList as SortedListInterface<_, _>>::find_re_insert_position(&1, &1, 5).unwrap();
		assert_ok!(LinkedList::re_insert(1, 1, 5, hint));
		assert_eq!(dump(1), vec![(2, 50), (3, 10), (1, 5)]);
	});
}

#[test]
fn re_insert_unknown_errors() {
	build_and_execute(|| {
		assert_storage_noop!(assert!(matches!(
			LinkedList::re_insert(1, 100, 50, Position::endpoints_only()),
			Err(Error::<Test>::ItemNotFound)
		)));
	});
}

/// Slow-path atomicity: when `walk_repair` exceeds the budget, the prior
/// `remove_at` must roll back so the item is still present after the failed
/// `re_insert`. This is the regression guard for the `with_transaction_opaque_err`
/// wrap.
#[test]
fn re_insert_slow_path_failure_leaves_storage_untouched() {
	build_and_execute(|| {
		// Build a chain longer than `MaxHintRepairSteps`.
		let chain_len = MaxHintRepairSteps::get() + 4;
		for i in 1..=chain_len {
			insert(1, u64::from(i), 100 - 10 * i + 10);
		}
		// Re-insert item 1 at priority 5 (tail-ward) but supply head hints; the
		// repair walk distance exceeds budget, so re_insert errors. The item
		// must still be in the list at its old position.
		assert_storage_noop!(assert!(matches!(
			LinkedList::re_insert(1, 1, 5, Position::at_head(1)),
			Err(Error::<Test>::InvalidPositionHints)
		)));
	});
}
