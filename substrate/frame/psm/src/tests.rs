// This file is part of Substrate.

// Copyright (C) Amforc AG.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::mock::*;
use crate::{
	AssetCeilingWeight, CircuitBreakerLevel, Error, Event, ExternalAssets, MaxPsmDebtOfTotal,
	MintingFee, PsmDebt, RedemptionFee,
};
use frame_support::{assert_noop, assert_ok, hypothetically};
use sp_runtime::{DispatchError, Permill, TokenError};

mod mint {
	use super::*;

	#[test]
	fn success_basic() {
		new_test_ext().execute_with(|| {
			let mint_amount = 1000 * PUSD_UNIT;
			let alice_usdc_before = get_asset_balance(USDC_ASSET_ID, ALICE);

			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, mint_amount));

			let fee = Permill::from_percent(1).mul_ceil(mint_amount);
			let pusd_to_user = mint_amount - fee;

			assert_eq!(get_asset_balance(USDC_ASSET_ID, ALICE), alice_usdc_before - mint_amount);
			assert_eq!(get_asset_balance(USDC_ASSET_ID, psm_account()), mint_amount);
			assert_eq!(get_asset_balance(PUSD_ASSET_ID, ALICE), pusd_to_user);
			assert_eq!(get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND), fee);
			assert_eq!(PsmDebt::<Test>::get(USDC_ASSET_ID), mint_amount);

			System::assert_has_event(
				Event::<Test>::Minted {
					who: ALICE,
					asset_id: USDC_ASSET_ID,
					external_amount: mint_amount,
					pusd_received: pusd_to_user,
					fee,
				}
				.into(),
			);
		});
	}

	#[test]
	fn fee_zero() {
		new_test_ext().execute_with(|| {
			set_minting_fee(USDC_ASSET_ID, Permill::zero());

			let mint_amount = 1000 * PUSD_UNIT;

			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, mint_amount));

			assert_eq!(get_asset_balance(PUSD_ASSET_ID, ALICE), mint_amount);
			assert_eq!(get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND), 0);
		});
	}

	#[test]
	fn fee_nonzero() {
		new_test_ext().execute_with(|| {
			set_minting_fee(USDC_ASSET_ID, Permill::from_percent(5));

			let mint_amount = 1000 * PUSD_UNIT;
			let fee = Permill::from_percent(5).mul_ceil(mint_amount);
			let pusd_to_user = mint_amount - fee;

			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, mint_amount));

			assert_eq!(get_asset_balance(PUSD_ASSET_ID, ALICE), pusd_to_user);
			assert_eq!(get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND), fee);
		});
	}

	#[test]
	fn fee_100_percent() {
		new_test_ext().execute_with(|| {
			set_minting_fee(USDC_ASSET_ID, Permill::from_percent(100));

			let mint_amount = 1000 * PUSD_UNIT;

			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, mint_amount));

			assert_eq!(get_asset_balance(PUSD_ASSET_ID, ALICE), 0);
			assert_eq!(get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND), mint_amount);
		});
	}

	#[test]
	fn fails_unsupported_asset() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(ALICE), UNSUPPORTED_ASSET_ID, 1000 * PUSD_UNIT),
				Error::<Test>::UnsupportedAsset
			);
		});
	}

	#[test]
	fn fails_asset_minting_disabled() {
		new_test_ext().execute_with(|| {
			set_asset_status(USDC_ASSET_ID, CircuitBreakerLevel::MintingDisabled);

			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, 1000 * PUSD_UNIT),
				Error::<Test>::MintingStopped
			);

			// Other assets should still work
			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDT_ASSET_ID, 1000 * PUSD_UNIT));
		});
	}

	#[test]
	fn fails_asset_all_disabled() {
		new_test_ext().execute_with(|| {
			set_asset_status(USDC_ASSET_ID, CircuitBreakerLevel::AllDisabled);

			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, 1000 * PUSD_UNIT),
				Error::<Test>::MintingStopped
			);

			// Other assets should still work
			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDT_ASSET_ID, 1000 * PUSD_UNIT));
		});
	}

	#[test]
	fn fails_below_minimum() {
		new_test_ext().execute_with(|| {
			let below_min = MinSwapAmount::get() - 1;

			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, below_min),
				Error::<Test>::BelowMinimumSwap
			);
		});
	}

	#[test]
	fn fails_exceeds_max_debt() {
		new_test_ext().execute_with(|| {
			// Set global ceiling to 1% and asset ratio to 100%
			set_max_psm_debt_ratio(Permill::from_percent(1));
			set_asset_ceiling_weight(USDC_ASSET_ID, Permill::from_percent(100));

			let max_debt = crate::Pallet::<Test>::max_asset_debt(USDC_ASSET_ID);
			let too_much = max_debt + 1;

			fund_external_asset(USDC_ASSET_ID, ALICE, too_much);

			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, too_much),
				Error::<Test>::ExceedsMaxPsmDebt
			);
		});
	}

	#[test]
	fn fails_with_extreme_debt_value() {
		new_test_ext().execute_with(|| {
			// When PSM debt is set to an extreme value, the aggregate ceiling check
			// will catch it before reaching the per-asset arithmetic overflow check.
			// This is correct behavior - ceiling checks provide safety.
			set_max_psm_debt_ratio(Permill::from_percent(100));
			set_asset_ceiling_weight(USDC_ASSET_ID, Permill::from_percent(100));

			PsmDebt::<Test>::insert(USDC_ASSET_ID, u128::MAX - 100);

			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, 1000 * PUSD_UNIT),
				Error::<Test>::ExceedsMaxPsmDebt
			);
		});
	}

	#[test]
	fn boundary_new_debt_equals_max() {
		new_test_ext().execute_with(|| {
			// Set USDC to 100% and USDT to 0% so USDC gets full ceiling
			set_max_psm_debt_ratio(Permill::from_percent(1));
			set_asset_ceiling_weight(USDC_ASSET_ID, Permill::from_percent(100));
			set_asset_ceiling_weight(USDT_ASSET_ID, Permill::from_percent(0));

			let max_debt = crate::Pallet::<Test>::max_asset_debt(USDC_ASSET_ID);

			fund_external_asset(USDC_ASSET_ID, ALICE, max_debt);

			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, max_debt));

			assert_eq!(PsmDebt::<Test>::get(USDC_ASSET_ID), max_debt);
		});
	}

	#[test]
	fn fails_insufficient_external_balance() {
		new_test_ext().execute_with(|| {
			let alice_usdc_before = get_asset_balance(USDC_ASSET_ID, ALICE);
			let alice_pusd_before = get_asset_balance(PUSD_ASSET_ID, ALICE);
			let psm_usdc_before = get_asset_balance(USDC_ASSET_ID, psm_account());
			let too_much = alice_usdc_before + 1000 * PUSD_UNIT;

			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, too_much),
				TokenError::FundsUnavailable
			);

			// Verify no state mutation occurred
			assert_eq!(PsmDebt::<Test>::get(USDC_ASSET_ID), 0);
			assert_eq!(get_asset_balance(USDC_ASSET_ID, ALICE), alice_usdc_before);
			assert_eq!(get_asset_balance(PUSD_ASSET_ID, ALICE), alice_pusd_before);
			assert_eq!(get_asset_balance(USDC_ASSET_ID, psm_account()), psm_usdc_before);
		});
	}

	#[test]
	fn fails_mint_exceeds_system_wide_issuance() {
		new_test_ext().execute_with(|| {
			let maximum_issuance = MockMaximumIssuance::get();

			// Simulate Vaults having minted most of the cap (leave only 100 pUSD room)
			let vault_minted = maximum_issuance - 100 * PUSD_UNIT;
			fund_pusd(BOB, vault_minted);

			// PSM per-asset ceiling would allow this, but system cap won't
			let mint_amount = 1000 * PUSD_UNIT;
			fund_external_asset(USDC_ASSET_ID, ALICE, mint_amount);

			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, mint_amount),
				Error::<Test>::ExceedsMaxIssuance
			);
		});
	}

	#[test]
	fn fails_mint_exceeds_aggregate_psm_ceiling() {
		new_test_ext().execute_with(|| {
			// Set both assets to 50% ratio each (100% total)
			// This tests that aggregate PSM ceiling is enforced even when per-asset ceilings allow
			set_asset_ceiling_weight(USDC_ASSET_ID, Permill::from_percent(50));
			set_asset_ceiling_weight(USDT_ASSET_ID, Permill::from_percent(50));

			let max_psm_debt = crate::Pallet::<Test>::max_psm_debt();

			// Mint 50% of PSM ceiling via USDC (succeeds)
			let usdc_amount = Permill::from_percent(50).mul_floor(max_psm_debt);
			fund_external_asset(USDC_ASSET_ID, ALICE, usdc_amount);
			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, usdc_amount));

			// Try to mint 50% + 1 via USDT (total would exceed PSM ceiling)
			let usdt_amount = Permill::from_percent(50).mul_floor(max_psm_debt) + 1;
			fund_external_asset(USDT_ASSET_ID, BOB, usdt_amount);

			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(BOB), USDT_ASSET_ID, usdt_amount),
				Error::<Test>::ExceedsMaxPsmDebt
			);
		});
	}
}

mod redeem {
	use super::*;

	#[test]
	fn success_basic() {
		ExtBuilder::default().mints(ALICE, 5000 * PUSD_UNIT).build_and_execute(|| {
			let redeem_amount = 1000 * PUSD_UNIT;
			let alice_pusd_before = get_asset_balance(PUSD_ASSET_ID, ALICE);
			let alice_usdc_before = get_asset_balance(USDC_ASSET_ID, ALICE);
			let psm_usdc_before = get_asset_balance(USDC_ASSET_ID, psm_account());
			let debt_before = PsmDebt::<Test>::get(USDC_ASSET_ID);

			assert_ok!(Psm::redeem(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, redeem_amount));

			let fee = Permill::from_percent(1).mul_ceil(redeem_amount);
			let external_to_user = redeem_amount - fee;

			assert_eq!(get_asset_balance(PUSD_ASSET_ID, ALICE), alice_pusd_before - redeem_amount);
			assert_eq!(
				get_asset_balance(USDC_ASSET_ID, ALICE),
				alice_usdc_before + external_to_user
			);
			assert_eq!(
				get_asset_balance(USDC_ASSET_ID, psm_account()),
				psm_usdc_before - external_to_user
			);
			assert_eq!(PsmDebt::<Test>::get(USDC_ASSET_ID), debt_before - external_to_user);

			System::assert_has_event(
				Event::<Test>::Redeemed {
					who: ALICE,
					asset_id: USDC_ASSET_ID,
					pusd_paid: redeem_amount,
					external_received: external_to_user,
					fee,
				}
				.into(),
			);
		});
	}

	#[test]
	fn fee_zero() {
		ExtBuilder::default().mints(ALICE, 5000 * PUSD_UNIT).build_and_execute(|| {
			set_redemption_fee(USDC_ASSET_ID, Permill::zero());

			let redeem_amount = 1000 * PUSD_UNIT;
			let alice_usdc_before = get_asset_balance(USDC_ASSET_ID, ALICE);

			assert_ok!(Psm::redeem(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, redeem_amount));

			assert_eq!(get_asset_balance(USDC_ASSET_ID, ALICE), alice_usdc_before + redeem_amount);
		});
	}

	#[test]
	fn fee_nonzero() {
		ExtBuilder::default().mints(ALICE, 5000 * PUSD_UNIT).build_and_execute(|| {
			set_redemption_fee(USDC_ASSET_ID, Permill::from_percent(5));

			let redeem_amount = 1000 * PUSD_UNIT;
			let fee = Permill::from_percent(5).mul_ceil(redeem_amount);
			let external_to_user = redeem_amount - fee;
			let alice_usdc_before = get_asset_balance(USDC_ASSET_ID, ALICE);

			assert_ok!(Psm::redeem(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, redeem_amount));

			assert_eq!(
				get_asset_balance(USDC_ASSET_ID, ALICE),
				alice_usdc_before + external_to_user
			);
		});
	}

	#[test]
	fn fee_100_percent() {
		ExtBuilder::default().mints(ALICE, 5000 * PUSD_UNIT).build_and_execute(|| {
			set_redemption_fee(USDC_ASSET_ID, Permill::from_percent(100));

			let redeem_amount = 1000 * PUSD_UNIT;
			let alice_usdc_before = get_asset_balance(USDC_ASSET_ID, ALICE);
			let insurance_pusd_before = get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND);

			assert_ok!(Psm::redeem(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, redeem_amount));

			assert_eq!(get_asset_balance(USDC_ASSET_ID, ALICE), alice_usdc_before);
			assert_eq!(
				get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND),
				insurance_pusd_before + redeem_amount
			);
		});
	}

	#[test]
	fn fails_unsupported_asset() {
		ExtBuilder::default().mints(ALICE, 5000 * PUSD_UNIT).build_and_execute(|| {
			assert_noop!(
				Psm::redeem(RuntimeOrigin::signed(ALICE), UNSUPPORTED_ASSET_ID, 1000 * PUSD_UNIT),
				Error::<Test>::UnsupportedAsset
			);
		});
	}

	#[test]
	fn fails_asset_all_disabled() {
		ExtBuilder::default().mints(ALICE, 5000 * PUSD_UNIT).build_and_execute(|| {
			set_asset_status(USDC_ASSET_ID, CircuitBreakerLevel::AllDisabled);

			assert_noop!(
				Psm::redeem(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, 1000 * PUSD_UNIT),
				Error::<Test>::AllSwapsStopped
			);
		});
	}

	#[test]
	fn allows_when_asset_minting_disabled() {
		ExtBuilder::default().mints(ALICE, 5000 * PUSD_UNIT).build_and_execute(|| {
			set_asset_status(USDC_ASSET_ID, CircuitBreakerLevel::MintingDisabled);

			// Redemption should still work when only minting is disabled
			assert_ok!(Psm::redeem(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, 1000 * PUSD_UNIT));
		});
	}

	#[test]
	fn fails_below_minimum() {
		ExtBuilder::default().mints(ALICE, 5000 * PUSD_UNIT).build_and_execute(|| {
			let below_min = 50 * PUSD_UNIT;

			assert_noop!(
				Psm::redeem(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, below_min),
				Error::<Test>::BelowMinimumSwap
			);
		});
	}

	#[test]
	fn fails_insufficient_reserve() {
		new_test_ext().execute_with(|| {
			fund_pusd(BOB, 10_000 * PUSD_UNIT);

			let reserve = get_asset_balance(USDC_ASSET_ID, psm_account());
			assert_eq!(reserve, 0);

			assert_noop!(
				Psm::redeem(RuntimeOrigin::signed(BOB), USDC_ASSET_ID, 1000 * PUSD_UNIT),
				Error::<Test>::InsufficientReserve
			);
		});
	}

	#[test]
	fn fails_insufficient_pusd_balance() {
		ExtBuilder::default()
			.mints(ALICE, 5000 * PUSD_UNIT)
			.mints(BOB, 10_000 * PUSD_UNIT)
			.build_and_execute(|| {
				let alice_pusd = get_asset_balance(PUSD_ASSET_ID, ALICE);
				let too_much = alice_pusd + 1000 * PUSD_UNIT;

				assert_noop!(
					Psm::redeem(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, too_much),
					TokenError::FundsUnavailable
				);
			});
	}

	#[test]
	fn boundary_reserve_equals_output() {
		new_test_ext().execute_with(|| {
			set_minting_fee(USDC_ASSET_ID, Permill::zero());
			set_redemption_fee(USDC_ASSET_ID, Permill::zero());

			let amount = 5000 * PUSD_UNIT;
			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, amount));
			assert_ok!(Psm::redeem(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, amount));

			assert_eq!(get_asset_balance(USDC_ASSET_ID, psm_account()), 0);
		});
	}

	#[test]
	fn fails_when_reserve_exceeds_debt_donated_reserves() {
		ExtBuilder::default().mints(ALICE, 5000 * PUSD_UNIT).build_and_execute(|| {
			set_redemption_fee(USDC_ASSET_ID, Permill::zero());

			let debt = PsmDebt::<Test>::get(USDC_ASSET_ID);
			let donation = 5000 * PUSD_UNIT;

			// Defensive path: simulate donated reserves by funding psm_account()
			// directly, bypassing mint to create a reserve > debt scenario.
			fund_external_asset(USDC_ASSET_ID, psm_account(), donation);

			let reserve = get_asset_balance(USDC_ASSET_ID, psm_account());
			assert!(reserve > debt, "reserve should exceed debt after donation");

			// Give user enough pUSD to try redeeming more than debt
			let redeem_amount = debt + donation;
			fund_pusd(ALICE, redeem_amount);

			// Should fail because redemption is limited by debt, not reserve
			assert_noop!(
				Psm::redeem(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, redeem_amount),
				Error::<Test>::InsufficientReserve
			);

			// Verify boundary: exactly debt works, but debt+1 does not
			hypothetically!({
				assert_ok!(Psm::redeem(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, debt));
				assert_eq!(get_asset_balance(USDC_ASSET_ID, psm_account()), donation);
			});

			assert_noop!(
				Psm::redeem(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, debt + 1),
				Error::<Test>::InsufficientReserve
			);
		});
	}
}

mod governance {
	use super::*;

	#[test]
	fn set_minting_fee_works() {
		new_test_ext().execute_with(|| {
			let old_fee = MintingFee::<Test>::get(USDC_ASSET_ID);
			let new_fee = Permill::from_percent(5);

			assert_ok!(Psm::set_minting_fee(RuntimeOrigin::root(), USDC_ASSET_ID, new_fee));

			assert_eq!(MintingFee::<Test>::get(USDC_ASSET_ID), new_fee);

			System::assert_has_event(
				Event::<Test>::MintingFeeUpdated {
					asset_id: USDC_ASSET_ID,
					old_value: old_fee,
					new_value: new_fee,
				}
				.into(),
			);
		});
	}

	#[test]
	fn set_minting_fee_unauthorized() {
		new_test_ext().execute_with(|| {
			let old_fee = MintingFee::<Test>::get(USDC_ASSET_ID);

			assert_noop!(
				Psm::set_minting_fee(
					RuntimeOrigin::signed(ALICE),
					USDC_ASSET_ID,
					Permill::from_percent(5)
				),
				DispatchError::BadOrigin
			);

			assert_eq!(MintingFee::<Test>::get(USDC_ASSET_ID), old_fee);
		});
	}

	#[test]
	fn set_redemption_fee_works() {
		new_test_ext().execute_with(|| {
			let old_fee = RedemptionFee::<Test>::get(USDC_ASSET_ID);
			let new_fee = Permill::from_percent(5);

			assert_ok!(Psm::set_redemption_fee(RuntimeOrigin::root(), USDC_ASSET_ID, new_fee));

			assert_eq!(RedemptionFee::<Test>::get(USDC_ASSET_ID), new_fee);

			System::assert_has_event(
				Event::<Test>::RedemptionFeeUpdated {
					asset_id: USDC_ASSET_ID,
					old_value: old_fee,
					new_value: new_fee,
				}
				.into(),
			);
		});
	}

	#[test]
	fn set_redemption_fee_unauthorized() {
		new_test_ext().execute_with(|| {
			let old_fee = RedemptionFee::<Test>::get(USDC_ASSET_ID);

			assert_noop!(
				Psm::set_redemption_fee(
					RuntimeOrigin::signed(ALICE),
					USDC_ASSET_ID,
					Permill::from_percent(5)
				),
				DispatchError::BadOrigin
			);

			assert_eq!(RedemptionFee::<Test>::get(USDC_ASSET_ID), old_fee);
		});
	}

	#[test]
	fn set_max_psm_debt_works() {
		new_test_ext().execute_with(|| {
			let old_ratio = MaxPsmDebtOfTotal::<Test>::get();
			let new_ratio = Permill::from_percent(20);

			assert_ok!(Psm::set_max_psm_debt(RuntimeOrigin::root(), new_ratio));

			assert_eq!(MaxPsmDebtOfTotal::<Test>::get(), new_ratio);

			System::assert_has_event(
				Event::<Test>::MaxPsmDebtOfTotalUpdated {
					old_value: old_ratio,
					new_value: new_ratio,
				}
				.into(),
			);
		});
	}

	#[test]
	fn set_max_psm_debt_unauthorized() {
		new_test_ext().execute_with(|| {
			let old_ratio = MaxPsmDebtOfTotal::<Test>::get();

			assert_noop!(
				Psm::set_max_psm_debt(RuntimeOrigin::signed(ALICE), Permill::from_percent(20)),
				DispatchError::BadOrigin
			);

			assert_eq!(MaxPsmDebtOfTotal::<Test>::get(), old_ratio);
		});
	}

	#[test]
	fn set_asset_status_works() {
		new_test_ext().execute_with(|| {
			let new_status = CircuitBreakerLevel::MintingDisabled;

			assert_ok!(Psm::set_asset_status(RuntimeOrigin::root(), USDC_ASSET_ID, new_status));

			assert_eq!(ExternalAssets::<Test>::get(USDC_ASSET_ID), Some(new_status));

			System::assert_has_event(
				Event::<Test>::AssetStatusUpdated { asset_id: USDC_ASSET_ID, status: new_status }
					.into(),
			);
		});
	}

	#[test]
	fn set_asset_status_unauthorized() {
		new_test_ext().execute_with(|| {
			let old_status = ExternalAssets::<Test>::get(USDC_ASSET_ID);

			assert_noop!(
				Psm::set_asset_status(
					RuntimeOrigin::signed(ALICE),
					USDC_ASSET_ID,
					CircuitBreakerLevel::MintingDisabled
				),
				DispatchError::BadOrigin
			);

			assert_eq!(ExternalAssets::<Test>::get(USDC_ASSET_ID), old_status);
		});
	}

	#[test]
	fn set_asset_status_fails_unapproved_asset() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Psm::set_asset_status(
					RuntimeOrigin::root(),
					UNSUPPORTED_ASSET_ID,
					CircuitBreakerLevel::MintingDisabled
				),
				Error::<Test>::AssetNotApproved
			);
		});
	}

	#[test]
	fn set_asset_ceiling_weight_works() {
		new_test_ext().execute_with(|| {
			let old_ratio = AssetCeilingWeight::<Test>::get(USDC_ASSET_ID);
			let new_ratio = Permill::from_percent(80);

			assert_ok!(Psm::set_asset_ceiling_weight(
				RuntimeOrigin::root(),
				USDC_ASSET_ID,
				new_ratio
			));

			assert_eq!(AssetCeilingWeight::<Test>::get(USDC_ASSET_ID), new_ratio);

			System::assert_has_event(
				Event::<Test>::AssetCeilingWeightUpdated {
					asset_id: USDC_ASSET_ID,
					old_value: old_ratio,
					new_value: new_ratio,
				}
				.into(),
			);
		});
	}

	#[test]
	fn set_asset_ceiling_weight_unauthorized() {
		new_test_ext().execute_with(|| {
			let old_ratio = AssetCeilingWeight::<Test>::get(USDC_ASSET_ID);

			assert_noop!(
				Psm::set_asset_ceiling_weight(
					RuntimeOrigin::signed(ALICE),
					USDC_ASSET_ID,
					Permill::from_percent(80)
				),
				DispatchError::BadOrigin
			);

			assert_eq!(AssetCeilingWeight::<Test>::get(USDC_ASSET_ID), old_ratio);
		});
	}

	#[test]
	fn add_external_asset_works() {
		new_test_ext().execute_with(|| {
			let new_asset = 99u32;
			create_asset_with_metadata(new_asset);
			assert!(!Psm::is_approved_asset(&new_asset));

			assert_ok!(Psm::add_external_asset(RuntimeOrigin::root(), new_asset));

			assert!(Psm::is_approved_asset(&new_asset));

			System::assert_has_event(
				Event::<Test>::ExternalAssetAdded { asset_id: new_asset }.into(),
			);
		});
	}

	#[test]
	fn add_external_asset_accepts_differing_decimals_within_range() {
		new_test_ext().execute_with(|| {
			let new_asset = 99u32;
			// Asset with 8 decimals vs pUSD's 6 — within MAX_DECIMALS_DIFF.
			assert_ok!(Assets::create(RuntimeOrigin::signed(ALICE), new_asset, ALICE, 1));
			assert_ok!(Assets::set_metadata(
				RuntimeOrigin::signed(ALICE),
				new_asset,
				b"Eight Decimals".to_vec(),
				b"EIG".to_vec(),
				8
			));

			assert_ok!(Psm::add_external_asset(RuntimeOrigin::root(), new_asset));
			assert_eq!(crate::AssetDecimals::<Test>::get(new_asset), Some(8));
		});
	}

	#[test]
	fn add_external_asset_fails_decimals_out_of_range() {
		new_test_ext().execute_with(|| {
			let new_asset = 99u32;
			// Decimals 6 + 25 = 31 exceeds MAX_DECIMALS_DIFF (24).
			assert_ok!(Assets::create(RuntimeOrigin::signed(ALICE), new_asset, ALICE, 1));
			assert_ok!(Assets::set_metadata(
				RuntimeOrigin::signed(ALICE),
				new_asset,
				b"Too Many Decimals".to_vec(),
				b"TMD".to_vec(),
				31
			));

			assert_noop!(
				Psm::add_external_asset(RuntimeOrigin::root(), new_asset),
				Error::<Test>::DecimalsRangeExceeded
			);
		});
	}

	#[test]
	fn add_external_asset_unauthorized() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Psm::add_external_asset(RuntimeOrigin::signed(ALICE), 99u32),
				DispatchError::BadOrigin
			);
		});
	}

	#[test]
	fn add_external_asset_fails_already_approved() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Psm::add_external_asset(RuntimeOrigin::root(), USDC_ASSET_ID),
				Error::<Test>::AssetAlreadyApproved
			);
		});
	}

	#[test]
	fn add_external_asset_fails_when_asset_does_not_exist() {
		new_test_ext().execute_with(|| {
			// 12345 is not created in the fungibles pallet and not approved in PSM.
			let ghost: u32 = 12345;
			assert!(!<Assets as frame_support::traits::fungibles::Inspect<u64>>::asset_exists(
				ghost
			));
			assert!(!crate::Pallet::<Test>::is_approved_asset(&ghost));

			assert_noop!(
				Psm::add_external_asset(RuntimeOrigin::root(), ghost),
				Error::<Test>::AssetDoesNotExist
			);
		});
	}

	#[test]
	fn add_external_asset_fails_too_many() {
		new_test_ext().execute_with(|| {
			use frame_support::traits::Get;
			let max: u32 = <Test as crate::Config>::MaxExternalAssets::get();
			let existing = crate::ExternalAssets::<Test>::count();
			// Fill up to the limit.
			for i in 0..(max - existing) {
				let asset_id = 1000 + i;
				create_asset_with_metadata(asset_id);
				assert_ok!(Psm::add_external_asset(RuntimeOrigin::root(), asset_id));
			}
			// One more should fail.
			create_asset_with_metadata(9999);
			assert_noop!(
				Psm::add_external_asset(RuntimeOrigin::root(), 9999),
				Error::<Test>::TooManyAssets
			);
		});
	}

	#[test]
	fn remove_external_asset_works() {
		new_test_ext().execute_with(|| {
			assert!(Psm::is_approved_asset(&USDC_ASSET_ID));

			assert_ok!(Psm::remove_external_asset(RuntimeOrigin::root(), USDC_ASSET_ID));

			assert!(!Psm::is_approved_asset(&USDC_ASSET_ID));

			System::assert_has_event(
				Event::<Test>::ExternalAssetRemoved { asset_id: USDC_ASSET_ID }.into(),
			);
		});
	}

	#[test]
	fn remove_external_asset_cleans_up_configuration() {
		new_test_ext().execute_with(|| {
			// Verify configuration exists before removal (explicitly set in genesis)
			assert!(MintingFee::<Test>::contains_key(USDC_ASSET_ID));
			assert!(RedemptionFee::<Test>::contains_key(USDC_ASSET_ID));
			assert!(AssetCeilingWeight::<Test>::contains_key(USDC_ASSET_ID));

			assert_ok!(Psm::remove_external_asset(RuntimeOrigin::root(), USDC_ASSET_ID));

			// Verify storage entries are removed (not just set to default)
			assert!(!MintingFee::<Test>::contains_key(USDC_ASSET_ID));
			assert!(!RedemptionFee::<Test>::contains_key(USDC_ASSET_ID));
			assert!(!AssetCeilingWeight::<Test>::contains_key(USDC_ASSET_ID));
		});
	}

	#[test]
	fn remove_external_asset_unauthorized() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Psm::remove_external_asset(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID),
				DispatchError::BadOrigin
			);
		});
	}

	#[test]
	fn remove_external_asset_fails_not_approved() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Psm::remove_external_asset(RuntimeOrigin::root(), 99u32),
				Error::<Test>::AssetNotApproved
			);
		});
	}

	#[test]
	fn remove_external_asset_fails_has_debt() {
		ExtBuilder::default().mints(ALICE, 1000 * PUSD_UNIT).build_and_execute(|| {
			assert_noop!(
				Psm::remove_external_asset(RuntimeOrigin::root(), USDC_ASSET_ID),
				Error::<Test>::AssetHasDebt
			);
		});
	}

	#[test]
	fn remove_external_asset_succeeds_after_debt_drained() {
		new_test_ext().execute_with(|| {
			// Zero fees so a single mint/redeem pair brings debt exactly to 0.
			set_minting_fee(USDC_ASSET_ID, Permill::zero());
			set_redemption_fee(USDC_ASSET_ID, Permill::zero());

			// With non-zero debt, removal is blocked.
			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, 1000 * PUSD_UNIT));
			assert_noop!(
				Psm::remove_external_asset(RuntimeOrigin::root(), USDC_ASSET_ID),
				Error::<Test>::AssetHasDebt
			);

			// Drain debt to zero — removal now succeeds.
			assert_ok!(Psm::redeem(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, 1000 * PUSD_UNIT));
			assert_eq!(PsmDebt::<Test>::get(USDC_ASSET_ID), 0);
			assert_ok!(Psm::remove_external_asset(RuntimeOrigin::root(), USDC_ASSET_ID));
			assert!(!ExternalAssets::<Test>::contains_key(USDC_ASSET_ID));
		});
	}

	#[test]
	fn emergency_origin_can_set_asset_status() {
		new_test_ext().execute_with(|| {
			let new_status = CircuitBreakerLevel::MintingDisabled;

			assert_ok!(Psm::set_asset_status(
				RuntimeOrigin::signed(EMERGENCY_ACCOUNT),
				USDC_ASSET_ID,
				new_status
			));

			assert_eq!(ExternalAssets::<Test>::get(USDC_ASSET_ID), Some(new_status));
		});
	}

	#[test]
	fn emergency_origin_cannot_set_minting_fee() {
		new_test_ext().execute_with(|| {
			let old_fee = MintingFee::<Test>::get(USDC_ASSET_ID);

			assert_noop!(
				Psm::set_minting_fee(
					RuntimeOrigin::signed(EMERGENCY_ACCOUNT),
					USDC_ASSET_ID,
					Permill::from_percent(5)
				),
				Error::<Test>::InsufficientPrivilege
			);

			assert_eq!(MintingFee::<Test>::get(USDC_ASSET_ID), old_fee);
		});
	}

	#[test]
	fn emergency_origin_cannot_set_redemption_fee() {
		new_test_ext().execute_with(|| {
			let old_fee = RedemptionFee::<Test>::get(USDC_ASSET_ID);

			assert_noop!(
				Psm::set_redemption_fee(
					RuntimeOrigin::signed(EMERGENCY_ACCOUNT),
					USDC_ASSET_ID,
					Permill::from_percent(5)
				),
				Error::<Test>::InsufficientPrivilege
			);

			assert_eq!(RedemptionFee::<Test>::get(USDC_ASSET_ID), old_fee);
		});
	}

	#[test]
	fn emergency_origin_cannot_set_max_psm_debt() {
		new_test_ext().execute_with(|| {
			let old_ratio = MaxPsmDebtOfTotal::<Test>::get();

			assert_noop!(
				Psm::set_max_psm_debt(
					RuntimeOrigin::signed(EMERGENCY_ACCOUNT),
					Permill::from_percent(20)
				),
				Error::<Test>::InsufficientPrivilege
			);

			assert_eq!(MaxPsmDebtOfTotal::<Test>::get(), old_ratio);
		});
	}

	#[test]
	fn emergency_origin_can_set_asset_ceiling_weight() {
		new_test_ext().execute_with(|| {
			let new_ratio = Permill::from_percent(80);

			assert_ok!(Psm::set_asset_ceiling_weight(
				RuntimeOrigin::signed(EMERGENCY_ACCOUNT),
				USDC_ASSET_ID,
				new_ratio
			));

			assert_eq!(AssetCeilingWeight::<Test>::get(USDC_ASSET_ID), new_ratio);
		});
	}

	#[test]
	fn emergency_origin_cannot_add_external_asset() {
		new_test_ext().execute_with(|| {
			let new_asset = 99u32;

			assert_noop!(
				Psm::add_external_asset(RuntimeOrigin::signed(EMERGENCY_ACCOUNT), new_asset),
				Error::<Test>::InsufficientPrivilege
			);

			assert!(!Psm::is_approved_asset(&new_asset));
		});
	}

	#[test]
	fn emergency_origin_cannot_remove_external_asset() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Psm::remove_external_asset(RuntimeOrigin::signed(EMERGENCY_ACCOUNT), USDC_ASSET_ID),
				Error::<Test>::InsufficientPrivilege
			);

			assert!(Psm::is_approved_asset(&USDC_ASSET_ID));
		});
	}

	#[test]
	fn set_minting_fee_fails_unapproved_asset() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Psm::set_minting_fee(
					RuntimeOrigin::root(),
					UNSUPPORTED_ASSET_ID,
					Permill::from_percent(5)
				),
				Error::<Test>::AssetNotApproved
			);
		});
	}

	#[test]
	fn set_redemption_fee_fails_unapproved_asset() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Psm::set_redemption_fee(
					RuntimeOrigin::root(),
					UNSUPPORTED_ASSET_ID,
					Permill::from_percent(5)
				),
				Error::<Test>::AssetNotApproved
			);
		});
	}

	#[test]
	fn set_asset_ceiling_weight_fails_unapproved_asset() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Psm::set_asset_ceiling_weight(
					RuntimeOrigin::root(),
					UNSUPPORTED_ASSET_ID,
					Permill::from_percent(50)
				),
				Error::<Test>::AssetNotApproved
			);
		});
	}
}

mod helpers {
	use super::*;

	#[test]
	fn max_psm_debt_calculation() {
		new_test_ext().execute_with(|| {
			set_mock_maximum_issuance(10_000_000 * PUSD_UNIT);
			set_max_psm_debt_ratio(Permill::from_percent(10));

			let max_debt = crate::Pallet::<Test>::max_psm_debt();
			let expected = Permill::from_percent(10).mul_floor(10_000_000 * PUSD_UNIT);

			assert_eq!(max_debt, expected);
		});
	}

	#[test]
	fn max_asset_debt_calculation() {
		new_test_ext().execute_with(|| {
			set_mock_maximum_issuance(10_000_000 * PUSD_UNIT);
			set_max_psm_debt_ratio(Permill::from_percent(10));
			set_asset_ceiling_weight(USDC_ASSET_ID, Permill::from_percent(60));

			let max_asset_debt = crate::Pallet::<Test>::max_asset_debt(USDC_ASSET_ID);
			// 10M * 10% * 60% = 600K
			let expected = Permill::from_percent(60)
				.mul_floor(Permill::from_percent(10).mul_floor(10_000_000 * PUSD_UNIT));

			assert_eq!(max_asset_debt, expected);
		});
	}

	#[test]
	fn is_approved_asset_true() {
		new_test_ext().execute_with(|| {
			assert!(crate::Pallet::<Test>::is_approved_asset(&USDC_ASSET_ID));
			assert!(crate::Pallet::<Test>::is_approved_asset(&USDT_ASSET_ID));
		});
	}

	#[test]
	fn is_approved_asset_false() {
		new_test_ext().execute_with(|| {
			assert!(!crate::Pallet::<Test>::is_approved_asset(&UNSUPPORTED_ASSET_ID));
			assert!(!crate::Pallet::<Test>::is_approved_asset(&PUSD_ASSET_ID));
		});
	}

	#[test]
	fn is_approved_asset_false_after_removal() {
		new_test_ext().execute_with(|| {
			// USDC is approved at genesis.
			assert!(crate::Pallet::<Test>::is_approved_asset(&USDC_ASSET_ID));

			// Removal flips the predicate.
			assert_ok!(Psm::remove_external_asset(RuntimeOrigin::root(), USDC_ASSET_ID));
			assert!(!crate::Pallet::<Test>::is_approved_asset(&USDC_ASSET_ID));
		});
	}

	#[test]
	fn get_reserve_returns_balance() {
		new_test_ext().execute_with(|| {
			assert_eq!(crate::Pallet::<Test>::get_reserve(USDC_ASSET_ID), 0);

			let mint_amount = 1000 * PUSD_UNIT;
			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, mint_amount));

			assert_eq!(crate::Pallet::<Test>::get_reserve(USDC_ASSET_ID), mint_amount);
		});
	}

	#[test]
	fn account_id_is_derived() {
		new_test_ext().execute_with(|| {
			let account = crate::Pallet::<Test>::account_id();
			assert_ne!(account, ALICE);
			assert_ne!(account, BOB);
			assert_ne!(account, INSURANCE_FUND);
		});
	}
}

mod circuit_breaker {
	use super::*;

	#[test]
	fn circuit_breaker_full_transition_flow() {
		new_test_ext().execute_with(|| {
			// Zero fees so every mint/redeem amount maps 1:1 onto debt.
			set_minting_fee(USDC_ASSET_ID, Permill::zero());
			set_redemption_fee(USDC_ASSET_ID, Permill::zero());

			let asset = USDC_ASSET_ID;
			let amount = 100 * PUSD_UNIT;

			// Seed debt upfront so every redeem below has something to drain
			// against — the circuit breaker check is what we want to exercise,
			// not the debt floor.
			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), asset, 500 * PUSD_UNIT));

			// Baseline: AllEnabled — both swaps work.
			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), asset, amount));
			assert_ok!(Psm::redeem(RuntimeOrigin::signed(ALICE), asset, amount));

			// Transition: AllEnabled -> MintingDisabled. Mint blocked, redeem
			// still works (useful for draining debt during a partial outage).
			assert_ok!(Psm::set_asset_status(
				RuntimeOrigin::root(),
				asset,
				CircuitBreakerLevel::MintingDisabled,
			));
			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(ALICE), asset, amount),
				Error::<Test>::MintingStopped
			);
			assert_ok!(Psm::redeem(RuntimeOrigin::signed(ALICE), asset, amount));

			// Transition: MintingDisabled -> AllDisabled. Both blocked. Debt is
			// still > 0 here, so a redeem rejection is a real circuit-breaker
			// rejection (AllSwapsStopped), not an InsufficientReserve one.
			assert_ok!(Psm::set_asset_status(
				RuntimeOrigin::root(),
				asset,
				CircuitBreakerLevel::AllDisabled,
			));
			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(ALICE), asset, amount),
				Error::<Test>::MintingStopped
			);
			assert_noop!(
				Psm::redeem(RuntimeOrigin::signed(ALICE), asset, amount),
				Error::<Test>::AllSwapsStopped
			);

			// Transition: AllDisabled -> AllEnabled. Both resume normally.
			assert_ok!(Psm::set_asset_status(
				RuntimeOrigin::root(),
				asset,
				CircuitBreakerLevel::AllEnabled,
			));
			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), asset, amount));
			assert_ok!(Psm::redeem(RuntimeOrigin::signed(ALICE), asset, amount));
		});
	}
}

mod ceiling_redistribution {
	use super::*;

	#[test]
	fn zero_weight_redistributes_ceiling_to_others() {
		new_test_ext().execute_with(|| {
			// Setup: USDC 60%, USDT 40% of PSM ceiling
			// PSM ceiling = 50% of 20M = 10M
			// USDC ceiling = 60% of 10M = 6M
			// USDT ceiling = 40% of 10M = 4M
			set_max_psm_debt_ratio(Permill::from_percent(50));
			set_asset_ceiling_weight(USDC_ASSET_ID, Permill::from_percent(60));
			set_asset_ceiling_weight(USDT_ASSET_ID, Permill::from_percent(40));

			let max_psm = crate::Pallet::<Test>::max_psm_debt();
			assert_eq!(max_psm, 10_000_000 * PUSD_UNIT);

			// Normal ceiling for USDT = 4M
			let usdt_normal_ceiling = crate::Pallet::<Test>::max_asset_debt(USDT_ASSET_ID);
			assert_eq!(usdt_normal_ceiling, 4_000_000 * PUSD_UNIT);

			// Disable USDC minting and set weight to 0% (governance workflow)
			set_asset_status(USDC_ASSET_ID, CircuitBreakerLevel::MintingDisabled);
			set_asset_ceiling_weight(USDC_ASSET_ID, Permill::from_percent(0));

			// Now USDT should be able to use the full PSM ceiling
			// total_weight_sum = 0% + 40% = 40%
			// effective_weight = 40% / 40% = 100%
			// effective_ceiling = 100% of 10M = 10M
			fund_external_asset(USDT_ASSET_ID, BOB, 10_000_000 * PUSD_UNIT);

			// Mint up to the old ceiling (4M) - should work
			assert_ok!(Psm::mint(RuntimeOrigin::signed(BOB), USDT_ASSET_ID, 4_000_000 * PUSD_UNIT));

			// Mint another 5M - this would fail with old logic but should work now
			assert_ok!(Psm::mint(RuntimeOrigin::signed(BOB), USDT_ASSET_ID, 5_000_000 * PUSD_UNIT));

			// Total USDT debt should be 9M
			assert_eq!(PsmDebt::<Test>::get(USDT_ASSET_ID), 9_000_000 * PUSD_UNIT);

			// Can't mint more than PSM ceiling (already at 9M, only 1M left)
			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(BOB), USDT_ASSET_ID, 2_000_000 * PUSD_UNIT),
				Error::<Test>::ExceedsMaxPsmDebt
			);
		});
	}

	#[test]
	fn multiple_assets_share_redistributed_ceiling() {
		new_test_ext().execute_with(|| {
			// Add a third asset
			let bridged_usdc_asset_id = 4u32;
			create_asset_with_metadata(bridged_usdc_asset_id);
			assert_ok!(Psm::add_external_asset(RuntimeOrigin::root(), bridged_usdc_asset_id));

			// Setup: USDC 50%, USDT 25%, ETH:USDC 25%
			set_max_psm_debt_ratio(Permill::from_percent(50));
			set_asset_ceiling_weight(USDC_ASSET_ID, Permill::from_percent(50));
			set_asset_ceiling_weight(USDT_ASSET_ID, Permill::from_percent(25));
			set_asset_ceiling_weight(bridged_usdc_asset_id, Permill::from_percent(25));

			// PSM ceiling = 10M. USDC ceiling = 5M.
			// Mint 4M against USDC: creating real debt before lowering ceiling.
			fund_external_asset(USDC_ASSET_ID, ALICE, 4_000_000 * PUSD_UNIT);
			assert_ok!(Psm::mint(
				RuntimeOrigin::signed(ALICE),
				USDC_ASSET_ID,
				4_000_000 * PUSD_UNIT
			));
			assert_eq!(PsmDebt::<Test>::get(USDC_ASSET_ID), 4_000_000 * PUSD_UNIT);

			// Now disable USDC and set weight to 0%
			set_asset_status(USDC_ASSET_ID, CircuitBreakerLevel::MintingDisabled);
			set_asset_ceiling_weight(USDC_ASSET_ID, Permill::from_percent(0));

			// USDT and ETH:USDC now split the full ceiling
			// total_weight_sum = 0% + 25% + 25% = 50%
			// USDT effective_weight = 25% / 50% = 50% -> 5M ceiling
			// ETH:USDC effective_weight = 25% / 50% = 50% -> 5M ceiling

			fund_external_asset(USDT_ASSET_ID, ALICE, 6_000_000 * PUSD_UNIT);

			// USDT can mint up to 5M (redistributed ceiling)
			assert_ok!(Psm::mint(
				RuntimeOrigin::signed(ALICE),
				USDT_ASSET_ID,
				5_000_000 * PUSD_UNIT
			));

			// USDT can't mint more than its redistributed ceiling
			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(ALICE), USDT_ASSET_ID, 1_000_000 * PUSD_UNIT),
				Error::<Test>::ExceedsMaxPsmDebt
			);
		});
	}

	#[test]
	fn normal_weights_use_proportional_ceilings() {
		new_test_ext().execute_with(|| {
			// Setup: USDC 60%, USDT 40%
			set_max_psm_debt_ratio(Permill::from_percent(50));
			set_asset_ceiling_weight(USDC_ASSET_ID, Permill::from_percent(60));
			set_asset_ceiling_weight(USDT_ASSET_ID, Permill::from_percent(40));

			// Both assets have non-zero weights, should use proportional ceilings
			// USDT ceiling = 40% of 10M = 4M

			fund_external_asset(USDT_ASSET_ID, BOB, 5_000_000 * PUSD_UNIT);

			// Can mint up to 4M
			assert_ok!(Psm::mint(RuntimeOrigin::signed(BOB), USDT_ASSET_ID, 4_000_000 * PUSD_UNIT));

			// Can't mint more - exceeds per-asset ceiling
			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(BOB), USDT_ASSET_ID, 1_000_000 * PUSD_UNIT),
				Error::<Test>::ExceedsMaxPsmDebt
			);
		});
	}

	#[test]
	fn single_asset_weight_always_normalizes_to_full_ceiling() {
		new_test_ext().execute_with(|| {
			// Remove USDT so only USDC remains
			assert_ok!(Psm::remove_external_asset(RuntimeOrigin::root(), USDT_ASSET_ID));

			set_max_psm_debt_ratio(Permill::from_percent(50));
			// PSM ceiling = 50% of 20M = 10M
			let mint_amount = 1000 * PUSD_UNIT;

			// Set USDC weight to 30% — with a single asset this normalizes to 100%
			set_asset_ceiling_weight(USDC_ASSET_ID, Permill::from_percent(30));
			let ceiling_at_30 = crate::Pallet::<Test>::max_asset_debt(USDC_ASSET_ID);
			assert_eq!(ceiling_at_30, 10_000_000 * PUSD_UNIT);
			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, mint_amount));

			// Change weight to 80% — still normalizes to 100%
			set_asset_ceiling_weight(USDC_ASSET_ID, Permill::from_percent(80));
			let ceiling_at_80 = crate::Pallet::<Test>::max_asset_debt(USDC_ASSET_ID);
			assert_eq!(ceiling_at_80, 10_000_000 * PUSD_UNIT);

			// Setting weight to 0% disables minting
			set_asset_ceiling_weight(USDC_ASSET_ID, Permill::from_percent(0));
			let ceiling_at_0 = crate::Pallet::<Test>::max_asset_debt(USDC_ASSET_ID);
			assert_eq!(ceiling_at_0, 0);
			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, mint_amount),
				Error::<Test>::ExceedsMaxPsmDebt
			);
		});
	}

	#[test]
	fn restoring_weight_restores_normal_ceilings() {
		new_test_ext().execute_with(|| {
			// Setup: USDC 60%, USDT 40%
			set_max_psm_debt_ratio(Permill::from_percent(50));
			set_asset_ceiling_weight(USDC_ASSET_ID, Permill::from_percent(60));
			set_asset_ceiling_weight(USDT_ASSET_ID, Permill::from_percent(40));

			fund_external_asset(USDT_ASSET_ID, BOB, 10_000_000 * PUSD_UNIT);

			// Disable USDC and set weight to 0% - USDT can use full ceiling
			set_asset_status(USDC_ASSET_ID, CircuitBreakerLevel::MintingDisabled);
			set_asset_ceiling_weight(USDC_ASSET_ID, Permill::from_percent(0));
			assert_ok!(Psm::mint(RuntimeOrigin::signed(BOB), USDT_ASSET_ID, 5_000_000 * PUSD_UNIT));

			// Re-enable USDC and restore weight
			set_asset_status(USDC_ASSET_ID, CircuitBreakerLevel::AllEnabled);
			set_asset_ceiling_weight(USDC_ASSET_ID, Permill::from_percent(60));

			// Now USDT ceiling is back to 4M, but we already have 5M debt
			// Can't mint more
			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(BOB), USDT_ASSET_ID, 1_000_000 * PUSD_UNIT),
				Error::<Test>::ExceedsMaxPsmDebt
			);
		});
	}
}

mod cycles {
	use super::*;

	fn last_event() -> Event<Test> {
		System::events()
			.into_iter()
			.filter_map(|r| if let RuntimeEvent::Psm(inner) = r.event { Some(inner) } else { None })
			.next_back()
			.expect("Expected at least one PSM event")
	}

	#[test]
	fn mint_redeem_cycles_accounting() {
		new_test_ext().execute_with(|| {
			let cycles = 10u128;
			let amount = 1000 * PUSD_UNIT;

			// Ensure user has enough funds for all cycles
			fund_external_asset(USDC_ASSET_ID, ALICE, 1_000_000 * PUSD_UNIT);
			fund_external_asset(PUSD_ASSET_ID, ALICE, 1_000_000 * PUSD_UNIT);

			// Record initial balances
			let user_external_before = get_asset_balance(USDC_ASSET_ID, ALICE);
			let user_pusd_before = get_asset_balance(PUSD_ASSET_ID, ALICE);
			let if_pusd_before = get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND);
			let psm_external_before = get_asset_balance(USDC_ASSET_ID, psm_account());

			let unit = PUSD_UNIT as f64;

			println!("=== Initial State ===");
			println!("User USDC: {:.2}", user_external_before as f64 / unit);
			println!("IF pUSD: {:.2}", if_pusd_before as f64 / unit);
			println!("PSM USDC: {:.2}", psm_external_before as f64 / unit);
			println!("PSM Debt: {:.2}", PsmDebt::<Test>::get(USDC_ASSET_ID) as f64 / unit);

			let mut total_mint_fees = 0u128;
			let mut total_redeem_fees = 0u128;

			for i in 0..cycles {
				// Mint
				assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, amount));

				let (mint_fee, pusd_received) = match last_event() {
					Event::Minted { fee, pusd_received, .. } => (fee, pusd_received),
					_ => panic!("Expected Minted event"),
				};
				total_mint_fees += mint_fee;

				println!(
					"\n=== Cycle {} - After Mint ({:.2} USDC) ===",
					i + 1,
					amount as f64 / unit
				);
				println!("Mint fee: {:.2}", mint_fee as f64 / unit);
				println!("pUSD received: {:.2}", pusd_received as f64 / unit);
				println!("User USDC: {:.2}", get_asset_balance(USDC_ASSET_ID, ALICE) as f64 / unit);
				println!("User pUSD: {:.2}", get_asset_balance(PUSD_ASSET_ID, ALICE) as f64 / unit);
				println!(
					"PSM USDC: {:.2}",
					get_asset_balance(USDC_ASSET_ID, psm_account()) as f64 / unit
				);
				println!("PSM Debt: {:.2}", PsmDebt::<Test>::get(USDC_ASSET_ID) as f64 / unit);
				println!(
					"IF pUSD: {:.2}",
					get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND) as f64 / unit
				);

				// Redeem all pUSD received
				assert_ok!(Psm::redeem(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, amount));

				let (redeem_fee, external_received) = match last_event() {
					Event::Redeemed { fee, external_received, .. } => (fee, external_received),
					_ => panic!("Expected Redeemed event"),
				};
				total_redeem_fees += redeem_fee;

				println!(
					"\n=== Cycle {} - After Redeem ({:.2} pUSD) ===",
					i + 1,
					amount as f64 / unit
				);
				println!("Redeem fee: {:.2}", redeem_fee as f64 / unit);
				println!("USDC received: {:.2}", external_received as f64 / unit);
				println!("User USDC: {:.2}", get_asset_balance(USDC_ASSET_ID, ALICE) as f64 / unit);
				println!("User pUSD: {:.2}", get_asset_balance(PUSD_ASSET_ID, ALICE) as f64 / unit);
				println!(
					"PSM USDC: {:.2}",
					get_asset_balance(USDC_ASSET_ID, psm_account()) as f64 / unit
				);
				println!("PSM Debt: {:.2}", PsmDebt::<Test>::get(USDC_ASSET_ID) as f64 / unit);
				println!(
					"IF pUSD: {:.2}",
					get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND) as f64 / unit
				);
			}

			// Final balances
			let user_external_after = get_asset_balance(USDC_ASSET_ID, ALICE);
			let user_pusd_after = get_asset_balance(PUSD_ASSET_ID, ALICE);

			let if_pusd_after = get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND);
			let psm_external_after = get_asset_balance(USDC_ASSET_ID, psm_account());
			let psm_debt_after = PsmDebt::<Test>::get(USDC_ASSET_ID);

			println!("\n=== Final State ===");
			println!("User USDC: {:.2}", user_external_after as f64 / unit);
			println!("User pUSD: {:.2}", get_asset_balance(PUSD_ASSET_ID, ALICE) as f64 / unit);
			println!("IF pUSD: {:.2}", if_pusd_after as f64 / unit);
			println!("PSM USDC: {:.2}", psm_external_after as f64 / unit);
			println!("PSM Debt: {:.2}", psm_debt_after as f64 / unit);
			println!("Total mint fees: {:.2}", total_mint_fees as f64 / unit);
			println!("Total redeem fees: {:.2}", total_redeem_fees as f64 / unit);

			let total_fees = total_mint_fees + total_redeem_fees;
			let if_increase = if_pusd_after - if_pusd_before;
			let user_decrease =
				user_external_before - user_external_after + user_pusd_before - user_pusd_after;

			println!("\n=== Verification ===");
			println!("Total fees collected: {:.2}", total_fees as f64 / unit);
			println!("IF increase: {:.2}", if_increase as f64 / unit);
			println!("User decrease: {:.2}", user_decrease as f64 / unit);

			// Assertions
			// 1. IF balance increased by total fees (mint fees + redeem fees)
			assert_eq!(if_increase, total_fees, "IF should receive all fees");

			// 2. PSM external balance equals what remained after redemptions
			assert_eq!(psm_external_after, psm_debt_after, "PSM external = PSM debt");

			// 3. User external decrease equals total fees paid
			assert_eq!(user_decrease, total_fees, "User loss equals fees");

			// 4. PSM debt equals PSM external stablecoin balance
			assert_eq!(
				psm_debt_after,
				get_asset_balance(USDC_ASSET_ID, psm_account()),
				"PSM debt equals PSM external balance"
			);
		});
	}

	#[test]
	fn infinite_until_debt_ceiling() {
		new_test_ext().execute_with(|| {
			let amount = 100_000 * PUSD_UNIT;

			// Set ceiling for ~1000 cycles
			// Each cycle: mint 100000, redeem 100000 → debt grows by ~1000 per cycle
			// For 1000 cycles: need ceiling > 110 + 1000 * 2.19 ≈ 2300
			// 10M * 0.025% = 2500 units ceiling
			set_max_psm_debt_ratio(Permill::from_percent(10));
			set_asset_ceiling_weight(USDC_ASSET_ID, Permill::from_percent(50));

			let max_debt = crate::Pallet::<Test>::max_asset_debt(USDC_ASSET_ID);

			println!("MAX DEBT: {}", max_debt);

			// Fund user with more than enough to hit the ceiling
			let funding = max_debt * 2;
			fund_external_asset(USDC_ASSET_ID, ALICE, funding);
			fund_external_asset(PUSD_ASSET_ID, ALICE, funding);
			// Record initial balances
			let user_external_before = get_asset_balance(USDC_ASSET_ID, ALICE);
			let user_pusd_before = get_asset_balance(PUSD_ASSET_ID, ALICE);
			let if_pusd_before = get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND);
			let psm_external_before = get_asset_balance(USDC_ASSET_ID, psm_account());

			let unit = PUSD_UNIT as f64;

			println!("=== Initial State ===");
			println!("Max debt ceiling: {:.2}", max_debt as f64 / unit);
			println!("User USDC: {:.2}", user_external_before as f64 / unit);
			println!("User pUSD: {:.2}", user_pusd_before as f64 / unit);
			println!("IF pUSD: {:.2}", if_pusd_before as f64 / unit);
			println!("PSM USDC: {:.2}", psm_external_before as f64 / unit);
			println!("PSM Debt: {:.2}", PsmDebt::<Test>::get(USDC_ASSET_ID) as f64 / unit);

			let mut total_mint_fees = 0u128;
			let mut total_redeem_fees = 0u128;
			let mut cycle = 0u128;

			loop {
				let current_debt = PsmDebt::<Test>::get(USDC_ASSET_ID);

				// Check if we can mint another `amount`
				if current_debt + amount > max_debt {
					println!("\n=== Debt ceiling reached after {} cycles ===", cycle);
					println!("Current debt: {:.2}", current_debt as f64 / unit);
					println!("Max debt: {:.2}", max_debt as f64 / unit);
					println!("Cannot mint {:.2} more (would exceed ceiling)", amount as f64 / unit);
					break;
				}

				cycle += 1;

				// Mint
				assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, amount));

				let (mint_fee, pusd_received) = match last_event() {
					Event::Minted { fee, pusd_received, .. } => (fee, pusd_received),
					_ => panic!("Expected Minted event"),
				};
				total_mint_fees += mint_fee;

				println!(
					"\n=== Cycle {} - After Mint ({:.2} USDC) ===",
					cycle,
					amount as f64 / unit
				);
				println!("Mint fee: {:.2}", mint_fee as f64 / unit);
				println!("pUSD received: {:.2}", pusd_received as f64 / unit);
				println!("User USDC: {:.2}", get_asset_balance(USDC_ASSET_ID, ALICE) as f64 / unit);
				println!("User pUSD: {:.2}", get_asset_balance(PUSD_ASSET_ID, ALICE) as f64 / unit);
				println!(
					"PSM USDC: {:.2}",
					get_asset_balance(USDC_ASSET_ID, psm_account()) as f64 / unit
				);
				println!("PSM Debt: {:.2}", PsmDebt::<Test>::get(USDC_ASSET_ID) as f64 / unit);
				println!(
					"IF pUSD: {:.2}",
					get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND) as f64 / unit
				);

				// Redeem all pUSD received
				assert_ok!(Psm::redeem(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, amount));

				let (redeem_fee, external_received) = match last_event() {
					Event::Redeemed { fee, external_received, .. } => (fee, external_received),
					_ => panic!("Expected Redeemed event"),
				};
				total_redeem_fees += redeem_fee;

				println!(
					"\n=== Cycle {} - After Redeem ({:.2} pUSD) ===",
					cycle,
					amount as f64 / unit
				);
				println!("Redeem fee: {:.2}", redeem_fee as f64 / unit);
				println!("USDC received: {:.2}", external_received as f64 / unit);
				println!("User USDC: {:.2}", get_asset_balance(USDC_ASSET_ID, ALICE) as f64 / unit);
				println!("User pUSD: {:.2}", get_asset_balance(PUSD_ASSET_ID, ALICE) as f64 / unit);
				println!(
					"PSM USDC: {:.2}",
					get_asset_balance(USDC_ASSET_ID, psm_account()) as f64 / unit
				);
				println!("PSM Debt: {:.2}", PsmDebt::<Test>::get(USDC_ASSET_ID) as f64 / unit);
				println!(
					"IF pUSD: {:.2}",
					get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND) as f64 / unit
				);
			}

			// Final balances
			let user_external_after = get_asset_balance(USDC_ASSET_ID, ALICE);
			let user_pusd_after = get_asset_balance(PUSD_ASSET_ID, ALICE);

			let if_pusd_after = get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND);
			let psm_external_after = get_asset_balance(USDC_ASSET_ID, psm_account());
			let psm_debt_after = PsmDebt::<Test>::get(USDC_ASSET_ID);

			println!("\n=== Final State ===");
			println!("Total cycles: {}", cycle);
			println!("User USDC: {:.2}", user_external_after as f64 / unit);
			println!("User pUSD: {:.2}", user_pusd_after as f64 / unit);

			println!("IF pUSD: {:.2}", if_pusd_after as f64 / unit);
			println!("PSM USDC: {:.2}", psm_external_after as f64 / unit);
			println!("PSM Debt: {:.2}", psm_debt_after as f64 / unit);
			println!("Total mint fees: {:.2}", total_mint_fees as f64 / unit);
			println!("Total redeem fees: {:.2}", total_redeem_fees as f64 / unit);

			let total_fees = total_mint_fees + total_redeem_fees;
			let if_increase = if_pusd_after - if_pusd_before;
			let user_decrease =
				user_external_before - user_external_after + user_pusd_before - user_pusd_after;

			println!("\n=== Verification ===");
			println!("Total fees collected: {:.2}", total_fees as f64 / unit);
			println!("IF increase: {:.2}", if_increase as f64 / unit);
			println!("User decrease: {:.2}", user_decrease as f64 / unit);

			// Assertions
			assert!(cycle > 0, "Should have completed at least one cycle");
			assert_eq!(if_increase, total_fees, "IF should receive all fees");
			assert_eq!(psm_external_after, psm_debt_after, "PSM external = PSM debt");
			assert_eq!(user_decrease, total_fees, "User loss equals fees");
			assert!(psm_debt_after <= max_debt, "PSM debt should not exceed ceiling");

			// Redeem to fully drain PSM debt to 0
			// When redeeming: external_received = pusd_paid - fee = pusd_paid * (1 - fee_rate)
			// So: pusd_paid = external_received / (1 - fee_rate)
			let fee_rate = RedemptionFee::<Test>::get(USDC_ASSET_ID);
			println!("Fee Rate: {:#?}", fee_rate);
			let complement_parts = 1_000_000u128 - fee_rate.deconstruct() as u128;
			println!("Complemenet Part: {:#?}", complement_parts);
			let pusd_needed = (psm_debt_after * 1_000_000).div_ceil(complement_parts);
			println!("pUSD Needed: {:#?}", pusd_needed);

			assert_ok!(Psm::redeem(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, pusd_needed));

			let (redeem_fee, _external_received) = match last_event() {
				Event::Redeemed { fee, external_received, .. } => (fee, external_received),
				_ => panic!("Expected Redeemed event"),
			};
			total_redeem_fees += redeem_fee;

			// Final balances (after the drain redemption)
			let user_external_after = get_asset_balance(USDC_ASSET_ID, ALICE);
			let user_pusd_after = get_asset_balance(PUSD_ASSET_ID, ALICE);
			let if_pusd_after = get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND);
			let psm_external_after = get_asset_balance(USDC_ASSET_ID, psm_account());
			let psm_debt_after = PsmDebt::<Test>::get(USDC_ASSET_ID);

			let total_fees = total_mint_fees + total_redeem_fees;
			let if_increase = if_pusd_after - if_pusd_before;
			let user_decrease =
				user_external_before - user_external_after + user_pusd_before - user_pusd_after;

			println!("\n=== Final State ===");
			println!("Total cycles: {}", cycle);
			println!("User USDC: {:.2}", user_external_after as f64 / unit);
			println!("User pUSD: {:.2}", user_pusd_after as f64 / unit);

			println!("IF pUSD: {:.2}", if_pusd_after as f64 / unit);
			println!("PSM USDC: {:.2}", psm_external_after as f64 / unit);
			println!("PSM Debt: {:.2}", psm_debt_after as f64 / unit);
			println!("Total mint fees: {:.2}", total_mint_fees as f64 / unit);
			println!("Total redeem fees: {:.2}", total_redeem_fees as f64 / unit);

			println!("\n=== Verification ===");
			println!("Total fees collected: {:.2}", total_fees as f64 / unit);
			println!("IF increase: {:.2}", if_increase as f64 / unit);
			println!("User decrease: {:.2}", user_decrease as f64 / unit);

			assert_eq!(psm_debt_after, 0, "PSM debt should be fully drained to 0");
			assert_eq!(psm_external_after, 0, "PSM USDC balance should be 0");
			assert_eq!(user_external_after, user_external_before, "User USDC should be unchanged");
			assert_eq!(
				user_pusd_after + if_pusd_after,
				user_pusd_before + if_pusd_before,
				"pUSD conservation: user + IF at end should equal initial balances"
			);
			assert_eq!(
				total_fees, if_increase,
				"Total fees (mint + redeem) should equal IF pUSD increase"
			);
		});
	}
}

/// Tests for normalization between pUSD and external assets with different decimal
/// precision. Uses `USDX` (2 decimals) and `DAI_MOCK` (18 decimals) against a
/// 6-decimal pUSD. Both helper assets are created in pallet-assets genesis but
/// registered with PSM via `register_external_asset_with_weight` inside each test.
mod decimal_scaling {
	use super::*;
	use crate::{AssetDecimals, MAX_DECIMALS_DIFF};

	fn set_zero_fees(asset_id: u32) {
		set_minting_fee(asset_id, Permill::zero());
		set_redemption_fee(asset_id, Permill::zero());
	}

	// Conversion helpers

	#[test]
	fn external_to_pusd_same_decimals_is_identity() {
		new_test_ext().execute_with(|| {
			assert_eq!(Psm::external_to_pusd(1_000_000, 6, 6).unwrap(), 1_000_000);
		});
	}

	#[test]
	fn external_to_pusd_scale_up_is_exact() {
		new_test_ext().execute_with(|| {
			// USDX (2) -> pUSD (6): multiply by 10^4.
			assert_eq!(Psm::external_to_pusd(100, 2, 6).unwrap(), 1_000_000);
		});
	}

	#[test]
	fn external_to_pusd_scale_down_truncates() {
		new_test_ext().execute_with(|| {
			// DAI (18) -> pUSD (6): divide by 10^12, floor.
			assert_eq!(Psm::external_to_pusd(1_500_000_000_000_000_123, 18, 6).unwrap(), 1_500_000);
		});
	}

	#[test]
	fn pusd_to_external_round_trip_bounds() {
		new_test_ext().execute_with(|| {
			// For any amount, round-trip should shrink or preserve.
			for (ext_decimals, pusd_decimals) in [(2u8, 6u8), (6, 6), (18, 6), (6, 18), (6, 2)] {
				for amount in [0u128, 1, 100, 1_234_567, 10u128.pow(18)] {
					let fwd = Psm::external_to_pusd(amount, ext_decimals, pusd_decimals).unwrap();
					let rtp = Psm::pusd_to_external(fwd, ext_decimals, pusd_decimals).unwrap();
					assert!(rtp <= amount, "round-trip grew: amount={} got {}", amount, rtp);
				}
			}
		});
	}

	#[test]
	fn conversion_overflow_surfaces_error() {
		new_test_ext().execute_with(|| {
			// 10^40 overflows u128 (max ~3.4e38).
			assert!(Psm::external_to_pusd(1, 0, 40).is_err());
		});
	}

	// Mint with scale-up (fewer external decimals)

	#[test]
	fn mint_scale_up_usdx_exact_no_dust() {
		new_test_ext().execute_with(|| {
			register_external_asset_with_weight(USDX_ASSET_ID, Permill::from_percent(100));
			set_zero_fees(USDX_ASSET_ID);

			// 200 USDX raw = 2_00 = pUSD equivalent = 200 * 10^4 = 2_000_000 = 2 pUSD.
			// Use 10_000 * USDX_UNIT = 1_000_000 raw USDX so pUSD equivalent is above
			// MinSwapAmount (100 * PUSD_UNIT = 10^8).
			let usdx_raw = 10_000 * USDX_UNIT; // 1_000_000 raw USDX
			let expected_pusd = 10_000 * PUSD_UNIT; // 10_000 pUSD
			let alice_usdx_before = get_asset_balance(USDX_ASSET_ID, ALICE);

			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, usdx_raw));

			// User spent exactly usdx_raw (no dust path on scale-up).
			assert_eq!(get_asset_balance(USDX_ASSET_ID, ALICE), alice_usdx_before - usdx_raw);
			assert_eq!(get_asset_balance(USDX_ASSET_ID, psm_account()), usdx_raw);
			assert_eq!(get_asset_balance(PUSD_ASSET_ID, ALICE), expected_pusd);
			assert_eq!(PsmDebt::<Test>::get(USDX_ASSET_ID), expected_pusd);
		});
	}

	// Mint with scale-down (more external decimals)

	#[test]
	fn mint_scale_down_dai_leaves_dust_with_user() {
		new_test_ext().execute_with(|| {
			register_external_asset_with_weight(DAI_MOCK_ASSET_ID, Permill::from_percent(100));
			set_zero_fees(DAI_MOCK_ASSET_ID);

			// 100 DAI + 123 wei. pUSD equivalent = 100 * 10^6 = 10^8 (= MinSwapAmount).
			let dai_raw = 100 * DAI_UNIT + 123;
			let effective_dai = 100 * DAI_UNIT; // truncated to round-trip boundary
			let expected_pusd = 100 * PUSD_UNIT;
			let alice_before = get_asset_balance(DAI_MOCK_ASSET_ID, ALICE);

			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), DAI_MOCK_ASSET_ID, dai_raw));

			// Only effective amount left the user; dust (123 wei) stays.
			assert_eq!(get_asset_balance(DAI_MOCK_ASSET_ID, ALICE), alice_before - effective_dai);
			assert_eq!(get_asset_balance(DAI_MOCK_ASSET_ID, psm_account()), effective_dai);
			assert_eq!(get_asset_balance(PUSD_ASSET_ID, ALICE), expected_pusd);
			assert_eq!(PsmDebt::<Test>::get(DAI_MOCK_ASSET_ID), expected_pusd);
		});
	}

	#[test]
	fn mint_scale_down_dai_with_fee_keeps_dust_charges_only_fee() {
		new_test_ext().execute_with(|| {
			register_external_asset_with_weight(DAI_MOCK_ASSET_ID, Permill::from_percent(100));
			set_minting_fee(DAI_MOCK_ASSET_ID, Permill::from_percent(1));

			// DAI factor is 10^12 (18 decimals vs 6 for pUSD).
			//   deposit            = 100 DAI + 123 wei  (raw external units)
			//   pusd_equivalent    = deposit / 10^12    = 100 pUSD
			//     (the 123 wei truncates — stays in ALICE's wallet)
			//   effective_external = pusd_equivalent * 10^12 = 100 DAI (exact)
			//   fee (1%)           = mul_ceil(1% * pusd_equivalent) = 1 pUSD
			//   pusd_to_user       = pusd_equivalent - fee = 99 pUSD
			//   dust (external)    = deposit - effective_external = 123 wei
			let deposit = 100 * DAI_UNIT + 123;
			let pusd_equivalent = 100 * PUSD_UNIT;
			let effective_external = 100 * DAI_UNIT;
			let fee = 1 * PUSD_UNIT;
			let pusd_to_user = 99 * PUSD_UNIT;
			let dust = 123u128;
			// Sanity: the submitted external amount is fully accounted for.
			assert_eq!(deposit, effective_external + dust);
			assert_eq!(pusd_equivalent, pusd_to_user + fee);

			let alice_dai_before = get_asset_balance(DAI_MOCK_ASSET_ID, ALICE);
			let alice_pusd_before = get_asset_balance(PUSD_ASSET_ID, ALICE);
			let if_before = get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND);
			let debt_before = PsmDebt::<Test>::get(DAI_MOCK_ASSET_ID);

			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), DAI_MOCK_ASSET_ID, deposit));

			// ALICE keeps exactly `dust` of the submitted DAI; only
			// `effective_external` left her wallet into the PSM reserve.
			assert_eq!(
				get_asset_balance(DAI_MOCK_ASSET_ID, ALICE),
				alice_dai_before - deposit + dust,
				"dust must remain with the caller"
			);
			assert_eq!(get_asset_balance(DAI_MOCK_ASSET_ID, psm_account()), effective_external);
			// Minted pUSD split: user receives `pusd_to_user`, fee dest gets `fee`.
			assert_eq!(get_asset_balance(PUSD_ASSET_ID, ALICE), alice_pusd_before + pusd_to_user);
			assert_eq!(get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND), if_before + fee);
			// Debt grows by the pUSD-equivalent of the backed deposit.
			assert_eq!(PsmDebt::<Test>::get(DAI_MOCK_ASSET_ID), debt_before + pusd_equivalent);

			// Minted event carries the effective external amount (what actually
			// entered the reserve), not the raw submission.
			System::assert_has_event(
				Event::<Test>::Minted {
					who: ALICE,
					asset_id: DAI_MOCK_ASSET_ID,
					external_amount: effective_external,
					pusd_received: pusd_to_user,
					fee,
				}
				.into(),
			);
		});
	}

	#[test]
	fn mint_rejects_when_pusd_equivalent_is_zero() {
		new_test_ext().execute_with(|| {
			register_external_asset_with_weight(DAI_MOCK_ASSET_ID, Permill::from_percent(100));

			// 999 wei DAI -> pUSD = 999 / 10^12 = 0.
			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(ALICE), DAI_MOCK_ASSET_ID, 999),
				Error::<Test>::AmountTooSmallAfterConversion
			);
		});
	}

	#[test]
	fn mint_min_swap_is_enforced_on_pusd_side() {
		new_test_ext().execute_with(|| {
			register_external_asset_with_weight(DAI_MOCK_ASSET_ID, Permill::from_percent(100));

			// 50 DAI = 50 pUSD equivalent, below MinSwapAmount (100 pUSD).
			let below = 50 * DAI_UNIT;
			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(ALICE), DAI_MOCK_ASSET_ID, below),
				Error::<Test>::BelowMinimumSwap
			);
		});
	}

	// Redeem with scale-up (more external decimals)

	#[test]
	fn redeem_scale_up_dai_exact_no_dust() {
		new_test_ext().execute_with(|| {
			register_external_asset_with_weight(DAI_MOCK_ASSET_ID, Permill::from_percent(100));
			set_zero_fees(DAI_MOCK_ASSET_ID);

			// First mint so PSM has reserve and debt.
			let pusd_amount = 1000 * PUSD_UNIT;
			let dai_raw = 1000 * DAI_UNIT;
			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), DAI_MOCK_ASSET_ID, dai_raw));
			assert_eq!(PsmDebt::<Test>::get(DAI_MOCK_ASSET_ID), pusd_amount);

			// Redeem 500 pUSD -> expect exactly 500 DAI back.
			let redeem = 500 * PUSD_UNIT;
			let alice_dai_before = get_asset_balance(DAI_MOCK_ASSET_ID, ALICE);
			let alice_pusd_before = get_asset_balance(PUSD_ASSET_ID, ALICE);

			assert_ok!(Psm::redeem(RuntimeOrigin::signed(ALICE), DAI_MOCK_ASSET_ID, redeem));

			assert_eq!(
				get_asset_balance(DAI_MOCK_ASSET_ID, ALICE),
				alice_dai_before + 500 * DAI_UNIT
			);
			assert_eq!(get_asset_balance(PUSD_ASSET_ID, ALICE), alice_pusd_before - redeem);
			assert_eq!(PsmDebt::<Test>::get(DAI_MOCK_ASSET_ID), redeem);
		});
	}

	// Redeem with scale-down (fewer external decimals)

	#[test]
	fn redeem_scale_down_usdx_with_fee_keeps_dust_charges_only_fee() {
		new_test_ext().execute_with(|| {
			register_external_asset_with_weight(USDX_ASSET_ID, Permill::from_percent(100));
			set_minting_fee(USDX_ASSET_ID, Permill::zero());

			// Seed reserve and ALICE's pUSD balance with a prior 0-fee mint.
			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, 10_000 * USDX_UNIT));

			set_redemption_fee(USDX_ASSET_ID, Permill::from_percent(1));

			// Pick a redeem amount that produces both a non-zero fee AND post-fee
			// round-trip dust. USDX factor is 10^4.
			//   redeem       = 200 pUSD + 17 units
			//   fee (1%)     = mul_ceil(1% * redeem) = 2 pUSD + 1 unit
			//   pusd_net     = redeem - fee         = 198 pUSD + 16 units
			//   external_out = pusd_net / 10^4      = 198 USDX (19_800 raw)
			//   eff_pusd_net = external_out * 10^4  = 198 pUSD (198_000_000 raw)
			//   dust         = pusd_net - eff_pusd_net = 16   ← stays with user
			let redeem = 200 * PUSD_UNIT + 17;
			let fee = 2 * PUSD_UNIT + 1;
			let eff_pusd_net = 198 * PUSD_UNIT;
			let external_out = 198 * USDX_UNIT;
			let dust = 16u128;
			// Sanity: the submitted amount is fully accounted for.
			assert_eq!(redeem, eff_pusd_net + fee + dust);

			let alice_usdx_before = get_asset_balance(USDX_ASSET_ID, ALICE);
			let alice_pusd_before = get_asset_balance(PUSD_ASSET_ID, ALICE);
			let if_before = get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND);
			let debt_before = PsmDebt::<Test>::get(USDX_ASSET_ID);

			assert_ok!(Psm::redeem(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, redeem));

			// User receives exactly `external_out` USDX.
			assert_eq!(get_asset_balance(USDX_ASSET_ID, ALICE), alice_usdx_before + external_out);
			// ALICE keeps exactly `dust` of the submitted amount — the rest
			// (eff_pusd_net burned + fee transferred) left her wallet.
			assert_eq!(
				get_asset_balance(PUSD_ASSET_ID, ALICE),
				alice_pusd_before - redeem + dust,
				"dust must remain with the caller"
			);
			// FeeDestination receives only the nominal fee, not fee + dust.
			assert_eq!(get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND), if_before + fee);
			// Debt reduces by exactly the round-tripped pUSD amount.
			assert_eq!(PsmDebt::<Test>::get(USDX_ASSET_ID), debt_before - eff_pusd_net);

			// Redeemed event matches the actual movements: `pusd_paid` reflects
			// the pUSD actually charged (burn + fee) with round-trip dust excluded,
			// `external_received` is the round-tripped external amount, and `fee`
			// is the nominal configured fee.
			System::assert_has_event(
				Event::<Test>::Redeemed {
					who: ALICE,
					asset_id: USDX_ASSET_ID,
					pusd_paid: eff_pusd_net + fee,
					external_received: external_out,
					fee,
				}
				.into(),
			);
		});
	}

	#[test]
	fn redeem_scale_down_usdx_dust_stays_with_user() {
		new_test_ext().execute_with(|| {
			register_external_asset_with_weight(USDX_ASSET_ID, Permill::from_percent(100));
			set_zero_fees(USDX_ASSET_ID);

			// Mint first so PSM has reserve. 10_000 USDX -> 10_000 pUSD debt.
			let usdx_raw = 10_000 * USDX_UNIT;
			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, usdx_raw));

			// Redeem 100 pUSD + 1 unit of dust. USDX has 2 decimals, so pUSD -> USDX
			// divides by 10^4. 100_000_001 pUSD -> 10_000 USDX (= 100_000_000 pUSD
			// worth). The 1-unit dust remains in ALICE's wallet (symmetric with
			// mint's behavior of leaving dust with the caller).
			let redeem = 100 * PUSD_UNIT + 1;
			let expected_usdx_out = 100 * USDX_UNIT;
			let alice_usdx_before = get_asset_balance(USDX_ASSET_ID, ALICE);
			let alice_pusd_before = get_asset_balance(PUSD_ASSET_ID, ALICE);
			let if_before = get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND);
			let debt_before = PsmDebt::<Test>::get(USDX_ASSET_ID);

			assert_ok!(Psm::redeem(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, redeem));

			assert_eq!(
				get_asset_balance(USDX_ASSET_ID, ALICE),
				alice_usdx_before + expected_usdx_out
			);
			// Only the round-tripped net (100 pUSD) was burned from ALICE; fees are
			// zero here so the dust (1 unit) stays in her wallet.
			assert_eq!(
				get_asset_balance(PUSD_ASSET_ID, ALICE),
				alice_pusd_before - 100 * PUSD_UNIT
			);
			// Fee destination receives nothing (fee rate is zero).
			assert_eq!(get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND), if_before);
			// Debt reduced by what actually left the reserve in pUSD terms.
			assert_eq!(PsmDebt::<Test>::get(USDX_ASSET_ID), debt_before - 100 * PUSD_UNIT);
		});
	}

	#[test]
	fn redeem_succeeds_with_100_percent_fee() {
		new_test_ext().execute_with(|| {
			register_external_asset_with_weight(USDX_ASSET_ID, Permill::from_percent(100));
			set_zero_fees(USDX_ASSET_ID);

			// Seed the PSM reserve and ALICE's pUSD balance with a prior mint so
			// the redeem below has something to operate on.
			let usdx_raw = 10_000 * USDX_UNIT;
			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, usdx_raw));
			set_redemption_fee(USDX_ASSET_ID, Permill::from_percent(100));

			let alice_usdx_before = get_asset_balance(USDX_ASSET_ID, ALICE);
			let if_before = get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND);

			// With fee = 100%, `pusd_net` is zero. `external_out = 0` is then
			// legitimate (no truncation bug), so the swap must succeed: no pUSD
			// is burned, no external asset is transferred, the entire redeem
			// amount moves to the fee destination.
			let redeem = 100 * PUSD_UNIT;
			assert_ok!(Psm::redeem(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, redeem));

			// User receives zero USDX (100% fee).
			assert_eq!(get_asset_balance(USDX_ASSET_ID, ALICE), alice_usdx_before);
			// Fee destination gets the full redeemed pUSD.
			assert_eq!(get_asset_balance(PUSD_ASSET_ID, INSURANCE_FUND), if_before + redeem);
		});
	}

	#[test]
	fn redeem_rejects_when_external_out_truncates_to_zero() {
		new_test_ext().execute_with(|| {
			register_external_asset_with_weight(USDX_ASSET_ID, Permill::from_percent(100));
			set_zero_fees(USDX_ASSET_ID);

			// Seed the PSM reserve and ALICE's pUSD balance with a prior mint.
			let usdx_raw = 10_000 * USDX_UNIT;
			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, usdx_raw));

			// Configure an extreme redemption fee so `pusd_net > 0` but falls
			// below one USDX raw unit (factor 10^4). With MinSwapAmount = 10^8
			// pUSD and a 99.9999% fee:
			//   fee      = mul_ceil(999_999 * 10^8 / 10^6) = 99_999_900
			//   pusd_net = 10^8 - 99_999_900 = 100
			//   external = 100 / 10^4 = 0  ← genuine truncation, must reject.
			set_redemption_fee(USDX_ASSET_ID, Permill::from_parts(999_999));

			let redeem = 100 * PUSD_UNIT;
			assert_noop!(
				Psm::redeem(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, redeem),
				Error::<Test>::AmountTooSmallAfterConversion
			);
		});
	}

	// Runtime decimals guard

	#[test]
	fn mint_halts_when_asset_decimals_drift() {
		new_test_ext().execute_with(|| {
			register_external_asset_with_weight(USDX_ASSET_ID, Permill::from_percent(100));

			// Owner (ALICE) unilaterally changes USDX decimals from 2 -> 4.
			assert_ok!(Assets::set_metadata(
				RuntimeOrigin::signed(ALICE),
				USDX_ASSET_ID,
				b"USDX".to_vec(),
				b"USDX".to_vec(),
				4
			));

			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(BOB), USDX_ASSET_ID, 10_000 * USDX_UNIT),
				Error::<Test>::DecimalsMismatch
			);
		});
	}

	#[test]
	fn redeem_halts_when_asset_decimals_drift() {
		new_test_ext().execute_with(|| {
			register_external_asset_with_weight(USDX_ASSET_ID, Permill::from_percent(100));
			set_zero_fees(USDX_ASSET_ID);

			// Mint first, then change decimals.
			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, 10_000 * USDX_UNIT));
			assert_ok!(Assets::set_metadata(
				RuntimeOrigin::signed(ALICE),
				USDX_ASSET_ID,
				b"USDX".to_vec(),
				b"USDX".to_vec(),
				4
			));

			assert_noop!(
				Psm::redeem(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, 100 * PUSD_UNIT),
				Error::<Test>::DecimalsMismatch
			);
		});
	}

	#[test]
	fn mint_halts_when_stable_decimals_drift() {
		new_test_ext().execute_with(|| {
			// pUSD starts at 6 decimals; StableDecimals snapshot matches. The owner
			// (ALICE) changes the stable asset's live metadata to simulate drift.
			assert_ok!(Assets::set_metadata(
				RuntimeOrigin::signed(ALICE),
				PUSD_ASSET_ID,
				b"pUSD".to_vec(),
				b"pUSD".to_vec(),
				8
			));

			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, 1000 * PUSD_UNIT),
				Error::<Test>::DecimalsMismatch
			);
		});
	}

	#[test]
	fn redeem_halts_when_stable_decimals_drift() {
		new_test_ext().execute_with(|| {
			// Seed ALICE's pUSD balance and PSM reserve with a prior mint, then
			// drift the stable asset's decimals.
			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, 1000 * PUSD_UNIT));
			assert_ok!(Assets::set_metadata(
				RuntimeOrigin::signed(ALICE),
				PUSD_ASSET_ID,
				b"pUSD".to_vec(),
				b"pUSD".to_vec(),
				8
			));

			assert_noop!(
				Psm::redeem(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, 100 * PUSD_UNIT),
				Error::<Test>::DecimalsMismatch
			);
		});
	}

	#[test]
	fn mint_fails_when_asset_decimals_snapshot_missing() {
		new_test_ext().execute_with(|| {
			// USDC is approved in genesis but we clear its decimals snapshot to
			// simulate a partially-migrated state.
			crate::AssetDecimals::<Test>::remove(USDC_ASSET_ID);

			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, 1000 * PUSD_UNIT),
				Error::<Test>::UnsupportedAsset
			);
		});
	}

	#[test]
	fn redeem_fails_when_asset_decimals_snapshot_missing() {
		new_test_ext().execute_with(|| {
			fund_pusd(ALICE, 1000 * PUSD_UNIT);
			crate::AssetDecimals::<Test>::remove(USDC_ASSET_ID);

			assert_noop!(
				Psm::redeem(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, 100 * PUSD_UNIT),
				Error::<Test>::UnsupportedAsset
			);
		});
	}

	#[test]
	fn mint_fails_when_stable_decimals_snapshot_missing() {
		new_test_ext().execute_with(|| {
			crate::StableDecimals::<Test>::kill();

			assert_noop!(
				Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, 1000 * PUSD_UNIT),
				Error::<Test>::Unexpected
			);
		});
	}

	#[test]
	fn redeem_fails_when_stable_decimals_snapshot_missing() {
		new_test_ext().execute_with(|| {
			fund_pusd(ALICE, 1000 * PUSD_UNIT);
			crate::StableDecimals::<Test>::kill();

			assert_noop!(
				Psm::redeem(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, 100 * PUSD_UNIT),
				Error::<Test>::Unexpected
			);
		});
	}

	// Snapshot and bookkeeping

	#[test]
	fn asset_decimals_snapshot_recorded_on_add_and_cleaned_on_remove() {
		new_test_ext().execute_with(|| {
			register_external_asset_with_weight(USDX_ASSET_ID, Permill::from_percent(100));
			assert_eq!(AssetDecimals::<Test>::get(USDX_ASSET_ID), Some(2));

			assert_ok!(Psm::remove_external_asset(RuntimeOrigin::root(), USDX_ASSET_ID));
			assert_eq!(AssetDecimals::<Test>::get(USDX_ASSET_ID), None);
		});
	}

	#[test]
	fn max_decimals_diff_const_is_protective() {
		// Compile-time sanity: the chosen bound is wide but below the overflow point.
		// 10^24 fits comfortably in u128 (< 10^38), and leaves ~10^14 headroom on
		// balances. The const is documented; this asserts it has not been widened
		// beyond the safe range.
		assert!(MAX_DECIMALS_DIFF <= 30);
	}

	// Mixed-decimal aggregate bookkeeping

	#[test]
	fn aggregate_debt_accrues_in_pusd_units_across_mixed_decimal_assets() {
		new_test_ext().execute_with(|| {
			register_external_asset_with_weight(USDX_ASSET_ID, Permill::from_percent(50));
			register_external_asset_with_weight(DAI_MOCK_ASSET_ID, Permill::from_percent(50));
			set_zero_fees(USDX_ASSET_ID);
			set_zero_fees(DAI_MOCK_ASSET_ID);

			// Mint 500 pUSD-equivalent via USDX, 1500 pUSD-equivalent via DAI.
			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, 500 * USDX_UNIT));
			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), DAI_MOCK_ASSET_ID, 1500 * DAI_UNIT));

			assert_eq!(PsmDebt::<Test>::get(USDX_ASSET_ID), 500 * PUSD_UNIT);
			assert_eq!(PsmDebt::<Test>::get(DAI_MOCK_ASSET_ID), 1500 * PUSD_UNIT);
			assert_eq!(Psm::total_psm_debt(), 2000 * PUSD_UNIT);

			// do_try_state asserts invariants; invoke manually.
			assert_ok!(Psm::do_try_state());
		});
	}

	#[test]
	fn mixed_decimal_mint_redeem_cycles_round_trip_to_zero_debt() {
		new_test_ext().execute_with(|| {
			register_external_asset_with_weight(USDX_ASSET_ID, Permill::from_percent(50));
			register_external_asset_with_weight(DAI_MOCK_ASSET_ID, Permill::from_percent(50));
			set_zero_fees(USDX_ASSET_ID);
			set_zero_fees(DAI_MOCK_ASSET_ID);

			// A mixed sequence: mint USDX, mint DAI, partially redeem each, mint
			// again, then drain both. After each step, try_state must hold. After
			// all steps, every asset's debt is back to zero.
			let steps: &[(u32, bool, u128)] = &[
				// (asset_id, is_mint, raw_amount)
				(USDX_ASSET_ID, true, 500 * USDX_UNIT), // mint 500 pUSD
				(DAI_MOCK_ASSET_ID, true, 1500 * DAI_UNIT), // mint 1500 pUSD
				(USDX_ASSET_ID, false, 200 * PUSD_UNIT), // redeem 200 pUSD via USDX
				(DAI_MOCK_ASSET_ID, false, 500 * PUSD_UNIT), // redeem 500 pUSD via DAI
				(USDX_ASSET_ID, true, 300 * USDX_UNIT), // mint another 300 pUSD
				(USDX_ASSET_ID, false, 600 * PUSD_UNIT), // drain USDX debt (500 - 200 + 300)
				(DAI_MOCK_ASSET_ID, false, 1000 * PUSD_UNIT), // drain DAI debt (1500 - 500)
			];

			for &(asset_id, is_mint, amount) in steps {
				if is_mint {
					assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), asset_id, amount));
				} else {
					assert_ok!(Psm::redeem(RuntimeOrigin::signed(ALICE), asset_id, amount));
				}
				// try_state must hold after every step.
				assert_ok!(Psm::do_try_state());
			}

			// After draining both, per-asset debt and aggregate are zero.
			assert_eq!(PsmDebt::<Test>::get(USDX_ASSET_ID), 0);
			assert_eq!(PsmDebt::<Test>::get(DAI_MOCK_ASSET_ID), 0);
			assert_eq!(Psm::total_psm_debt(), 0);

			// Reserves are also empty (zero fees, so no dust was charged).
			assert_eq!(get_asset_balance(USDX_ASSET_ID, psm_account()), 0);
			assert_eq!(get_asset_balance(DAI_MOCK_ASSET_ID, psm_account()), 0);
		});
	}

	#[test]
	fn try_state_holds_with_donated_mixed_decimal_reserve() {
		new_test_ext().execute_with(|| {
			register_external_asset_with_weight(DAI_MOCK_ASSET_ID, Permill::from_percent(100));
			set_zero_fees(DAI_MOCK_ASSET_ID);

			// Mint so the PSM has tracked debt + matching DAI reserve.
			assert_ok!(Psm::mint(RuntimeOrigin::signed(ALICE), DAI_MOCK_ASSET_ID, 1000 * DAI_UNIT));
			assert_eq!(PsmDebt::<Test>::get(DAI_MOCK_ASSET_ID), 1000 * PUSD_UNIT);

			// Donate extra DAI straight to the PSM account. Reserve now exceeds
			// pusd_to_external(debt). try_state check 2 uses the external-side
			// comparison and must still pass.
			let psm = psm_account();
			fund_external_asset(DAI_MOCK_ASSET_ID, psm, 7 * DAI_UNIT);
			assert_eq!(get_asset_balance(DAI_MOCK_ASSET_ID, psm), 1007 * DAI_UNIT);

			// Invariants hold under a donated scale-up reserve.
			assert_ok!(Psm::do_try_state());

			// A redeem that exhausts tracked debt drains only the debt-backed
			// share; the donated 7 DAI stays trapped in reserve, and try_state
			// continues to hold.
			assert_ok!(Psm::redeem(
				RuntimeOrigin::signed(ALICE),
				DAI_MOCK_ASSET_ID,
				1000 * PUSD_UNIT
			));
			assert_eq!(PsmDebt::<Test>::get(DAI_MOCK_ASSET_ID), 0);
			assert_eq!(get_asset_balance(DAI_MOCK_ASSET_ID, psm), 7 * DAI_UNIT);
			assert_ok!(Psm::do_try_state());
		});
	}
}
