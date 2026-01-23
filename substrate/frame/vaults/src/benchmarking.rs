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

//! Benchmarking setup for pallet-vaults

use super::*;
use crate::Pallet as Vaults;
use frame_benchmarking::v2::*;
use frame_support::{
	traits::{Get, Hooks},
	weights::Weight,
};
use frame_system::RawOrigin;
use pallet::BalanceOf;
use sp_runtime::{FixedU128, Permill, SaturatedConversion, Saturating};

/// Minimum deposit amount for vault creation (must be >= T::MinimumDeposit)
fn minimum_deposit<T: Config>() -> BalanceOf<T> {
	T::MinimumDeposit::get()
}

/// A larger deposit for scenarios requiring extra collateral headroom
fn large_deposit<T: Config>() -> BalanceOf<T> {
	// 10x minimum to allow minting and withdrawals
	T::MinimumDeposit::get().saturating_mul(10u32.into())
}

/// Safe mint amount that maintains ICR with large_deposit
/// With 200% ICR and $4.21/DOT price:
/// large_deposit (1000 DOT) = $4210 collateral value
/// Max safe mint = $4210 / 2.0 ≈ $2105 pUSD
/// We use a conservative $2000 pUSD (2_000_000_000 with 6 decimals)
fn safe_mint_amount<T: Config>() -> BalanceOf<T> {
	// 2000 pUSD with 6 decimals = 2_000_000_000
	2_000_000_000u128.try_into().unwrap_or_else(|_| 1u32.into())
}

/// Fund an account with native currency (DOT)
fn fund_account<T: Config>(account: &T::AccountId, amount: BalanceOf<T>) {
	T::BenchmarkHelper::fund_account(account, amount);
}

/// Ensure the InsuranceFund account can receive funds
fn ensure_insurance_fund<T: Config>() {
	let insurance_fund = T::InsuranceFund::get();
	if !frame_system::Pallet::<T>::account_exists(&insurance_fund) {
		frame_system::Pallet::<T>::inc_providers(&insurance_fund);
	}
	fund_account::<T>(&insurance_fund, T::MinimumDeposit::get());
}

/// Ensure the stablecoin asset exists
fn ensure_stablecoin_asset<T: Config>() {
	T::BenchmarkHelper::create_stablecoin_asset(T::StablecoinAssetId::get());
}

/// Set up the system for minting by ensuring MaximumIssuance is set high enough
fn ensure_can_mint<T: Config>(amount: BalanceOf<T>) {
	use frame_support::traits::fungibles::Inspect;
	let current_issuance = T::Asset::total_issuance(T::StablecoinAssetId::get());
	let required = current_issuance.saturating_add(amount).saturating_mul(2u32.into());
	let current_max = crate::MaximumIssuance::<T>::get();
	if current_max < required {
		crate::MaximumIssuance::<T>::put(required);
	}
}

/// Ensure MaxPositionAmount is set high enough for the given mint amount
fn ensure_max_position_amount<T: Config>(amount: BalanceOf<T>) {
	let required = amount.saturating_mul(2u32.into());
	let current_max = crate::MaxPositionAmount::<T>::get();
	if current_max < required {
		crate::MaxPositionAmount::<T>::put(required);
	}
}

/// Mint pUSD to an account (for repay scenarios)
fn mint_pusd_to<T: Config>(
	account: &T::AccountId,
	amount: BalanceOf<T>,
) -> Result<(), BenchmarkError> {
	ensure_stablecoin_asset::<T>();
	T::BenchmarkHelper::mint_stablecoin_to(T::StablecoinAssetId::get(), account, amount);
	Ok(())
}

/// Create a vault with collateral for the given account
fn create_vault_for<T: Config>(
	owner: &T::AccountId,
	deposit: BalanceOf<T>,
) -> Result<(), BenchmarkError> {
	fund_account::<T>(owner, deposit.saturating_mul(2u32.into()));
	Vaults::<T>::create_vault(RawOrigin::Signed(owner.clone()).into(), deposit)
		.map_err(|_| BenchmarkError::Stop("Failed to create vault"))?;
	Ok(())
}

/// Create a vault with debt for the given account
fn create_vault_with_debt<T: Config>(owner: &T::AccountId) -> Result<BalanceOf<T>, BenchmarkError> {
	let deposit = large_deposit::<T>();
	let mint_amount = safe_mint_amount::<T>();

	ensure_stablecoin_asset::<T>();
	ensure_can_mint::<T>(mint_amount);
	ensure_max_position_amount::<T>(mint_amount);

	create_vault_for::<T>(owner, deposit)?;

	Vaults::<T>::mint(RawOrigin::Signed(owner.clone()).into(), mint_amount).map_err(|e| {
		log::error!(
			target: "runtime::vaults::benchmark",
			"Failed to mint in vault: {:?}",
			e
		);
		BenchmarkError::Stop("Failed to mint in vault")
	})?;

	Ok(mint_amount)
}

/// Advance timestamp to trigger fee accrual.
///
/// For worst-case benchmarking, we advance by `StaleVaultThreshold` milliseconds.
/// This represents the realistic maximum time a vault could go without
/// fee updates, since `on_idle` processes vaults that exceed this threshold.
fn advance_to_stale_threshold<T: Config>() {
	let stale_threshold: u64 = T::StaleVaultThreshold::get().saturated_into();
	T::BenchmarkHelper::advance_time(stale_threshold + 1);
}

#[benchmarks]
mod benchmarks {
	use super::*;

	// ============================================
	// User Operations
	// ============================================

	/// Benchmark: create_vault
	/// Creates a new vault with initial collateral deposit.
	#[benchmark]
	fn create_vault() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let deposit = minimum_deposit::<T>();

		// Fund account with enough balance
		fund_account::<T>(&caller, deposit.saturating_mul(2u32.into()));

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), deposit);

		// Verify vault was created
		assert!(crate::Vaults::<T>::contains_key(&caller));
		Ok(())
	}

	/// Benchmark: deposit_collateral
	/// Deposits additional collateral into an existing vault.
	/// Worst case: vault at StaleVaultThreshold blocks since last update,
	/// triggering maximum realistic fee accrual computation.
	#[benchmark]
	fn deposit_collateral() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();

		// Create vault with debt so fee accrual has work to do
		create_vault_with_debt::<T>(&caller)?;

		// Advance to stale threshold (worst case - just before on_idle would process)
		advance_to_stale_threshold::<T>();

		// Fund additional collateral
		let additional = minimum_deposit::<T>();
		fund_account::<T>(&caller, additional.saturating_mul(2u32.into()));

		let collateral_before = crate::Vaults::<T>::get(&caller)
			.ok_or(BenchmarkError::Stop("Vault not found"))?
			.get_held_collateral(&caller);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), additional);

		// Verify collateral increased
		let collateral_after = crate::Vaults::<T>::get(&caller)
			.ok_or(BenchmarkError::Stop("Vault not found"))?
			.get_held_collateral(&caller);
		assert!(collateral_after > collateral_before);
		Ok(())
	}

	/// Benchmark: withdraw_collateral
	/// Withdraws collateral from a vault.
	/// Worst case: vault at StaleVaultThreshold, requires CR check after fee update.
	#[benchmark]
	fn withdraw_collateral() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();

		// Create vault with excess collateral (no debt for simpler withdrawal)
		let deposit = large_deposit::<T>();
		create_vault_for::<T>(&caller, deposit)?;

		// Advance to stale threshold (worst case)
		advance_to_stale_threshold::<T>();

		// Withdraw a small amount (must remain above minimum)
		let withdraw_amount = minimum_deposit::<T>();

		let collateral_before = crate::Vaults::<T>::get(&caller)
			.ok_or(BenchmarkError::Stop("Vault not found"))?
			.get_held_collateral(&caller);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), withdraw_amount);

		// Verify collateral decreased
		let collateral_after = crate::Vaults::<T>::get(&caller)
			.ok_or(BenchmarkError::Stop("Vault not found"))?
			.get_held_collateral(&caller);
		assert!(collateral_after < collateral_before);
		Ok(())
	}

	/// Benchmark: mint
	/// Mints stablecoin against vault collateral.
	/// Worst case: vault at StaleVaultThreshold, CR validation, max debt check.
	#[benchmark]
	fn mint() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();

		// Create vault with collateral
		let deposit = large_deposit::<T>();
		create_vault_for::<T>(&caller, deposit)?;

		// Advance to stale threshold (worst case)
		advance_to_stale_threshold::<T>();

		// Mint a safe amount - ensure stablecoin asset exists and we can mint
		let mint_amount = safe_mint_amount::<T>();
		ensure_stablecoin_asset::<T>();
		ensure_can_mint::<T>(mint_amount);
		ensure_max_position_amount::<T>(mint_amount);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), mint_amount);

		// Verify debt increased
		let vault =
			crate::Vaults::<T>::get(&caller).ok_or(BenchmarkError::Stop("Vault not found"))?;
		assert_eq!(vault.principal, mint_amount);
		Ok(())
	}

	/// Benchmark: repay
	/// Repays stablecoin debt.
	/// Worst case: vault at StaleVaultThreshold with accrued interest,
	/// interest payment to InsuranceFund, pUSD burn.
	#[benchmark]
	fn repay() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		ensure_insurance_fund::<T>();

		// Create vault with debt
		let debt = create_vault_with_debt::<T>(&caller)?;

		// Advance to stale threshold to accrue interest (worst case)
		advance_to_stale_threshold::<T>();

		// Caller already has pUSD from minting, repay half
		let repay_amount = debt / 2u32.into();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), repay_amount);

		// Verify debt decreased
		let vault =
			crate::Vaults::<T>::get(&caller).ok_or(BenchmarkError::Stop("Vault not found"))?;
		assert!(vault.principal < debt);
		Ok(())
	}

	/// Benchmark: liquidate_vault
	/// Liquidates an undercollateralized vault.
	/// Worst case: vault at StaleVaultThreshold with accrued fees,
	/// penalty calculation, auction start.
	#[benchmark]
	fn liquidate_vault() -> Result<(), BenchmarkError> {
		// Create a vault owner (victim) and a liquidator (keeper)
		let vault_owner: T::AccountId = account("vault_owner", 0, 0);
		let keeper: T::AccountId = whitelisted_caller();

		// Create vault with debt at normal price
		create_vault_with_debt::<T>(&vault_owner)?;

		// Advance to stale threshold (worst case)
		advance_to_stale_threshold::<T>();

		// Crash the price to make vault undercollateralized
		// With large_deposit (1000 DOLLARS) and safe_mint_amount (2000 pUSD):
		// - collateral_value = large_deposit * price / 10^18
		// - For CR < 180% (MCR): collateral_value < 2000 * 1.8 = 3600 pUSD
		// - Need: large_deposit * price / 10^18 < 3600 * 10^6
		// - With large_deposit = 10^17: price < 36 * 10^9
		// Set price to 1_000_000_000 (very low) to ensure CR << MCR
		T::BenchmarkHelper::set_price(FixedU128::from_inner(1_000_000_000));

		#[extrinsic_call]
		_(RawOrigin::Signed(keeper), vault_owner.clone());

		// Verify vault is in liquidation
		let vault =
			crate::Vaults::<T>::get(&vault_owner).ok_or(BenchmarkError::Stop("Vault not found"))?;
		assert_eq!(vault.status, crate::VaultStatus::InLiquidation);

		// Reset price for other tests (normalized $4.21)
		T::BenchmarkHelper::set_price(FixedU128::from_inner(421_000_000_000_000));

		Ok(())
	}

	/// Benchmark: close_vault
	/// Closes a debt-free vault and returns all collateral.
	/// Worst case: vault at StaleVaultThreshold, fee path traversal, collateral release.
	#[benchmark]
	fn close_vault() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		ensure_insurance_fund::<T>();

		// Create vault without debt (just collateral)
		let deposit = large_deposit::<T>();
		create_vault_for::<T>(&caller, deposit)?;

		// Advance to stale threshold (worst case - tests fee path even without debt)
		advance_to_stale_threshold::<T>();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()));

		// Verify vault was removed
		assert!(!crate::Vaults::<T>::contains_key(&caller));
		Ok(())
	}

	/// Benchmark: heal
	/// Repays bad debt by burning pUSD from InsuranceFund.
	#[benchmark]
	fn heal() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		ensure_insurance_fund::<T>();

		// Set up bad debt
		let bad_debt_amount: BalanceOf<T> = safe_mint_amount::<T>();
		crate::BadDebt::<T>::put(bad_debt_amount);

		// Mint pUSD to InsuranceFund so it can be burned
		mint_pusd_to::<T>(&T::InsuranceFund::get(), bad_debt_amount)?;

		let heal_amount = bad_debt_amount / 2u32.into();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), heal_amount);

		// Verify bad debt reduced
		let remaining_bad_debt = crate::BadDebt::<T>::get();
		assert!(remaining_bad_debt < bad_debt_amount);
		Ok(())
	}

	/// Benchmark: poke
	/// Forces fee accrual on any vault.
	/// Worst case: vault at StaleVaultThreshold, maximum realistic fee calculation.
	#[benchmark]
	fn poke() -> Result<(), BenchmarkError> {
		let vault_owner: T::AccountId = account("vault_owner", 0, 0);
		let caller: T::AccountId = whitelisted_caller();

		// Create vault with debt (so fee accrual has work to do)
		create_vault_with_debt::<T>(&vault_owner)?;

		// Advance to stale threshold (worst case - just before on_idle would process)
		advance_to_stale_threshold::<T>();

		let vault_before =
			crate::Vaults::<T>::get(&vault_owner).ok_or(BenchmarkError::Stop("Vault not found"))?;
		let last_update_before = vault_before.last_fee_update;

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), vault_owner.clone());

		// Verify last_fee_update was updated
		let vault_after =
			crate::Vaults::<T>::get(&vault_owner).ok_or(BenchmarkError::Stop("Vault not found"))?;
		assert!(vault_after.last_fee_update > last_update_before);
		Ok(())
	}

	// ============================================
	// Governance Operations (Root origin)
	// ============================================

	/// Benchmark: set_minimum_collateralization_ratio
	#[benchmark]
	fn set_minimum_collateralization_ratio() -> Result<(), BenchmarkError> {
		let new_ratio = FixedU128::from_rational(140, 100); // 140%

		#[extrinsic_call]
		_(RawOrigin::Root, new_ratio);

		// Verify ratio was updated
		assert_eq!(crate::MinimumCollateralizationRatio::<T>::get(), new_ratio);
		Ok(())
	}

	/// Benchmark: set_initial_collateralization_ratio
	#[benchmark]
	fn set_initial_collateralization_ratio() -> Result<(), BenchmarkError> {
		let new_ratio = FixedU128::from_rational(220, 100); // 220%

		#[extrinsic_call]
		_(RawOrigin::Root, new_ratio);

		// Verify ratio was updated
		assert_eq!(crate::InitialCollateralizationRatio::<T>::get(), new_ratio);
		Ok(())
	}

	/// Benchmark: set_stability_fee
	#[benchmark]
	fn set_stability_fee() -> Result<(), BenchmarkError> {
		let new_fee = Permill::from_percent(5); // 5%

		#[extrinsic_call]
		_(RawOrigin::Root, new_fee);

		// Verify fee was updated
		assert_eq!(crate::StabilityFee::<T>::get(), new_fee);
		Ok(())
	}

	/// Benchmark: set_liquidation_penalty
	#[benchmark]
	fn set_liquidation_penalty() -> Result<(), BenchmarkError> {
		let new_penalty = Permill::from_percent(15); // 15%

		#[extrinsic_call]
		_(RawOrigin::Root, new_penalty);

		// Verify penalty was updated
		assert_eq!(crate::LiquidationPenalty::<T>::get(), new_penalty);
		Ok(())
	}

	/// Benchmark: set_max_liquidation_amount
	#[benchmark]
	fn set_max_liquidation_amount() -> Result<(), BenchmarkError> {
		let new_amount: BalanceOf<T> = safe_mint_amount::<T>().saturating_mul(1000u32.into());

		#[extrinsic_call]
		_(RawOrigin::Root, new_amount);

		// Verify amount was updated
		assert_eq!(crate::MaxLiquidationAmount::<T>::get(), new_amount);
		Ok(())
	}

	/// Benchmark: set_max_issuance
	/// Tests with Full privilege level (can raise or lower).
	#[benchmark]
	fn set_max_issuance() -> Result<(), BenchmarkError> {
		let new_amount: BalanceOf<T> = safe_mint_amount::<T>().saturating_mul(10000u32.into());

		#[extrinsic_call]
		_(RawOrigin::Root, new_amount);

		// Verify amount was updated
		assert_eq!(crate::MaximumIssuance::<T>::get(), new_amount);
		Ok(())
	}

	// ============================================
	// Hooks
	// ============================================

	/// Benchmark: on_idle processing a single stale vault with debt.
	///
	/// This measures the worst-case per-vault cost in `on_idle`:
	/// - Vault has debt (fee calculation required)
	/// - Vault is stale (at StaleVaultThreshold)
	/// - Fee accrual computation and storage write
	///
	/// The resulting weight is used by `on_idle` to determine how many
	/// vaults can be processed within the available weight budget.
	#[benchmark]
	fn on_idle_one_vault() -> Result<(), BenchmarkError> {
		let vault_owner: T::AccountId = account("vault_owner", 0, 0);

		// Create vault with debt (worst case - fee calculation required)
		create_vault_with_debt::<T>(&vault_owner)?;

		// Advance to stale threshold so vault will be processed
		advance_to_stale_threshold::<T>();

		// Clear cursor to start fresh iteration
		crate::OnIdleCursor::<T>::kill();

		let vault_before =
			crate::Vaults::<T>::get(&vault_owner).ok_or(BenchmarkError::Stop("Vault not found"))?;
		let last_update_before = vault_before.last_fee_update;

		let current_block = frame_system::Pallet::<T>::block_number();

		#[block]
		{
			// Process with unlimited weight - will process exactly one vault
			Vaults::<T>::on_idle(current_block, Weight::MAX);
		}

		// Verify vault was processed (last_fee_update timestamp changed)
		let vault_after =
			crate::Vaults::<T>::get(&vault_owner).ok_or(BenchmarkError::Stop("Vault not found"))?;
		assert!(
			vault_after.last_fee_update > last_update_before,
			"last_fee_update should be updated"
		);

		Ok(())
	}

	impl_benchmark_test_suite!(Vaults, crate::mock::new_test_ext(), crate::mock::Test);
}
