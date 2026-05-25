use super::*;

#[require_transactional]
pub fn open_vault<T: Config>(
	owner: T::AccountId,
	collateral_id: T::AssetId,
	initial_collateral: BalanceOf<T>,
	initial_debt: BalanceOf<T>,
	annual_rate: FixedU128,
	hint: Position<T::AccountId>,
) -> Result<(), DispatchError> {
	ensure_not_frozen::<T>(&collateral_id)?;
	ensure!(!Vaults::<T>::contains_key(&collateral_id, &owner), Error::<T>::VaultAlreadyExists);
	let cfg = branch_cfg_of::<T>(&collateral_id)?;
	ensure!(initial_debt >= cfg.minimum_debt, Error::<T>::DebtBelowMinimum);
	ensure!(initial_collateral >= cfg.minimum_collateral, Error::<T>::InsufficientCollateral);
	validate_rate::<T>(&cfg, annual_rate)?;

	let now = T::TimeProvider::now();
	let price = T::Oracle::provide_price(&collateral_id)?.price;
	update_aggregate_interest::<T>(&collateral_id, now)?;

	let bs_before = branch_state_of::<T>(&collateral_id)?;
	ensure!(
		bs_before.debt.principal.saturating_add(initial_debt) <= cfg.debt_ceiling,
		Error::<T>::DebtCeilingExceeded
	);

	let upfront_fee = open_upfront_fee::<T>(&bs_before, &cfg, initial_debt, annual_rate);

	let vault = Vault {
		debt: VaultDebt { principal: initial_debt, interest: upfront_fee },
		annual_rate,
		last_interest_update: now,
		last_rate_update: now,
		redistribution_stake: initial_collateral,
		redist_snapshot: bs_before.redist,
	};

	let total_debt = initial_debt.saturating_add(upfront_fee);
	let cr = math::collateralization_ratio::<BalanceOf<T>>(initial_collateral, total_debt, price)
		.ok_or(Error::<T>::UnsafeCollateralizationRatio)?;
	ensure!(cr >= cfg.initial_collateralization_ratio, Error::<T>::UnsafeCollateralizationRatio);

	let pre_tcr = compute_tcr::<T>(&bs_before, price, now)?;
	let mut bs_after = bs_before.clone();
	bs_after.attach_vault(&vault);
	bs_after.add_collateral(initial_collateral);
	let post_tcr = compute_tcr::<T>(&bs_after, price, now)?;
	enforce_mode_rules::<T>(&cfg, &bs_before, pre_tcr, post_tcr, false)?;
	T::CollateralAssets::hold(
		collateral_id.clone(),
		&HoldReason::VaultCollateral.into(),
		&owner,
		initial_collateral,
	)?;

	T::StableAsset::mint_into(&owner, initial_debt)?;
	charge_upfront_fee::<T>(&collateral_id, &owner, upfront_fee);

	Vaults::<T>::insert(&collateral_id, &owner, &vault);
	BranchStates::<T>::insert(&collateral_id, &bs_after);

	T::VaultLists::insert(rate_list_id(&collateral_id), owner.clone(), annual_rate, hint)
		.map_err(|_| Error::<T>::InvalidPositionHints)?;

	Pallet::<T>::deposit_event(Event::Borrowed {
		collateral_id: collateral_id.clone(),
		owner: owner.clone(),
		recipient: owner.clone(),
		amount: initial_debt,
	});
	Pallet::<T>::deposit_event(Event::CollateralDeposited {
		collateral_id: collateral_id.clone(),
		owner: owner.clone(),
		from: owner.clone(),
		amount: initial_collateral,
	});
	Pallet::<T>::deposit_event(Event::VaultOpened { collateral_id, owner });
	Ok(())
}

/// Permissionless deposit. This call does not change debt, so a target that
/// is still `Dormant` after `touch_vault` (i.e. accrued interest hasn't lifted
/// debt above `MinimumDebt`) is rejected. If touch auto-revived the vault to
/// `Active`, the deposit proceeds against the now-Active row.
#[require_transactional]
pub fn deposit_collateral_for<T: Config>(
	from: T::AccountId,
	owner: T::AccountId,
	collateral_id: T::AssetId,
	amount: BalanceOf<T>,
) -> Result<(), DispatchError> {
	ensure_not_frozen::<T>(&collateral_id)?;
	ensure!(Vaults::<T>::contains_key(&collateral_id, &owner), Error::<T>::VaultNotFound);
	let now = T::TimeProvider::now();
	update_aggregate_interest::<T>(&collateral_id, now)?;
	let mut vault =
		touch_vault::<T>(&collateral_id, &owner, now, None)?.ok_or(Error::<T>::VaultNotFound)?;
	ensure!(!vault.status::<T>(&collateral_id, &owner).is_dormant(), Error::<T>::DebtBelowMinimum);

	T::CollateralAssets::transfer_and_hold(
		collateral_id.clone(),
		&HoldReason::VaultCollateral.into(),
		&from,
		&owner,
		amount,
		Precision::Exact,
		Preservation::Expendable,
		Fortitude::Polite,
	)?;

	let old_stake = vault.redistribution_stake;
	let new_stake = old_stake.saturating_add(amount);
	vault.redistribution_stake = new_stake;

	BranchStates::<T>::try_mutate(&collateral_id, |maybe| -> Result<_, DispatchError> {
		let bs = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
		bs.total_collateral = bs.total_collateral.saturating_add(amount);
		bs.refresh_vault_stake(vault.annual_rate, old_stake, new_stake);
		Ok(())
	})?;
	Vaults::<T>::insert(&collateral_id, &owner, &vault);

	Pallet::<T>::deposit_event(Event::CollateralDeposited { collateral_id, owner, from, amount });
	Ok(())
}

#[require_transactional]
pub fn withdraw_collateral<T: Config>(
	owner: T::AccountId,
	collateral_id: T::AssetId,
	amount: BalanceOf<T>,
	recipient: Option<T::AccountId>,
) -> Result<(), DispatchError> {
	ensure_not_frozen::<T>(&collateral_id)?;
	let recipient = recipient.unwrap_or(owner.clone());

	let now = T::TimeProvider::now();
	let price = T::Oracle::provide_price(&collateral_id)?.price;
	update_aggregate_interest::<T>(&collateral_id, now)?;
	let mut vault =
		touch_vault::<T>(&collateral_id, &owner, now, None)?.ok_or(Error::<T>::VaultNotFound)?;
	ensure!(
		!vault.status::<T>(&collateral_id, &owner).is_final_recovery(),
		Error::<T>::VaultInFinalRecovery
	);

	let cfg = branch_cfg_of::<T>(&collateral_id)?;
	let bs_before = branch_state_of::<T>(&collateral_id)?;
	let coll = T::CollateralAssets::balance_on_hold(
		collateral_id.clone(),
		&HoldReason::VaultCollateral.into(),
		&owner,
	);
	ensure!(coll >= amount, Error::<T>::InsufficientCollateral);

	let total_debt = vault.debt.total();
	let new_coll = coll.saturating_sub(amount);
	if !total_debt.is_zero() {
		let cr = math::collateralization_ratio::<BalanceOf<T>>(new_coll, total_debt, price)
			.ok_or(Error::<T>::UnsafeCollateralizationRatio)?;
		ensure!(
			cr >= cfg.initial_collateralization_ratio,
			Error::<T>::UnsafeCollateralizationRatio
		);
	}

	let pre_tcr = compute_tcr::<T>(&bs_before, price, now)?;
	let mut bs_after = bs_before.clone();
	bs_after.remove_collateral(amount);
	bs_after.refresh_vault_stake(vault.annual_rate, vault.redistribution_stake, new_coll);
	let post_tcr = compute_tcr::<T>(&bs_after, price, now)?;
	enforce_mode_rules::<T>(&cfg, &bs_before, pre_tcr, post_tcr, false)?;

	T::CollateralAssets::transfer_on_hold(
		collateral_id.clone(),
		&HoldReason::VaultCollateral.into(),
		&owner,
		&recipient,
		amount,
		Precision::Exact,
		Restriction::Free,
		Fortitude::Polite,
	)?;

	vault.redistribution_stake = new_coll;
	Vaults::<T>::insert(&collateral_id, &owner, &vault);
	BranchStates::<T>::insert(&collateral_id, &bs_after);
	Pallet::<T>::deposit_event(Event::CollateralWithdrawn {
		collateral_id,
		owner,
		recipient,
		amount,
	});
	Ok(())
}

#[require_transactional]
pub fn borrow<T: Config>(
	owner: T::AccountId,
	collateral_id: T::AssetId,
	amount: BalanceOf<T>,
	maybe_new_rate: Option<FixedU128>,
	recipient: Option<T::AccountId>,
	hint: Position<T::AccountId>,
) -> Result<(), DispatchError> {
	ensure_not_frozen::<T>(&collateral_id)?;
	let recipient = recipient.unwrap_or(owner.clone());

	let now = T::TimeProvider::now();
	let price = T::Oracle::provide_price(&collateral_id)?.price;
	update_aggregate_interest::<T>(&collateral_id, now)?;
	// If the caller isn't changing the rate, hand the hint to touch_vault so
	// a Dormant→Active revival inside the touch uses the user-supplied O(1)
	// position instead of an O(N) `find_position` walk. With a rate change,
	// the hint is for the new rate; touch would insert at the old rate, so
	// pass None and let the subsequent `re_insert` consume the hint.
	let touch_hint = if maybe_new_rate.is_none() { Some(hint.clone()) } else { None };
	let mut vault = touch_vault::<T>(&collateral_id, &owner, now, touch_hint)?
		.ok_or(Error::<T>::VaultNotFound)?;
	let pre_status = vault.status::<T>(&collateral_id, &owner);
	ensure!(!pre_status.is_final_recovery(), Error::<T>::VaultInFinalRecovery);

	let cfg = branch_cfg_of::<T>(&collateral_id)?;
	let old_rate = vault.annual_rate;
	let new_rate = maybe_new_rate.unwrap_or(old_rate);
	validate_rate::<T>(&cfg, new_rate)?;

	let bs_before = branch_state_of::<T>(&collateral_id)?;
	let new_ib_debt = vault.debt.principal.saturating_add(amount);
	ensure!(
		bs_before.debt.principal.saturating_add(amount) <= cfg.debt_ceiling,
		Error::<T>::DebtCeilingExceeded
	);

	let cooldown_elapsed =
		now.saturating_sub(vault.last_rate_update) >= cfg.rate_adjustment_cooldown;
	let rate_change_fee_base = if maybe_new_rate.is_some() && !cooldown_elapsed {
		vault.debt.principal
	} else {
		BalanceOf::<T>::zero()
	};
	let (mut bs_after, upfront_fee) =
		simulate_borrow::<T>(&bs_before, &cfg, &vault, amount, new_rate, rate_change_fee_base);
	bs_after.debt.minted_interest = bs_after.debt.minted_interest.saturating_add(upfront_fee);

	let dormant_to_active = pre_status.is_dormant() && new_ib_debt >= cfg.minimum_debt;
	vault.debt.principal = new_ib_debt;
	vault.debt.interest = vault.debt.interest.saturating_add(upfront_fee);
	if maybe_new_rate.is_some() {
		vault.annual_rate = new_rate;
		vault.last_rate_update = now;
	}
	ensure!(vault.debt.principal >= cfg.minimum_debt, Error::<T>::DebtBelowMinimum);

	let coll = T::CollateralAssets::balance_on_hold(
		collateral_id.clone(),
		&HoldReason::VaultCollateral.into(),
		&owner,
	);
	let total_debt = vault.debt.total();
	let cr = math::collateralization_ratio::<BalanceOf<T>>(coll, total_debt, price)
		.ok_or(Error::<T>::UnsafeCollateralizationRatio)?;
	ensure!(cr >= cfg.initial_collateralization_ratio, Error::<T>::UnsafeCollateralizationRatio);

	let pre_tcr = compute_tcr::<T>(&bs_before, price, now)?;
	let post_tcr = compute_tcr::<T>(&bs_after, price, now)?;
	enforce_mode_rules::<T>(&cfg, &bs_before, pre_tcr, post_tcr, false)?;

	if dormant_to_active && bs_after.last_dormant_vault_owner.as_ref() == Some(&owner) {
		bs_after.last_dormant_vault_owner = None;
	}

	T::StableAsset::mint_into(&recipient, amount)?;
	charge_upfront_fee::<T>(&collateral_id, &owner, upfront_fee);

	BranchStates::<T>::insert(&collateral_id, &bs_after);
	Vaults::<T>::insert(&collateral_id, &owner, &vault);

	if dormant_to_active {
		T::VaultLists::insert(rate_list_id(&collateral_id), owner.clone(), new_rate, hint)
			.map_err(|_| Error::<T>::InvalidPositionHints)?;
		Pallet::<T>::deposit_event(Event::VaultStatusChanged {
			collateral_id: collateral_id.clone(),
			owner: owner.clone(),
			old_status: VaultStatus::Dormant,
			new_status: VaultStatus::Active,
		});
	} else if old_rate != new_rate {
		T::VaultLists::re_insert(rate_list_id(&collateral_id), owner.clone(), new_rate, hint)
			.map_err(|_| Error::<T>::InvalidPositionHints)?;
	}

	if old_rate != new_rate {
		Pallet::<T>::deposit_event(Event::BorrowRateChanged {
			collateral_id: collateral_id.clone(),
			owner: owner.clone(),
			old_rate,
			new_rate,
		});
	}
	Pallet::<T>::deposit_event(Event::Borrowed { collateral_id, owner, recipient, amount });
	Ok(())
}

#[require_transactional]
pub fn repay_for<T: Config>(
	from: T::AccountId,
	owner: T::AccountId,
	collateral_id: T::AssetId,
	amount: BalanceOf<T>,
) -> Result<(), DispatchError> {
	ensure_not_frozen::<T>(&collateral_id)?;
	let now = T::TimeProvider::now();
	update_aggregate_interest::<T>(&collateral_id, now)?;
	let mut vault =
		touch_vault::<T>(&collateral_id, &owner, now, None)?.ok_or(Error::<T>::VaultNotFound)?;
	let pre_status = vault.status::<T>(&collateral_id, &owner);
	ensure!(!pre_status.is_final_recovery(), Error::<T>::VaultInFinalRecovery);

	let cfg = branch_cfg_of::<T>(&collateral_id)?;

	T::StableAsset::burn_from(
		&from,
		amount,
		Preservation::Expendable,
		Precision::Exact,
		Fortitude::Polite,
	)?;

	let payment = vault.debt.cancel(amount);
	ensure!(payment.total() == amount, Error::<T>::InsufficientRepayment);

	// User repayments must leave `Debt == 0` (and close in
	// same op) OR `Debt >= MinimumDebt`.
	let new_total = vault.debt.total();
	if !new_total.is_zero() && new_total < cfg.minimum_debt {
		return Err(Error::<T>::DebtWouldBecomeDust.into());
	}

	if new_total.is_zero() {
		let price = T::Oracle::provide_price(&collateral_id)?.price;
		close_inner::<T>(
			&collateral_id,
			&owner,
			&owner,
			&vault,
			pre_status,
			&cfg,
			now,
			price,
			Some((payment, vault.annual_rate)),
		)?;
		Pallet::<T>::deposit_event(Event::Repaid { collateral_id, owner, from, amount });
		return Ok(());
	}

	BranchStates::<T>::try_mutate(&collateral_id, |maybe| -> Result<_, DispatchError> {
		let bs = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
		bs.apply_debt_payment(payment, vault.annual_rate);
		Ok(())
	})?;

	Vaults::<T>::insert(&collateral_id, &owner, &vault);
	Pallet::<T>::deposit_event(Event::Repaid { collateral_id, owner, from, amount });
	Ok(())
}

#[require_transactional]
pub fn change_rate<T: Config>(
	owner: T::AccountId,
	collateral_id: T::AssetId,
	new_rate: FixedU128,
	hint: Position<T::AccountId>,
) -> Result<(), DispatchError> {
	ensure_not_frozen::<T>(&collateral_id)?;
	let now = T::TimeProvider::now();
	update_aggregate_interest::<T>(&collateral_id, now)?;
	let mut vault =
		touch_vault::<T>(&collateral_id, &owner, now, None)?.ok_or(Error::<T>::VaultNotFound)?;
	ensure!(vault.status::<T>(&collateral_id, &owner).is_active(), Error::<T>::InvalidVaultStatus);
	let old_rate = vault.annual_rate;
	if old_rate == new_rate {
		return Ok(());
	}

	let cfg = branch_cfg_of::<T>(&collateral_id)?;
	validate_rate::<T>(&cfg, new_rate)?;

	// Build the post-change branch state in a clone so we can compute
	// `post_tcr` and run `enforce_mode_rules` BEFORE applying anything.
	let bs_before = branch_state_of::<T>(&collateral_id)?;
	let cooldown_elapsed =
		now.saturating_sub(vault.last_rate_update) >= cfg.rate_adjustment_cooldown;
	let (mut bs_after, upfront_fee) =
		simulate_change_rate::<T>(&bs_before, &cfg, &vault, new_rate, cooldown_elapsed);
	bs_after.debt.minted_interest = bs_after.debt.minted_interest.saturating_add(upfront_fee);

	// Gates premature changes that would worsen TCR in Safety mode and rate
	// changes that would push the branch into Safety mode from Normal.
	let price = T::Oracle::provide_price(&collateral_id)?.price;
	let pre_tcr = compute_tcr::<T>(&bs_before, price, now)?;
	let post_tcr = compute_tcr::<T>(&bs_after, price, now)?;
	enforce_mode_rules::<T>(&cfg, &bs_before, pre_tcr, post_tcr, false)?;

	charge_upfront_fee::<T>(&collateral_id, &owner, upfront_fee);

	BranchStates::<T>::insert(&collateral_id, &bs_after);

	vault.annual_rate = new_rate;
	vault.last_rate_update = now;
	vault.debt.interest = vault.debt.interest.saturating_add(upfront_fee);
	Vaults::<T>::insert(&collateral_id, &owner, &vault);

	T::VaultLists::re_insert(rate_list_id(&collateral_id), owner.clone(), new_rate, hint)
		.map_err(|_| Error::<T>::InvalidPositionHints)?;
	Pallet::<T>::deposit_event(Event::BorrowRateChanged {
		collateral_id,
		owner,
		old_rate,
		new_rate,
	});
	Ok(())
}

#[require_transactional]
pub fn close_vault<T: Config>(
	owner: T::AccountId,
	collateral_id: T::AssetId,
	recipient: Option<T::AccountId>,
) -> Result<(), DispatchError> {
	ensure_not_frozen::<T>(&collateral_id)?;
	let recipient = recipient.unwrap_or(owner.clone());

	let now = T::TimeProvider::now();
	let price = T::Oracle::provide_price(&collateral_id)?.price;
	update_aggregate_interest::<T>(&collateral_id, now)?;
	let vault =
		touch_vault::<T>(&collateral_id, &owner, now, None)?.ok_or(Error::<T>::VaultNotFound)?;
	let status = vault.status::<T>(&collateral_id, &owner);
	ensure!(vault.debt.total().is_zero(), Error::<T>::InsufficientRepayment);

	let cfg = branch_cfg_of::<T>(&collateral_id)?;
	close_inner::<T>(&collateral_id, &owner, &recipient, &vault, status, &cfg, now, price, None)
}

/// Apply a vault close. Caller must have already touched the vault and confirmed
/// `vault.debt.total() == 0` (or supplies `maybe_payment` whose application
/// zeroes the debt). Detaches branch aggregates, runs the Safety-mode TCR
/// check, releases held collateral, deletes the row, and emits `VaultClosed`.
#[require_transactional]
#[allow(clippy::too_many_arguments)]
fn close_inner<T: Config>(
	collateral_id: &T::AssetId,
	owner: &T::AccountId,
	recipient: &T::AccountId,
	vault: &Vault<BalanceOf<T>, MomentOf<T>>,
	status: VaultStatus,
	cfg: &BranchConfig<BalanceOf<T>, MomentOf<T>>,
	now: MomentOf<T>,
	price: FixedU128,
	maybe_payment: Option<(DebtPayment<BalanceOf<T>>, FixedU128)>,
) -> Result<(), DispatchError> {
	let coll = T::CollateralAssets::balance_on_hold(
		collateral_id.clone(),
		&HoldReason::VaultCollateral.into(),
		owner,
	);
	let bs_before = branch_state_of::<T>(collateral_id)?;
	let mut bs_after = bs_before.clone();
	if let Some((payment, rate)) = maybe_payment {
		bs_after.apply_debt_payment(payment, rate);
	}
	bs_after.detach_vault(vault);
	bs_after.remove_collateral(coll);
	if bs_after.last_dormant_vault_owner.as_ref() == Some(owner) {
		bs_after.last_dormant_vault_owner = None;
	}

	let pre_tcr = compute_tcr::<T>(&bs_before, price, now)?;
	let post_tcr = compute_tcr::<T>(&bs_after, price, now)?;
	enforce_mode_rules::<T>(cfg, &bs_before, pre_tcr, post_tcr, false)?;

	if !coll.is_zero() {
		T::CollateralAssets::transfer_on_hold(
			collateral_id.clone(),
			&HoldReason::VaultCollateral.into(),
			owner,
			recipient,
			coll,
			Precision::Exact,
			Restriction::Free,
			Fortitude::Polite,
		)?;
	}

	BranchStates::<T>::insert(collateral_id, &bs_after);

	match status {
		VaultStatus::Active => {
			// Invariant: Active vaults are in the rate index, so `remove` succeeds.
			let _ = T::VaultLists::remove(&rate_list_id(collateral_id), owner);
		},
		VaultStatus::FinalRecovery => {
			let _ = recovery::remove::<T>(collateral_id, owner);
		},
		VaultStatus::Dormant => {},
	}

	Vaults::<T>::remove(collateral_id, owner);

	Pallet::<T>::deposit_event(Event::VaultClosed {
		collateral_id: collateral_id.clone(),
		owner: owner.clone(),
		recipient: recipient.clone(),
	});
	Ok(())
}

#[require_transactional]
pub fn poke<T: Config>(
	owner: T::AccountId,
	collateral_id: T::AssetId,
) -> Result<(), DispatchError> {
	let now = T::TimeProvider::now();
	update_aggregate_interest::<T>(&collateral_id, now)?;
	touch_vault::<T>(&collateral_id, &owner, now, None).map(|_| ())
}

#[require_transactional]
pub fn enter_final_recovery<T: Config>(
	owner: T::AccountId,
	collateral_id: T::AssetId,
) -> Result<(), DispatchError> {
	ensure_not_frozen::<T>(&collateral_id)?;
	let now = T::TimeProvider::now();
	let price = T::Oracle::provide_price(&collateral_id)?.price;
	update_aggregate_interest::<T>(&collateral_id, now)?;
	let mut vault =
		touch_vault::<T>(&collateral_id, &owner, now, None)?.ok_or(Error::<T>::VaultNotFound)?;
	ensure!(vault.status::<T>(&collateral_id, &owner).is_active(), Error::<T>::InvalidVaultStatus);

	let cfg = branch_cfg_of::<T>(&collateral_id)?;
	let coll = T::CollateralAssets::balance_on_hold(
		collateral_id.clone(),
		&HoldReason::VaultCollateral.into(),
		&owner,
	);
	let total_debt = vault.debt.total();
	let cr = math::collateralization_ratio::<BalanceOf<T>>(coll, total_debt, price)
		.ok_or(Error::<T>::UnsafeCollateralizationRatio)?;
	ensure!(cr < cfg.minimum_collateralization_ratio, Error::<T>::UnsafeCollateralizationRatio);

	// Last eligible redistribution recipient: the candidate is the only
	// stake-bearer left, so `bs.stakes.total == vault.redistribution_stake`.
	let bs_check = branch_state_of::<T>(&collateral_id)?;
	ensure!(bs_check.stakes.total == vault.redistribution_stake, Error::<T>::NotLastEligibleVault);

	T::VaultLists::remove(&rate_list_id(&collateral_id), &owner)
		.map_err(|_| Error::<T>::RateIndexInvariantBroken)?;
	let stake_weighted = vault.annual_rate.saturating_mul_int(vault.redistribution_stake);
	let detached_stake = vault.redistribution_stake;
	BranchStates::<T>::try_mutate(&collateral_id, |maybe| -> Result<_, DispatchError> {
		let bs = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
		bs.stakes.total = bs.stakes.total.saturating_sub(detached_stake);
		bs.stakes.weighted_sum = bs.stakes.weighted_sum.saturating_sub(stake_weighted);
		Ok(())
	})?;
	vault.redistribution_stake = BalanceOf::<T>::zero();
	Vaults::<T>::insert(&collateral_id, &owner, &vault);
	recovery::append::<T>(&collateral_id, owner.clone())?;

	Pallet::<T>::deposit_event(Event::VaultStatusChanged {
		collateral_id,
		owner,
		old_status: VaultStatus::Active,
		new_status: VaultStatus::FinalRecovery,
	});
	Ok(())
}

/// Permissionless explicit `FinalRecovery` exit. Touches the vault, checks
/// the fully-accrued CR is at or above the MCR, re-adds the stake + weighted
/// stake contribution, then either:
/// - if `debt >= MinimumDebt` rejoins the rate index using the caller-supplied `hint` (status →
///   Active, O(1) with valid hint),
/// - otherwise leaves the vault out of the index (status → Dormant) and parks the owner in
///   `last_dormant_vault_owner` so the next redemption can pick it up. The `hint` argument is
///   ignored in the Dormant branch.
#[require_transactional]
pub fn exit_final_recovery<T: Config>(
	owner: T::AccountId,
	collateral_id: T::AssetId,
	hint: Position<T::AccountId>,
) -> Result<(), DispatchError> {
	ensure_not_frozen::<T>(&collateral_id)?;
	let now = T::TimeProvider::now();
	let price = T::Oracle::provide_price(&collateral_id)?.price;
	update_aggregate_interest::<T>(&collateral_id, now)?;
	let mut vault =
		touch_vault::<T>(&collateral_id, &owner, now, None)?.ok_or(Error::<T>::VaultNotFound)?;
	ensure!(
		vault.status::<T>(&collateral_id, &owner).is_final_recovery(),
		Error::<T>::InvalidVaultStatus
	);

	let cfg = branch_cfg_of::<T>(&collateral_id)?;
	let coll = T::CollateralAssets::balance_on_hold(
		collateral_id.clone(),
		&HoldReason::VaultCollateral.into(),
		&owner,
	);
	let total_debt = vault.debt.total();
	let cr = math::collateralization_ratio::<BalanceOf<T>>(coll, total_debt, price)
		.ok_or(Error::<T>::UnsafeCollateralizationRatio)?;
	ensure!(cr >= cfg.minimum_collateralization_ratio, Error::<T>::UnsafeCollateralizationRatio);

	let rejoin_active = total_debt >= cfg.minimum_debt;
	let new_status = if rejoin_active { VaultStatus::Active } else { VaultStatus::Dormant };

	recovery::remove::<T>(&collateral_id, &owner)?;
	// Stamp the snapshot and set redistribution_stake to the
	// current held collateral *before* rejoining recipient accounting.
	let bs_redist = branch_state_of::<T>(&collateral_id)?.redist;
	vault.redist_snapshot = bs_redist;
	vault.redistribution_stake = coll;
	let stake_weighted = vault.annual_rate.saturating_mul_int(coll);
	BranchStates::<T>::try_mutate(&collateral_id, |maybe| -> Result<_, DispatchError> {
		let bs = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
		bs.stakes.total = bs.stakes.total.saturating_add(coll);
		bs.stakes.weighted_sum = bs.stakes.weighted_sum.saturating_add(stake_weighted);
		if !rejoin_active && !total_debt.is_zero() {
			bs.last_dormant_vault_owner = Some(owner.clone());
		}
		Ok(())
	})?;
	Vaults::<T>::insert(&collateral_id, &owner, &vault);
	if rejoin_active {
		T::VaultLists::insert(rate_list_id(&collateral_id), owner.clone(), vault.annual_rate, hint)
			.map_err(|_| Error::<T>::InvalidPositionHints)?;
	}
	Pallet::<T>::deposit_event(Event::VaultStatusChanged {
		collateral_id,
		owner,
		old_status: VaultStatus::FinalRecovery,
		new_status,
	});
	Ok(())
}
/// Refresh the next handful of vaults across each branch using the cursor.
pub fn on_idle_walk<T: Config>(remaining: Weight) -> Weight {
	let per_call = T::WeightInfo::on_idle_one_vault();
	if remaining.any_lt(per_call) {
		return Weight::zero();
	}
	let now = T::TimeProvider::now();
	let mut consumed = Weight::zero();
	let mut budget = T::MaxOnIdleVaultRefresh::get();
	let touch_one = |collateral_id: &T::AssetId, owner: &T::AccountId| -> bool {
		// Returns `true` iff a touch landed (budget consumed); `false` to stop.
		if !Vaults::<T>::contains_key(collateral_id, owner) {
			return true;
		}
		let _ = with_storage_layer(|| touch_vault::<T>(collateral_id, owner, now, None));
		true
	};
	for collateral_id in Branches::<T>::get().iter() {
		if budget == 0 || (remaining.saturating_sub(consumed)).any_lt(per_call) {
			break;
		}
		// Keep oracle-induced Frozen state in sync with the
		// live oracle. Ignore errors: a stuck branch just stays as-is.
		let _ = refresh_branch::<T>(collateral_id);
		// One aggregate-interest mint per branch; touch_vault has no work to do
		// until the branch's `last_interest_update` advances.
		if update_aggregate_interest::<T>(collateral_id, now).is_err() {
			continue;
		}
		let Some(branch) = BranchStates::<T>::get(collateral_id) else { continue };
		let rate_list = rate_list_id(collateral_id);
		let initial_cursor = branch.idle_cursor.or_else(|| T::VaultLists::head(&rate_list));
		let mut cursor = initial_cursor.clone();
		let final_recovery_head = recovery::next_target::<T>(collateral_id);
		let last_dormant = branch.last_dormant_vault_owner;

		while budget > 0 {
			let Some(owner) = cursor.clone() else { break };
			if !touch_one(collateral_id, &owner) {
				break;
			}
			cursor = T::VaultLists::neighbors(&rate_list, &owner).and_then(|p| p.next);
			budget = budget.saturating_sub(1);
			consumed = consumed.saturating_add(per_call);
			if (remaining.saturating_sub(consumed)).any_lt(per_call) {
				break;
			}
		}

		let try_extra = |owner: T::AccountId, budget: &mut u32, consumed: &mut Weight| {
			if *budget == 0 || (remaining.saturating_sub(*consumed)).any_lt(per_call) {
				return;
			}
			if touch_one(collateral_id, &owner) {
				*budget = budget.saturating_sub(1);
				*consumed = consumed.saturating_add(per_call);
			}
		};
		if let Some(owner) = final_recovery_head {
			try_extra(owner, &mut budget, &mut consumed);
		}
		if let Some(owner) = last_dormant {
			try_extra(owner, &mut budget, &mut consumed);
		}

		if cursor != initial_cursor {
			BranchStates::<T>::mutate(collateral_id, |maybe| {
				if let Some(bs) = maybe {
					bs.idle_cursor = cursor.take();
				}
			});
		}
	}
	consumed
}
