//! `VaultBadDebtInterface` recording and healing.

use crate::{mock::*, pallet::BranchStates};
use frame::deps::frame_support::{
	assert_err, assert_noop, assert_ok,
	traits::{fungible::Balanced, tokens::Imbalance},
};
use pusd_primitives::VaultBadDebtInterface;

fn record(amount: Balance) -> frame::deps::sp_runtime::DispatchResult {
	<crate::Pallet<Test> as VaultBadDebtInterface<AssetId, Balance, _>>::record_bad_debt(
		DOT, amount,
	)
}

/// Issue a fresh credit of `amount`, heal with it, and return the surplus
/// handed back (the unconsumed part of the credit).
fn heal(amount: Balance) -> Result<Balance, frame::deps::sp_runtime::DispatchError> {
	let credit = <Pusd as Balanced<AccountId>>::issue(amount);
	<crate::Pallet<Test> as VaultBadDebtInterface<AssetId, Balance, _>>::heal(DOT, credit)
		.map(|surplus| surplus.peek())
}

fn bad_debt() -> Balance {
	BranchStates::<Test>::get(DOT).expect("branch state").debt.bad_debt
}

#[test]
fn record_bad_debt_increments_and_emits() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(record(1_000));
		assert_eq!(bad_debt(), 1_000);
		assert_ok!(record(500));
		assert_eq!(bad_debt(), 1_500);
		System::assert_has_event(RuntimeEvent::Vaults(crate::Event::BadDebtRecorded {
			collateral_id: DOT,
			amount: 500,
		}));
	});
}

#[test]
fn record_bad_debt_rejects_unknown_branch_and_skips_zero() {
	build_and_execute(|| {
		register_default_branch();
		assert_noop!(
			<crate::Pallet<Test> as VaultBadDebtInterface<AssetId, Balance, _>>::record_bad_debt(
				TOKEN_X, 100,
			),
			crate::Error::<Test>::UnknownCollateral
		);
		let events_before = System::events().len();
		assert_ok!(record(0));
		assert_eq!(bad_debt(), 0);
		assert_eq!(System::events().len(), events_before, "zero record emits nothing");
	});
}

#[test]
fn heal_partial_then_exact_clears_bad_debt() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(record(1_000));

		assert_eq!(heal(400), Ok(0), "fully consumed, no surplus");
		assert_eq!(bad_debt(), 600);
		assert_eq!(heal(600), Ok(0));
		assert_eq!(bad_debt(), 0);
		System::assert_has_event(RuntimeEvent::Vaults(crate::Event::BadDebtHealed {
			collateral_id: DOT,
			amount: 600,
		}));
	});
}

#[test]
fn heal_caps_at_recorded_and_returns_surplus() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(record(500));

		// Over-supplying heals what is recorded and hands the rest back; the
		// caller decides whether a surplus is an error.
		assert_eq!(heal(501), Ok(1));
		assert_eq!(bad_debt(), 0);
		System::assert_has_event(RuntimeEvent::Vaults(crate::Event::BadDebtHealed {
			collateral_id: DOT,
			amount: 500,
		}));

		// Nothing recorded any more: the whole credit comes back, and no
		// further `BadDebtHealed` lands (the test helper's issue/drop still
		// emits asset events, so count only ours).
		let healed_events = || {
			System::events()
				.into_iter()
				.filter(|e| {
					matches!(e.event, RuntimeEvent::Vaults(crate::Event::BadDebtHealed { .. }))
				})
				.count()
		};
		let before = healed_events();
		assert_eq!(heal(50), Ok(50));
		assert_eq!(healed_events(), before, "no-op heal emits no BadDebtHealed");
	});
}

#[test]
fn heal_unknown_branch_errors() {
	build_and_execute(|| {
		register_default_branch();
		let credit = <Pusd as Balanced<AccountId>>::issue(10);
		assert_err!(
			<crate::Pallet<Test> as VaultBadDebtInterface<AssetId, Balance, _>>::heal(
				TOKEN_X, credit,
			)
			.map(|surplus| surplus.peek()),
			crate::Error::<Test>::UnknownCollateral
		);
	});
}
