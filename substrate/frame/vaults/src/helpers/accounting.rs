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
		.saturating_add(bs.debt.bad_debt);
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
	collateral_id: T::AssetId,
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

/// Issue `amount` pUSD and route per `SpYieldShare`: a portion goes to
/// `T::SpYieldSink`, the residual goes to `T::FeeHandler`.
pub(super) fn mint_and_route_yield<T: Config>(
	collateral_id: T::AssetId,
	amount: BalanceOf<T>,
	source: YieldSource,
) {
	let credit = T::StableAsset::issue(amount);
	let share: Permill = T::SpYieldShare::get();
	let sp_amount = share * credit.peek();
	let (sp_credit, residual) = credit.split(sp_amount);
	if let Err(e) = <T::SpYieldSink as pusd_primitives::OnBranchYield<_, _>>::on_branch_yield(
		collateral_id,
		sp_credit,
	) {
		crate::log!(error, "SpYieldSink rejected {:?}: {:?}", source, e);
	}
	T::FeeHandler::on_unbalanced(residual);
}

/// Touch a vault: bring it forward to `now` (apply pending stored interest,
/// pending redistribution principal/collateral/interest, and re-stamp
/// snapshots). Returns the post-touch vault (or `None` if the row was
/// missing) so callers don't need a follow-up storage read.
///
/// The caller MUST have already called `update_aggregate_interest` for this
/// branch in the same dispatch.
pub fn touch_vault<T: Config>(
	collateral_id: T::AssetId,
	owner: &T::AccountId,
	now: MomentOf<T>,
) -> Result<Option<Vault<BalanceOf<T>, MomentOf<T>>>, DispatchError> {
	let mut vault = match Vaults::<T>::get(collateral_id, owner) {
		Some(v) => v,
		None => return Ok(None),
	};
	let mut bs = branch_state_of::<T>(collateral_id)?;

	// 1. Stored-principal pending interest.
	let elapsed = millis_diff::<T>(now, vault.last_interest_update);
	let principal_interest =
		math::simple_interest_floor(vault.debt.principal, vault.annual_rate, elapsed);

	// 2. Pending redistribution principal/collateral/interest if epoch lags.
	let mut redist_debt_principal = BalanceOf::<T>::zero();
	let mut redist_collat = BalanceOf::<T>::zero();
	let mut redist_interest = BalanceOf::<T>::zero();
	let redist = bs.redist;
	let snap = vault.redist_snapshot;
	if snap != redist {
		let delta_debt_per_stake = redist.debt_per_stake.saturating_sub(snap.debt_per_stake);
		let delta_collat_per_stake = redist.collat_per_stake.saturating_sub(snap.collat_per_stake);
		let delta_dt_per_stake =
			redist.debt_time_per_stake.saturating_sub(snap.debt_time_per_stake);
		// Floor everything for vault attribution. `saturating_mul_int(stake)`
		// computes `floor(per_stake_fixed * stake)` directly into Balance.
		redist_debt_principal = delta_debt_per_stake.saturating_mul_int(vault.redistribution_stake);
		redist_collat = delta_collat_per_stake.saturating_mul_int(vault.redistribution_stake);
		// Interest accrued on redistributed debt from the time it was
		// liquidated up to `now`. Approximated by:
		// `delta_debt_per_stake * stake * annual_rate * (now - last_redist) / year`.
		// Since we only track cumulative debt_time_per_stake, we get the
		// area-under-the-curve per stake directly.
		// Round down for vault attribution, dust stays in branch aggregates.
		let now_fp = FixedU128::saturating_from_integer(moment_to_millis::<T>(now));
		let extra_per_stake =
			now_fp.saturating_mul(delta_debt_per_stake).saturating_sub(delta_dt_per_stake);
		// MILLIS_PER_YEAR is a non-zero compile-time constant, so `checked_div`
		// can only return `None` on overflow.
		let rate_factor = vault
			.annual_rate
			.checked_div(&FixedU128::saturating_from_integer(pusd_primitives::MILLIS_PER_YEAR))
			.defensive_unwrap_or_else(FixedU128::zero);
		redist_interest = extra_per_stake
			.saturating_mul(rate_factor)
			.saturating_mul_int(vault.redistribution_stake);
	}

	// 3. Apply.
	let total_pending_interest = principal_interest.saturating_add(redist_interest);
	if !total_pending_interest.is_zero() {
		vault.debt.interest = vault.debt.interest.saturating_add(total_pending_interest);
		Pallet::<T>::deposit_event(Event::InterestAccrued {
			collateral_id,
			owner: owner.clone(),
			amount: total_pending_interest,
		});
	}
	if !redist_debt_principal.is_zero() {
		// Move principal from pending to interest_bearing.
		let actual = core::cmp::min(redist_debt_principal, bs.debt.pending_redist_principal);
		bs.debt.pending_redist_principal = bs.debt.pending_redist_principal.saturating_sub(actual);
		bs.debt.principal = bs.debt.principal.saturating_add(actual);
		// Reconcile this vault's share of the avg-rate weighted contribution
		// that was folded into the branch interest base at liquidation. Subtract
		// the avg-rate share, add the recipient-rate share.
		let delta_weight_per_stake = redist.weight_per_stake.saturating_sub(snap.weight_per_stake);
		let weight_to_remove =
			delta_weight_per_stake.saturating_mul_int(vault.redistribution_stake);
		let weight_to_add = vault.annual_rate.saturating_mul_int(actual);
		bs.debt.weighted_principal_sum = bs
			.debt
			.weighted_principal_sum
			.saturating_sub(weight_to_remove)
			.saturating_add(weight_to_add);
		vault.debt.principal = vault.debt.principal.saturating_add(actual);
	}
	if !redist_collat.is_zero() {
		T::CollateralAssets::transfer_on_hold(
			collateral_id,
			&HoldReason::VaultCollateral.into(),
			&Pallet::<T>::redistribution_account(),
			owner,
			redist_collat,
			Precision::Exact,
			Restriction::OnHold,
			Fortitude::Polite,
		)?;
		bs.total_collateral = bs.total_collateral.saturating_add(redist_collat);
	}

	if snap != redist {
		vault.redist_snapshot = redist;
	}
	vault.last_interest_update = now;

	// FinalRecovery exit is not auto-applied here; callers (typically the
	// dedicated `exit_final_recovery` extrinsic) own the rate-index reinsertion
	// so they can supply the O(1) hints.
	Vaults::<T>::insert(collateral_id, owner, &vault);
	BranchStates::<T>::insert(collateral_id, &bs);
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
