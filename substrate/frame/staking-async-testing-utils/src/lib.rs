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

//! Testing utilities for pallet-staking-async.
//!
//! This crate provides common functions to setup staking state for testing,
//! such as bonding validators, nominators, and generating different types of solutions.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use core::marker::PhantomData;

use frame_benchmarking::account;
use frame_support::traits::{fungible::Mutate, Get};
use frame_system::RawOrigin;
use pallet_staking_async::{
	ActiveEra, ActiveEraInfo, BalanceOf, Bonded, BondedEras, Config, CurrentEra,
	ErasStakersOverview, ErasTotalStake, ErasValidatorPrefs, ErasValidatorReward, Ledger,
	MinValidatorBond, Pallet, RewardDestination, StakingLedger, ValidatorPrefs,
};
use sp_runtime::{traits::Zero, Perbill};
use sp_staking::{EraIndex, PagedExposureMetadata};

// Re-export commonly used types
pub use pallet_staking_async::asset;

// Re-export core testing utilities from the main pallet
pub use pallet_staking_async::testing_utils::{
	clear_validators_and_nominators, create_funded_user, create_stash_controller,
	create_stash_controller_with_balance, create_unique_stash_controller, create_validators,
	create_validators_with_nominators_for_era, create_validators_with_seed,
	migrate_to_old_currency, set_active_era, AccountIdLookupOf,
};

const SEED: u32 = 0;

/// A simple implementation of [`EraPayout`](pallet_staking_async::EraPayout) that returns a fixed
/// payout for testing purposes.
///
/// This can be used in test runtimes where `pallet-staking-async` is a dependency to avoid
/// having to implement the `pallet_staking_async::EraPayout` trait manually in each test runtime.
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

impl<Balance: Clone, EraPayoutProvider: Get<(Balance, Balance)>>
	pallet_staking_async::EraPayout<Balance> for TestEraPayout<Balance, EraPayoutProvider>
{
	fn era_payout(
		_total_staked: Balance,
		_total_issuance: Balance,
		_era_duration_millis: u64,
	) -> (Balance, Balance) {
		EraPayoutProvider::get()
	}
}

/// Grab a funded user with max Balance.
pub fn create_funded_user_with_balance<T: Config>(
	string: &'static str,
	n: u32,
	balance: BalanceOf<T>,
) -> T::AccountId {
	let user = account(string, n, SEED);
	let _ = T::Currency::set_balance(&user, balance);
	user
}

/// Create a stash and controller pair, where payouts go to a dead payee account. This is used to
/// test worst case payout scenarios.
pub fn create_stash_and_dead_payee<T: Config>(
	n: u32,
	balance_factor: u32,
) -> Result<(T::AccountId, T::AccountId), &'static str> {
	let staker = create_funded_user::<T>("stash", n, balance_factor);
	// payee has no funds
	let payee = create_funded_user::<T>("payee", n, 0);
	let amount = asset::existential_deposit::<T>() * (balance_factor / 10).max(1).into();
	Pallet::<T>::bond(
		RawOrigin::Signed(staker.clone()).into(),
		amount,
		RewardDestination::Account(payee),
	)?;
	Ok((staker.clone(), staker))
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
	let _ = T::Currency::set_balance(&validator, balance);

	let stake = MinValidatorBond::<T>::get() * 100u32.into();
	Bonded::<T>::insert(validator.clone(), validator.clone());
	Ledger::<T>::insert(validator.clone(), StakingLedger::<T>::new(validator.clone(), stake));
	Pallet::<T>::do_add_validator(
		&validator,
		ValidatorPrefs { commission: Perbill::zero(), blocked: false },
	);

	validator
}
