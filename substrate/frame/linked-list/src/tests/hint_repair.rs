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

use crate::{mock::*, Error, Position, SortedListInterface};
use frame::testing_prelude::assert_storage_noop;

fn build_chain(priorities: &[(ItemId, Priority)]) {
	for &(item, priority) in priorities {
		insert(1, item, priority);
	}
}

/// Items in head→tail priority order, length deliberately greater than
/// `MaxHintRepairSteps` so that "wrong-end hint" tests cannot be bridged within
/// the budget.
fn build_long_chain() {
	for i in 1..=(MaxHintRepairSteps::get() + 4) {
		insert(1, u64::from(i), 100 - 10 * i + 10); // 100, 90, 80, ..., 30 when budget=4
	}
}

#[test]
fn stale_hint_within_budget_repairs_succeeds() {
	build_and_execute(|| {
		// MaxHintRepairSteps = 4 (set in mock).
		// 5 items; correct insert pos for priority=15 is between 4 (priority 20) and 5 (priority
		// 10).
		build_chain(&[(1, 100), (2, 80), (3, 50), (4, 20), (5, 10)]);

		// Stale hint: caller thinks pos is between 1 and 2 (head region).
		// Real pos is between 4 and 5: 3 nodes away, within the 4-step budget.
		let steps = LinkedList::insert(1, 100, 15, Position::between(1, 2))
			.expect("repair walks within budget");
		assert!(steps > 0 && steps <= MaxHintRepairSteps::get());
		assert_eq!(dump(1), vec![(1, 100), (2, 80), (3, 50), (4, 20), (100, 15), (5, 10)]);
	});
}

#[test]
fn stale_hint_beyond_budget_returns_invalid_hints() {
	build_and_execute(|| {
		// Long chain so that priority=5 (correct pos = tail) is unreachable from a
		// head-region hint within `MaxHintRepairSteps`.
		build_long_chain();
		assert_storage_noop!(assert!(matches!(
			LinkedList::insert(1, 99, 5, Position::at_head(1)),
			Err(Error::<Test>::InvalidPositionHints)
		)));
	});
}

#[test]
fn repair_steps_needed_zero_for_valid_hint() {
	build_and_execute(|| {
		build_chain(&[(1, 90), (2, 70), (3, 50)]);
		assert_eq!(
			<LinkedList as SortedListInterface<_, _>>::repair_steps_needed(
				&1,
				80,
				Position::between(1, 2),
			),
			0
		);
	});
}

#[test]
fn repair_steps_needed_positive_for_stale_hint() {
	build_and_execute(|| {
		build_chain(&[(1, 90), (2, 70), (3, 50)]);
		let n = <LinkedList as SortedListInterface<_, _>>::repair_steps_needed(
			&1,
			60,
			Position::at_head(1),
		);
		assert!(n > 0);
	});
}

/// `repair_steps_needed` now reflects the *actual* `walk_repair` cost,
/// including dangling-hint clamps that a head-positional approximation
/// missed. A hint pointing at a deleted item should report a non-zero step
/// count.
#[test]
fn repair_steps_needed_counts_dangling_hint_clamp() {
	build_and_execute(|| {
		build_chain(&[(1, 90), (2, 70), (3, 50)]);
		// `Some(999)` is dangling. Even though priority 80 is positionally between
		// (1, 2), `walk_repair` must spend at least one step clamping the
		// invalid `next` before it can land on the correct position.
		let n = <LinkedList as SortedListInterface<_, _>>::repair_steps_needed(
			&1,
			80,
			Position::between(1, 999),
		);
		assert!(n > 0);
	});
}

#[test]
fn repair_steps_needed_exceeds_budget_signals_infeasible() {
	build_and_execute(|| {
		build_long_chain();
		// priority=5 is tail; hint claims head. Distance > `MaxHintRepairSteps`.
		let n = <LinkedList as SortedListInterface<_, _>>::repair_steps_needed(
			&1,
			5,
			Position::at_head(1),
		);
		assert!(n > MaxHintRepairSteps::get());
	});
}

#[test]
fn strict_mode_zero_budget_accepts_valid_hint_rejects_invalid() {
	build_and_execute(|| {
		MaxHintRepairSteps::set(0);
		// An empty-list insert with the empty-position hint is already valid.
		let steps = LinkedList::insert(1, 100, 90, Position::endpoints_only()).expect("valid hint");
		assert_eq!(steps, 0);

		// A second insert at the tail with a perfect hint is also 0 steps.
		let steps = LinkedList::insert(1, 200, 50, Position::at_tail(100)).expect("valid hint");
		assert_eq!(steps, 0);

		// Any stale hint must fail immediately — no walk loop runs at budget 0.
		assert_storage_noop!(assert!(matches!(
			LinkedList::insert(1, 300, 70, Position::at_head(100)),
			Err(Error::<Test>::InvalidPositionHints)
		)));
	});
}

#[test]
fn inconsistent_hint_one_side_stale_re_anchors() {
	build_and_execute(|| {
		// List head→tail: items 1..=N with priorities 100, 90, 80, ..., chosen so that
		// the correct insert position for priority 85 is between item 2 (90) and
		// item 3 (80). The caller's hint claims (tail=last, Some(item 3)),
		// which is link-inconsistent; the tail is not item 3's prev.
		// Walking from prev=tail toward the head would need more than
		// `MaxHintRepairSteps` steps. Re-anchoring via `next.prev` lands at
		// (item 2, item 3) in 1 step.
		build_long_chain();
		let chain_len = MaxHintRepairSteps::get() + 4;
		let stale_prev = u64::from(chain_len); // tail item id
		let steps = LinkedList::insert(1, 99, 85, Position::between(stale_prev, 3))
			.expect("re-anchor handles partially-stale hint within budget");
		assert!(steps > 0 && steps <= MaxHintRepairSteps::get());
		// Head→tail: (1,100), (2,90), the inserted (99,85), then the unchanged
		// tail-side suffix (3,80), (4,70), …
		let mut expected = vec![(1u64, 100u32), (2, 90), (99, 85)];
		expected.extend((3..=chain_len).map(|i| (u64::from(i), 100u32 - 10 * i + 10)));
		assert_eq!(dump(1), expected);
	});
}
