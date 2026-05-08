//! `try_state` invariant tests. The pallet's `try_state` is exercised
//! continuously by `build_and_execute`'s post-test check; these tests
//! additionally prove that deliberate corruption is detected.

#![cfg(feature = "try-runtime")]

use crate::{mock::*, ListHeads, ListNodes, ListSizes, ListTails, Node, SortedListInterface};

#[test]
fn consistent_after_random_op_sequence() {
	build_and_execute(|| {
		// Mix of inserts/removes/re_inserts; the post-test `do_try_state` runs
		// at the end via `build_and_execute`.
		insert(1, 1, 90);
		insert(1, 2, 50);
		insert(1, 3, 70);
		insert(1, 4, 30);
		insert(2, 1, 100);
		<LinkedList>::do_try_state().unwrap();

		<LinkedList as SortedListInterface<_, _>>::remove(&1, &3).unwrap();
		<LinkedList as SortedListInterface<_, _>>::re_insert(1, 2, 5, None, None).unwrap();
	});
}

#[test]
fn corrupt_size_detected() {
	build_and_execute_no_post_check(|| {
		insert(1, 1, 50);
		insert(1, 2, 30);
		// Force the count to lie.
		ListSizes::<Test>::insert(1, 99u32);
		assert!(<LinkedList>::do_try_state().is_err());
	});
}

#[test]
fn corrupt_head_tail_mismatch_detected() {
	build_and_execute_no_post_check(|| {
		insert(1, 1, 50);
		// Tail set without head.
		ListHeads::<Test>::remove(1);
		assert!(<LinkedList>::do_try_state().is_err());
	});
}

#[test]
fn corrupt_link_cycle_detected() {
	build_and_execute_no_post_check(|| {
		insert(1, 1, 50);
		insert(1, 2, 30);
		// Forge a cycle: 2.next = 1.
		ListNodes::<Test>::mutate(1, 2, |maybe| {
			if let Some(n) = maybe {
				n.next = Some(1);
			}
		});
		// Patch the size and tail to match; the walk should still detect the
		// cycle by exceeding the visited-count cap.
		ListSizes::<Test>::insert(1, 99);
		ListTails::<Test>::insert(1, 1);
		assert!(<LinkedList>::do_try_state().is_err());
	});
}

#[test]
fn empty_list_with_stale_size_detected() {
	build_and_execute_no_post_check(|| {
		ListSizes::<Test>::insert(1, 5);
		assert!(<LinkedList>::do_try_state().is_err());
	});
}

#[test]
fn dangling_node_pointer_detected() {
	build_and_execute_no_post_check(|| {
		insert(1, 1, 50);
		insert(1, 2, 30);
		// 1.next points to a non-existent item.
		ListNodes::<Test>::insert(1, 1, Node { prev: None, next: Some(999u64), score: 50u32 });
		assert!(<LinkedList>::do_try_state().is_err());
	});
}

#[test]
fn orphan_unreachable_node_detected() {
	build_and_execute_no_post_check(|| {
		insert(1, 1, 50);
		insert(1, 2, 30);
		// Add a row not on the head→tail chain. The forward/reverse walks
		// still see exactly the 2 reachable nodes and `ListSizes` agrees, so
		// only the total-node-count check catches this.
		ListNodes::<Test>::insert(1, 999u64, Node { prev: None, next: None, score: 100u32 });
		assert!(<LinkedList>::do_try_state().is_err());
	});
}
