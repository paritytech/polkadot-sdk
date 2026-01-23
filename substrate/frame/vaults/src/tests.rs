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

use crate::{
	mock::*, BadDebt, Error, Event, InitialCollateralizationRatio, LiquidationPenalty,
	MaxPositionAmount, MaximumIssuance, MinimumCollateralizationRatio, StabilityFee,
};
use frame_support::{assert_err, assert_noop, assert_ok, traits::Hooks};
use sp_runtime::{
	traits::{Bounded, Zero},
	FixedPointNumber, FixedU128, Permill, Saturating, TokenError,
};

// DOT has 10 decimals: 1 DOT = 10^10 smallest units
const DOT: u128 = 10_000_000_000; // 10^10

// pUSD has 6 decimals: 1 pUSD = 10^6 smallest units
const PUSD: u128 = 1_000_000; // 10^6

// Tolerance for interest calculations in tests.
// 6000 units = 0.006 pUSD - covers ~0.07% timestamp variance from jump_to_block.
const INTEREST_TOLERANCE: u128 = 6000;

// Helper to create FixedU128 ratios (e.g., 150% = 1.5)
fn ratio(percent: u32) -> FixedU128 {
	FixedU128::from_rational(percent as u128, 100)
}

// Helper to check if actual is within tolerance of expected
fn assert_approx_eq(actual: u128, expected: u128, tolerance: u128, context: &str) {
	assert!(
		actual >= expected.saturating_sub(tolerance) &&
			actual <= expected.saturating_add(tolerance),
		"{}: expected ~{}, got {} (tolerance: {}) (raw: {} vs {})",
		context,
		expected / PUSD,
		actual / PUSD,
		tolerance,
		actual,
		expected
	);
}

mod create_vault {
	use super::*;

	/// **Test: Opening a new Vault**
	///
	/// Verifies that a user can open a new vault by depositing collateral (DOT).
	/// - The collateral is locked in the system
	/// - Initial debt is zero (no stablecoin borrowed yet)
	/// - No interest has accrued yet
	#[test]
	fn works_with_initial_deposit() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			// Check vault exists
			let vault = crate::Vaults::<Test>::get(ALICE).expect("vault should exist");
			assert_eq!(vault.principal, 0);
			assert_eq!(vault.accrued_interest, 0);

			// Check collateral is held
			assert_eq!(vault.get_held_collateral(&ALICE), deposit);

			// Check events
			System::assert_has_event(Event::<Test>::VaultCreated { owner: ALICE }.into());
			System::assert_has_event(
				Event::<Test>::CollateralDeposited { owner: ALICE, amount: deposit }.into(),
			);
		});
	}

	/// **Test: One vault per user policy**
	///
	/// Ensures each user can only have ONE active vault at a time.
	/// This simplifies risk management and prevents users from fragmenting
	/// their collateral across multiple positions.
	#[test]
	fn fails_if_vault_already_exists() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			assert_noop!(
				Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit),
				Error::<Test>::VaultAlreadyExists
			);
		});
	}

	/// **Test: Cannot deposit more collateral than owned**
	///
	/// Prevents users from opening vaults with funds they don't have.
	/// This is a basic solvency check.
	#[test]
	fn fails_with_insufficient_balance() {
		new_test_ext().execute_with(|| {
			// Try to deposit more than ALICE has
			let excessive_deposit = INITIAL_BALANCE + 1;

			// The hold mechanism returns TokenError::FundsUnavailable when balance is insufficient
			assert_noop!(
				Vaults::create_vault(RuntimeOrigin::signed(ALICE), excessive_deposit),
				TokenError::FundsUnavailable
			);
		});
	}
}

mod deposit_collateral {
	use super::*;

	/// **Test: Adding more collateral to an existing vault**
	///
	/// Users can deposit additional collateral to improve their collateralization ratio.
	#[test]
	fn works() {
		new_test_ext().execute_with(|| {
			let initial = 100 * DOT;
			let additional = 50 * DOT;
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), initial));
			assert_ok!(Vaults::deposit_collateral(RuntimeOrigin::signed(ALICE), additional));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.get_held_collateral(&ALICE), initial + additional);

			System::assert_has_event(
				Event::<Test>::CollateralDeposited { owner: ALICE, amount: additional }.into(),
			);
		});
	}

	/// **Test: Cannot deposit to a non-existent vault**
	///
	/// Users must first create a vault before they can deposit collateral.
	#[test]
	fn fails_if_vault_not_found() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Vaults::deposit_collateral(RuntimeOrigin::signed(ALICE), 100 * DOT),
				Error::<Test>::VaultNotFound
			);
		});
	}
}

mod withdraw_collateral {
	use super::*;

	/// **Test: Withdrawing collateral from a debt-free vault**
	///
	/// When a vault has no outstanding debt, the user can freely withdraw
	/// any amount of their collateral (as long as remaining >= `MinimumDeposit`).
	#[test]
	fn works_without_debt() {
		new_test_ext().execute_with(|| {
			let initial = 200 * DOT;
			let withdraw = 50 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), initial));
			assert_ok!(Vaults::withdraw_collateral(RuntimeOrigin::signed(ALICE), withdraw));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.get_held_collateral(&ALICE), initial - withdraw);

			System::assert_has_event(
				Event::<Test>::CollateralWithdrawn { owner: ALICE, amount: withdraw }.into(),
			);
		});
	}

	/// **Test: Cannot withdraw more collateral than deposited**
	///
	/// Basic validation that prevents withdrawing non-existent funds.
	#[test]
	fn fails_if_insufficient_collateral() {
		new_test_ext().execute_with(|| {
			let initial = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), initial));

			assert_noop!(
				Vaults::withdraw_collateral(RuntimeOrigin::signed(ALICE), initial + 1),
				Error::<Test>::InsufficientCollateral
			);
		});
	}

	/// **Test: Collateralization ratio protection on withdrawal**
	///
	/// When a vault has debt, withdrawals are restricted to maintain the
	/// initial collateralization ratio (200%). This prevents users from
	/// removing collateral and leaving an undercollateralized position.
	///
	/// Example scenario with 300 DOT:
	/// - 300 DOT collateral at $4.21 = $1263 value
	/// - 300 pUSD debt (ratio = 421%)
	/// - To breach 200% ratio, need value < $600, i.e., < 142.5 DOT
	/// - Withdrawing 160 DOT leaves 140 DOT = $589 value, ratio = 196% < 200%
	#[test]
	fn fails_if_would_breach_initial_ratio_with_debt() {
		new_test_ext().execute_with(|| {
			// Oracle: 1 DOT = 4.21 USD
			// Deposit 300 DOT = 1263 USD collateral value
			let initial = 300 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), initial));

			// Mint 300 pUSD debt (valid at 200% ICR since $1263 > $600)
			// Current ratio = 1263/300 = 421%
			let mint_amount = 300 * PUSD;
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), mint_amount));

			// With 300 pUSD debt, need 300 * 2.0 = 600 USD = ~142.5 DOT minimum collateral
			// Try to withdraw 160 DOT (leaves 140 DOT = $589 value, ratio = 196% < 200%)
			// Note: 140 DOT > 100 DOT MinimumDeposit, so we hit ratio check first
			assert_noop!(
				Vaults::withdraw_collateral(RuntimeOrigin::signed(ALICE), 160 * DOT),
				Error::<Test>::UnsafeCollateralizationRatio
			);

			// Try to withdraw 158 DOT (leaves 142 DOT = $598 value, ratio = 199% < 200%)
			assert_noop!(
				Vaults::withdraw_collateral(RuntimeOrigin::signed(ALICE), 158 * DOT),
				Error::<Test>::UnsafeCollateralizationRatio
			);

			// Withdrawing 155 DOT should work (leaves 145 DOT = $610.5 value, ratio = 203% > 200%)
			// And 145 DOT > 100 DOT MinimumDeposit
			assert_ok!(Vaults::withdraw_collateral(RuntimeOrigin::signed(ALICE), 155 * DOT));
		});
	}

	/// **Test: Cannot create dust vaults via withdrawal**
	///
	/// Prevents users from withdrawing collateral such that the remaining
	/// amount is below `MinimumDeposit` (100 DOT). This prevents storage bloat
	/// from tiny "dust" vaults.
	///
	/// Withdrawing ALL collateral is allowed (when debt == 0) and auto-closes the vault.
	#[test]
	fn fails_if_would_create_dust_vault() {
		new_test_ext().execute_with(|| {
			let initial = 200 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), initial));

			// Try to withdraw 150 DOT, leaving only 50 DOT (below `MinimumDeposit` of 100 DOT)
			assert_noop!(
				Vaults::withdraw_collateral(RuntimeOrigin::signed(ALICE), 150 * DOT),
				Error::<Test>::BelowMinimumDeposit
			);

			// Withdrawing 100 DOT should work (leaves 100 DOT = `MinimumDeposit`)
			assert_ok!(Vaults::withdraw_collateral(RuntimeOrigin::signed(ALICE), 100 * DOT));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.get_held_collateral(&ALICE), 100 * DOT);
		});
	}

	/// **Test: Cannot withdraw all collateral when vault has debt**
	///
	/// Even if the collateralization ratio would be "infinite" (0 collateral / debt),
	/// we don't allow withdrawing all collateral when there's outstanding debt.
	/// Users must repay debt first, then close the vault.
	#[test]
	fn fails_if_withdrawing_all_with_debt() {
		new_test_ext().execute_with(|| {
			let initial = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), initial));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 100 * PUSD));

			// Try to withdraw all collateral while having debt
			assert_noop!(
				Vaults::withdraw_collateral(RuntimeOrigin::signed(ALICE), initial),
				Error::<Test>::VaultHasDebt // Cannot leave vault with 0 collateral and debt
			);
		});
	}

	/// **Test: Withdrawing all collateral closes the vault immediately**
	///
	/// When a user withdraws all their collateral (only possible with zero debt),
	/// the vault is immediately removed from storage.
	/// Both `CollateralWithdrawn` and `VaultClosed` events are emitted.
	#[test]
	fn auto_closes_vault_when_withdrawing_all() {
		new_test_ext().execute_with(|| {
			let initial = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), initial));
			assert!(crate::Vaults::<Test>::get(ALICE).is_some());

			// Withdraw all collateral (no debt, so this is allowed)
			assert_ok!(Vaults::withdraw_collateral(RuntimeOrigin::signed(ALICE), initial));

			// Vault should be immediately removed.
			assert!(crate::Vaults::<Test>::get(ALICE).is_none());

			// Both events should be emitted
			System::assert_has_event(
				Event::<Test>::CollateralWithdrawn { owner: ALICE, amount: initial }.into(),
			);
			System::assert_has_event(Event::<Test>::VaultClosed { owner: ALICE }.into());

			// User should be able to create a new vault immediately
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), initial));
		});
	}
}

mod mint {
	use super::*;

	/// **Test: Borrowing stablecoin (pUSD) against collateral**
	///
	/// This is the core vault operation - users lock collateral and mint stablecoin.
	/// The amount they can borrow depends on:
	/// - The value of their collateral (DOT price × amount)
	/// - The initial collateralization ratio (200%)
	///
	/// Example: 100 DOT at $4.21 = $421 → max borrow = $421 / 2.0 = 210.5 pUSD
	#[test]
	fn works() {
		new_test_ext().execute_with(|| {
			// Oracle: 1 DOT = 4.21 USD
			// Deposit 100 DOT = 421 USD collateral value
			let deposit = 100 * DOT;
			// At 200% initial ratio, max mint = 421 / 2.0 = 210.5 pUSD
			let mint_amount = 200 * PUSD; // Safe amount below max

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), mint_amount));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.principal, mint_amount);

			// Check pUSD was minted to ALICE
			assert_eq!(Assets::balance(STABLECOIN_ASSET_ID, ALICE), mint_amount);

			System::assert_has_event(
				Event::<Test>::Minted { owner: ALICE, amount: mint_amount }.into(),
			);
		});
	}

	/// **Test: Initial collateralization ratio enforcement (200%)**
	///
	/// When minting NEW debt, the vault must maintain 200% collateralization
	/// (not just the 180% minimum). This creates a safety buffer so users
	/// don't get liquidated immediately after borrowing.
	#[test]
	fn fails_if_exceeds_initial_ratio() {
		new_test_ext().execute_with(|| {
			// Oracle: 1 DOT = 4.21 USD
			// 100 DOT = 421 USD, max at 200% = 210.5 pUSD
			let deposit = 100 * DOT;
			let excessive_mint = 220 * PUSD; // Would result in ~191% ratio < 200%

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			assert_noop!(
				Vaults::mint(RuntimeOrigin::signed(ALICE), excessive_mint),
				Error::<Test>::UnsafeCollateralizationRatio
			);
		});
	}

	/// **Test: System-wide debt ceiling enforcement**
	///
	/// The protocol has a maximum total debt limit to manage systemic risk.
	/// Even if a user has sufficient collateral, they cannot mint if it would
	/// push total system debt above the ceiling.
	#[test]
	fn fails_if_exceeds_max_debt() {
		new_test_ext().execute_with(|| {
			// Set a low max debt
			MaximumIssuance::<Test>::put(100 * PUSD);

			// Deposit enough collateral for large mint (but not all balance due to existential
			// deposit)
			let deposit = 500 * DOT;
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			// Try to mint more than max debt
			// With 500 DOT at price 4.21 = 2105 USD value, we can mint up to ~1052 pUSD at 200% ICR
			// But max debt is only 100 pUSD
			assert_noop!(
				Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD),
				Error::<Test>::ExceedsMaxDebt
			);
		});
	}

	/// **Test: Cannot mint without an existing vault**
	///
	/// Users must first create a vault with collateral before borrowing.
	#[test]
	fn fails_if_vault_not_found() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Vaults::mint(RuntimeOrigin::signed(ALICE), 100 * PUSD),
				Error::<Test>::VaultNotFound
			);
		});
	}

	/// **Test: Oracle price required for minting**
	///
	/// The system needs a valid price feed to calculate collateralization ratios.
	/// If the oracle is unavailable, minting is blocked to prevent under-collateralized loans.
	#[test]
	fn fails_if_price_not_available() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			// Remove oracle price
			set_mock_price(None);

			assert_noop!(
				Vaults::mint(RuntimeOrigin::signed(ALICE), 100 * PUSD),
				Error::<Test>::PriceNotAvailable
			);
		});
	}
}

mod repay {
	use super::*;

	/// **Test: Partial debt repayment**
	///
	/// Users can repay any portion of their debt at any time.
	/// The repaid pUSD is burned (removed from circulation), reducing
	/// the vault's debt and improving its collateralization ratio.
	#[test]
	fn works_partial_repayment() {
		new_test_ext().execute_with(|| {
			// Oracle: 1 DOT = 4.21 USD
			// 100 DOT = 421 USD, max mint at 200% = 210.5 pUSD
			let deposit = 100 * DOT;
			let mint_amount = 200 * PUSD;
			let repay_amount = 80 * PUSD;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), mint_amount));
			assert_ok!(Vaults::repay(RuntimeOrigin::signed(ALICE), repay_amount));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.principal, mint_amount - repay_amount);

			// Check pUSD was burned
			assert_eq!(Assets::balance(STABLECOIN_ASSET_ID, ALICE), mint_amount - repay_amount);

			System::assert_has_event(
				Event::<Test>::Repaid { owner: ALICE, amount: repay_amount }.into(),
			);
		});
	}

	/// **Test: Overpayment is capped to actual debt**
	///
	/// If a user tries to repay more than they owe (but has sufficient balance),
	/// only the actual debt amount is burned. The excess pUSD is not consumed and a
	/// `ReturnedExcess` event is emitted.
	#[test]
	fn caps_repayment_to_debt() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;
			let mint_amount = 200 * PUSD;

			// ALICE creates vault and mints 200 pUSD
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), mint_amount));

			// BOB creates vault with enough collateral to mint 300 pUSD
			// At $4.21/DOT and 200% ICR: need 300 * 2 / 4.21 = ~143 DOT minimum
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(BOB), 150 * DOT));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(BOB), 300 * PUSD));
			assert_ok!(Assets::transfer(
				RuntimeOrigin::signed(BOB),
				STABLECOIN_ASSET_ID,
				ALICE,
				300 * PUSD
			));

			// ALICE now has 500 pUSD (200 minted + 300 from BOB) but only 200 pUSD debt
			assert_eq!(Assets::balance(STABLECOIN_ASSET_ID, ALICE), 500 * PUSD);

			// Try to repay 500 pUSD - should only repay actual debt (200 pUSD)
			// No interest has accrued (same block), so excess = 500 - 200 = 300 pUSD
			let repay_amount = 500 * PUSD;
			let expected_excess = repay_amount - mint_amount; // 300 pUSD
			assert_ok!(Vaults::repay(RuntimeOrigin::signed(ALICE), repay_amount));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.principal, 0);

			// Only 200 pUSD should be burned (the debt), leaving 300 pUSD
			assert_eq!(Assets::balance(STABLECOIN_ASSET_ID, ALICE), 300 * PUSD);

			// ReturnedExcess event should be emitted for the unused amount
			System::assert_has_event(
				Event::<Test>::ReturnedExcess { owner: ALICE, amount: expected_excess }.into(),
			);
		});
	}
}

mod liquidate {
	use super::*;

	/// **Test: Liquidation of undercollateralized vault**
	///
	/// When the collateral value drops below 180% of debt (due to price decline),
	/// anyone can trigger liquidation. This protects the protocol from bad debt.
	///
	/// Example scenario:
	/// - Initial: 100 DOT at $4.21 = $421, debt = 200 pUSD → ratio = 210%
	/// - After price drop to $3: 100 DOT = $300, debt = 200 pUSD → ratio = 150%
	/// - Since 150% < 180% minimum, liquidation is allowed
	#[test]
	fn works_when_undercollateralized() {
		new_test_ext().execute_with(|| {
			// Oracle: 1 DOT = 4.21 USD
			// 100 DOT = 421 USD, max mint at 200% = 210.5 pUSD
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			// Mint at safe ratio: 200 pUSD gives ratio = 421/200 = 210%
			let mint_amount = 200 * PUSD;
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), mint_amount));

			// Drop price to make vault undercollateralized
			// Current: 100 DOT * 4.21 USD = 421 USD, debt = 200 USD, ratio = 210%
			// New price: 100 DOT * 3.0 USD = 300 USD, debt = 200 USD, ratio = 150% < 180%
			set_mock_price(Some(FixedU128::from_u32(3)));

			// BOB liquidates ALICE's vault
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));

			// Vault should still exist but with InLiquidation status
			let vault =
				crate::Vaults::<Test>::get(ALICE).expect("Vault should exist during liquidation");
			assert_eq!(
				vault.status,
				crate::VaultStatus::InLiquidation,
				"Vault should be in InLiquidation status"
			);

			// Collateral should now be held with Seized reason (for auction)
			// Check that ALICE has funds on hold with Seized reason
			use frame_support::traits::fungible::InspectHold;
			let seized_balance =
				Balances::balance_on_hold(&crate::HoldReason::Seized.into(), &ALICE);
			assert_eq!(seized_balance, deposit, "All collateral should be seized");

			// Check InLiquidation event
			let events = System::events();
			let liquidated_event = events
				.iter()
				.find(|e| matches!(e.event, RuntimeEvent::Vaults(Event::InLiquidation { .. })));
			assert!(liquidated_event.is_some(), "Should emit InLiquidation event");
		});
	}

	/// **Test: Cannot liquidate a healthy vault**
	///
	/// Vaults with collateralization ratio ≥ 180% are considered "safe" and
	/// cannot be liquidated. This protects vault owners from unfair liquidation.
	#[test]
	fn fails_if_vault_is_safe() {
		new_test_ext().execute_with(|| {
			// Oracle: 1 DOT = 4.21 USD
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			// Mint conservatively: 200 pUSD gives ratio = 421/200 = 210% > 180%
			let mint_amount = 200 * PUSD;
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), mint_amount));

			assert_noop!(
				Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE),
				Error::<Test>::VaultIsSafe
			);
		});
	}

	/// **Test: Liquidating a debt-free vault must not panic**
	///
	/// A vault with zero debt is always safe and should return VaultIsSafe.
	#[test]
	fn zero_debt_vault_is_safe() {
		new_test_ext().execute_with(|| {
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));

			assert_noop!(
				Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE),
				Error::<Test>::VaultIsSafe
			);
		});
	}

	/// **Test: Cannot liquidate non-existent vault**
	///
	/// Basic validation that liquidation requires an actual vault to exist.
	#[test]
	fn fails_if_vault_not_found() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE),
				Error::<Test>::VaultNotFound
			);
		});
	}
}

mod close_vault {
	use super::*;

	/// **Test: Close vault and withdraw all collateral from a debt-free vault**
	///
	/// When a vault has no outstanding debt, the owner can close it and withdraw all
	/// collateral. The vault is immediately removed from storage.
	/// This is the normal "exit" flow for users who no longer need their Vault.
	#[test]
	fn works_with_no_debt() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			let alice_before = Balances::free_balance(ALICE);
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			assert_ok!(Vaults::close_vault(RuntimeOrigin::signed(ALICE)));

			// Vault should be removed immediately
			assert!(crate::Vaults::<Test>::get(ALICE).is_none());

			// Collateral should be returned - ALICE's balance restored to original
			// (alice_before - deposit during hold + deposit released = alice_before)
			assert_eq!(Balances::free_balance(ALICE), alice_before);

			System::assert_has_event(Event::<Test>::VaultClosed { owner: ALICE }.into());
		});
	}

	/// **Test: Cannot close vault with outstanding debt**
	///
	/// Vaults with unpaid debt cannot be closed. Users must first
	/// repay all borrowed pUSD before they can close the vault.
	/// This ensures the protocol's stablecoin remains fully backed.
	#[test]
	fn fails_with_outstanding_debt() {
		new_test_ext().execute_with(|| {
			// Oracle: 1 DOT = 4.21 USD
			// 100 DOT = 421 USD, max mint at 200% = 210.5 pUSD
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 100 * PUSD));

			assert_noop!(
				Vaults::close_vault(RuntimeOrigin::signed(ALICE)),
				Error::<Test>::VaultHasDebt
			);
		});
	}

	/// **Test: `close_vault` requires all debt (principal + interest) to be repaid**
	///
	/// Per spec, Debt == 0 means both principal AND accrued_interest must be zero.
	/// If only principal is zero but interest remains, closing should fail.
	#[test]
	fn close_vault_fails_with_accrued_interest() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;
			let mint_amount = 200 * PUSD;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), mint_amount));

			// Accrue interest over time.
			jump_to_block(5_256_000);

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			let interest = vault.accrued_interest;
			assert!(interest > 0, "Interest should be accrued");

			// Directly set principal=0 while keeping interest>0 to test defensive check.
			// This state cannot occur through normal extrinsics (repay pays interest first).
			crate::Vaults::<Test>::mutate(ALICE, |maybe_vault| {
				let vault = maybe_vault.as_mut().expect("vault should exist");
				vault.principal = 0;
			});

			// Cannot close - still has accrued interest
			assert_noop!(
				Vaults::close_vault(RuntimeOrigin::signed(ALICE)),
				Error::<Test>::VaultHasDebt
			);
		});
	}

	/// **Test: `close_vault` succeeds after repaying all debt including interest**
	///
	/// User must repay both principal and accrued interest before closing.
	/// With the mint-on-accrual model, interest is minted to InsuranceFund when
	/// fees accrue, and burned from the user when repaid.
	#[test]
	fn close_vault_succeeds_after_full_debt_repayment() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;
			let mint_amount = 200 * PUSD;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), mint_amount));

			// Check IF balance before interest accrues
			let insurance_before_accrual = Assets::balance(STABLECOIN_ASSET_ID, INSURANCE_FUND);

			// Accrue interest over time.
			jump_to_block(5_256_000);
			assert_ok!(Vaults::poke(RuntimeOrigin::signed(BOB), ALICE));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			let total_debt = vault.principal + vault.accrued_interest;
			assert!(vault.accrued_interest > 0, "Interest should be accrued");

			// Verify IF received interest during accrual (mint-on-accrual model)
			let insurance_after_accrual = Assets::balance(STABLECOIN_ASSET_ID, INSURANCE_FUND);
			assert_eq!(
				insurance_after_accrual,
				insurance_before_accrual + vault.accrued_interest,
				"`InsuranceFund` should receive interest on accrual"
			);

			// Alice only has the minted pUSD, need extra to cover interest
			// Mint extra pUSD directly to Alice for interest payment
			assert_ok!(Assets::mint(
				RuntimeOrigin::signed(ALICE),
				STABLECOIN_ASSET_ID,
				ALICE,
				vault.accrued_interest
			));

			// Repay all debt (principal + interest)
			assert_ok!(Vaults::repay(RuntimeOrigin::signed(ALICE), total_debt));

			// IF balance should not change on repay (interest was minted on accrual)
			let insurance_after_repay = Assets::balance(STABLECOIN_ASSET_ID, INSURANCE_FUND);
			assert_eq!(
				insurance_after_repay, insurance_after_accrual,
				"`InsuranceFund` balance unchanged on repay (already received on accrual)"
			);

			// Now can close the vault
			assert_ok!(Vaults::close_vault(RuntimeOrigin::signed(ALICE)));
			assert!(crate::Vaults::<Test>::get(ALICE).is_none());
		});
	}
}

mod parameter_setters {
	use super::*;

	/// **Test: `ManagerOrigin` can update minimum collateralization ratio**
	///
	/// The `ManagerOrigin` with `Full` privilege can adjust the minimum ratio (default 180%)
	/// that determines when vaults become liquidatable. Lowering this makes the
	/// protocol more capital-efficient but riskier; raising it makes it safer
	/// but requires more collateral per borrowed pUSD.
	#[test]
	fn set_minimum_collateralization_ratio_works() {
		new_test_ext().execute_with(|| {
			let old_ratio = ratio(180); // Genesis default
			let new_ratio = ratio(190);

			assert_ok!(Vaults::set_minimum_collateralization_ratio(
				RuntimeOrigin::root(),
				new_ratio
			));

			assert_eq!(MinimumCollateralizationRatio::<Test>::get(), new_ratio);
			System::assert_has_event(
				Event::<Test>::MinimumCollateralizationRatioUpdated {
					old_value: old_ratio,
					new_value: new_ratio,
				}
				.into(),
			);
		});
	}

	/// **Test: Only `ManagerOrigin` can change minimum ratio**
	///
	/// Regular users cannot change protocol parameters. This ensures
	/// critical risk parameters are controlled by `ManagerOrigin` only.
	#[test]
	fn set_minimum_collateralization_ratio_fails_for_non_manager() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Vaults::set_minimum_collateralization_ratio(
					RuntimeOrigin::signed(ALICE),
					ratio(140)
				),
				frame_support::error::BadOrigin
			);
		});
	}

	/// **Test: Governance can update initial collateralization ratio**
	///
	/// The initial ratio (default 200%) determines the maximum amount
	/// users can borrow when minting. It's higher than the minimum ratio
	/// to provide a safety buffer against immediate liquidation.
	#[test]
	fn set_initial_collateralization_ratio_works() {
		new_test_ext().execute_with(|| {
			let old_ratio = ratio(200); // Genesis default
			let new_ratio = ratio(210);

			assert_ok!(Vaults::set_initial_collateralization_ratio(
				RuntimeOrigin::root(),
				new_ratio
			));

			assert_eq!(InitialCollateralizationRatio::<Test>::get(), new_ratio);
			System::assert_has_event(
				Event::<Test>::InitialCollateralizationRatioUpdated {
					old_value: old_ratio,
					new_value: new_ratio,
				}
				.into(),
			);
		});
	}

	/// **Test: Governance can update stability fee**
	///
	/// The stability fee (default 4% annually) is the interest rate charged
	/// on borrowed pUSD. It's denominated in pUSD but paid from collateral (DOT).
	/// Revenue goes to the protocol treasury.
	#[test]
	fn set_stability_fee_works() {
		new_test_ext().execute_with(|| {
			let old_fee = Permill::from_percent(4); // Genesis default
			let new_fee = Permill::from_percent(10);

			assert_ok!(Vaults::set_stability_fee(RuntimeOrigin::root(), new_fee));

			assert_eq!(StabilityFee::<Test>::get(), new_fee);
			System::assert_has_event(
				Event::<Test>::StabilityFeeUpdated { old_value: old_fee, new_value: new_fee }
					.into(),
			);
		});
	}

	/// **Test: Governance can update liquidation penalty**
	///
	/// The liquidation penalty (default 13%) is charged when a vault is
	/// liquidated due to undercollateralization. This fee goes to the keeper
	/// who initiates the liquidation, incentivizing timely vault liquidations.
	#[test]
	fn set_liquidation_penalty_works() {
		new_test_ext().execute_with(|| {
			let old_penalty = Permill::from_percent(13); // Genesis default
			let new_penalty = Permill::from_percent(15);

			assert_ok!(Vaults::set_liquidation_penalty(RuntimeOrigin::root(), new_penalty));

			assert_eq!(LiquidationPenalty::<Test>::get(), new_penalty);
			System::assert_has_event(
				Event::<Test>::LiquidationPenaltyUpdated {
					old_value: old_penalty,
					new_value: new_penalty,
				}
				.into(),
			);
		});
	}
}

mod authorization_levels {
	use super::*;
	use crate::{mock::EMERGENCY_ADMIN, MaximumIssuance};

	/// **Test: `Emergency` privilege cannot set minimum collateralization ratio**
	#[test]
	fn emergency_privilege_cannot_set_minimum_collateralization_ratio() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Vaults::set_minimum_collateralization_ratio(
					RuntimeOrigin::signed(EMERGENCY_ADMIN),
					ratio(140)
				),
				crate::Error::<Test>::InsufficientPrivilege
			);
		});
	}

	/// **Test: `Emergency` privilege cannot set initial collateralization ratio**
	#[test]
	fn emergency_privilege_cannot_set_initial_collateralization_ratio() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Vaults::set_initial_collateralization_ratio(
					RuntimeOrigin::signed(EMERGENCY_ADMIN),
					ratio(160)
				),
				crate::Error::<Test>::InsufficientPrivilege
			);
		});
	}

	/// **Test: `Emergency` privilege cannot set stability fee**
	#[test]
	fn emergency_privilege_cannot_set_stability_fee() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Vaults::set_stability_fee(
					RuntimeOrigin::signed(EMERGENCY_ADMIN),
					Permill::from_percent(10)
				),
				crate::Error::<Test>::InsufficientPrivilege
			);
		});
	}

	/// **Test: `Emergency` privilege cannot set liquidation penalty**
	#[test]
	fn emergency_privilege_cannot_set_liquidation_penalty() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Vaults::set_liquidation_penalty(
					RuntimeOrigin::signed(EMERGENCY_ADMIN),
					Permill::from_percent(15)
				),
				crate::Error::<Test>::InsufficientPrivilege
			);
		});
	}

	/// **Test: `Emergency` privilege cannot set max liquidation amount**
	#[test]
	fn emergency_privilege_cannot_set_max_liquidation_amount() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Vaults::set_max_liquidation_amount(
					RuntimeOrigin::signed(EMERGENCY_ADMIN),
					500_000 * PUSD
				),
				crate::Error::<Test>::InsufficientPrivilege
			);
		});
	}

	/// **Test: `Full` privilege (`ManagerOrigin`) can raise maximum debt**
	#[test]
	fn full_privilege_can_raise_maximum_debt() {
		new_test_ext().execute_with(|| {
			let current = MaximumIssuance::<Test>::get();
			let new_debt = current + 1_000_000 * PUSD;
			assert_ok!(Vaults::set_max_issuance(RuntimeOrigin::root(), new_debt));
			assert_eq!(MaximumIssuance::<Test>::get(), new_debt);
		});
	}

	/// **Test: `Full` privilege (`ManagerOrigin`) can lower maximum debt**
	#[test]
	fn full_privilege_can_lower_maximum_debt() {
		new_test_ext().execute_with(|| {
			let current = MaximumIssuance::<Test>::get();
			let new_debt = current / 2;
			assert_ok!(Vaults::set_max_issuance(RuntimeOrigin::root(), new_debt));
			assert_eq!(MaximumIssuance::<Test>::get(), new_debt);
		});
	}

	/// **Test: `Emergency` privilege can lower maximum debt**
	///
	/// This is the key emergency action - allowing fast-track lowering of the
	/// debt ceiling in response to oracle attacks or other emergencies.
	#[test]
	fn emergency_privilege_can_lower_maximum_debt() {
		new_test_ext().execute_with(|| {
			let current = MaximumIssuance::<Test>::get();
			let new_debt = current / 2;
			assert_ok!(Vaults::set_max_issuance(RuntimeOrigin::signed(EMERGENCY_ADMIN), new_debt));
			assert_eq!(MaximumIssuance::<Test>::get(), new_debt);
		});
	}

	/// **Test: `Emergency` privilege cannot raise maximum debt**
	///
	/// `Emergency` actions are defensive only - they cannot increase risk exposure.
	#[test]
	fn emergency_privilege_cannot_raise_maximum_debt() {
		new_test_ext().execute_with(|| {
			let current = MaximumIssuance::<Test>::get();
			let new_debt = current + 1_000_000 * PUSD;
			assert_noop!(
				Vaults::set_max_issuance(RuntimeOrigin::signed(EMERGENCY_ADMIN), new_debt),
				crate::Error::<Test>::CanOnlyLowerMaxDebt
			);
		});
	}

	/// **Test: `Emergency` privilege can set maximum debt to zero**
	///
	/// In an extreme emergency, the debt ceiling can be set to zero to
	/// completely halt new minting.
	#[test]
	fn emergency_privilege_can_set_max_issuance_to_zero() {
		new_test_ext().execute_with(|| {
			assert_ok!(Vaults::set_max_issuance(RuntimeOrigin::signed(EMERGENCY_ADMIN), 0));
			assert_eq!(MaximumIssuance::<Test>::get(), 0);
		});
	}

	/// **Test: Regular signed origin has no privilege**
	///
	/// Neither `Full` nor `Emergency` - should fail with `BadOrigin`.
	#[test]
	fn regular_signed_origin_has_no_privilege() {
		new_test_ext().execute_with(|| {
			// ALICE is a regular user, not a privileged origin
			assert_noop!(
				Vaults::set_minimum_collateralization_ratio(
					RuntimeOrigin::signed(ALICE),
					ratio(140)
				),
				frame_support::error::BadOrigin
			);

			assert_noop!(
				Vaults::set_max_issuance(RuntimeOrigin::signed(ALICE), 0),
				frame_support::error::BadOrigin
			);
		});
	}
}

mod fee_accrual {
	use super::*;

	/// **Test: Stability fee accrues over time**
	///
	/// Interest on debt is calculated continuously based on time elapsed.
	/// Accrued interest (in pUSD) is tracked in the vault's `accrued_interest` field.
	///
	/// Example: 200 pUSD debt × 4% annual × 1 year = 8 pUSD interest
	#[test]
	fn interest_accrues_over_time() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;
			let mint_amount = 200 * PUSD;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), mint_amount));

			// Check initial state
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.accrued_interest, 0);
			let initial_held = vault.get_held_collateral(&ALICE);

			// Advance 1 year worth of blocks (5,256,000 blocks at 6s each).
			jump_to_block(5_256_000);

			// With 4% annual fee on 200 pUSD = 8 pUSD interest
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			let expected_interest = 8 * PUSD;
			assert_approx_eq(
				vault.accrued_interest,
				expected_interest,
				INTEREST_TOLERANCE,
				"Interest after 1 year",
			);

			// Collateral should NOT be reduced (interest is not collected yet)
			let final_held = vault.get_held_collateral(&ALICE);
			assert_eq!(
				final_held, initial_held,
				"Collateral should not be reduced until close_vault"
			);

			// `InterestAccrued` event should be emitted
			let events = System::events();
			let has_interest = events
				.iter()
				.any(|e| matches!(e.event, RuntimeEvent::Vaults(Event::InterestAccrued { .. })));
			assert!(has_interest, "Should emit `InterestAccrued` event");
		});
	}

	/// **Test: Stability fee calculation example**
	///
	/// This test validates the complete interest calculation using a concrete example:
	///
	/// **Setup:**
	/// - User deposits 10,000 DOT as collateral
	/// - Borrows 10,000 pUSD (at $2/DOT, this is 200% collateralized)
	/// - Stability fee: 4% annually
	///
	/// **After 1 year:**
	/// - Interest accrued: 10,000 pUSD × 4% = 400 pUSD
	/// - Collateral remains unchanged at 10,000 DOT
	/// - New collateralization ratio: (10,000 × $2) / (10,000 + 400) = 192%
	#[test]
	fn example_stability_fee_calculation() {
		new_test_ext().execute_with(|| {
			use frame_support::traits::fungible::Mutate as FungibleMutate;

			// Set price to $2/DOT (2 pUSD per DOT)
			set_mock_price(Some(FixedU128::from_u32(2)));

			// Setup: 10k DOT collateral, 10k pUSD debt
			let collateral = 10_000 * DOT;
			let debt = 10_000 * PUSD;

			// ALICE starts with INITIAL_BALANCE (1000 DOT) defined at genesis.
			// She needs 10,000 DOT for collateral + 1 extra unit to remain after the hold.
			// Additional needed: collateral + buffer - INITIAL_BALANCE = 9000 DOT + 1 unit.
			let additional_needed = collateral + 1 - INITIAL_BALANCE;
			let _ = Balances::mint_into(&ALICE, additional_needed);

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), collateral));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), debt));

			// Verify initial state
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(
				vault.get_held_collateral(&ALICE),
				collateral,
				"Initial collateral: 10k DOT"
			);
			assert_eq!(vault.principal, debt, "Initial principal: 10k pUSD");

			// Verify initial collateralization ratio is 200%
			// CR = (collateral × price) / (debt + interest) = (10,000 × 2) / 10,000 = 200%
			let initial_ratio = Vaults::get_collateralization_ratio(&vault, &ALICE).unwrap();
			assert_eq!(initial_ratio, ratio(200), "Initial ratio should be 200%");

			// Advance exactly 1 year (5,256,000 blocks at 6s each).
			jump_to_block(5_256_000);

			// Calculate expected values:
			// Interest in pUSD = 4% * 10,000 pUSD = 400 pUSD
			let expected_interest = 400 * PUSD;

			// Verify interest was accrued
			// Use larger tolerance (0.5 pUSD) for this test due to ~0.07% timestamp variance
			// over 1 year with jump_to_block (273k units variance on 400 pUSD expected)
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_approx_eq(
				vault.accrued_interest,
				expected_interest,
				PUSD / 2,
				"Interest after 1 year",
			);

			// Verify collateral is unchanged
			assert_eq!(
				vault.get_held_collateral(&ALICE),
				collateral,
				"Collateral should remain 10k DOT"
			);

			// Verify new collateralization ratio is ~192.31%
			// Collateral value: 10,000 DOT × $2 = $20,000
			// Total debt: 10,000 pUSD principal + 400 pUSD interest = 10,400 pUSD
			// CR = 20,000 / 10,400 = ≈ 1.9231 = 192.31%
			let final_ratio = Vaults::get_collateralization_ratio(&vault, &ALICE).unwrap();
			let expected_ratio = FixedU128::from_rational(20000, 10400);
			// Allow 0.01% tolerance for interest rounding
			let tolerance = expected_ratio / FixedU128::from_u32(10_000);
			assert!(
				final_ratio >= expected_ratio.saturating_sub(tolerance) &&
					final_ratio <= expected_ratio.saturating_add(tolerance),
				"Final ratio should be ~192.31%, got: {:?}, expected: {:?}",
				final_ratio,
				expected_ratio
			);

			// Verify debt unchanged
			assert_eq!(vault.principal, debt, "Principal should remain 10k pUSD");
		});
	}
}

mod edge_cases {
	use super::*;

	/// **Test: Collateralization ratio is max without debt**
	///
	/// When a vault has no debt, its collateralization ratio returns
	/// `FixedU128::max_value()` representing infinite CR (healthy).
	#[test]
	fn collateralization_ratio_max_without_debt() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			let ratio = crate::Pallet::<Test>::get_collateralization_ratio(&vault, &ALICE).unwrap();

			// Without debt, ratio is max value (infinite CR = healthy)
			assert_eq!(ratio, FixedU128::max_value());
		});
	}

	/// **Test: `MinimumDeposit` requirement**
	///
	/// The protocol requires a minimum deposit (100 DOT) to create a vault.
	/// This prevents spam/dust vaults and ensures each vault has meaningful
	/// economic value to make liquidation worthwhile.
	#[test]
	fn fails_below_minimum_deposit() {
		new_test_ext().execute_with(|| {
			// `MinimumDeposit` is 100 DOT, try with 99 DOT
			let below_minimum = 99 * DOT;

			assert_noop!(
				Vaults::create_vault(RuntimeOrigin::signed(ALICE), below_minimum),
				Error::<Test>::BelowMinimumDeposit
			);

			// Zero deposit should also fail
			assert_noop!(
				Vaults::create_vault(RuntimeOrigin::signed(ALICE), 0),
				Error::<Test>::BelowMinimumDeposit
			);

			// Exactly minimum should work
			let minimum = 100 * DOT;
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), minimum));
		});
	}
}

mod liquidation_edge_cases {
	use super::*;
	use crate::CollateralManager;

	/// **Test: Liquidation accrues interest and applies penalty**
	///
	/// When liquidating a vault with accrued interest, the protocol
	/// calculates both the stability fees and liquidation penalty.
	/// Interest is accrued via `on_idle` (stale vault processing) or during liquidation,
	/// and penalty is included in the auction tab.
	#[test]
	fn liquidation_with_accrued_interest() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;
			let mint_amount = 200 * PUSD;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), mint_amount));

			// Advance time to accrue interest.
			jump_to_block(2_628_000); // ~6 months

			// Drop price to trigger liquidation (below 180% minimum)
			set_mock_price(Some(FixedU128::from_u32(3)));

			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));

			// Vault should be in liquidation status
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.status, crate::VaultStatus::InLiquidation);

			// Should have both events:
			// - InterestAccrued from on_idle during jump_to_block
			// - LiquidationPenaltyAdded from liquidate_vault
			let events = System::events();
			let has_interest = events
				.iter()
				.any(|e| matches!(e.event, RuntimeEvent::Vaults(Event::InterestAccrued { .. })));
			let has_penalty = events.iter().any(|e| {
				matches!(e.event, RuntimeEvent::Vaults(Event::LiquidationPenaltyAdded { .. }))
			});

			assert!(has_interest, "Should emit `InterestAccrued` event");
			assert!(has_penalty, "Should emit `LiquidationPenaltyAdded` event");
		});
	}

	/// **Test: Liquidation boundary at exactly 180% ratio**
	///
	/// Vaults at EXACTLY the minimum ratio (180%) are NOT liquidatable.
	/// Liquidation only triggers when the ratio drops BELOW 180%.
	/// This test verifies the boundary condition precisely.
	#[test]
	fn liquidation_at_exact_boundary() {
		new_test_ext().execute_with(|| {
			// Set up vault that will be exactly at 180% after price drop
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			// Mint 200 pUSD (safe at 200% ICR)
			let mint_amount = 200 * PUSD;
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), mint_amount));

			// Set price so ratio is exactly 180%
			// Need: 100 DOT * price = 200 * 1.8 = 360 USD
			// price = 3.60 USD/DOT
			set_mock_price(Some(FixedU128::from_rational(360, 100)));

			// At exactly 180%, vault should NOT be liquidatable (ratio >= min_ratio)
			assert_noop!(
				Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE),
				Error::<Test>::VaultIsSafe
			);

			// Drop price slightly below boundary
			set_mock_price(Some(FixedU128::from_rational(359, 100)));

			// Now liquidation should work
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));
		});
	}

	/// **Test: Liquidation in severely underwater scenario**
	///
	/// All collateral is always seized for auction during liquidation.
	/// In underwater scenarios, the auction cannot cover the full principal.
	///
	/// Only unpaid PRINCIPAL becomes bad debt (unbacked stablecoin in circulation).
	/// Interest and penalty are simply not collected if there's insufficient collateral.
	///
	/// Example: At $0.50/DOT, 100 DOT = $50 value, principal = 200 pUSD.
	/// Auction raises 50 pUSD, which pays down principal first.
	/// Remaining principal = 200 - 50 = 150 pUSD becomes bad debt.
	#[test]
	fn liquidation_in_underwater_scenario() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			// Mint 200 pUSD (at 200% ratio with initial price $4.21/DOT)
			let mint_amount = 200 * PUSD;
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), mint_amount));

			// Crash price severely - collateral value drops dramatically
			// At $0.50/DOT: 100 DOT = $50 value, principal = 200 pUSD
			// Ratio = 50/200 = 25% (way under 180% minimum)
			let crash_price = FixedU128::from_rational(50, 100); // $0.50/DOT
			set_mock_price(Some(crash_price));

			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));

			// Vault should be in liquidation with all collateral seized
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.status, crate::VaultStatus::InLiquidation);

			// Simulate auction completion:
			// - Collateral value in pUSD = raw_price × deposit × (PUSD/DOT) for decimal conversion
			// - Payment priority: principal first (per Tab::apply_payment)
			// - Only remaining PRINCIPAL is bad debt (interest/penalty not collected, not bad debt)
			let collateral_value_pusd = crash_price.saturating_mul_int(deposit) * PUSD / DOT;
			let remaining_principal = mint_amount.saturating_sub(collateral_value_pusd);
			let bad_debt_before = crate::BadDebt::<Test>::get();

			assert_ok!(Vaults::complete_auction(&ALICE, 0, remaining_principal, &BOB, 0));

			// Bad debt should equal remaining principal only
			assert_eq!(
				crate::BadDebt::<Test>::get(),
				bad_debt_before + remaining_principal,
				"Only unpaid principal becomes bad debt"
			);

			// Vault should be removed after auction
			assert!(crate::Vaults::<Test>::get(ALICE).is_none());

			// AuctionShortfall event should be emitted
			System::assert_has_event(
				Event::<Test>::AuctionShortfall { shortfall: remaining_principal }.into(),
			);
			System::assert_has_event(
				Event::<Test>::BadDebtAccrued { owner: ALICE, amount: remaining_principal }.into(),
			);
		});
	}
}

mod interest_edge_cases {
	use super::*;

	/// **Test: No interest without debt**
	///
	/// Stability fees only apply to borrowed amounts. A vault with only
	/// collateral (no pUSD minted) accrues zero interest regardless of time.
	#[test]
	fn no_interest_accrues_without_debt() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			// Don't mint anything - no debt

			// Advance significant time
			jump_to_block(5_256_000); // 1 year

			// Trigger fee update
			assert_ok!(Vaults::deposit_collateral(RuntimeOrigin::signed(ALICE), 0));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.accrued_interest, 0, "No interest without debt");
			assert_eq!(vault.get_held_collateral(&ALICE), deposit, "Full collateral available");
		});
	}

	/// **Test: No interest accrues within the same block**
	///
	/// Interest is calculated based on block time elapsed. Multiple operations
	/// in the same block don't accrue additional interest because no time
	/// has passed between them.
	#[test]
	fn no_interest_in_same_block() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;
			let mint_amount = 200 * PUSD;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), mint_amount));

			// Multiple operations in same block should not accrue interest
			assert_ok!(Vaults::deposit_collateral(RuntimeOrigin::signed(ALICE), DOT));
			assert_ok!(Vaults::deposit_collateral(RuntimeOrigin::signed(ALICE), DOT));
			assert_ok!(Vaults::deposit_collateral(RuntimeOrigin::signed(ALICE), DOT));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.accrued_interest, 0, "No interest in same block");
		});
	}
}

mod boundary_conditions {
	use super::*;

	/// **Test: Maximum borrowing at exactly 200% ratio boundary**
	///
	/// Users can borrow up to the exact point where their collateralization
	/// ratio equals 200% (initial ratio). Attempting to borrow even 1 more
	/// pUSD unit will fail.
	#[test]
	fn mint_at_exact_initial_ratio() {
		new_test_ext().execute_with(|| {
			// Oracle: 1 DOT = 4.21 USD
			// 100 DOT = 421 USD
			// At exactly 200%: max_mint = 421 / 2.0 = 210.5 pUSD
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			// Calculate exact max mint
			// collateral_value = 100 DOT * 4.21 USD = 421 USD
			// 421 USD / 2.0 = 210.5 pUSD
			// Try 210 pUSD - should work
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 210 * PUSD));

			// Try to mint 5 more pUSD - should fail (would be below 200%)
			assert_noop!(
				Vaults::mint(RuntimeOrigin::signed(ALICE), 5 * PUSD),
				Error::<Test>::UnsafeCollateralizationRatio
			);
		});
	}

	/// **Test: Dust amount minting is rejected**
	///
	/// The system rejects mints below the `MinimumMint` threshold (5 pUSD).
	#[test]
	fn dust_amounts_handled_correctly() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			// Mint a very small amount (1 smallest unit of pUSD) - should fail
			assert_noop!(
				Vaults::mint(RuntimeOrigin::signed(ALICE), 1),
				Error::<Test>::BelowMinimumMint
			);

			// Mint just below minimum (4.999999 pUSD) - should fail
			assert_noop!(
				Vaults::mint(RuntimeOrigin::signed(ALICE), 5 * PUSD - 1),
				Error::<Test>::BelowMinimumMint
			);

			// Mint exactly minimum (5 pUSD) - should succeed
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 5 * PUSD));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.principal, 5 * PUSD);

			// Repay works for any amount
			assert_ok!(Vaults::repay(RuntimeOrigin::signed(ALICE), 5 * PUSD));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.principal, 0);
		});
	}

	/// **Test: Zero deposit still triggers fee update**
	///
	/// Depositing 0 collateral is a valid operation that triggers fee
	/// calculation and interest collection. This allows users to
	/// voluntarily settle their accrued interest without changing position.
	#[test]
	fn zero_deposit_collateral_triggers_fee_update() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;
			let mint_amount = 200 * PUSD;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), mint_amount));

			let vault_before = crate::Vaults::<Test>::get(ALICE).unwrap();
			let last_update_before = vault_before.last_fee_update;

			// Advance time
			run_to_block(1000);

			// Deposit 0 - should still trigger fee update
			assert_ok!(Vaults::deposit_collateral(RuntimeOrigin::signed(ALICE), 0));

			let vault_after = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert!(
				vault_after.last_fee_update > last_update_before,
				"Fee update timestamp should advance"
			);
		});
	}
}

mod poke {
	use super::*;

	/// **Test: Poke fails for non-existent vault**
	///
	/// Cannot poke a vault that doesn't exist.
	#[test]
	fn poke_fails_for_nonexistent_vault() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Vaults::poke(RuntimeOrigin::signed(BOB), ALICE),
				Error::<Test>::VaultNotFound
			);
		});
	}

	/// **Test: Poke updates vault fee timestamp and accrues interest**
	///
	/// When a vault has debt, poking it can trigger fee calculations.
	/// After 1 year, the vault becomes stale and `on_idle` processes it.
	/// This test verifies that poke works correctly in this scenario.
	#[test]
	fn poke_accrues_interest_and_emits_event() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;
			let mint_amount = 200 * PUSD;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), mint_amount));

			let initial_collateral =
				crate::Vaults::<Test>::get(ALICE).unwrap().get_held_collateral(&ALICE);

			// Advance 1 year.
			jump_to_block(5_256_000);

			// Verify interest was accrued
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert!(vault.accrued_interest > 0, "Interest should be accrued");

			// With 4% annual fee on 200 pUSD = 8 pUSD
			let expected = 8 * PUSD;
			assert_approx_eq(
				vault.accrued_interest,
				expected,
				INTEREST_TOLERANCE,
				"Interest after 1 year",
			);

			// Poke is still callable (even though vault was just updated)
			assert_ok!(Vaults::poke(RuntimeOrigin::signed(CHARLIE), ALICE));

			// Collateral should NOT be reduced (interest not collected yet)
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			let final_collateral = vault.get_held_collateral(&ALICE);
			assert_eq!(
				final_collateral, initial_collateral,
				"Collateral should not be reduced by poke"
			);

			// `InterestAccrued` event should be emitted
			let events = System::events();
			let has_interest_event = events
				.iter()
				.any(|e| matches!(e.event, RuntimeEvent::Vaults(Event::InterestAccrued { .. })));
			assert!(has_interest_event, "Should emit `InterestAccrued` event");
		});
	}

	/// **Test: Vault owner can poke their own vault**
	///
	/// The vault owner can also call poke on their own vault.
	#[test]
	fn owner_can_poke_own_vault() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			// Record timestamp before advancing time
			let vault_before = crate::Vaults::<Test>::get(ALICE).unwrap();
			let timestamp_before = vault_before.last_fee_update;

			run_to_block(1000);

			// Owner pokes own vault - should succeed
			assert_ok!(Vaults::poke(RuntimeOrigin::signed(ALICE), ALICE));

			// Vault's last_fee_update should be updated to current timestamp
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			let current_timestamp = MockTimestamp::get();
			assert_eq!(
				vault.last_fee_update, current_timestamp,
				"last_fee_update should be updated to current timestamp"
			);
			assert!(
				vault.last_fee_update > timestamp_before,
				"last_fee_update should have advanced from initial value"
			);
		});
	}
}

mod heal_permissionless {
	use super::*;
	use frame_support::traits::fungibles::Mutate as FungiblesMutate;

	/// **Test: Anyone can trigger bad debt repayment from `InsuranceFund`**
	///
	/// Bad debt repayment burns pUSD from the `InsuranceFund` account.
	/// Any user can call `heal` to trigger this.
	#[test]
	fn anyone_can_heal() {
		new_test_ext().execute_with(|| {
			// Setup: Create some bad debt
			BadDebt::<Test>::put(100 * PUSD);

			// Give `InsuranceFund` pUSD to burn
			assert_ok!(Assets::mint_into(STABLECOIN_ASSET_ID, &INSURANCE_FUND, 200 * PUSD));

			let insurance_before = Assets::balance(STABLECOIN_ASSET_ID, INSURANCE_FUND);

			// ALICE calls heal to repay bad debt from `InsuranceFund`
			assert_ok!(Vaults::heal(RuntimeOrigin::signed(ALICE), 50 * PUSD));

			// Bad debt should be reduced
			assert_eq!(BadDebt::<Test>::get(), 50 * PUSD);

			// `InsuranceFund`'s pUSD should be burned
			assert_eq!(
				Assets::balance(STABLECOIN_ASSET_ID, INSURANCE_FUND),
				insurance_before - 50 * PUSD
			);

			// Event should show the amount repaid
			System::assert_has_event(Event::<Test>::BadDebtRepaid { amount: 50 * PUSD }.into());
		});
	}

	/// **Test: Multiple `heal` calls burn from `InsuranceFund`**
	///
	/// Different users can call `heal` multiple times, each burning from `InsuranceFund`.
	#[test]
	fn multiple_users_can_heal() {
		new_test_ext().execute_with(|| {
			// Setup: Create bad debt
			BadDebt::<Test>::put(300 * PUSD);

			// Give `InsuranceFund` pUSD to burn
			assert_ok!(Assets::mint_into(STABLECOIN_ASSET_ID, &INSURANCE_FUND, 300 * PUSD));

			// Each user calls heal
			assert_ok!(Vaults::heal(RuntimeOrigin::signed(ALICE), 100 * PUSD));
			assert_eq!(BadDebt::<Test>::get(), 200 * PUSD);

			assert_ok!(Vaults::heal(RuntimeOrigin::signed(BOB), 100 * PUSD));
			assert_eq!(BadDebt::<Test>::get(), 100 * PUSD);

			assert_ok!(Vaults::heal(RuntimeOrigin::signed(CHARLIE), 100 * PUSD));
			assert_eq!(BadDebt::<Test>::get(), 0);

			// All `InsuranceFund` pUSD should be burned
			assert_eq!(Assets::balance(STABLECOIN_ASSET_ID, INSURANCE_FUND), 0);
		});
	}

	/// **Test: Repayment capped to actual bad debt**
	///
	/// If a user tries to repay more than the current bad debt,
	/// only the actual bad debt amount is burned from `InsuranceFund`.
	#[test]
	fn repayment_capped_to_actual_bad_debt() {
		new_test_ext().execute_with(|| {
			// Setup: Small bad debt
			BadDebt::<Test>::put(50 * PUSD);

			// Give `InsuranceFund` more pUSD than bad debt
			assert_ok!(Assets::mint_into(STABLECOIN_ASSET_ID, &INSURANCE_FUND, 200 * PUSD));

			// Try to repay more than bad debt
			assert_ok!(Vaults::heal(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			// Only 50 pUSD should be burned from `InsuranceFund`
			assert_eq!(BadDebt::<Test>::get(), 0);
			assert_eq!(Assets::balance(STABLECOIN_ASSET_ID, INSURANCE_FUND), 150 * PUSD);

			// Event should show actual amount
			System::assert_has_event(Event::<Test>::BadDebtRepaid { amount: 50 * PUSD }.into());
		});
	}

	/// **Test: No-op when no bad debt exists**
	///
	/// Attempting to repay when there's no bad debt does nothing.
	#[test]
	fn noop_when_no_bad_debt() {
		new_test_ext().execute_with(|| {
			// No bad debt
			assert_eq!(BadDebt::<Test>::get(), 0);

			// Give `InsuranceFund` some pUSD
			assert_ok!(Assets::mint_into(STABLECOIN_ASSET_ID, &INSURANCE_FUND, 100 * PUSD));

			// Try to repay - should succeed but do nothing
			assert_ok!(Vaults::heal(RuntimeOrigin::signed(ALICE), 100 * PUSD));

			// No pUSD should be burned from `InsuranceFund`
			assert_eq!(Assets::balance(STABLECOIN_ASSET_ID, INSURANCE_FUND), 100 * PUSD);

			// No event should be emitted (check no BadDebtRepaid event)
			let events = System::events();
			let has_repaid_event = events
				.iter()
				.any(|e| matches!(e.event, RuntimeEvent::Vaults(Event::BadDebtRepaid { .. })));
			assert!(!has_repaid_event, "No event when no bad debt to repay");
		});
	}

	/// **Test: Fails if `InsuranceFund` has insufficient pUSD**
	///
	/// `InsuranceFund` must have enough pUSD to cover the repayment amount.
	#[test]
	fn fails_with_insufficient_pusd() {
		new_test_ext().execute_with(|| {
			// Setup: Bad debt exists
			BadDebt::<Test>::put(100 * PUSD);

			// `InsuranceFund` has no pUSD
			assert_eq!(Assets::balance(STABLECOIN_ASSET_ID, INSURANCE_FUND), 0);

			// Should fail when trying to repay - `InsuranceFund` has no pUSD to burn
			assert_noop!(
				Vaults::heal(RuntimeOrigin::signed(ALICE), 50 * PUSD),
				TokenError::FundsUnavailable
			);

			// Bad debt unchanged
			assert_eq!(BadDebt::<Test>::get(), 100 * PUSD);
		});
	}
}

mod vault_status {
	use super::*;
	use crate::{CollateralManager, VaultStatus};

	/// **Test: New vaults start with `Healthy` status**
	#[test]
	fn new_vault_is_healthy() {
		new_test_ext().execute_with(|| {
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.status, VaultStatus::Healthy);
		});
	}

	/// **Test: Cannot deposit to vault in liquidation**
	#[test]
	fn cannot_deposit_to_liquidating_vault() {
		new_test_ext().execute_with(|| {
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			set_mock_price(Some(FixedU128::from_u32(3)));
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));

			assert_noop!(
				Vaults::deposit_collateral(RuntimeOrigin::signed(ALICE), 10 * DOT),
				Error::<Test>::VaultInLiquidation
			);
		});
	}

	/// **Test: Cannot withdraw from vault in liquidation**
	#[test]
	fn cannot_withdraw_from_liquidating_vault() {
		new_test_ext().execute_with(|| {
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			set_mock_price(Some(FixedU128::from_u32(3)));
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));

			assert_noop!(
				Vaults::withdraw_collateral(RuntimeOrigin::signed(ALICE), 10 * DOT),
				Error::<Test>::VaultInLiquidation
			);
		});
	}

	/// **Test: Cannot mint from vault in liquidation**
	#[test]
	fn cannot_mint_from_liquidating_vault() {
		new_test_ext().execute_with(|| {
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			set_mock_price(Some(FixedU128::from_u32(3)));
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));

			assert_noop!(
				Vaults::mint(RuntimeOrigin::signed(ALICE), 10 * PUSD),
				Error::<Test>::VaultInLiquidation
			);
		});
	}

	/// **Test: Cannot repay vault in liquidation**
	#[test]
	fn cannot_repay_liquidating_vault() {
		new_test_ext().execute_with(|| {
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			set_mock_price(Some(FixedU128::from_u32(3)));
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));

			assert_noop!(
				Vaults::repay(RuntimeOrigin::signed(ALICE), 10 * PUSD),
				Error::<Test>::VaultInLiquidation
			);
		});
	}

	/// **Test: Cannot close vault in liquidation**
	#[test]
	fn cannot_close_liquidating_vault() {
		new_test_ext().execute_with(|| {
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			set_mock_price(Some(FixedU128::from_u32(3)));
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));

			assert_noop!(
				Vaults::close_vault(RuntimeOrigin::signed(ALICE)),
				Error::<Test>::VaultInLiquidation
			);
		});
	}

	/// **Test: Cannot poke vault in liquidation**
	#[test]
	fn cannot_poke_liquidating_vault() {
		new_test_ext().execute_with(|| {
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			set_mock_price(Some(FixedU128::from_u32(3)));
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));

			assert_noop!(
				Vaults::poke(RuntimeOrigin::signed(BOB), ALICE),
				Error::<Test>::VaultInLiquidation
			);
		});
	}

	/// **Test: `complete_auction` removes vault immediately**
	#[test]
	fn auction_completed_removes_vault() {
		new_test_ext().execute_with(|| {
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			set_mock_price(Some(FixedU128::from_u32(3)));
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));

			// Vault exists with InLiquidation status
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.status, VaultStatus::InLiquidation);

			// Simulate auction completion
			assert_ok!(Vaults::complete_auction(&ALICE, 0, 0, &BOB, 0));

			// Vault should be removed immediately (no deferred cleanup)
			assert!(crate::Vaults::<Test>::get(ALICE).is_none());

			// `VaultClosed` event should be emitted
			System::assert_has_event(Event::<Test>::VaultClosed { owner: ALICE }.into());
		});
	}

	/// **Test: `on_idle` respects weight limits for fee updates**
	#[test]
	fn on_idle_respects_weight_limit() {
		new_test_ext().execute_with(|| {
			// Create vaults
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(BOB), 100 * DOT));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(BOB), 200 * PUSD));

			// Record initial timestamps
			let initial_timestamp_a = crate::Vaults::<Test>::get(ALICE).unwrap().last_fee_update;
			let initial_timestamp_b = crate::Vaults::<Test>::get(BOB).unwrap().last_fee_update;

			// Make vaults stale by advancing timestamp beyond threshold
			let stale_threshold = <Test as crate::Config>::StaleVaultThreshold::get();
			advance_timestamp(stale_threshold + 1);

			// Run on_idle with zero weight - should not do anything
			let weight = Vaults::on_idle(1, Weight::zero());
			assert_eq!(weight, Weight::zero());

			// Both vaults should still have old last_fee_update.
			let vault_a = crate::Vaults::<Test>::get(ALICE).unwrap();
			let vault_b = crate::Vaults::<Test>::get(BOB).unwrap();
			assert_eq!(vault_a.last_fee_update, initial_timestamp_a);
			assert_eq!(vault_b.last_fee_update, initial_timestamp_b);
		});
	}

	/// **Test: Can create new vault immediately after auction completion**
	#[test]
	fn can_create_vault_after_auction_completion() {
		new_test_ext().execute_with(|| {
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			set_mock_price(Some(FixedU128::from_u32(3)));
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));
			assert_ok!(Vaults::complete_auction(&ALICE, 0, 0, &BOB, 0));

			// Vault is removed immediately - ALICE can create a new vault right away
			set_mock_price(Some(FixedU128::from_rational(421, 100)));
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.status, VaultStatus::Healthy);
		});
	}

	/// **Test: Operations fail on non-existent vault after liquidation**
	///
	/// After complete_auction, the vault is removed, so operations
	/// fail with VaultNotFound instead of VaultInLiquidation.
	#[test]
	fn operations_fail_after_vault_liquidated() {
		new_test_ext().execute_with(|| {
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			set_mock_price(Some(FixedU128::from_u32(3)));
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));
			assert_ok!(Vaults::complete_auction(&ALICE, 0, 0, &BOB, 0));

			// Vault no longer exists - operations fail with VaultNotFound
			assert_noop!(
				Vaults::deposit_collateral(RuntimeOrigin::signed(ALICE), 10 * DOT),
				Error::<Test>::VaultNotFound
			);
			assert_noop!(
				Vaults::poke(RuntimeOrigin::signed(BOB), ALICE),
				Error::<Test>::VaultNotFound
			);
		});
	}
}

mod liquidation_limits {
	use super::*;
	use crate::{CurrentLiquidationAmount, MaxLiquidationAmount};
	use sp_pusd::CollateralManager;

	/// **Test: Liquidation blocked when `MaxLiquidationAmount` exceeded**
	///
	/// The `MaxLiquidationAmount` parameter is a HARD limit.
	/// Liquidations are blocked when `CurrentLiquidationAmount` + debt > max.
	#[test]
	fn liquidation_blocked_when_max_exceeded() {
		new_test_ext().execute_with(|| {
			// Set MaxLiquidationAmount to a very low value (100 pUSD)
			assert_ok!(Vaults::set_max_liquidation_amount(RuntimeOrigin::root(), 100 * PUSD));

			// Create a vault with enough collateral
			let deposit = 100 * DOT;
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			// Mint 200 pUSD - liquidation tab (debt + penalty) > MaxLiquidationAmount
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			// Drop price to make vault undercollateralized
			set_mock_price(Some(FixedU128::from_u32(2))); // $2 per DOT

			// Liquidation should FAIL because tab (226 pUSD) > MaxLiquidationAmount (100 pUSD)
			assert_noop!(
				Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE),
				Error::<Test>::ExceedsMaxLiquidationAmount
			);

			// CurrentLiquidationAmount should remain at 0
			assert_eq!(CurrentLiquidationAmount::<Test>::get(), 0);

			// Vault should still be healthy (liquidation didn't proceed)
			let vault = crate::Vaults::<Test>::get(ALICE).expect("Vault should still exist");
			assert_eq!(vault.status, crate::VaultStatus::Healthy);
		});
	}

	/// **Test: Liquidation succeeds when within limit**
	///
	/// When `CurrentLiquidationAmount` + tab <= `MaxLiquidationAmount`, liquidations
	/// proceed normally and `CurrentLiquidationAmount` is updated to track the new auction.
	#[test]
	fn liquidation_updates_current_amount() {
		new_test_ext().execute_with(|| {
			// MaxLiquidationAmount is 20,000,000 pUSD by default (from genesis)
			assert_eq!(MaxLiquidationAmount::<Test>::get(), 20_000_000 * PUSD);
			assert_eq!(CurrentLiquidationAmount::<Test>::get(), 0);

			// Create a vault
			let deposit = 100 * DOT;
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			// Drop price to make vault undercollateralized (below 180%)
			set_mock_price(Some(FixedU128::from_u32(3))); // $3 per DOT

			// Liquidation should succeed
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));

			// CurrentLiquidationAmount only tracks principal (not interest or penalty)
			// This ensures the counter returns to zero when principal is paid off
			let expected_principal = 200 * PUSD;
			assert_eq!(CurrentLiquidationAmount::<Test>::get(), expected_principal);
		});
	}

	/// **Test: `set_max_liquidation_amount` governance function**
	///
	/// Governance can update the `MaxLiquidationAmount` parameter to control
	/// liquidation throughput.
	#[test]
	fn set_max_liquidation_amount_works() {
		new_test_ext().execute_with(|| {
			let old_max = 20_000_000 * PUSD; // Genesis default
			let new_max = 500_000 * PUSD;

			assert_ok!(Vaults::set_max_liquidation_amount(RuntimeOrigin::root(), new_max));
			assert_eq!(MaxLiquidationAmount::<Test>::get(), new_max);

			System::assert_has_event(
				crate::Event::<Test>::MaxLiquidationAmountUpdated {
					old_value: old_max,
					new_value: new_max,
				}
				.into(),
			);
		});
	}

	/// **Test: `set_max_liquidation_amount` requires `ManagerOrigin`**
	///
	/// Only `ManagerOrigin` can modify the `MaxLiquidationAmount` parameter.
	#[test]
	fn set_max_liquidation_amount_requires_manager_origin() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Vaults::set_max_liquidation_amount(RuntimeOrigin::signed(ALICE), 500_000 * PUSD),
				frame_support::error::BadOrigin
			);
		});
	}

	/// **Test: `on_auction_debt_collected` reduces `CurrentLiquidationAmount`**
	///
	/// When the Auctions pallet collects pUSD from a bidder, it calls back
	/// to reduce `CurrentLiquidationAmount`, freeing up room for more liquidations.
	#[test]
	fn auction_callback_reduces_current_amount() {
		new_test_ext().execute_with(|| {
			// Manually set CurrentLiquidationAmount to simulate an active auction
			CurrentLiquidationAmount::<Test>::put(1000 * PUSD);

			// Simulate auction collecting 400 pUSD
			assert_ok!(Vaults::test_reduce_liquidation_amount(400 * PUSD));

			assert_eq!(CurrentLiquidationAmount::<Test>::get(), 600 * PUSD);

			System::assert_has_event(
				crate::Event::<Test>::AuctionDebtCollected { amount: 400 * PUSD }.into(),
			);
		});
	}

	/// **Test: Auction shortfall increases `BadDebt` and reduces `CurrentLiquidationAmount`**
	///
	/// When an auction completes with a shortfall (couldn't raise enough to cover the tab),
	/// the shortfall is recorded as `BadDebt` AND `CurrentLiquidationAmount` is reduced
	/// (since that portion of the tab will never be collected).
	#[test]
	fn auction_shortfall_increases_bad_debt_and_reduces_current_amount() {
		new_test_ext().execute_with(|| {
			// Setup: Simulate an auction started with 1000 pUSD tab
			CurrentLiquidationAmount::<Test>::put(1000 * PUSD);

			// Initially no bad debt
			assert_eq!(BadDebt::<Test>::get(), 0);

			// Simulate auction completing with 500 pUSD shortfall
			// (auction raised 500 pUSD but needed 1000 pUSD)
			assert_ok!(Vaults::test_record_shortfall(ALICE, 500 * PUSD));

			// BadDebt should increase by shortfall
			assert_eq!(BadDebt::<Test>::get(), 500 * PUSD);

			// CurrentLiquidationAmount should decrease by shortfall (uncollected portion)
			assert_eq!(CurrentLiquidationAmount::<Test>::get(), 500 * PUSD);

			// Event should be emitted
			System::assert_has_event(
				crate::Event::<Test>::AuctionShortfall { shortfall: 500 * PUSD }.into(),
			);
			System::assert_has_event(
				crate::Event::<Test>::BadDebtAccrued { owner: ALICE, amount: 500 * PUSD }.into(),
			);

			// Multiple shortfalls accumulate bad debt and reduce CurrentLiquidationAmount
			assert_ok!(Vaults::test_record_shortfall(ALICE, 200 * PUSD));
			assert_eq!(BadDebt::<Test>::get(), 700 * PUSD);
			assert_eq!(CurrentLiquidationAmount::<Test>::get(), 300 * PUSD);
			System::assert_has_event(
				crate::Event::<Test>::BadDebtAccrued { owner: ALICE, amount: 200 * PUSD }.into(),
			);
		});
	}

	/// **Test: Multiple liquidations track cumulative `CurrentLiquidationAmount`**
	///
	/// When multiple vaults are liquidated, `CurrentLiquidationAmount` accumulates correctly.
	/// At $2/DOT: 100 DOT = $200 collateral value.
	/// With 200 pUSD debt: ratio = 200/200 = 100% (< 130% minimum, liquidatable).
	#[test]
	fn multiple_liquidations_accumulate_current_amount() {
		new_test_ext().execute_with(|| {
			// Create two vaults with same parameters
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(BOB), 100 * DOT));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(BOB), 200 * PUSD));

			// Drop price to make both vaults undercollateralized
			set_mock_price(Some(FixedU128::from_u32(2))); // $2 per DOT

			// Liquidate both
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(CHARLIE), ALICE));
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(CHARLIE), BOB));

			// CurrentLiquidationAmount = sum of principals for both vaults
			// Each vault has 200 pUSD principal (penalty/interest not counted)
			let principal_per_vault = 200 * PUSD;
			let expected_total = principal_per_vault * 2;
			assert_eq!(CurrentLiquidationAmount::<Test>::get(), expected_total);
		});
	}

	/// **Test: `CurrentLiquidationAmount` returns to zero after complete auction**
	///
	/// This is an end-to-end test that verifies the counter properly tracks
	/// only principal and returns to zero when purchases pay off all principal.
	#[test]
	fn current_liquidation_amount_returns_to_zero_after_complete_auction() {
		use frame_support::traits::fungibles::Mutate as FungiblesMutate;

		new_test_ext().execute_with(|| {
			// Start with zero
			assert_eq!(CurrentLiquidationAmount::<Test>::get(), 0);

			// Create and liquidate vault
			let principal = 200 * PUSD;
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), principal));

			set_mock_price(Some(FixedU128::from_u32(3)));
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));

			// Counter should equal principal (not total_debt with penalty)
			assert_eq!(CurrentLiquidationAmount::<Test>::get(), principal);

			// Simulate a full purchase that pays off all principal
			// In real flow, auctions pallet calls execute_purchase with principal_paid
			let payment = crate::PaymentBreakdown::new(
				principal, // principal_paid
				0,         // interest_paid (no interest for simplicity)
				26 * PUSD, // penalty_paid (13% penalty)
			);

			// Give buyer enough pUSD
			Assets::mint_into(STABLECOIN_ASSET_ID, &BOB, payment.total()).unwrap();

			// Execute purchase - this decrements the counter
			assert_ok!(Vaults::execute_purchase(&BOB, 100 * DOT, payment, &CHARLIE, &ALICE,));

			// Counter should be zero after principal is fully paid
			assert_eq!(CurrentLiquidationAmount::<Test>::get(), 0);

			// Complete auction - no shortfall since principal was fully paid
			assert_ok!(Vaults::complete_auction(&ALICE, 0, 0, &BOB, 0));

			// Counter should still be zero
			assert_eq!(CurrentLiquidationAmount::<Test>::get(), 0);
		});
	}

	/// **Test: Partial purchases correctly decrement counter by `principal_paid`**
	///
	/// When an auction has multiple partial purchases, each one should
	/// decrement `CurrentLiquidationAmount` by the `principal_paid` amount,
	/// not the total collected amount.
	#[test]
	fn partial_purchases_decrement_counter_by_principal_paid() {
		new_test_ext().execute_with(|| {
			let principal = 200 * PUSD;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), principal));

			set_mock_price(Some(FixedU128::from_u32(3)));
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));

			// Counter starts at principal only
			assert_eq!(CurrentLiquidationAmount::<Test>::get(), principal);

			// Simulate partial purchase paying 50% of principal
			// The payment breakdown tracks principal_paid separately
			let half_principal = principal / 2;
			let payment = crate::PaymentBreakdown::new(
				half_principal, // principal_paid
				10 * PUSD,      // interest_paid
				13 * PUSD,      // penalty_paid
			);

			// Give BOB enough pUSD to pay
			use frame_support::traits::fungibles::Mutate as FungiblesMutate;
			Assets::mint_into(STABLECOIN_ASSET_ID, &BOB, payment.total()).unwrap();

			// Execute partial purchase
			assert_ok!(Vaults::execute_purchase(&BOB, 25 * DOT, payment, &CHARLIE, &ALICE,));

			// Counter should be decremented by principal_paid only (half remaining)
			let remaining_principal = principal - half_principal;
			assert_eq!(CurrentLiquidationAmount::<Test>::get(), remaining_principal);

			// Complete the auction with remaining principal as shortfall
			// (simulates auction ending without full debt recovery)
			assert_ok!(Vaults::complete_auction(&ALICE, 0, remaining_principal, &BOB, 0));

			// Counter should now be zero (decremented by shortfall)
			assert_eq!(CurrentLiquidationAmount::<Test>::get(), 0);
		});
	}
}

mod missing_error_cases {
	use super::*;
	use frame_support::traits::fungibles::Mutate as FungiblesMutate;

	/// **Test: repay fails if vault not found**
	///
	/// Cannot repay debt on a non-existent vault.
	#[test]
	fn repay_fails_if_vault_not_found() {
		new_test_ext().execute_with(|| {
			// ALICE has no vault
			assert!(crate::Vaults::<Test>::get(ALICE).is_none());

			// Give ALICE some pUSD to attempt repayment
			assert_ok!(Assets::mint_into(STABLECOIN_ASSET_ID, &ALICE, 100 * PUSD));

			// Should fail with VaultNotFound
			assert_noop!(
				Vaults::repay(RuntimeOrigin::signed(ALICE), 50 * PUSD),
				Error::<Test>::VaultNotFound
			);
		});
	}

	/// **Test: close_vault fails if vault not found**
	///
	/// Cannot close a non-existent vault.
	#[test]
	fn close_vault_fails_if_vault_not_found() {
		new_test_ext().execute_with(|| {
			// ALICE has no vault
			assert!(crate::Vaults::<Test>::get(ALICE).is_none());

			// Should fail with VaultNotFound
			assert_noop!(
				Vaults::close_vault(RuntimeOrigin::signed(ALICE)),
				Error::<Test>::VaultNotFound
			);
		});
	}

	/// **Test: liquidate fails if price not available**
	///
	/// Liquidation requires a valid oracle price to calculate
	/// the collateralization ratio. Without price, liquidation
	/// cannot proceed.
	#[test]
	fn liquidate_fails_if_price_not_available() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			// Remove oracle price
			set_mock_price(None);

			// Liquidation should fail - can't calculate collateralization ratio
			assert_noop!(
				Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE),
				Error::<Test>::PriceNotAvailable
			);
		});
	}

	/// **Test: Cannot liquidate a vault that's already in liquidation**
	///
	/// Once a vault enters liquidation, it cannot be liquidated again.
	/// This prevents double-counting in the auction system and ensures
	/// the vault lifecycle is properly managed.
	#[test]
	fn liquidate_fails_if_already_in_liquidation() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			// Drop price to make vault undercollateralized
			set_mock_price(Some(FixedU128::from_u32(3)));

			// First liquidation succeeds
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));

			// Verify vault is in liquidation
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.status, crate::VaultStatus::InLiquidation);

			// Second liquidation attempt should fail with VaultInLiquidation
			// The vault status check happens before any other processing
			assert_noop!(
				Vaults::liquidate_vault(RuntimeOrigin::signed(CHARLIE), ALICE),
				Error::<Test>::VaultInLiquidation
			);
		});
	}

	/// **Test: withdraw_collateral fails if price not available when vault has debt**
	///
	/// When a vault has outstanding debt, withdrawing collateral requires
	/// calculating the collateralization ratio, which needs the oracle price.
	#[test]
	fn withdraw_collateral_fails_if_price_not_available_with_debt() {
		new_test_ext().execute_with(|| {
			// Use 200 DOT so we can withdraw 10 DOT and still have 190 DOT (> 100 DOT min)
			let deposit = 200 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			// Mint 200 pUSD (well under max of ~421 pUSD at 200% ICR)
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			// Remove oracle price
			set_mock_price(None);

			// Should fail because we need price to:
			// 1. Calculate interest in DOT for collection
			// 2. Calculate collateralization ratio after withdrawal
			assert_noop!(
				Vaults::withdraw_collateral(RuntimeOrigin::signed(ALICE), 10 * DOT),
				Error::<Test>::PriceNotAvailable
			);
		});
	}
}

mod oracle_edge_cases {
	use super::*;

	/// **Test: withdraw_collateral succeeds without price when vault has no debt**
	///
	/// When a vault has no debt, we don't need the oracle price to withdraw
	/// because there's no collateralization ratio to check and no interest
	/// to collect.
	#[test]
	fn withdraw_succeeds_without_price_when_no_debt() {
		new_test_ext().execute_with(|| {
			let deposit = 200 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			// No debt minted - vault has no debt

			// Remove oracle price
			set_mock_price(None);

			// Should succeed - no price needed when there's no debt
			assert_ok!(Vaults::withdraw_collateral(RuntimeOrigin::signed(ALICE), 50 * DOT));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.get_held_collateral(&ALICE), 150 * DOT);
		});
	}

	/// **Test: close_vault succeeds without price**
	///
	/// The simplified interest model calculates interest purely in pUSD,
	/// so close_vault never needs the oracle price.
	#[test]
	fn close_vault_succeeds_without_price() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			// Remove oracle price
			set_mock_price(None);

			// Should succeed - close_vault doesn't need price
			assert_ok!(Vaults::close_vault(RuntimeOrigin::signed(ALICE)));

			// Vault should be removed
			assert!(crate::Vaults::<Test>::get(ALICE).is_none());
		});
	}
}

mod mint_edge_cases {
	use super::*;

	/// **Test: Minting zero amount fails with BelowMinimumMint**
	///
	/// Minting 0 pUSD is not allowed per the spec. Users must mint at least
	/// MinimumMint (5 pUSD). To trigger fee updates without minting, use `poke()`.
	#[test]
	fn mint_zero_amount_fails() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			// First mint some debt
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 100 * PUSD));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.principal, 100 * PUSD);

			// Mint 0 pUSD - should fail (below MinimumMint)
			assert_noop!(
				Vaults::mint(RuntimeOrigin::signed(ALICE), 0),
				Error::<Test>::BelowMinimumMint
			);

			// Principal unchanged
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.principal, 100 * PUSD, "Principal should remain unchanged");
		});
	}

	/// **Test: Multiple mints accumulate debt correctly**
	///
	/// Users can mint pUSD multiple times, with each mint adding to
	/// the total debt. The collateralization ratio must remain safe
	/// after each mint.
	#[test]
	fn mint_multiple_times_accumulates_debt() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			// First mint: 100 pUSD
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 100 * PUSD));
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.principal, 100 * PUSD);

			// Second mint: 50 pUSD
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 50 * PUSD));
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.principal, 150 * PUSD);

			// Third mint: 30 pUSD
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 30 * PUSD));
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.principal, 180 * PUSD);

			// Total pUSD minted
			assert_eq!(Assets::balance(STABLECOIN_ASSET_ID, ALICE), 180 * PUSD);
		});
	}

	/// **Test: Accrued interest reduces available minting capacity**
	///
	/// When a vault has accrued interest, the total obligation (debt + interest)
	/// is used for collateralization ratio calculation, reducing how much
	/// additional pUSD can be minted.
	#[test]
	fn mint_after_significant_interest_reduces_available_credit() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			// Mint 100 pUSD (ratio = 421/100 = 421%)
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 100 * PUSD));

			// Advance 1 year to accrue 4% interest = 4 pUSD.
			jump_to_block(5_256_000);

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert!(vault.accrued_interest > 0, "Should have accrued interest");

			// Now total obligation = 100 + ~4 = ~104 pUSD
			// At 200% initial ratio: max_total = 421 / 2.0 = 210.5 pUSD
			// Available to mint = 210.5 - 104 = ~106.5 pUSD

			// Try to mint 110 pUSD - should fail (would exceed with interest)
			assert_noop!(
				Vaults::mint(RuntimeOrigin::signed(ALICE), 110 * PUSD),
				Error::<Test>::UnsafeCollateralizationRatio
			);

			// Mint 100 pUSD - should succeed
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 100 * PUSD));
		});
	}

	/// **Test: Cannot mint after vault removed (post-auction)**
	///
	/// Once a vault is removed after auction completion,
	/// minting fails with VaultNotFound. Users must create a new vault.
	#[test]
	fn mint_to_liquidated_vault_fails() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			// Trigger liquidation
			set_mock_price(Some(FixedU128::from_u32(3)));
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));

			// Complete auction - vault is immediately removed
			use crate::CollateralManager;
			assert_ok!(Vaults::complete_auction(&ALICE, 0, 0, &BOB, 0));

			// Vault should be removed
			assert!(crate::Vaults::<Test>::get(ALICE).is_none());

			// Reset price
			set_mock_price(Some(FixedU128::from_rational(421, 100)));

			// Cannot mint - vault doesn't exist
			assert_noop!(
				Vaults::mint(RuntimeOrigin::signed(ALICE), 10 * PUSD),
				Error::<Test>::VaultNotFound
			);
		});
	}
}

mod repay_edge_cases {
	use super::*;
	use frame_support::traits::fungibles::Mutate as FungiblesMutate;

	/// **Test: Repaying zero amount succeeds as a no-op**
	///
	/// Repaying 0 pUSD is allowed but has no effect. This triggers
	/// fee updates without actually changing the debt.
	#[test]
	fn repay_zero_amount_succeeds_noop() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			let vault_before = crate::Vaults::<Test>::get(ALICE).unwrap();

			// Repay 0 pUSD
			assert_ok!(Vaults::repay(RuntimeOrigin::signed(ALICE), 0));

			let vault_after = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(
				vault_after.principal, vault_before.principal,
				"Principal should be unchanged"
			);
		});
	}

	/// **Test: Repay fails if user has insufficient pUSD balance**
	///
	/// Users cannot repay debt if they don't have enough pUSD.
	/// This tests the case where user spent/transferred their minted pUSD.
	#[test]
	fn repay_insufficient_pusd_balance_fails() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			// Transfer all pUSD to BOB (simulating spending)
			assert_ok!(Assets::transfer(
				RuntimeOrigin::signed(ALICE),
				STABLECOIN_ASSET_ID,
				BOB,
				200 * PUSD
			));

			// ALICE now has 0 pUSD but 200 pUSD debt
			assert_eq!(Assets::balance(STABLECOIN_ASSET_ID, ALICE), 0);

			// Repay should fail - no pUSD to burn
			assert_noop!(
				Vaults::repay(RuntimeOrigin::signed(ALICE), 100 * PUSD),
				TokenError::FundsUnavailable
			);
		});
	}

	/// **Test: Repay with interest-first ordering (mint-on-accrual model)**
	///
	/// The repay function pays in order:
	/// 1. Accrued interest first (burned from user)
	/// 2. Remaining amount goes to principal debt (burned)
	///
	/// Note: With the mint-on-accrual model, interest is minted to the Insurance Fund
	/// when fees accrue. On repay, both interest and principal are burned from the user.
	/// The IF keeps the pUSD that was minted during accrual.
	///
	/// Scenario: User repays 50 pUSD when interest is ~8 pUSD and debt is 200 pUSD.
	/// Result: Interest is fully paid, remaining 42 pUSD reduces principal.
	#[test]
	fn repay_reduces_debt_and_transfers_interest() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			// Check IF balance before any interest accrues
			let insurance_before_accrual = Assets::balance(STABLECOIN_ASSET_ID, INSURANCE_FUND);

			// Advance 1 year to accrue ~8 pUSD interest.
			jump_to_block(5_256_000);

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			let interest = vault.accrued_interest;
			assert!(interest > 0, "Should have accrued interest");

			// Verify IF received interest during accrual (mint-on-accrual model)
			let insurance_after_accrual = Assets::balance(STABLECOIN_ASSET_ID, INSURANCE_FUND);
			assert_eq!(
				insurance_after_accrual,
				insurance_before_accrual + interest,
				"InsuranceFund should receive interest on accrual"
			);

			// Repay 50 pUSD with interest-first ordering:
			// - First ~8 pUSD goes to interest (burned)
			// - Remaining ~42 pUSD goes to debt (burned)
			let repay_amount = 50 * PUSD;
			assert_ok!(Vaults::repay(RuntimeOrigin::signed(ALICE), repay_amount));

			let vault_after = crate::Vaults::<Test>::get(ALICE).unwrap();

			// Interest should be cleared (paid first)
			assert_eq!(vault_after.accrued_interest, 0, "Interest should be cleared");

			// Debt should be reduced by (repay_amount - interest)
			let debt_paid = repay_amount - interest;
			assert_eq!(
				vault_after.principal,
				200 * PUSD - debt_paid,
				"Principal should be reduced by remaining after interest"
			);

			// IF balance should not change on repay (interest was minted on accrual)
			let insurance_after_repay = Assets::balance(STABLECOIN_ASSET_ID, INSURANCE_FUND);
			assert_eq!(
				insurance_after_repay, insurance_after_accrual,
				"InsuranceFund balance unchanged on repay (already received on accrual)"
			);
		});
	}

	/// **Test: Multiple repays progressively reduce debt**
	///
	/// Sequential repayments with interest-first ordering:
	/// - First repay: pays interest, then remaining to debt
	/// - Subsequent repays: all goes to debt (no interest accrued in same block)
	#[test]
	fn repay_multiple_times_reduces_debt() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			// Advance 1 year to accrue ~8 pUSD interest.
			jump_to_block(5_256_000);

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			let interest = vault.accrued_interest;
			assert!(interest > 0, "Should have accrued interest");

			// Give ALICE extra pUSD to cover multiple repayments
			assert_ok!(Assets::mint_into(STABLECOIN_ASSET_ID, &ALICE, 100 * PUSD));

			// First repay: 50 pUSD pays interest first, then debt
			// - ~8 pUSD to interest
			// - ~42 pUSD to debt
			assert_ok!(Vaults::repay(RuntimeOrigin::signed(ALICE), 50 * PUSD));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.accrued_interest, 0, "Interest should be cleared");
			let debt_after_first = 200 * PUSD - (50 * PUSD - interest);
			assert_eq!(
				vault.principal, debt_after_first,
				"Debt reduced by remaining after interest"
			);

			// Second repay: all 50 pUSD goes to debt (no new interest since same block)
			assert_ok!(Vaults::repay(RuntimeOrigin::signed(ALICE), 50 * PUSD));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(
				vault.principal,
				debt_after_first - 50 * PUSD,
				"Principal should be reduced by 50"
			);

			// Third repay: all 50 pUSD goes to debt
			assert_ok!(Vaults::repay(RuntimeOrigin::signed(ALICE), 50 * PUSD));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(
				vault.principal,
				debt_after_first - 100 * PUSD,
				"Principal should be reduced by another 50"
			);
		});
	}
}

mod liquidation_additional {
	use super::*;

	/// **Test: Owner can liquidate their own vault (self-liquidation)**
	///
	/// There's no restriction preventing vault owners from triggering
	/// liquidation on their own undercollateralized vault.
	#[test]
	fn self_liquidation_works() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			// Drop price to make vault undercollateralized
			set_mock_price(Some(FixedU128::from_u32(3)));

			// ALICE liquidates her own vault
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(ALICE), ALICE));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.status, crate::VaultStatus::InLiquidation);
		});
	}

	/// **Test: Accrued interest can push vault under liquidation threshold**
	///
	/// A vault that was safe can become liquidatable if enough interest
	/// accrues without the owner taking action.
	#[test]
	fn interest_accrual_can_trigger_liquidation() {
		new_test_ext().execute_with(|| {
			// Set price to $2/DOT for easier math
			set_mock_price(Some(FixedU128::from_u32(2)));

			let deposit = 100 * DOT; // $200 value

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			// Mint 100 pUSD (ratio = 200/100 = 200%, exactly at ICR)
			// With interest, it will eventually drop below 180% MCR
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 100 * PUSD));

			// Verify initially safe
			assert_noop!(
				Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE),
				Error::<Test>::VaultIsSafe
			);

			// Advance 5 years to accrue significant interest.
			// 4% * 5 years * 100 = 20 pUSD interest
			// Total obligation = 100 + 20 = 120 pUSD
			// Ratio = 200 / 120 = 166.7% < 180%
			jump_to_block(5_256_000 * 5);

			// Now liquidation should work.
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));
		});
	}

	/// **Test: Liquidation fails at zero price**
	///
	/// A zero price indicates an oracle bug, not a valid market condition.
	/// The system should treat zero price as invalid and reject the operation.
	#[test]
	fn liquidation_fails_at_zero_price() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			// Set price to zero.
			set_mock_price(Some(FixedU128::zero()));

			assert_noop!(
				Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE),
				Error::<Test>::PriceNotAvailable
			);
		});
	}

	/// **Test: Vault is safe at very high price**
	///
	/// When price increases significantly, an undercollateralized vault
	/// becomes safe and cannot be liquidated.
	#[test]
	fn vault_safe_at_very_high_price() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			// At $4.21: 100 DOT = $421, ratio = 421/200 = 210.5%

			// Set very high price: $100/DOT
			set_mock_price(Some(FixedU128::from_u32(100)));

			// At $100: 100 DOT = $10,000, ratio = 10000/200 = 5000%
			assert_noop!(
				Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE),
				Error::<Test>::VaultIsSafe
			);
		});
	}
}

mod collateral_manager {
	use super::*;
	use crate::CollateralManager;
	use frame_support::traits::{fungible::InspectHold, fungibles::Mutate as FungiblesMutate};
	use sp_runtime::traits::CheckedDiv;

	/// **Test: `execute_purchase` burns pUSD and transfers collateral**
	///
	/// When an auction purchase is executed, the Vaults pallet:
	/// 1. Burns pUSD from the buyer
	/// 2. Releases collateral from Seized hold
	/// 3. Transfers collateral to the recipient
	#[test]
	fn execute_purchase_burns_pusd_and_transfers_collateral() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			// Trigger liquidation to seize collateral
			set_mock_price(Some(FixedU128::from_u32(3)));
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));

			// Verify collateral is seized
			let seized = Balances::balance_on_hold(&crate::HoldReason::Seized.into(), &ALICE);
			assert!(seized > 0, "Collateral should be seized");

			// Give BOB pUSD to purchase
			assert_ok!(Assets::mint_into(STABLECOIN_ASSET_ID, &BOB, 100 * PUSD));

			let bob_pusd_before = Assets::balance(STABLECOIN_ASSET_ID, BOB);
			let charlie_dot_before = Balances::free_balance(CHARLIE);

			// Execute purchase: BOB pays 50 pUSD for 20 DOT, sent to CHARLIE
			// For testing, we treat the entire amount as principal (burned)
			assert_ok!(Vaults::execute_purchase(
				&BOB,
				20 * DOT,
				crate::PaymentBreakdown::new(50 * PUSD, 0, 0),
				&CHARLIE,
				&ALICE,
			));

			// BOB's pUSD burned (50 pUSD)
			assert_eq!(Assets::balance(STABLECOIN_ASSET_ID, BOB), bob_pusd_before - 50 * PUSD);

			// CHARLIE received collateral (20 DOT)
			assert_eq!(Balances::free_balance(CHARLIE), charlie_dot_before + 20 * DOT);
		});
	}

	/// **Test: `execute_purchase` fails with insufficient pUSD**
	///
	/// If the buyer doesn't have enough pUSD, the purchase fails.
	#[test]
	fn execute_purchase_fails_insufficient_pusd() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			set_mock_price(Some(FixedU128::from_u32(3)));
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));

			// BOB has no pUSD
			assert_eq!(Assets::balance(STABLECOIN_ASSET_ID, BOB), 0);

			// Execute purchase should fail (BOB has no pUSD to burn)
			assert_err!(
				Vaults::execute_purchase(
					&BOB,
					20 * DOT,
					crate::PaymentBreakdown::new(50 * PUSD, 0, 0),
					&CHARLIE,
					&ALICE,
				),
				TokenError::FundsUnavailable
			);
		});
	}

	/// **Test: `complete_auction` returns excess collateral to owner**
	///
	/// When an auction completes with remaining collateral (debt fully satisfied),
	/// the excess is returned to the vault owner.
	#[test]
	fn complete_auction_returns_excess_collateral() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			set_mock_price(Some(FixedU128::from_u32(3)));
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));

			let alice_balance_before = Balances::free_balance(ALICE);

			// Simulate auction completion with 30 DOT remaining
			let remaining_collateral = 30 * DOT;
			assert_ok!(Vaults::complete_auction(&ALICE, remaining_collateral, 0, &BOB, 0));

			// Vault should be immediately removed
			assert!(crate::Vaults::<Test>::get(ALICE).is_none());

			// ALICE should receive the remaining collateral
			assert_eq!(Balances::free_balance(ALICE), alice_balance_before + remaining_collateral);

			// `VaultClosed` event should be emitted
			System::assert_has_event(Event::<Test>::VaultClosed { owner: ALICE }.into());
		});
	}

	/// **Test: `complete_auction` records shortfall as bad debt**
	///
	/// When an auction completes with a shortfall (remaining unpaid principal),
	/// the shortfall is recorded as system bad debt.
	///
	/// Only principal shortfall becomes bad debt. Interest/penalty shortfall
	/// is not collected, not recorded as bad debt.
	#[test]
	fn complete_auction_records_shortfall_as_bad_debt() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			set_mock_price(Some(FixedU128::from_u32(3)));
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));

			let bad_debt_before = crate::BadDebt::<Test>::get();

			// Simulate auction completion with 100 pUSD remaining principal (bad debt)
			let shortfall = 100 * PUSD;
			assert_ok!(Vaults::complete_auction(&ALICE, 0, shortfall, &BOB, 0));

			// Bad debt should increase
			assert_eq!(crate::BadDebt::<Test>::get(), bad_debt_before + shortfall);

			// AuctionShortfall event
			System::assert_has_event(Event::<Test>::AuctionShortfall { shortfall }.into());
			System::assert_has_event(
				Event::<Test>::BadDebtAccrued { owner: ALICE, amount: shortfall }.into(),
			);
		});
	}

	/// **Test: `get_dot_price` returns oracle price for the collateral asset**
	///
	/// The `CollateralManager` trait exposes the oracle price to the Auctions pallet.
	/// Note: The mock oracle normalizes prices (raw_price * 10^6 / 10^10).
	#[test]
	fn get_dot_price_returns_oracle_price() {
		new_test_ext().execute_with(|| {
			// Default raw price is 4.21 USD/DOT
			// Normalized: 4.21 * 10^6 / 10^10 = 0.000421
			let price = Vaults::get_dot_price();
			assert!(price.is_some(), "Price should be available");
			let expected_normalized = FixedU128::from_rational(421, 100)
				.saturating_mul(FixedU128::saturating_from_integer(1_000_000u128))
				.checked_div(&FixedU128::saturating_from_integer(10_000_000_000u128))
				.unwrap();
			assert_eq!(price, Some(expected_normalized));

			// Change price to $5/DOT
			set_mock_price(Some(FixedU128::from_u32(5)));
			let price = Vaults::get_dot_price();
			let expected_normalized = FixedU128::from_u32(5)
				.saturating_mul(FixedU128::saturating_from_integer(1_000_000u128))
				.checked_div(&FixedU128::saturating_from_integer(10_000_000_000u128))
				.unwrap();
			assert_eq!(price, Some(expected_normalized));

			// Remove price
			set_mock_price(None);
			let price = Vaults::get_dot_price();
			assert_eq!(price, None);
		});
	}
}

mod lifecycle_integration {
	use super::*;
	use frame_support::traits::fungibles::Mutate as FungiblesMutate;

	/// **Test: Full vault lifecycle from creation to closure**
	///
	/// Tests the complete happy path: create → deposit → mint → repay → close.
	#[test]
	fn full_lifecycle_create_mint_repay_close() {
		new_test_ext().execute_with(|| {
			let initial_deposit = 100 * DOT;
			let additional_deposit = 50 * DOT;
			let total_collateral = initial_deposit + additional_deposit;

			// Record starting balance
			let alice_balance_before = Balances::free_balance(ALICE);

			// 1. Create vault
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), initial_deposit));
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.status, crate::VaultStatus::Healthy);
			assert_eq!(vault.principal, 0);

			// 2. Mint pUSD
			let mint_amount = 200 * PUSD;
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), mint_amount));
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.principal, mint_amount);

			// 3. Add more collateral
			assert_ok!(Vaults::deposit_collateral(
				RuntimeOrigin::signed(ALICE),
				additional_deposit
			));
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.get_held_collateral(&ALICE), total_collateral);

			// 4. Advance time to accrue interest.
			jump_to_block(2_628_000); // ~6 months, ~4 pUSD interest

			// 5. Repay all debt + interest
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			let interest = vault.accrued_interest;

			// Give ALICE enough to cover interest
			if interest > 0 {
				assert_ok!(Assets::mint_into(STABLECOIN_ASSET_ID, &ALICE, interest));
			}

			// Repay everything
			assert_ok!(Vaults::repay(RuntimeOrigin::signed(ALICE), mint_amount + interest));
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.principal, 0);
			assert_eq!(vault.accrued_interest, 0);

			// 6. Close vault
			assert_ok!(Vaults::close_vault(RuntimeOrigin::signed(ALICE)));
			assert!(crate::Vaults::<Test>::get(ALICE).is_none());

			// Verify all collateral returned - ALICE's balance should be restored
			let alice_balance_after = Balances::free_balance(ALICE);
			assert_eq!(
				alice_balance_after, alice_balance_before,
				"All collateral should be returned after closing vault"
			);

			// Verify ALICE has no remaining pUSD (all burned during repay)
			assert_eq!(
				Assets::balance(STABLECOIN_ASSET_ID, ALICE),
				0,
				"All pUSD should be burned after full repayment"
			);
		});
	}

	/// **Test: Vault becomes safe after price increase, can mint more**
	///
	/// When price increases, the collateralization ratio improves,
	/// allowing the user to mint additional pUSD.
	#[test]
	fn vault_becomes_safe_after_price_increase_can_mint_more() {
		new_test_ext().execute_with(|| {
			// Start at $4.21/DOT
			let deposit = 100 * DOT; // $421 value

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));

			// Mint near max: 200 pUSD (ratio = 421/200 = 210.5%)
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			// Cannot mint more (would breach 200%)
			assert_noop!(
				Vaults::mint(RuntimeOrigin::signed(ALICE), 15 * PUSD),
				Error::<Test>::UnsafeCollateralizationRatio
			);

			// Price increases to $10/DOT
			set_mock_price(Some(FixedU128::from_u32(10)));

			// Now: 100 DOT = $1000, ratio = 1000/200 = 500%
			// Max mint at 200%: 1000/2.0 = 500 pUSD
			// Available: 500 - 200 = 300 pUSD

			// Can now mint more
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 280 * PUSD));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.principal, 480 * PUSD);
		});
	}

	/// **Test: Interest accrues correctly over multiple time periods**
	///
	/// Interest calculation is consistent across multiple time periods.
	#[test]
	fn interest_accrues_over_multiple_operations() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			// Advance 3 months.
			jump_to_block(1_314_000);

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			let interest_q1 = vault.accrued_interest;
			assert!(interest_q1 > 0, "Should accrue interest in Q1");

			// Advance another 3 months (to 6 months total)
			jump_to_block(2_628_000);

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			let interest_q2 = vault.accrued_interest;
			assert!(interest_q2 > interest_q1, "Should accrue more interest in Q2");

			// Advance another 6 months (to 1 year total)
			jump_to_block(5_256_000);

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			let interest_full_year = vault.accrued_interest;

			// 4% of 200 pUSD for 1 year = 8 pUSD
			let expected = 8 * PUSD;
			assert_approx_eq(
				interest_full_year,
				expected,
				INTEREST_TOLERANCE,
				"Interest after full year",
			);
		});
	}
}

mod overflow_protection {
	use super::*;
	use frame_support::traits::fungible::Mutate as FungibleMutate;

	/// **Test: Very large debt amounts don't overflow**
	///
	/// The system correctly handles very large debt values without
	/// arithmetic overflow in ratio calculations.
	#[test]
	fn very_large_debt_does_not_overflow() {
		new_test_ext().execute_with(|| {
			// Set very high max debt and position limits
			MaximumIssuance::<Test>::put(u128::MAX / 2);
			MaxPositionAmount::<Test>::put(u128::MAX / 2);

			// Give ALICE a lot of DOT
			let huge_deposit = 1_000_000_000 * DOT; // 1 billion DOT
			let _ = Balances::mint_into(&ALICE, huge_deposit);

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), huge_deposit));

			// Mint a large amount (but within ratio)
			// At $4.21/DOT: 1B DOT = $4.21B, max mint = $2.8B pUSD
			let large_mint = 2_000_000_000 * PUSD; // 2 billion pUSD
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), large_mint));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.principal, large_mint);
		});
	}

	/// **Test: Very large collateral amounts don't overflow**
	///
	/// The system handles very large collateral values without overflow
	/// in hold operations and ratio calculations.
	#[test]
	fn very_large_collateral_does_not_overflow() {
		new_test_ext().execute_with(|| {
			// Give ALICE maximum DOT
			let huge_deposit = u128::MAX / 4;
			let _ = Balances::mint_into(&ALICE, huge_deposit);

			// Create vault with huge deposit
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), huge_deposit));

			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.get_held_collateral(&ALICE), huge_deposit);
		});
	}
}

mod on_idle_edge_cases {
	use super::*;

	/// **Test: `on_idle` with no vaults uses minimal weight**
	///
	/// When there are no vaults to process, `on_idle` should return
	/// quickly with minimal weight consumption.
	#[test]
	fn on_idle_with_no_vaults_uses_minimal_weight() {
		new_test_ext().execute_with(|| {
			// No vaults created
			assert!(crate::Vaults::<Test>::iter().next().is_none());

			let weight = Vaults::on_idle(1, Weight::MAX);

			// Should consume minimum weight for reading cursor and config
			// (on_idle_base_weight = reads_writes(2, 1) for cursor and stale threshold)
			assert_eq!(weight, Vaults::on_idle_base_weight(), "No vaults = only base weight");
		});
	}

	/// **Test: `on_idle` updates stale vault fees**
	///
	/// Vaults that haven't been touched for `StaleVaultThreshold`
	/// get their fees updated during `on_idle`.
	#[test]
	fn on_idle_updates_stale_vault_fees() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			let vault_before = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault_before.accrued_interest, 0);

			// Advance past StaleVaultThreshold (14_400_000 ms = 4 hours) plus enough
			// time to accrue meaningful interest
			jump_to_block(5_256_000); // 1 year worth of blocks.

			// Run on_idle (already called by jump_to_block, but call again to verify)
			let weight = Vaults::on_idle(5_256_000, Weight::MAX);
			assert!(weight.ref_time() > 0, "Should have done work");

			// Vault should have updated fees
			let vault_after = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert!(vault_after.accrued_interest > 0, "on_idle should update stale vault fees");
			assert!(
				vault_after.last_fee_update > vault_before.last_fee_update,
				"Fee timestamp should be updated"
			);
		});
	}

	/// **Test: `on_idle` skips healthy non-stale vaults**
	///
	/// Vaults that have been recently touched (within `StaleVaultThreshold`)
	/// are not updated by `on_idle`.
	#[test]
	fn on_idle_skips_healthy_non_stale_vaults() {
		new_test_ext().execute_with(|| {
			let deposit = 100 * DOT;

			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), deposit));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			// Touch the vault
			run_to_block(100);
			assert_ok!(Vaults::deposit_collateral(RuntimeOrigin::signed(ALICE), 0));

			let vault_before = crate::Vaults::<Test>::get(ALICE).unwrap();

			// Advance less than StaleVaultThreshold (14_400_000 ms = 4 hours)
			// 1900 blocks = 11,400,000 ms < 14,400,000 ms threshold
			run_to_block(2000);

			// Run on_idle
			let _weight = Vaults::on_idle(2000, Weight::MAX);

			// Should not have updated the vault (not stale yet)
			let vault_after = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(
				vault_after.last_fee_update, vault_before.last_fee_update,
				"Non-stale vault should not be updated"
			);
		});
	}

	/// **Test: `on_idle` processes multiple vault types correctly**
	///
	/// When there are `Healthy` and `InLiquidation` vaults,
	/// `on_idle` handles each appropriately.
	#[test]
	fn on_idle_handles_mixed_vault_states() {
		new_test_ext().execute_with(|| {
			// Create ALICE's vault - will be `Healthy`
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));

			// Create BOB's vault - will be `InLiquidation`
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(BOB), 100 * DOT));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(BOB), 200 * PUSD));

			set_mock_price(Some(FixedU128::from_u32(3)));
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(CHARLIE), BOB));

			use crate::CollateralManager;
			assert_ok!(Vaults::complete_auction(&BOB, 0, 0, &ALICE, 0));

			// BOB's vault should be immediately removed after auction completion
			assert!(crate::Vaults::<Test>::get(BOB).is_none());

			// Reset price
			set_mock_price(Some(FixedU128::from_rational(421, 100)));

			// Create CHARLIE's vault - will be `InLiquidation`
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(CHARLIE), 100 * DOT));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(CHARLIE), 200 * PUSD));

			set_mock_price(Some(FixedU128::from_u32(3)));
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(ALICE), CHARLIE));

			// Verify states - only ALICE (`Healthy`) and CHARLIE (`InLiquidation`)
			assert_eq!(
				crate::Vaults::<Test>::get(ALICE).unwrap().status,
				crate::VaultStatus::Healthy
			);
			assert_eq!(
				crate::Vaults::<Test>::get(CHARLIE).unwrap().status,
				crate::VaultStatus::InLiquidation
			);

			// Run `on_idle` - no cleanup needed, only stale fee updates
			Vaults::on_idle(1, Weight::MAX);

			// ALICE's `Healthy` vault should remain
			assert!(crate::Vaults::<Test>::get(ALICE).is_some());

			// CHARLIE's `InLiquidation` vault should remain (auction in progress)
			assert!(crate::Vaults::<Test>::get(CHARLIE).is_some());
		});
	}
}

mod parameter_edge_cases {
	use super::*;

	/// **Test: Setting initial ratio below minimum ratio fails**
	///
	/// Initial ratio must be >= minimum ratio to allow borrowing.
	/// Setting initial_ratio < min_ratio would make all loans impossible.
	#[test]
	fn set_initial_ratio_below_minimum_fails() {
		new_test_ext().execute_with(|| {
			// Set minimum to 160% first
			assert_ok!(Vaults::set_minimum_collateralization_ratio(
				RuntimeOrigin::root(),
				ratio(160)
			));

			// Try to set initial to 140% (below 160% minimum) - should fail
			assert_noop!(
				Vaults::set_initial_collateralization_ratio(RuntimeOrigin::root(), ratio(140)),
				Error::<Test>::InitialRatioMustExceedMinimum
			);

			// Initial ratio should remain at original value (200%)
			assert_eq!(InitialCollateralizationRatio::<Test>::get(), ratio(200));
		});
	}

	/// **Test: Setting minimum ratio above initial ratio fails**
	///
	/// Minimum ratio cannot exceed initial ratio, as this would allow
	/// immediate liquidation after minting (mint at initial CR, liquidate at min CR).
	#[test]
	fn set_minimum_ratio_above_initial_fails() {
		new_test_ext().execute_with(|| {
			// Initial ratio is 200% (genesis default)
			assert_eq!(InitialCollateralizationRatio::<Test>::get(), ratio(200));

			// Try to set minimum to 220% (above 200% initial) - should fail
			assert_noop!(
				Vaults::set_minimum_collateralization_ratio(RuntimeOrigin::root(), ratio(220)),
				Error::<Test>::InitialRatioMustExceedMinimum
			);

			// Minimum ratio should remain at original value (180%)
			assert_eq!(MinimumCollateralizationRatio::<Test>::get(), ratio(180));
		});
	}

	/// **Test: Setting liquidation penalty to zero works**
	///
	/// Governance can set liquidation penalty to 0%, removing the
	/// financial penalty for being liquidated.
	#[test]
	fn set_liquidation_penalty_to_zero() {
		new_test_ext().execute_with(|| {
			assert_ok!(Vaults::set_liquidation_penalty(RuntimeOrigin::root(), Permill::zero()));

			assert_eq!(LiquidationPenalty::<Test>::get(), Permill::zero());

			// Create and liquidate a vault to verify it works
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			set_mock_price(Some(FixedU128::from_u32(3)));
			assert_ok!(Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE));

			// Liquidation should work with 0% penalty
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.status, crate::VaultStatus::InLiquidation);
		});
	}

	/// **Test: Setting stability fee to zero stops interest accrual**
	///
	/// With 0% stability fee, vaults should not accrue any interest
	/// over time.
	#[test]
	fn set_stability_fee_to_zero() {
		new_test_ext().execute_with(|| {
			assert_ok!(Vaults::set_stability_fee(RuntimeOrigin::root(), Permill::zero()));

			assert_eq!(StabilityFee::<Test>::get(), Permill::zero());

			// Create vault with debt
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 200 * PUSD));

			// Advance significant time
			jump_to_block(5_256_000); // 1 year

			// Trigger fee update
			assert_ok!(Vaults::poke(RuntimeOrigin::signed(BOB), ALICE));

			// No interest should have accrued
			let vault = crate::Vaults::<Test>::get(ALICE).unwrap();
			assert_eq!(vault.accrued_interest, 0, "Zero fee = zero interest");
		});
	}

	/// **Test: Setting max debt to zero blocks all minting**
	///
	/// With `MaximumIssuance` set to 0, no new pUSD can be minted by anyone.
	#[test]
	fn set_max_debt_to_zero_blocks_all_minting() {
		new_test_ext().execute_with(|| {
			// First create a vault with some debt
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 100 * PUSD));

			// Set max debt to 0
			MaximumIssuance::<Test>::put(0);

			// BOB creates a new vault
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(BOB), 100 * DOT));

			// BOB cannot mint any pUSD above the minimum mint amount
			assert_noop!(
				Vaults::mint(RuntimeOrigin::signed(BOB), 5 * PUSD),
				Error::<Test>::ExceedsMaxDebt
			);

			// ALICE also cannot mint more
			assert_noop!(
				Vaults::mint(RuntimeOrigin::signed(ALICE), 5 * PUSD),
				Error::<Test>::ExceedsMaxDebt
			);
		});
	}
}

mod oracle_staleness {
	use super::*;

	/// Get staleness threshold from config
	fn staleness_threshold() -> u64 {
		OracleStalenessThreshold::get()
	}

	/// **Test: Mint fails when oracle is stale**
	///
	/// When the oracle price timestamp is older than the staleness threshold,
	/// minting should fail with `OracleStale` error.
	#[test]
	fn mint_fails_when_oracle_is_stale() {
		new_test_ext().execute_with(|| {
			// Create vault with collateral
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));

			// Make oracle stale by setting timestamp to 2x threshold ago
			let stale_timestamp = MockTimestamp::get().saturating_sub(2 * staleness_threshold());
			set_mock_price_timestamp(stale_timestamp);

			// Mint should fail with OracleStale
			assert_noop!(
				Vaults::mint(RuntimeOrigin::signed(ALICE), 10 * PUSD),
				Error::<Test>::OracleStale
			);
		});
	}

	/// **Test: Withdraw with debt fails when oracle is stale**
	///
	/// When the vault has debt, withdrawing collateral requires a price check.
	/// This should fail if the oracle is stale.
	#[test]
	fn withdraw_with_debt_fails_when_oracle_stale() {
		new_test_ext().execute_with(|| {
			// Setup vault with debt (use 200 DOT to allow partial withdrawal)
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 200 * DOT));
			set_mock_price_timestamp(MockTimestamp::get());
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 10 * PUSD));

			// Make oracle stale
			let stale_timestamp = MockTimestamp::get().saturating_sub(2 * staleness_threshold());
			set_mock_price_timestamp(stale_timestamp);

			// Withdraw should fail due to stale oracle (remaining 190 DOT > min deposit)
			assert_noop!(
				Vaults::withdraw_collateral(RuntimeOrigin::signed(ALICE), 10 * DOT),
				Error::<Test>::OracleStale
			);
		});
	}

	/// **Test: Withdraw without debt succeeds even when oracle is stale**
	///
	/// When the vault has no debt, withdrawing collateral doesn't need a price check,
	/// so it should succeed even with a stale oracle.
	#[test]
	fn withdraw_without_debt_succeeds_even_when_oracle_stale() {
		new_test_ext().execute_with(|| {
			// Vault with no debt
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 200 * DOT));

			// Make oracle stale
			let stale_timestamp = MockTimestamp::get().saturating_sub(2 * staleness_threshold());
			set_mock_price_timestamp(stale_timestamp);

			// Withdraw should succeed (no debt, no price check needed)
			assert_ok!(Vaults::withdraw_collateral(RuntimeOrigin::signed(ALICE), 50 * DOT));
		});
	}

	/// **Test: Liquidation fails when oracle is stale**
	///
	/// Liquidating a vault requires checking the collateralization ratio,
	/// which requires a fresh price.
	#[test]
	fn liquidate_fails_when_oracle_stale() {
		new_test_ext().execute_with(|| {
			// Set a specific price for predictable ratio calculation
			set_mock_price(Some(FixedU128::from_u32(2))); // $2/DOT

			// Setup vault that will be undercollateralized
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));
			set_mock_price_timestamp(MockTimestamp::get());
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 100 * PUSD)); // 200% ratio

			// Drop price to make vault undercollateralized
			set_mock_price(Some(FixedU128::from_u32(1))); // $1/DOT -> 100% ratio < 180% MCR

			// Make oracle stale
			let stale_timestamp = MockTimestamp::get().saturating_sub(2 * staleness_threshold());
			set_mock_price_timestamp(stale_timestamp);

			// Liquidation should fail
			assert_noop!(
				Vaults::liquidate_vault(RuntimeOrigin::signed(BOB), ALICE),
				Error::<Test>::OracleStale
			);
		});
	}

	/// **Test: Operations auto-resume when oracle becomes fresh**
	///
	/// After the oracle was stale and operations failed, they should
	/// succeed again once a fresh price is available.
	#[test]
	fn operations_auto_resume_when_oracle_becomes_fresh() {
		new_test_ext().execute_with(|| {
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));

			// Oracle stale - mint fails
			let stale_timestamp = MockTimestamp::get().saturating_sub(2 * staleness_threshold());
			set_mock_price_timestamp(stale_timestamp);
			assert_noop!(
				Vaults::mint(RuntimeOrigin::signed(ALICE), 10 * PUSD),
				Error::<Test>::OracleStale
			);

			// Oracle fresh again - mint succeeds
			set_mock_price_timestamp(MockTimestamp::get());
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 10 * PUSD));
		});
	}

	/// **Test: Oracle at exact threshold boundary is considered fresh**
	///
	/// A price that is exactly at the staleness threshold should still
	/// be considered valid.
	#[test]
	fn oracle_at_exact_threshold_is_fresh() {
		new_test_ext().execute_with(|| {
			assert_ok!(Vaults::create_vault(RuntimeOrigin::signed(ALICE), 100 * DOT));

			// Set timestamp to exactly at threshold boundary
			let at_threshold = MockTimestamp::get().saturating_sub(staleness_threshold());
			set_mock_price_timestamp(at_threshold);

			// Should still succeed (at boundary, not past it)
			assert_ok!(Vaults::mint(RuntimeOrigin::signed(ALICE), 10 * PUSD));
		});
	}
}

mod parameter_invariants {
	use super::*;

	/// **Mathematical invariant test: Liquidation penalty vs keeper incentive**
	///
	/// This test verifies a mathematical property of the protocol parameters.
	/// It uses assumed keeper incentive values (tip=1 pUSD, chip=0.1%) that should
	/// match the auctions pallet configuration.
	///
	/// Keeper incentive formula (from auctions pallet):
	///   keeper_incentive = tip + (chip × tab)
	///
	/// Where:
	///   - tip = flat fee (default: 1 pUSD)
	///   - chip = percentage of tab (default: 0.1%)
	///   - tab = principal + interest + penalty
	///
	/// The auction pallet caps keeper_incentive to penalty, but this test
	/// verifies that with reasonable parameters, the penalty naturally exceeds
	/// the keeper incentive for typical vault sizes.
	#[test]
	fn liquidation_penalty_exceeds_keeper_incentive() {
		new_test_ext().execute_with(|| {
			// Keeper incentive parameters.
			let tip = 1 * PUSD; // 1 pUSD flat fee
			let chip = Permill::from_parts(1000); // 0.1%

			// Get liquidation penalty from storage
			let liquidation_penalty = LiquidationPenalty::<Test>::get();

			// Test various vault sizes to ensure the invariant holds
			let test_principals = [
				100 * PUSD,       // Small vault
				1_000 * PUSD,     // Medium vault
				10_000 * PUSD,    // Large vault
				100_000 * PUSD,   // Very large vault
				1_000_000 * PUSD, // Massive vault
			];

			for principal in test_principals {
				// Assume 10% interest for a stressed scenario
				let interest = principal / 10;

				// Calculate penalty
				let penalty = liquidation_penalty.mul_floor(principal);

				// Calculate tab (total debt for auction)
				let tab = principal + interest + penalty;

				// Calculate keeper incentive (before capping)
				let keeper_incentive_raw = tip + chip.mul_floor(tab);

				// The penalty should exceed the raw keeper incentive
				// (the auction pallet will cap it anyway, but we want margin)
				assert!(
					penalty > keeper_incentive_raw,
					"Penalty ({} pUSD) should exceed keeper incentive ({} pUSD) for principal {} pUSD. \
					 This ensures keeper is paid only from penalty, not from principal/interest.",
					penalty / PUSD,
					keeper_incentive_raw / PUSD,
					principal / PUSD
				);

				// Log the margin for visibility
				let margin = penalty.saturating_sub(keeper_incentive_raw);
				let margin_percent = if penalty > 0 { (margin * 100) / penalty } else { 0 };

				// Ensure there's meaningful margin (at least 50% of penalty remains for IF)
				assert!(
					margin_percent >= 50,
					"At least 50% of penalty should remain for Insurance Fund after keeper. \
					 Got {}% for principal {} pUSD",
					margin_percent,
					principal / PUSD
				);
			}
		});
	}

	/// **Mathematical invariant test: Penalty vs keeper incentive at minimum vault**
	///
	/// This test documents an expected edge case: at the minimum vault size (5 pUSD),
	/// the penalty (0.65 pUSD) is LESS than the keeper incentive (~1.006 pUSD).
	/// This is expected because the flat tip (1 pUSD) dominates for tiny vaults.
	///
	/// The auction pallet MUST cap `keeper_incentive` to `penalty` to handle this case.
	#[test]
	fn penalty_exceeds_keeper_at_minimum_vault() {
		new_test_ext().execute_with(|| {
			let tip = 1 * PUSD;
			let chip = Permill::from_parts(1000); // 0.1%
			let liquidation_penalty = LiquidationPenalty::<Test>::get(); // 13%

			// Minimum principal (5 pUSD from MinimumMint)
			let principal = 5 * PUSD;
			let interest = 0; // Fresh vault, no interest

			let penalty = liquidation_penalty.mul_floor(principal);
			let tab = principal + interest + penalty;
			let keeper_incentive = tip + chip.mul_floor(tab);

			// For 5 pUSD principal with 13% penalty:
			// penalty = 0.65 pUSD
			// tab = 5 + 0 + 0.65 = 5.65 pUSD
			// keeper = 1 + 0.001 * 5.65 = 1.00565 pUSD
			//
			// At minimum vault size, penalty < keeper because tip dominates.
			// This documents that the auction pallet's capping mechanism is essential.
			assert!(
				penalty < keeper_incentive,
				"At minimum vault (5 pUSD), penalty ({}) should be less than uncapped keeper \
				 incentive ({}). This edge case requires auction pallet capping.",
				penalty,
				keeper_incentive
			);

			// Verify the penalty is indeed less than the tip
			assert!(
				penalty < tip,
				"For tiny vaults, penalty ({}) should be less than tip ({})",
				penalty,
				tip
			);
		});
	}

	/// **Test: Minimum principal where penalty exceeds keeper incentive**
	///
	/// Math test to find the crossover point where liquidation penalty exceeds keeper
	/// incentive. Documents the relationship between MinimumMint and this crossover.
	///
	/// Vaults below ~8 pUSD principal have penalty < keeper_incentive, meaning they
	/// rely on the auction pallet's capping mechanism. Since MinimumMint (5 pUSD) is
	/// below this crossover, this test verifies that relationship is understood.
	#[test]
	fn find_minimum_principal_for_penalty_dominance() {
		new_test_ext().execute_with(|| {
			let tip = 1 * PUSD;
			let chip = Permill::from_parts(1000); // 0.1%
			let liquidation_penalty = LiquidationPenalty::<Test>::get(); // 13%

			// Solve for principal where penalty = keeper_incentive:
			// penalty = principal * 0.13
			// keeper = tip + chip * (principal + penalty)
			// keeper = tip + chip * principal * (1 + 0.13)
			// keeper = tip + chip * principal * 1.13
			//
			// Set penalty = keeper:
			// principal * 0.13 = tip + chip * principal * 1.13
			// principal * (0.13 - chip * 1.13) = tip
			// principal = tip / (0.13 - 0.001 * 1.13)
			// principal = tip / (0.13 - 0.00113)
			// principal = tip / 0.12887
			// principal ≈ 7.76 pUSD

			let mut min_principal = 0u128;
			for principal in (1..100).map(|x| x * PUSD) {
				let penalty = liquidation_penalty.mul_floor(principal);
				let tab = principal + penalty; // No interest for simplicity
				let keeper = tip + chip.mul_floor(tab);

				if penalty > keeper {
					min_principal = principal;
					break;
				}
			}

			// The minimum principal should be around 8 pUSD
			// (above our MinimumMint of 5 pUSD, so small vaults rely on capping)
			assert!(
				min_principal > 0 && min_principal <= 10 * PUSD,
				"Expected minimum principal around 8 pUSD, got {} pUSD",
				min_principal / PUSD
			);

			// Verify that MinimumMint is below the crossover point - documenting the
			// dependency on auction pallet capping for small vaults
			let min_mint = 5 * PUSD;
			assert!(
				min_mint < min_principal,
				"MinimumMint ({} pUSD) should be below crossover ({} pUSD), \
				 meaning small vaults rely on auction pallet capping",
				min_mint / PUSD,
				min_principal / PUSD
			);
		});
	}
}
