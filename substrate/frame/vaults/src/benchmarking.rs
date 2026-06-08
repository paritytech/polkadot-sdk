//! Benchmarks for `pallet-vaults`. Rate-index dispatchables feed the
//! linked-list a hint that is exactly `hint_repair_budget` steps stale, so
//! the worst-case repair walk is what gets measured.

#![cfg(feature = "runtime-benchmarks")]

use crate::{
	helpers::{self, rate_list_id},
	pallet::{BalanceOf, BranchStates, Branches, Config, HoldReason, MomentOf, Pallet, Vaults},
	types::{BranchConfig, VaultStatus},
	BenchmarkHelper as _,
};
use alloc::vec::Vec;
use frame::{
	benchmarking::prelude::*,
	deps::{
		frame_support::traits::{
			fungible::Mutate as FungibleMutate, fungibles::MutateHold as FungiblesMutateHold,
			EnsureOrigin, Time,
		},
		sp_runtime::{
			traits::{SaturatedConversion, Zero},
			FixedU128, Permill,
		},
	},
};
use frame_system::RawOrigin;
use pallet_linked_list::{Position, SortedListInterface};

const ORACLE_PRICE: u128 = 10;
const ACCOUNT_FUNDING: u128 = 10_000_000;
const SEED_COLL: u128 = 1_000_000;
/// Must exceed `default_branch_config::minimum_debt` (200).
const SEED_DEBT: u128 = 300;
/// Price drop that pushes a `coll=200, debt=300` vault below the 110% MCR,
/// so `enter_final_recovery` accepts it.
const RECOVERY_TRIGGER_PRICE: u32 = 1;
/// One hour in milliseconds — enough for `update_aggregate_interest` and
/// `touch_vault` to accrue non-zero interest at the default 5% vault rate.
const ONE_HOUR_MS: u64 = 60 * 60 * 1_000;
const RECOVERY_VAULT_COLL: u128 = 200;
const REDIST_PER_STAKE_NUM: u128 = 1;
const REDIST_PER_STAKE_DEN: u128 = 100;
const REDIST_WEIGHT_PER_STAKE_NUM: u128 = 1;
const REDIST_WEIGHT_PER_STAKE_DEN: u128 = 10_000;
const REDIST_PRESTAGE_COLL: u128 = 10_000_000;

fn balance<T: Config>(value: u128) -> BalanceOf<T> {
	value.saturated_into()
}

fn rate(numerator: u128, denominator: u128) -> FixedU128 {
	FixedU128::from_rational(numerator, denominator)
}

fn default_branch_config<T: Config>() -> BranchConfig<BalanceOf<T>, MomentOf<T>> {
	const DAY_MS: u64 = 24 * 3_600 * 1_000;
	BranchConfig {
		minimum_collateralization_ratio: rate(110, 100),
		initial_collateralization_ratio: rate(120, 100),
		safety_collateralization_ratio: rate(130, 100),
		debt_ceiling: balance::<T>(100_000_000),
		minimum_debt: balance::<T>(200),
		minimum_collateral: balance::<T>(1),
		minimum_borrow_rate: rate(1, 1_000),
		maximum_borrow_rate: rate(100, 100),
		upfront_fee_period: (7 * DAY_MS).saturated_into(),
		rate_adjustment_cooldown: DAY_MS.saturated_into(),
		redistribution_penalty: Permill::from_percent(5),
	}
}

fn manager_origin<T: Config>() -> Result<T::RuntimeOrigin, BenchmarkError> {
	T::ManagerOrigin::try_successful_origin()
		.map_err(|_| BenchmarkError::Stop("manager origin unavailable"))
}

fn register_default_branch<T: Config>() -> Result<T::AssetId, BenchmarkError> {
	let asset = T::BenchmarkHelper::collateral_asset_id();
	Pallet::<T>::register_branch(
		manager_origin::<T>()?,
		asset.clone(),
		default_branch_config::<T>(),
	)?;
	T::BenchmarkHelper::set_oracle_price(
		asset.clone(),
		FixedU128::saturating_from_integer(ORACLE_PRICE),
	);
	Ok(asset)
}

fn funded_account<T: Config>(seed: &'static str, asset: &T::AssetId) -> T::AccountId {
	let who: T::AccountId = account(seed, 0, 0);
	T::BenchmarkHelper::mint_collateral(asset.clone(), &who, balance::<T>(ACCOUNT_FUNDING));
	who
}

/// Runtime-adaptive rate fixture derived from the live branch's borrow-rate
/// bounds.
struct RateBounds {
	/// Highest seed-chain rate.
	base: FixedU128,
	/// Gap between consecutive seed rates.
	step: FixedU128,
	/// A rate strictly above every seed-chain rate, used for "land at head"
	/// insert worst cases. Stays below `maximum_borrow_rate`.
	above: FixedU128,
	/// A rate inside the seed-chain range, used by `close_vault`'s
	/// middle-of-list removal case.
	middle: FixedU128,
}

fn rate_bounds<T: Config>(asset: &T::AssetId) -> Result<RateBounds, BenchmarkError> {
	let cfg = helpers::current_branch_config::<T>(asset)
		.map_err(|_| BenchmarkError::Stop("missing branch config"))?;
	let count = T::VaultLists::repair_budget().saturating_add(2);
	let safety_floor =
		cfg.minimum_borrow_rate.saturating_mul(FixedU128::saturating_from_integer(2u32));
	// Use half of `maximum_borrow_rate` as the ceiling so `above = safety_ceiling`
	// always satisfies `validate_rate`.
	let safety_ceiling = cfg
		.maximum_borrow_rate
		.const_checked_div(FixedU128::saturating_from_integer(2u32))
		.ok_or(BenchmarkError::Stop("maximum_borrow_rate halving overflowed"))?;
	if safety_ceiling <= safety_floor {
		return Err(BenchmarkError::Stop("borrow-rate range too narrow for seeding"));
	}
	let span = safety_ceiling.saturating_sub(safety_floor);
	let divisor = FixedU128::saturating_from_integer(count.saturating_add(2));
	let step = span
		.const_checked_div(divisor)
		.ok_or(BenchmarkError::Stop("rate step computation failed"))?;
	if step.is_zero() {
		return Err(BenchmarkError::Stop("borrow-rate span too narrow for repair_budget"));
	}
	let base = safety_ceiling.saturating_sub(step);
	let above = safety_ceiling;
	let middle_offset = step.saturating_mul(FixedU128::saturating_from_integer(count / 2));
	let middle = base.saturating_sub(middle_offset);
	Ok(RateBounds { base, step, above, middle })
}

/// Seed the rate index with the smallest chain that admits a worst-case
/// stale hint (`hint_repair_budget + 2`), each insert hinted via
/// `find_position` to keep seeding O(count) — independent of the
/// hint-repair budget. Returns owners in head→tail order.
fn seed_worst_case_chain<T: Config>(
	asset: &T::AssetId,
) -> Result<Vec<T::AccountId>, BenchmarkError> {
	let count = T::VaultLists::repair_budget().saturating_add(2);
	let mut owners = Vec::with_capacity(count as usize);
	let bounds = rate_bounds::<T>(asset)?;
	for i in 0..count {
		let offset = bounds.step.saturating_mul(FixedU128::saturating_from_integer(i));
		let r = bounds.base.saturating_sub(offset);
		let who: T::AccountId = account("seed", i, 0);
		T::BenchmarkHelper::mint_collateral(asset.clone(), &who, balance::<T>(ACCOUNT_FUNDING));
		let hint = T::VaultLists::find_position(&rate_list_id(asset), r);
		Pallet::<T>::open_vault(
			RawOrigin::Signed(who.clone()).into(),
			asset.clone(),
			balance::<T>(SEED_COLL),
			balance::<T>(SEED_DEBT),
			r,
			hint,
		)?;
		owners.push(who);
	}
	Ok(owners)
}

/// Returns a head-of-list hint that is exactly `hint_repair_budget` steps
/// stale, forcing the full repair walk on insert. Pair with a priority
/// above every seeded rate to land at the new head. Errors out for
/// `repair_budget == 0` (no walk to construct) or short seed chains.
fn worst_case_head_hint<T: Config>(
	seeds: &[T::AccountId],
) -> Result<Position<T::AccountId>, BenchmarkError> {
	let s = T::VaultLists::repair_budget() as usize;
	if s == 0 || seeds.len() <= s {
		return Err(BenchmarkError::Stop("repair_budget too small for worst-case hint"));
	}
	Ok(Position::between(seeds[s - 1].clone(), seeds[s].clone()))
}

/// Plant a non-trivial branch redistribution snapshot so every `touch_vault`
/// call enters the `snap != redist` branch with non-zero `redist_collat` and
/// `redist_debt_principal`.
fn seed_pending_redistribution<T: Config>(asset: &T::AssetId) -> Result<(), BenchmarkError> {
	let per_stake = rate(REDIST_PER_STAKE_NUM, REDIST_PER_STAKE_DEN);
	let weight_per_stake = rate(REDIST_WEIGHT_PER_STAKE_NUM, REDIST_WEIGHT_PER_STAKE_DEN);

	let redist_acct = Pallet::<T>::redistribution_account();
	let pre_stage = balance::<T>(REDIST_PRESTAGE_COLL);
	T::BenchmarkHelper::mint_collateral(asset.clone(), &redist_acct, pre_stage);
	T::CollateralAssets::hold(
		asset.clone(),
		&HoldReason::VaultCollateral.into(),
		&redist_acct,
		pre_stage,
	)
	.map_err(|_| BenchmarkError::Stop("hold on redistribution account failed"))?;

	BranchStates::<T>::try_mutate(asset, |maybe| -> Result<(), BenchmarkError> {
		let bs = maybe.as_mut().ok_or(BenchmarkError::Stop("branch missing"))?;
		bs.redist.debt_per_stake = per_stake;
		bs.redist.collat_per_stake = per_stake;
		bs.redist.weight_per_stake = weight_per_stake;
		bs.redist.debt_time_per_stake = FixedU128::zero();
		bs.debt.pending_redist_principal = per_stake.saturating_mul_int(bs.stakes.total);
		Ok(())
	})
}

/// Open a fresh "only-eligible" vault, drop the oracle so it qualifies for
/// recovery, push it into the FinalRecovery FIFO via `enter_final_recovery`,
/// then restore the oracle.
fn recovery_cycle<T: Config>(
	seed_index: u32,
	asset: &T::AssetId,
) -> Result<T::AccountId, BenchmarkError> {
	let owner: T::AccountId = account("rec", seed_index, 0);
	T::BenchmarkHelper::mint_collateral(asset.clone(), &owner, balance::<T>(ACCOUNT_FUNDING));
	Pallet::<T>::open_vault(
		RawOrigin::Signed(owner.clone()).into(),
		asset.clone(),
		balance::<T>(RECOVERY_VAULT_COLL),
		balance::<T>(SEED_DEBT),
		rate(5, 100),
		Position::endpoints_only(),
	)?;
	T::BenchmarkHelper::set_oracle_price(
		asset.clone(),
		FixedU128::saturating_from_integer(RECOVERY_TRIGGER_PRICE),
	);
	let keeper: T::AccountId = whitelisted_caller();
	Pallet::<T>::enter_final_recovery(
		RawOrigin::Signed(keeper).into(),
		owner.clone(),
		asset.clone(),
	)?;
	T::BenchmarkHelper::set_oracle_price(
		asset.clone(),
		FixedU128::saturating_from_integer(ORACLE_PRICE),
	);
	Ok(owner)
}

fn prefill_branches<T: Config>(count: u32) {
	let ids: Vec<T::AssetId> = (0..count).map(T::BenchmarkHelper::synth_asset_id).collect();
	let bounded: BoundedVec<T::AssetId, T::MaxBranches> =
		ids.try_into().expect("prefill count <= MaxBranches");
	Branches::<T>::put(bounded);
}

#[benchmarks]
mod benchmarks {
	use super::*;
	use crate::pallet::Call;

	#[benchmark]
	fn open_vault() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let seeds = seed_worst_case_chain::<T>(&asset)?;
		let caller = funded_account::<T>("caller", &asset);
		let coll = balance::<T>(SEED_COLL);
		let debt = balance::<T>(SEED_DEBT);
		let hint = worst_case_head_hint::<T>(&seeds)?;
		let new_rate = rate_bounds::<T>(&asset)?.above;

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), asset.clone(), coll, debt, new_rate, hint);

		assert!(Vaults::<T>::contains_key(&asset, &caller));
		Ok(())
	}

	#[benchmark]
	fn deposit_collateral_for() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let owner = funded_account::<T>("owner", &asset);
		Pallet::<T>::open_vault(
			RawOrigin::Signed(owner.clone()).into(),
			asset.clone(),
			balance::<T>(SEED_COLL),
			balance::<T>(SEED_DEBT),
			rate(5, 100),
			Position::endpoints_only(),
		)?;
		let caller = funded_account::<T>("caller", &asset);
		let deposit = balance::<T>(SEED_COLL);
		seed_pending_redistribution::<T>(&asset)?;
		T::BenchmarkHelper::advance_time(ONE_HOUR_MS);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), owner.clone(), asset.clone(), deposit);

		assert!(Vaults::<T>::contains_key(&asset, &owner));
		Ok(())
	}

	#[benchmark]
	fn withdraw_collateral() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let caller = funded_account::<T>("caller", &asset);
		let initial_coll = balance::<T>(SEED_COLL * 10);
		Pallet::<T>::open_vault(
			RawOrigin::Signed(caller.clone()).into(),
			asset.clone(),
			initial_coll,
			balance::<T>(SEED_DEBT),
			rate(5, 100),
			Position::endpoints_only(),
		)?;
		let withdraw = balance::<T>(SEED_COLL);
		seed_pending_redistribution::<T>(&asset)?;
		T::BenchmarkHelper::advance_time(ONE_HOUR_MS);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), asset.clone(), withdraw, None);

		assert!(Vaults::<T>::contains_key(&asset, &caller));
		Ok(())
	}

	#[benchmark]
	fn borrow() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let seeds = seed_worst_case_chain::<T>(&asset)?;
		let bounds = rate_bounds::<T>(&asset)?;
		let caller = funded_account::<T>("caller", &asset);
		let caller_rate = bounds.middle;
		let caller_hint = T::VaultLists::find_position(&rate_list_id(&asset), caller_rate);
		Pallet::<T>::open_vault(
			RawOrigin::Signed(caller.clone()).into(),
			asset.clone(),
			balance::<T>(SEED_COLL * 10),
			balance::<T>(SEED_DEBT),
			caller_rate,
			caller_hint,
		)?;
		let extra_debt = balance::<T>(SEED_DEBT);
		let new_rate = Some(bounds.above);
		let hint = worst_case_head_hint::<T>(&seeds)?;
		seed_pending_redistribution::<T>(&asset)?;
		T::BenchmarkHelper::advance_time(ONE_HOUR_MS);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), asset.clone(), extra_debt, new_rate, None, hint);

		assert!(Vaults::<T>::contains_key(&asset, &caller));
		Ok(())
	}

	#[benchmark]
	fn repay_for() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let owner = funded_account::<T>("owner", &asset);
		Pallet::<T>::open_vault(
			RawOrigin::Signed(owner.clone()).into(),
			asset.clone(),
			balance::<T>(SEED_COLL),
			balance::<T>(SEED_DEBT * 10),
			rate(5, 100),
			Position::endpoints_only(),
		)?;
		let caller: T::AccountId = whitelisted_caller();
		T::StableAsset::mint_into(&caller, balance::<T>(SEED_DEBT * 100))
			.expect("mint pUSD to repay caller");
		let amount = balance::<T>(SEED_DEBT);
		seed_pending_redistribution::<T>(&asset)?;
		T::BenchmarkHelper::advance_time(ONE_HOUR_MS);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), owner.clone(), asset.clone(), amount);

		assert!(Vaults::<T>::contains_key(&asset, &owner));
		Ok(())
	}

	#[benchmark]
	fn change_rate() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let seeds = seed_worst_case_chain::<T>(&asset)?;
		let bounds = rate_bounds::<T>(&asset)?;
		let caller = funded_account::<T>("caller", &asset);
		let caller_rate = bounds.middle;
		let caller_hint = T::VaultLists::find_position(&rate_list_id(&asset), caller_rate);
		Pallet::<T>::open_vault(
			RawOrigin::Signed(caller.clone()).into(),
			asset.clone(),
			balance::<T>(SEED_COLL * 10),
			balance::<T>(SEED_DEBT),
			caller_rate,
			caller_hint,
		)?;
		let new_rate = bounds.above;
		let hint = worst_case_head_hint::<T>(&seeds)?;
		seed_pending_redistribution::<T>(&asset)?;
		T::BenchmarkHelper::advance_time(ONE_HOUR_MS);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), asset.clone(), new_rate, hint);

		assert!(Vaults::<T>::contains_key(&asset, &caller));
		Ok(())
	}

	#[benchmark]
	fn close_vault() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		// Seed worst-case chain BEFORE opening caller, then place caller at a
		// rate strictly inside the seed range so the close removes a
		// middle-of-list node (both rate-index neighbors get a node-row write).
		let _seeds = seed_worst_case_chain::<T>(&asset)?;
		let bounds = rate_bounds::<T>(&asset)?;
		let caller = funded_account::<T>("caller", &asset);
		let caller_rate = bounds.middle;
		let caller_hint = T::VaultLists::find_position(&rate_list_id(&asset), caller_rate);
		Pallet::<T>::open_vault(
			RawOrigin::Signed(caller.clone()).into(),
			asset.clone(),
			balance::<T>(SEED_COLL),
			balance::<T>(SEED_DEBT),
			caller_rate,
			caller_hint,
		)?;
		T::BenchmarkHelper::advance_time(ONE_HOUR_MS);
		let total_debt = Vaults::<T>::get(&asset, &caller)
			.ok_or(BenchmarkError::Stop("vault missing after open"))?
			.debt
			.total();
		T::StableAsset::mint_into(&caller, total_debt).expect("mint pUSD to benchmark caller");
		Pallet::<T>::repay_for(
			RawOrigin::Signed(caller.clone()).into(),
			caller.clone(),
			asset.clone(),
			total_debt,
		)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), asset.clone(), None);

		assert!(!Vaults::<T>::contains_key(&asset, &caller));
		Ok(())
	}

	#[benchmark]
	fn poke() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let owner = funded_account::<T>("owner", &asset);
		Pallet::<T>::open_vault(
			RawOrigin::Signed(owner.clone()).into(),
			asset.clone(),
			balance::<T>(SEED_COLL),
			balance::<T>(SEED_DEBT),
			rate(5, 100),
			Position::endpoints_only(),
		)?;
		seed_pending_redistribution::<T>(&asset)?;
		T::BenchmarkHelper::advance_time(ONE_HOUR_MS);
		let caller: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), owner.clone(), asset.clone());

		assert!(Vaults::<T>::contains_key(&asset, &owner));
		Ok(())
	}

	#[benchmark]
	fn enter_final_recovery() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let _prior = recovery_cycle::<T>(0, &asset)?;
		let owner = funded_account::<T>("target", &asset);
		Pallet::<T>::open_vault(
			RawOrigin::Signed(owner.clone()).into(),
			asset.clone(),
			balance::<T>(RECOVERY_VAULT_COLL),
			balance::<T>(SEED_DEBT),
			rate(5, 100),
			Position::endpoints_only(),
		)?;
		seed_pending_redistribution::<T>(&asset)?;
		T::BenchmarkHelper::advance_time(ONE_HOUR_MS);
		T::BenchmarkHelper::set_oracle_price(
			asset.clone(),
			FixedU128::saturating_from_integer(RECOVERY_TRIGGER_PRICE),
		);
		let caller: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), owner.clone(), asset.clone());

		assert_eq!(Pallet::<T>::vault_status(asset, owner), Some(VaultStatus::FinalRecovery));
		Ok(())
	}

	#[benchmark]
	fn exit_final_recovery() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let _a = recovery_cycle::<T>(0, &asset)?;
		let owner = recovery_cycle::<T>(1, &asset)?;
		let _c = recovery_cycle::<T>(2, &asset)?;
		let seeds = seed_worst_case_chain::<T>(&asset)?;
		seed_pending_redistribution::<T>(&asset)?;
		T::BenchmarkHelper::advance_time(ONE_HOUR_MS);
		let caller: T::AccountId = whitelisted_caller();
		let hint = worst_case_head_hint::<T>(&seeds)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), owner.clone(), asset.clone(), hint);

		assert_eq!(Pallet::<T>::vault_status(asset, owner), Some(VaultStatus::Active));
		Ok(())
	}

	#[benchmark]
	fn register_branch() -> Result<(), BenchmarkError> {
		let prefill = <T::MaxBranches as Get<u32>>::get().saturating_sub(1);
		prefill_branches::<T>(prefill);
		let asset = T::BenchmarkHelper::collateral_asset_id();
		let cfg = default_branch_config::<T>();
		let origin = manager_origin::<T>()?;

		#[extrinsic_call]
		_(origin, asset.clone(), cfg);

		assert!(BranchStates::<T>::contains_key(&asset));
		Ok(())
	}

	#[benchmark]
	fn set_param() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let origin = manager_origin::<T>()?;
		let new_value = rate(150, 100);

		#[extrinsic_call]
		set_minimum_collateralization_ratio(origin, asset, new_value);

		Ok(())
	}

	#[benchmark]
	fn enable_frozen_mode() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let origin = manager_origin::<T>()?;

		#[extrinsic_call]
		_(origin, asset.clone());

		let state = BranchStates::<T>::get(&asset).expect("branch state present after register");
		assert!(state.frozen.is_some());
		Ok(())
	}

	#[benchmark]
	fn on_idle_one_vault() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let owner = funded_account::<T>("owner", &asset);
		Pallet::<T>::open_vault(
			RawOrigin::Signed(owner.clone()).into(),
			asset.clone(),
			balance::<T>(SEED_COLL),
			balance::<T>(SEED_DEBT),
			rate(5, 100),
			Position::endpoints_only(),
		)?;
		seed_pending_redistribution::<T>(&asset)?;
		T::BenchmarkHelper::advance_time(ONE_HOUR_MS);
		let now = T::TimeProvider::now();

		#[block]
		{
			if Vaults::<T>::contains_key(&asset, &owner) {
				let _ = crate::helpers::update_aggregate_interest::<T>(&asset, now);
				let _ = crate::helpers::touch_vault::<T>(&asset, &owner, now);
				let _ = T::VaultLists::neighbors(&rate_list_id(&asset), &owner);
			}
		}

		assert!(Vaults::<T>::contains_key(&asset, &owner));
		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
