use crate::{
	mock::*,
	pallet::{BranchStates, Vaults},
	tests::{rate_pct, vault_status},
};
use frame::deps::{
	frame_support::{assert_noop, assert_ok},
	sp_runtime::FixedU128,
};

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
	assert!(!BranchStates::<Test>::get(DOT).expect("branch state").is_frozen());
}

// There is no MCR-band liquidation test here because
// `pallet-vaults::prepare_liquidation` performs no MCR/ICR check itself — that
// gate lives in the orchestrator (`pallet-stability-pool`). The vault pallet's
// liquidation guards are: branch frozen, vault in FinalRecovery, last vault.

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
				Position::endpoints_only(),
			),
			crate::Error::<Test>::SafetyModeTcrWorsening
		);
	});
}

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
			Position::endpoints_only(),
		));
	});
}

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

// In Safety mode, a withdraw paired with a matching repay is done by repaying
// first (always allowed — `repay_for` does not enforce mode rules) and then
// withdrawing. After enough debt is repaid, the branch may exit Safety
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

// Withdrawing collateral with too-low (or no) matching repayment reduces to
// `safety_mode_blocks_withdraw_alone` above. There is no single atomic `adjust`
// extrinsic combining collateral and debt deltas, so the per-call safety gate
// fires standalone the moment `withdraw_collateral` would worsen TCR. No
// additional coverage.

// In Normal mode, a premature rate change that would push TCR below the
// safety threshold reverts. The upfront fee bumps
// `bs.debt.minted_interest` and lowers post-TCR; if pre-TCR is
// just above Safety, post-TCR can land below it.
#[test]
fn normal_mode_blocks_premature_rate_change_pulling_into_safety() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 5_000, rate_pct(5, 100)));
		// Drop price to $6.55 — TCR ≈ 1000*6.55/5005 ≈ 130.87% (just above
		// Safety 130%). The upfront fee on a premature rate hike bumps
		// `debt.minted_interest` enough to land post-TCR below
		// Safety, tripping the Normal-branch rule in `enforce_mode_rules`.
		set_price(DOT, FixedU128::from_rational(655u128, 100u128));
		assert_noop!(
			crate::Pallet::<Test>::change_rate(
				RuntimeOrigin::signed(1),
				DOT,
				rate_pct(50, 100),
				Position::endpoints_only(),
			),
			crate::Error::<Test>::WouldEnterSafetyMode
		);
	});
}

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
				Position::endpoints_only(),
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
			Position::endpoints_only(),
		));
	});
}

#[test]
fn safety_mode_blocks_close_with_collateral() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 5_000, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(5, 100)));
		// Top up acct 2's pUSD so the repay can cover principal + upfront fee.
		let v = Vaults::<Test>::get(DOT, 2).expect("vault stored");
		let total = v.debt.principal + v.debt.interest;
		let _ = <Pusd as frame::deps::frame_support::traits::fungible::Mutate<u64>>::transfer(
			&1,
			&2,
			v.debt.interest,
			frame::deps::frame_support::traits::tokens::Preservation::Expendable,
		);
		// Drop the price; the post-close TCR will be below the safety
		// threshold, so the auto-close inside repay_for must revert.
		set_price(DOT, FixedU128::from_rational(63u128, 10u128));
		assert_noop!(
			crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(2), 2, DOT, total),
			crate::Error::<Test>::WouldEnterSafetyMode
		);
	});
}

#[test]
fn safety_mode_allows_close_zero_collateral() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 5_000, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(5, 100)));
		// Fully redeem vault 2 — it becomes Dormant with residual collateral
		// and zero debt.
		assert_ok!(redeem(DOT, 3, 1_000));
		// Withdraw the residual collateral while still in Normal mode.
		let residual = held(DOT, 2);
		assert!(residual > 0);
		assert_ok!(crate::Pallet::<Test>::withdraw_collateral(
			RuntimeOrigin::signed(2),
			DOT,
			residual,
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

// In Safety mode, a vault whose CR has fallen below ICR (e.g., due to a
// price drop) cannot borrow until enough collateral is deposited to bring
// the CR back above ICR. A zero-amount borrow still triggers the ICR guard,
// so we use `borrow(+0)` as a CR-gate-only probe that validates CR without
// changing debt.
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
			crate::Pallet::<Test>::borrow(
				RuntimeOrigin::signed(2),
				DOT,
				0,
				None,
				None,
				Position::endpoints_only(),
			),
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
			Position::endpoints_only(),
		));
	});
}

// The withdraw-side analogue of the borrow guard above: a vault below ICR
// cannot make itself worse via `withdraw_collateral` (the per-call ICR guard
// fires). Depositing enough collateral to lift branch TCR back above the
// safety threshold lets a subsequent withdraw pass the Normal-mode gate.
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

// Redemptions are the protocol's settlement mechanism: unlike borrow / withdraw
// / close / change_rate (gated above), `apply_redemption` is deliberately not
// subject to the Safety-mode TCR rules, so a redemption settles even while the
// branch sits in Safety mode.
#[test]
fn redemption_proceeds_in_safety_mode() {
	build_and_execute(|| {
		enter_safety_mode_single_vault();
		let tcr = crate::Pallet::<Test>::branch_tcr(DOT).expect("tcr");
		assert!(tcr < rate_pct(130, 100), "setup must leave the branch in Safety mode");

		let target = redeem(DOT, 3, 1_000).expect("redemption settles in Safety mode");
		assert_eq!(target, 1);
		// The vault kept enough debt to remain Active; the redemption simply went
		// through despite Safety mode.
		assert_eq!(vault_status(DOT, 1), crate::types::VaultStatus::Active);
	});
}
