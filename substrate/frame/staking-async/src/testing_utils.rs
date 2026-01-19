// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Testing utils for staking. Provides some common functions to setup staking state, such as
//! bonding validators, nominators, and generating different types of solutions.

use crate::{Pallet as Staking, *};
use frame_benchmarking::account;
use frame_election_provider_support::SortedListProvider;
use frame_support::pallet_prelude::*;
use frame_system::RawOrigin;
use rand_chacha::{
	rand_core::{RngCore, SeedableRng},
	ChaChaRng,
};
use sp_io::hashing::blake2_256;
use sp_runtime::{traits::StaticLookup, Perbill};

#[cfg(feature = "runtime-benchmarks")]
use crate::session_rotation::{Eras, Rotator};
#[cfg(feature = "runtime-benchmarks")]
use frame_support::traits::TryCollect;
#[cfg(feature = "runtime-benchmarks")]
use sp_runtime::traits::Zero;

const SEED: u32 = 0;

/// This function removes all validators and nominators from storage.
pub fn clear_validators_and_nominators<T: Config>() {
	#[allow(deprecated)]
	Validators::<T>::remove_all();

	// whenever we touch nominators counter we should update `T::VoterList` as well.
	#[allow(deprecated)]
	Nominators::<T>::remove_all();

	// NOTE: safe to call outside block production
	T::VoterList::unsafe_clear();
}

/// Grab a funded user.
pub fn create_funded_user<T: Config>(
	string: &'static str,
	n: u32,
	balance_factor: u32,
) -> T::AccountId {
	let user = account(string, n, SEED);
	let balance = asset::existential_deposit::<T>() * balance_factor.into();
	let _ = asset::set_stakeable_balance::<T>(&user, balance);
	user
}

/// Grab a funded user with max Balance.
pub fn create_funded_user_with_balance<T: Config>(
	string: &'static str,
	n: u32,
	balance: BalanceOf<T>,
) -> T::AccountId {
	let user = account(string, n, SEED);
	let _ = asset::set_stakeable_balance::<T>(&user, balance);
	user
}

/// Create a stash and controller pair.
pub fn create_stash_controller<T: Config>(
	n: u32,
	balance_factor: u32,
	destination: RewardDestination<T::AccountId>,
) -> Result<(T::AccountId, T::AccountId), &'static str> {
	let staker = create_funded_user::<T>("stash", n, balance_factor);
	let amount =
		asset::existential_deposit::<T>().max(1u64.into()) * (balance_factor / 10).max(1).into();
	Staking::<T>::bond(RawOrigin::Signed(staker.clone()).into(), amount, destination)?;
	Ok((staker.clone(), staker))
}

/// Create a unique stash and controller pair.
pub fn create_unique_stash_controller<T: Config>(
	n: u32,
	balance_factor: u32,
	destination: RewardDestination<T::AccountId>,
	dead_controller: bool,
) -> Result<(T::AccountId, T::AccountId), &'static str> {
	let stash = create_funded_user::<T>("stash", n, balance_factor);

	let controller = if dead_controller {
		create_funded_user::<T>("controller", n, 0)
	} else {
		create_funded_user::<T>("controller", n, balance_factor)
	};
	let amount = asset::existential_deposit::<T>() * (balance_factor / 10).max(1).into();
	Staking::<T>::bond(RawOrigin::Signed(stash.clone()).into(), amount, destination)?;

	// update ledger to be a *different* controller to stash
	if let Some(l) = Ledger::<T>::take(&stash) {
		<Ledger<T>>::insert(&controller, l);
	}
	// update bonded account to be unique controller
	<Bonded<T>>::insert(&stash, &controller);

	Ok((stash, controller))
}

/// Create a stash and controller pair with fixed balance.
pub fn create_stash_controller_with_balance<T: Config>(
	n: u32,
	balance: crate::BalanceOf<T>,
	destination: RewardDestination<T::AccountId>,
) -> Result<(T::AccountId, T::AccountId), &'static str> {
	let staker = create_funded_user_with_balance::<T>("stash", n, balance);
	Staking::<T>::bond(RawOrigin::Signed(staker.clone()).into(), balance, destination)?;
	Ok((staker.clone(), staker))
}

/// Create a stash and controller pair, where payouts go to a dead payee account. This is used to
/// test worst case payout scenarios.
pub fn create_stash_and_dead_payee<T: Config>(
	n: u32,
	balance_factor: u32,
) -> Result<(T::AccountId, T::AccountId), &'static str> {
	let staker = create_funded_user::<T>("stash", n, 0);
	// payee has no funds
	let payee = create_funded_user::<T>("payee", n, 0);
	let amount = asset::existential_deposit::<T>() * (balance_factor / 10).max(1).into();
	Staking::<T>::bond(
		RawOrigin::Signed(staker.clone()).into(),
		amount,
		RewardDestination::Account(payee),
	)?;
	Ok((staker.clone(), staker))
}

/// create `max` validators.
pub fn create_validators<T: Config>(
	max: u32,
	balance_factor: u32,
) -> Result<Vec<AccountIdLookupOf<T>>, &'static str> {
	create_validators_with_seed::<T>(max, balance_factor, 0)
}

/// create `max` validators, with a seed to help unintentional prevent account collisions.
pub fn create_validators_with_seed<T: Config>(
	max: u32,
	balance_factor: u32,
	seed: u32,
) -> Result<Vec<AccountIdLookupOf<T>>, &'static str> {
	let mut validators: Vec<AccountIdLookupOf<T>> = Vec::with_capacity(max as usize);
	for i in 0..max {
		let (stash, controller) =
			create_stash_controller::<T>(i + seed, balance_factor, RewardDestination::Staked)?;
		let validator_prefs =
			ValidatorPrefs { commission: Perbill::from_percent(50), ..Default::default() };
		Staking::<T>::validate(RawOrigin::Signed(controller).into(), validator_prefs)?;
		let stash_lookup = T::Lookup::unlookup(stash);
		validators.push(stash_lookup);
	}
	Ok(validators)
}

/// Creates validators and nominators for benchmarking. Clears existing state and creates fresh
/// validators and nominators where nominators randomly nominate `edge_per_nominator` validators
/// (limited to `to_nominate` if set).
///
/// Returns the validators chosen to be nominated.
pub fn setup_validators_with_nominators_for_era<T: Config>(
	validators: u32,
	nominators: u32,
	edge_per_nominator: usize,
	randomize_stake: bool,
	to_nominate: Option<u32>,
) -> Result<Vec<AccountIdLookupOf<T>>, &'static str> {
	clear_validators_and_nominators::<T>();

	let mut validators_stash: Vec<AccountIdLookupOf<T>> = Vec::with_capacity(validators as usize);
	let mut rng = ChaChaRng::from_seed(SEED.using_encoded(blake2_256));

	// Create validators
	for i in 0..validators {
		let balance_factor = if randomize_stake { rng.next_u32() % 255 + 10 } else { 100u32 };
		let (v_stash, v_controller) =
			create_stash_controller::<T>(i, balance_factor, RewardDestination::Staked)?;
		let validator_prefs =
			ValidatorPrefs { commission: Perbill::from_percent(50), ..Default::default() };
		Staking::<T>::validate(RawOrigin::Signed(v_controller.clone()).into(), validator_prefs)?;
		let stash_lookup = T::Lookup::unlookup(v_stash.clone());
		validators_stash.push(stash_lookup.clone());
	}

	let to_nominate = to_nominate.unwrap_or(validators_stash.len() as u32) as usize;
	let validator_chosen = validators_stash[0..to_nominate].to_vec();

	// Create nominators
	for j in 0..nominators {
		let balance_factor = if randomize_stake { rng.next_u32() % 255 + 10 } else { 100u32 };
		let (_n_stash, n_controller) =
			create_stash_controller::<T>(u32::MAX - j, balance_factor, RewardDestination::Staked)?;

		// Have them randomly validate
		let mut available_validators = validator_chosen.clone();
		let mut selected_validators: Vec<AccountIdLookupOf<T>> =
			Vec::with_capacity(edge_per_nominator);

		for _ in 0..validators.min(edge_per_nominator as u32) {
			let selected = rng.next_u32() as usize % available_validators.len();
			let validator = available_validators.remove(selected);
			selected_validators.push(validator);
			if available_validators.is_empty() {
				break
			}
		}
		Staking::<T>::nominate(
			RawOrigin::Signed(n_controller.clone()).into(),
			selected_validators,
		)?;
	}

	ValidatorCount::<T>::put(validators);

	Ok(validator_chosen)
}

/// Returns a validator with n nominators for benchmarking. Attempts to reuse existing stakers when
/// a validator with sufficient exposure exists in a claimable era, otherwise creates fresh state.
///
/// Used by: `payout_stakers_alive_staked`, `apply_slash`
///
/// NOTE: These benchmarks cannot benefit from genesis state reuse because payout/slash require
/// exposure in a *past* era (era < active_era). Genesis typically starts with active_era = 0,
/// meaning no past era exists and no exposure can be claimable. This function will always fall
/// back to creating fresh state in such cases.
#[cfg(feature = "runtime-benchmarks")]
pub fn setup_validator_with_nominators<T: Config>(
	n: u32,
	upper_bound: u32,
	dead_controller: bool,
	unique_controller: bool,
	destination: RewardDestination<T::AccountId>,
) -> Result<(T::AccountId, Vec<(T::AccountId, T::AccountId)>, EraIndex), &'static str> {
	// For payout to work, we need an era that has ended (< ActiveEra).
	// Check if there's a claimable era with valid exposure.
	let active_era = ActiveEra::<T>::get().map(|e| e.index).unwrap_or(0);

	// Try to find a claimable era (active_era - 1 if it exists)
	if active_era > 0 {
		let claimable_era = active_era - 1;

		// Try to find an existing validator with sufficient exposure in the claimable era
		// Must have actual stake (not just an empty default exposure)
		let existing_validator = Validators::<T>::iter_keys().find(|v| {
			let exposure = Eras::<T>::get_full_exposure(claimable_era, v);
			!exposure.total.is_zero() && exposure.others.len() >= n as usize
		});

		if let Some(v_stash) = existing_validator {
			// Use existing validator and its nominators
			let exposure = Eras::<T>::get_full_exposure(claimable_era, &v_stash);
			let nominators: Vec<(T::AccountId, T::AccountId)> = exposure
				.others
				.iter()
				.take(n as usize)
				.map(|ind| {
					let controller = Bonded::<T>::get(&ind.who).unwrap_or_else(|| ind.who.clone());
					(ind.who.clone(), controller)
				})
				.collect();

			// Set up era points if not already present
			if ErasRewardPoints::<T>::get(claimable_era).total == 0 {
				let reward = EraRewardPoints::<T> {
					total: 10,
					individual: vec![(v_stash.clone(), 10)].into_iter().try_collect()?,
				};
				ErasRewardPoints::<T>::insert(claimable_era, reward);
			}

			// Set up validator reward if not already present
			if ErasValidatorReward::<T>::get(claimable_era).is_none() {
				let total_payout = asset::existential_deposit::<T>()
					.saturating_mul(upper_bound.into())
					.saturating_mul(1000u32.into());
				<ErasValidatorReward<T>>::insert(claimable_era, total_payout);
			}

			return Ok((v_stash, nominators, claimable_era));
		}
	}

	// Fall back to clearing and creating fresh state
	clear_validators_and_nominators::<T>();
	let mut points_total = 0;
	let mut points_individual = Vec::new();

	let (v_stash, v_controller) = if unique_controller {
		create_unique_stash_controller::<T>(0, 100, destination.clone(), false)?
	} else {
		create_stash_controller::<T>(0, 100, destination.clone())?
	};

	let validator_prefs =
		ValidatorPrefs { commission: Perbill::from_percent(50), ..Default::default() };
	Staking::<T>::validate(RawOrigin::Signed(v_controller).into(), validator_prefs)?;
	let stash_lookup = T::Lookup::unlookup(v_stash.clone());

	points_total += 10;
	points_individual.push((v_stash.clone(), 10));

	let original_nominator_count = Nominators::<T>::count();
	let mut nominators = Vec::new();

	// Give the validator n nominators, but keep total users in the system the same.
	for i in 0..upper_bound {
		let (n_stash, n_controller) = if !dead_controller {
			create_stash_controller::<T>(u32::MAX - i, 100, destination.clone())?
		} else {
			create_unique_stash_controller::<T>(u32::MAX - i, 100, destination.clone(), true)?
		};
		if i < n {
			Staking::<T>::nominate(
				RawOrigin::Signed(n_controller.clone()).into(),
				vec![stash_lookup.clone()],
			)?;
			nominators.push((n_stash, n_controller));
		}
	}

	ValidatorCount::<T>::put(1);

	// Start a new Era
	let new_validators = Rotator::<T>::legacy_insta_plan_era();
	let new_planned_era = CurrentEra::<T>::get().unwrap_or_default();

	assert_eq!(new_validators.len(), 1, "New validators is not 1");
	assert_eq!(new_validators[0], v_stash, "Our validator was not selected");
	assert_ne!(Validators::<T>::count(), 0, "New validators count wrong");
	assert_eq!(
		Nominators::<T>::count(),
		original_nominator_count + nominators.len() as u32,
		"New nominators count wrong"
	);

	// Give Era Points
	let reward = EraRewardPoints::<T> {
		total: points_total,
		individual: points_individual.into_iter().try_collect()?,
	};

	ErasRewardPoints::<T>::insert(new_planned_era, reward);

	// Create reward pool
	let total_payout = asset::existential_deposit::<T>()
		.saturating_mul(upper_bound.into())
		.saturating_mul(1000u32.into());
	<ErasValidatorReward<T>>::insert(new_planned_era, total_payout);

	Ok((v_stash, nominators, new_planned_era))
}

/// get the current era.
pub fn current_era<T: Config>() -> EraIndex {
	CurrentEra::<T>::get().unwrap_or(0)
}

pub fn migrate_to_old_currency<T: Config>(who: T::AccountId) {
	use frame_support::traits::LockableCurrency;
	let staked = asset::staked::<T>(&who);

	// apply locks (this also adds a consumer).
	T::OldCurrency::set_lock(
		STAKING_ID,
		&who,
		staked,
		frame_support::traits::WithdrawReasons::all(),
	);
	// remove holds.
	asset::kill_stake::<T>(&who).expect("remove hold failed");

	// replicate old behaviour of explicit increment of consumer.
	frame_system::Pallet::<T>::inc_consumers(&who).expect("increment consumer failed");
}

/// Set active era to the given era index.
pub fn set_active_era<T: Config>(era: EraIndex) {
	// set the current era.
	CurrentEra::<T>::put(era);
	// set the active era.
	ActiveEra::<T>::put(ActiveEraInfo { index: era, start: None });
}
