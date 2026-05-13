//! Storage-touching helpers: vault lifecycle, branch mode, interest,
//! redistribution, fees, governance, on-idle.
//!
//! Most extrinsics in `lib.rs` are thin wrappers over these.

use crate::{
	math,
	pallet::{
		BalanceOf, BranchConfigs, BranchRedistStates, BranchStates, Branches, Config, Error, Event,
		HoldReason, MomentOf, OnIdleCursor, Pallet, VaultRedistSnapshots, Vaults,
	},
	recovery,
	types::{
		BranchConfig, BranchMode, BranchState, FrozenReason, FrozenState, Vault,
		VaultRedistSnapshot, VaultStatus,
	},
	weights::WeightInfo,
};
use alloc::vec::Vec;
use frame::{
	deps::{
		frame_support::{
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
use pallet_linked_list::{Position, SortedListInterface};
use pusd_primitives::{PriceFeed, ProvidePrice};

fn moment_to_millis<T: Config>(m: MomentOf<T>) -> u64 {
	use frame::deps::sp_runtime::traits::SaturatedConversion;
	m.saturated_into::<u64>()
}

fn millis_diff<T: Config>(now: MomentOf<T>, then: MomentOf<T>) -> u64 {
	moment_to_millis::<T>(now.saturating_sub(then))
}

/// Pull collateral on hold from the storage layer.
fn held_collateral<T: Config>(collateral_id: T::AssetId, owner: &T::AccountId) -> BalanceOf<T> {
	T::CollateralAssets::balance_on_hold(collateral_id, &HoldReason::VaultCollateral.into(), owner)
}

/// Read the live oracle price for `collateral_id`, returning the protocol
/// error variants for stale/missing prices.
fn oracle_price<T: Config>(
	collateral_id: T::AssetId,
) -> Result<PriceFeed<MomentOf<T>>, DispatchError> {
	T::Oracle::provide_price(&collateral_id)
}

/// Read the branch state, returning `UnknownCollateral` when missing.
pub(crate) fn branch_state_of<T: Config>(
	collateral_id: T::AssetId,
) -> Result<BranchState<T::AccountId, BalanceOf<T>, MomentOf<T>>, DispatchError> {
	BranchStates::<T>::get(collateral_id).ok_or_else(|| Error::<T>::UnknownCollateral.into())
}

/// Read the branch config, returning `UnknownCollateral` when missing.
pub(crate) fn branch_cfg_of<T: Config>(
	collateral_id: T::AssetId,
) -> Result<BranchConfig<BalanceOf<T>, MomentOf<T>>, DispatchError> {
	BranchConfigs::<T>::get(collateral_id).ok_or_else(|| Error::<T>::UnknownCollateral.into())
}

/// Read a vault row, returning `VaultNotFound` when missing.
pub(crate) fn vault_of<T: Config>(
	collateral_id: T::AssetId,
	owner: &T::AccountId,
) -> Result<Vault<BalanceOf<T>, MomentOf<T>>, DispatchError> {
	Vaults::<T>::get(collateral_id, owner).ok_or_else(|| Error::<T>::VaultNotFound.into())
}

/// Mode is `Frozen` if persisted, otherwise derived from live TCR.
pub fn current_mode<T: Config>(collateral_id: &T::AssetId) -> Result<BranchMode, DispatchError> {
	let bs = branch_state_of::<T>(*collateral_id)?;
	if bs.is_frozen() {
		return Ok(BranchMode::Frozen);
	}
	// Try to read price; mode without a price falls back to `Normal`. The
	// caller is expected to gate state-changing ops on a fresh price first.
	let price = match T::Oracle::provide_price(collateral_id) {
		Ok(feed) => feed.price,
		Err(_) => return Ok(BranchMode::Normal),
	};
	let cfg = branch_cfg_of::<T>(*collateral_id)?;
	let now = T::TimeProvider::now();
	let tcr = compute_tcr::<T>(&bs, price, now)?;
	if tcr < cfg.safety_collateralization_ratio {
		Ok(BranchMode::Safety)
	} else {
		Ok(BranchMode::Normal)
	}
}

/// Compute TCR including aggregate interest accrued since the last update,
/// mirroring §7.4.
pub fn compute_tcr<T: Config>(
	bs: &BranchState<T::AccountId, BalanceOf<T>, MomentOf<T>>,
	price: FixedU128,
	now: MomentOf<T>,
) -> Result<FixedU128, DispatchError> {
	let elapsed = millis_diff::<T>(now, bs.last_aggregate_interest_update);
	let pending_aggregate = math::simple_interest_ceil(
		bs.weighted_interest_bearing_debt_sum,
		FixedU128::one(),
		elapsed,
	);
	let total_debt = bs
		.total_interest_bearing_debt
		.saturating_add(bs.total_minted_aggregate_interest)
		.saturating_add(pending_aggregate)
		.saturating_add(bs.pending_redistribution_debt)
		.saturating_add(bs.bad_debt);
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
/// branch's `last_aggregate_interest_update` is `now` and its
/// `total_minted_aggregate_interest` reflects the freshly minted total.
pub fn update_aggregate_interest<T: Config>(
	collateral_id: T::AssetId,
	now: MomentOf<T>,
) -> Result<(), DispatchError> {
	BranchStates::<T>::try_mutate(collateral_id, |maybe| -> Result<_, DispatchError> {
		let bs = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
		// Frozen branches do not accrue further aggregate interest.
		if bs.is_frozen() {
			bs.last_aggregate_interest_update = now;
			return Ok(());
		}
		let elapsed = millis_diff::<T>(now, bs.last_aggregate_interest_update);
		if elapsed == 0 {
			return Ok(());
		}
		let new_interest = math::simple_interest_ceil(
			bs.weighted_interest_bearing_debt_sum,
			FixedU128::one(),
			elapsed,
		);
		bs.last_aggregate_interest_update = now;
		if new_interest.is_zero() {
			return Ok(());
		}
		bs.total_minted_aggregate_interest =
			bs.total_minted_aggregate_interest.saturating_add(new_interest);
		mint_and_route_yield::<T>(collateral_id, new_interest, YieldSource::BranchInterest);
		Ok(())
	})
}

/// Origin of a pUSD yield credit routed by [`mint_and_route_yield`].
#[derive(Debug, Clone, Copy)]
enum YieldSource {
	/// Aggregate branch interest minted in `update_aggregate_interest`.
	BranchInterest,
	/// Upfront fee charged on borrow / change-rate.
	UpfrontFee,
}

/// Issue `amount` pUSD and route per `SpYieldShare`: a portion goes to
/// `T::SpYieldSink`, the residual goes to `T::FeeHandler`.
fn mint_and_route_yield<T: Config>(
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
	let mut redist =
		BranchRedistStates::<T>::get(collateral_id).ok_or(Error::<T>::UnknownCollateral)?;

	// 1. Stored-principal pending interest.
	let elapsed = millis_diff::<T>(now, vault.last_interest_update);
	let principal_interest =
		math::simple_interest_floor(vault.interest_bearing_debt, vault.annual_rate, elapsed);

	// 2. Pending redistribution principal/collateral/interest if epoch lags.
	let mut redist_debt_principal = BalanceOf::<T>::zero();
	let mut redist_collat = BalanceOf::<T>::zero();
	let mut redist_interest = BalanceOf::<T>::zero();
	if vault.redist_epoch != bs.redist_epoch {
		let snap = VaultRedistSnapshots::<T>::get(collateral_id, owner).unwrap_or_default();
		let delta_debt_per_stake =
			redist.cumulative_redist_debt_per_stake.saturating_sub(snap.debt_per_stake);
		let delta_collat_per_stake =
			redist.cumulative_redist_collat_per_stake.saturating_sub(snap.collat_per_stake);
		let delta_dt_per_stake = redist
			.cumulative_redist_debt_time_per_stake
			.saturating_sub(snap.debt_time_per_stake);
		// Floor everything for vault attribution. `saturating_mul_int(stake)`
		// computes `floor(per_stake_fixed * stake)` directly into Balance.
		redist_debt_principal = delta_debt_per_stake.saturating_mul_int(vault.stake);
		redist_collat = delta_collat_per_stake.saturating_mul_int(vault.stake);
		// Interest accrued on redistributed debt from the time it was
		// liquidated up to `now`. Approximated by:
		// `delta_debt_per_stake * stake * annual_rate * (now - last_redist) / year`.
		// Since we only track cumulative_redist_debt_time_per_stake, we get the
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
		redist_interest =
			extra_per_stake.saturating_mul(rate_factor).saturating_mul_int(vault.stake);
	}

	// 3. Apply.
	let total_pending_interest = principal_interest.saturating_add(redist_interest);
	if !total_pending_interest.is_zero() {
		vault.accrued_interest = vault.accrued_interest.saturating_add(total_pending_interest);
		Pallet::<T>::deposit_event(Event::InterestAccrued {
			collateral_id,
			owner: owner.clone(),
			amount: total_pending_interest,
		});
	}
	if !redist_debt_principal.is_zero() {
		// Move principal from pending to interest_bearing.
		let actual = core::cmp::min(redist_debt_principal, bs.pending_redistribution_debt);
		bs.pending_redistribution_debt = bs.pending_redistribution_debt.saturating_sub(actual);
		bs.total_interest_bearing_debt = bs.total_interest_bearing_debt.saturating_add(actual);
		// Reconcile this vault's share of the avg-rate weighted contribution
		// that was folded into the branch interest base at liquidation. Subtract
		// the avg-rate share, add the recipient-rate share.
		let snap = VaultRedistSnapshots::<T>::get(collateral_id, owner).unwrap_or_default();
		let delta_weight_per_stake =
			redist.cumulative_redist_weight_per_stake.saturating_sub(snap.weight_per_stake);
		let weight_to_remove = delta_weight_per_stake.saturating_mul_int(vault.stake);
		let weight_to_add = vault.annual_rate.saturating_mul_int(actual);
		bs.weighted_interest_bearing_debt_sum = bs
			.weighted_interest_bearing_debt_sum
			.saturating_sub(weight_to_remove)
			.saturating_add(weight_to_add);
		vault.interest_bearing_debt = vault.interest_bearing_debt.saturating_add(actual);
	}
	if !redist_collat.is_zero() {
		// Transfer collateral on hold from the pallet redistribution account
		// to the owner. `Restriction::OnHold` keeps the receiver's hold
		// aligned with the vault's collateral.
		let actual = T::CollateralAssets::transfer_on_hold(
			collateral_id,
			&HoldReason::VaultCollateral.into(),
			&Pallet::<T>::redistribution_account(),
			owner,
			redist_collat,
			Precision::BestEffort,
			Restriction::OnHold,
			Fortitude::Polite,
		)
		.defensive_unwrap_or(BalanceOf::<T>::zero());
		bs.total_collateral = bs.total_collateral.saturating_add(actual);
	}

	// 4. Re-stamp snapshot and timestamps.
	if vault.redist_epoch != bs.redist_epoch {
		vault.redist_epoch = bs.redist_epoch;
		VaultRedistSnapshots::<T>::insert(
			collateral_id,
			owner,
			VaultRedistSnapshot {
				collat_per_stake: redist.cumulative_redist_collat_per_stake,
				debt_per_stake: redist.cumulative_redist_debt_per_stake,
				debt_time_per_stake: redist.cumulative_redist_debt_time_per_stake,
				weight_per_stake: redist.cumulative_redist_weight_per_stake,
			},
		);
		// Re-stamp doesn't change `redist`'s accumulators; re-fetching is a
		// no-op but keeps the local copy in sync if other paths run later.
		// `BranchRedistStates::get` returning `None` here would mean the row
		// was deleted under us — defensively fall back to the local copy.
		redist = BranchRedistStates::<T>::get(collateral_id).defensive_unwrap_or(redist);
	}
	vault.last_interest_update = now;

	// 5. Persist. FinalRecovery exit is no longer auto-applied here; callers
	// (typically the dedicated `exit_final_recovery` extrinsic) own the
	// rate-index reinsertion so the caller supplies the O(1) hints.
	Vaults::<T>::insert(collateral_id, owner, &vault);
	BranchStates::<T>::insert(collateral_id, &bs);
	BranchRedistStates::<T>::insert(collateral_id, &redist);
	Ok(Some(vault))
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

/// Compute the upfront fee for opening a vault: charged on the new debt at
/// the post-change average branch rate.
pub fn open_upfront_fee<T: Config>(
	bs: &BranchState<T::AccountId, BalanceOf<T>, MomentOf<T>>,
	cfg: &BranchConfig<BalanceOf<T>, MomentOf<T>>,
	new_debt: BalanceOf<T>,
	new_rate: FixedU128,
) -> BalanceOf<T> {
	let total_ib = bs
		.total_interest_bearing_debt
		.saturating_add(bs.pending_redistribution_debt)
		.saturating_add(new_debt);
	let weighted = bs
		.weighted_interest_bearing_debt_sum
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
fn simulate_borrow<T: Config>(
	bs: &BranchState<T::AccountId, BalanceOf<T>, MomentOf<T>>,
	cfg: &BranchConfig<BalanceOf<T>, MomentOf<T>>,
	vault: &Vault<BalanceOf<T>, MomentOf<T>>,
	debt_increase: BalanceOf<T>,
	new_rate: FixedU128,
	rate_change_fee_base: BalanceOf<T>,
) -> (BranchState<T::AccountId, BalanceOf<T>, MomentOf<T>>, BalanceOf<T>) {
	let mut bs_after = bs.clone();
	bs_after.total_interest_bearing_debt =
		bs.total_interest_bearing_debt.saturating_add(debt_increase);
	let weighted_old = vault.annual_rate.saturating_mul_int(vault.interest_bearing_debt);
	let weighted_new =
		new_rate.saturating_mul_int(vault.interest_bearing_debt.saturating_add(debt_increase));
	bs_after.weighted_interest_bearing_debt_sum = bs
		.weighted_interest_bearing_debt_sum
		.saturating_sub(weighted_old)
		.saturating_add(weighted_new);
	let avg = math::average_branch_rate(
		bs_after.weighted_interest_bearing_debt_sum,
		bs_after
			.total_interest_bearing_debt
			.saturating_add(bs_after.pending_redistribution_debt),
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
fn simulate_change_rate<T: Config>(
	bs: &BranchState<T::AccountId, BalanceOf<T>, MomentOf<T>>,
	cfg: &BranchConfig<BalanceOf<T>, MomentOf<T>>,
	vault: &Vault<BalanceOf<T>, MomentOf<T>>,
	new_rate: FixedU128,
	cooldown_elapsed: bool,
) -> (BranchState<T::AccountId, BalanceOf<T>, MomentOf<T>>, BalanceOf<T>) {
	let mut bs_after = bs.clone();
	bs_after.weighted_interest_bearing_debt_sum = bs_after
		.weighted_interest_bearing_debt_sum
		.saturating_sub(vault.annual_rate.saturating_mul_int(vault.interest_bearing_debt))
		.saturating_add(new_rate.saturating_mul_int(vault.interest_bearing_debt));
	bs_after.weighted_stake_sum = bs_after
		.weighted_stake_sum
		.saturating_sub(vault.annual_rate.saturating_mul_int(vault.stake))
		.saturating_add(new_rate.saturating_mul_int(vault.stake));
	let fee = if cooldown_elapsed {
		BalanceOf::<T>::zero()
	} else {
		let avg = math::average_branch_rate(
			bs_after.weighted_interest_bearing_debt_sum,
			bs_after
				.total_interest_bearing_debt
				.saturating_add(bs_after.pending_redistribution_debt),
		);
		math::simple_interest_ceil(
			vault.interest_bearing_debt,
			avg,
			moment_to_millis::<T>(cfg.upfront_fee_period),
		)
	};
	(bs_after, fee)
}

pub fn open_vault<T: Config>(
	owner: T::AccountId,
	collateral_id: T::AssetId,
	initial_collateral: BalanceOf<T>,
	initial_debt: BalanceOf<T>,
	annual_rate: FixedU128,
	hint: Position<T::AccountId>,
) -> Result<(), DispatchError> {
	ensure_not_frozen::<T>(collateral_id)?;
	ensure!(!Vaults::<T>::contains_key(collateral_id, &owner), Error::<T>::VaultAlreadyExists);
	let cfg = branch_cfg_of::<T>(collateral_id)?;
	ensure!(initial_debt >= cfg.minimum_debt, Error::<T>::DebtBelowMinimum);
	ensure!(initial_collateral >= cfg.minimum_collateral, Error::<T>::InsufficientCollateral);
	validate_rate::<T>(&cfg, annual_rate)?;

	let now = T::TimeProvider::now();
	update_aggregate_interest::<T>(collateral_id, now)?;

	// Hold collateral on the owner's account.
	T::CollateralAssets::hold(
		collateral_id,
		&HoldReason::VaultCollateral.into(),
		&owner,
		initial_collateral,
	)?;

	// Compute upfront fee and check ceiling.
	let bs_before = branch_state_of::<T>(collateral_id)?;
	let new_total_ib = bs_before.total_interest_bearing_debt.saturating_add(initial_debt);
	ensure!(new_total_ib <= cfg.debt_ceiling, Error::<T>::DebtCeilingExceeded);

	let upfront_fee = open_upfront_fee::<T>(&bs_before, &cfg, initial_debt, annual_rate);

	let redist =
		BranchRedistStates::<T>::get(collateral_id).ok_or(Error::<T>::UnknownCollateral)?;
	// Stake at open is the initial collateral; it stays frozen through the
	// vault's lifetime so the per-stake redistribution math is internally
	// consistent with `bs.total_stakes`. (See `Vault.stake` doc.)
	let stake = initial_collateral;

	// Build vault.
	let vault = Vault {
		status: VaultStatus::Active,
		interest_bearing_debt: initial_debt,
		accrued_interest: upfront_fee,
		annual_rate,
		last_interest_update: now,
		last_rate_update: now,
		stake,
		redist_epoch: bs_before.redist_epoch,
	};

	// CR/ICR check (use post-state).
	let price = oracle_price::<T>(collateral_id)?.price;
	let total_debt = initial_debt.saturating_add(upfront_fee);
	let cr = math::collateralization_ratio::<BalanceOf<T>>(initial_collateral, total_debt, price)
		.ok_or(Error::<T>::UnsafeCollateralizationRatio)?;
	ensure!(cr >= cfg.initial_collateralization_ratio, Error::<T>::UnsafeCollateralizationRatio);

	// Mode-aware TCR check (Normal/Safety) is pre/post on net branch state.
	let pre_tcr = compute_tcr::<T>(&bs_before, price, now)?;
	let mut bs_after = bs_before.clone();
	bs_after.total_collateral = bs_after.total_collateral.saturating_add(initial_collateral);
	bs_after.total_interest_bearing_debt = new_total_ib;
	bs_after.weighted_interest_bearing_debt_sum = bs_after
		.weighted_interest_bearing_debt_sum
		.saturating_add(annual_rate.saturating_mul_int(initial_debt));
	bs_after.total_stakes = bs_after.total_stakes.saturating_add(stake);
	bs_after.weighted_stake_sum = bs_after
		.weighted_stake_sum
		.saturating_add(annual_rate.saturating_mul_int(stake));
	bs_after.total_minted_aggregate_interest =
		bs_after.total_minted_aggregate_interest.saturating_add(upfront_fee);
	let post_tcr = compute_tcr::<T>(&bs_after, price, now)?;
	enforce_mode_rules::<T>(&cfg, &bs_before, pre_tcr, post_tcr, false)?;

	// Mint pUSD to owner (less the upfront fee — the fee is added to
	// `accrued_interest` and minted to fee handlers).
	T::StableAsset::mint_into(&owner, initial_debt)?;
	if !upfront_fee.is_zero() {
		mint_and_route_yield::<T>(collateral_id, upfront_fee, YieldSource::UpfrontFee);
		Pallet::<T>::deposit_event(Event::UpfrontFeeCharged {
			collateral_id,
			owner: owner.clone(),
			amount: upfront_fee,
		});
	}

	// Persist vault and branch state.
	Vaults::<T>::insert(collateral_id, &owner, &vault);
	BranchStates::<T>::insert(collateral_id, &bs_after);
	VaultRedistSnapshots::<T>::insert(
		collateral_id,
		&owner,
		VaultRedistSnapshot {
			collat_per_stake: redist.cumulative_redist_collat_per_stake,
			debt_per_stake: redist.cumulative_redist_debt_per_stake,
			debt_time_per_stake: redist.cumulative_redist_debt_time_per_stake,
			weight_per_stake: redist.cumulative_redist_weight_per_stake,
		},
	);

	// Insert into rate index.
	T::RateIndex::insert(collateral_id, owner.clone(), annual_rate, hint)
		.map_err(|_| Error::<T>::InvalidPositionHints)?;

	Pallet::<T>::deposit_event(Event::Borrowed {
		collateral_id,
		owner: owner.clone(),
		recipient: owner.clone(),
		amount: initial_debt,
	});
	Pallet::<T>::deposit_event(Event::CollateralDeposited {
		collateral_id,
		owner: owner.clone(),
		from: owner.clone(),
		amount: initial_collateral,
	});
	Pallet::<T>::deposit_event(Event::VaultOpened { collateral_id, owner });
	Ok(())
}

/// Permissionless deposit. Intentionally accepts deposits to `Dormant` vaults
/// without requiring same-op revival to `Debt >= MinimumDebt`: a deposit is
/// strictly TCR-improving so there is no economic reason to gate it. (This
/// is a deliberate relaxation of DESIGN.md §4.3.)
pub fn deposit_collateral_for<T: Config>(
	from: T::AccountId,
	owner: T::AccountId,
	collateral_id: T::AssetId,
	amount: BalanceOf<T>,
) -> Result<(), DispatchError> {
	ensure_not_frozen::<T>(collateral_id)?;
	ensure!(Vaults::<T>::contains_key(collateral_id, &owner), Error::<T>::VaultNotFound);
	let now = T::TimeProvider::now();
	update_aggregate_interest::<T>(collateral_id, now)?;
	let _ = touch_vault::<T>(collateral_id, &owner, now)?;

	// Move collateral from caller to owner, on hold.
	T::CollateralAssets::transfer_and_hold(
		collateral_id,
		&HoldReason::VaultCollateral.into(),
		&from,
		&owner,
		amount,
		Precision::Exact,
		Preservation::Expendable,
		Fortitude::Polite,
	)?;

	BranchStates::<T>::try_mutate(collateral_id, |maybe| -> Result<_, DispatchError> {
		let bs = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
		bs.total_collateral = bs.total_collateral.saturating_add(amount);
		Ok(())
	})?;

	Pallet::<T>::deposit_event(Event::CollateralDeposited { collateral_id, owner, from, amount });
	Ok(())
}

pub fn withdraw_collateral<T: Config>(
	owner: T::AccountId,
	collateral_id: T::AssetId,
	amount: BalanceOf<T>,
	recipient: Option<T::AccountId>,
) -> Result<(), DispatchError> {
	ensure_not_frozen::<T>(collateral_id)?;
	let recipient = recipient.unwrap_or(owner.clone());
	// Pre-touch status check; re-read after touch for fresh debt fields.
	ensure!(
		!vault_of::<T>(collateral_id, &owner)?.status.is_final_recovery(),
		Error::<T>::VaultInFinalRecovery
	);

	let now = T::TimeProvider::now();
	update_aggregate_interest::<T>(collateral_id, now)?;
	let vault = touch_vault::<T>(collateral_id, &owner, now)?.ok_or(Error::<T>::VaultNotFound)?;

	let cfg = branch_cfg_of::<T>(collateral_id)?;
	let bs_before = branch_state_of::<T>(collateral_id)?;
	let coll = held_collateral::<T>(collateral_id, &owner);
	ensure!(coll >= amount, Error::<T>::InsufficientCollateral);

	// The branch TCR check always needs the live oracle price; the per-vault
	// CR check is skipped entirely when the vault has zero debt (no ratio to
	// validate). Using a synthetic `price = 1` for the branch TCR — as the
	// previous version did — under-prices branch collateral and falsely
	// trips `SafetyModeTcrWorsening` on legitimate withdraws from zero-debt
	// vaults.
	let price = oracle_price::<T>(collateral_id)?.price;
	let total_debt = vault.interest_bearing_debt.saturating_add(vault.accrued_interest);
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
	bs_after.total_collateral = bs_after.total_collateral.saturating_sub(amount);
	let post_tcr = compute_tcr::<T>(&bs_after, price, now)?;
	enforce_mode_rules::<T>(&cfg, &bs_before, pre_tcr, post_tcr, false)?;

	T::CollateralAssets::transfer_on_hold(
		collateral_id,
		&HoldReason::VaultCollateral.into(),
		&owner,
		&recipient,
		amount,
		Precision::Exact,
		Restriction::Free,
		Fortitude::Polite,
	)?;

	BranchStates::<T>::insert(collateral_id, &bs_after);
	Pallet::<T>::deposit_event(Event::CollateralWithdrawn {
		collateral_id,
		owner,
		recipient,
		amount,
	});
	Ok(())
}

pub fn borrow<T: Config>(
	owner: T::AccountId,
	collateral_id: T::AssetId,
	amount: BalanceOf<T>,
	maybe_new_rate: Option<FixedU128>,
	recipient: Option<T::AccountId>,
	hint: Position<T::AccountId>,
) -> Result<(), DispatchError> {
	ensure_not_frozen::<T>(collateral_id)?;
	let recipient = recipient.unwrap_or(owner.clone());
	let pre_status = vault_of::<T>(collateral_id, &owner)?.status;
	ensure!(!pre_status.is_final_recovery(), Error::<T>::VaultInFinalRecovery);

	let now = T::TimeProvider::now();
	update_aggregate_interest::<T>(collateral_id, now)?;
	let mut vault =
		touch_vault::<T>(collateral_id, &owner, now)?.ok_or(Error::<T>::VaultNotFound)?;

	let cfg = branch_cfg_of::<T>(collateral_id)?;
	let old_rate = vault.annual_rate;
	let new_rate = maybe_new_rate.unwrap_or(old_rate);
	validate_rate::<T>(&cfg, new_rate)?;

	let bs_before = branch_state_of::<T>(collateral_id)?;
	let new_ib_debt = vault.interest_bearing_debt.saturating_add(amount);

	// Branch debt-ceiling check on the user-initiated debt increase.
	ensure!(
		bs_before.total_interest_bearing_debt.saturating_add(amount) <= cfg.debt_ceiling,
		Error::<T>::DebtCeilingExceeded
	);

	// Upfront fee on the borrowed delta + (rate-change component if not on
	// cooldown).
	let cooldown_elapsed =
		now.saturating_sub(vault.last_rate_update) >= cfg.rate_adjustment_cooldown;
	let rate_change_fee_base = if maybe_new_rate.is_some() && !cooldown_elapsed {
		vault.interest_bearing_debt
	} else {
		BalanceOf::<T>::zero()
	};
	let (mut bs_after, upfront_fee) =
		simulate_borrow::<T>(&bs_before, &cfg, &vault, amount, new_rate, rate_change_fee_base);
	bs_after.total_minted_aggregate_interest =
		bs_after.total_minted_aggregate_interest.saturating_add(upfront_fee);

	// Mint pUSD to recipient.
	T::StableAsset::mint_into(&recipient, amount)?;
	if !upfront_fee.is_zero() {
		mint_and_route_yield::<T>(collateral_id, upfront_fee, YieldSource::UpfrontFee);
		Pallet::<T>::deposit_event(Event::UpfrontFeeCharged {
			collateral_id,
			owner: owner.clone(),
			amount: upfront_fee,
		});
	}

	// Update vault.
	let dormant_to_active = vault.status.is_dormant() && new_ib_debt >= cfg.minimum_debt;
	vault.interest_bearing_debt = new_ib_debt;
	vault.accrued_interest = vault.accrued_interest.saturating_add(upfront_fee);
	if maybe_new_rate.is_some() {
		vault.annual_rate = new_rate;
		vault.last_rate_update = now;
	}
	if dormant_to_active {
		vault.status = VaultStatus::Active;
	}
	ensure!(vault.interest_bearing_debt >= cfg.minimum_debt, Error::<T>::DebtBelowMinimum);

	let coll = held_collateral::<T>(collateral_id, &owner);
	let total_debt = vault.interest_bearing_debt.saturating_add(vault.accrued_interest);
	let price = oracle_price::<T>(collateral_id)?.price;
	let cr = math::collateralization_ratio::<BalanceOf<T>>(coll, total_debt, price)
		.ok_or(Error::<T>::UnsafeCollateralizationRatio)?;
	ensure!(cr >= cfg.initial_collateralization_ratio, Error::<T>::UnsafeCollateralizationRatio);

	let pre_tcr = compute_tcr::<T>(&bs_before, price, now)?;
	let post_tcr = compute_tcr::<T>(&bs_after, price, now)?;
	enforce_mode_rules::<T>(&cfg, &bs_before, pre_tcr, post_tcr, false)?;

	BranchStates::<T>::insert(collateral_id, &bs_after);
	Vaults::<T>::insert(collateral_id, &owner, &vault);

	if dormant_to_active {
		// Insert/reinsert into the rate index at `new_rate`.
		T::RateIndex::insert(collateral_id, owner.clone(), new_rate, hint)
			.map_err(|_| Error::<T>::InvalidPositionHints)?;
		Pallet::<T>::deposit_event(Event::VaultStatusChanged {
			collateral_id,
			owner: owner.clone(),
			old_status: VaultStatus::Dormant,
			new_status: VaultStatus::Active,
		});
		// Clear dormant pointer if it referenced this vault.
		BranchStates::<T>::mutate(collateral_id, |maybe| {
			if let Some(b) = maybe {
				if b.last_dormant_vault_owner.as_ref() == Some(&owner) {
					b.last_dormant_vault_owner = None;
				}
			}
		});
	} else if old_rate != new_rate {
		T::RateIndex::re_insert(collateral_id, owner.clone(), new_rate, hint)
			.map_err(|_| Error::<T>::InvalidPositionHints)?;
	}

	if old_rate != new_rate {
		Pallet::<T>::deposit_event(Event::BorrowRateChanged {
			collateral_id,
			owner: owner.clone(),
			old_rate,
			new_rate,
		});
	}
	Pallet::<T>::deposit_event(Event::Borrowed { collateral_id, owner, recipient, amount });
	Ok(())
}

pub fn repay_for<T: Config>(
	from: T::AccountId,
	owner: T::AccountId,
	collateral_id: T::AssetId,
	amount: BalanceOf<T>,
) -> Result<(), DispatchError> {
	ensure_not_frozen::<T>(collateral_id)?;
	let pre_status = vault_of::<T>(collateral_id, &owner)?.status;
	ensure!(!pre_status.is_final_recovery(), Error::<T>::VaultInFinalRecovery);

	let now = T::TimeProvider::now();
	update_aggregate_interest::<T>(collateral_id, now)?;
	let mut vault =
		touch_vault::<T>(collateral_id, &owner, now)?.ok_or(Error::<T>::VaultNotFound)?;

	let cfg = branch_cfg_of::<T>(collateral_id)?;

	// Burn pUSD from caller.
	T::StableAsset::burn_from(
		&from,
		amount,
		Preservation::Expendable,
		Precision::Exact,
		Fortitude::Polite,
	)?;

	// Apply to accrued_interest first, then interest_bearing_debt.
	let mut remaining = amount;
	let pay_accrued = core::cmp::min(remaining, vault.accrued_interest);
	vault.accrued_interest = vault.accrued_interest.saturating_sub(pay_accrued);
	remaining = remaining.saturating_sub(pay_accrued);
	let pay_principal = core::cmp::min(remaining, vault.interest_bearing_debt);
	vault.interest_bearing_debt = vault.interest_bearing_debt.saturating_sub(pay_principal);
	remaining = remaining.saturating_sub(pay_principal);
	ensure!(remaining.is_zero(), Error::<T>::InsufficientRepayment);

	// Dust check: user repayments must leave Debt == 0 (and close in same op
	// — handled by close_vault) OR Debt >= MinimumDebt.
	let new_total = vault.interest_bearing_debt.saturating_add(vault.accrued_interest);
	if !new_total.is_zero() && new_total < cfg.minimum_debt {
		return Err(Error::<T>::DebtWouldBecomeDust.into());
	}

	BranchStates::<T>::try_mutate(collateral_id, |maybe| -> Result<_, DispatchError> {
		let bs = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
		bs.total_interest_bearing_debt =
			bs.total_interest_bearing_debt.saturating_sub(pay_principal);
		bs.total_minted_aggregate_interest =
			bs.total_minted_aggregate_interest.saturating_sub(pay_accrued);
		// Remove the principal's weighted contribution at the vault's rate.
		let weighted = vault.annual_rate.saturating_mul_int(pay_principal);
		bs.weighted_interest_bearing_debt_sum =
			bs.weighted_interest_bearing_debt_sum.saturating_sub(weighted);
		Ok(())
	})?;

	Vaults::<T>::insert(collateral_id, &owner, &vault);
	Pallet::<T>::deposit_event(Event::Repaid { collateral_id, owner, from, amount });
	Ok(())
}

pub fn change_rate<T: Config>(
	owner: T::AccountId,
	collateral_id: T::AssetId,
	new_rate: FixedU128,
	hint: Position<T::AccountId>,
) -> Result<(), DispatchError> {
	ensure_not_frozen::<T>(collateral_id)?;
	let pre_status = vault_of::<T>(collateral_id, &owner)?.status;
	ensure!(pre_status.is_active(), Error::<T>::InvalidVaultStatus);

	let now = T::TimeProvider::now();
	update_aggregate_interest::<T>(collateral_id, now)?;
	let mut vault =
		touch_vault::<T>(collateral_id, &owner, now)?.ok_or(Error::<T>::VaultNotFound)?;
	let old_rate = vault.annual_rate;
	if old_rate == new_rate {
		return Ok(());
	}

	let cfg = branch_cfg_of::<T>(collateral_id)?;
	validate_rate::<T>(&cfg, new_rate)?;

	// Build the post-change branch state in a clone so we can compute
	// `post_tcr` and run `enforce_mode_rules` BEFORE applying anything.
	let bs_before = branch_state_of::<T>(collateral_id)?;
	let cooldown_elapsed =
		now.saturating_sub(vault.last_rate_update) >= cfg.rate_adjustment_cooldown;
	let (mut bs_after, upfront_fee) =
		simulate_change_rate::<T>(&bs_before, &cfg, &vault, new_rate, cooldown_elapsed);
	bs_after.total_minted_aggregate_interest =
		bs_after.total_minted_aggregate_interest.saturating_add(upfront_fee);

	// Mode-aware TCR check (troves.md §4.3 — "Change borrow rate"). Gates
	// premature changes that would worsen TCR in Safety mode and rate
	// changes that would push the branch into Safety mode from Normal.
	let price = oracle_price::<T>(collateral_id)?.price;
	let pre_tcr = compute_tcr::<T>(&bs_before, price, now)?;
	let post_tcr = compute_tcr::<T>(&bs_after, price, now)?;
	enforce_mode_rules::<T>(&cfg, &bs_before, pre_tcr, post_tcr, false)?;

	// All gates passed — apply state changes.
	if !upfront_fee.is_zero() {
		mint_and_route_yield::<T>(collateral_id, upfront_fee, YieldSource::UpfrontFee);
		Pallet::<T>::deposit_event(Event::UpfrontFeeCharged {
			collateral_id,
			owner: owner.clone(),
			amount: upfront_fee,
		});
	}

	BranchStates::<T>::insert(collateral_id, &bs_after);

	vault.annual_rate = new_rate;
	vault.last_rate_update = now;
	vault.accrued_interest = vault.accrued_interest.saturating_add(upfront_fee);
	Vaults::<T>::insert(collateral_id, &owner, &vault);

	T::RateIndex::re_insert(collateral_id, owner.clone(), new_rate, hint)
		.map_err(|_| Error::<T>::InvalidPositionHints)?;
	Pallet::<T>::deposit_event(Event::BorrowRateChanged {
		collateral_id,
		owner,
		old_rate,
		new_rate,
	});
	Ok(())
}

pub fn close_vault<T: Config>(
	owner: T::AccountId,
	collateral_id: T::AssetId,
	recipient: Option<T::AccountId>,
) -> Result<(), DispatchError> {
	ensure_not_frozen::<T>(collateral_id)?;
	let recipient = recipient.unwrap_or(owner.clone());

	let now = T::TimeProvider::now();
	update_aggregate_interest::<T>(collateral_id, now)?;
	let vault = touch_vault::<T>(collateral_id, &owner, now)?.ok_or(Error::<T>::VaultNotFound)?;

	let total_debt = vault.interest_bearing_debt.saturating_add(vault.accrued_interest);
	ensure!(total_debt.is_zero(), Error::<T>::InsufficientRepayment);

	let cfg = branch_cfg_of::<T>(collateral_id)?;
	let coll = held_collateral::<T>(collateral_id, &owner);

	// Build the post-close branch state in a clone for the mode-rule check.
	// Closing a vault removes its collateral and stake contribution while
	// leaving total_debt unchanged (the `total_debt.is_zero()` precondition
	// above guarantees this vault contributes no debt). post_TCR < pre_TCR
	// whenever `coll > 0`, which is the case the Safety-mode rule gates.
	let bs_before = branch_state_of::<T>(collateral_id)?;
	let stake_weighted = vault.annual_rate.saturating_mul_int(vault.stake);
	let mut bs_after = bs_before.clone();
	bs_after.total_collateral = bs_after.total_collateral.saturating_sub(coll);
	bs_after.total_stakes = bs_after.total_stakes.saturating_sub(vault.stake);
	bs_after.weighted_stake_sum = bs_after.weighted_stake_sum.saturating_sub(stake_weighted);
	if bs_after.last_dormant_vault_owner.as_ref() == Some(&owner) {
		bs_after.last_dormant_vault_owner = None;
	}

	// Mode-aware TCR check (troves.md §4.3 — "Close active vault" / "Close
	// dormant"). When `coll == 0` (e.g., a fully-redeemed Dormant vault)
	// post_TCR == pre_TCR so the gate is a no-op; otherwise the Safety
	// branch enforces post_TCR ≥ pre_TCR.
	let price = oracle_price::<T>(collateral_id)?.price;
	let pre_tcr = compute_tcr::<T>(&bs_before, price, now)?;
	let post_tcr = compute_tcr::<T>(&bs_after, price, now)?;
	enforce_mode_rules::<T>(&cfg, &bs_before, pre_tcr, post_tcr, false)?;

	// Release any held collateral to the recipient.
	if !coll.is_zero() {
		T::CollateralAssets::transfer_on_hold(
			collateral_id,
			&HoldReason::VaultCollateral.into(),
			&owner,
			&recipient,
			coll,
			Precision::Exact,
			Restriction::Free,
			Fortitude::Polite,
		)?;
	}

	BranchStates::<T>::insert(collateral_id, &bs_after);

	// Remove from rate index if Active; from FIFO if FinalRecovery.
	match vault.status {
		VaultStatus::Active => {
			let _ = T::RateIndex::remove(&collateral_id, &owner);
		},
		VaultStatus::FinalRecovery => {
			let _ = recovery::remove::<T>(&collateral_id, &owner);
		},
		VaultStatus::Dormant => {},
	}

	Vaults::<T>::remove(collateral_id, &owner);
	VaultRedistSnapshots::<T>::remove(collateral_id, &owner);

	Pallet::<T>::deposit_event(Event::VaultClosed { collateral_id, owner, recipient });
	Ok(())
}

pub fn poke<T: Config>(
	owner: T::AccountId,
	collateral_id: T::AssetId,
) -> Result<(), DispatchError> {
	let now = T::TimeProvider::now();
	update_aggregate_interest::<T>(collateral_id, now)?;
	touch_vault::<T>(collateral_id, &owner, now).map(|_| ())
}

pub fn enter_final_recovery<T: Config>(
	owner: T::AccountId,
	collateral_id: T::AssetId,
) -> Result<(), DispatchError> {
	ensure_not_frozen::<T>(collateral_id)?;
	let now = T::TimeProvider::now();
	update_aggregate_interest::<T>(collateral_id, now)?;
	let mut vault =
		touch_vault::<T>(collateral_id, &owner, now)?.ok_or(Error::<T>::VaultNotFound)?;
	ensure!(vault.status.is_active(), Error::<T>::InvalidVaultStatus);

	// CR < MCR.
	let cfg = branch_cfg_of::<T>(collateral_id)?;
	let coll = held_collateral::<T>(collateral_id, &owner);
	let total_debt = vault.interest_bearing_debt.saturating_add(vault.accrued_interest);
	let price = oracle_price::<T>(collateral_id)?.price;
	let cr = math::collateralization_ratio::<BalanceOf<T>>(coll, total_debt, price)
		.ok_or(Error::<T>::UnsafeCollateralizationRatio)?;
	ensure!(cr < cfg.minimum_collateralization_ratio, Error::<T>::UnsafeCollateralizationRatio);

	// Last redistribution recipient: if removing the vault would leave
	// `total_stakes - vault.stake == 0` for the redistribution recipients,
	// it is the last eligible. We approximate by counting Active+Dormant
	// vaults — but with the storage shape we have, the cheap proxy is
	// `bs.total_stakes == vault.stake`.
	let bs_check = branch_state_of::<T>(collateral_id)?;
	ensure!(bs_check.total_stakes == vault.stake, Error::<T>::NotLastEligibleVault);

	// Remove from rate index, from redistribution recipient accounting, and
	// append to FIFO.
	T::RateIndex::remove(&collateral_id, &owner)
		.map_err(|_| Error::<T>::RateIndexInvariantBroken)?;
	BranchStates::<T>::try_mutate(collateral_id, |maybe| -> Result<_, DispatchError> {
		let bs = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
		let stake_weighted = vault.annual_rate.saturating_mul_int(vault.stake);
		bs.total_stakes = bs.total_stakes.saturating_sub(vault.stake);
		bs.weighted_stake_sum = bs.weighted_stake_sum.saturating_sub(stake_weighted);
		Ok(())
	})?;
	vault.status = VaultStatus::FinalRecovery;
	Vaults::<T>::insert(collateral_id, &owner, &vault);
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
/// the fully-accrued CR is at or above the MCR, then re-adds stake +
/// weighted contributions and reinserts into the rate index using the
/// caller-supplied `hint` so the operation is O(1) (or O(MaxHintRepairSteps)
/// for stale hints, bounded by the linked-list crate's repair budget).
pub fn exit_final_recovery<T: Config>(
	owner: T::AccountId,
	collateral_id: T::AssetId,
	hint: Position<T::AccountId>,
) -> Result<(), DispatchError> {
	ensure_not_frozen::<T>(collateral_id)?;
	let now = T::TimeProvider::now();
	update_aggregate_interest::<T>(collateral_id, now)?;
	let mut vault =
		touch_vault::<T>(collateral_id, &owner, now)?.ok_or(Error::<T>::VaultNotFound)?;
	ensure!(vault.status.is_final_recovery(), Error::<T>::InvalidVaultStatus);

	let cfg = branch_cfg_of::<T>(collateral_id)?;
	let coll = held_collateral::<T>(collateral_id, &owner);
	let total_debt = vault.interest_bearing_debt.saturating_add(vault.accrued_interest);
	let price = oracle_price::<T>(collateral_id)?.price;
	let cr = math::collateralization_ratio::<BalanceOf<T>>(coll, total_debt, price)
		.ok_or(Error::<T>::UnsafeCollateralizationRatio)?;
	ensure!(cr >= cfg.minimum_collateralization_ratio, Error::<T>::UnsafeCollateralizationRatio);

	// Remove from FIFO, restore stake + weighted contributions, reinsert into
	// the rate index using caller-supplied hints.
	recovery::remove::<T>(&collateral_id, &owner)?;
	BranchStates::<T>::try_mutate(collateral_id, |maybe| -> Result<_, DispatchError> {
		let bs = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
		bs.total_stakes = bs.total_stakes.saturating_add(vault.stake);
		bs.weighted_stake_sum = bs
			.weighted_stake_sum
			.saturating_add(vault.annual_rate.saturating_mul_int(vault.stake));
		bs.weighted_interest_bearing_debt_sum = bs
			.weighted_interest_bearing_debt_sum
			.saturating_add(vault.annual_rate.saturating_mul_int(vault.interest_bearing_debt));
		Ok(())
	})?;
	let old_status = vault.status;
	vault.status = VaultStatus::Active;
	Vaults::<T>::insert(collateral_id, &owner, &vault);
	T::RateIndex::insert(collateral_id, owner.clone(), vault.annual_rate, hint)
		.map_err(|_| Error::<T>::InvalidPositionHints)?;
	Pallet::<T>::deposit_event(Event::VaultStatusChanged {
		collateral_id,
		owner,
		old_status,
		new_status: VaultStatus::Active,
	});
	Ok(())
}

pub fn register_branch<T: Config>(
	collateral_id: T::AssetId,
	config: BranchConfig<BalanceOf<T>, MomentOf<T>>,
) -> Result<(), DispatchError> {
	ensure!(!BranchConfigs::<T>::contains_key(collateral_id), Error::<T>::BranchAlreadyRegistered);
	Branches::<T>::try_mutate(|list| -> Result<_, DispatchError> {
		list.try_push(collateral_id).map_err(|_| Error::<T>::TooManyBranches)?;
		Ok(())
	})?;
	BranchConfigs::<T>::insert(collateral_id, config);
	BranchStates::<T>::insert(
		collateral_id,
		BranchState {
			total_collateral: BalanceOf::<T>::zero(),
			total_interest_bearing_debt: BalanceOf::<T>::zero(),
			total_minted_aggregate_interest: BalanceOf::<T>::zero(),
			pending_redistribution_debt: BalanceOf::<T>::zero(),
			bad_debt: BalanceOf::<T>::zero(),
			weighted_interest_bearing_debt_sum: BalanceOf::<T>::zero(),
			last_aggregate_interest_update: T::TimeProvider::now(),
			total_stakes: BalanceOf::<T>::zero(),
			weighted_stake_sum: BalanceOf::<T>::zero(),
			redist_epoch: 0,
			final_recovery_head: None,
			final_recovery_tail: None,
			last_dormant_vault_owner: None,
			frozen: None,
		},
	);
	BranchRedistStates::<T>::insert(
		collateral_id,
		crate::types::BranchRedistState {
			cumulative_redist_collat_per_stake: FixedU128::zero(),
			cumulative_redist_debt_per_stake: FixedU128::zero(),
			cumulative_redist_debt_time_per_stake: FixedU128::zero(),
			cumulative_redist_weight_per_stake: FixedU128::zero(),
		},
	);
	Pallet::<T>::deposit_event(Event::BranchRegistered { collateral_id });
	Ok(())
}

/// Apply `mutator` to the branch config and emit `ParameterUpdated`. Caller
/// is responsible for any defensive-action / authorization gating.
pub fn update_branch_config<T: Config, F>(
	collateral_id: T::AssetId,
	parameter: crate::types::ParameterId,
	mutator: F,
) -> Result<(), DispatchError>
where
	F: FnOnce(&mut BranchConfig<BalanceOf<T>, MomentOf<T>>),
{
	BranchConfigs::<T>::try_mutate(collateral_id, |maybe| -> Result<_, DispatchError> {
		let cfg = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
		mutator(cfg);
		Ok(())
	})?;
	Pallet::<T>::deposit_event(Event::ParameterUpdated { collateral_id, parameter });
	Ok(())
}

/// Convenience: read a branch's current config (or error `UnknownCollateral`).
pub fn current_branch_config<T: Config>(
	collateral_id: T::AssetId,
) -> Result<BranchConfig<BalanceOf<T>, MomentOf<T>>, DispatchError> {
	BranchConfigs::<T>::get(collateral_id).ok_or_else(|| Error::<T>::UnknownCollateral.into())
}

pub fn enable_frozen_mode<T: Config>(collateral_id: T::AssetId) -> Result<(), DispatchError> {
	BranchStates::<T>::try_mutate(collateral_id, |maybe| -> Result<_, DispatchError> {
		let bs = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
		if !bs.is_frozen() {
			bs.frozen = Some(FrozenState {
				reason: FrozenReason::Governance,
				entered_at: T::TimeProvider::now(),
			});
			Pallet::<T>::deposit_event(Event::ModeChanged {
				collateral_id,
				old_mode: BranchMode::Normal,
				new_mode: BranchMode::Frozen,
			});
		}
		Ok(())
	})
}

pub(crate) fn ensure_not_frozen<T: Config>(collateral_id: T::AssetId) -> Result<(), DispatchError> {
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

pub fn view_vault_cr<T: Config>(
	collateral_id: &T::AssetId,
	owner: &T::AccountId,
) -> Option<FixedU128> {
	let vault = Vaults::<T>::get(collateral_id, owner)?;
	let coll = held_collateral::<T>(*collateral_id, owner);
	let total_debt = vault.interest_bearing_debt.saturating_add(vault.accrued_interest);
	let price = T::Oracle::provide_price(collateral_id).ok()?.price;
	math::collateralization_ratio::<BalanceOf<T>>(coll, total_debt, price)
}

pub fn view_branch_tcr<T: Config>(collateral_id: &T::AssetId) -> Option<FixedU128> {
	let bs = BranchStates::<T>::get(collateral_id)?;
	let price = T::Oracle::provide_price(collateral_id).ok()?.price;
	let now = T::TimeProvider::now();
	compute_tcr::<T>(&bs, price, now).ok()
}

pub fn view_redemption_queue_head<T: Config>(
	collateral_id: &T::AssetId,
	n: u32,
) -> Vec<T::AccountId> {
	let mut out: Vec<T::AccountId> = Vec::with_capacity(n as usize);
	let mut taken = 0u32;
	// 1. FinalRecovery FIFO.
	for owner in recovery::queue_head::<T>(collateral_id, n) {
		if taken >= n {
			break;
		}
		out.push(owner);
		taken = taken.saturating_add(1);
	}
	if taken >= n {
		return out;
	}
	// 2. last_dormant_vault_owner if set.
	if let Some(bs) = BranchStates::<T>::get(collateral_id) {
		if let Some(owner) = bs.last_dormant_vault_owner {
			out.push(owner);
			taken = taken.saturating_add(1);
		}
	}
	if taken >= n {
		return out;
	}
	// 3. Tail-first rate index.
	let remaining = n.saturating_sub(taken);
	for owner in T::RateIndex::iter_from_tail(collateral_id, remaining) {
		out.push(owner);
	}
	out
}

pub fn view_debt_in_front<T: Config>(collateral_id: &T::AssetId, rate: FixedU128) -> BalanceOf<T> {
	// Walk tail-first; sum interest_bearing_debt while node.priority < rate.
	let mut total = BalanceOf::<T>::zero();
	let mut cursor = T::RateIndex::tail(collateral_id);
	while let Some(o) = cursor {
		let priority = match T::RateIndex::priority(collateral_id, &o) {
			Some(p) => p,
			None => break,
		};
		if priority >= rate {
			break;
		}
		if let Some(v) = Vaults::<T>::get(collateral_id, &o) {
			total = total.saturating_add(v.interest_bearing_debt);
		}
		cursor = match T::RateIndex::neighbors(collateral_id, &o) {
			Some(p) => p.prev,
			None => break,
		};
	}
	total
}

pub fn predict_upfront_fee_open<T: Config>(
	collateral_id: &T::AssetId,
	initial_debt: BalanceOf<T>,
	annual_rate: FixedU128,
) -> BalanceOf<T> {
	match (BranchConfigs::<T>::get(collateral_id), BranchStates::<T>::get(collateral_id)) {
		(Some(cfg), Some(bs)) => open_upfront_fee::<T>(&bs, &cfg, initial_debt, annual_rate),
		_ => BalanceOf::<T>::zero(),
	}
}

pub fn predict_upfront_fee_borrow<T: Config>(
	collateral_id: &T::AssetId,
	owner: &T::AccountId,
	debt_increase: BalanceOf<T>,
	maybe_new_rate: Option<FixedU128>,
) -> BalanceOf<T> {
	let (cfg, bs, vault) = match predict_inputs::<T>(collateral_id, owner) {
		Some(t) => t,
		None => return BalanceOf::<T>::zero(),
	};
	let new_rate = maybe_new_rate.unwrap_or(vault.annual_rate);
	let now = T::TimeProvider::now();
	let cooldown_elapsed =
		now.saturating_sub(vault.last_rate_update) >= cfg.rate_adjustment_cooldown;
	let rate_change_fee_base = if maybe_new_rate.is_some() && !cooldown_elapsed {
		vault.interest_bearing_debt
	} else {
		BalanceOf::<T>::zero()
	};
	simulate_borrow::<T>(&bs, &cfg, &vault, debt_increase, new_rate, rate_change_fee_base).1
}

pub fn predict_upfront_fee_rate_change<T: Config>(
	collateral_id: &T::AssetId,
	owner: &T::AccountId,
	new_rate: FixedU128,
) -> BalanceOf<T> {
	let (cfg, bs, vault) = match predict_inputs::<T>(collateral_id, owner) {
		Some(t) => t,
		None => return BalanceOf::<T>::zero(),
	};
	let now = T::TimeProvider::now();
	let cooldown_elapsed =
		now.saturating_sub(vault.last_rate_update) >= cfg.rate_adjustment_cooldown;
	simulate_change_rate::<T>(&bs, &cfg, &vault, new_rate, cooldown_elapsed).1
}

/// Read the `(cfg, branch state, vault)` triple for a `predict_*` view.
/// Returns `None` if any row is missing — the predict APIs treat that as
/// "no fee" rather than an error.
fn predict_inputs<T: Config>(
	collateral_id: &T::AssetId,
	owner: &T::AccountId,
) -> Option<(
	BranchConfig<BalanceOf<T>, MomentOf<T>>,
	BranchState<T::AccountId, BalanceOf<T>, MomentOf<T>>,
	Vault<BalanceOf<T>, MomentOf<T>>,
)> {
	Some((
		BranchConfigs::<T>::get(collateral_id)?,
		BranchStates::<T>::get(collateral_id)?,
		Vaults::<T>::get(collateral_id, owner)?,
	))
}

/// Refresh the next handful of vaults across each branch using the cursor.
pub fn on_idle_walk<T: Config>(remaining: Weight) -> Weight {
	let per_call = T::WeightInfo::on_idle_one_vault();
	if remaining.any_lt(per_call) {
		return Weight::zero();
	}
	let mut consumed = Weight::zero();
	let mut budget = T::MaxOnIdleVaultRefresh::get();
	let touch_one = |collateral_id: T::AssetId, owner: &T::AccountId| -> bool {
		// Returns `true` iff a touch landed (budget consumed); `false` to stop.
		if Vaults::<T>::get(collateral_id, owner).is_none() {
			return true;
		}
		let now = T::TimeProvider::now();
		if update_aggregate_interest::<T>(collateral_id, now).is_err() {
			return false;
		}
		let _ = touch_vault::<T>(collateral_id, owner, now);
		true
	};
	for collateral_id in Branches::<T>::get().into_iter() {
		if budget == 0 || (remaining.saturating_sub(consumed)).any_lt(per_call) {
			break;
		}
		// (1) Rate-index walk using the cursor.
		let mut cursor =
			OnIdleCursor::<T>::get(collateral_id).or_else(|| T::RateIndex::head(&collateral_id));
		while budget > 0 {
			let owner = match cursor.clone() {
				Some(o) => o,
				None => break,
			};
			if !touch_one(collateral_id, &owner) {
				break;
			}
			cursor = T::RateIndex::neighbors(&collateral_id, &owner).and_then(|p| p.next);
			budget = budget.saturating_sub(1);
			consumed = consumed.saturating_add(per_call);
			if (remaining.saturating_sub(consumed)).any_lt(per_call) {
				break;
			}
		}
		OnIdleCursor::<T>::set(collateral_id, cursor);

		// (2) FinalRecovery FIFO head — single touch per pass.
		if budget > 0 && !(remaining.saturating_sub(consumed)).any_lt(per_call) {
			if let Some(owner) =
				BranchStates::<T>::get(collateral_id).and_then(|s| s.final_recovery_head)
			{
				if touch_one(collateral_id, &owner) {
					budget = budget.saturating_sub(1);
					consumed = consumed.saturating_add(per_call);
				}
			}
		}

		// (3) Dormant continuation pointer — single touch per pass.
		if budget > 0 && !(remaining.saturating_sub(consumed)).any_lt(per_call) {
			if let Some(owner) =
				BranchStates::<T>::get(collateral_id).and_then(|s| s.last_dormant_vault_owner)
			{
				if touch_one(collateral_id, &owner) {
					budget = budget.saturating_sub(1);
					consumed = consumed.saturating_add(per_call);
				}
			}
		}
	}
	consumed
}
