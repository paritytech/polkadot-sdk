use crate::{
	mock::*, Error, Event, ListHeads, ListNodes, ListSizes, ListTails, SortedListInterface,
};
use frame::testing_prelude::{assert_ok, assert_storage_noop, hypothetically};

#[test]
fn insert_into_empty_list_sets_head_tail_size() {
	build_and_execute(|| {
		let steps =
			<LinkedList as SortedListInterface<_, _>>::insert(1, 100, 50, None, None).unwrap();
		assert_eq!(steps, 0);
		assert_eq!(ListHeads::<Test>::get(1), Some(100));
		assert_eq!(ListTails::<Test>::get(1), Some(100));
		assert_eq!(ListSizes::<Test>::get(1), 1);
		assert_eq!(dump(1), vec![(100, 50)]);
		System::assert_last_event(Event::ItemInserted { list_id: 1, item: 100, score: 50 }.into());

		// Re-inserting the same `(list_id, item)` rejects without touching state.
		hypothetically!({
			assert_storage_noop!(assert!(matches!(
				<LinkedList as SortedListInterface<_, _>>::insert(1, 100, 50, None, None),
				Err(Error::<Test>::ItemAlreadyExists)
			)));
		});
	});
}

#[test]
fn insert_with_valid_hints_o1() {
	build_and_execute(|| {
		insert(1, 100, 90); // head
		insert(1, 200, 50); // tail

		let steps =
			<LinkedList as SortedListInterface<_, _>>::insert(1, 150, 70, Some(100), Some(200))
				.unwrap();
		assert_eq!(steps, 0);
		assert_eq!(dump(1), vec![(100, 90), (150, 70), (200, 50)]);
	});
}

#[test]
fn insert_at_head() {
	build_and_execute(|| {
		insert(1, 100, 50);
		assert_ok!(<LinkedList as SortedListInterface<_, _>>::insert(1, 200, 90, None, Some(100)));
		assert_eq!(ListHeads::<Test>::get(1), Some(200));
		assert_eq!(dump(1), vec![(200, 90), (100, 50)]);
	});
}

#[test]
fn insert_at_tail() {
	build_and_execute(|| {
		insert(1, 100, 90);
		assert_ok!(<LinkedList as SortedListInterface<_, _>>::insert(1, 200, 10, Some(100), None));
		assert_eq!(ListTails::<Test>::get(1), Some(200));
		assert_eq!(dump(1), vec![(100, 90), (200, 10)]);
	});
}

#[test]
fn insert_same_score_lands_at_tail_side_of_cluster() {
	build_and_execute(|| {
		insert(1, 1, 50);
		insert(1, 2, 50);
		insert(1, 3, 50);
		assert_eq!(dump(1), vec![(1, 50), (2, 50), (3, 50)]);
		assert_eq!(<LinkedList as SortedListInterface<_, _>>::iter_from_tail(&1, 3), vec![3, 2, 1]);
	});
}

#[test]
fn insert_existing_item_errors() {
	build_and_execute(|| {
		insert(1, 100, 50);
		assert_storage_noop!(assert!(matches!(
			<LinkedList as SortedListInterface<_, _>>::insert(1, 100, 50, None, None),
			Err(Error::<Test>::ItemAlreadyExists)
		)));
	});
}

#[test]
fn insert_existing_item_errors_before_hint_repair() {
	build_and_execute(|| {
		// Chain length must exceed `MaxHintRepairSteps` so the hint walk would
		// otherwise be exercised; the duplicate check has to fire first.
		let chain_len = MaxHintRepairSteps::get() + 4;
		for i in 1..=chain_len {
			insert(1, u64::from(i), 100 - 10 * i + 10);
		}
		assert_storage_noop!(assert!(matches!(
			<LinkedList as SortedListInterface<_, _>>::insert(1, 1, 5, None, Some(1)),
			Err(Error::<Test>::ItemAlreadyExists)
		)));
	});
}

#[test]
fn insert_does_not_saturate_size_counter() {
	// Manually corrupts `ListSizes` to exercise the saturation guard, so we
	// skip the post-test invariant check.
	build_and_execute_no_post_check(|| {
		ListSizes::<Test>::insert(1, u32::MAX);
		assert_storage_noop!(assert!(matches!(
			<LinkedList as SortedListInterface<_, _>>::insert(1, 100, 50, None, None),
			Err(Error::<Test>::ListTooLong)
		)));
		assert!(!ListNodes::<Test>::contains_key(1, 100));
		assert!(ListHeads::<Test>::get(1).is_none());
		assert!(ListTails::<Test>::get(1).is_none());
	});
}
