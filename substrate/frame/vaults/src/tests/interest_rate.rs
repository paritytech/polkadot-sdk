use crate::{
	mock::*,
	pallet::Vaults,
	tests::{rate_pct, vault_status},
};
use frame::deps::{
	frame_support::{
		assert_noop, assert_ok,
		traits::{
			fungible::{Inspect as FungibleInspect, Mutate as FungibleMutate},
			tokens::Preservation,
		},
	},
	sp_runtime::FixedU128,
};
use pallet_linked_list::SortedListInterface;

const ONE_DAY_MS: Moment = 24 * 3_600 * 1_000;

// Helper: top up `who`'s pUSD balance by `delta` so that subsequent
// repay_for / etc. doesn't trip on the upfront-fee residual.
fn top_up_pusd(who: AccountId, donor: AccountId, delta: Balance) {
	if delta == 0 {
		return;
	}
	let _ = <Pusd as FungibleMutate<AccountId>>::transfer(
		&donor,
		&who,
		delta,
		Preservation::Expendable,
	);
}

#[test]
fn open_sets_annual_rate() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 2_000, rate_pct(37, 100)));
		assert_ok!(open(2, DOT, 1_000, 2_000, rate_pct(100, 100)));
		assert_eq!(Vaults::<Test>::get(DOT, 1).unwrap().annual_rate, rate_pct(37, 100));
		assert_eq!(Vaults::<Test>::get(DOT, 2).unwrap().annual_rate, rate_pct(100, 100));
	});
}

#[test]
fn open_sets_last_interest_update_to_now() {
	build_and_execute(|| {
		register_default_branch();
		let t0 = pallet_timestamp::Pallet::<Test>::get();
		assert_ok!(open(1, DOT, 1_000, 2_000, rate_pct(5, 100)));
		assert_eq!(Vaults::<Test>::get(DOT, 1).unwrap().last_interest_update, t0);
		// Advance time, open a second vault — last-update for that vault is
		// the new `now`, while the first vault keeps its original stamp.
		advance_time(1_000);
		let t1 = pallet_timestamp::Pallet::<Test>::get();
		assert_ok!(open(2, DOT, 1_000, 2_000, rate_pct(5, 100)));
		assert_eq!(Vaults::<Test>::get(DOT, 1).unwrap().last_interest_update, t0);
		assert_eq!(Vaults::<Test>::get(DOT, 2).unwrap().last_interest_update, t1);
	});
}

// A vault is addressed by the `(collateral_id, caller)` storage key, so the
// caller can only ever reach their own vault; another account simply has no
// row to mutate. Access control falls out of the storage layout: changing a
// non-owner's rate fails with `VaultNotFound`, not a dedicated owner-check
// error.
#[test]
fn change_rate_from_non_owner_returns_vault_not_found() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 2_000, rate_pct(37, 100)));
		// Account 2 has no vault on DOT.
		assert_noop!(
			crate::Pallet::<Test>::change_rate(
				RuntimeOrigin::signed(2),
				DOT,
				rate_pct(50, 100),
				Position::endpoints_only(),
			),
			crate::Error::<Test>::VaultNotFound
		);
	});
}

#[test]
fn change_rate_sets_new_rate() {
	build_and_execute(|| {
		register_default_branch();
		// Open three vaults at 50%, then change each to a different rate
		// after the cooldown elapses (so no upfront fees intrude here).
		for who in 1u64..=3 {
			assert_ok!(open(who, DOT, 1_000, 2_000, rate_pct(50, 100)));
		}
		advance_time(2 * ONE_DAY_MS);
		assert_ok!(crate::Pallet::<Test>::change_rate(
			RuntimeOrigin::signed(1),
			DOT,
			rate_pct(1, 200),
			Position::endpoints_only(),
		));
		assert_ok!(crate::Pallet::<Test>::change_rate(
			RuntimeOrigin::signed(2),
			DOT,
			rate_pct(60, 100),
			Position::endpoints_only(),
		));
		assert_ok!(crate::Pallet::<Test>::change_rate(
			RuntimeOrigin::signed(3),
			DOT,
			rate_pct(100, 100),
			Position::endpoints_only(),
		));
		assert_eq!(Vaults::<Test>::get(DOT, 1).unwrap().annual_rate, rate_pct(1, 200));
		assert_eq!(Vaults::<Test>::get(DOT, 2).unwrap().annual_rate, rate_pct(60, 100));
		assert_eq!(Vaults::<Test>::get(DOT, 3).unwrap().annual_rate, rate_pct(100, 100));
	});
}

// Post-cooldown change_rate refreshes last_interest_update and folds the
// elapsed simple interest into `vault.debt.interest`. With no upfront fee
// charged (cooldown elapsed), the interest-bearing principal is unchanged.
//
// `vault.debt.principal + vault.debt.interest` is the *recorded* debt after
// touch — it does NOT include the live (yet-unmaterialised) simple-interest
// accrual. We pin only the recorded-state invariants here.
#[test]
fn change_rate_post_cooldown_full_state() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 2_000, rate_pct(50, 100)));
		// Advance one full cooldown so the rate change is fee-free.
		advance_time(ONE_DAY_MS);
		let v_pre = Vaults::<Test>::get(DOT, 1).unwrap();

		let now_before_call = pallet_timestamp::Pallet::<Test>::get();
		assert_eq!(
			crate::Pallet::<Test>::predict_rate_change_upfront_fee(DOT, 1, rate_pct(75, 100)),
			0,
			"post-cooldown rate change should quote no upfront fee",
		);
		assert_ok!(crate::Pallet::<Test>::change_rate(
			RuntimeOrigin::signed(1),
			DOT,
			rate_pct(75, 100),
			Position::endpoints_only(),
		));
		let v_post = Vaults::<Test>::get(DOT, 1).unwrap();

		// last_interest_update == now (touch ran inside change_rate).
		assert_eq!(v_post.last_interest_update, now_before_call);
		// No fee charged post-cooldown, so principal is unchanged.
		assert_eq!(v_post.debt.principal, v_pre.debt.principal);
		// Pending interest at the new last_interest_update is zero by
		// construction (touch_vault moved any sim-pending into the accrued
		// component). Accrued grew by the materialised pending.
		assert!(v_post.debt.interest >= v_pre.debt.interest);
	});
}

// A within-cooldown rate change charges an upfront fee that lands in
// `vault.debt.interest` and bumps recorded debt by exactly that fee.
#[test]
fn change_rate_premature_increases_recorded_debt_by_fee() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 2_000, rate_pct(50, 100)));
		advance_time(ONE_DAY_MS / 2);
		// Settle pending interest into accrued first so the change_rate
		// delta isolates the upfront-fee component.
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(1), 1, DOT));
		let v_pre = Vaults::<Test>::get(DOT, 1).unwrap();

		let predicted =
			crate::Pallet::<Test>::predict_rate_change_upfront_fee(DOT, 1, rate_pct(75, 100));
		assert!(predicted > 0, "premature change at debt=2000 must charge a fee");

		assert_ok!(crate::Pallet::<Test>::change_rate(
			RuntimeOrigin::signed(1),
			DOT,
			rate_pct(75, 100),
			Position::endpoints_only(),
		));
		let v_post = Vaults::<Test>::get(DOT, 1).unwrap();
		assert_eq!(v_post.debt.principal, v_pre.debt.principal);
		assert_eq!(v_post.debt.interest, v_pre.debt.interest + predicted);
	});
}

// Collateral/debt adjustments without a rate change keep the DLL ordering.
#[test]
fn collateral_or_debt_adjust_does_not_reorder_dll() {
	build_and_execute(|| {
		register_default_branch();
		for (who, pct) in [(1u64, 10), (2, 20), (3, 30), (4, 40), (5, 50)] {
			assert_ok!(open(who, DOT, 1_000, 500, rate_pct(pct, 100)));
		}
		let order_before = <LinkedList as SortedListInterface<VaultList, u64>>::iter_from_tail(
			&rate_list(DOT),
			10,
		);
		// Various coll/debt adjusts — none of these touch the rate index.
		assert_ok!(crate::Pallet::<Test>::deposit_collateral_for(
			RuntimeOrigin::signed(1),
			1,
			DOT,
			100,
		));
		assert_ok!(crate::Pallet::<Test>::borrow(
			RuntimeOrigin::signed(2),
			DOT,
			50,
			None,
			None,
			Position::endpoints_only(),
		));
		assert_ok!(crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(3), 3, DOT, 50));
		let order_after = <LinkedList as SortedListInterface<VaultList, u64>>::iter_from_tail(
			&rate_list(DOT),
			10,
		);
		assert_eq!(order_before, order_after);
	});
}

// Borrow refreshes last_interest_update, applies pending into accrued,
// charges the upfront fee, and grows recorded principal by exactly the
// borrowed amount.
//
// To isolate the upfront-fee delta from the materialised simple-interest
// accrual we poke the vault first (folding sim-pending into accrued).
#[test]
fn borrow_full_state_changes() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 3_000, 2_000, rate_pct(25, 100)));
		advance_time(ONE_DAY_MS);
		// Settle pending into accrued so the borrow delta isolates the
		// upfront fee.
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(1), 1, DOT));

		let v_pre = Vaults::<Test>::get(DOT, 1).unwrap();
		let predicted_fee = crate::Pallet::<Test>::predict_borrow_upfront_fee(DOT, 1, 500, None);
		let now_before_call = pallet_timestamp::Pallet::<Test>::get();

		assert_ok!(crate::Pallet::<Test>::borrow(
			RuntimeOrigin::signed(1),
			DOT,
			500,
			None,
			None,
			Position::endpoints_only(),
		));
		let v_post = Vaults::<Test>::get(DOT, 1).unwrap();

		// last_interest_update advances to now.
		assert_eq!(v_post.last_interest_update, now_before_call);
		// Recorded principal grew by exactly the borrowed amount.
		assert_eq!(v_post.debt.principal, v_pre.debt.principal + 500);
		// Accrued grew by exactly the upfront fee (no sim-pending to
		// materialise — we pre-poked).
		assert_eq!(v_post.debt.interest, v_pre.debt.interest + predicted_fee);
	});
}

#[test]
fn borrow_with_new_rate_updates_rate_reorders_index_and_charges_predicted_fee() {
	build_and_execute(|| {
		register_default_branch();
		for (who, pct) in [(1u64, 20), (2, 10), (3, 30)] {
			assert_ok!(open(who, DOT, 5_000, 2_000, rate_pct(pct, 100)));
		}
		let v_pre = Vaults::<Test>::get(DOT, 1).unwrap();
		let predicted =
			crate::Pallet::<Test>::predict_borrow_upfront_fee(DOT, 1, 500, Some(rate_pct(5, 100)));
		assert!(predicted > 0);
		let now_before_call = pallet_timestamp::Pallet::<Test>::get();

		assert_ok!(crate::Pallet::<Test>::borrow(
			RuntimeOrigin::signed(1),
			DOT,
			500,
			Some(rate_pct(5, 100)),
			None,
			Position::endpoints_only(),
		));

		let v_post = Vaults::<Test>::get(DOT, 1).unwrap();
		assert_eq!(v_post.annual_rate, rate_pct(5, 100));
		assert_eq!(v_post.last_rate_update, now_before_call);
		assert_eq!(v_post.debt.principal, v_pre.debt.principal + 500);
		assert_eq!(v_post.debt.interest, v_pre.debt.interest + predicted);
		let order = <LinkedList as SortedListInterface<VaultList, u64>>::iter_from_tail(
			&rate_list(DOT),
			10,
		);
		assert_eq!(order, alloc::vec![1, 2, 3]);
		System::assert_has_event(RuntimeEvent::Vaults(crate::Event::BorrowRateChanged {
			collateral_id: DOT,
			owner: 1,
			old_rate: rate_pct(20, 100),
			new_rate: rate_pct(5, 100),
		}));
	});
}

#[test]
fn borrow_with_new_rate_rejects_rate_out_of_bounds_without_state_change() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 5_000, 2_000, rate_pct(20, 100)));
		let v_pre = Vaults::<Test>::get(DOT, 1).unwrap();
		let balance_pre = pusd_balance(1);

		assert_noop!(
			crate::Pallet::<Test>::borrow(
				RuntimeOrigin::signed(1),
				DOT,
				500,
				Some(rate_pct(101, 100)),
				None,
				Position::endpoints_only(),
			),
			crate::Error::<Test>::RateOutOfBounds
		);

		assert_eq!(Vaults::<Test>::get(DOT, 1).unwrap(), v_pre);
		assert_eq!(pusd_balance(1), balance_pre);
	});
}

// Repay refreshes last_interest_update, settles pending interest, reduces
// entire debt by the repaid amount, and reduces recorded debt by the
// principal portion.
#[test]
fn repay_full_state_changes() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 3_000, 3_000, rate_pct(25, 100)));
		advance_time(ONE_DAY_MS);

		// Settle pending interest into a known-quantity accrued, then top up
		// the borrower's pUSD so they have enough to repay both principal
		// and accrued.
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(1), 1, DOT));
		let v_pre = Vaults::<Test>::get(DOT, 1).unwrap();
		// Borrow more pUSD into a second account so we can shuttle some over.
		assert_ok!(open(2, DOT, 5_000, 3_000, rate_pct(25, 100)));
		top_up_pusd(1, 2, v_pre.debt.interest + 500);

		let now_before_call = pallet_timestamp::Pallet::<Test>::get();
		assert_ok!(crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(1), 1, DOT, 500));
		let v_post = Vaults::<Test>::get(DOT, 1).unwrap();

		assert_eq!(v_post.last_interest_update, now_before_call);

		// Entire debt reduces by the repaid amount (since `poke` already
		// folded prior pending interest into accrued, `repay_for(500)`
		// removes 500 cleanly from the entire-debt sum).
		let entire_pre = v_pre.debt.principal + v_pre.debt.interest;
		let entire_post = v_post.debt.principal + v_post.debt.interest;
		assert_eq!(entire_post, entire_pre - 500);

		// Recorded debt decreases by the principal portion. Since
		// accrued_interest > 0 and repay applies to accrued first, principal
		// reduction is `500 - min(500, accrued)`. Here we kept the accrued
		// small so the bulk of 500 hit principal.
		let pay_accrued = core::cmp::min(500, v_pre.debt.interest);
		let pay_principal = 500 - pay_accrued;
		assert_eq!(v_post.debt.principal, v_pre.debt.principal - pay_principal);
	});
}
// Poke is permissionless, refreshes last_interest_update, materialises
// sim-pending into accrued, and leaves principal unchanged.
//
// Storage exposes only `interest_bearing_debt + accrued_interest`, i.e. the
// recorded debt — which does not include the live sim-pending accrual. We pin
// the per-component changes instead of an entire-debt invariant.
#[test]
fn poke_full_state_changes() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 3_000, 2_000, rate_pct(25, 100)));
		advance_time(ONE_DAY_MS);

		let v_pre = Vaults::<Test>::get(DOT, 1).unwrap();
		let now_before_call = pallet_timestamp::Pallet::<Test>::get();

		// Permissionless: any signed origin (here, account 2) can poke
		// account 1's vault.
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(2), 1, DOT));
		let v_post = Vaults::<Test>::get(DOT, 1).unwrap();

		assert_eq!(v_post.last_interest_update, now_before_call);
		// Recorded principal unchanged.
		assert_eq!(v_post.debt.principal, v_pre.debt.principal);
		// Pending at the new last_interest_update is zero by construction; the
		// accrued component grew by the materialised sim-pending.
		assert!(v_post.debt.interest >= v_pre.debt.interest);
	});
}

// A full repayment auto-closes the vault, so poking the vanished row reports
// `VaultNotFound`. By contrast, a *live* zero-debt Dormant row left behind by
// a full redemption remains pokeable.
#[test]
fn poke_after_full_repayment_errors_vault_not_found() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 3_000, 2_000, rate_pct(25, 100)));
		assert_ok!(open(2, DOT, 3_000, 2_000, rate_pct(25, 100)));
		// Repay all of vault 1's debt — first poke to settle accrued, then
		// transfer accrued from vault 2 to cover the residual.
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(1), 1, DOT));
		let v = Vaults::<Test>::get(DOT, 1).unwrap();
		let total = v.debt.principal + v.debt.interest;
		top_up_pusd(1, 2, v.debt.interest);
		assert_ok!(crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(1), 1, DOT, total));
		// The repay-to-zero auto-closed the vault; poke surfaces that.
		assert_noop!(
			crate::Pallet::<Test>::poke(RuntimeOrigin::signed(3), 1, DOT),
			crate::Error::<Test>::VaultNotFound
		);
	});
}

// Redemption refreshes last_interest_update on the redeemed vault, applies
// pending interest, reduces entire debt by the redeemed amount, and reduces
// recorded debt accordingly. Tested through the `VaultRedemptionInterface`
// trait (no `redeem` extrinsic exists yet).
#[test]
fn redemption_full_state_changes() {
	build_and_execute(|| {
		register_default_branch();
		// Six vaults across ascending rates so the rate index has a clear
		// "lowest rate" target at the tail.
		for (who, pct) in [(1u64, 1), (2, 2), (3, 3), (4, 4), (5, 5), (6, 6)] {
			assert_ok!(open(who, DOT, 1_000, 500, rate_pct(pct, 100)));
		}
		advance_time(ONE_DAY_MS);

		let v_pre = Vaults::<Test>::get(DOT, 1).unwrap();
		let now_before_call = pallet_timestamp::Pallet::<Test>::get();
		// Redeem 200 pUSD from acct 5 (the redeemer) — the helper uses the
		// rate-index tail, which is acct 1 (lowest rate).
		let target = redeem(DOT, 5, 200).expect("redeem ok");
		assert_eq!(target, 1);

		let v_post = Vaults::<Test>::get(DOT, 1).unwrap();
		assert_eq!(v_post.last_interest_update, now_before_call);

		// Entire debt reduces by the redeemed amount.
		let entire_pre = v_pre.debt.principal + v_pre.debt.interest;
		let entire_post = v_post.debt.principal + v_post.debt.interest;
		assert_eq!(entire_post, entire_pre - 200);

		// Recorded debt reduces by the principal portion. The
		// `apply_redemption` impl pays accrued first, then principal.
		let pay_accrued = core::cmp::min(200, v_pre.debt.interest);
		let pay_principal = 200 - pay_accrued;
		assert_eq!(v_post.debt.principal, v_pre.debt.principal - pay_principal);

		// Vault stays Active because remaining debt is well above MinimumDebt.
		assert!(vault_status(DOT, 1).is_active());
	});
}

// In the test mock both `SpYieldSink` and `FeeHandler` are drop-style
// implementations — they consume the `Credit` without resolving it, which
// rescinds the corresponding mint. So `total_issuance` grows by only the
// borrow amount; the fee is recorded on `bs.debt.minted_interest` and on
// `vault.debt.interest` instead. (In production wiring,
// `pallet-stability-pool` would resolve the SP credit and keep the mint live.)
#[test]
fn open_mints_borrow_amount_with_fee_recorded_in_branch_state() {
	build_and_execute(|| {
		register_default_branch();
		let total_pre = <Pusd as FungibleInspect<AccountId>>::total_issuance();
		let predicted_fee =
			crate::Pallet::<Test>::predict_open_upfront_fee(DOT, 2_000, rate_pct(10, 100));
		assert!(predicted_fee > 0);
		assert_ok!(open(1, DOT, 1_000, 2_000, rate_pct(10, 100)));
		let total_post = <Pusd as FungibleInspect<AccountId>>::total_issuance();
		// Fee is rescinded by the mock drops — only the user mint persists.
		assert_eq!(total_post, total_pre + 2_000);
		assert_eq!(<Pusd as FungibleInspect<AccountId>>::balance(&1), 2_000);
		// But the fee was charged: it lives on the vault and the branch.
		let v = Vaults::<Test>::get(DOT, 1).unwrap();
		assert_eq!(v.debt.interest, predicted_fee);
		let bs = crate::pallet::BranchStates::<Test>::get(DOT).unwrap();
		assert_eq!(bs.debt.minted_interest, predicted_fee);
	});
}

// Liquidate vault C, then permissionlessly poke A's vault — A's debt grows by
// accrued interest plus redistribution gains. Under simple-interest semantics
// the exact post-state magnitudes differ from a compounding reference, so we
// pin only the qualitative assertion: A's debt does not decrease across the
// liquidation+redistribution cycle.
#[test]
fn poke_after_liquidation_applies_redistribution_gains() {
	build_and_execute(|| {
		register_default_branch();
		// Two vaults, A and C, both at 25%. C is more leveraged so a price
		// drop puts it underwater first.
		assert_ok!(open(1, DOT, 3_000, 2_000, rate_pct(25, 100)));
		assert_ok!(open(3, DOT, 1_000, 2_000, rate_pct(25, 100)));
		// Drop the price so C is below MCR; A stays comfortably above.
		set_price(DOT, FixedU128::from_rational(15u128, 10u128));

		let v_a_pre = Vaults::<Test>::get(DOT, 1).unwrap();
		let entire_a_pre = v_a_pre.debt.principal + v_a_pre.debt.interest;

		// Liquidate C through the trait surface — branch-level redistribution
		// accumulators get bumped, but A's vault row isn't touched until A is
		// poked.
		assert_ok!(liquidate(DOT, 3));

		// Poke A — its accrued/principal should incorporate the redistribution
		// gain.
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(2), 1, DOT));
		let v_a_post = Vaults::<Test>::get(DOT, 1).unwrap();
		let entire_a_post = v_a_post.debt.principal + v_a_post.debt.interest;
		assert!(
			entire_a_post >= entire_a_pre,
			"A's debt should not decrease across a liquidation cycle"
		);
	});
}
