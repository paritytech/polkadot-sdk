// Copyright (C) 2021 Parity Technologies (UK) Ltd.
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

//! Benchmarking setup for pallet-collator-selection

#![cfg(feature = "runtime-benchmarks")]

use super::*;

#[allow(unused)]
use crate::Pallet as CollatorSelection;
use codec::Decode;
use frame_benchmarking::{
	account, impl_benchmark_test_suite, v2::*, whitelisted_caller, BenchmarkError,
};
use frame_support::{
	dispatch::DispatchResult,
	traits::{Currency, EnsureOrigin, Get, ReservableCurrency},
};
use frame_system::{pallet_prelude::BlockNumberFor, EventRecord, RawOrigin};
use pallet_authorship::EventHandler;
use pallet_session::{self as session, SessionManager};
use sp_std::prelude::*;

pub type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

const SEED: u32 = 0;

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	let events = frame_system::Pallet::<T>::events();
	let system_event: <T as frame_system::Config>::RuntimeEvent = generic_event.into();
	// compare to the last event record
	let EventRecord { event, .. } = &events[events.len() - 1];
	assert_eq!(event, &system_event);
}

fn create_funded_user<T: Config>(
	string: &'static str,
	n: u32,
	balance_factor: u32,
) -> T::AccountId {
	let user = account(string, n, SEED);
	let balance = T::Currency::minimum_balance() * balance_factor.into();
	let _ = T::Currency::make_free_balance_be(&user, balance);
	user
}

fn keys<T: Config + session::Config>(c: u32) -> <T as session::Config>::Keys {
	use rand::{RngCore, SeedableRng};

	let keys = {
		let mut keys = [0u8; 128];

		if c > 0 {
			let mut rng = rand::rngs::StdRng::seed_from_u64(c as u64);
			rng.fill_bytes(&mut keys);
		}

		keys
	};

	Decode::decode(&mut &keys[..]).unwrap()
}

fn validator<T: Config + session::Config>(c: u32) -> (T::AccountId, <T as session::Config>::Keys) {
	(create_funded_user::<T>("candidate", c, 1000), keys::<T>(c))
}

fn register_validators<T: Config + session::Config>(count: u32) -> Vec<T::AccountId> {
	let validators = (0..count).map(|c| validator::<T>(c)).collect::<Vec<_>>();

	for (who, keys) in validators.clone() {
		<session::Pallet<T>>::set_keys(RawOrigin::Signed(who).into(), keys, Vec::new()).unwrap();
	}

	validators.into_iter().map(|(who, _)| who).collect()
}

fn register_candidates<T: Config>(count: u32) {
	let candidates = (0..count).map(|c| account("candidate", c, SEED)).collect::<Vec<_>>();
	assert!(<CandidacyBond<T>>::get() > 0u32.into(), "Bond cannot be zero!");

	for who in candidates {
		T::Currency::make_free_balance_be(&who, <CandidacyBond<T>>::get() * 2u32.into());
		<CollatorSelection<T>>::register_as_candidate(RawOrigin::Signed(who).into()).unwrap();
	}
}

fn min_candidates<T: Config>() -> u32 {
	let min_collators = T::MinEligibleCollators::get();
	let invulnerable_length = <Invulnerables<T>>::get().len();
	min_collators.saturating_sub(invulnerable_length.try_into().unwrap())
}

fn min_invulnerables<T: Config>() -> u32 {
	let min_collators = T::MinEligibleCollators::get();
	let candidates_length = <Candidates<T>>::get().len();
	min_collators.saturating_sub(candidates_length.try_into().unwrap())
}

#[benchmarks(where T: pallet_authorship::Config + session::Config)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_invulnerables(
		b: Linear<1, { T::MaxInvulnerables::get() }>,
	) -> Result<(), BenchmarkError> {
		let origin =
			T::UpdateOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		let new_invulnerables = register_validators::<T>(b);
		let mut sorted_new_invulnerables = new_invulnerables.clone();
		sorted_new_invulnerables.sort();

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, new_invulnerables.clone());

		// assert that it comes out sorted
		assert_last_event::<T>(
			Event::NewInvulnerables { invulnerables: sorted_new_invulnerables }.into(),
		);
		Ok(())
	}

	#[benchmark]
	fn add_invulnerable(
		b: Linear<1, { T::MaxInvulnerables::get() - 1 }>,
		c: Linear<1, { T::MaxCandidates::get() - 1 }>,
	) -> Result<(), BenchmarkError> {
		let origin =
			T::UpdateOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		// need to fill up candidates
		<CandidacyBond<T>>::put(T::Currency::minimum_balance());
		<DesiredCandidates<T>>::put(c);
		// get accounts and keys for the `c` candidates
		let mut candidates = (0..c).map(|cc| validator::<T>(cc)).collect::<Vec<_>>();
		// add one more to the list. should not be in `b` (invulnerables) because it's the account
		// we will _add_ to invulnerables. we want it to be in `candidates` because we need the
		// weight associated with removing it.
		let (new_invulnerable, new_invulnerable_keys) = validator::<T>(b.max(c) + 1);
		candidates.push((new_invulnerable.clone(), new_invulnerable_keys));
		// set their keys ...
		for (who, keys) in candidates.clone() {
			<session::Pallet<T>>::set_keys(RawOrigin::Signed(who).into(), keys, Vec::new())
				.unwrap();
		}
		// ... and register them.
		for (who, _) in candidates {
			let deposit = <CandidacyBond<T>>::get();
			T::Currency::make_free_balance_be(&who, deposit * 1000_u32.into());
			let incoming = CandidateInfo { who: who.clone(), deposit };
			<Candidates<T>>::try_mutate(|candidates| -> DispatchResult {
				if !candidates.iter().any(|candidate| candidate.who == who) {
					T::Currency::reserve(&who, deposit)?;
					candidates.try_push(incoming).expect("we've respected the bounded vec limit");
					<LastAuthoredBlock<T>>::insert(
						who.clone(),
						frame_system::Pallet::<T>::block_number() + T::KickThreshold::get(),
					);
				}
				Ok(())
			})
			.expect("only returns ok");
		}

		// now we need to fill up invulnerables
		let mut invulnerables = register_validators::<T>(b);
		invulnerables.sort();
		let invulnerables: frame_support::BoundedVec<_, T::MaxInvulnerables> =
			frame_support::BoundedVec::try_from(invulnerables).unwrap();
		<Invulnerables<T>>::put(invulnerables);

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, new_invulnerable.clone());

		assert_last_event::<T>(Event::InvulnerableAdded { account_id: new_invulnerable }.into());
		Ok(())
	}

	#[benchmark]
	fn remove_invulnerable(
		b: Linear<{ min_invulnerables::<T>() + 1 }, { T::MaxInvulnerables::get() }>,
	) -> Result<(), BenchmarkError> {
		let origin =
			T::UpdateOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let mut invulnerables = register_validators::<T>(b);
		invulnerables.sort();
		let invulnerables: frame_support::BoundedVec<_, T::MaxInvulnerables> =
			frame_support::BoundedVec::try_from(invulnerables).unwrap();
		<Invulnerables<T>>::put(invulnerables);
		let to_remove = <Invulnerables<T>>::get().first().unwrap().clone();

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, to_remove.clone());

		assert_last_event::<T>(Event::InvulnerableRemoved { account_id: to_remove }.into());
		Ok(())
	}

	#[benchmark]
	fn set_desired_candidates() -> Result<(), BenchmarkError> {
		let max: u32 = T::MaxCandidates::get();
		let origin =
			T::UpdateOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, max);

		assert_last_event::<T>(Event::NewDesiredCandidates { desired_candidates: max }.into());
		Ok(())
	}

	#[benchmark]
	fn set_candidacy_bond() -> Result<(), BenchmarkError> {
		let bond_amount: BalanceOf<T> = T::Currency::minimum_balance() * 10u32.into();
		let origin =
			T::UpdateOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, bond_amount);

		assert_last_event::<T>(Event::NewCandidacyBond { bond_amount }.into());
		Ok(())
	}

	// worse case is when we have all the max-candidate slots filled except one, and we fill that
	// one.
	#[benchmark]
	fn register_as_candidate(c: Linear<1, { T::MaxCandidates::get() - 1 }>) {
		<CandidacyBond<T>>::put(T::Currency::minimum_balance());
		<DesiredCandidates<T>>::put(c + 1);

		register_validators::<T>(c);
		register_candidates::<T>(c);

		let caller: T::AccountId = whitelisted_caller();
		let bond: BalanceOf<T> = T::Currency::minimum_balance() * 2u32.into();
		T::Currency::make_free_balance_be(&caller, bond);

		<session::Pallet<T>>::set_keys(
			RawOrigin::Signed(caller.clone()).into(),
			keys::<T>(c + 1),
			Vec::new(),
		)
		.unwrap();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()));

		assert_last_event::<T>(
			Event::CandidateAdded { account_id: caller, deposit: bond / 2u32.into() }.into(),
		);
	}

	// worse case is the last candidate leaving.
	#[benchmark]
	fn leave_intent(c: Linear<{ min_candidates::<T>() + 1 }, { T::MaxCandidates::get() }>) {
		<CandidacyBond<T>>::put(T::Currency::minimum_balance());
		<DesiredCandidates<T>>::put(c);

		register_validators::<T>(c);
		register_candidates::<T>(c);

		let leaving = <Candidates<T>>::get().last().unwrap().who.clone();
		v2::whitelist!(leaving);

		#[extrinsic_call]
		_(RawOrigin::Signed(leaving.clone()));

		assert_last_event::<T>(Event::CandidateRemoved { account_id: leaving }.into());
	}

	// worse case is paying a non-existing candidate account.
	#[benchmark]
	fn note_author() {
		<CandidacyBond<T>>::put(T::Currency::minimum_balance());
		T::Currency::make_free_balance_be(
			&<CollatorSelection<T>>::account_id(),
			T::Currency::minimum_balance() * 4u32.into(),
		);
		let author = account("author", 0, SEED);
		let new_block: BlockNumberFor<T> = 10u32.into();

		frame_system::Pallet::<T>::set_block_number(new_block);
		assert!(T::Currency::free_balance(&author) == 0u32.into());

		#[block]
		{
			<CollatorSelection<T> as EventHandler<_, _>>::note_author(author.clone())
		}

		assert!(T::Currency::free_balance(&author) > 0u32.into());
		assert_eq!(frame_system::Pallet::<T>::block_number(), new_block);
	}

	// worst case for new session.
	#[benchmark]
	fn new_session(
		r: Linear<1, { T::MaxCandidates::get() }>,
		c: Linear<1, { T::MaxCandidates::get() }>,
	) {
		<CandidacyBond<T>>::put(T::Currency::minimum_balance());
		<DesiredCandidates<T>>::put(c);
		frame_system::Pallet::<T>::set_block_number(0u32.into());

		register_validators::<T>(c);
		register_candidates::<T>(c);

		let new_block: BlockNumberFor<T> = 1800u32.into();
		let zero_block: BlockNumberFor<T> = 0u32.into();
		let candidates = <Candidates<T>>::get();

		let non_removals = c.saturating_sub(r);

		for i in 0..c {
			<LastAuthoredBlock<T>>::insert(candidates[i as usize].who.clone(), zero_block);
		}

		if non_removals > 0 {
			for i in 0..non_removals {
				<LastAuthoredBlock<T>>::insert(candidates[i as usize].who.clone(), new_block);
			}
		} else {
			for i in 0..c {
				<LastAuthoredBlock<T>>::insert(candidates[i as usize].who.clone(), new_block);
			}
		}

		let min_candidates = min_candidates::<T>();
		let pre_length = <Candidates<T>>::get().len();

		frame_system::Pallet::<T>::set_block_number(new_block);

		assert!(<Candidates<T>>::get().len() == c as usize);

		#[block]
		{
			<CollatorSelection<T> as SessionManager<_>>::new_session(0);
		}

		if c > r && non_removals >= min_candidates {
			// candidates > removals and remaining candidates > min candidates
			// => remaining candidates should be shorter than before removal, i.e. some were
			//    actually removed.
			assert!(<Candidates<T>>::get().len() < pre_length);
		} else if c > r && non_removals < min_candidates {
			// candidates > removals and remaining candidates would be less than min candidates
			// => remaining candidates should equal min candidates, i.e. some were removed up to
			//    the minimum, but then any more were "forced" to stay in candidates.
			assert!(<Candidates<T>>::get().len() == min_candidates as usize);
		} else {
			// removals >= candidates, non removals must == 0
			// can't remove more than exist
			assert!(<Candidates<T>>::get().len() == pre_length);
		}
	}

	impl_benchmark_test_suite!(CollatorSelection, crate::mock::new_test_ext(), crate::mock::Test,);
}
