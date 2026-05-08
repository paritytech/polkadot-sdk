use crate::{mock::*, Error, Event};
use frame::testing_prelude::{assert_noop, assert_ok};

#[test]
fn relist_no_op_when_score_unchanged() {
	build_and_execute(|| {
		insert(1, 100, 50);
		set_real_score(1, 100, 50);
		assert_ok!(LinkedList::relist(RuntimeOrigin::signed(1), 1, 100, None, None));
		assert_eq!(dump(1), vec![(100, 50)]);
	});
}

#[test]
fn relist_repositions_when_score_changes() {
	build_and_execute(|| {
		insert(1, 1, 90);
		insert(1, 2, 50);
		insert(1, 3, 10);
		// Real score for item 2 just rose to 99; relist should move it to head.
		set_real_score(1, 2, 99);
		// Hint: target's new neighbors (None, Some(1)): head insertion.
		assert_ok!(LinkedList::relist(RuntimeOrigin::signed(1), 1, 2, None, Some(1)));
		assert_eq!(dump(1), vec![(2, 99), (1, 90), (3, 10)]);
		System::assert_has_event(Event::Relisted { list_id: 1, item: 2, new_score: 99 }.into());
	});
}

#[test]
fn relist_unknown_item_errors() {
	build_and_execute(|| {
		// No score in StaticScores → ScoreProvider returns None.
		assert_noop!(
			LinkedList::relist(RuntimeOrigin::signed(1), 1, 100, None, None),
			Error::<Test>::ItemNotFound
		);
	});
}

#[test]
fn relist_removes_existing_item_when_score_disappears() {
	build_and_execute(|| {
		insert(1, 1, 90);
		insert(1, 2, 50);
		insert(1, 3, 10);

		assert_ok!(LinkedList::relist(RuntimeOrigin::signed(1), 1, 2, None, None));

		assert_eq!(dump(1), vec![(1, 90), (3, 10)]);
		System::assert_has_event(Event::ItemRemoved { list_id: 1, item: 2 }.into());
	});
}

#[test]
fn relist_with_stale_hint_within_budget_succeeds() {
	build_and_execute(|| {
		insert(1, 1, 90);
		insert(1, 2, 50);
		insert(1, 3, 10);
		// Real score for item 2 just rose to 99; the caller's hint is stale
		// (tail region) but the correct head position is within budget.
		set_real_score(1, 2, 99);
		assert_ok!(LinkedList::relist(RuntimeOrigin::signed(1), 1, 2, Some(3), None));
		assert_eq!(dump(1), vec![(2, 99), (1, 90), (3, 10)]);
	});
}

#[test]
fn relist_with_hint_beyond_budget_errors() {
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
		set_real_score(1, tail, 200);
		assert_noop!(
			LinkedList::relist(RuntimeOrigin::signed(1), 1, tail, Some(tail - 1), None),
			Error::<Test>::InvalidPositionHints
		);
	});
}
