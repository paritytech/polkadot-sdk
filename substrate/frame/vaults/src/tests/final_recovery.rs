//! FinalRecovery queue behavior backed by `pallet-linked-list`.

use crate::{
	mock::*,
	tests::{rate_pct, vault_status},
};
use frame::deps::{
	frame_support::assert_ok,
	sp_runtime::{FixedPointNumber, FixedU128},
};
use pusd_primitives::{RedemptionAllocation, VaultRedemptionInterface};

fn low_recovery_price() -> FixedU128 {
	FixedU128::from_rational(1u128, 10u128)
}

fn enter_recovery(who: AccountId, rate: FixedU128) {
	set_price(DOT, FixedU128::from_rational(10u128, 1u128));
	assert_ok!(open(who, DOT, 1_000, 500, rate));
	set_price(DOT, low_recovery_price());
	assert_ok!(crate::Pallet::<Test>::enter_final_recovery(RuntimeOrigin::signed(99), who, DOT,));
}

fn direct_redeem(owner: AccountId, redeemer: AccountId, amount: Balance) {
	let post_touch = <crate::Pallet<Test> as VaultRedemptionInterface<
		AccountId,
		AssetId,
		Balance,
	>>::touch_for_redemption(DOT, owner)
	.expect("touch target");
	let debt_to_cancel = core::cmp::min(amount, post_touch);
	let price = MockPrices::get().get(&DOT).copied().expect("price set");
	let collateral_to_redeemer =
		(FixedU128::saturating_from_integer(debt_to_cancel) / price).saturating_mul_int(1u128);
	assert_ok!(
		<crate::Pallet<Test> as VaultRedemptionInterface<AccountId, AssetId, Balance>>::apply_redemption(
			DOT,
			owner,
			redeemer,
			RedemptionAllocation {
				debt_to_cancel,
				collateral_to_redeemer,
				fee_collateral_retained: 0,
			},
		)
	);
}

#[test]
fn final_recovery_queue_is_fifo_across_multiple_vaults() {
	build_and_execute(|| {
		register_default_branch();

		enter_recovery(1, rate_pct(1, 100));
		enter_recovery(2, rate_pct(2, 100));
		enter_recovery(3, rate_pct(3, 100));

		assert_eq!(crate::Pallet::<Test>::final_recovery_queue_head(DOT, 10), alloc::vec![1, 2, 3]);
		assert_eq!(
			<crate::Pallet<Test> as VaultRedemptionInterface<AccountId, AssetId, Balance>>::next_redemption_target(
				DOT, None,
			),
			Some(1)
		);
	});
}

#[test]
fn final_recovery_middle_exit_splices_queue() {
	build_and_execute(|| {
		register_default_branch();

		enter_recovery(1, rate_pct(1, 100));
		enter_recovery(2, rate_pct(2, 100));
		enter_recovery(3, rate_pct(3, 100));

		set_price(DOT, FixedU128::from_rational(10u128, 1u128));
		assert_ok!(crate::Pallet::<Test>::exit_final_recovery(
			RuntimeOrigin::signed(42),
			2,
			DOT,
			Position::endpoints_only(),
		));

		assert!(vault_status(DOT, 2).is_active());
		assert_eq!(crate::Pallet::<Test>::final_recovery_queue_head(DOT, 10), alloc::vec![1, 3]);
	});
}

#[test]
fn redemption_queue_composes_recovery_dormant_and_rate_index() {
	build_and_execute(|| {
		register_default_branch();

		enter_recovery(1, rate_pct(1, 100));
		enter_recovery(2, rate_pct(2, 100));

		set_price(DOT, FixedU128::from_rational(10u128, 1u128));
		assert_ok!(open(3, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(4, DOT, 1_000, 500, rate_pct(2, 100)));
		assert_ok!(open(5, DOT, 1_000, 500, rate_pct(3, 100)));

		direct_redeem(3, 10, 350);
		assert!(vault_status(DOT, 3).is_dormant());

		assert_eq!(
			crate::Pallet::<Test>::redemption_queue_head(DOT, 10),
			alloc::vec![1, 2, 3, 4, 5]
		);
	});
}
