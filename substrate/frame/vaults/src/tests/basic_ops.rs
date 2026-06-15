use crate::{
	mock::*,
	pallet::Vaults,
	tests::{rate_pct, vault_status},
};
use frame::deps::frame_support::{assert_noop, assert_ok};
use pallet_linked_list::SortedListInterface;

// Opening a vault from an account whose free balance is below the requested
// collateral fails at the token layer: the `fungible::hold` call returns an
// error. Account 100 is not funded by genesis (only 1..=10 are).
#[test]
fn open_vault_fails_without_balance() {
	build_and_execute(|| {
		register_default_branch();
		assert!(open(100, DOT, 1_000, 500, rate_pct(5, 100)).is_err());
	});
}

// There is no single `adjust_vault` extrinsic — adjustments are applied
// through the per-action dispatchables (`deposit_collateral_for`, `borrow`,
// `withdraw_collateral`, `repay_for`). This exercises a +collateral then
// +debt sequence and asserts the end-state shows both deltas.
#[test]
fn adjust_vault_via_deposit_then_borrow() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		// +200 collateral.
		assert_ok!(crate::Pallet::<Test>::deposit_collateral_for(
			RuntimeOrigin::signed(1),
			1,
			DOT,
			200,
		));
		// +300 debt (no rate change).
		assert_ok!(crate::Pallet::<Test>::borrow(
			RuntimeOrigin::signed(1),
			DOT,
			300,
			None,
			None,
			Position::endpoints_only(),
		));
		assert_eq!(held(DOT, 1), 1_200);
		let v = Vaults::<Test>::get(DOT, 1).expect("vault stored");
		assert_eq!(v.debt.principal, 800);
		// pUSD net to user: initial 500 + 300 borrowed (fees go to fee handler
		// dropper, not the user).
		assert_eq!(pusd_balance(1), 800);
	});
}

#[test]
fn borrow_with_recipient_mints_to_recipient_not_owner() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 2_000, 500, rate_pct(5, 100)));
		let owner_pre = pusd_balance(1);
		let recipient_pre = pusd_balance(4);

		assert_ok!(crate::Pallet::<Test>::borrow(
			RuntimeOrigin::signed(1),
			DOT,
			300,
			None,
			Some(4),
			Position::endpoints_only(),
		));

		assert_eq!(pusd_balance(1), owner_pre);
		assert_eq!(pusd_balance(4), recipient_pre + 300);
		let v = Vaults::<Test>::get(DOT, 1).expect("vault stored");
		assert_eq!(v.debt.principal, 800);
	});
}

#[test]
fn withdraw_collateral_with_recipient_transfers_to_recipient() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 3_000, 500, rate_pct(5, 100)));
		let recipient_pre = collateral_balance(DOT, 4);

		assert_ok!(crate::Pallet::<Test>::withdraw_collateral(
			RuntimeOrigin::signed(1),
			DOT,
			250,
			Some(4),
		));

		assert_eq!(held(DOT, 1), 2_750);
		assert_eq!(collateral_balance(DOT, 4), recipient_pre + 250);
	});
}

#[test]
fn repay_for_by_third_party_burns_payer_balance_and_updates_owner_vault() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 2_000, 500, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 2_000, 500, rate_pct(5, 100)));
		let payer_pre = pusd_balance(2);
		let v_pre = Vaults::<Test>::get(DOT, 1).expect("vault stored");

		assert_ok!(crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(2), 1, DOT, 100));

		assert_eq!(pusd_balance(2), payer_pre - 100);
		let v_post = Vaults::<Test>::get(DOT, 1).expect("vault stored");
		assert_eq!(v_post.debt.total(), v_pre.debt.total() - 100);
	});
}

#[test]
fn close_vault_with_recipient_releases_collateral_to_recipient() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		let v = Vaults::<Test>::get(DOT, 1).expect("vault stored");
		let total = v.debt.total();
		assert_eq!(redeem(DOT, 3, total).expect("redeem ok"), 1);
		assert!(vault_status(DOT, 1).is_dormant());

		let residual = held(DOT, 1);
		let recipient_pre = collateral_balance(DOT, 4);
		assert_ok!(crate::Pallet::<Test>::close_vault(RuntimeOrigin::signed(1), DOT, Some(4)));

		assert!(Vaults::<Test>::get(DOT, 1).is_none());
		assert_eq!(held(DOT, 1), 0);
		assert_eq!(collateral_balance(DOT, 4), recipient_pre + residual);
	});
}

// A `repay_for` that brings debt to zero closes the vault in
// the same op — removes it from the rate index, releases held collateral to
// the owner, deletes the Vaults row, and emits VaultClosed.
#[test]
fn repay_for_to_zero_closes_active_vault() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(5, 100)));
		let v = Vaults::<Test>::get(DOT, 1).expect("vault stored");
		let total = v.debt.principal + v.debt.interest;
		let _ = <Pusd as frame::deps::frame_support::traits::fungible::Mutate<u64>>::transfer(
			&2,
			&1,
			v.debt.interest,
			frame::deps::frame_support::traits::tokens::Preservation::Expendable,
		);
		let coll_before = collateral_balance(DOT, 1);
		assert_ok!(crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(1), 1, DOT, total));
		assert!(Vaults::<Test>::get(DOT, 1).is_none(), "vault row removed");
		assert_eq!(held(DOT, 1), 0, "held collateral released");
		assert_eq!(
			collateral_balance(DOT, 1),
			coll_before + 1_000,
			"owner received the collateral"
		);
		assert!(
			!<LinkedList as SortedListInterface<VaultList, u64>>::contains(&rate_list(DOT), &1),
			"vault removed from rate index"
		);
		System::assert_has_event(crate::mock::RuntimeEvent::Vaults(crate::Event::VaultClosed {
			collateral_id: DOT,
			owner: 1,
			recipient: 1,
		}));
	});
}

// Poking a nonexistent vault is an error, not a silent success — a typo'd
// owner must not look like a completed refresh.
#[test]
fn poke_missing_vault_errors() {
	build_and_execute(|| {
		register_default_branch();
		assert_noop!(
			crate::Pallet::<Test>::poke(RuntimeOrigin::signed(1), 99, DOT),
			crate::Error::<Test>::VaultNotFound
		);
	});
}

// `repay_for` caps at the outstanding debt: over-asking burns only what is
// owed, closes the vault, and `Repaid` (with the actual amount) precedes
// `VaultClosed`.
#[test]
fn repay_overpay_burns_only_debt_and_closes() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(5, 100)));
		// Acct 2's minted pUSD funds the surplus over acct 1's own balance.
		let _ = <Pusd as frame::deps::frame_support::traits::fungible::Mutate<u64>>::transfer(
			&2,
			&1,
			400,
			frame::deps::frame_support::traits::tokens::Preservation::Expendable,
		);
		let v = Vaults::<Test>::get(DOT, 1).expect("vault stored");
		let total = v.debt.principal + v.debt.interest;
		let balance_before = pusd_balance(1);
		assert!(balance_before > total, "overpay setup needs a surplus");

		assert_ok!(crate::Pallet::<Test>::repay_for(
			RuntimeOrigin::signed(1),
			1,
			DOT,
			balance_before
		));

		assert_eq!(pusd_balance(1), balance_before - total, "only the debt burned");
		assert!(Vaults::<Test>::get(DOT, 1).is_none(), "vault closed");
		let events = System::events();
		let repaid = events.iter().position(|r| {
			matches!(
				&r.event,
				RuntimeEvent::Vaults(crate::Event::Repaid { amount, .. }) if *amount == total
			)
		});
		let closed = events.iter().position(|r| {
			matches!(&r.event, RuntimeEvent::Vaults(crate::Event::VaultClosed { .. }))
		});
		assert!(repaid.expect("Repaid emitted") < closed.expect("VaultClosed emitted"));
	});
}

// The cap turns full repayment of a sub-minimum Dormant residual from an
// exact-amount guessing game into "send at least the dust".
#[test]
fn repay_overpay_rescues_subminimum_dormant_vault() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		// Push acct 1 to Dormant with a small residual debt.
		assert_ok!(redeem(DOT, 3, 350));
		assert!(vault_status(DOT, 1).is_dormant());
		let v = Vaults::<Test>::get(DOT, 1).expect("dormant vault stored");
		let residual = v.debt.principal + v.debt.interest;
		assert!(residual > 0);
		assert!(residual < 200, "residual sits below MinimumDebt");

		let balance_before = pusd_balance(1);
		assert_ok!(crate::Pallet::<Test>::repay_for(
			RuntimeOrigin::signed(1),
			1,
			DOT,
			balance_before
		));

		assert_eq!(pusd_balance(1), balance_before - residual);
		assert!(Vaults::<Test>::get(DOT, 1).is_none());
	});
}

// A redemption-driven dormant
// residual that is repaid to zero auto-closes (and clears any matching
// `last_dormant_vault_owner` pointer).
#[test]
fn repay_for_to_zero_closes_dormant_vault() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		// Push acct 1 to Dormant with a small residual debt.
		assert_ok!(redeem(DOT, 3, 350));
		assert!(vault_status(DOT, 1).is_dormant());
		let v = Vaults::<Test>::get(DOT, 1).expect("dormant vault stored");
		let total = v.debt.principal + v.debt.interest;
		assert!(total > 0);
		let _ = <Pusd as frame::deps::frame_support::traits::fungible::Mutate<u64>>::transfer(
			&2,
			&1,
			total.saturating_sub(pusd_balance(1)),
			frame::deps::frame_support::traits::tokens::Preservation::Expendable,
		);
		assert_ok!(crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(1), 1, DOT, total));
		assert!(Vaults::<Test>::get(DOT, 1).is_none());
		assert_eq!(held(DOT, 1), 0);
		let bs = crate::pallet::BranchStates::<Test>::get(DOT).expect("bs");
		assert_eq!(bs.last_dormant_vault_owner, None);
	});
}

// Closing the last debt-bearing vault used to dead-end: the branch mints
// aggregate interest with per-op ceilings while vaults accrue floors, so a
// drift residual stayed in `minted_interest` forever, read as TCR 0, and
// `WouldEnterSafetyMode` blocked the close. The close must instead sweep the
// orphan into `bad_debt` (it is unbacked circulating pUSD) and settle.
#[test]
fn closing_last_vault_sweeps_interest_drift_to_bad_debt() {
	use frame::deps::frame_support::traits::{
		fungible::{Balanced, Mutate},
		tokens::Imbalance,
	};
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 1_000, 400, rate_pct(7, 100)));
		// Distinct accrual timestamps make several ceiling mints land while
		// the vaults only ever accrue floors.
		advance_time(30 * 24 * 3_600 * 1_000);
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(9), 1, DOT));
		advance_time(24 * 3_600 * 1_000);
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(9), 2, DOT));
		// Top up both owners so overpay-repays can cover accrued interest.
		assert_ok!(<Pusd as Mutate<u64>>::mint_into(&1, 100));
		assert_ok!(<Pusd as Mutate<u64>>::mint_into(&2, 100));

		assert_ok!(crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(2), 2, DOT, 10_000));
		assert!(Vaults::<Test>::get(DOT, 2).is_none(), "vault 2 closed");
		let bs = crate::pallet::BranchStates::<Test>::get(DOT).expect("branch state");
		assert_eq!(bs.debt.bad_debt, 0, "no sweep while a debt-bearing vault remains");

		// The last close: previously rejected with `WouldEnterSafetyMode`.
		assert_ok!(crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(1), 1, DOT, 10_000));
		assert!(Vaults::<Test>::get(DOT, 1).is_none(), "vault 1 closed");

		let bs = crate::pallet::BranchStates::<Test>::get(DOT).expect("branch state");
		assert_eq!(bs.debt.principal, 0);
		assert_eq!(bs.stakes.total, 0);
		assert_eq!(bs.debt.minted_interest, 0, "drift swept out of minted_interest");
		assert_eq!(bs.rounding.ownerless_pusd_debt, 0);
		assert!(bs.debt.bad_debt > 0, "drift recorded as bad debt");
		System::assert_has_event(RuntimeEvent::Vaults(crate::Event::BadDebtRecorded {
			collateral_id: DOT,
			amount: bs.debt.bad_debt,
		}));

		// The insurance flow can now heal the branch clean.
		let credit = <Pusd as Balanced<AccountId>>::issue(bs.debt.bad_debt);
		let surplus = <crate::Pallet<Test> as pusd_primitives::VaultBadDebtInterface<
			AssetId,
			Balance,
			_,
		>>::heal(DOT, credit)
		.expect("heal succeeds");
		assert_eq!(surplus.peek(), 0);
		let bs = crate::pallet::BranchStates::<Test>::get(DOT).expect("branch state");
		assert_eq!(bs.debt.bad_debt, 0, "branch fully settled");
	});
}

#[test]
fn redemption_slot_overwrites_previous_owner() {
	use pusd_primitives::{RedemptionAllocation, VaultRedemptionInterface};
	fn park(owner: AccountId) {
		let post_touch = <crate::Pallet<Test> as VaultRedemptionInterface<
			AccountId,
			AssetId,
			Balance,
		>>::touch_for_redemption(DOT, owner)
		.expect("touch");
		// Leave 150 — below the 200 minimum debt, above zero — so the vault
		// goes Dormant and parks.
		let allocation = RedemptionAllocation {
			debt_to_cancel: post_touch - 150,
			collateral_to_redeemer: (post_touch - 150) / 10,
			fee_collateral_retained: 0,
		};
		assert_ok!(<crate::Pallet<Test> as VaultRedemptionInterface<
			AccountId,
			AssetId,
			Balance,
		>>::apply_redemption(DOT, owner, 7, allocation));
	}
	fn parked() -> Option<AccountId> {
		crate::pallet::BranchStates::<Test>::get(DOT)
			.expect("branch state")
			.last_dormant_vault_owner
	}
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		assert_ok!(open(3, DOT, 1_000, 500, rate_pct(3, 100)));

		park(1);
		assert_eq!(parked(), Some(1));
		park(2);
		assert_eq!(parked(), Some(2), "second parking overwrites the first");
		assert!(vault_status(DOT, 1).is_dormant(), "overwritten owner stays Dormant");
	});
}
