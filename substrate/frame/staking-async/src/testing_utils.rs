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
use core::marker::PhantomData;
use frame_benchmarking::account;
use frame_system::RawOrigin;
use rand_chacha::{
	rand_core::{RngCore, SeedableRng},
	ChaChaRng,
};
use sp_io::hashing::blake2_256;

use frame_election_provider_support::SortedListProvider;
use frame_support::pallet_prelude::*;
use sp_runtime::{traits::StaticLookup, Perbill};

const SEED: u32 = 0;

/// A simple implementation of [`EraPayout`] that returns a fixed payout for testing purposes.
///
/// This can be used in test runtimes where `pallet-staking-async` is a dependency to avoid
/// having to implement the `EraPayout` trait manually in each test runtime.
///
/// To use this, you need to provide a parameter type that returns the desired payout tuple:
///
/// ```ignore
/// parameter_types! {
///     pub static EraPayout: (Balance, Balance) = (1000, 100);
/// }
///
/// impl pallet_staking_async::Config for Runtime {
///     // ... other config items
///     type EraPayout = TestEraPayout<Balance, EraPayout>;
/// }
/// ```
pub struct TestEraPayout<Balance, EraPayoutProvider>(PhantomData<(Balance, EraPayoutProvider)>);

impl<Balance: Clone, EraPayoutProvider: Get<(Balance, Balance)>> crate::EraPayout<Balance>
	for TestEraPayout<Balance, EraPayoutProvider>
{
	fn era_payout(
		_total_staked: Balance,
		_total_issuance: Balance,
		_era_duration_millis: u64,
	) -> (Balance, Balance) {
		EraPayoutProvider::get()
	}
}

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

/// This function generates validators and nominators who are randomly nominating
/// `edge_per_nominator` random validators (until `to_nominate` if provided).
///
/// NOTE: This function will remove any existing validators or nominators to ensure
/// we are working with a clean state.
///
/// Parameters:
/// - `validators`: number of bonded validators
/// - `nominators`: number of bonded nominators.
/// - `edge_per_nominator`: number of edge (vote) per nominator.
/// - `randomize_stake`: whether to randomize the stakes.
/// - `to_nominate`: if `Some(n)`, only the first `n` bonded validator are voted upon. Else, all of
///   them are considered and `edge_per_nominator` random validators are voted for.
///
/// Return the validators chosen to be nominated.
pub fn create_validators_with_nominators_for_era<T: Config>(
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

/// get the current era.
pub fn current_era<T: Config>() -> EraIndex {
	CurrentEra::<T>::get().unwrap_or(0)
}

/// Initialize BondedEras storage to satisfy try_state requirements.
/// BondedEras must contain the range [active_era - bonding_duration .. active_era].
pub fn initialize_bonded_eras<T: Config>(era: EraIndex, bonding_duration: u32) {
	use frame_support::BoundedVec;

	let start_era = era.saturating_sub(bonding_duration);
	let mut bonded_eras = Vec::new();

	for e in start_era..=era {
		// Use era index as session index for simplicity in tests
		bonded_eras.push((e, e));
		// Initialize ErasTotalStake for each era to satisfy era_present checks
		ErasTotalStake::<T>::insert(e, BalanceOf::<T>::zero());
	}

	let bonded_vec =
		BoundedVec::try_from(bonded_eras).expect("BondedEras should fit within bounds");
	BondedEras::<T>::put(bonded_vec);
}

/// Comprehensive staking era setup that satisfies try_state consistency checks.
/// This function sets up all the required staking storage items for proper era state,
/// allowing other pallets to use `AllPalletsWithSystem::try_state()` without issues.
///
/// Sets up:
/// - CurrentEra and ActiveEra
/// - BondedEras with proper range
/// - ErasTotalStake for each era
/// - ErasValidatorReward for each era
/// - ErasValidatorPrefs for each era (if validators provided)
/// - ErasStakersOverview for each era (if validators provided)
pub fn setup_staking_era_state<T: Config>(
	era: EraIndex,
	bonding_duration: u32,
	validators: Option<Vec<T::AccountId>>,
) {
	use sp_runtime::Perbill;

	CurrentEra::<T>::set(Some(era));
	ActiveEra::<T>::set(Some(ActiveEraInfo { index: era, start: None }));

	initialize_bonded_eras::<T>(era, bonding_duration);

	// Set up era-related storage for consistency
	let start_era = era.saturating_sub(bonding_duration);
	for e in start_era..=era {
		ErasValidatorReward::<T>::insert(e, BalanceOf::<T>::zero());

		// If validators are provided, set up their preferences and stakers overview
		if let Some(ref validator_list) = validators {
			for validator in validator_list {
				ErasValidatorPrefs::<T>::insert(
					e,
					validator,
					ValidatorPrefs { commission: Perbill::from_percent(0), blocked: false },
				);

				ErasStakersOverview::<T>::insert(
					e,
					validator,
					PagedExposureMetadata {
						total: BalanceOf::<T>::zero(),
						own: BalanceOf::<T>::zero(),
						nominator_count: 0,
						page_count: 0,
					},
				);
			}
		}
	}
}

/// Create a validator with given balance and stake.
/// Returns the validator's account id.
pub fn create_validator<T: Config>(n: u32, balance: BalanceOf<T>) -> T::AccountId {
	let validator: T::AccountId = account("validator", n, SEED);
	let _ = asset::set_stakeable_balance::<T>(&validator, balance);

	let stake = MinValidatorBond::<T>::get() * 100u32.into();
	Bonded::<T>::insert(validator.clone(), validator.clone());
	Ledger::<T>::insert(validator.clone(), StakingLedger::<T>::new(validator.clone(), stake));
	Pallet::<T>::do_add_validator(
		&validator,
		ValidatorPrefs { commission: Perbill::zero(), blocked: false },
	);

	validator
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
