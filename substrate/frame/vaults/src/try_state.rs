//! `try_state` invariant verification (`troves.md` §9).
//!
//! Gated on `feature = "try-runtime"`. Run after every test by the mock's
//! `next_block` and end-to-end by the runtime's pre-upgrade hook.

use crate::{
	pallet::{
		BalanceOf, BranchRedistStates, BranchStates, Branches, Config, FinalRecoveryNodes,
		HoldReason, Pallet, VaultRedistSnapshots, Vaults,
	},
	types::VaultStatus,
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
	for c in branches.iter().copied() {
		// (1) Per-vault membership rules.
		for (owner, vault) in Vaults::<T>::iter_prefix(c) {
			let in_rate_index = T::RateIndex::contains(&c, &owner);
			let in_recovery = FinalRecoveryNodes::<T>::contains_key(c, &owner);
			match vault.status {
				VaultStatus::Active => {
					if !in_rate_index {
						return Err("active vault not in rate index".into());
					}
					if in_recovery {
						return Err("active vault in recovery FIFO".into());
					}
				},
				VaultStatus::Dormant => {
					if in_rate_index {
						return Err("dormant vault in rate index".into());
					}
					if in_recovery {
						return Err("dormant vault in recovery FIFO".into());
					}
				},
				VaultStatus::FinalRecovery => {
					if in_rate_index {
						return Err("recovery vault in rate index".into());
					}
					if !in_recovery {
						return Err("recovery vault missing from FIFO".into());
					}
				},
			}
		}
		// (2) Branch state invariants.
		if let Some(bs) = BranchStates::<T>::get(c) {
			if bs.final_recovery_head.is_some() != bs.final_recovery_tail.is_some() {
				return Err("FinalRecovery FIFO endpoints inconsistent".into());
			}
			if let Some(owner) = bs.last_dormant_vault_owner.clone() {
				if let Some(v) = Vaults::<T>::get(c, &owner) {
					if !v.status.is_dormant() {
						return Err("last_dormant_vault_owner points at non-Dormant".into());
					}
				} else {
					return Err("last_dormant_vault_owner points at missing vault".into());
				}
			}
		}
		// (3) Redistribution accounting identities. Mirrors
		// `pallet-assets`'s "Σ per-account balances == supply" pattern.
		check_accounting_identities::<T>(c)?;
	}
	Ok(())
}

fn check_accounting_identities<T: Config>(c: T::AssetId) -> Result<(), TryRuntimeError> {
	let Some(bs) = BranchStates::<T>::get(c) else { return Ok(()) };
	let Some(redist) = BranchRedistStates::<T>::get(c) else { return Ok(()) };
	let cumul_debt_ps = redist.cumulative_redist_debt_per_stake;
	let cumul_collat_ps = redist.cumulative_redist_collat_per_stake;

	let mut sum_stake = BalanceOf::<T>::zero();
	let mut sum_owner_held = BalanceOf::<T>::zero();
	let mut sum_pending_debt_share = BalanceOf::<T>::zero();
	let mut sum_pending_collat_share = BalanceOf::<T>::zero();
	let mut n_live_vaults: u128 = 0;

	for (owner, vault) in Vaults::<T>::iter_prefix(c) {
		let held =
			T::CollateralAssets::balance_on_hold(c, &HoldReason::VaultCollateral.into(), &owner);
		sum_owner_held = sum_owner_held.saturating_add(held);
		if vault.status.is_final_recovery() {
			continue;
		}
		sum_stake = sum_stake.saturating_add(vault.stake);
		let snap = VaultRedistSnapshots::<T>::get(c, &owner).unwrap_or_default();
		let delta_debt = cumul_debt_ps.saturating_sub(snap.debt_per_stake);
		sum_pending_debt_share =
			sum_pending_debt_share.saturating_add(delta_debt.saturating_mul_int(vault.stake));
		let delta_collat = cumul_collat_ps.saturating_sub(snap.collat_per_stake);
		sum_pending_collat_share =
			sum_pending_collat_share.saturating_add(delta_collat.saturating_mul_int(vault.stake));
		n_live_vaults = n_live_vaults.saturating_add(1);
	}

	if bs.total_stakes != sum_stake {
		return Err("total_stakes != Σ active+dormant vault.stake".into());
	}

	let tolerance: BalanceOf<T> = n_live_vaults.unique_saturated_into();

	let debt_drift = if bs.pending_redistribution_debt >= sum_pending_debt_share {
		bs.pending_redistribution_debt.saturating_sub(sum_pending_debt_share)
	} else {
		sum_pending_debt_share.saturating_sub(bs.pending_redistribution_debt)
	};
	if debt_drift > tolerance {
		return Err("pending_redistribution_debt drift exceeds rounding tolerance".into());
	}

	let held_redist = T::CollateralAssets::balance_on_hold(
		c,
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
