//! Port of liquity_v2/contracts/test/criticalThreshold.t.sol (lines 20822-21011).
//!
//! Mode-aware operation rules under polkadot's `Safety` mode (Liquity's
//! "Critical Threshold" — `pre_TCR < SafetyCollateralizationRatio`). In the
//! polkadot port `enforce_mode_rules` is invoked from only three extrinsics
//! (`open_vault`, `borrow`, `withdraw_collateral` — see helpers.rs:603, 733,
//! 867). Operations that monotonically improve TCR
//! (`deposit_collateral_for`, `repay_for`) skip the gate; `change_rate` and
//! `close_vault` don't gate on TCR either, which is a real divergence from
//! Liquity worth flagging.
//!
//! Liquity error → polkadot Error mapping:
//! - `TCRBelowCCR` → `SafetyModeTcrWorsening`
//! - `ICRBelowMCR` → `UnsafeCollateralizationRatio`
//! - `RepaymentNotMatchingCollWithdrawal` → no analog; the polkadot `withdraw_collateral` reverts
//!   directly via the per-call TCR check.

use crate::{
	mock::*,
	pallet::{BranchStates, Vaults},
	tests::rate_pct,
};
use frame::deps::{
	frame_support::{assert_noop, assert_ok},
	sp_runtime::FixedU128,
};

// --- shared setup -------------------------------------------------------

/// Open one vault, then drop the oracle price so the branch enters Safety
/// mode (TCR ≈ 125.87% — between ICR=120% and Safety=130%).
///
/// pre-state: bs.total_collateral=1000 DOT, bs.total_ib=5000 pUSD, price=$6.30.
/// Vault (acct 1) starts at CR=199.6% before the price drop and stays the
/// only vault on the branch.
fn enter_safety_mode_single_vault() {
	register_default_branch();
	assert_ok!(open(1, DOT, 1_000, 5_000, rate_pct(5, 100)));
	set_price(DOT, FixedU128::from_rational(63u128, 10u128));
	// Sanity: `bs.frozen` must remain `None`; mode is *derived* from TCR.
	assert!(BranchStates::<Test>::get(DOT).expect("branch state").frozen.is_none());
}

// --- ports --------------------------------------------------------------

// SKIPPED row 1: testTrovesAreNotLiquidatedBetweenMCRAndCT.
// `pallet-vaults::prepare_liquidation` performs no MCR/ICR check — that gate
// lives in the orchestrator (`pallet-stability-pool`). The vault pallet's
// liquidation guards are: branch frozen, vault in FinalRecovery, last vault.
// No `NothingToLiquidate` Error variant exists.

// row 2: testNoNewTrovesWithFinalTCRBelowCT.
//
// In Safety mode, opening a vault whose CR is above ICR but below the branch
// TCR strictly lowers TCR — `enforce_mode_rules` rejects the open with
// `SafetyModeTcrWorsening`.
#[test]
fn safety_mode_blocks_new_vault_that_worsens_tcr() {
	build_and_execute(|| {
		enter_safety_mode_single_vault();
		// New vault B at CR ≈ 123% (above ICR 120%, below TCR_pre 125.87%).
		assert_noop!(
			open(2, DOT, 100, 510, rate_pct(5, 100)),
			crate::Error::<Test>::SafetyModeTcrWorsening
		);
	});
}

// row 3: testNewTrovesWithFinalTCRAboveCT.
//
// In Safety mode, opening a large healthy vault that drives TCR up is allowed
// — even when it implicitly exits Safety mode. The Safety-branch rule is
// "post_tcr ≥ pre_tcr" (non-worsening), with no upper bound.
#[test]
fn safety_mode_allows_new_vault_that_improves_tcr() {
	build_and_execute(|| {
		enter_safety_mode_single_vault();
		// New vault B with CR = 630% — improves TCR substantially.
		assert_ok!(open(2, DOT, 1_000, 1_000, rate_pct(5, 100)));
	});
}

// row 4: testNoIncreaseDebtAloneWithFinalTCRBelowCT.
//
// In Safety mode, borrowing more pUSD without adding collateral worsens TCR
// by exactly the upfront-fee proportion → reverts.
#[test]
fn safety_mode_blocks_borrow_alone() {
	build_and_execute(|| {
		enter_safety_mode_single_vault();
		assert_noop!(
			crate::Pallet::<Test>::borrow(
				RuntimeOrigin::signed(1),
				DOT,
				200,
				None,
				None,
				None,
				None,
			),
			crate::Error::<Test>::SafetyModeTcrWorsening
		);
	});
}

// row 5: testNoIncreaseDebtWithAddCollWithFinalTCRBelowCT.
//
// In Safety mode, adding a *small* amount of collateral (insufficient to
// offset the borrow) and then borrowing still worsens TCR. The deposit
// itself is unconditionally allowed (`deposit_collateral_for` does not
// enforce mode rules in the polkadot port); the borrow that follows is
// what reverts.
#[test]
fn safety_mode_blocks_borrow_with_small_deposit() {
	build_and_execute(|| {
		enter_safety_mode_single_vault();
		assert_ok!(crate::Pallet::<Test>::deposit_collateral_for(
			RuntimeOrigin::signed(1),
			1,
			DOT,
			1,
		));
		assert_noop!(
			crate::Pallet::<Test>::borrow(
				RuntimeOrigin::signed(1),
				DOT,
				200,
				None,
				None,
				None,
				None,
			),
			crate::Error::<Test>::SafetyModeTcrWorsening
		);
	});
}

// row 6: testIncreaseDebtWithAddCollWithFinalTCRAboveCT.
//
// In Safety mode, adding *enough* collateral first lifts TCR back above the
// safety threshold, after which a moderate borrow is allowed because both
// pre- and post-states sit in Normal mode.
#[test]
fn safety_mode_allows_borrow_after_large_deposit() {
	build_and_execute(|| {
		enter_safety_mode_single_vault();
		assert_ok!(crate::Pallet::<Test>::deposit_collateral_for(
			RuntimeOrigin::signed(1),
			1,
			DOT,
			200,
		));
		// post-deposit TCR ≈ 1200*6.3/5005 ≈ 151%. Now borrow a moderate amount
		// while staying in Normal mode and well above Safety.
		assert_ok!(crate::Pallet::<Test>::borrow(
			RuntimeOrigin::signed(1),
			DOT,
			200,
			None,
			None,
			None,
			None,
		));
	});
}

// row 7: testNoIncreaseDebtWithWithdrawCollBelowCT.
//
// In Safety mode, withdrawing collateral always worsens TCR (less collateral,
// same debt). The `withdraw_collateral` extrinsic guard fires before any
// follow-up borrow can be attempted.
#[test]
fn safety_mode_blocks_withdraw_alone() {
	build_and_execute(|| {
		enter_safety_mode_single_vault();
		assert_noop!(
			crate::Pallet::<Test>::withdraw_collateral(RuntimeOrigin::signed(1), DOT, 1, None,),
			crate::Error::<Test>::SafetyModeTcrWorsening
		);
	});
}

// row 8: testWithdrawCollAlongWithRepaymentBelowCT.
//
// In Safety mode, the polkadot equivalent of "withdraw + matching repay" is
// to repay first (always allowed — `repay_for` does not enforce mode rules)
// and then withdraw. After enough debt is repaid, the branch may exit Safety
// mode entirely; the subsequent withdraw still passes the per-call TCR check
// because post_TCR ≥ Safety in Normal mode.
#[test]
fn safety_mode_allows_repay_then_withdraw() {
	build_and_execute(|| {
		enter_safety_mode_single_vault();
		// Repay 3000 pUSD: total debt drops from 5005 to ~2005, TCR rises to
		// 1000*6.3/2005 ≈ 314%. Branch exits Safety mode.
		assert_ok!(crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(1), 1, DOT, 3_000));
		// Now withdraw 100 DOT — TCR drops to 900*6.3/2005 ≈ 282%, still in
		// Normal mode and well above Safety threshold.
		assert_ok!(crate::Pallet::<Test>::withdraw_collateral(
			RuntimeOrigin::signed(1),
			DOT,
			100,
			None,
		));
	});
}

// rows 9, 10: testNoCollWithdrawalWith{Low,No}RepaymentBelowCT — these
// reduce to `safety_mode_blocks_withdraw_alone` above. Liquity's
// `RepaymentNotMatchingCollWithdrawal` error is specific to its atomic
// `adjustTrove(coll_delta, debt_delta)` signature; the polkadot port's
// per-call gate fires the moment the standalone `withdraw_collateral` would
// worsen TCR. No additional coverage.

// row 11: testNoPrematureInterestRateAdjustmentIfItWouldPullTCRBelowCCR.
//
// In Normal mode, a premature rate change that would push TCR below the
// safety threshold reverts. The upfront fee bumps
// `bs.total_minted_aggregate_interest` and lowers post-TCR; if pre-TCR is
// just above Safety, post-TCR can land below it.
#[test]
fn normal_mode_blocks_premature_rate_change_pulling_into_safety() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 5_000, rate_pct(5, 100)));
		// Drop price to $6.55 — TCR ≈ 1000*6.55/5005 ≈ 130.87% (just above
		// Safety 130%). The upfront fee on a premature rate hike bumps
		// `total_minted_aggregate_interest` enough to land post-TCR below
		// Safety, tripping the Normal-branch rule in `enforce_mode_rules`.
		set_price(DOT, FixedU128::from_rational(655u128, 100u128));
		assert_noop!(
			crate::Pallet::<Test>::change_rate(
				RuntimeOrigin::signed(1),
				DOT,
				rate_pct(50, 100),
				None,
				None,
			),
			crate::Error::<Test>::SafetyModeTcrWorsening
		);
	});
}

// row 12: testPrematureInterestRateAdjustmentDisallowedIfTCRAlreadyBelowCCR.
//
// Once the branch is in Safety mode, a *premature* (fee-charging) rate
// change is rejected outright — the upfront fee strictly worsens TCR.
// A *post-cooldown* (zero-fee) rate change is still allowed because the
// upfront fee is zero and so post_TCR == pre_TCR.
#[test]
fn safety_mode_blocks_premature_rate_change() {
	build_and_execute(|| {
		enter_safety_mode_single_vault();
		// Premature change (within cooldown) charges a non-zero upfront fee
		// → reverts.
		assert_noop!(
			crate::Pallet::<Test>::change_rate(
				RuntimeOrigin::signed(1),
				DOT,
				rate_pct(7, 100),
				None,
				None,
			),
			crate::Error::<Test>::SafetyModeTcrWorsening
		);
	});
}

#[test]
fn safety_mode_allows_post_cooldown_rate_change() {
	build_and_execute(|| {
		enter_safety_mode_single_vault();
		// Wait out the cooldown so the rate change carries no upfront fee.
		// Default rate_adjustment_cooldown = 1 day = 86_400_000 ms.
		advance_time(86_400_000);
		assert_ok!(crate::Pallet::<Test>::change_rate(
			RuntimeOrigin::signed(1),
			DOT,
			rate_pct(7, 100),
			None,
			None,
		));
	});
}

// rows N/A in tests.md but per troves.md §4.3 "Close active vault":
// in Safety mode close requires post-TCR >= pre-TCR; closing a vault with
// non-zero collateral worsens TCR and is rejected.
#[test]
fn safety_mode_blocks_close_with_collateral() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 5_000, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(5, 100)));
		// Repay vault 2's debt fully so it satisfies the zero-debt close
		// precondition. Top up the upfront-fee residual from acct 1.
		let v = Vaults::<Test>::get(DOT, 2).expect("vault stored");
		let total = v.interest_bearing_debt + v.accrued_interest;
		let _ = <Pusd as frame::deps::frame_support::traits::fungible::Mutate<u64>>::transfer(
			&1,
			&2,
			v.accrued_interest,
			frame::deps::frame_support::traits::tokens::Preservation::Expendable,
		);
		assert_ok!(crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(2), 2, DOT, total));
		// Drop the price into Safety mode.
		set_price(DOT, FixedU128::from_rational(63u128, 10u128));
		// Closing vault 2 (which still has 1000 DOT held) drops branch
		// total_collateral and worsens TCR → reverts in Safety.
		assert_noop!(
			crate::Pallet::<Test>::close_vault(RuntimeOrigin::signed(2), DOT, None),
			crate::Error::<Test>::SafetyModeTcrWorsening
		);
	});
}

#[test]
fn safety_mode_allows_close_zero_collateral() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 5_000, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(5, 100)));
		// Repay vault 2 fully and withdraw all of its collateral while still
		// in Normal mode (price 10).
		let v = Vaults::<Test>::get(DOT, 2).expect("vault stored");
		let total = v.interest_bearing_debt + v.accrued_interest;
		let _ = <Pusd as frame::deps::frame_support::traits::fungible::Mutate<u64>>::transfer(
			&1,
			&2,
			v.accrued_interest,
			frame::deps::frame_support::traits::tokens::Preservation::Expendable,
		);
		assert_ok!(crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(2), 2, DOT, total));
		assert_ok!(crate::Pallet::<Test>::withdraw_collateral(
			RuntimeOrigin::signed(2),
			DOT,
			1_000,
			None,
		));
		assert_eq!(held(DOT, 2), 0);
		// Drop into Safety. Closing the empty vault does not change branch
		// collateral → post_TCR == pre_TCR → allowed.
		set_price(DOT, FixedU128::from_rational(63u128, 10u128));
		assert_ok!(crate::Pallet::<Test>::close_vault(RuntimeOrigin::signed(2), DOT, None));
		assert!(Vaults::<Test>::get(DOT, 2).is_none());
	});
}

// row 13: testNoAdjustmentIfFinalICRLtMCRFromAbove100AddingColl.
//
// In Safety mode, a vault whose CR has fallen below ICR (e.g., due to a
// price drop) cannot borrow until enough collateral is deposited to bring
// the CR back above ICR. The Liquity test uses `adjustTrove(+coll,+0)`
// where the +0-debt adjust still triggers the ICR guard; in polkadot we
// exercise the same guard via `borrow(+0)` (which validates CR without
// changing debt).
#[test]
fn safety_mode_blocks_borrow_when_cr_below_icr() {
	build_and_execute(|| {
		register_default_branch();
		// Open a healthy whale (acct 1) so acct 2's vault isn't the last on
		// the branch and so the price drop puts both vaults into Safety mode.
		assert_ok!(open(1, DOT, 1_000, 5_000, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 100, 200, rate_pct(5, 100)));
		// Drop price to $2.10: acct 2's CR ≈ 100*2.10/205 ≈ 102.4% — below
		// MCR 110%, above 100%. Branch TCR also enters Safety mode.
		set_price(DOT, FixedU128::from_rational(21u128, 10u128));

		// borrow(+0) revalidates CR without touching debt, so we use it as a
		// gate-only probe. CR is below ICR → reverts.
		assert_noop!(
			crate::Pallet::<Test>::borrow(RuntimeOrigin::signed(2), DOT, 0, None, None, None, None,),
			crate::Error::<Test>::UnsafeCollateralizationRatio
		);

		// Top up enough collateral to push acct 2's CR comfortably above
		// ICR (200 DOT * 2.10 / 205 ≈ 204.9%).
		assert_ok!(crate::Pallet::<Test>::deposit_collateral_for(
			RuntimeOrigin::signed(2),
			2,
			DOT,
			100,
		));
		// borrow(+0) now passes the CR gate. The TCR check passes too because
		// post_TCR == pre_TCR for a zero-amount borrow (Safety-branch allows
		// equal TCR).
		assert_ok!(crate::Pallet::<Test>::borrow(
			RuntimeOrigin::signed(2),
			DOT,
			0,
			None,
			None,
			None,
			None,
		));
	});
}

// row 14: testNoAdjustmentIfFinalICRLtMCRFromAbove100Repaying.
//
// Mirror of row 13 from the repayment side: a vault below ICR cannot make
// itself worse via `withdraw_collateral` (the per-call ICR guard fires).
// Depositing enough collateral to lift branch TCR back above the safety
// threshold lets a subsequent withdraw pass the Normal-mode gate.
#[test]
fn safety_mode_blocks_withdraw_when_cr_below_icr() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 5_000, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 100, 200, rate_pct(5, 100)));
		set_price(DOT, FixedU128::from_rational(21u128, 10u128));

		// Withdrawing any collateral fails because post-CR < ICR (and so does
		// pre-CR; the per-call gate uses the post-state).
		assert_noop!(
			crate::Pallet::<Test>::withdraw_collateral(RuntimeOrigin::signed(2), DOT, 1, None,),
			crate::Error::<Test>::UnsafeCollateralizationRatio
		);

		// Top up enough collateral to lift the branch back out of Safety
		// mode entirely (target TCR > 130%). bs.total_debt ≈ 5206; we need
		// total_coll * 2.10 / 5206 ≥ 1.30 → total_coll ≥ 3223 DOT, so a
		// deposit of 3000 DOT puts us comfortably in Normal mode.
		assert_ok!(crate::Pallet::<Test>::deposit_collateral_for(
			RuntimeOrigin::signed(2),
			2,
			DOT,
			3_000,
		));
		// Withdraw 1 DOT now — vault 2 CR is huge, branch is in Normal mode
		// well above Safety, so the per-call gate passes from both directions.
		assert_ok!(crate::Pallet::<Test>::withdraw_collateral(
			RuntimeOrigin::signed(2),
			DOT,
			1,
			None,
		));
	});
}

// rows 15, 16: testNoAdjustmentIfFinalICRLtMCRFromBelow100{AddingColl,Repaying}.
//
// Same shape as rows 13 and 14 but with the vault's CR already below 100%
// (vault is bad-debt-bearing). In polkadot the per-call ICR gate fires for
// any post-CR < ICR regardless of whether pre-CR was > or < 100%, so the
// behaviour is identical to rows 13 and 14. Captured here for completeness
// of the source-of-truth catalog; not duplicated as separate Rust tests.
