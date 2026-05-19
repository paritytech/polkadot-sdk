//! Benchmarks for `pallet-vaults`. Rate-index dispatchables feed the
//! linked-list a hint that is exactly `hint_repair_budget` steps stale, so
//! the worst-case repair walk is what gets measured.

#![cfg(feature = "runtime-benchmarks")]

use crate::{
	pallet::{BalanceOf, BranchStates, Config, MomentOf, Pallet, Vaults},
	types::{BranchConfig, VaultListId, VaultStatus},
	BenchmarkHelper as _,
};
use alloc::vec::Vec;
use frame::{
	benchmarking::prelude::*,
	deps::{
		frame_support::traits::{fungible::Mutate as FungibleMutate, EnsureOrigin, Time},
		sp_runtime::{traits::SaturatedConversion, FixedU128},
	},
};
use frame_system::RawOrigin;
use pallet_linked_list::{Position, SortedListInterface};

const ORACLE_PRICE: u128 = 10;
const ACCOUNT_FUNDING: u128 = 10_000_000;
const SEED_COLL: u128 = 1_000_000;
/// Must exceed `default_branch_config::minimum_debt` (200).
const SEED_DEBT: u128 = 300;
/// Price drop that pushes a `coll=50, debt=300` vault below the 110% MCR,
/// so `enter_final_recovery` accepts it.
const RECOVERY_TRIGGER_PRICE: u32 = 1;
/// One hour in milliseconds — enough for `poke` / `on_idle` to accrue
/// non-zero interest at the default 5% rate.
const ONE_HOUR_MS: u64 = 60 * 60 * 1_000;

fn balance<T: Config>(value: u128) -> BalanceOf<T> {
	value.saturated_into()
}

fn rate(numerator: u128, denominator: u128) -> FixedU128 {
	FixedU128::from_rational(numerator, denominator)
}

fn rate_index<T: Config>(asset: T::AssetId) -> VaultListId<T::AssetId> {
	VaultListId::Rate(asset)
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
		minimum_total_stakes: balance::<T>(100),
		minimum_borrow_rate: rate(1, 1_000),
		maximum_borrow_rate: rate(100, 100),
		upfront_fee_period: (7 * DAY_MS).saturated_into(),
		rate_adjustment_cooldown: DAY_MS.saturated_into(),
		redistribution_penalty: rate(5, 100),
	}
}

fn manager_origin<T: Config>() -> Result<T::RuntimeOrigin, BenchmarkError> {
	T::ManagerOrigin::try_successful_origin()
		.map_err(|_| BenchmarkError::Stop("manager origin unavailable"))
}

fn register_default_branch<T: Config>() -> Result<T::AssetId, BenchmarkError> {
	let asset = T::BenchmarkHelper::collateral_asset_id();
	Pallet::<T>::register_branch(manager_origin::<T>()?, asset, default_branch_config::<T>())?;
	T::BenchmarkHelper::set_oracle_price(asset, FixedU128::saturating_from_integer(ORACLE_PRICE));
	T::BenchmarkHelper::mint_collateral(
		asset,
		&Pallet::<T>::redistribution_account(),
		balance::<T>(1),
	);
	Ok(asset)
}

fn funded_account<T: Config>(seed: &'static str, asset: T::AssetId) -> T::AccountId {
	let who: T::AccountId = account(seed, 0, 0);
	T::BenchmarkHelper::mint_collateral(asset, &who, balance::<T>(ACCOUNT_FUNDING));
	who
}

/// Seed the rate index with the smallest chain that admits a worst-case
/// stale hint (`hint_repair_budget + 2`), each insert hinted via
/// `find_position` to keep seeding O(count) — independent of the
/// hint-repair budget. Returns owners in head→tail order.
fn seed_worst_case_chain<T: Config>(
	asset: T::AssetId,
) -> Result<Vec<T::AccountId>, BenchmarkError> {
	let count = T::VaultLists::repair_budget() + 2;
	let mut owners = Vec::with_capacity(count as usize);
	let base = rate(50, 100);
	let step = rate(1, 100);
	for i in 0..count {
		let offset = step.saturating_mul(FixedU128::saturating_from_integer(i));
		let r = base.saturating_sub(offset);
		let who = account("seed", i, 0);
		T::BenchmarkHelper::mint_collateral(asset, &who, balance::<T>(ACCOUNT_FUNDING));
		let hint = T::VaultLists::find_position(&rate_index::<T>(asset), r);
		Pallet::<T>::open_vault(
			RawOrigin::Signed(who.clone()).into(),
			asset,
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
/// above every seeded rate to land at the new head.
fn worst_case_head_hint<T: Config>(seeds: &[T::AccountId]) -> Position<T::AccountId> {
	let s = T::VaultLists::repair_budget() as usize;
	assert!(seeds.len() > s, "seed chain too short for worst-case hint");
	Position::between(seeds[s - 1].clone(), seeds[s].clone())
}

/// Strictly above every rate produced by `seed_worst_case_chain`.
fn rate_above_seed_head() -> FixedU128 {
	rate(60, 100)
}

#[benchmarks]
mod benchmarks {
	use super::*;
	use crate::pallet::Call;

	#[benchmark]
	fn open_vault() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let seeds = seed_worst_case_chain::<T>(asset)?;
		let caller = funded_account::<T>("caller", asset);
		let coll = balance::<T>(SEED_COLL);
		let debt = balance::<T>(SEED_DEBT);
		let hint = worst_case_head_hint::<T>(&seeds);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), asset, coll, debt, rate_above_seed_head(), hint);

		assert!(Vaults::<T>::contains_key(asset, &caller));
		Ok(())
	}

	#[benchmark]
	fn deposit_collateral_for() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let owner = funded_account::<T>("owner", asset);
		Pallet::<T>::open_vault(
			RawOrigin::Signed(owner.clone()).into(),
			asset,
			balance::<T>(SEED_COLL),
			balance::<T>(SEED_DEBT),
			rate(5, 100),
			Position::endpoints_only(),
		)?;
		let caller = funded_account::<T>("caller", asset);
		let deposit = balance::<T>(SEED_COLL);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), owner.clone(), asset, deposit);

		assert!(Vaults::<T>::contains_key(asset, &owner));
		Ok(())
	}

	#[benchmark]
	fn withdraw_collateral() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let caller = funded_account::<T>("caller", asset);
		let initial_coll = balance::<T>(SEED_COLL * 10);
		Pallet::<T>::open_vault(
			RawOrigin::Signed(caller.clone()).into(),
			asset,
			initial_coll,
			balance::<T>(SEED_DEBT),
			rate(5, 100),
			Position::endpoints_only(),
		)?;
		let withdraw = balance::<T>(SEED_COLL);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), asset, withdraw, None);

		assert!(Vaults::<T>::contains_key(asset, &caller));
		Ok(())
	}

	#[benchmark]
	fn borrow() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let seeds = seed_worst_case_chain::<T>(asset)?;
		let caller = funded_account::<T>("caller", asset);
		let caller_rate = rate(25, 100);
		let caller_hint = T::VaultLists::find_position(&rate_index::<T>(asset), caller_rate);
		Pallet::<T>::open_vault(
			RawOrigin::Signed(caller.clone()).into(),
			asset,
			balance::<T>(SEED_COLL * 10),
			balance::<T>(SEED_DEBT),
			caller_rate,
			caller_hint,
		)?;
		let extra_debt = balance::<T>(SEED_DEBT);
		let new_rate = Some(rate_above_seed_head());
		let hint = worst_case_head_hint::<T>(&seeds);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), asset, extra_debt, new_rate, None, hint);

		assert!(Vaults::<T>::contains_key(asset, &caller));
		Ok(())
	}

	#[benchmark]
	fn repay_for() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let owner = funded_account::<T>("owner", asset);
		Pallet::<T>::open_vault(
			RawOrigin::Signed(owner.clone()).into(),
			asset,
			balance::<T>(SEED_COLL),
			balance::<T>(SEED_DEBT * 10),
			rate(5, 100),
			Position::endpoints_only(),
		)?;
		let caller: T::AccountId = whitelisted_caller();
		T::StableAsset::mint_into(&caller, balance::<T>(SEED_DEBT * 100))
			.expect("mint pUSD to repay caller");
		let amount = balance::<T>(SEED_DEBT);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), owner.clone(), asset, amount);

		assert!(Vaults::<T>::contains_key(asset, &owner));
		Ok(())
	}

	#[benchmark]
	fn change_rate() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let seeds = seed_worst_case_chain::<T>(asset)?;
		let caller = funded_account::<T>("caller", asset);
		let caller_rate = rate(25, 100);
		let caller_hint = T::VaultLists::find_position(&rate_index::<T>(asset), caller_rate);
		Pallet::<T>::open_vault(
			RawOrigin::Signed(caller.clone()).into(),
			asset,
			balance::<T>(SEED_COLL * 10),
			balance::<T>(SEED_DEBT),
			caller_rate,
			caller_hint,
		)?;
		let new_rate = rate_above_seed_head();
		let hint = worst_case_head_hint::<T>(&seeds);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), asset, new_rate, hint);

		assert!(Vaults::<T>::contains_key(asset, &caller));
		Ok(())
	}

	#[benchmark]
	fn close_vault() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let caller = funded_account::<T>("caller", asset);
		Pallet::<T>::open_vault(
			RawOrigin::Signed(caller.clone()).into(),
			asset,
			balance::<T>(SEED_COLL),
			balance::<T>(SEED_DEBT),
			rate(5, 100),
			Position::endpoints_only(),
		)?;
		let total_debt = Vaults::<T>::get(asset, &caller)
			.ok_or(BenchmarkError::Stop("vault missing after open"))?
			.debt
			.total();
		T::StableAsset::mint_into(&caller, total_debt).expect("mint pUSD to benchmark caller");
		Pallet::<T>::repay_for(
			RawOrigin::Signed(caller.clone()).into(),
			caller.clone(),
			asset,
			total_debt,
		)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), asset, None);

		assert!(!Vaults::<T>::contains_key(asset, &caller));
		Ok(())
	}

	#[benchmark]
	fn poke() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let owner = funded_account::<T>("owner", asset);
		Pallet::<T>::open_vault(
			RawOrigin::Signed(owner.clone()).into(),
			asset,
			balance::<T>(SEED_COLL),
			balance::<T>(SEED_DEBT),
			rate(5, 100),
			Position::endpoints_only(),
		)?;
		T::BenchmarkHelper::advance_time(ONE_HOUR_MS);
		let caller: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), owner.clone(), asset);

		assert!(Vaults::<T>::contains_key(asset, &owner));
		Ok(())
	}

	#[benchmark]
	fn enter_final_recovery() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let owner = funded_account::<T>("owner", asset);
		Pallet::<T>::open_vault(
			RawOrigin::Signed(owner.clone()).into(),
			asset,
			balance::<T>(50),
			balance::<T>(SEED_DEBT),
			rate(5, 100),
			Position::endpoints_only(),
		)?;
		T::BenchmarkHelper::set_oracle_price(
			asset,
			FixedU128::saturating_from_integer(RECOVERY_TRIGGER_PRICE),
		);
		let caller: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), owner.clone(), asset);

		assert_eq!(Pallet::<T>::vault_status(asset, owner), Some(VaultStatus::FinalRecovery),);
		Ok(())
	}

	#[benchmark]
	fn exit_final_recovery() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let owner = funded_account::<T>("owner", asset);
		Pallet::<T>::open_vault(
			RawOrigin::Signed(owner.clone()).into(),
			asset,
			balance::<T>(50),
			balance::<T>(SEED_DEBT),
			rate_above_seed_head(),
			Position::endpoints_only(),
		)?;
		T::BenchmarkHelper::set_oracle_price(
			asset,
			FixedU128::saturating_from_integer(RECOVERY_TRIGGER_PRICE),
		);
		let keeper: T::AccountId = whitelisted_caller();
		Pallet::<T>::enter_final_recovery(RawOrigin::Signed(keeper).into(), owner.clone(), asset)?;
		T::BenchmarkHelper::set_oracle_price(
			asset,
			FixedU128::saturating_from_integer(ORACLE_PRICE),
		);
		let seeds = seed_worst_case_chain::<T>(asset)?;
		let caller: T::AccountId = whitelisted_caller();
		let hint = worst_case_head_hint::<T>(&seeds);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), owner.clone(), asset, hint);

		assert_eq!(Pallet::<T>::vault_status(asset, owner), Some(VaultStatus::Active));
		Ok(())
	}

	#[benchmark]
	fn register_branch() -> Result<(), BenchmarkError> {
		let asset = T::BenchmarkHelper::collateral_asset_id();
		let cfg = default_branch_config::<T>();
		let origin = manager_origin::<T>()?;

		#[extrinsic_call]
		_(origin, asset, cfg);

		assert!(BranchStates::<T>::contains_key(asset));
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
		_(origin, asset);

		let state = BranchStates::<T>::get(asset).expect("branch state present after register");
		assert!(state.frozen.is_some());
		Ok(())
	}

	#[benchmark]
	fn on_idle_one_vault() -> Result<(), BenchmarkError> {
		let asset = register_default_branch::<T>()?;
		let owner = funded_account::<T>("owner", asset);
		Pallet::<T>::open_vault(
			RawOrigin::Signed(owner.clone()).into(),
			asset,
			balance::<T>(SEED_COLL),
			balance::<T>(SEED_DEBT),
			rate(5, 100),
			Position::endpoints_only(),
		)?;
		T::BenchmarkHelper::advance_time(ONE_HOUR_MS);
		let now = T::TimeProvider::now();

		#[block]
		{
			if Vaults::<T>::contains_key(asset, &owner) {
				let _ = crate::helpers::update_aggregate_interest::<T>(asset, now);
				let _ = crate::helpers::touch_vault::<T>(asset, &owner, now);
				let _ = T::VaultLists::neighbors(&rate_index::<T>(asset), &owner);
			}
		}

		assert!(Vaults::<T>::contains_key(asset, &owner));
		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
