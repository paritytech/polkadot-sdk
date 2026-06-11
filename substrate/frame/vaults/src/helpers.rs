//! Storage-touching helpers: vault lifecycle, branch mode, interest,
//! redistribution, fees, governance, on-idle.
//!
//! Most extrinsics in `lib.rs` are thin wrappers over these.

use crate::{
	math,
	pallet::{
		BalanceOf, BranchConfigs, BranchStates, Branches, Config, Error, Event, HoldReason,
		MomentOf, Pallet, Vaults,
	},
	recovery,
	types::{
		BranchConfig, BranchDebt, BranchMode, BranchStakes, BranchState, DebtPayment, FrozenReason,
		FrozenState, RedistSnapshot, Vault, VaultDebt, VaultListId, VaultStatus,
	},
	weights::WeightInfo,
};
use alloc::vec::Vec;
use frame::{
	deps::{
		frame_support::{
			require_transactional,
			storage::with_storage_layer,
			traits::{
				fungible::{Balanced as FungibleBalanced, Mutate as FungibleMutate},
				fungibles::{
					InspectHold as FungiblesInspectHold, MutateHold as FungiblesMutateHold,
				},
				tokens::{Fortitude, Imbalance, Precision, Preservation, Restriction},
				OnUnbalanced, Time,
			},
			weights::Weight,
		},
		sp_runtime::{
			traits::{CheckedDiv, Saturating, Zero},
			DispatchError, FixedPointNumber, FixedU128, Permill,
		},
	},
	prelude::*,
};
use pallet_linked_list::{ListError, Position, SortedListInterface};
use pusd_primitives::ProvidePrice;

fn moment_to_millis<T: Config>(m: MomentOf<T>) -> u64 {
	use frame::deps::sp_runtime::traits::SaturatedConversion;
	m.saturated_into::<u64>()
}

pub(crate) fn millis_diff<T: Config>(now: MomentOf<T>, then: MomentOf<T>) -> u64 {
	moment_to_millis::<T>(now.saturating_sub(then))
}

/// Translate a rate-index insert/re-insert failure. A stale user-supplied
/// hint surfaces as [`Error::InvalidPositionHints`]; every other kind means
/// the index and the vault rows disagree.
pub(crate) fn map_error<T: Config>(e: ListError) -> Error<T> {
	match e {
		ListError::InvalidPositionHints => Error::<T>::InvalidPositionHints,
		ListError::ItemNotFound |
		ListError::ItemAlreadyExists |
		ListError::ListTooLong |
		ListError::CorruptList => Error::<T>::RateIndexInvariantBroken,
	}
}

/// Read the branch state, returning `UnknownCollateral` when missing.
pub(crate) fn branch_state_of<T: Config>(
	collateral_id: &T::AssetId,
) -> Result<BranchState<T::AccountId, BalanceOf<T>, MomentOf<T>>, DispatchError> {
	BranchStates::<T>::get(collateral_id).ok_or_else(|| Error::<T>::UnknownCollateral.into())
}

/// Read the branch config, returning `UnknownCollateral` when missing.
pub(crate) fn branch_cfg_of<T: Config>(
	collateral_id: &T::AssetId,
) -> Result<BranchConfig<BalanceOf<T>, MomentOf<T>>, DispatchError> {
	BranchConfigs::<T>::get(collateral_id).ok_or_else(|| Error::<T>::UnknownCollateral.into())
}

/// Read a vault row, returning `VaultNotFound` when missing.
pub(crate) fn vault_of<T: Config>(
	collateral_id: &T::AssetId,
	owner: &T::AccountId,
) -> Result<Vault<BalanceOf<T>, MomentOf<T>>, DispatchError> {
	Vaults::<T>::get(collateral_id, owner).ok_or_else(|| Error::<T>::VaultNotFound.into())
}

impl<Balance, Moment> Vault<Balance, Moment> {
	/// Derive this vault's lifecycle status from queue/index membership.
	///
	/// Status is not stored on the row, and the keys must be re-supplied
	/// because the row does not carry them. The `&self` receiver is a proof
	/// of existence.
	pub(crate) fn status<T: Config>(
		&self,
		collateral_id: &T::AssetId,
		owner: &T::AccountId,
	) -> VaultStatus {
		if T::VaultLists::contains(&VaultListId::Rate(collateral_id.clone()), owner) {
			return VaultStatus::Active;
		}
		if T::VaultLists::contains(&VaultListId::FinalRecovery(collateral_id.clone()), owner) {
			return VaultStatus::FinalRecovery;
		}
		VaultStatus::Dormant
	}
}

mod accounting;
mod branch;
mod ops;
mod views;

use accounting::{charge_upfront_fee, simulate_borrow, simulate_change_rate};
pub(crate) use accounting::{
	compute_tcr, open_upfront_fee, pending_touch_for, touch_vault, update_aggregate_interest,
};
pub(crate) use branch::{
	clear_governance_frozen_mode, current_branch_config, current_mode, enable_frozen_mode,
	enforce_mode_rules, ensure_not_frozen, refresh_branch, register_branch, update_branch_config,
	validate_rate,
};
pub(crate) use ops::{
	borrow, change_rate, close_vault, deposit_collateral_for, enter_final_recovery,
	exit_final_recovery, on_idle_walk, open_vault, poke, repay_for, withdraw_collateral,
};
pub(crate) use views::{
	predict_upfront_fee_borrow, predict_upfront_fee_open, predict_upfront_fee_rate_change,
	view_branch_tcr, view_debt_in_front, view_redemption_queue_head, view_vault_cr,
	view_vault_status,
};
