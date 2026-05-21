//! `FinalRecovery` FIFO operations.
//!
//! See `troves.md` §6 (FIFO ops). The §7.6 settlement-pricing surface is
//! intentionally not implemented here — the redemption orchestrator pallet
//! owns recovery-pricing math and passes the resulting `RedemptionAllocation`
//! to `apply_redemption`.

use crate::{
	helpers::final_recovery_list_id,
	pallet::{BranchStates, Config, Error, Event, Pallet},
};
use alloc::vec::Vec;
use frame::deps::{
	frame_support::{ensure, require_transactional},
	sp_runtime::DispatchError,
};
use pallet_linked_list::{Position, SortedListInterface};

/// Append `owner` to the per-branch FIFO. Errors if already present.
#[require_transactional]
pub fn append<T: Config>(
	collateral_id: &T::AssetId,
	owner: T::AccountId,
) -> Result<(), DispatchError> {
	let list_id = final_recovery_list_id(collateral_id);
	ensure!(!T::VaultLists::contains(&list_id, &owner), Error::<T>::FinalRecoveryInvariantBroken,);

	let priority =
		BranchStates::<T>::try_mutate(collateral_id, |maybe_branch| -> Result<_, DispatchError> {
			let branch = maybe_branch.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
			let nonce = branch.queues.next_final_recovery_nonce;
			branch.queues.next_final_recovery_nonce =
				nonce.checked_add(1).ok_or(Error::<T>::FinalRecoverySequenceOverflow)?;
			Ok(frame::deps::sp_runtime::FixedU128::from_inner(nonce))
		})?;

	let hint = Position { prev: None, next: T::VaultLists::head(&list_id) };
	T::VaultLists::insert(list_id, owner.clone(), priority, hint)
		.map_err(|_| Error::<T>::FinalRecoveryInvariantBroken)?;

	Pallet::<T>::deposit_event(Event::FinalRecoveryEntered {
		collateral_id: collateral_id.clone(),
		owner,
	});
	Ok(())
}

/// Remove `owner` from the per-branch FIFO. Errors if not present.
#[require_transactional]
pub fn remove<T: Config>(
	collateral_id: &T::AssetId,
	owner: &T::AccountId,
) -> Result<(), DispatchError> {
	let list_id = final_recovery_list_id(collateral_id);
	T::VaultLists::remove(&list_id, owner).map_err(|_| Error::<T>::FinalRecoveryInvariantBroken)?;
	Pallet::<T>::deposit_event(Event::FinalRecoveryExited {
		collateral_id: collateral_id.clone(),
		owner: owner.clone(),
	});
	Ok(())
}

/// Peek the head of the FIFO, if any.
pub fn next_target<T: Config>(collateral_id: &T::AssetId) -> Option<T::AccountId> {
	T::VaultLists::tail(&final_recovery_list_id(collateral_id))
}

/// First `n` FIFO owners, oldest first.
pub fn queue_head<T: Config>(collateral_id: &T::AssetId, n: u32) -> Vec<T::AccountId> {
	T::VaultLists::iter_from_tail(&final_recovery_list_id(collateral_id), n)
}
