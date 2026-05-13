//! Port of liquity_v2/contracts/test/SortedTroves.t.sol (lines 29901-30503).
//!
//! Liquity ships these as fuzz tests over a custom rate-ordered DLL. In the
//! polkadot port the DLL itself lives in `pallet-linked-list` (which carries
//! its own test suite); these tests pin the *integration* between
//! `pallet-vaults` and the rate index — i.e., that user-facing ops insert
//! and remove items at the correct position.
//!
//! Liquity rows 2 and 5 (batched-trove ordering / contiguity) are out of
//! scope: the polkadot port has no batch managers in v1. The other four
//! rows (1, 3, 4, 6) are exercised here.

use crate::{mock::*, tests::rate_pct};
use frame::deps::frame_support::assert_ok;
use pallet_linked_list::SortedListInterface;

const ONE_DAY_MS: Moment = 24 * 3_600 * 1_000;

// row 1: test_SortsIndividualTrovesByAnnualInterestRate — open vaults in
// arbitrary order, walk tail-first (lowest score → highest), expect ascending
// order.
#[test]
fn open_orders_dll_by_annual_interest_rate() {
	build_and_execute(|| {
		register_default_branch();
		// Open in scrambled order with distinct rates.
		for (who, pct) in [(3u64, 30), (1, 5), (5, 50), (2, 10), (4, 40)] {
			assert_ok!(open(who, DOT, 1_000, 500, rate_pct(pct, 100)));
		}
		// Tail-first walk gives ascending rate. Expect [1, 2, 3, 4, 5].
		let order = <LinkedList as SortedListInterface<u32, u64>>::iter_from_tail(&DOT, 10);
		assert_eq!(order, alloc::vec![1, 2, 3, 4, 5]);
	});
}

// row 3: test_FindsValidInsertPosition — `find_rate_position` returns valid
// neighbors for any new score.
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

// row 4: test_CanRemoveIndividualTroves — closing a vault removes it from
// the rate index.
#[test]
fn close_vault_removes_from_rate_index() {
	build_and_execute(|| {
		register_default_branch();
		for (who, pct) in [(1u64, 5), (2, 10), (3, 20), (4, 30), (5, 50)] {
			assert_ok!(open(who, DOT, 1_000, 500, rate_pct(pct, 100)));
		}
		// Repay vault 3 fully (top up from acct 4) then close it.
		let v = crate::pallet::Vaults::<Test>::get(DOT, 3).unwrap();
		let total = v.interest_bearing_debt + v.accrued_interest;
		let _ = <Pusd as frame::deps::frame_support::traits::fungible::Mutate<u64>>::transfer(
			&4,
			&3,
			v.accrued_interest,
			frame::deps::frame_support::traits::tokens::Preservation::Expendable,
		);
		assert_ok!(crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(3), 3, DOT, total));
		assert_ok!(crate::Pallet::<Test>::close_vault(RuntimeOrigin::signed(3), DOT, None));
		assert!(!<LinkedList as SortedListInterface<u32, u64>>::contains(&DOT, &3));
		// Order without acct 3: [1, 2, 4, 5].
		let order = <LinkedList as SortedListInterface<u32, u64>>::iter_from_tail(&DOT, 10);
		assert_eq!(order, alloc::vec![1, 2, 4, 5]);
	});
}

// row 6: test_CanReInsert — `change_rate` re-inserts the vault at its new
// rate position. We walk through several adjustments and assert the final
// ordering matches the expected ascending-by-rate sequence.
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
		let order = <LinkedList as SortedListInterface<u32, u64>>::iter_from_tail(&DOT, 10);
		assert_eq!(order, alloc::vec![3, 2, 4, 5, 1]);
	});
}

// Polkadot-specific: same-rate insertion lands at the tail of the same-rate
// run (LIFO). Already covered in `lifecycle::same_rate_lifo_redemption_order`,
// re-asserted here with five vaults to confirm the pattern doesn't degrade.
#[test]
fn same_rate_insertion_lands_at_tail_of_run() {
	build_and_execute(|| {
		register_default_branch();
		for who in 1u64..=5 {
			assert_ok!(open(who, DOT, 1_000, 500, rate_pct(5, 100)));
		}
		// Tail-first iteration of a same-rate run is reverse insertion order.
		let order = <LinkedList as SortedListInterface<u32, u64>>::iter_from_tail(&DOT, 10);
		assert_eq!(order, alloc::vec![5, 4, 3, 2, 1]);
	});
}
