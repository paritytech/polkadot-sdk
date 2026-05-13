//! `FinalRecovery` FIFO operations.
//!
//! See `troves.md` §6 (FIFO ops). The §7.6 settlement-pricing surface is
//! intentionally not implemented here — the redemption orchestrator pallet
//! owns recovery-pricing math and passes the resulting `RedemptionAllocation`
//! to `apply_redemption`.

use crate::{
	pallet::{BranchStates, Config, Error, Event, FinalRecoveryNodes, Pallet},
	types::FinalRecoveryNode,
};
use alloc::vec::Vec;
use frame::deps::{frame_support::traits::Time, sp_runtime::DispatchError};

/// Append `owner` to the per-branch FIFO. Errors if already present.
pub fn append<T: Config>(
	collateral_id: &T::AssetId,
	owner: T::AccountId,
) -> Result<(), DispatchError> {
	if FinalRecoveryNodes::<T>::contains_key(collateral_id, &owner) {
		return Err(Error::<T>::FinalRecoveryInvariantBroken.into());
	}

	let now = T::TimeProvider::now();
	BranchStates::<T>::try_mutate(collateral_id, |maybe_branch| -> Result<_, DispatchError> {
		let branch = maybe_branch.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
		let prev = branch.final_recovery_tail.clone();
		let node = FinalRecoveryNode { prev: prev.clone(), next: None, entered_at: now };
		FinalRecoveryNodes::<T>::insert(collateral_id, &owner, node);

		if let Some(prev_owner) = prev {
			FinalRecoveryNodes::<T>::mutate(collateral_id, &prev_owner, |maybe| {
				if let Some(n) = maybe {
					n.next = Some(owner.clone());
				}
			});
		} else {
			branch.final_recovery_head = Some(owner.clone());
		}
		branch.final_recovery_tail = Some(owner.clone());
		Ok(())
	})?;

	Pallet::<T>::deposit_event(Event::FinalRecoveryEntered {
		collateral_id: *collateral_id,
		owner,
	});
	Ok(())
}

/// Remove `owner` from the per-branch FIFO. Errors if not present.
pub fn remove<T: Config>(
	collateral_id: &T::AssetId,
	owner: &T::AccountId,
) -> Result<(), DispatchError> {
	let node = FinalRecoveryNodes::<T>::take(collateral_id, owner)
		.ok_or(Error::<T>::FinalRecoveryInvariantBroken)?;
	BranchStates::<T>::try_mutate(collateral_id, |maybe_branch| -> Result<_, DispatchError> {
		let branch = maybe_branch.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
		match (&node.prev, &node.next) {
			(Some(p), Some(n)) => {
				FinalRecoveryNodes::<T>::mutate(collateral_id, p, |maybe| {
					if let Some(left) = maybe {
						left.next = Some(n.clone());
					}
				});
				FinalRecoveryNodes::<T>::mutate(collateral_id, n, |maybe| {
					if let Some(right) = maybe {
						right.prev = Some(p.clone());
					}
				});
			},
			(Some(p), None) => {
				FinalRecoveryNodes::<T>::mutate(collateral_id, p, |maybe| {
					if let Some(left) = maybe {
						left.next = None;
					}
				});
				branch.final_recovery_tail = Some(p.clone());
			},
			(None, Some(n)) => {
				FinalRecoveryNodes::<T>::mutate(collateral_id, n, |maybe| {
					if let Some(right) = maybe {
						right.prev = None;
					}
				});
				branch.final_recovery_head = Some(n.clone());
			},
			(None, None) => {
				branch.final_recovery_head = None;
				branch.final_recovery_tail = None;
			},
		}
		Ok(())
	})?;
	Pallet::<T>::deposit_event(Event::FinalRecoveryExited {
		collateral_id: *collateral_id,
		owner: owner.clone(),
	});
	Ok(())
}

/// Peek the head of the FIFO, if any.
pub fn next_target<T: Config>(collateral_id: &T::AssetId) -> Option<T::AccountId> {
	BranchStates::<T>::get(collateral_id).and_then(|s| s.final_recovery_head)
}

/// First `n` FIFO owners, head-first.
pub fn queue_head<T: Config>(collateral_id: &T::AssetId, n: u32) -> Vec<T::AccountId> {
	let mut out = Vec::with_capacity(n as usize);
	let mut cursor = next_target::<T>(collateral_id);
	let mut taken = 0u32;
	while let Some(owner) = cursor {
		if taken >= n {
			break;
		}
		let node = match FinalRecoveryNodes::<T>::get(collateral_id, &owner) {
			Some(node) => node,
			None => break,
		};
		out.push(owner);
		cursor = node.next;
		taken = taken.saturating_add(1);
	}
	out
}
