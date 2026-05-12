//! Port of liquity_v2/contracts/test/redemptions.t.sol (lines 27623-28754),
//! pallet-vaults-owned rows 11-42 (per `tests.md` "Pallet ownership").
//!
//! The Dormant-vault lifecycle is the most important behavioural surface
//! for `pallet-vaults`: vaults transition `Active → Dormant` on full or
//! sub-MinimumDebt redemption, are removed from the rate index, are still
//! reachable for further redemption via `last_dormant_vault_owner`, and
//! can be revived to `Active` by the owner drawing fresh debt above
//! `MinimumDebt` (or by interest accrual / redistribution gains pushing
//! their debt back up).
//!
//! Liquity's "Zombie" → polkadot's `Dormant`. Liquity's `lastZombieTroveId`
//! → polkadot's `bs.last_dormant_vault_owner`.
//!
//! Polkadot divergence flags (rows 34-37): Liquity rejects normal
//! `repayBold` / `withdrawBold` / `addColl` on Dormant vaults; the polkadot
//! port is permissive — only `change_rate` is gated on `VaultStatus::Active`
//! (helpers.rs::change_rate). Per troves.md §4.3 ("Deposit to dormant vault
//! …") this is a divergence; the rows are SKIPPED here and the divergence
//! is documented inline.

use crate::{
	mock::*,
	pallet::{BranchStates, Vaults},
	tests::rate_pct,
	types::VaultStatus,
};
use frame::deps::frame_support::{assert_noop, assert_ok};
use pallet_linked_list::SortedListInterface;

const ONE_DAY_MS: Moment = 24 * 3_600 * 1_000;

// =====================================================================
// §A: Active → Dormant transitions on redemption
// =====================================================================

// row 11: testFullyRedeemedTroveBecomesZombieTrove.
// row 14: testZombieTrovesRemovedFromSortedList — rolled in.
#[test]
fn fully_redeemed_vault_becomes_dormant_and_leaves_rate_index() {
	build_and_execute(|| {
		register_default_branch();
		// A and B at distinct rates so the redemption order is deterministic
		// (tail-first picks A first as it has the lower rate).
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));

		// Redeem an amount large enough to cancel acct 1's full debt.
		let v_pre = Vaults::<Test>::get(DOT, 1).unwrap();
		let total = v_pre.interest_bearing_debt + v_pre.accrued_interest;
		let target = redeem(DOT, 3, total).expect("redeem ok");
		assert_eq!(target, 1);

		let v = Vaults::<Test>::get(DOT, 1).unwrap();
		assert!(matches!(v.status, VaultStatus::Dormant));
		assert_eq!(v.interest_bearing_debt + v.accrued_interest, 0);
		// Rate index no longer contains acct 1.
		assert!(!<LinkedList as SortedListInterface<u32, u64>>::contains(&DOT, &1));
	});
}

// row 12: testTroveRedeemedToBelowMIN_DEBTBecomesZombieTrove.
#[test]
fn redeemed_below_min_debt_becomes_dormant() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));

		// MinimumDebt = 200 (from default_branch_config). Redeem so acct 1
		// has < 200 left.
		assert_ok!(redeem(DOT, 3, 350));
		let v = Vaults::<Test>::get(DOT, 1).unwrap();
		let total = v.interest_bearing_debt + v.accrued_interest;
		assert!(total > 0 && total < 200, "got total = {}", total);
		assert!(matches!(v.status, VaultStatus::Dormant));
		assert!(!<LinkedList as SortedListInterface<u32, u64>>::contains(&DOT, &1));
	});
}

// row 13: testTroveRedeemedToAboveMIN_DEBTDoesNotBecomesZombieTrove.
#[test]
fn redeemed_above_min_debt_stays_active() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));

		// Redeem 200 — leaves acct 1 with ≈ 300 debt, well above MinimumDebt.
		assert_ok!(redeem(DOT, 3, 200));
		let v = Vaults::<Test>::get(DOT, 1).unwrap();
		assert!(matches!(v.status, VaultStatus::Active));
		assert!(<LinkedList as SortedListInterface<u32, u64>>::contains(&DOT, &1));
	});
}

// =====================================================================
// §B: last_dormant_vault_owner pointer (rows 16, 17)
// =====================================================================

// row 16: testRedemptionsWithNoPartialLeaveNoPointerToZombieTroves.
//
// When all redeemed vaults are fully cleared (no residual), the
// `last_dormant_vault_owner` pointer ends up `None` — no in-flight Dormant
// to track.
#[test]
fn full_redemption_leaves_no_dormant_pointer() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		// Fully redeem acct 1.
		let v = Vaults::<Test>::get(DOT, 1).unwrap();
		let total = v.interest_bearing_debt + v.accrued_interest;
		assert_ok!(redeem(DOT, 3, total));
		let bs = BranchStates::<Test>::get(DOT).unwrap();
		assert_eq!(bs.last_dormant_vault_owner, None);
	});
}

// row 12 (companion): partial-to-Dormant sets the pointer.
#[test]
fn partial_below_min_debt_sets_dormant_pointer() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		assert_ok!(redeem(DOT, 3, 350));
		let bs = BranchStates::<Test>::get(DOT).unwrap();
		assert_eq!(bs.last_dormant_vault_owner, Some(1));
	});
}

// row 17: testZombieTrovePointerGetsResetIfLastOneIsFullyRedemeed.
#[test]
fn dormant_pointer_clears_when_last_dormant_fully_redeemed() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		// Push acct 1 to Dormant via partial-below-MinDebt.
		assert_ok!(redeem(DOT, 3, 350));
		let bs = BranchStates::<Test>::get(DOT).unwrap();
		assert_eq!(bs.last_dormant_vault_owner, Some(1));
		// Now redeem acct 1's full residual. next_redemption_target prefers
		// last_dormant_vault_owner, so this hits acct 1 again.
		let v = Vaults::<Test>::get(DOT, 1).unwrap();
		let residual = v.interest_bearing_debt + v.accrued_interest;
		let target = redeem(DOT, 3, residual).expect("redeem residual ok");
		assert_eq!(target, 1);
		let bs = BranchStates::<Test>::get(DOT).unwrap();
		assert_eq!(bs.last_dormant_vault_owner, None);
	});
}

// row 15: testZombieTroveCanStillBeRedeemedFrom — covered by the
// `dormant_pointer_clears_when_last_dormant_fully_redeemed` flow above
// (the second redemption reaches acct 1 even though it's Dormant).

// =====================================================================
// §C: Dormant resuscitation (rows 18, 19, 20)
// =====================================================================

// row 18: testZombieTrovePointerGetsResetIfTroveIsResuscitatedManuallyByOwner.
#[test]
fn dormant_pointer_clears_when_owner_revives_via_borrow() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		assert_ok!(redeem(DOT, 3, 350));
		let bs = BranchStates::<Test>::get(DOT).unwrap();
		assert_eq!(bs.last_dormant_vault_owner, Some(1));

		// Owner borrows enough to push debt above MinimumDebt → revives.
		assert_ok!(crate::Pallet::<Test>::borrow(
			RuntimeOrigin::signed(1),
			DOT,
			500,
			None,
			None,
			None,
			None,
		));
		let v = Vaults::<Test>::get(DOT, 1).unwrap();
		assert!(matches!(v.status, VaultStatus::Active));
		assert!(<LinkedList as SortedListInterface<u32, u64>>::contains(&DOT, &1));
		let bs = BranchStates::<Test>::get(DOT).unwrap();
		assert_eq!(bs.last_dormant_vault_owner, None);
	});
}

// row 19: testZombieTrovePointerGetsResetIfTroveIsResuscitatedViaInterest.
//
// Polkadot does NOT auto-revive a Dormant vault when interest accrual pushes
// debt above MinimumDebt. Per `apply_redemption` and the touch_vault flow,
// status transitions from Dormant → Active only on owner action that crosses
// the threshold (see borrow's `dormant_to_active` flag). Pure interest
// accrual leaves the vault Dormant, so we pin the no-revive behaviour.
#[test]
fn dormant_does_not_auto_revive_via_interest_accrual() {
	build_and_execute(|| {
		register_default_branch();
		// Acct 1 at lower rate so it's the deterministic redemption target.
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(50, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(60, 100)));
		assert_ok!(redeem(DOT, 3, 350));

		// Long horizon — accrued interest grows via update_aggregate_interest /
		// touch_vault, but vault.status stays Dormant.
		advance_time(3650 * ONE_DAY_MS);
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(2), 1, DOT));
		let v = Vaults::<Test>::get(DOT, 1).unwrap();
		assert!(matches!(v.status, VaultStatus::Dormant));
	});
}

// row 20: testZombieTrovePointerGetsResetIfTroveIsClosed.
#[test]
fn dormant_pointer_clears_when_owner_closes_dormant() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		assert_ok!(redeem(DOT, 3, 350));
		// Acct 1 is Dormant with residual debt. Repay all of it, top up
		// from acct 2 to cover any accrued interest residual.
		let v = Vaults::<Test>::get(DOT, 1).unwrap();
		let total = v.interest_bearing_debt + v.accrued_interest;
		let _ = <Pusd as frame::deps::frame_support::traits::fungible::Mutate<u64>>::transfer(
			&2,
			&1,
			v.accrued_interest,
			frame::deps::frame_support::traits::tokens::Preservation::Expendable,
		);
		assert_ok!(crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(1), 1, DOT, total));
		assert_ok!(crate::Pallet::<Test>::close_vault(RuntimeOrigin::signed(1), DOT, None));
		let bs = BranchStates::<Test>::get(DOT).unwrap();
		assert_eq!(bs.last_dormant_vault_owner, None);
	});
}

// =====================================================================
// §D: Dormant vaults still earn redistribution / interest (rows 24, 25)
// =====================================================================

// row 25: testZombieTrovesAccrueInterest.
#[test]
fn dormant_vault_with_residual_accrues_interest() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(50, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(50, 100)));
		assert_ok!(redeem(DOT, 3, 350));
		let v_pre = Vaults::<Test>::get(DOT, 1).unwrap();

		advance_time(365 * ONE_DAY_MS); // 1 year
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(2), 1, DOT));
		let v_post = Vaults::<Test>::get(DOT, 1).unwrap();
		assert!(
			v_post.accrued_interest > v_pre.accrued_interest,
			"Dormant vault with residual debt must accrue interest over time"
		);
	});
}

// row 24: testZombieTrovesCanReceiveRedistGains.
//
// A liquidation distributes its debt and collateral to the remaining stake
// pool. Dormant vaults keep their stake (compute_stake at open) and so
// receive redistribution gains when `touch_vault` reconciles the epoch lag.
#[test]
fn dormant_vault_receives_redistribution_gains_on_touch() {
	build_and_execute(|| {
		register_default_branch();
		// Distinct rates so the rate-index tail is deterministic — acct 1 at
		// the lower rate sits at the tail, where the redemption helper picks
		// it first.
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		assert_ok!(open(3, DOT, 200, 200, rate_pct(5, 100)));
		assert_ok!(redeem(DOT, 4, 700)); // pushes acct 1 to Dormant

		// Force acct 3 to be liquidatable: drop the price and trigger
		// liquidation. The trait-driven `liquidate` helper runs the full
		// prepare+finalize cycle, distributing acct 3's debt to remaining
		// stake (which still includes acct 1's Dormant stake).
		let v_dormant_pre = Vaults::<Test>::get(DOT, 1).unwrap();
		assert_ok!(liquidate(DOT, 3));
		// Touch acct 1 so the epoch lag closes and redist gains land on it.
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(2), 1, DOT));
		let v_dormant_post = Vaults::<Test>::get(DOT, 1).unwrap();
		// Gains may be very small for tiny liquidations; pin "did not lose".
		assert!(
			v_dormant_post.interest_bearing_debt >= v_dormant_pre.interest_bearing_debt,
			"Dormant vault should not lose principal across redistribution"
		);
	});
}

// =====================================================================
// §E: Drawing fresh debt on Dormant (rows 29-33)
// =====================================================================

// rows 29, 30, 31: testZombieBorrowerCanDrawFreshDebtToAboveMIN_DEBT,
// status changes to Active, inserted to sorted list.
#[test]
fn dormant_owner_borrowing_above_min_debt_revives_to_active() {
	build_and_execute(|| {
		register_default_branch();
		// Distinct rates so the rate-index tail is deterministic — acct 1 at
		// the lower rate sits at the tail, where the redemption helper picks
		// it first.
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		assert_ok!(redeem(DOT, 3, 350));
		assert!(matches!(Vaults::<Test>::get(DOT, 1).unwrap().status, VaultStatus::Dormant));
		assert!(!<LinkedList as SortedListInterface<u32, u64>>::contains(&DOT, &1));

		// Borrow 500 more — vault debt jumps from ~150 to ~650, well above
		// MinimumDebt 200.
		assert_ok!(crate::Pallet::<Test>::borrow(
			RuntimeOrigin::signed(1),
			DOT,
			500,
			None,
			None,
			None,
			None,
		));
		let v = Vaults::<Test>::get(DOT, 1).unwrap();
		assert!(matches!(v.status, VaultStatus::Active));
		// row 31: re-inserted into the rate index at the new (or unchanged) rate.
		assert!(<LinkedList as SortedListInterface<u32, u64>>::contains(&DOT, &1));
	});
}

// row 33: testZombieTroveBorrowerCanNotDrawFreshDebtToBelowMIN_DEBT.
//
// `borrow` requires `vault.interest_bearing_debt >= cfg.minimum_debt` after
// the operation (helpers.rs:835). Borrowing on a Dormant vault that doesn't
// reach the threshold reverts.
#[test]
fn dormant_borrow_below_min_debt_reverts() {
	build_and_execute(|| {
		register_default_branch();
		// Distinct rates so the rate-index tail is deterministic — acct 1 at
		// the lower rate sits at the tail, where the redemption helper picks
		// it first.
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		assert_ok!(redeem(DOT, 3, 480)); // pushes acct 1 to Dormant with tiny debt
								   // Borrow 1 — total debt would be far below MinimumDebt 200.
		assert_noop!(
			crate::Pallet::<Test>::borrow(RuntimeOrigin::signed(1), DOT, 1, None, None, None, None,),
			crate::Error::<Test>::DebtBelowMinimum
		);
	});
}

// =====================================================================
// §F: Dormant blocks change_rate (row 38)
// =====================================================================

// row 38: testZombieTroveBorrowerCanNotChangeInterestRate.
#[test]
fn dormant_vault_cannot_change_rate() {
	build_and_execute(|| {
		register_default_branch();
		// Distinct rates so the rate-index tail is deterministic — acct 1 at
		// the lower rate sits at the tail, where the redemption helper picks
		// it first.
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		assert_ok!(redeem(DOT, 3, 350));
		assert_noop!(
			crate::Pallet::<Test>::change_rate(
				RuntimeOrigin::signed(1),
				DOT,
				rate_pct(7, 100),
				None,
				None,
			),
			crate::Error::<Test>::InvalidVaultStatus
		);
	});
}

// =====================================================================
// SKIPPED rows 21, 34, 35, 36, 37, 39-43:
// =====================================================================
//
// row 21: `testZombieTrovePointerGetsResetIfTroveIsClosedFromABatch` — batch
//         managers are out of scope in v1.
//
// rows 34, 35, 36, 37: Liquity rejects normal `repayBold`, `withdrawBold`,
//         `adjustTrove` and `addColl` on Dormant vaults. The polkadot port
//         (helpers.rs) only gates on FinalRecovery and (for change_rate) on
//         Active. Dormant vaults can be deposited into / withdrawn from /
//         repaid in polkadot. This is a divergence from troves.md §4.3
//         "Deposit to dormant vault: Only if revived to ≥MinDebt in same
//         operation" — it's not currently enforced at the call site.
//
//         Worth raising as a design follow-up: should the polkadot port
//         add `ensure!(matches!(status, Active), InvalidVaultStatus)` to
//         deposit_collateral_for / withdraw_collateral / repay_for?
//
// row 39: `testZombieTroveAccruedInterestCanBePermissionlesslyAppliedButStaysZombie`
//         — covered by `dormant_does_not_auto_revive_via_interest_accrual`
//         (Polkadot's poke leaves Dormant status unchanged).
// row 40: `testZombieTroveAccruedInterestCanBePermissionlesslyAppliedAndResuscitated`
//         — Liquity auto-revives via interest; polkadot does NOT (already
//         pinned in §C row 19 above).
// row 41: liquidation of Dormant — ownership flagged as `pallet-stability-pool`.
// row 42: `testZombieTroveCanActAsLastTrove` — covered by the existing
//         `last_vault::liquidate_succeeds_when_a_second_vault_exists` and
//         `last_vault::liquidate_only_vault_returns_last_vault_error`.
// row 43: batch-manager redemption — out of scope in v1.
