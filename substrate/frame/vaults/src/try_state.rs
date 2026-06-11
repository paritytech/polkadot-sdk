//! `try_state` invariant verification.
//!
//! Gated on `feature = "try-runtime"`. Run after every test by the mock's
//! `next_block` and end-to-end by the runtime's pre-upgrade hook.

use crate::{
	pallet::{
		BalanceOf, BranchConfigs, BranchStates, Branches, Config, HoldReason, Pallet, Vaults,
	},
	types::VaultListId,
};
use frame::{
	deps::{
		frame_support::traits::fungibles::InspectHold,
		sp_runtime::{
			traits::{One, Saturating, UniqueSaturatedInto, Zero},
			FixedPointNumber, FixedU128,
		},
	},
	try_runtime::TryRuntimeError,
};
use pallet_linked_list::SortedListInterface;

pub fn do_try_state<T: Config>() -> Result<(), TryRuntimeError> {
	let branches = Branches::<T>::get();
	for c in branches.iter() {
		let rate_list = VaultListId::Rate(c.clone());
		let recovery_list = VaultListId::FinalRecovery(c.clone());
		check_branch_identities::<T>(c, &rate_list, &recovery_list)?;
		// Mirrors `pallet-assets`'s "Σ per-account balances == supply" check:
		// `last_dormant_vault_owner` must point at a Dormant vault.
		if let Some(bs) = BranchStates::<T>::get(c) {
			if let Some(owner) = bs.last_dormant_vault_owner.clone() {
				let Some(vault) = Vaults::<T>::get(c, &owner) else {
					return Err("last_dormant_vault_owner points at missing vault".into());
				};
				if !vault.status::<T>(c, &owner).is_dormant() {
					return Err("last_dormant_vault_owner points at non-Dormant".into());
				}
			}
		}
	}
	Ok(())
}

/// Single pass over `Vaults::<T>::iter_prefix(c)`: per-vault membership
/// invariants and redistribution accounting sums.
fn check_branch_identities<T: Config>(
	c: &T::AssetId,
	rate_list: &VaultListId<T::AssetId>,
	recovery_list: &VaultListId<T::AssetId>,
) -> Result<(), TryRuntimeError> {
	let Some(bs) = BranchStates::<T>::get(c) else { return Ok(()) };
	let cumul_debt_ps = bs.redist.debt_per_stake;
	let cumul_collat_ps = bs.redist.collat_per_stake;

	let mut sum_stake = BalanceOf::<T>::zero();
	let mut sum_owner_held = BalanceOf::<T>::zero();
	let mut sum_pending_debt_share = BalanceOf::<T>::zero();
	let mut sum_pending_collat_share = BalanceOf::<T>::zero();
	let mut sum_principal = BalanceOf::<T>::zero();
	let mut sum_weighted_principal = BalanceOf::<T>::zero();
	let mut sum_weighted_stake = BalanceOf::<T>::zero();
	let mut n_live_vaults: u128 = 0;

	for (owner, vault) in Vaults::<T>::iter_prefix(c) {
		let in_rate_index = T::VaultLists::contains(rate_list, &owner);
		let in_recovery = T::VaultLists::contains(recovery_list, &owner);
		if in_rate_index && in_recovery {
			return Err("vault in both rate index and recovery FIFO".into());
		}
		let held = T::CollateralAssets::balance_on_hold(
			c.clone(),
			&HoldReason::VaultCollateral.into(),
			&owner,
		);
		sum_owner_held = sum_owner_held.saturating_add(held);
		// Every row — FinalRecovery included — keeps its debt attached to the
		// branch aggregates; only the stake is detached while in the FIFO.
		sum_principal = sum_principal.saturating_add(vault.debt.principal);
		sum_weighted_principal = sum_weighted_principal
			.saturating_add(vault.annual_rate.saturating_mul_int(vault.debt.principal));
		sum_weighted_stake = sum_weighted_stake
			.saturating_add(vault.annual_rate.saturating_mul_int(vault.redistribution_stake));
		if in_recovery {
			if !vault.redistribution_stake.is_zero() {
				return Err("FinalRecovery vault has non-zero redistribution_stake".into());
			}
			continue;
		}
		if vault.redistribution_stake != held {
			return Err("vault.redistribution_stake != held collateral".into());
		}
		sum_stake = sum_stake.saturating_add(vault.redistribution_stake);
		let snap = vault.redist_snapshot;
		let delta_debt = cumul_debt_ps.saturating_sub(snap.debt_per_stake);
		sum_pending_debt_share = sum_pending_debt_share
			.saturating_add(delta_debt.saturating_mul_int(vault.redistribution_stake));
		let delta_collat = cumul_collat_ps.saturating_sub(snap.collat_per_stake);
		sum_pending_collat_share = sum_pending_collat_share
			.saturating_add(delta_collat.saturating_mul_int(vault.redistribution_stake));
		n_live_vaults = n_live_vaults.saturating_add(1);
	}

	if bs.stakes.total != sum_stake {
		return Err("total_stakes != Σ active+dormant vault.redistribution_stake".into());
	}
	// Every writer moves principal on the branch and the vault by the same
	// amount, so this identity is exact (the prepare→finalize liquidation gap
	// is intra-extrinsic and invisible at block end).
	if bs.debt.principal != sum_principal {
		return Err("branch principal != Σ vault principal".into());
	}
	// Every stake mutation swaps full `floor(rate · stake)` contributions, so
	// this identity is exact as well.
	if bs.stakes.weighted_sum != sum_weighted_stake {
		return Err("stakes.weighted_sum != Σ floor(rate · stake)".into());
	}
	check_weighted_principal_sum::<T>(
		c,
		bs.debt.weighted_principal_sum,
		bs.debt.pending_redist_principal.saturating_add(bs.rounding.ownerless_pusd_debt),
		sum_weighted_principal,
		n_live_vaults,
	)?;

	let tolerance: BalanceOf<T> = n_live_vaults.unique_saturated_into();

	let debt_drift = if bs.debt.pending_redist_principal >= sum_pending_debt_share {
		bs.debt.pending_redist_principal.saturating_sub(sum_pending_debt_share)
	} else {
		sum_pending_debt_share.saturating_sub(bs.debt.pending_redist_principal)
	};
	if debt_drift > tolerance {
		return Err("pending redist principal drift exceeds rounding tolerance".into());
	}

	let held_redist = T::CollateralAssets::balance_on_hold(
		c.clone(),
		&HoldReason::VaultCollateral.into(),
		&Pallet::<T>::redistribution_account(),
	);
	let physical = sum_owner_held.saturating_add(held_redist);
	if bs.total_collateral != physical {
		return Err("total_collateral != Σ owner-held + redistribution-account hold".into());
	}

	// The redistribution account's hold = Σ pending collateral shares vaults
	// will pick up on next touch + ownerless collateral surplus. Per-vault
	// flooring may leave shares slightly below the held amount; treat the gap
	// as tolerance plus the explicit ownerless bucket.
	let claimed_plus_surplus =
		sum_pending_collat_share.saturating_add(bs.rounding.ownerless_collateral_surplus);
	let collat_drift = if held_redist >= claimed_plus_surplus {
		held_redist.saturating_sub(claimed_plus_surplus)
	} else {
		claimed_plus_surplus.saturating_sub(held_redist)
	};
	if collat_drift > tolerance {
		return Err("pending collateral share drift exceeds rounding tolerance".into());
	}
	Ok(())
}

fn check_weighted_principal_sum<T: Config>(
	c: &T::AssetId,
	weighted_principal_sum: BalanceOf<T>,
	pending_pool: BalanceOf<T>,
	sum_weighted_principal: BalanceOf<T>,
	n_live_vaults: u128,
) -> Result<(), TryRuntimeError> {
	if weighted_principal_sum < sum_weighted_principal {
		return Err("weighted_principal_sum below Σ floor(rate · principal)".into());
	}
	let Some(cfg) = BranchConfigs::<T>::get(c) else {
		return Err("registered branch without config".into());
	};
	let rate_bound = cfg.maximum_borrow_rate.max(FixedU128::one());
	let w_pending = rate_bound.saturating_mul_int(pending_pool);
	let slack: BalanceOf<T> = n_live_vaults.saturating_add(1).unique_saturated_into();
	let upper = sum_weighted_principal.saturating_add(w_pending).saturating_add(slack);
	if weighted_principal_sum > upper {
		return Err("weighted_principal_sum exceeds Σ floor(rate · principal) + allowance".into());
	}
	Ok(())
}
