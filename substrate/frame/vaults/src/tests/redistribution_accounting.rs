//! Tests for the redistribution / aggregate-interest accounting identities and
//! the FinalRecovery exit / orchestrator-trust hot spots after liquidations.
//!
//! Conventions:
//! - Vaults are opened with `stake == collateral`; `vault.redistribution_stake` mirrors the live
//!   `VaultCollateral` hold for every Active/Dormant vault, so the two stay equal across the test.
//! - "Recipient rate" means the recipient vault's `annual_rate`, not the liquidated vault's rate.

use crate::{
	mock::*,
	pallet::{BranchStates, Vaults},
	tests::{rate_pct, vault_status},
};
use frame::deps::{
	frame_support::assert_ok,
	sp_runtime::{traits::Saturating, FixedPointNumber, FixedU128},
};
use pusd_primitives::{KeeperCompensation, LiquidationAllocation, OffsetAllocation};

const ONE_YEAR_MS: Moment = 31_557_600_000;

/// `floor(x * rate)` for the recipient-rate assertions.
fn weighted(x: Balance, rate: FixedU128) -> Balance {
	rate.saturating_mul_int(x)
}

// After a redistribute-everything liquidation with recipients all at 5%, the
// branch's `debt.weighted_principal_sum` must reflect the economic debt at the
// recipients' actual rate: ≈ avg recipient rate × total economic debt, not the
// redistributed principal carried at rate=1.0.
#[test]
fn weighted_sum_after_redistribution_matches_avg_recipient_rate() {
	build_and_execute(|| {
		register_default_branch();
		// Two vaults at 5% / $500 each. Both stakes are 1000.
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(5, 100)));

		// Drop price below MCR. Vault 1 is now liquidatable.
		set_price(DOT, FixedU128::from_rational(5u128, 100u128));

		let coll_1 = held(DOT, 1);
		assert_ok!(liquidate_with(DOT, 1, |_| LiquidationAllocation {
			offset: OffsetAllocation { recipient: 0, debt: 0, collateral: 0 },
			redistribution_collateral: coll_1,
			keeper: KeeperCompensation { recipient: 1, collateral: 0 },
		}));

		let bs = BranchStates::<Test>::get(DOT).expect("branch state");
		let total_econ = bs.debt.principal.saturating_add(bs.debt.pending_redist_principal);
		// weighted_sum ≈ total_econ * 0.05 (B's rate, the only recipient).
		let expected = weighted(total_econ, rate_pct(5, 100));
		let actual = bs.debt.weighted_principal_sum;
		// Tolerance: a couple of dust units from ceil/floor mismatches.
		assert!(
			actual.abs_diff(expected) <= 3,
			"weighted_sum after redistribution out of bounds: actual={}, expected={} (5% of {})",
			actual,
			expected,
			total_econ,
		);
	});
}

// Post-liquidation: advance one year, force a poke so `update_aggregate_interest`
// runs against the redistributed share, and check that the minted aggregate
// interest after 1y is bounded by the recipient rates (≈ 5%/yr on the total
// economic debt), reflecting what recipients actually owe.
#[test]
fn aggregate_interest_post_redistribution_bounded_by_recipient_rates() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(5, 100)));
		set_price(DOT, FixedU128::from_rational(5u128, 100u128));
		let coll_1 = held(DOT, 1);
		assert_ok!(liquidate_with(DOT, 1, |_| LiquidationAllocation {
			offset: OffsetAllocation { recipient: 0, debt: 0, collateral: 0 },
			redistribution_collateral: coll_1,
			keeper: KeeperCompensation { recipient: 1, collateral: 0 },
		}));

		let pre_minted = BranchStates::<Test>::get(DOT).unwrap().debt.minted_interest;
		let bs_pre = BranchStates::<Test>::get(DOT).unwrap();
		let total_econ_pre =
			bs_pre.debt.principal.saturating_add(bs_pre.debt.pending_redist_principal);

		// Advance one year, then poke vault 2 to fold pending interest into
		// `debt.minted_interest` and trigger redistribution apply.
		advance_time(ONE_YEAR_MS);
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(99), 2, DOT));

		let post_minted = BranchStates::<Test>::get(DOT).unwrap().debt.minted_interest;
		let delta = post_minted.saturating_sub(pre_minted);

		// Expected: ~5% / yr on the total economic debt at liquidation time.
		// Allow a +/- 20% band: simple_interest_ceil rounds up, and per-vault
		// attribution may slightly under- or over-attribute redistribution.
		let target = weighted(total_econ_pre, rate_pct(5, 100));
		let lower = target.saturating_mul(80).saturating_div(100);
		let upper = target.saturating_mul(120).saturating_div(100);
		assert!(
			delta >= lower && delta <= upper,
			"1y interest mint out of band: delta={}, target≈{}, total_econ={}",
			delta,
			target,
			total_econ_pre,
		);
	});
}

// Setup three vaults at different rates; liquidate one with full
// redistribution. Each per-vault touch reconciles the redistributed principal
// to that vault's own rate, so after both surviving recipients touch the branch
// `debt.weighted_principal_sum` equals the sum of each recipient's post-touch
// contribution at its **own** rate, within a small rounding tolerance.
#[test]
fn mixed_rate_recipients_reconcile_on_touch() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100))); // A — recipient
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(50, 100))); // B — recipient
		assert_ok!(open(3, DOT, 1_000, 500, rate_pct(10, 100))); // C — liquidated

		set_price(DOT, FixedU128::from_rational(5u128, 100u128));
		let coll_3 = held(DOT, 3);
		assert_ok!(liquidate_with(DOT, 3, |_| LiquidationAllocation {
			offset: OffsetAllocation { recipient: 0, debt: 0, collateral: 0 },
			redistribution_collateral: coll_3,
			keeper: KeeperCompensation { recipient: 3, collateral: 0 },
		}));

		// Force each survivor to touch so their redistribution share is
		// reconciled.
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(99), 1, DOT));
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(99), 2, DOT));

		let bs = BranchStates::<Test>::get(DOT).unwrap();
		let v_a = Vaults::<Test>::get(DOT, 1).unwrap();
		let v_b = Vaults::<Test>::get(DOT, 2).unwrap();
		// Expected: each vault contributes ib_debt * own_rate, summed.
		let expected = weighted(v_a.debt.principal, rate_pct(5, 100))
			.saturating_add(weighted(v_b.debt.principal, rate_pct(50, 100)));
		let actual = bs.debt.weighted_principal_sum;
		// Larger tolerance (10 units): two vaults × per-stake rounding plus the
		// avg-rate-vs-true-rate composition delta.
		assert!(
			actual.abs_diff(expected) <= 10,
			"mixed-rate weighted sum drift too large: actual={}, expected={}",
			actual,
			expected,
		);
	});
}

// A follow-on `borrow` against a recipient must keep the branch weighted_sum
// consistent with each vault's own-rate contribution: the borrow first touches
// the vault to fold in its redistribution share, then updates the weighted-sum
// bookkeeping so the aggregate still equals Σ (own ib_debt × own rate).
#[test]
fn borrow_after_redistribution_keeps_weighted_sum_consistent() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100))); // A — recipient + borrower
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(50, 100))); // B — recipient
		assert_ok!(open(3, DOT, 1_000, 500, rate_pct(10, 100))); // C — liquidated

		set_price(DOT, FixedU128::from_rational(5u128, 100u128));
		let coll_3 = held(DOT, 3);
		assert_ok!(liquidate_with(DOT, 3, |_| LiquidationAllocation {
			offset: OffsetAllocation { recipient: 0, debt: 0, collateral: 0 },
			redistribution_collateral: coll_3,
			keeper: KeeperCompensation { recipient: 3, collateral: 0 },
		}));
		// Restore price so A can borrow.
		set_price(DOT, FixedU128::from_rational(10u128, 1u128));

		// A borrows 200 more. This implicitly touches A (folding redistribution)
		// and then updates the weighted-sum bookkeeping.
		assert_ok!(crate::Pallet::<Test>::borrow(
			RuntimeOrigin::signed(1),
			DOT,
			200,
			None,
			None,
			Position::endpoints_only(),
		));
		// Touch B too so its share is reconciled.
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(99), 2, DOT));

		let bs = BranchStates::<Test>::get(DOT).unwrap();
		let v_a = Vaults::<Test>::get(DOT, 1).unwrap();
		let v_b = Vaults::<Test>::get(DOT, 2).unwrap();
		let expected = weighted(v_a.debt.principal, rate_pct(5, 100))
			.saturating_add(weighted(v_b.debt.principal, rate_pct(50, 100)));
		let actual = bs.debt.weighted_principal_sum;
		assert!(
			actual.abs_diff(expected) <= 10,
			"weighted_sum drift after borrow: actual={}, expected={}",
			actual,
			expected,
		);
	});
}

// Push a vault into FinalRecovery, raise the price so the fully-accrued CR
// goes above MCR, and `poke` it. Exit from FinalRecovery requires an explicit
// hint and is NOT automatic on poke: poke leaves the vault in FinalRecovery,
// and a dedicated `exit_final_recovery` extrinsic does the index re-insert with
// caller-supplied hints.
#[test]
fn final_recovery_exit_requires_explicit_hint() {
	build_and_execute(|| {
		register_default_branch();
		// Single vault so it's the "last eligible redistribution recipient"
		// — `enter_final_recovery` only allows the last stake holder.
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		// Drop the price below MCR so the vault becomes recovery-eligible.
		set_price(DOT, FixedU128::from_rational(5u128, 100u128));
		assert_ok!(crate::Pallet::<Test>::enter_final_recovery(RuntimeOrigin::signed(99), 1, DOT,));
		assert!(matches!(vault_status(DOT, 1), crate::types::VaultStatus::FinalRecovery));
		// Raise the price back so the vault's CR is now ≥ MCR.
		set_price(DOT, FixedU128::from_rational(10u128, 1u128));
		// Poke MUST NOT auto-exit.
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(99), 1, DOT));
		assert!(
			matches!(vault_status(DOT, 1), crate::types::VaultStatus::FinalRecovery),
			"poke must not auto-exit FinalRecovery; exit requires an explicit hint",
		);
		// Explicit `exit_final_recovery` does.
		assert_ok!(crate::Pallet::<Test>::exit_final_recovery(
			RuntimeOrigin::signed(99),
			1,
			DOT,
			Position::endpoints_only(),
		));
		assert!(matches!(vault_status(DOT, 1), crate::types::VaultStatus::Active));
	});
}

// `OffsetAllocation` carries a `recipient` AccountId, and `finalize_liquidation`
// moves `offset.collateral` to that recipient rather than leaking it back to the
// liquidatee.
#[test]
fn finalize_liquidation_doesnt_leak_offset_collateral_to_liquidatee() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(5, 100)));
		set_price(DOT, FixedU128::from_rational(5u128, 100u128));

		let recipient: AccountId = 999;
		let pre_recipient = collateral_balance(DOT, recipient);

		assert_ok!(liquidate_with(DOT, 1, |post_touch| LiquidationAllocation {
			offset: OffsetAllocation { recipient, debt: post_touch, collateral: 500 },
			redistribution_collateral: 0,
			keeper: KeeperCompensation { recipient: 1, collateral: 0 },
		}));

		let post_recipient = collateral_balance(DOT, recipient);
		assert_eq!(
			post_recipient.saturating_sub(pre_recipient),
			500,
			"offset.collateral should land on the offset recipient, not the liquidatee",
		);
	});
}

#[test]
fn back_to_back_near_empty_redistributions_preserve_accounting_identity() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(3, DOT, 5_000, 500, rate_pct(5, 100)));

		set_price(DOT, FixedU128::from_rational(5u128, 100u128));

		for liquidatee in [1u64, 2u64] {
			let coll = held(DOT, liquidatee);
			assert_ok!(liquidate_with(DOT, liquidatee, |_| LiquidationAllocation {
				offset: OffsetAllocation { recipient: 0, debt: 0, collateral: 0 },
				redistribution_collateral: coll,
				keeper: KeeperCompensation { recipient: liquidatee, collateral: 0 },
			}));
			assert_accounting_identity_holds();
		}
	});
}

#[test]
fn vault_cr_view_includes_pending_redistribution() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(3, DOT, 1_000, 500, rate_pct(5, 100)));

		set_price(DOT, FixedU128::from_rational(5u128, 100u128));
		let coll_3 = held(DOT, 3);
		assert_ok!(liquidate_with(DOT, 3, |_| LiquidationAllocation {
			offset: OffsetAllocation { recipient: 0, debt: 0, collateral: 0 },
			redistribution_collateral: coll_3,
			keeper: KeeperCompensation { recipient: 3, collateral: 0 },
		}));
		// Restore price so the view's CR is defined for vault 1.
		set_price(DOT, FixedU128::from_rational(10u128, 1u128));

		let view_pre = crate::Pallet::<Test>::vault_cr(DOT, 1).expect("cr");
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(99), 1, DOT));
		let view_post = crate::Pallet::<Test>::vault_cr(DOT, 1).expect("cr");
		// Pre- and post-touch view should agree: the view replays the same
		// pending-redistribution math `touch_vault` commits.
		assert_eq!(view_pre, view_post);
	});
}

fn assert_accounting_identity_holds() {
	let bs = BranchStates::<Test>::get(DOT).unwrap();
	let cumul = bs.redist.debt_per_stake;
	let mut sum_shares: Balance = 0;
	let mut sum_held: Balance = 0;
	let mut n: u128 = 0;
	for (owner, vault) in Vaults::<Test>::iter_prefix(DOT) {
		let snap = vault.redist_snapshot;
		let delta = cumul.saturating_sub(snap.debt_per_stake);
		sum_shares =
			sum_shares.saturating_add(delta.saturating_mul_int(vault.redistribution_stake));
		let h = held(DOT, owner);
		sum_held = sum_held.saturating_add(h);
		n += 1;
	}
	// Live (Active+Dormant) per-vault stake must equal the branch aggregate.
	// FinalRecovery vaults are zeroed on entry, so summing `redistribution_stake`
	// over all vault rows gives the same answer.
	let sum_stake: Balance =
		Vaults::<Test>::iter_prefix(DOT).map(|(_, v)| v.redistribution_stake).sum();
	assert_eq!(
		bs.stakes.total, sum_stake,
		"stakes.total must equal Σ vault.redistribution_stake of live recipients",
	);
	let _ = sum_held;
	// pending_redist_principal now holds only the recipient-attributable share;
	// per-stake flooring dust lives in rounding.ownerless_pusd_debt separately.
	let tolerance: Balance = n;
	let drift = bs.debt.pending_redist_principal.abs_diff(sum_shares);
	assert!(
		drift <= tolerance,
		"pending redist principal drift: pending={}, sum_shares={}, ownerless={}, drift={}, tol={}",
		bs.debt.pending_redist_principal,
		sum_shares,
		bs.rounding.ownerless_pusd_debt,
		drift,
		tolerance,
	);
}

#[test]
fn touch_does_not_revive_dormant_when_interest_lifts_above_min_debt() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 100_000, 500, rate_pct(50, 100))); // co-recipient
		assert_ok!(open(2, DOT, 10_000, 500, rate_pct(50, 100))); // target

		// Reduce vault 2 to a small dust principal (well under MinimumDebt=200)
		// via a redemption cancel so the vault becomes Dormant with non-zero
		// residual debt.
		use pusd_primitives::{RedemptionAllocation, VaultRedemptionInterface};
		assert_ok!(<crate::Pallet<Test> as VaultRedemptionInterface<_, _, _>>::apply_redemption(
			DOT,
			2,
			99,
			RedemptionAllocation {
				debt_to_cancel: 450,
				collateral_to_redeemer: 9_000,
				fee_collateral_retained: 0,
			},
		));
		assert!(crate::Pallet::<Test>::vault_status(DOT, 2).unwrap().is_dormant());

		// Advance time so that simple interest at 50% APR pushes the residual
		// principal back over MinimumDebt=200.
		advance_time(ONE_YEAR_MS * 10);
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(99), 2, DOT));

		// Dormant status is sticky: passive accrual never re-indexes a vault.
		// Even though the debt has crossed MinimumDebt again, the vault stays
		// Dormant until an explicit, hint-bearing activation (`borrow` /
		// `activate_dormant`).
		assert!(
			Vaults::<Test>::get(DOT, 2).unwrap().debt.total() >= 200,
			"sanity: accrual should have lifted residual debt back over MinimumDebt",
		);
		assert!(
			crate::Pallet::<Test>::vault_status(DOT, 2).unwrap().is_dormant(),
			"poke must NOT auto-revive a Dormant vault; re-entry requires an explicit hint",
		);
		let bs = BranchStates::<Test>::get(DOT).unwrap();
		assert_eq!(
			bs.last_dormant_vault_owner,
			Some(2),
			"the dormant slot is retained; nothing revived the vault",
		);
	});
}

#[test]
fn redistribution_residue_lands_in_ownerless_pusd_debt() {
	build_and_execute(|| {
		register_default_branch();
		// Two recipients with stakes that do not evenly divide an
		// arbitrary redistributed debt, guaranteeing per-stake floor residue.
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 999, 500, rate_pct(5, 100)));
		assert_ok!(open(3, DOT, 5_000, 500, rate_pct(5, 100))); // liquidatee

		let pre_owner = BranchStates::<Test>::get(DOT).unwrap().rounding.ownerless_pusd_debt;
		set_price(DOT, FixedU128::from_rational(5u128, 100u128));
		// Force a full redistribution path: no offset, all debt redistributed.
		assert_ok!(liquidate_with(DOT, 3, |_post_touch| LiquidationAllocation {
			offset: OffsetAllocation { recipient: 0, debt: 0, collateral: 0 },
			redistribution_collateral: 0,
			keeper: KeeperCompensation { recipient: 3, collateral: 0 },
		}));

		let bs = BranchStates::<Test>::get(DOT).unwrap();
		assert!(
			bs.rounding.ownerless_pusd_debt > pre_owner,
			"per-stake flooring of an indivisible redistribution must surface in ownerless_pusd_debt",
		);
	});
}

// Full-lifecycle identity soak: open → liquidate with a redistribution split
// → recipient touches → partial repay → redemption → overpay-close. The
// `try_state` identities (Σ principal exact, Σ floor(rate·stake) exact,
// weighted-principal bounds) must hold at every stage, not just at the end.
#[test]
fn full_lifecycle_holds_branch_identities() {
	fn assert_identities() {
		#[cfg(feature = "try-runtime")]
		crate::try_state::do_try_state::<Test>().expect("branch identities hold");
	}
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 2_000, 800, rate_pct(25, 100)));
		assert_ok!(open(3, DOT, 3_000, 1_000, rate_pct(50, 100)));
		assert_identities();

		// A month of accrual so touches materialise real interest.
		advance_time(30 * 24 * 3_600 * 1_000);

		// Liquidate vault 1 with a genuine three-way split: one third offset,
		// the rest redistributed, plus keeper compensation.
		set_price(DOT, FixedU128::from_rational(55u128, 100u128));
		assert_ok!(liquidate_with(DOT, 1, |post_touch| LiquidationAllocation {
			offset: OffsetAllocation { recipient: 9, debt: post_touch / 3, collateral: 100 },
			redistribution_collateral: 500,
			keeper: KeeperCompensation { recipient: 8, collateral: 10 },
		}));
		assert_identities();

		// A recipient touch absorbs its redistribution share.
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(9), 2, DOT));
		assert_identities();

		// Partial repay exercises the full-contribution weighted-sum swap.
		assert_ok!(crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(2), 2, DOT, 300));
		assert_identities();

		// Redemption against the cheapest vault at a healthy price.
		set_price(DOT, FixedU128::from_rational(10u128, 1u128));
		assert_ok!(redeem(DOT, 7, 400));
		assert_identities();

		// Touch the remaining whale, then close it by overpaying.
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(9), 3, DOT));
		assert_identities();
		let _ = <Pusd as frame::deps::frame_support::traits::fungible::Mutate<u64>>::transfer(
			&1,
			&3,
			pusd_balance(1),
			frame::deps::frame_support::traits::tokens::Preservation::Expendable,
		);
		assert_ok!(crate::Pallet::<Test>::repay_for(
			RuntimeOrigin::signed(3),
			3,
			DOT,
			pusd_balance(3)
		));
		assert!(crate::pallet::Vaults::<Test>::get(DOT, 3).is_none(), "vault 3 closed");
		assert_identities();
	});
}

// Epoch-relative debt-time accounting: interest on a redistributed share
// accrues from the liquidation moment t1, not from the branch epoch or any
// absolute origin. Liquidate at t1, touch the recipient at t2, and the
// redistribution part of the accrued interest must equal
// `share · rate · (t2 - t1) / year` to within fixed-point flooring.
#[test]
fn redistributed_principal_accrues_interest_from_liquidation_moment() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 2_000, 800, rate_pct(50, 100)));
		// Age the branch so the epoch and t1 are well separated.
		advance_time(10 * 24 * 3_600 * 1_000);
		// Settle vault 2's own interest at t1 so the t2 delta decomposes into
		// "own principal interest" + "redistribution interest" cleanly.
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(9), 2, DOT));

		// Fully redistribute vault 1's debt at t1.
		set_price(DOT, FixedU128::from_rational(55u128, 100u128));
		let mut redistributed: Balance = 0;
		assert_ok!(liquidate_with(DOT, 1, |post_touch| {
			redistributed = post_touch;
			LiquidationAllocation {
				offset: OffsetAllocation { recipient: 9, debt: 0, collateral: 0 },
				redistribution_collateral: 0,
				keeper: KeeperCompensation { recipient: 8, collateral: 0 },
			}
		}));
		let v_pre = Vaults::<Test>::get(DOT, 2).expect("vault 2 stored");

		let elapsed: u128 = 30 * 24 * 3_600 * 1_000;
		advance_time(elapsed as Moment);
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(9), 2, DOT));
		let v_post = Vaults::<Test>::get(DOT, 2).expect("vault 2 stored");

		// Vault 2 is the only recipient: per-stake quantization loses at most
		// one unit of the redistributed principal.
		let share = v_post.debt.principal - v_pre.debt.principal;
		assert!(share > 0);
		assert!(redistributed.abs_diff(share) <= 1, "share ≈ full redistributed debt");

		// Both expectations follow `floor(P · rate · Δt / year)` with the 50%
		// rate folded as a halving.
		let own_expected = v_pre.debt.principal * elapsed / 2 / u128::from(ONE_YEAR_MS);
		let redist_expected = share * elapsed / 2 / u128::from(ONE_YEAR_MS);
		let interest_delta = v_post.debt.interest - v_pre.debt.interest;
		assert!(interest_delta >= own_expected);
		let redist_part = interest_delta - own_expected;
		assert!(
			redist_part.abs_diff(redist_expected) <= 2,
			"redistribution interest accrues from t1: got {redist_part}, want ≈{redist_expected}"
		);
	});
}
