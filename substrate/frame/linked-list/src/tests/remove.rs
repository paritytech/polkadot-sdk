use crate::{mock::*, Error, Event, ListHeads, ListSizes, ListTails, SortedListInterface};
use frame::testing_prelude::{assert_ok, assert_storage_noop, hypothetically};

#[test]
fn remove_only_item_clears_head_tail_size() {
	build_and_execute(|| {
		insert(1, 100, 50);
		assert_ok!(<LinkedList as SortedListInterface<_, _>>::remove(&1, &100));
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
		assert_ok!(<LinkedList as SortedListInterface<_, _>>::remove(&1, &100));
		assert_eq!(ListHeads::<Test>::get(1), Some(200));
		assert_eq!(dump(1), vec![(200, 50)]);
	});
}

#[test]
fn remove_tail_promotes_prev() {
	build_and_execute(|| {
		insert(1, 100, 90);
		insert(1, 200, 50);
		assert_ok!(<LinkedList as SortedListInterface<_, _>>::remove(&1, &200));
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
		assert_ok!(<LinkedList as SortedListInterface<_, _>>::remove(&1, &150));
		assert_eq!(dump(1), vec![(100, 90), (200, 50)]);
	});
}

#[test]
fn remove_unknown_errors() {
	build_and_execute(|| {
		assert_storage_noop!(assert!(matches!(
			<LinkedList as SortedListInterface<_, _>>::remove(&1, &100),
			Err(Error::<Test>::ItemNotFound)
		)));
	});
}

#[test]
fn pop_tail_empty_list_returns_none() {
	build_and_execute(|| {
		assert_eq!(<LinkedList as SortedListInterface<_, _>>::pop_tail(&1).unwrap(), None);
	});
}

#[test]
fn pop_tail_removes_lowest_priority_tail() {
	build_and_execute(|| {
		insert(1, 100, 90);
		insert(1, 200, 50);
		insert(1, 300, 10);

		assert_eq!(
			<LinkedList as SortedListInterface<_, _>>::pop_tail(&1).unwrap(),
			Some((300, 10))
		);
		assert_eq!(dump(1), vec![(100, 90), (200, 50)]);
		System::assert_last_event(Event::ItemRemoved { list_id: 1, item: 300 }.into());

		// Continuing to drain leaves the list empty and tears down all metadata.
		hypothetically!({
			assert_eq!(
				<LinkedList as SortedListInterface<_, _>>::pop_tail(&1).unwrap(),
				Some((200, 50))
			);
			assert_eq!(
				<LinkedList as SortedListInterface<_, _>>::pop_tail(&1).unwrap(),
				Some((100, 90))
			);
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

		assert_eq!(
			<LinkedList as SortedListInterface<_, _>>::pop_tail(&1).unwrap(),
			Some((30, 50))
		);
		assert_eq!(
			<LinkedList as SortedListInterface<_, _>>::pop_tail(&1).unwrap(),
			Some((20, 50))
		);
		assert_eq!(dump(1), vec![(10, 50)]);
	});
}
