//! Port of liquity_v2/contracts/test/events.t.sol — `TroveEventsTest`
//! (rows 1-18, lines 21857-22424).
//!
//! Liquity emits two unified events per dispatchable: `TroveUpdated` and
//! `TroveOperation`. The polkadot port instead emits per-operation events
//! (`VaultOpened`, `CollateralDeposited`, `Borrowed`, `Repaid`,
//! `CollateralWithdrawn`, `BorrowRateChanged`, `UpfrontFeeCharged`,
//! `InterestAccrued`, `VaultClosed`, `VaultRedeemed`, `VaultStatusChanged`,
//! `FinalRecoveryEntered`, `BranchRegistered`, `ParameterUpdated`,
//! `DebtCeilingUpdated`).
//!
//! Each test below pins the canonical event(s) emitted by one pallet
//! call-site (or trait method) using `frame_system::Pallet::assert_has_event`.

use crate::{mock::*, tests::rate_pct};
use frame::deps::frame_support::assert_ok;

/// Convenience: assert the system event log contains exactly this
/// `pallet_vaults::Event<Test>` value (wrapped in the runtime event).
fn assert_event(event: crate::Event<Test>) {
	System::assert_has_event(RuntimeEvent::Vaults(event));
}

// row 1, 2: open emits the canonical opening trio (VaultOpened, plus
// CollateralDeposited and Borrowed for the inputs, plus UpfrontFeeCharged
// for the protocol-favored fee).
#[test]
fn open_vault_emits_canonical_events() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 2_000, rate_pct(10, 100)));
		assert_event(crate::Event::VaultOpened { collateral_id: DOT, owner: 1 });
		assert_event(crate::Event::CollateralDeposited {
			collateral_id: DOT,
			owner: 1,
			from: 1,
			amount: 1_000,
		});
		assert_event(crate::Event::Borrowed {
			collateral_id: DOT,
			owner: 1,
			recipient: 1,
			amount: 2_000,
		});
		// Upfront fee is non-trivial after the math fix.
		let predicted_fee =
			crate::Pallet::<Test>::predict_open_upfront_fee(DOT, 2_000, rate_pct(10, 100));
		assert!(predicted_fee > 0);
		// We can't compute the predicted fee post-hoc (state changed), so
		// we re-derive it before the open. To keep it simple, assert the
		// event was emitted with that value (read out of vault.accrued).
		let v = crate::pallet::Vaults::<Test>::get(DOT, 1).unwrap();
		assert_event(crate::Event::UpfrontFeeCharged {
			collateral_id: DOT,
			owner: 1,
			amount: v.accrued_interest,
		});
	});
}

// rows 3, 4: adjust → polkadot has separate events. Cover the deposit-then-
// borrow combination.
#[test]
fn deposit_collateral_emits_collateral_deposited() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(crate::Pallet::<Test>::deposit_collateral_for(
			RuntimeOrigin::signed(2),
			1,
			DOT,
			100,
		));
		// `from` is the caller (acct 2), `owner` is the vault owner (acct 1).
		assert_event(crate::Event::CollateralDeposited {
			collateral_id: DOT,
			owner: 1,
			from: 2,
			amount: 100,
		});
	});
}

#[test]
fn borrow_emits_borrowed() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 3_000, 2_000, rate_pct(5, 100)));
		assert_ok!(crate::Pallet::<Test>::borrow(
			RuntimeOrigin::signed(1),
			DOT,
			500,
			None,
			None,
			Position::endpoints_only(),
		));
		assert_event(crate::Event::Borrowed {
			collateral_id: DOT,
			owner: 1,
			recipient: 1,
			amount: 500,
		});
	});
}

#[test]
fn withdraw_collateral_emits_collateral_withdrawn() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 3_000, 500, rate_pct(5, 100)));
		assert_ok!(crate::Pallet::<Test>::withdraw_collateral(
			RuntimeOrigin::signed(1),
			DOT,
			100,
			None,
		));
		assert_event(crate::Event::CollateralWithdrawn {
			collateral_id: DOT,
			owner: 1,
			recipient: 1,
			amount: 100,
		});
	});
}

#[test]
fn repay_emits_repaid() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 1_000, rate_pct(5, 100)));
		assert_ok!(crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(1), 1, DOT, 200));
		assert_event(crate::Event::Repaid { collateral_id: DOT, owner: 1, from: 1, amount: 200 });
	});
}

// rows 5, 6: rate adjust emits BorrowRateChanged + UpfrontFeeCharged when
// premature.
#[test]
fn change_rate_emits_borrow_rate_changed() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 2_000, rate_pct(5, 100)));
		// After the cooldown, no fee — only BorrowRateChanged.
		advance_time(24 * 3_600 * 1_000);
		assert_ok!(crate::Pallet::<Test>::change_rate(
			RuntimeOrigin::signed(1),
			DOT,
			rate_pct(7, 100),
			Position::endpoints_only(),
		));
		assert_event(crate::Event::BorrowRateChanged {
			collateral_id: DOT,
			owner: 1,
			old_rate: rate_pct(5, 100),
			new_rate: rate_pct(7, 100),
		});
	});
}

#[test]
fn premature_change_rate_emits_upfront_fee_charged() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 2_000, rate_pct(5, 100)));
		// Within the cooldown window — fee charged.
		advance_time(12 * 3_600 * 1_000);
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(1), 1, DOT));
		let predicted =
			crate::Pallet::<Test>::predict_rate_change_upfront_fee(DOT, 1, rate_pct(7, 100));
		assert!(predicted > 0);
		assert_ok!(crate::Pallet::<Test>::change_rate(
			RuntimeOrigin::signed(1),
			DOT,
			rate_pct(7, 100),
			Position::endpoints_only(),
		));
		assert_event(crate::Event::UpfrontFeeCharged {
			collateral_id: DOT,
			owner: 1,
			amount: predicted,
		});
		assert_event(crate::Event::BorrowRateChanged {
			collateral_id: DOT,
			owner: 1,
			old_rate: rate_pct(5, 100),
			new_rate: rate_pct(7, 100),
		});
	});
}

// rows 7, 8: poke (applyPendingDebt) emits InterestAccrued when there is
// any pending interest to materialise.
#[test]
fn poke_emits_interest_accrued() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 3_000, 2_000, rate_pct(50, 100)));
		advance_time(7 * 24 * 3_600 * 1_000);
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(2), 1, DOT));
		// We don't pin the magnitude (it depends on the upfront fee period
		// and rate), but we assert that *some* InterestAccrued event for
		// this vault landed in the log.
		let saw = System::events().into_iter().any(|e| {
			matches!(
				e.event,
				RuntimeEvent::Vaults(crate::Event::InterestAccrued { collateral_id, owner, .. })
					if collateral_id == DOT && owner == 1
			)
		});
		assert!(saw, "expected an InterestAccrued event on poke");
	});
}

// rows 9, 10: close emits VaultClosed.
#[test]
fn close_vault_emits_vault_closed() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(5, 100)));
		// Repay vault 2 and close.
		let v = crate::pallet::Vaults::<Test>::get(DOT, 2).unwrap();
		let total = v.interest_bearing_debt + v.accrued_interest;
		let _ = <Pusd as frame::deps::frame_support::traits::fungible::Mutate<u64>>::transfer(
			&1,
			&2,
			v.accrued_interest,
			frame::deps::frame_support::traits::tokens::Preservation::Expendable,
		);
		assert_ok!(crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(2), 2, DOT, total));
		assert_ok!(crate::Pallet::<Test>::close_vault(RuntimeOrigin::signed(2), DOT, None));
		assert_event(crate::Event::VaultClosed { collateral_id: DOT, owner: 2, recipient: 2 });
	});
}

// rows 17, 18: redemption emits VaultRedeemed (one per redeemed vault).
#[test]
fn redemption_emits_vault_redeemed() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		let target = redeem(DOT, 3, 200).expect("redeem ok");
		assert_eq!(target, 1);
		// VaultRedeemed event: don't pin the exact magnitudes (collateral
		// rounding depends on price), just confirm the event landed.
		let saw = System::events().into_iter().any(|e| {
			matches!(
				e.event,
				RuntimeEvent::Vaults(crate::Event::VaultRedeemed {
					collateral_id, owner, redeemer, debt_cancelled, ..
				}) if collateral_id == DOT
					&& owner == 1
					&& redeemer == 3
					&& debt_cancelled == 200
			)
		});
		assert!(saw, "expected a VaultRedeemed event");
	});
}

// register_branch emits BranchRegistered.
#[test]
fn register_branch_emits_branch_registered() {
	build_and_execute(|| {
		register_default_branch();
		assert_event(crate::Event::BranchRegistered { collateral_id: DOT });
	});
}

// enable_frozen_mode emits ModeChanged.
#[test]
fn enable_frozen_mode_emits_mode_changed() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(crate::Pallet::<Test>::enable_frozen_mode(RuntimeOrigin::root(), DOT));
		// Branch starts in Normal mode (no debt yet, TCR is treated as
		// infinity); after enable_frozen_mode it transitions to Frozen.
		let saw = System::events().into_iter().any(|e| {
			matches!(
				e.event,
				RuntimeEvent::Vaults(crate::Event::ModeChanged { collateral_id, new_mode, .. })
					if collateral_id == DOT
						&& matches!(new_mode, crate::BranchMode::Frozen { .. })
			)
		});
		assert!(saw, "expected a ModeChanged → Frozen event");
	});
}

// Governance setters emit ParameterUpdated with the matching ParameterId.
#[test]
fn set_parameter_emits_parameter_updated() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(crate::Pallet::<Test>::set_minimum_collateralization_ratio(
			RuntimeOrigin::root(),
			DOT,
			frame::deps::sp_runtime::FixedU128::from_rational(115u128, 100u128),
		));
		assert_event(crate::Event::ParameterUpdated {
			collateral_id: DOT,
			parameter: crate::types::ParameterId::MinimumCollateralizationRatio,
		});
	});
}

// set_debt_ceiling emits DebtCeilingUpdated with old/new values.
#[test]
fn set_debt_ceiling_emits_debt_ceiling_updated() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(
			crate::Pallet::<Test>::set_debt_ceiling(RuntimeOrigin::root(), DOT, 50_000_000,)
		);
		assert_event(crate::Event::DebtCeilingUpdated {
			collateral_id: DOT,
			old_value: 100_000_000,
			new_value: 50_000_000,
		});
	});
}

// enter_final_recovery emits VaultStatusChanged + FinalRecoveryEntered.
#[test]
fn enter_final_recovery_emits_status_change_and_fifo_entry() {
	build_and_execute(|| {
		register_default_branch();
		// Single vault that we'll push into FinalRecovery via a price drop.
		assert_ok!(open(1, DOT, 1_000, 2_000, rate_pct(5, 100)));
		set_price(DOT, frame::deps::sp_runtime::FixedU128::from_rational(2u128, 100u128));
		assert_ok!(crate::Pallet::<Test>::enter_final_recovery(RuntimeOrigin::signed(2), 1, DOT,));
		assert_event(crate::Event::FinalRecoveryEntered { collateral_id: DOT, owner: 1 });
		assert_event(crate::Event::VaultStatusChanged {
			collateral_id: DOT,
			owner: 1,
			old_status: crate::types::VaultStatus::Active,
			new_status: crate::types::VaultStatus::FinalRecovery,
		});
	});
}
