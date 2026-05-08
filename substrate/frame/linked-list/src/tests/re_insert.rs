use crate::{mock::*, Error, Event, SortedListInterface};
use frame::testing_prelude::{assert_ok, assert_storage_noop};

#[test]
fn re_insert_unchanged_score_no_op() {
	build_and_execute(|| {
		insert(1, 100, 50);
		let steps =
			<LinkedList as SortedListInterface<_, _>>::re_insert(1, 100, 50, None, None).unwrap();
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
		let steps =
			<LinkedList as SortedListInterface<_, _>>::re_insert(1, 200, 30, None, None).unwrap();
		assert_eq!(steps, 0);
		assert_eq!(dump(1), vec![(100, 90), (200, 30), (300, 10)]);
		System::assert_has_event(
			Event::ItemReinserted { list_id: 1, item: 200, old_score: 50, new_score: 30 }.into(),
		);
	});
}

#[test]
fn re_insert_score_increase_moves_toward_head() {
	build_and_execute(|| {
		insert(1, 1, 90);
		insert(1, 2, 50);
		insert(1, 3, 10);
		let (prev, next) =
			<LinkedList as SortedListInterface<_, _>>::find_re_insert_position(&1, &3, 95).unwrap();
		assert_ok!(<LinkedList as SortedListInterface<_, _>>::re_insert(1, 3, 95, prev, next));
		assert_eq!(dump(1), vec![(3, 95), (1, 90), (2, 50)]);
	});
}

#[test]
fn re_insert_score_decrease_moves_toward_tail() {
	build_and_execute(|| {
		insert(1, 1, 90);
		insert(1, 2, 50);
		insert(1, 3, 10);
		let (prev, next) =
			<LinkedList as SortedListInterface<_, _>>::find_re_insert_position(&1, &1, 5).unwrap();
		assert_ok!(<LinkedList as SortedListInterface<_, _>>::re_insert(1, 1, 5, prev, next));
		assert_eq!(dump(1), vec![(2, 50), (3, 10), (1, 5)]);
	});
}

#[test]
fn re_insert_unknown_errors() {
	build_and_execute(|| {
		assert_storage_noop!(assert!(matches!(
			<LinkedList as SortedListInterface<_, _>>::re_insert(1, 100, 50, None, None),
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
		// Re-insert item 1 at score 5 (tail-ward) but supply head hints; the
		// repair walk distance exceeds budget, so re_insert errors. The item
		// must still be in the list at its old position.
		assert_storage_noop!(assert!(matches!(
			<LinkedList as SortedListInterface<_, _>>::re_insert(1, 1, 5, None, Some(1)),
			Err(Error::<Test>::InvalidPositionHints)
		)));
	});
}
