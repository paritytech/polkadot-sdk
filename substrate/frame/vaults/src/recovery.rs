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
use frame::deps::{
	frame_support::{defensive, ensure, traits::Time},
	sp_runtime::DispatchError,
};

/// Set `node.next` on an existing FIFO node. The node is expected to exist
/// (every link is asserted by the FIFO invariant); a missing node is logged
/// defensively rather than silently skipped.
fn set_next<T: Config>(
	collateral_id: &T::AssetId,
	owner: &T::AccountId,
	next: Option<T::AccountId>,
) {
	FinalRecoveryNodes::<T>::mutate(collateral_id, owner, |maybe| {
		if let Some(node) = maybe {
			node.next = next;
		} else {
			defensive!("FinalRecoveryNodes: setting `next` on missing owner");
		}
	});
}

/// Set `node.prev` on an existing FIFO node. See [`set_next`].
fn set_prev<T: Config>(
	collateral_id: &T::AssetId,
	owner: &T::AccountId,
	prev: Option<T::AccountId>,
) {
	FinalRecoveryNodes::<T>::mutate(collateral_id, owner, |maybe| {
		if let Some(node) = maybe {
			node.prev = prev;
		} else {
			defensive!("FinalRecoveryNodes: setting `prev` on missing owner");
		}
	});
}

/// Append `owner` to the per-branch FIFO. Errors if already present.
pub fn append<T: Config>(
	collateral_id: &T::AssetId,
	owner: T::AccountId,
) -> Result<(), DispatchError> {
	ensure!(
		!FinalRecoveryNodes::<T>::contains_key(collateral_id, &owner),
		Error::<T>::FinalRecoveryInvariantBroken,
	);

	let now = T::TimeProvider::now();
	BranchStates::<T>::try_mutate(collateral_id, |maybe_branch| -> Result<_, DispatchError> {
		let branch = maybe_branch.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
		let prev = branch.final_recovery_tail.clone();
		let node = FinalRecoveryNode { prev: prev.clone(), next: None, entered_at: now };
		FinalRecoveryNodes::<T>::insert(collateral_id, &owner, node);

		match prev {
			Some(prev_owner) => set_next::<T>(collateral_id, &prev_owner, Some(owner.clone())),
			None => branch.final_recovery_head = Some(owner.clone()),
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
				set_next::<T>(collateral_id, p, Some(n.clone()));
				set_prev::<T>(collateral_id, n, Some(p.clone()));
			},
			(Some(p), None) => {
				set_next::<T>(collateral_id, p, None);
				branch.final_recovery_tail = Some(p.clone());
			},
			(None, Some(n)) => {
				set_prev::<T>(collateral_id, n, None);
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
		let Some(node) = FinalRecoveryNodes::<T>::get(collateral_id, &owner) else {
			break;
		};
		out.push(owner);
		cursor = node.next;
		taken = taken.saturating_add(1);
	}
	out
}
