//! Implementations of the pUSD primitive trait surfaces:
//! `VaultLiquidationInterface`, `VaultRedemptionInterface`,
//! `VaultBadDebtInterface`.
//!
//! See `troves.md` §10.3, §10.4, §10.5.
//!
//! **FinalRecovery pricing (`troves.md` §7.6) is orchestrator-owned.** When
//! the orchestrator calls `apply_redemption` against a `FinalRecovery` vault
//! it is expected to have already applied the recovery-bonus / insurance-
//! adjusted recovery-rate rules and pass the resulting `RedemptionAllocation`.
//! The vault pallet treats FinalRecovery and ordinary vaults uniformly inside
//! `apply_redemption` — only the redemption-order priority and stake exclusion
//! differ.

use crate::{
	helpers, math,
	pallet::{
		BalanceOf, BranchConfigs, BranchRedistStates, BranchStates, Config, Error, Event,
		HoldReason, MomentOf, Pallet, StableCreditOf, VaultRedistSnapshots, Vaults,
	},
	recovery,
	types::VaultStatus,
};
use frame::deps::{
	frame_support::traits::{
		fungible::Balanced as FungibleBalanced,
		fungibles::{InspectHold as FungiblesInspectHold, MutateHold as FungiblesMutateHold},
		tokens::{Fortitude, Imbalance, Precision, Restriction},
		SameOrOther, Time,
	},
	sp_runtime::{
		traits::{AtLeast32Bit, CheckedDiv, SaturatedConversion, Saturating, Zero},
		DispatchError, DispatchResult, FixedPointNumber, FixedU128,
	},
};
use pallet_linked_list::SortedListInterface;
use pusd_primitives::{
	LiquidationAllocation, RedemptionAllocation, VaultBadDebtInterface, VaultLiquidationInterface,
	VaultRedemptionInterface,
};

impl<T: Config> VaultLiquidationInterface<T::AccountId, T::AssetId, BalanceOf<T>> for Pallet<T>
where
	MomentOf<T>: AtLeast32Bit + Copy,
{
	fn prepare_liquidation(
		collateral_id: T::AssetId,
		owner: T::AccountId,
	) -> Result<BalanceOf<T>, DispatchError> {
		let bs = BranchStates::<T>::get(collateral_id).ok_or(Error::<T>::UnknownCollateral)?;
		if bs.frozen.is_some() {
			return Err(Error::<T>::BranchFrozen.into());
		}
		let now = T::TimeProvider::now();
		helpers::update_aggregate_interest::<T>(collateral_id, now)?;
		helpers::touch_vault::<T>(collateral_id, &owner, now)?;
		let vault = Vaults::<T>::get(collateral_id, &owner).ok_or(Error::<T>::VaultNotFound)?;
		if matches!(vault.status, VaultStatus::FinalRecovery) {
			return Err(Error::<T>::VaultInFinalRecovery.into());
		}
		// Last-vault guard: liquidation cannot redistribute to itself.
		let bs = BranchStates::<T>::get(collateral_id).ok_or(Error::<T>::UnknownCollateral)?;
		if bs.total_stakes == vault.stake {
			return Err(Error::<T>::LastVaultCannotBeLiquidated.into());
		}
		// Remove from rate index, subtract from branch aggregates.
		let _ = T::RateIndex::remove(&collateral_id, &owner);
		let post_touch_debt = vault.interest_bearing_debt.saturating_add(vault.accrued_interest);
		BranchStates::<T>::try_mutate(collateral_id, |maybe| -> Result<_, DispatchError> {
			let bs = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
			bs.total_interest_bearing_debt =
				bs.total_interest_bearing_debt.saturating_sub(vault.interest_bearing_debt);
			bs.total_minted_aggregate_interest =
				bs.total_minted_aggregate_interest.saturating_sub(vault.accrued_interest);
			bs.weighted_interest_bearing_debt_sum = bs
				.weighted_interest_bearing_debt_sum
				.saturating_sub(vault.annual_rate.saturating_mul_int(vault.interest_bearing_debt));
			bs.weighted_stake_sum = bs
				.weighted_stake_sum
				.saturating_sub(vault.annual_rate.saturating_mul_int(vault.stake));
			bs.total_stakes = bs.total_stakes.saturating_sub(vault.stake);
			Ok(())
		})?;

		Ok(post_touch_debt)
	}

	fn finalize_liquidation(
		collateral_id: T::AssetId,
		owner: T::AccountId,
		allocation: LiquidationAllocation<T::AccountId, BalanceOf<T>>,
	) -> DispatchResult {
		let vault = Vaults::<T>::get(collateral_id, &owner).ok_or(Error::<T>::VaultNotFound)?;
		let post_touch_debt = vault.interest_bearing_debt.saturating_add(vault.accrued_interest);
		let held = T::CollateralAssets::balance_on_hold(
			collateral_id,
			&HoldReason::VaultCollateral.into(),
			&owner,
		);
		// Conservation checks.
		if allocation.offset.debt > post_touch_debt {
			return Err(Error::<T>::InvalidLiquidationAllocation.into());
		}
		let total_paid_out = allocation
			.offset
			.collateral
			.saturating_add(allocation.redistribution_collateral)
			.saturating_add(allocation.keeper.collateral);
		if total_paid_out > held {
			return Err(Error::<T>::InvalidLiquidationAllocation.into());
		}

		let redistributed_debt = post_touch_debt.saturating_sub(allocation.offset.debt);

		// Advance redistribution accumulators.
		if !redistributed_debt.is_zero() || !allocation.redistribution_collateral.is_zero() {
			BranchStates::<T>::try_mutate(collateral_id, |maybe_bs| -> Result<_, DispatchError> {
				let bs = maybe_bs.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
				// Average recipient rate: weighted_stake_sum / total_stakes.
				// This is the rate at which redistributed principal enters the
				// branch's interest base. Per-vault reconciliation in
				// `touch_vault` later replaces this avg-rate share with the
				// recipient's own rate.
				let avg_rate =
					math::average_branch_rate(bs.weighted_stake_sum, bs.total_stakes);
				BranchRedistStates::<T>::try_mutate(
					collateral_id,
					|maybe_redist| -> Result<_, DispatchError> {
						let redist = maybe_redist.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
						let stakes_fp = FixedU128::saturating_from_integer(bs.total_stakes);
						if !stakes_fp.is_zero() {
							let debt_fp = FixedU128::saturating_from_integer(redistributed_debt);
							let coll_fp = FixedU128::saturating_from_integer(
								allocation.redistribution_collateral,
							);
							let now_fp = FixedU128::saturating_from_integer(
								T::TimeProvider::now().saturated_into::<u128>(),
							);
							// `stakes_fp != 0` is asserted above; `checked_div` only returns
							// `None` on overflow, in which case we treat the per-stake
							// increment as zero (defensive — accumulator is monotonically
							// growing, an overflowed division means the input was already
							// numerically saturated).
							let debt_per_stake =
								debt_fp.checked_div(&stakes_fp).unwrap_or_else(FixedU128::zero);
							let coll_per_stake =
								coll_fp.checked_div(&stakes_fp).unwrap_or_else(FixedU128::zero);
							redist.cumulative_redist_debt_per_stake = redist
								.cumulative_redist_debt_per_stake
								.saturating_add(debt_per_stake);
							redist.cumulative_redist_collat_per_stake = redist
								.cumulative_redist_collat_per_stake
								.saturating_add(coll_per_stake);
							redist.cumulative_redist_debt_time_per_stake = redist
								.cumulative_redist_debt_time_per_stake
								.saturating_add(now_fp.saturating_mul(debt_per_stake));
							// Per-stake share of the avg-rate weighted contribution
							// folded into the branch interest base. On touch, each
							// recipient subtracts `stake * delta` from
							// `weighted_interest_bearing_debt_sum` and adds back
							// `applied_share * vault.annual_rate`.
							redist.cumulative_redist_weight_per_stake = redist
								.cumulative_redist_weight_per_stake
								.saturating_add(debt_per_stake.saturating_mul(avg_rate));
						}
						Ok(())
					},
				)?;
				bs.pending_redistribution_debt =
					bs.pending_redistribution_debt.saturating_add(redistributed_debt);
				bs.redist_epoch = bs.redist_epoch.saturating_add(1);
				// Fold the redistributed principal into the branch interest base at
				// the average recipient rate. Per-vault reconciliation in
				// `touch_vault` replaces each recipient's share with its own rate.
				bs.weighted_interest_bearing_debt_sum = bs
					.weighted_interest_bearing_debt_sum
					.saturating_add(avg_rate.saturating_mul_int(redistributed_debt));
				Ok(())
			})?;
		}

		// 2. Move redistribution_collateral on hold from owner to redistribution account.
		if !allocation.redistribution_collateral.is_zero() {
			let _ = T::CollateralAssets::transfer_on_hold(
				collateral_id,
				&HoldReason::VaultCollateral.into(),
				&owner,
				&Pallet::<T>::redistribution_account(),
				allocation.redistribution_collateral,
				Precision::Exact,
				Restriction::OnHold,
				Fortitude::Polite,
			)?;
		}

		// 3. Move offset.collateral to the offset recipient. The vault pallet
		// owns this transfer so a buggy or stale orchestrator can't accidentally
		// leak liquidated collateral back to the liquidatee as surplus.
		if !allocation.offset.collateral.is_zero() {
			let _ = T::CollateralAssets::transfer_on_hold(
				collateral_id,
				&HoldReason::VaultCollateral.into(),
				&owner,
				&allocation.offset.recipient,
				allocation.offset.collateral,
				Precision::Exact,
				Restriction::Free,
				Fortitude::Polite,
			)?;
		}

		// 4. Pay keeper.
		if !allocation.keeper.collateral.is_zero() {
			let _ = T::CollateralAssets::transfer_on_hold(
				collateral_id,
				&HoldReason::VaultCollateral.into(),
				&owner,
				&allocation.keeper.recipient,
				allocation.keeper.collateral,
				Precision::Exact,
				Restriction::Free,
				Fortitude::Polite,
			)?;
		}

		// 5. Surplus to owner (whatever is still on hold after offset/redist/keeper outflows).
		let after_outflow = T::CollateralAssets::balance_on_hold(
			collateral_id,
			&HoldReason::VaultCollateral.into(),
			&owner,
		);
		if !after_outflow.is_zero() {
			T::CollateralAssets::release(
				collateral_id,
				&HoldReason::VaultCollateral.into(),
				&owner,
				after_outflow,
				Precision::Exact,
			)?;
		}

		// 6. Update branch collateral aggregate.
		BranchStates::<T>::mutate(collateral_id, |maybe| {
			if let Some(bs) = maybe {
				bs.total_collateral = bs
					.total_collateral
					.saturating_sub(total_paid_out)
					.saturating_sub(after_outflow);
			}
		});

		// 7. Remove vault row + redist snapshot.
		Vaults::<T>::remove(collateral_id, &owner);
		VaultRedistSnapshots::<T>::remove(collateral_id, &owner);
		Ok(())
	}
}

impl<T: Config> VaultRedemptionInterface<T::AccountId, T::AssetId, BalanceOf<T>> for Pallet<T>
where
	MomentOf<T>: AtLeast32Bit + Copy,
{
	fn next_redemption_target(
		collateral_id: T::AssetId,
		_cursor: Option<T::AccountId>,
	) -> Option<T::AccountId> {
		// 1. FinalRecovery FIFO head.
		if let Some(o) = recovery::next_target::<T>(&collateral_id) {
			return Some(o);
		}
		// 2. last_dormant_vault_owner.
		if let Some(bs) = BranchStates::<T>::get(collateral_id) {
			if let Some(o) = bs.last_dormant_vault_owner {
				return Some(o);
			}
		}
		// 3. Tail of rate index.
		T::RateIndex::tail(&collateral_id)
	}

	fn touch_for_redemption(
		collateral_id: T::AssetId,
		owner: T::AccountId,
	) -> Result<BalanceOf<T>, DispatchError> {
		let bs = BranchStates::<T>::get(collateral_id).ok_or(Error::<T>::UnknownCollateral)?;
		if bs.frozen.is_some() {
			return Err(Error::<T>::BranchFrozen.into());
		}
		let now = T::TimeProvider::now();
		helpers::update_aggregate_interest::<T>(collateral_id, now)?;
		helpers::touch_vault::<T>(collateral_id, &owner, now)?;
		let vault = Vaults::<T>::get(collateral_id, &owner).ok_or(Error::<T>::VaultNotFound)?;
		Ok(vault.interest_bearing_debt.saturating_add(vault.accrued_interest))
	}

	fn apply_redemption(
		collateral_id: T::AssetId,
		owner: T::AccountId,
		redeemer: T::AccountId,
		allocation: RedemptionAllocation<BalanceOf<T>>,
	) -> DispatchResult {
		let mut vault = Vaults::<T>::get(collateral_id, &owner).ok_or(Error::<T>::VaultNotFound)?;
		let post_touch_debt = vault.interest_bearing_debt.saturating_add(vault.accrued_interest);
		let held = T::CollateralAssets::balance_on_hold(
			collateral_id,
			&HoldReason::VaultCollateral.into(),
			&owner,
		);
		if allocation.debt_to_cancel > post_touch_debt {
			return Err(Error::<T>::InvalidRedemptionAllocation.into());
		}
		if allocation
			.collateral_to_redeemer
			.saturating_add(allocation.fee_collateral_retained) >
			held
		{
			return Err(Error::<T>::InvalidRedemptionAllocation.into());
		}

		// Apply debt cancellation: accrued interest first, then principal.
		let mut remaining = allocation.debt_to_cancel;
		let pay_accrued = core::cmp::min(remaining, vault.accrued_interest);
		vault.accrued_interest = vault.accrued_interest.saturating_sub(pay_accrued);
		remaining = remaining.saturating_sub(pay_accrued);
		let pay_principal = core::cmp::min(remaining, vault.interest_bearing_debt);
		vault.interest_bearing_debt = vault.interest_bearing_debt.saturating_sub(pay_principal);
		remaining = remaining.saturating_sub(pay_principal);
		if !remaining.is_zero() {
			return Err(Error::<T>::InvalidRedemptionAllocation.into());
		}

		// Move collateral.
		if !allocation.collateral_to_redeemer.is_zero() {
			T::CollateralAssets::transfer_on_hold(
				collateral_id,
				&HoldReason::VaultCollateral.into(),
				&owner,
				&redeemer,
				allocation.collateral_to_redeemer,
				Precision::Exact,
				Restriction::Free,
				Fortitude::Polite,
			)?;
		}

		// Branch aggregates.
		let cfg = BranchConfigs::<T>::get(collateral_id).ok_or(Error::<T>::UnknownCollateral)?;
		BranchStates::<T>::try_mutate(collateral_id, |maybe| -> Result<_, DispatchError> {
			let bs = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
			bs.total_interest_bearing_debt =
				bs.total_interest_bearing_debt.saturating_sub(pay_principal);
			bs.total_minted_aggregate_interest =
				bs.total_minted_aggregate_interest.saturating_sub(pay_accrued);
			bs.weighted_interest_bearing_debt_sum = bs
				.weighted_interest_bearing_debt_sum
				.saturating_sub(vault.annual_rate.saturating_mul_int(pay_principal));
			bs.total_collateral =
				bs.total_collateral.saturating_sub(allocation.collateral_to_redeemer);
			Ok(())
		})?;

		// Internal status transition.
		let new_total = vault.interest_bearing_debt.saturating_add(vault.accrued_interest);
		let was_active = matches!(vault.status, VaultStatus::Active);
		let is_recovery = matches!(vault.status, VaultStatus::FinalRecovery);
		if !is_recovery {
			if new_total.is_zero() {
				// Fully redeemed: leave Dormant with whatever residual coll.
				vault.status = VaultStatus::Dormant;
				if was_active {
					let _ = T::RateIndex::remove(&collateral_id, &owner);
				}
				BranchStates::<T>::mutate(collateral_id, |maybe| {
					if let Some(bs) = maybe {
						if bs.last_dormant_vault_owner.as_ref() == Some(&owner) {
							bs.last_dormant_vault_owner = None;
						}
					}
				});
			} else if new_total < cfg.minimum_debt {
				// Dust dormant: track in continuation pointer.
				vault.status = VaultStatus::Dormant;
				if was_active {
					let _ = T::RateIndex::remove(&collateral_id, &owner);
				}
				BranchStates::<T>::mutate(collateral_id, |maybe| {
					if let Some(bs) = maybe {
						bs.last_dormant_vault_owner = Some(owner.clone());
					}
				});
			}
		} else if new_total.is_zero() {
			// FinalRecovery vault fully settled: drop FIFO node and mark dormant.
			let _ = recovery::remove::<T>(&collateral_id, &owner);
			vault.status = VaultStatus::Dormant;
		}

		let vault_rate = vault.annual_rate;
		Vaults::<T>::insert(collateral_id, &owner, &vault);

		Pallet::<T>::deposit_event(Event::VaultRedeemed {
			collateral_id,
			owner,
			redeemer,
			debt_cancelled: allocation.debt_to_cancel,
			collateral_to_redeemer: allocation.collateral_to_redeemer,
			fee_collateral_retained: allocation.fee_collateral_retained,
			vault_annual_rate: vault_rate,
		});
		Ok(())
	}
}

impl<T: Config> VaultBadDebtInterface<T::AssetId, StableCreditOf<T>> for Pallet<T> {
	fn heal(collateral_id: T::AssetId, credit: StableCreditOf<T>) -> DispatchResult {
		let amount = credit.peek();
		if amount.is_zero() {
			drop(credit);
			return Ok(());
		}
		// Rescind matching pUSD to net the imbalance to zero.
		let debt = T::StableAsset::rescind(amount);
		// `offset` returns `SameOrOther<credit-side, debt-side>`. With
		// matching peeks the result is `None`, which is perfect netting.
		match credit.offset(debt) {
			SameOrOther::None => {},
			SameOrOther::Same(remaining_credit) => {
				// Defensive: `peek == amount` rescind should fully net.
				drop(remaining_credit);
				return Err(Error::<T>::ArithmeticOverflow.into());
			},
			SameOrOther::Other(remaining_debt) => {
				drop(remaining_debt);
				return Err(Error::<T>::ArithmeticOverflow.into());
			},
		}
		BranchStates::<T>::try_mutate(collateral_id, |maybe| -> Result<_, DispatchError> {
			let bs = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
			bs.bad_debt = bs.bad_debt.saturating_sub(amount);
			Ok(())
		})?;
		Pallet::<T>::deposit_event(Event::BadDebtHealed { collateral_id, amount });
		Ok(())
	}
}
