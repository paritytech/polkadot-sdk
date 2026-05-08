use crate::{mock::*, SortedListInterface};

#[test]
fn find_position_empty_list_returns_none_none() {
	build_and_execute(|| {
		assert_eq!(<LinkedList as SortedListInterface<_, _>>::find_position(&1, 50), (None, None));
	});
}

#[test]
fn find_position_above_all_returns_none_some_head() {
	build_and_execute(|| {
		insert(1, 1, 90);
		insert(1, 2, 50);
		// score 100 is greater than head (90), so prev=None, next=head=1.
		assert_eq!(
			<LinkedList as SortedListInterface<_, _>>::find_position(&1, 100),
			(None, Some(1))
		);
	});
}

#[test]
fn find_position_below_all_returns_some_tail_none() {
	build_and_execute(|| {
		insert(1, 1, 90);
		insert(1, 2, 50);
		// score 5 is less than tail (50), so prev=tail=2, next=None.
		assert_eq!(
			<LinkedList as SortedListInterface<_, _>>::find_position(&1, 5),
			(Some(2), None)
		);
	});
}

#[test]
fn find_position_middle() {
	build_and_execute(|| {
		insert(1, 1, 90);
		insert(1, 2, 50);
		insert(1, 3, 10);
		assert_eq!(
			<LinkedList as SortedListInterface<_, _>>::find_position(&1, 70),
			(Some(1), Some(2))
		);
	});
}

#[test]
fn find_position_same_score_lands_at_tail_side() {
	build_and_execute(|| {
		insert(1, 1, 90);
		insert(1, 2, 50);
		insert(1, 3, 10);
		// score 50 == 2's score. Asymmetric rule: prev.score >= 50 (1 has 90, 2
		// has 50; both qualify), but next.score < 50 (3 has 10). Walking from
		// the head, we step past 1, then past 2 (since 50 > 50 is false → keep
		// walking), and stop at 3.
		assert_eq!(
			<LinkedList as SortedListInterface<_, _>>::find_position(&1, 50),
			(Some(2), Some(3))
		);
	});
}

#[test]
fn find_re_insert_position_treats_item_as_logically_removed() {
	build_and_execute(|| {
		insert(1, 1, 90);
		insert(1, 2, 50);
		insert(1, 3, 10);
		// Re-insert 2 (currently 50) at score 95: should be at head, prev=None,
		// next=1. The algorithm must skip 2 itself.
		assert_eq!(
			<LinkedList as SortedListInterface<_, _>>::find_re_insert_position(&1, &2, 95),
			Some((None, Some(1)))
		);
	});
}

#[test]
fn neighbors_returns_none_for_unknown_item() {
	build_and_execute(|| {
		insert(1, 1, 90);
		assert_eq!(<LinkedList as SortedListInterface<_, _>>::neighbors(&1, &999), None);
	});
}

#[test]
fn neighbors_returns_links_for_known_item() {
	build_and_execute(|| {
		insert(1, 1, 90);
		insert(1, 2, 50);
		insert(1, 3, 10);
		assert_eq!(
			<LinkedList as SortedListInterface<_, _>>::neighbors(&1, &2),
			Some((Some(1), Some(3)))
		);
	});
}

#[test]
fn score_returns_none_for_unknown_item() {
	build_and_execute(|| {
		insert(1, 1, 90);
		assert_eq!(<LinkedList as SortedListInterface<_, _>>::score(&1, &999), None);
	});
}

#[test]
fn score_returns_stored_score_for_known_item() {
	build_and_execute(|| {
		insert(1, 1, 90);
		insert(1, 2, 50);
		assert_eq!(<LinkedList as SortedListInterface<_, _>>::score(&1, &1), Some(90));
		assert_eq!(<LinkedList as SortedListInterface<_, _>>::score(&1, &2), Some(50));
	});
}
