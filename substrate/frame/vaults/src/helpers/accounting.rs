use super::*;

/// Compute TCR including aggregate interest accrued since the last update.
pub fn compute_tcr<T: Config>(
	bs: &BranchState<T::AccountId, BalanceOf<T>, MomentOf<T>>,
	price: FixedU128,
	now: MomentOf<T>,
) -> Result<FixedU128, DispatchError> {
	let elapsed = millis_diff::<T>(now, bs.debt.last_interest_update);
	let pending_aggregate =
		math::simple_interest_ceil(bs.debt.weighted_principal_sum, FixedU128::one(), elapsed);
	let total_debt = bs
		.debt
		.principal
		.saturating_add(bs.debt.minted_interest)
		.saturating_add(pending_aggregate)
		.saturating_add(bs.debt.pending_redist_principal)
		.saturating_add(bs.debt.bad_debt)
		.saturating_add(bs.rounding.ownerless_pusd_debt);
	if total_debt.is_zero() {
		// Branch with no debt is treated as "infinitely well-collateralized".
		return Ok(FixedU128::max_value());
	}
	let value = price
		.checked_mul_int(bs.total_collateral)
		.ok_or(Error::<T>::ArithmeticOverflow)?;
	FixedU128::checked_from_rational(value, total_debt)
		.ok_or_else(|| Error::<T>::ArithmeticOverflow.into())
}

/// Mint and route newly accrued aggregate interest. After this call, the
/// branch debt's `last_interest_update` is `now` and its `minted_interest`
/// reflects the freshly minted total.
pub fn update_aggregate_interest<T: Config>(
	collateral_id: &T::AssetId,
	now: MomentOf<T>,
) -> Result<(), DispatchError> {
	BranchStates::<T>::try_mutate(collateral_id, |maybe| -> Result<_, DispatchError> {
		let bs = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
		// Frozen branches do not accrue further aggregate interest.
		if bs.is_frozen() {
			bs.debt.last_interest_update = now;
			return Ok(());
		}
		let elapsed = millis_diff::<T>(now, bs.debt.last_interest_update);
		if elapsed == 0 {
			return Ok(());
		}
		let new_interest =
			math::simple_interest_ceil(bs.debt.weighted_principal_sum, FixedU128::one(), elapsed);
		bs.debt.last_interest_update = now;
		if new_interest.is_zero() {
			return Ok(());
		}
		bs.debt.minted_interest = bs.debt.minted_interest.saturating_add(new_interest);
		mint_and_route_yield::<T>(collateral_id, new_interest, YieldSource::BranchInterest);
		Ok(())
	})
}

/// Origin of a pUSD yield credit routed by [`mint_and_route_yield`].
#[derive(Debug, Clone, Copy)]
pub(super) enum YieldSource {
	/// Aggregate branch interest minted in `update_aggregate_interest`.
	BranchInterest,
	/// Upfront fee charged on borrow / change-rate.
	UpfrontFee,
}

/// Mint and route an upfront fee, then emit `UpfrontFeeCharged`. No-op when
/// `amount == 0`.
pub(super) fn charge_upfront_fee<T: Config>(
	collateral_id: &T::AssetId,
	owner: &T::AccountId,
	amount: BalanceOf<T>,
) {
	if amount.is_zero() {
		return;
	}
	mint_and_route_yield::<T>(collateral_id, amount, YieldSource::UpfrontFee);
	Pallet::<T>::deposit_event(Event::UpfrontFeeCharged {
		collateral_id: collateral_id.clone(),
		owner: owner.clone(),
		amount,
	});
}

/// Issue `amount` pUSD and route per `SpYieldShare`: a portion goes to
/// `T::SpYieldSink`, the residual goes to `T::FeeHandler`.
pub(super) fn mint_and_route_yield<T: Config>(
	collateral_id: &T::AssetId,
	amount: BalanceOf<T>,
	source: YieldSource,
) {
	let credit = T::StableAsset::issue(amount);
	let share: Permill = T::SpYieldShare::get();
	let sp_amount = share * credit.peek();
	let (sp_credit, residual) = credit.split(sp_amount);
	if let Err(e) = <T::SpYieldSink as pusd_primitives::OnBranchYield<_, _>>::on_branch_yield(
		collateral_id.clone(),
		sp_credit,
	) {
		crate::log!(error, "SpYieldSink rejected {:?}: {:?}", source, e);
	}
	T::FeeHandler::on_unbalanced(residual);
}

/// Pending touch values for a vault: the deltas the next `touch_vault` would
/// apply. Used by `touch_vault` for the write path and by view helpers to
/// project the post-touch state without mutating storage.
pub(crate) struct PendingTouch<Balance> {
	/// Capped redistributed principal moved into `vault.debt.principal`
	/// (and out of `bs.debt.pending_redist_principal`).
	pub principal: Balance,
	/// Redistributed collateral released to the owner's hold.
	pub collateral: Balance,
	/// Stored-principal pending interest plus redistribution interest, both
	/// folded into `vault.debt.interest`.
	pub interest: Balance,
}

/// Compute the pending touch deltas for `vault` against `bs` at time `now`.
/// Mirrors `touch_vault`'s on-write math but without storage mutation, so view
/// helpers can return the same post-touch numbers the runtime would compute on
/// the next write.
pub(crate) fn pending_touch_for<T: Config>(
	vault: &Vault<BalanceOf<T>, MomentOf<T>>,
	bs: &BranchState<T::AccountId, BalanceOf<T>, MomentOf<T>>,
	now: MomentOf<T>,
) -> PendingTouch<BalanceOf<T>> {
	let elapsed = millis_diff::<T>(now, vault.last_interest_update);
	let principal_interest =
		math::simple_interest_floor(vault.debt.principal, vault.annual_rate, elapsed);

	let redist = bs.redist;
	let snap = vault.redist_snapshot;
	if snap == redist {
		return PendingTouch {
			principal: BalanceOf::<T>::zero(),
			collateral: BalanceOf::<T>::zero(),
			interest: principal_interest,
		};
	}

	let delta_debt_per_stake = redist.debt_per_stake.saturating_sub(snap.debt_per_stake);
	let delta_collat_per_stake = redist.collat_per_stake.saturating_sub(snap.collat_per_stake);
	let delta_dt_per_stake = redist.debt_time_per_stake.saturating_sub(snap.debt_time_per_stake);
	// Floor for vault attribution. `saturating_mul_int(stake)` computes
	// `floor(per_stake_fixed * stake)` directly into `Balance`. Cap the
	// principal to what the branch counter still holds; rounding dust stays
	// in branch aggregates.
	let raw_principal = delta_debt_per_stake.saturating_mul_int(vault.redistribution_stake);
	let principal = core::cmp::min(raw_principal, bs.debt.pending_redist_principal);
	let collateral = delta_collat_per_stake.saturating_mul_int(vault.redistribution_stake);
	// `(now - last_redist) * delta_debt_per_stake - delta_debt_time_per_stake`
	// is the area-under-the-curve giving redistributed-debt × time-since
	// per stake. Multiply by `rate / year` for the interest accrued on the
	// redistributed principal since redistribution.
	let now_fp = FixedU128::saturating_from_integer(moment_to_millis::<T>(now));
	let extra_per_stake =
		now_fp.saturating_mul(delta_debt_per_stake).saturating_sub(delta_dt_per_stake);
	let rate_factor = vault
		.annual_rate
		.checked_div(&FixedU128::saturating_from_integer(pusd_primitives::MILLIS_PER_YEAR))
		.defensive_unwrap_or_else(FixedU128::zero);
	let redist_interest = extra_per_stake
		.saturating_mul(rate_factor)
		.saturating_mul_int(vault.redistribution_stake);

	PendingTouch {
		principal,
		collateral,
		interest: principal_interest.saturating_add(redist_interest),
	}
}

/// Touch a vault: bring it forward to `now` (apply pending stored interest,
/// pending redistribution principal/collateral/interest, and re-stamp
/// snapshots). Returns the post-touch vault (or `None` if the row was
/// missing) so callers don't need a follow-up storage read.
///
/// The caller MUST have already called `update_aggregate_interest` for this
/// branch in the same dispatch.
///
/// `hint` is consulted only when a Dormant vault's freshly accrued debt has
/// risen above `MinimumDebt` and needs to rejoin the rate index. `None`
/// triggers an unhinted `find_position` walk.
#[require_transactional]
pub fn touch_vault<T: Config>(
	collateral_id: &T::AssetId,
	owner: &T::AccountId,
	now: MomentOf<T>,
	hint: Option<Position<T::AccountId>>,
) -> Result<Option<Vault<BalanceOf<T>, MomentOf<T>>>, DispatchError> {
	let mut vault = match Vaults::<T>::get(collateral_id, owner) {
		Some(v) => v,
		None => return Ok(None),
	};
	let pre_status = vault.status::<T>(collateral_id, owner);
	let mut bs = branch_state_of::<T>(collateral_id)?;
	let pending = pending_touch_for::<T>(&vault, &bs, now);

	if !pending.interest.is_zero() {
		vault.debt.interest = vault.debt.interest.saturating_add(pending.interest);
		Pallet::<T>::deposit_event(Event::InterestAccrued {
			collateral_id: collateral_id.clone(),
			owner: owner.clone(),
			amount: pending.interest,
		});
	}
	if !pending.principal.is_zero() {
		bs.debt.pending_redist_principal =
			bs.debt.pending_redist_principal.saturating_sub(pending.principal);
		bs.debt.principal = bs.debt.principal.saturating_add(pending.principal);
		// Reconcile this vault's share of the avg-rate weighted contribution
		// that was folded into the branch interest base at liquidation. Subtract
		// the avg-rate share, add the recipient-rate share.
		let delta_weight_per_stake = bs
			.redist
			.weight_per_stake
			.saturating_sub(vault.redist_snapshot.weight_per_stake);
		let weight_to_remove =
			delta_weight_per_stake.saturating_mul_int(vault.redistribution_stake);
		let weight_to_add = vault.annual_rate.saturating_mul_int(pending.principal);
		bs.debt.weighted_principal_sum = bs
			.debt
			.weighted_principal_sum
			.saturating_sub(weight_to_remove)
			.saturating_add(weight_to_add);
		vault.debt.principal = vault.debt.principal.saturating_add(pending.principal);
	}
	if !pending.collateral.is_zero() {
		// The collateral was already part of `bs.total_collateral` at
		// `finalize_liquidation` time; this only moves it from the
		// redistribution account onto the owner's hold.
		T::CollateralAssets::transfer_on_hold(
			collateral_id.clone(),
			&HoldReason::VaultCollateral.into(),
			&Pallet::<T>::redistribution_account(),
			owner,
			pending.collateral,
			Precision::Exact,
			Restriction::OnHold,
			Fortitude::Polite,
		)?;
	}

	if vault.redist_snapshot != bs.redist {
		vault.redist_snapshot = bs.redist;
	}
	vault.last_interest_update = now;

	// FinalRecovery vaults are excluded from stake accounting entirely; their
	// `redistribution_stake` is zeroed on entry and stays zero until they exit.
	if !pre_status.is_final_recovery() {
		let new_held = T::CollateralAssets::balance_on_hold(
			collateral_id.clone(),
			&HoldReason::VaultCollateral.into(),
			owner,
		);
		if vault.redistribution_stake != new_held {
			bs.refresh_vault_stake(vault.annual_rate, vault.redistribution_stake, new_held);
			vault.redistribution_stake = new_held;
		}
	}

	let revived_dormant = if pre_status.is_dormant() {
		let cfg = branch_cfg_of::<T>(collateral_id)?;
		if vault.debt.total() >= cfg.minimum_debt {
			let position = hint.unwrap_or_else(|| {
				T::VaultLists::find_position(&rate_list_id(collateral_id), vault.annual_rate)
			});
			T::VaultLists::insert(
				rate_list_id(collateral_id),
				owner.clone(),
				vault.annual_rate,
				position,
			)
			.map_err(|_| Error::<T>::InvalidPositionHints)?;
			if bs.last_dormant_vault_owner.as_ref() == Some(owner) {
				bs.last_dormant_vault_owner = None;
			}
			true
		} else {
			false
		}
	} else {
		false
	};

	Vaults::<T>::insert(collateral_id, owner, &vault);
	BranchStates::<T>::insert(collateral_id, &bs);

	if revived_dormant {
		Pallet::<T>::deposit_event(Event::VaultStatusChanged {
			collateral_id: collateral_id.clone(),
			owner: owner.clone(),
			old_status: VaultStatus::Dormant,
			new_status: VaultStatus::Active,
		});
	}

	Ok(Some(vault))
}

/// Compute the upfront fee for opening a vault: charged on the new debt at
/// the post-change average branch rate.
pub fn open_upfront_fee<T: Config>(
	bs: &BranchState<T::AccountId, BalanceOf<T>, MomentOf<T>>,
	cfg: &BranchConfig<BalanceOf<T>, MomentOf<T>>,
	new_debt: BalanceOf<T>,
	new_rate: FixedU128,
) -> BalanceOf<T> {
	let total_ib = bs
		.debt
		.principal
		.saturating_add(bs.debt.pending_redist_principal)
		.saturating_add(new_debt);
	let weighted = bs
		.debt
		.weighted_principal_sum
		.saturating_add(new_rate.saturating_mul_int(new_debt));
	let avg = math::average_branch_rate(weighted, total_ib);
	math::simple_interest_ceil(new_debt, avg, moment_to_millis::<T>(cfg.upfront_fee_period))
}

/// Simulate a `borrow` (which optionally adjusts the rate) and return the
/// `(post-state, upfront-fee)` pair. Shared by the live extrinsic and the
/// `predict_upfront_fee_borrow` view so the two can't drift.
///
/// `rate_change_fee_base` is the existing principal that the rate-change
/// component of the upfront fee is charged against (zero when the call is a
/// pure debt increase or the cooldown has elapsed).
pub(super) fn simulate_borrow<T: Config>(
	bs: &BranchState<T::AccountId, BalanceOf<T>, MomentOf<T>>,
	cfg: &BranchConfig<BalanceOf<T>, MomentOf<T>>,
	vault: &Vault<BalanceOf<T>, MomentOf<T>>,
	debt_increase: BalanceOf<T>,
	new_rate: FixedU128,
	rate_change_fee_base: BalanceOf<T>,
) -> (BranchState<T::AccountId, BalanceOf<T>, MomentOf<T>>, BalanceOf<T>) {
	let mut bs_after = bs.clone();
	bs_after.debt.principal = bs.debt.principal.saturating_add(debt_increase);
	let weighted_old = vault.annual_rate.saturating_mul_int(vault.debt.principal);
	let weighted_new =
		new_rate.saturating_mul_int(vault.debt.principal.saturating_add(debt_increase));
	bs_after.debt.weighted_principal_sum = bs
		.debt
		.weighted_principal_sum
		.saturating_sub(weighted_old)
		.saturating_add(weighted_new);
	if new_rate != vault.annual_rate {
		let stake_w_old = vault.annual_rate.saturating_mul_int(vault.redistribution_stake);
		let stake_w_new = new_rate.saturating_mul_int(vault.redistribution_stake);
		bs_after.stakes.weighted_sum =
			bs.stakes.weighted_sum.saturating_sub(stake_w_old).saturating_add(stake_w_new);
	}
	let avg = math::average_branch_rate(
		bs_after.debt.weighted_principal_sum,
		bs_after.debt.principal.saturating_add(bs_after.debt.pending_redist_principal),
	);
	let fee = math::simple_interest_ceil(
		debt_increase.saturating_add(rate_change_fee_base),
		avg,
		moment_to_millis::<T>(cfg.upfront_fee_period),
	);
	(bs_after, fee)
}

/// Simulate a `change_rate` and return the `(post-state, upfront-fee)` pair.
/// Shared by the live extrinsic and `predict_upfront_fee_rate_change`.
pub(super) fn simulate_change_rate<T: Config>(
	bs: &BranchState<T::AccountId, BalanceOf<T>, MomentOf<T>>,
	cfg: &BranchConfig<BalanceOf<T>, MomentOf<T>>,
	vault: &Vault<BalanceOf<T>, MomentOf<T>>,
	new_rate: FixedU128,
	cooldown_elapsed: bool,
) -> (BranchState<T::AccountId, BalanceOf<T>, MomentOf<T>>, BalanceOf<T>) {
	let mut bs_after = bs.clone();
	bs_after.change_vault_rate(
		vault.annual_rate,
		new_rate,
		vault.debt.principal,
		vault.redistribution_stake,
	);
	let fee = if cooldown_elapsed {
		BalanceOf::<T>::zero()
	} else {
		let avg = math::average_branch_rate(
			bs_after.debt.weighted_principal_sum,
			bs_after.debt.principal.saturating_add(bs_after.debt.pending_redist_principal),
		);
		math::simple_interest_ceil(
			vault.debt.principal,
			avg,
			moment_to_millis::<T>(cfg.upfront_fee_period),
		)
	};
	(bs_after, fee)
}
