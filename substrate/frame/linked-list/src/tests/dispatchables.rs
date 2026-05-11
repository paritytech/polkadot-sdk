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

use crate::{mock::*, Error, Event};
use frame::testing_prelude::{assert_noop, assert_ok};

#[test]
fn reprioritize_no_op_when_priority_unchanged() {
	build_and_execute(|| {
		insert(1, 100, 50);
		set_real_priority(1, 100, 50);
		assert_ok!(LinkedList::reprioritize(RuntimeOrigin::signed(1), 1, 100, None, None));
		assert_eq!(dump(1), vec![(100, 50)]);
	});
}

#[test]
fn reprioritize_repositions_when_priority_changes() {
	build_and_execute(|| {
		insert(1, 1, 90);
		insert(1, 2, 50);
		insert(1, 3, 10);
		// Real priority for item 2 just rose to 99; reprioritize should move it to head.
		set_real_priority(1, 2, 99);
		// Hint: target's new neighbors (None, Some(1)): head insertion.
		assert_ok!(LinkedList::reprioritize(RuntimeOrigin::signed(1), 1, 2, None, Some(1)));
		assert_eq!(dump(1), vec![(2, 99), (1, 90), (3, 10)]);
		System::assert_has_event(
			Event::Reprioritized { list_id: 1, item: 2, new_priority: 99 }.into(),
		);
	});
}

#[test]
fn reprioritize_unknown_item_errors() {
	build_and_execute(|| {
		// No priority in StaticPriorities → PriorityProvider returns None.
		assert_noop!(
			LinkedList::reprioritize(RuntimeOrigin::signed(1), 1, 100, None, None),
			Error::<Test>::ItemNotFound
		);
	});
}

#[test]
fn reprioritize_removes_existing_item_when_priority_disappears() {
	build_and_execute(|| {
		insert(1, 1, 90);
		insert(1, 2, 50);
		insert(1, 3, 10);

		assert_ok!(LinkedList::reprioritize(RuntimeOrigin::signed(1), 1, 2, None, None));

		assert_eq!(dump(1), vec![(1, 90), (3, 10)]);
		System::assert_has_event(Event::ItemRemoved { list_id: 1, item: 2 }.into());
	});
}

#[test]
fn reprioritize_with_stale_hint_within_budget_succeeds() {
	build_and_execute(|| {
		insert(1, 1, 90);
		insert(1, 2, 50);
		insert(1, 3, 10);
		// Real priority for item 2 just rose to 99; the caller's hint is stale
		// (tail region) but the correct head position is within budget.
		set_real_priority(1, 2, 99);
		assert_ok!(LinkedList::reprioritize(RuntimeOrigin::signed(1), 1, 2, Some(3), None));
		assert_eq!(dump(1), vec![(2, 99), (1, 90), (3, 10)]);
	});
}

#[test]
fn reprioritize_with_hint_beyond_budget_errors() {
	build_and_execute(|| {
		// Build a chain longer than `MaxHintRepairSteps` so that a wrong-end
		// hint cannot reach the correct position.
		let chain_len = MaxHintRepairSteps::get() + 4;
		for i in 1..=chain_len {
			insert(1, u64::from(i), 100u32 - 10 * i + 10);
		}
		// Tail item drifts up to 200; correct position is the head, but the
		// supplied hint is at the tail and the budget cannot bridge that gap.
		let tail = u64::from(chain_len);
		set_real_priority(1, tail, 200);
		assert_noop!(
			LinkedList::reprioritize(RuntimeOrigin::signed(1), 1, tail, Some(tail - 1), None),
			Error::<Test>::InvalidPositionHints
		);
	});
}
