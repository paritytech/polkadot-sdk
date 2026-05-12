//! `try_state` invariant verification (`troves.md` §9).
//!
//! Gated on `feature = "try-runtime"`. Run after every test by the mock's
//! `next_block` and end-to-end by the runtime's pre-upgrade hook.

use crate::{
	pallet::{BalanceOf, BranchStates, Branches, Config, FinalRecoveryNodes, MomentOf, Vaults},
	types::VaultStatus,
};
use frame::{
	deps::sp_runtime::{
		traits::{AtLeast32Bit, Saturating, Zero},
		FixedU128,
	},
	try_runtime::TryRuntimeError,
};
use pallet_linked_list::SortedListInterface;

pub fn do_try_state<T: Config>() -> Result<(), TryRuntimeError>
where
	BalanceOf<T>: Copy + Zero + Saturating + Into<u128>,
	MomentOf<T>: AtLeast32Bit + Copy,
{
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
					if !matches!(v.status, VaultStatus::Dormant) {
						return Err("last_dormant_vault_owner points at non-Dormant".into());
					}
				} else {
					return Err("last_dormant_vault_owner points at missing vault".into());
				}
			}
		}
	}
	Ok(())
}

// Silence unused-warning for `FixedU128` until additional invariants land.
#[allow(unused_imports)]
use FixedU128 as _Unused;
