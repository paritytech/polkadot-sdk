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

use super::*;

#[allow(unused)]
use crate::Pallet as CollatorSelection;
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_support::{
	assert_ok,
	codec::Decode,
	traits::{Currency, EnsureOrigin, Get},
};
use frame_system::{EventRecord, RawOrigin};
use pallet_authorship::EventHandler;
use pallet_session::{self as session, SessionManager};
use sp_std::prelude::*;

pub type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

const SEED: u32 = 0;

// TODO: remove if this is given in substrate commit.
macro_rules! whitelist {
	($acc:ident) => {
		frame_benchmarking::benchmarking::add_to_whitelist(
			frame_system::Account::<T>::hashed_key_for(&$acc).into(),
		);
	};
}

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

benchmarks! {
	where_clause { where T: pallet_authorship::Config + session::Config }

	set_invulnerables {
		let b in 1 .. T::MaxInvulnerables::get();
		let new_invulnerables = register_validators::<T>(b);
		let origin = T::UpdateOrigin::successful_origin();
	}: {
		assert_ok!(
			<CollatorSelection<T>>::set_invulnerables(origin, new_invulnerables.clone())
		);
	}
	verify {
		assert_last_event::<T>(Event::NewInvulnerables{invulnerables: new_invulnerables}.into());
	}

	set_desired_candidates {
		let max: u32 = 999;
		let origin = T::UpdateOrigin::successful_origin();
	}: {
		assert_ok!(
			<CollatorSelection<T>>::set_desired_candidates(origin, max.clone())
		);
	}
	verify {
		assert_last_event::<T>(Event::NewDesiredCandidates{desired_candidates: max}.into());
	}

	set_candidacy_bond {
		let bond_amount: BalanceOf<T> = T::Currency::minimum_balance() * 10u32.into();
		let origin = T::UpdateOrigin::successful_origin();
	}: {
		assert_ok!(
			<CollatorSelection<T>>::set_candidacy_bond(origin, bond_amount.clone())
		);
	}
	verify {
		assert_last_event::<T>(Event::NewCandidacyBond{bond_amount}.into());
	}

	// worse case is when we have all the max-candidate slots filled except one, and we fill that
	// one.
	register_as_candidate {
		let c in 1 .. T::MaxCandidates::get();

		<CandidacyBond<T>>::put(T::Currency::minimum_balance());
		<DesiredCandidates<T>>::put(c + 1);

		register_validators::<T>(c);
		register_candidates::<T>(c);

		let caller: T::AccountId = whitelisted_caller();
		let bond: BalanceOf<T> = T::Currency::minimum_balance() * 2u32.into();
		T::Currency::make_free_balance_be(&caller, bond.clone());

		<session::Pallet<T>>::set_keys(
			RawOrigin::Signed(caller.clone()).into(),
			keys::<T>(c + 1),
			Vec::new()
		).unwrap();

	}: _(RawOrigin::Signed(caller.clone()))
	verify {
		assert_last_event::<T>(Event::CandidateAdded{account_id: caller, deposit: bond / 2u32.into()}.into());
	}

	// worse case is the last candidate leaving.
	leave_intent {
		let c in (T::MinCandidates::get() + 1) .. T::MaxCandidates::get();
		<CandidacyBond<T>>::put(T::Currency::minimum_balance());
		<DesiredCandidates<T>>::put(c);

		register_validators::<T>(c);
		register_candidates::<T>(c);

		let leaving = <Candidates<T>>::get().last().unwrap().who.clone();
		whitelist!(leaving);
	}: _(RawOrigin::Signed(leaving.clone()))
	verify {
		assert_last_event::<T>(Event::CandidateRemoved{account_id: leaving}.into());
	}

	// worse case is paying a non-existing candidate account.
	note_author {
		<CandidacyBond<T>>::put(T::Currency::minimum_balance());
		T::Currency::make_free_balance_be(
			&<CollatorSelection<T>>::account_id(),
			T::Currency::minimum_balance() * 4u32.into(),
		);
		let author = account("author", 0, SEED);
		let new_block: T::BlockNumber = 10u32.into();

		frame_system::Pallet::<T>::set_block_number(new_block);
		assert!(T::Currency::free_balance(&author) == 0u32.into());
	}: {
		<CollatorSelection<T> as EventHandler<_, _>>::note_author(author.clone())
	} verify {
		assert!(T::Currency::free_balance(&author) > 0u32.into());
		assert_eq!(frame_system::Pallet::<T>::block_number(), new_block);
	}

	// worst case for new session.
	new_session {
		let r in 1 .. T::MaxCandidates::get();
		let c in 1 .. T::MaxCandidates::get();

		<CandidacyBond<T>>::put(T::Currency::minimum_balance());
		<DesiredCandidates<T>>::put(c);
		frame_system::Pallet::<T>::set_block_number(0u32.into());

		register_validators::<T>(c);
		register_candidates::<T>(c);

		let new_block: T::BlockNumber = 1800u32.into();
		let zero_block: T::BlockNumber = 0u32.into();
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

		let pre_length = <Candidates<T>>::get().len();

		frame_system::Pallet::<T>::set_block_number(new_block);

		assert!(<Candidates<T>>::get().len() == c as usize);
	}: {
		<CollatorSelection<T> as SessionManager<_>>::new_session(0)
	} verify {
		if c > r && non_removals >= T::MinCandidates::get() {
			assert!(<Candidates<T>>::get().len() < pre_length);
		} else if c > r && non_removals < T::MinCandidates::get() {
			assert!(<Candidates<T>>::get().len() == T::MinCandidates::get() as usize);
		} else {
			assert!(<Candidates<T>>::get().len() == pre_length);
		}
	}
}

impl_benchmark_test_suite!(CollatorSelection, crate::mock::new_test_ext(), crate::mock::Test,);
