//! Implementations of the pUSD primitive trait surfaces:
//! `VaultLiquidationInterface`, `VaultRedemptionInterface`,
//! `VaultBadDebtInterface`.
//!
//!
//! **FinalRecovery pricing is orchestrator-owned.** When the orchestrator
//! calls `apply_redemption` against a `FinalRecovery` vault it is expected to
//! have already applied the recovery-bonus / insurance-adjusted recovery-rate
//! rules and pass the resulting `RedemptionAllocation`. The vault pallet
//! treats FinalRecovery and ordinary vaults uniformly inside `apply_redemption`
//! — only the redemption-order priority and stake exclusion differ.

use crate::{
	helpers, math,
	pallet::{
		BalanceOf, BranchStates, Config, Error, Event, HoldReason, Pallet, StableCreditOf, Vaults,
	},
	recovery,
	types::{VaultListId, VaultStatus},
};
use frame::deps::{
	frame_support::{
		ensure,
		traits::{
			fungible::Balanced as FungibleBalanced,
			fungibles::{InspectHold as FungiblesInspectHold, MutateHold as FungiblesMutateHold},
			tokens::{Fortitude, Imbalance, Precision, Restriction},
			SameOrOther, Time,
		},
		transactional,
	},
	sp_runtime::{
		traits::{Saturating, Zero},
		DispatchError, DispatchResult, FixedPointNumber, FixedU128,
	},
};
use pallet_linked_list::SortedListInterface;
use pusd_primitives::{
	LiquidationAllocation, ProvidePrice, RedemptionAllocation, VaultBadDebtInterface,
	VaultLiquidationInterface, VaultRedemptionInterface,
};

impl<T: Config> VaultLiquidationInterface<T::AccountId, T::AssetId, BalanceOf<T>> for Pallet<T> {
	#[transactional]
	fn prepare_liquidation(
		collateral_id: T::AssetId,
		owner: T::AccountId,
	) -> Result<BalanceOf<T>, DispatchError> {
		helpers::ensure_not_frozen::<T>(&collateral_id)?;
		let now = T::TimeProvider::now();
		let price = <T::Oracle as ProvidePrice>::provide_price(&collateral_id)?.price;
		helpers::update_aggregate_interest::<T>(&collateral_id, now)?;
		let vault = helpers::touch_vault::<T>(&collateral_id, &owner, now)?
			.ok_or(Error::<T>::VaultNotFound)?;
		ensure!(
			!vault.status::<T>(&collateral_id, &owner).is_final_recovery(),
			Error::<T>::VaultInFinalRecovery
		);
		let post_touch_debt = vault.debt.total();
		let cfg = helpers::branch_cfg_of::<T>(&collateral_id)?;
		// Defense-in-depth (DESIGN.md §9.1): refuse to prepare a liquidation
		// for a vault whose fully-accrued CR is still at or above MCR.
		let held = T::CollateralAssets::balance_on_hold(
			collateral_id.clone(),
			&HoldReason::VaultCollateral.into(),
			&owner,
		);
		let cr = math::collateralization_ratio::<BalanceOf<T>>(held, post_touch_debt, price)
			.ok_or(Error::<T>::UnsafeCollateralizationRatio)?;
		ensure!(cr < cfg.minimum_collateralization_ratio, Error::<T>::UnsafeCollateralizationRatio);
		BranchStates::<T>::try_mutate(&collateral_id, |maybe| -> Result<_, DispatchError> {
			let bs = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
			ensure!(
				bs.stakes.total != vault.redistribution_stake,
				Error::<T>::LastVaultCannotBeLiquidated
			);
			bs.detach_vault(&vault);
			Ok(())
		})?;
		// Dormant vaults aren't in the rate index; silently absorbing
		// `ItemNotFound` is correct here.
		let _ = T::VaultLists::remove(&VaultListId::Rate(collateral_id), &owner);

		Ok(post_touch_debt)
	}

	#[transactional]
	fn finalize_liquidation(
		collateral_id: T::AssetId,
		owner: T::AccountId,
		allocation: LiquidationAllocation<T::AccountId, BalanceOf<T>>,
	) -> DispatchResult {
		let vault = helpers::vault_of::<T>(&collateral_id, &owner)?;
		// A prepared vault was detached from the aggregates and de-listed, so
		// its derived status is `Dormant`.
		ensure!(
			vault.status::<T>(&collateral_id, &owner).is_dormant(),
			Error::<T>::LiquidationNotPrepared
		);
		let post_touch_debt = vault.debt.total();
		let held = T::CollateralAssets::balance_on_hold(
			collateral_id.clone(),
			&HoldReason::VaultCollateral.into(),
			&owner,
		);
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
		let do_redistribute =
			!redistributed_debt.is_zero() || !allocation.redistribution_collateral.is_zero();
		BranchStates::<T>::try_mutate(&collateral_id, |maybe_bs| -> Result<_, DispatchError> {
			let bs = maybe_bs.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
			// The row vanishes below; a parked dormant pointer to this owner
			// must not survive it.
			if bs.last_dormant_vault_owner.as_ref() == Some(&owner) {
				bs.last_dormant_vault_owner = None;
			}
			// The redistribution collateral stays counted in
			// branch collateral until vault touch moves it to the recipient's
			// hold, so only the SP-offset + keeper portions leave the total.
			let non_redist_out = held.saturating_sub(allocation.redistribution_collateral);
			bs.total_collateral = bs.total_collateral.saturating_sub(non_redist_out);
			if do_redistribute {
				let avg_rate = math::average_branch_rate(bs.stakes.weighted_sum, bs.stakes.total);
				let debt_per_stake = math::redist_per_stake(redistributed_debt, bs.stakes.total)
					.ok_or(Error::<T>::RedistributionWouldOverflow)?;
				let coll_per_stake =
					math::redist_per_stake(allocation.redistribution_collateral, bs.stakes.total)
						.ok_or(Error::<T>::RedistributionWouldOverflow)?;
				let weight_per_stake =
					math::redist_weight_per_stake(redistributed_debt, avg_rate, bs.stakes.total)
						.ok_or(Error::<T>::RedistributionWouldOverflow)?;
				let now_fp = FixedU128::saturating_from_integer(T::TimeProvider::now());
				bs.redist.debt_per_stake = bs.redist.debt_per_stake.saturating_add(debt_per_stake);
				bs.redist.collat_per_stake =
					bs.redist.collat_per_stake.saturating_add(coll_per_stake);
				bs.redist.debt_time_per_stake = bs
					.redist
					.debt_time_per_stake
					.saturating_add(now_fp.saturating_mul(debt_per_stake));
				bs.redist.weight_per_stake =
					bs.redist.weight_per_stake.saturating_add(weight_per_stake);
				let distributed_debt = debt_per_stake.saturating_mul_int(bs.stakes.total);
				let debt_dust = redistributed_debt.saturating_sub(distributed_debt);
				bs.debt.pending_redist_principal =
					bs.debt.pending_redist_principal.saturating_add(distributed_debt);
				bs.debt.weighted_principal_sum = bs
					.debt
					.weighted_principal_sum
					.saturating_add(avg_rate.saturating_mul_int(redistributed_debt));
				if !debt_dust.is_zero() {
					bs.add_ownerless_pusd_debt(debt_dust);
				}
				let distributed_coll = coll_per_stake.saturating_mul_int(bs.stakes.total);
				let coll_dust =
					allocation.redistribution_collateral.saturating_sub(distributed_coll);
				if !coll_dust.is_zero() {
					bs.add_ownerless_collateral_surplus(coll_dust);
				}
			}
			Ok(())
		})?;

		if !allocation.redistribution_collateral.is_zero() {
			T::CollateralAssets::transfer_on_hold(
				collateral_id.clone(),
				&HoldReason::VaultCollateral.into(),
				&owner,
				&Pallet::<T>::redistribution_account(),
				allocation.redistribution_collateral,
				Precision::Exact,
				Restriction::OnHold,
				Fortitude::Polite,
			)?;
		}

		// Vault pallet owns the offset transfer so a buggy or stale orchestrator
		// can't accidentally leak liquidated collateral back to the liquidatee as
		// surplus.
		if !allocation.offset.collateral.is_zero() {
			T::CollateralAssets::transfer_on_hold(
				collateral_id.clone(),
				&HoldReason::VaultCollateral.into(),
				&owner,
				&allocation.offset.recipient,
				allocation.offset.collateral,
				Precision::Exact,
				Restriction::Free,
				Fortitude::Polite,
			)?;
		}

		if !allocation.keeper.collateral.is_zero() {
			T::CollateralAssets::transfer_on_hold(
				collateral_id.clone(),
				&HoldReason::VaultCollateral.into(),
				&owner,
				&allocation.keeper.recipient,
				allocation.keeper.collateral,
				Precision::Exact,
				Restriction::Free,
				Fortitude::Polite,
			)?;
		}

		// Surplus to owner: any held collateral still left after the
		// offset/redist/keeper outflows is released back to the liquidatee.
		let after_outflow = T::CollateralAssets::balance_on_hold(
			collateral_id.clone(),
			&HoldReason::VaultCollateral.into(),
			&owner,
		);
		if !after_outflow.is_zero() {
			T::CollateralAssets::release(
				collateral_id.clone(),
				&HoldReason::VaultCollateral.into(),
				&owner,
				after_outflow,
				Precision::Exact,
			)?;
		}

		Vaults::<T>::remove(&collateral_id, &owner);
		Ok(())
	}
}

impl<T: Config> VaultRedemptionInterface<T::AccountId, T::AssetId, BalanceOf<T>> for Pallet<T> {
	/// Priority order: `FinalRecovery` FIFO head, then `last_dormant_vault_owner`,
	/// then the rate-index tail.
	fn next_redemption_target(
		collateral_id: T::AssetId,
		_cursor: Option<T::AccountId>,
	) -> Option<T::AccountId> {
		if let Some(o) = recovery::next_target::<T>(&collateral_id) {
			return Some(o);
		}
		if let Some(bs) = BranchStates::<T>::get(&collateral_id) {
			if let Some(o) = bs.last_dormant_vault_owner {
				return Some(o);
			}
		}
		T::VaultLists::tail(&VaultListId::Rate(collateral_id))
	}

	#[transactional]
	fn touch_for_redemption(
		collateral_id: T::AssetId,
		owner: T::AccountId,
	) -> Result<BalanceOf<T>, DispatchError> {
		helpers::ensure_not_frozen::<T>(&collateral_id)?;
		let now = T::TimeProvider::now();
		helpers::update_aggregate_interest::<T>(&collateral_id, now)?;
		let vault = helpers::touch_vault::<T>(&collateral_id, &owner, now)?
			.ok_or(Error::<T>::VaultNotFound)?;
		Ok(vault.debt.total())
	}

	#[transactional]
	fn apply_redemption(
		collateral_id: T::AssetId,
		owner: T::AccountId,
		redeemer: T::AccountId,
		allocation: RedemptionAllocation<BalanceOf<T>>,
	) -> DispatchResult {
		let mut vault = helpers::vault_of::<T>(&collateral_id, &owner)?;
		let status = vault.status::<T>(&collateral_id, &owner);
		let post_touch_debt = vault.debt.total();
		let held = T::CollateralAssets::balance_on_hold(
			collateral_id.clone(),
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

		let payment = vault.debt.cancel(allocation.debt_to_cancel);
		debug_assert_eq!(payment.total(), allocation.debt_to_cancel);

		if !allocation.collateral_to_redeemer.is_zero() {
			T::CollateralAssets::transfer_on_hold(
				collateral_id.clone(),
				&HoldReason::VaultCollateral.into(),
				&owner,
				&redeemer,
				allocation.collateral_to_redeemer,
				Precision::Exact,
				Restriction::Free,
				Fortitude::Polite,
			)?;
		}

		let cfg = helpers::branch_cfg_of::<T>(&collateral_id)?;
		let new_total = vault.debt.total();
		let stake_changes = matches!(status, VaultStatus::Active | VaultStatus::Dormant) &&
			!allocation.collateral_to_redeemer.is_zero();
		let old_stake = vault.redistribution_stake;
		let new_stake = old_stake.saturating_sub(allocation.collateral_to_redeemer);
		BranchStates::<T>::try_mutate(&collateral_id, |maybe| -> Result<_, DispatchError> {
			let bs = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
			bs.apply_debt_payment(payment, vault.annual_rate);
			bs.remove_collateral(allocation.collateral_to_redeemer);
			if stake_changes {
				bs.refresh_vault_stake(vault.annual_rate, old_stake, new_stake);
			}
			if matches!(status, VaultStatus::Active | VaultStatus::Dormant) {
				if new_total.is_zero() {
					if bs.last_dormant_vault_owner.as_ref() == Some(&owner) {
						bs.last_dormant_vault_owner = None;
					}
				} else if new_total < cfg.minimum_debt {
					bs.last_dormant_vault_owner = Some(owner.clone());
				}
			}
			Ok(())
		})?;
		if stake_changes {
			vault.redistribution_stake = new_stake;
		}

		match status {
			VaultStatus::Active if new_total < cfg.minimum_debt => {
				// Invariant: Active vaults are in the rate index, so `remove` succeeds.
				let _ = T::VaultLists::remove(&VaultListId::Rate(collateral_id.clone()), &owner);
			},
			VaultStatus::FinalRecovery if new_total.is_zero() => {
				let _ = recovery::remove::<T>(&collateral_id, &owner);
				// Vault leaves FinalRecovery and becomes Dormant. Refresh stake
				// to current held and rejoin recipient accounting so
				// `vault.redistribution_stake == held` holds across the new
				// Dormant state (per try_state invariant).
				let held_now = T::CollateralAssets::balance_on_hold(
					collateral_id.clone(),
					&HoldReason::VaultCollateral.into(),
					&owner,
				);
				BranchStates::<T>::try_mutate(
					&collateral_id,
					|maybe| -> Result<_, DispatchError> {
						let bs = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
						bs.stakes.total = bs.stakes.total.saturating_add(held_now);
						bs.stakes.weighted_sum = bs
							.stakes
							.weighted_sum
							.saturating_add(vault.annual_rate.saturating_mul_int(held_now));
						vault.redist_snapshot = bs.redist;
						Ok(())
					},
				)?;
				vault.redistribution_stake = held_now;
			},
			_ => {},
		}

		let vault_rate = vault.annual_rate;
		Vaults::<T>::insert(&collateral_id, &owner, &vault);

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

impl<T: Config> VaultBadDebtInterface<T::AssetId, BalanceOf<T>, StableCreditOf<T>> for Pallet<T> {
	#[transactional]
	fn record_bad_debt(collateral_id: T::AssetId, amount: BalanceOf<T>) -> DispatchResult {
		if amount.is_zero() {
			return Ok(());
		}
		BranchStates::<T>::try_mutate(&collateral_id, |maybe| -> Result<_, DispatchError> {
			let bs = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
			bs.debt.bad_debt = bs.debt.bad_debt.saturating_add(amount);
			Ok(())
		})?;
		Pallet::<T>::deposit_event(Event::BadDebtRecorded { collateral_id, amount });
		Ok(())
	}

	#[transactional]
	fn heal(
		collateral_id: T::AssetId,
		credit: StableCreditOf<T>,
	) -> Result<StableCreditOf<T>, DispatchError> {
		let bs = helpers::branch_state_of::<T>(&collateral_id)?;
		let healable = credit.peek().min(bs.debt.bad_debt);
		if healable.is_zero() {
			// Nothing recorded (or an empty credit) — hand everything back.
			return Ok(credit);
		}
		let (to_burn, surplus) = credit.split(healable);
		// Rescind matching pUSD to net the imbalance to zero.
		let debt = T::StableAsset::rescind(healable);
		// `offset` returns `SameOrOther<credit-side, debt-side>`. With
		// matching peeks the result is `None`, which is perfect netting.
		match to_burn.offset(debt) {
			SameOrOther::None => {},
			SameOrOther::Same(remaining_credit) => {
				// Defensive: `peek == healable` rescind should fully net.
				drop(remaining_credit);
				return Err(Error::<T>::ArithmeticOverflow.into());
			},
			SameOrOther::Other(remaining_debt) => {
				drop(remaining_debt);
				return Err(Error::<T>::ArithmeticOverflow.into());
			},
		}
		BranchStates::<T>::try_mutate(&collateral_id, |maybe| -> Result<_, DispatchError> {
			let bs = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
			bs.debt.bad_debt = bs.debt.bad_debt.saturating_sub(healable);
			Ok(())
		})?;
		Pallet::<T>::deposit_event(Event::BadDebtHealed { collateral_id, amount: healable });
		Ok(surplus)
	}
}
