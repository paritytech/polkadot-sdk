//! Rate-index (`VaultListId::Rate`) ordering: insertion, removal, and
//! re-insertion of vaults keyed by their annual borrow rate.

use crate::{mock::*, tests::rate_pct};
use frame::deps::frame_support::assert_ok;
use pallet_linked_list::SortedListInterface;

const ONE_DAY_MS: Moment = 24 * 3_600 * 1_000;

// Open vaults in arbitrary order; walking the rate index tail-first (lowest
// rate → highest) yields ascending order.
#[test]
fn open_orders_dll_by_annual_interest_rate() {
	build_and_execute(|| {
		register_default_branch();
		// Open in scrambled order with distinct rates.
		for (who, pct) in [(3u64, 30), (1, 5), (5, 50), (2, 10), (4, 40)] {
			assert_ok!(open(who, DOT, 1_000, 500, rate_pct(pct, 100)));
		}
		// Tail-first walk gives ascending rate. Expect [1, 2, 3, 4, 5].
		let order = <LinkedList as SortedListInterface<VaultList, u64>>::iter_from_tail(
			&rate_list(DOT),
			10,
		);
		assert_eq!(order, alloc::vec![1, 2, 3, 4, 5]);
	});
}

// `find_rate_position` returns valid neighbors for any new score.
#[test]
fn find_rate_position_returns_valid_neighbors() {
	build_and_execute(|| {
		register_default_branch();
		// Vaults at 5%, 10%, 20%, 30%, 50%.
		for (who, pct) in [(1u64, 5), (2, 10), (3, 20), (4, 30), (5, 50)] {
			assert_ok!(open(who, DOT, 1_000, 500, rate_pct(pct, 100)));
		}
		// Insert position for 15% should be between 10% (acct 2) and 20%
		// (acct 3). The DLL stores low-at-tail; "prev" walking head-first is
		// higher-score, "next" is lower-score — so prev=acct 3, next=acct 2.
		let pos = crate::Pallet::<Test>::find_rate_position(DOT, rate_pct(15, 100));
		assert_eq!(pos.prev, Some(3));
		assert_eq!(pos.next, Some(2));

		// Position for 0.001% — lower than the lowest, so next = None
		// (we'd be inserted at the very tail).
		let pos = crate::Pallet::<Test>::find_rate_position(DOT, rate_pct(1, 100_000));
		assert_eq!(pos.next, None);

		// Position for 100% — higher than the highest, prev = None.
		let pos = crate::Pallet::<Test>::find_rate_position(DOT, rate_pct(100, 100));
		assert_eq!(pos.prev, None);
	});
}

// Closing a vault removes it from the rate index.
#[test]
fn close_vault_removes_from_rate_index() {
	build_and_execute(|| {
		register_default_branch();
		for (who, pct) in [(1u64, 5), (2, 10), (3, 20), (4, 30), (5, 50)] {
			assert_ok!(open(who, DOT, 1_000, 500, rate_pct(pct, 100)));
		}
		// Repay vault 3 fully (top up from acct 4) then close it.
		let v = crate::pallet::Vaults::<Test>::get(DOT, 3).unwrap();
		let total = v.debt.principal + v.debt.interest;
		let _ = <Pusd as frame::deps::frame_support::traits::fungible::Mutate<u64>>::transfer(
			&4,
			&3,
			v.debt.interest,
			frame::deps::frame_support::traits::tokens::Preservation::Expendable,
		);
		// repay-to-zero auto-closes, which also removes the vault from the
		// rate index.
		assert_ok!(crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(3), 3, DOT, total));
		assert!(!<LinkedList as SortedListInterface<VaultList, u64>>::contains(
			&rate_list(DOT),
			&3
		));
		// Order without acct 3: [1, 2, 4, 5].
		let order = <LinkedList as SortedListInterface<VaultList, u64>>::iter_from_tail(
			&rate_list(DOT),
			10,
		);
		assert_eq!(order, alloc::vec![1, 2, 4, 5]);
	});
}

// `change_rate` re-inserts the vault at its new rate position. Walk through
// several adjustments and assert the final ordering matches the expected
// ascending-by-rate sequence.
#[test]
fn change_rate_re_inserts_in_correct_position() {
	build_and_execute(|| {
		register_default_branch();
		for (who, pct) in [(1u64, 10), (2, 20), (3, 30), (4, 40), (5, 50)] {
			assert_ok!(open(who, DOT, 1_000, 500, rate_pct(pct, 100)));
		}
		advance_time(2 * ONE_DAY_MS);

		// Move acct 3 from 30% to 5% — should land at the tail.
		assert_ok!(crate::Pallet::<Test>::change_rate(
			RuntimeOrigin::signed(3),
			DOT,
			rate_pct(5, 100),
			Position::endpoints_only(),
		));
		// Move acct 1 from 10% to 60% — should land at the head.
		assert_ok!(crate::Pallet::<Test>::change_rate(
			RuntimeOrigin::signed(1),
			DOT,
			rate_pct(60, 100),
			Position::endpoints_only(),
		));

		// Final ascending order: 3 (5%), 2 (20%), 4 (40%), 5 (50%), 1 (60%).
		let order = <LinkedList as SortedListInterface<VaultList, u64>>::iter_from_tail(
			&rate_list(DOT),
			10,
		);
		assert_eq!(order, alloc::vec![3, 2, 4, 5, 1]);
	});
}

// `find_re_insert_position` returns where a listed vault would land at a new
// rate (skipping its own node), and `None` for a vault that is not listed.
#[test]
fn find_re_insert_position_locates_target_and_none_for_unlisted() {
	build_and_execute(|| {
		register_default_branch();
		for (who, pct) in [(1u64, 5), (2, 10), (3, 20), (4, 30), (5, 50)] {
			assert_ok!(open(who, DOT, 1_000, 500, rate_pct(pct, 100)));
		}
		// Move vault 3 (currently 20%) to 25%: its own node is skipped, so among
		// the remaining {5, 10, 30, 50} the new rate sits between 10% (vault 2,
		// tail side) and 30% (vault 4, head side).
		let pos = crate::Pallet::<Test>::find_re_insert_position(DOT, 3, rate_pct(25, 100))
			.expect("vault 3 is listed");
		assert_eq!(pos.prev, Some(4));
		assert_eq!(pos.next, Some(2));
		// A never-opened owner is not in the rate index.
		assert_eq!(
			crate::Pallet::<Test>::find_re_insert_position(DOT, 99, rate_pct(25, 100)),
			None
		);
	});
}

// `vault_rate_index_neighbors` reports a listed vault's live neighbors (`None`
// at the head/tail ends) and `None` for a vault outside the index.
#[test]
fn vault_rate_index_neighbors_reports_ends_and_none_when_unlisted() {
	build_and_execute(|| {
		register_default_branch();
		for (who, pct) in [(1u64, 5), (2, 10), (3, 20), (4, 30), (5, 50)] {
			assert_ok!(open(who, DOT, 1_000, 500, rate_pct(pct, 100)));
		}
		// Tail = lowest rate (vault 1, 5%): no lower (tail-side) neighbor.
		let tail = crate::Pallet::<Test>::vault_rate_index_neighbors(DOT, 1).expect("listed");
		assert_eq!(tail.next, None);
		assert_eq!(tail.prev, Some(2));
		// Head = highest rate (vault 5, 50%): no higher (head-side) neighbor.
		let head = crate::Pallet::<Test>::vault_rate_index_neighbors(DOT, 5).expect("listed");
		assert_eq!(head.prev, None);
		assert_eq!(head.next, Some(4));
		// Middle vault has both neighbors.
		let mid = crate::Pallet::<Test>::vault_rate_index_neighbors(DOT, 3).expect("listed");
		assert_eq!(mid.prev, Some(4));
		assert_eq!(mid.next, Some(2));
		// A never-opened owner is not in the index.
		assert_eq!(crate::Pallet::<Test>::vault_rate_index_neighbors(DOT, 99), None);
		// Redeeming the tail vault to zero drops it from the index → no neighbors.
		assert_ok!(redeem(DOT, 6, 600));
		assert_eq!(crate::Pallet::<Test>::vault_rate_index_neighbors(DOT, 1), None);
	});
}
