use super::*;
use frame::deps::frame_support::traits::fungibles::Inspect as _;

/// Mode is `Frozen` if persisted, otherwise derived from live TCR.
pub fn current_mode<T: Config>(collateral_id: &T::AssetId) -> Result<BranchMode, DispatchError> {
	let bs = branch_state_of::<T>(collateral_id)?;
	if bs.is_frozen() {
		return Ok(BranchMode::Frozen);
	}
	// Try to read price; mode without a price falls back to `Normal`. The
	// caller is expected to gate state-changing ops on a fresh price first.
	let price = match T::Oracle::provide_price(collateral_id) {
		Ok(feed) => feed.price,
		Err(_) => return Ok(BranchMode::Normal),
	};
	let cfg = branch_cfg_of::<T>(collateral_id)?;
	let now = T::TimeProvider::now();
	let tcr = compute_tcr::<T>(&bs, price, now)?;
	if tcr < cfg.safety_collateralization_ratio {
		Ok(BranchMode::Safety)
	} else {
		Ok(BranchMode::Normal)
	}
}

/// Validate the rate is within branch bounds.
pub fn validate_rate<T: Config>(
	cfg: &BranchConfig<BalanceOf<T>, MomentOf<T>>,
	rate: FixedU128,
) -> Result<(), DispatchError> {
	if rate < cfg.minimum_borrow_rate || rate > cfg.maximum_borrow_rate {
		return Err(Error::<T>::RateOutOfBounds.into());
	}
	Ok(())
}

#[require_transactional]
pub fn register_branch<T: Config>(
	collateral_id: T::AssetId,
	config: BranchConfig<BalanceOf<T>, MomentOf<T>>,
) -> Result<(), DispatchError> {
	ensure!(!BranchConfigs::<T>::contains_key(&collateral_id), Error::<T>::BranchAlreadyRegistered);
	ensure!(
		T::CollateralAssets::asset_exists(collateral_id.clone()),
		Error::<T>::UnknownCollateral
	);
	Branches::<T>::try_mutate(|list| -> Result<_, DispatchError> {
		list.try_push(collateral_id.clone()).map_err(|_| Error::<T>::TooManyBranches)?;
		Ok(())
	})?;
	BranchConfigs::<T>::insert(&collateral_id, config);
	BranchStates::<T>::insert(
		&collateral_id,
		BranchState {
			total_collateral: BalanceOf::<T>::zero(),
			debt: BranchDebt {
				principal: BalanceOf::<T>::zero(),
				minted_interest: BalanceOf::<T>::zero(),
				pending_redist_principal: BalanceOf::<T>::zero(),
				bad_debt: BalanceOf::<T>::zero(),
				weighted_principal_sum: BalanceOf::<T>::zero(),
				last_interest_update: T::TimeProvider::now(),
			},
			stakes: BranchStakes {
				total: BalanceOf::<T>::zero(),
				weighted_sum: BalanceOf::<T>::zero(),
			},
			redist: RedistSnapshot::default(),
			queues: BranchQueues {
				next_final_recovery_nonce: 0,
				last_dormant_vault_owner: None,
				idle_cursor: None,
			},
			frozen: None,
		},
	);
	Pallet::<T>::deposit_event(Event::BranchRegistered { collateral_id });
	Ok(())
}

/// Apply `update` to the branch config and emit `ParameterUpdated`. Caller is
/// responsible for any defensive-action / authorization gating.
#[require_transactional]
pub fn update_branch_config<T: Config>(
	collateral_id: &T::AssetId,
	update: crate::types::BranchConfigUpdate<BalanceOf<T>, MomentOf<T>>,
) -> Result<(), DispatchError> {
	let parameter = update.parameter_id();
	BranchConfigs::<T>::try_mutate(collateral_id, |maybe| -> Result<_, DispatchError> {
		let cfg = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
		update.apply_to(cfg);
		Ok(())
	})?;
	Pallet::<T>::deposit_event(Event::ParameterUpdated {
		collateral_id: collateral_id.clone(),
		parameter,
	});
	Ok(())
}

/// Convenience: read a branch's current config (or error `UnknownCollateral`).
pub fn current_branch_config<T: Config>(
	collateral_id: &T::AssetId,
) -> Result<BranchConfig<BalanceOf<T>, MomentOf<T>>, DispatchError> {
	BranchConfigs::<T>::get(collateral_id).ok_or_else(|| Error::<T>::UnknownCollateral.into())
}

#[require_transactional]
pub fn enable_frozen_mode<T: Config>(collateral_id: &T::AssetId) -> Result<(), DispatchError> {
	BranchStates::<T>::try_mutate(collateral_id, |maybe| -> Result<_, DispatchError> {
		let bs = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
		if !bs.is_frozen() {
			bs.frozen = Some(FrozenState {
				reason: FrozenReason::Governance,
				entered_at: T::TimeProvider::now(),
			});
			Pallet::<T>::deposit_event(Event::ModeChanged {
				collateral_id: collateral_id.clone(),
				old_mode: BranchMode::Normal,
				new_mode: BranchMode::Frozen,
			});
		}
		Ok(())
	})
}

pub(crate) fn ensure_not_frozen<T: Config>(
	collateral_id: &T::AssetId,
) -> Result<(), DispatchError> {
	let bs = branch_state_of::<T>(collateral_id)?;
	ensure!(!bs.is_frozen(), Error::<T>::BranchFrozen);
	Ok(())
}

/// Apply Normal/Safety mode-aware TCR rules.
///
/// `is_settlement` is true for `FinalRecovery` redemptions/recovery offsets,
/// which are explicit settlement exceptions to the Safety-mode non-worsening
/// rule.
pub fn enforce_mode_rules<T: Config>(
	cfg: &BranchConfig<BalanceOf<T>, MomentOf<T>>,
	bs_pre: &BranchState<T::AccountId, BalanceOf<T>, MomentOf<T>>,
	pre_tcr: FixedU128,
	post_tcr: FixedU128,
	is_settlement: bool,
) -> Result<(), DispatchError> {
	if bs_pre.is_frozen() {
		return Err(Error::<T>::BranchFrozen.into());
	}
	if pre_tcr < cfg.safety_collateralization_ratio {
		// Safety mode.
		if !is_settlement && post_tcr < pre_tcr {
			return Err(Error::<T>::SafetyModeTcrWorsening.into());
		}
	} else {
		// Normal mode.
		if !is_settlement && post_tcr < cfg.safety_collateralization_ratio {
			return Err(Error::<T>::SafetyModeTcrWorsening.into());
		}
	}
	Ok(())
}
