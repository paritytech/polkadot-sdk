//! `try_state` invariant verification.
//!
//! Gated on `feature = "try-runtime"`. Run after every test by the mock's
//! `next_block` and end-to-end by the runtime's pre-upgrade hook.

use crate::{
	helpers,
	pallet::{BalanceOf, BranchStates, Branches, Config, HoldReason, Pallet, Vaults},
	types::VaultListId,
};
use frame::{
	deps::{
		frame_support::traits::fungibles::InspectHold,
		sp_runtime::{
			traits::{Saturating, UniqueSaturatedInto, Zero},
			FixedPointNumber,
		},
	},
	try_runtime::TryRuntimeError,
};
use pallet_linked_list::SortedListInterface;

pub fn do_try_state<T: Config>() -> Result<(), TryRuntimeError> {
	let branches = Branches::<T>::get();
	for c in branches.iter() {
		let rate_list = helpers::rate_list_id(c);
		let recovery_list = helpers::final_recovery_list_id(c);
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
		if in_recovery {
			continue;
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
	let expected_total = physical.saturating_sub(sum_pending_collat_share);
	let coll_drift = if bs.total_collateral >= expected_total {
		bs.total_collateral.saturating_sub(expected_total)
	} else {
		expected_total.saturating_sub(bs.total_collateral)
	};
	if coll_drift > tolerance {
		return Err("total_collateral drift exceeds rounding tolerance".into());
	}
	Ok(())
}
