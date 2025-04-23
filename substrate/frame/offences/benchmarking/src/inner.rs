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

//! Offences pallet benchmarking.

use alloc::{vec, vec::Vec};
use codec::Decode;
use frame_benchmarking::v2::*;
use frame_support::traits::Get;
use frame_system::{Config as SystemConfig, Pallet as System, RawOrigin};
use pallet_babe::EquivocationOffence as BabeEquivocationOffence;
use pallet_balances::Config as BalancesConfig;
use pallet_grandpa::{
	EquivocationOffence as GrandpaEquivocationOffence, TimeSlot as GrandpaTimeSlot,
};
use pallet_offences::{Config as OffencesConfig, Pallet as Offences};
use pallet_session::{
	historical::{Config as HistoricalConfig, IdentificationTuple},
	Config as SessionConfig, Pallet as Session,
};
use pallet_staking::{
	Config as StakingConfig, Exposure, IndividualExposure, MaxNominationsOf, Pallet as Staking,
	RewardDestination, ValidatorPrefs,
};
use sp_runtime::{
	traits::{Convert, Saturating, StaticLookup},
	Perbill,
};
use sp_staking::offence::ReportOffence;

const SEED: u32 = 0;

const MAX_NOMINATORS: u32 = 100;

pub struct Pallet<T: Config>(Offences<T>);

pub trait Config:
	SessionConfig<ValidatorId = <Self as frame_system::Config>::AccountId>
	+ StakingConfig
	+ OffencesConfig
	+ HistoricalConfig
	+ BalancesConfig
	+ IdTupleConvert<Self>
{
}

/// A helper trait to make sure we can convert `IdentificationTuple` coming from historical
/// and the one required by offences.
pub trait IdTupleConvert<T: HistoricalConfig + OffencesConfig> {
	/// Convert identification tuple from `historical` trait to the one expected by `offences`.
	fn convert(id: IdentificationTuple<T>) -> <T as OffencesConfig>::IdentificationTuple;
}

impl<T: HistoricalConfig + OffencesConfig> IdTupleConvert<T> for T
where
	<T as OffencesConfig>::IdentificationTuple: From<IdentificationTuple<T>>,
{
	fn convert(id: IdentificationTuple<T>) -> <T as OffencesConfig>::IdentificationTuple {
		id.into()
	}
}

type LookupSourceOf<T> = <<T as SystemConfig>::Lookup as StaticLookup>::Source;
type BalanceOf<T> = <T as StakingConfig>::CurrencyBalance;

struct Offender<T: Config> {
	pub controller: T::AccountId,
	#[allow(dead_code)]
	pub stash: T::AccountId,
	#[allow(dead_code)]
	pub nominator_stashes: Vec<T::AccountId>,
}

fn bond_amount<T: Config>() -> BalanceOf<T> {
	pallet_staking::asset::existential_deposit::<T>().saturating_mul(10_000u32.into())
}

fn create_offender<T: Config>(n: u32, nominators: u32) -> Result<Offender<T>, &'static str> {
	let stash: T::AccountId = account("stash", n, SEED);
	let stash_lookup: LookupSourceOf<T> = T::Lookup::unlookup(stash.clone());
	let reward_destination = RewardDestination::Staked;
	let amount = bond_amount::<T>();
	// add twice as much balance to prevent the account from being killed.
	let free_amount = amount.saturating_mul(2u32.into());
	pallet_staking::asset::set_stakeable_balance::<T>(&stash, free_amount);
	Staking::<T>::bond(
		RawOrigin::Signed(stash.clone()).into(),
		amount,
		reward_destination.clone(),
	)?;

	let validator_prefs =
		ValidatorPrefs { commission: Perbill::from_percent(50), ..Default::default() };
	Staking::<T>::validate(RawOrigin::Signed(stash.clone()).into(), validator_prefs)?;

	// set some fake keys for the validators.
	let keys =
		<T as SessionConfig>::Keys::decode(&mut sp_runtime::traits::TrailingZeroInput::zeroes())
			.unwrap();
	let proof: Vec<u8> = vec![0, 1, 2, 3];
	Session::<T>::set_keys(RawOrigin::Signed(stash.clone()).into(), keys, proof)?;

	let mut individual_exposures = vec![];
	let mut nominator_stashes = vec![];
	// Create n nominators
	for i in 0..nominators {
		let nominator_stash: T::AccountId =
			account("nominator stash", n * MAX_NOMINATORS + i, SEED);
		pallet_staking::asset::set_stakeable_balance::<T>(&nominator_stash, free_amount);

		Staking::<T>::bond(
			RawOrigin::Signed(nominator_stash.clone()).into(),
			amount,
			reward_destination.clone(),
		)?;

		let selected_validators: Vec<LookupSourceOf<T>> = vec![stash_lookup.clone()];
		Staking::<T>::nominate(
			RawOrigin::Signed(nominator_stash.clone()).into(),
			selected_validators,
		)?;

		individual_exposures
			.push(IndividualExposure { who: nominator_stash.clone(), value: amount });
		nominator_stashes.push(nominator_stash.clone());
	}

	let exposure = Exposure { total: amount * n.into(), own: amount, others: individual_exposures };
	let current_era = 0u32;
	Staking::<T>::add_era_stakers(current_era, stash.clone(), exposure);

	Ok(Offender { controller: stash.clone(), stash, nominator_stashes })
}

fn make_offenders<T: Config>(
	num_offenders: u32,
	num_nominators: u32,
) -> Result<Vec<IdentificationTuple<T>>, &'static str> {
	let mut offenders = vec![];
	for i in 0..num_offenders {
		let offender = create_offender::<T>(i + 1, num_nominators)?;
		// add them to the session validators -- this is needed since `FullIdentificationOf` usually
		// checks this.
		pallet_session::Validators::<T>::mutate(|v| v.push(offender.controller.clone()));
		offenders.push(offender);
	}

	let id_tuples = offenders
		.iter()
		.map(|offender| {
			<T as SessionConfig>::ValidatorIdOf::convert(offender.controller.clone())
				.expect("failed to get validator id from account id")
		})
		.map(|validator_id| {
			<T as HistoricalConfig>::FullIdentificationOf::convert(validator_id.clone())
				.map(|full_id| (validator_id, full_id))
				.unwrap()
		})
		.collect::<Vec<IdentificationTuple<T>>>();

	if pallet_staking::ActiveEra::<T>::get().is_none() {
		pallet_staking::ActiveEra::<T>::put(pallet_staking::ActiveEraInfo {
			index: 0,
			start: Some(0),
		});
	}

	Ok(id_tuples)
}

#[cfg(test)]
fn assert_all_slashes_applied<T>(offender_count: usize)
where
	T: Config,
	<T as frame_system::Config>::RuntimeEvent: TryInto<pallet_staking::Event<T>>,
	<T as frame_system::Config>::RuntimeEvent: TryInto<pallet_balances::Event<T>>,
	<T as frame_system::Config>::RuntimeEvent: TryInto<pallet_offences::Event>,
	<T as frame_system::Config>::RuntimeEvent: TryInto<frame_system::Event<T>>,
{
	// make sure that all slashes have been applied
	// deposit to reporter + reporter account endowed.
	assert_eq!(System::<T>::read_events_for_pallet::<pallet_balances::Event<T>>().len(), 2);
	// (n nominators + one validator) * slashed + Slash Reported + Slash Computed
	assert_eq!(
		System::<T>::read_events_for_pallet::<pallet_staking::Event<T>>().len(),
		1 * (offender_count + 1) as usize + 1
	);
	// offence
	assert_eq!(System::<T>::read_events_for_pallet::<pallet_offences::Event>().len(), 1);
	// reporter new account
	assert_eq!(System::<T>::read_events_for_pallet::<frame_system::Event<T>>().len(), 1);
}

#[benchmarks(
	where
		<T as frame_system::Config>::RuntimeEvent: TryInto<pallet_staking::Event<T>>,
		<T as frame_system::Config>::RuntimeEvent: TryInto<pallet_balances::Event<T>>,
		<T as frame_system::Config>::RuntimeEvent: TryInto<pallet_offences::Event>,
		<T as frame_system::Config>::RuntimeEvent: TryInto<frame_system::Event<T>>,
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	pub fn report_offence_grandpa(
		n: Linear<0, { MAX_NOMINATORS.min(MaxNominationsOf::<T>::get()) }>,
	) -> Result<(), BenchmarkError> {
		// for grandpa equivocation reports the number of reporters
		// and offenders is always 1
		let reporters = vec![account("reporter", 1, SEED)];

		// make sure reporters actually get rewarded
		Staking::<T>::set_slash_reward_fraction(Perbill::one());

		let mut offenders = make_offenders::<T>(1, n)?;
		let validator_set_count = Session::<T>::validators().len() as u32;

		let offence = GrandpaEquivocationOffence {
			time_slot: GrandpaTimeSlot { set_id: 0, round: 0 },
			session_index: 0,
			validator_set_count,
			offender: T::convert(offenders.pop().unwrap()),
		};
		assert_eq!(System::<T>::event_count(), 0);

		#[block]
		{
			let _ = Offences::<T>::report_offence(reporters, offence);
		}

		#[cfg(test)]
		{
			assert_all_slashes_applied::<T>(n as usize);
		}

		Ok(())
	}

	#[benchmark]
	fn report_offence_babe(
		n: Linear<0, { MAX_NOMINATORS.min(MaxNominationsOf::<T>::get()) }>,
	) -> Result<(), BenchmarkError> {
		// for babe equivocation reports the number of reporters
		// and offenders is always 1
		let reporters = vec![account("reporter", 1, SEED)];

		// make sure reporters actually get rewarded
		Staking::<T>::set_slash_reward_fraction(Perbill::one());

		let mut offenders = make_offenders::<T>(1, n)?;
		let validator_set_count = Session::<T>::validators().len() as u32;

		let offence = BabeEquivocationOffence {
			slot: 0u64.into(),
			session_index: 0,
			validator_set_count,
			offender: T::convert(offenders.pop().unwrap()),
		};
		assert_eq!(System::<T>::event_count(), 0);

		#[block]
		{
			let _ = Offences::<T>::report_offence(reporters, offence);
		}
		#[cfg(test)]
		{
			assert_all_slashes_applied::<T>(n as usize);
		}

		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
