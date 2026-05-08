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
