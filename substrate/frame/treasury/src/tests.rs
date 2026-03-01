// Tests for ordered spend payout logic.
// WHY: These tests directly encode the scenario from Issue #11100 and the
// three invariants from the triage: (1) out-of-order payout fails,
// (2) head-of-queue with insufficient funds fails but blocks,
// (3) skip_expired_spend unblocks the queue for subsequent spends.

#![cfg(test)]

use crate::{mock::*, Error, Event, NextSpendIndex, SpendCount, Spends};
use frame_support::{assert_noop, assert_ok};
use sp_runtime::traits::AccountIdConversion;

/// Helper: fund the treasury account with `amount`.
fn fund_treasury(amount: u64) {
    let treasury = TreasuryPalletId::get().into_account_truncating();
    let _ = Balances::deposit_creating(&treasury, amount);
}

/// Scenario from Issue #11100:
/// Spend1 (200) approved first, Spend2 (50) approved second.
/// Treasury only has 100. Spend2 should NOT be payable before Spend1.
#[test]
fn out_of_order_payout_is_rejected() {
    new_test_ext().execute_with(|| {
        // Approve Spend1 (large, approved first).
        assert_ok!(Treasury::approve_spend(
            RuntimeOrigin::root(),
            200,
            2u64
        ));
        let spend1_index = 0u32;

        // Advance past maturity for Spend1.
        run_to_block(10);

        // Approve Spend2 (small, approved second).
        assert_ok!(Treasury::approve_spend(
            RuntimeOrigin::root(),
            50,
            3u64
        ));
        let spend2_index = 1u32;

        // Advance past maturity for Spend2 as well.
        run_to_block(20);

        // Fund treasury with only 100 (enough for Spend2 but not Spend1).
        fund_treasury(100);

        // INVARIANT CHECK: Spend2 is NOT at the head of queue.
        // WHY: NextSpendIndex is 0 (Spend1). Spend2 at index 1 must be rejected.
        assert_noop!(
            Treasury::payout_spend(RuntimeOrigin::signed(1), spend2_index),
            Error::<Test>::NotNextInQueue
        );

        // Spend1 also fails due to insufficient funds (200 > 100).
        // WHY: Even the head of queue cannot drain more than available balance.
        assert_noop!(
            Treasury::payout_spend(RuntimeOrigin::signed(1), spend1_index),
            Error::<Test>::InsufficientFunds
        );

        // Queue remains at index 0 — nothing advanced.
        assert_eq!(NextSpendIndex::<Test>::get(), 0);
    });
}

/// After Spend1 expires, skip_expired_spend advances the queue,
/// allowing Spend2 to execute once treasury has enough funds.
#[test]
fn skip_expired_unblocks_queue() {
    new_test_ext().execute_with(|| {
        // Approve Spend1 large, matures at block 5, expires at block 10.
        assert_ok!(Treasury::approve_spend(
            RuntimeOrigin::root(),
            200,
            2u64
        ));

        run_to_block(5); // Spend1 matures.

        // Approve Spend2 small.
        assert_ok!(Treasury::approve_spend(
            RuntimeOrigin::root(),
            50,
            3u64
        ));

        // Only 50 in treasury — Spend1 cannot pay.
        fund_treasury(50);

        run_to_block(5);

        // Cannot skip before expiry.
        // WHY: Prevents prematurely removing a spend that could still be funded.
        assert_noop!(
            Treasury::skip_expired_spend(RuntimeOrigin::signed(1), 0),
            Error::<Test>::NotExpired
        );

        // Advance to expiry of Spend1.
        run_to_block(100);

        // Now skip is permitted.
        assert_ok!(Treasury::skip_expired_spend(RuntimeOrigin::signed(1), 0));

        // Queue advanced to Spend2.
        assert_eq!(NextSpendIndex::<Test>::get(), 1);

        // Spend2 can now execute (50 <= 50 treasury balance).
        // Advance past Spend2 maturity first.
        run_to_block(110);
        assert_ok!(Treasury::payout_spend(RuntimeOrigin::signed(1), 1));

        // Queue advanced past Spend2.
        assert_eq!(NextSpendIndex::<Test>::get(), 2);
    });
}

/// Happy path: sufficient balance, single spend at head of queue executes normally.
#[test]
fn ordered_payout_succeeds_when_funded() {
    new_test_ext().execute_with(|| {
        assert_ok!(Treasury::approve_spend(
            RuntimeOrigin::root(),
            100,
            2u64
        ));

        run_to_block(10); // past maturity
        fund_treasury(200);

        assert_ok!(Treasury::payout_spend(RuntimeOrigin::signed(1), 0));

        // Queue pointer advanced to 1.
        assert_eq!(NextSpendIndex::<Test>::get(), 1);

        // Verify the spend is marked paid.
        let spend = Spends::<Test>::get(0).unwrap();
        assert!(spend.paid);
    });
}
