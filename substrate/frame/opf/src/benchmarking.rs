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

//! # OPF pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]
use super::*;
use crate::{Conviction, Pallet as Opf};
//use pallet_distribution as Distribution;
pub use frame_benchmarking::{
	v1::{account, BenchmarkError},
	v2::*,
};
use frame_support::{assert_ok, ensure, traits::Hooks};
use frame_system::RawOrigin;
use sp_runtime::traits::One;

const SEED: u32 = 0;

pub fn next_block<T: Config>() {
	T::BlockNumberProvider::set_block_number(T::BlockNumberProvider::current_block_number().saturating_add(One::one()));
	Opf::<T>::on_idle(
		0u32.into(),
		Weight::MAX,
	);
}
pub fn run_to_block<T: Config>() {
		Opf::<T>::on_idle(
			0u32.into(),
			Weight::MAX,
		);
		next_block::<T>();

}

fn add_whitelisted_project<T: Config>(n: u32, caller: T::AccountId) -> Result<(), &'static str> {
	let mut batch = BoundedVec::<ProjectId<T>, <T as Config>::MaxProjects>::new();
	for i in 1..=n {
		let project_id = account("project", i, SEED);
		let caller_balance = T::NativeBalance::minimum_balance() * 100000000u32.into();
		T::NativeBalance::mint_into(&project_id, caller_balance)?;
		let _ = batch.try_push(project_id).map_err(|_| "Exceeded max projects")?;
	}
	crate::Pallet::<T>::register_projects_batch(T::AdminOrigin::try_successful_origin().expect("couldn'create origin"), batch)?;

	Ok(())
}

fn setup_pot_account<T: Config>() -> AccountIdOf<T> {
	let pot_account = crate::Pallet::<T>::pot_account();
	let value = T::NativeBalance::minimum_balance().saturating_mul(1_000_000_000u32.into());
	let _ = T::NativeBalance::set_balance(&pot_account, value);
	pot_account
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn register_projects_batch(
		r: Linear<1, { T::MaxProjects::get() }>,
	) -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let origin = T::AdminOrigin::try_successful_origin().expect("Failed to create origin");
		let caller_balance = T::NativeBalance::minimum_balance() * 100000000u32.into();
		T::NativeBalance::mint_into(&caller, caller_balance)?;
		let account0: T::AccountId = account("project", r, SEED);
		T::NativeBalance::mint_into(&account0, caller_balance)?;
		let mut batch = BoundedVec::<ProjectId<T>, <T as Config>::MaxProjects>::new();

		let project_id = account("project", r, SEED);
		let _ = batch.try_push(project_id).map_err(|_| "Exceeded max projects")?;
		#[extrinsic_call]
		_(origin, batch);

		assert_eq!(WhiteListedProjectAccounts::<T>::contains_key(account0.clone()), true);

		Ok(())
	}

	#[benchmark]
	fn unregister_project(r: Linear<1, { T::MaxProjects::get() }>) -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let caller_balance = T::NativeBalance::minimum_balance() * 100000000u32.into();
		T::NativeBalance::mint_into(&caller, caller_balance)?;
		let account0: T::AccountId = account("project", r, SEED);
		let origin = T::AdminOrigin::try_successful_origin().expect("Failed to create origin");
		add_whitelisted_project::<T>(r, caller.clone())?;
		assert_eq!(WhiteListedProjectAccounts::<T>::contains_key(account0.clone()), true);
		#[extrinsic_call]
		_(origin, account0.clone());

		assert_eq!(!WhiteListedProjectAccounts::<T>::contains_key(account0.clone()), true);

		Ok(())
	}

	#[benchmark]
	fn vote(r: Linear<1, { T::MaxProjects::get() }>) -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let caller_balance = T::NativeBalance::minimum_balance() * 100000000u32.into();
		T::NativeBalance::mint_into(&caller, caller_balance)?;
		let account0: T::AccountId = account("project", r, SEED);
		add_whitelisted_project::<T>(r, caller.clone())?;
		assert_eq!(WhiteListedProjectAccounts::<T>::contains_key(account0.clone()), true);

		let _ = assert_eq!(VotingRounds::<T>::get(0).is_some(), true);
		let caller_balance = T::NativeBalance::minimum_balance() * 1000000u32.into();

		T::NativeBalance::mint_into(&caller, caller_balance)?;

		let value: BalanceOf<T> = T::NativeBalance::minimum_balance() * 10u32.into() * (r).into();
		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), account0.clone(), value, true, Conviction::Locked1x);

		// Verify the vote was recorded
		let vote_info = Votes::<T>::get(r-1, caller.clone())
			.ok_or("Vote not recorded!")
			.unwrap();
		assert_eq!(vote_info.amount, value, "Vote value mismatch!");
		Ok(())
	}

	#[benchmark]
	fn remove_vote(r: Linear<1, { T::MaxProjects::get() }>) -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let caller_balance = T::NativeBalance::minimum_balance() * 100000000u32.into();
		T::NativeBalance::mint_into(&caller, caller_balance)?;
		let account0: T::AccountId = account("project", r, SEED);
		add_whitelisted_project::<T>(r, caller.clone())?;
		ensure!(
			WhiteListedProjectAccounts::<T>::contains_key(account0.clone()),
			"Project_id not set up correctly."
		);

		ensure!(VotingRounds::<T>::get(0).is_some(), "Round not created!");
		let caller_balance = T::NativeBalance::minimum_balance() * 100000000u32.into();
		let caller: T::AccountId = whitelisted_caller();
		let _ = T::NativeBalance::mint_into(&caller, caller_balance);
		let value: BalanceOf<T> = T::NativeBalance::minimum_balance() * 100u32.into() * (r).into();
		Opf::<T>::vote(
			RawOrigin::Signed(caller.clone()).into(),
			account0.clone(),
			value,
			true,
			Conviction::Locked1x,
		)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), account0);

		Ok(())
	}

	#[benchmark]
	fn release_voter_funds(r: Linear<1, { T::MaxProjects::get() }>) -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let caller_balance = T::NativeBalance::minimum_balance() * 100000000u32.into();
		T::NativeBalance::mint_into(&caller, caller_balance)?;
		let account0: T::AccountId = account("project", r, SEED);
		add_whitelisted_project::<T>(r, caller.clone())?;
		ensure!(
			WhiteListedProjectAccounts::<T>::contains_key(account0.clone()),
			"Project_id not set up correctly."
		);

		ensure!(VotingRounds::<T>::get(0).is_some(), "Round not created!");
		let caller_balance = T::NativeBalance::minimum_balance() * 100000000u32.into();

		let _ = T::NativeBalance::mint_into(&caller, caller_balance);

		let value: BalanceOf<T> = T::NativeBalance::minimum_balance() * 100u32.into() * (r).into();
		Opf::<T>::vote(
			RawOrigin::Signed(caller.clone()).into(),
			account0.clone(),
			value,
			true,
			Conviction::Locked1x,
		)?;

		let when = Votes::<T>::get(r-1, caller.clone()).unwrap().funds_unlock_block;

		T::BlockNumberProvider::set_block_number(when);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), account0, caller.clone());

		Ok(())
	}

	#[benchmark]
	fn on_registration(r: Linear<1, { T::MaxProjects::get() }>) -> Result<(), BenchmarkError> {
		let caller0: T::AccountId = account("caller", 1, SEED);
		let caller1: T::AccountId = account("caller", 2, SEED);
		let caller2: T::AccountId = account("caller", 3, SEED);
		let account0: T::AccountId = account("project", r, SEED);
		let caller_balance = T::NativeBalance::minimum_balance() * 100000000u32.into();
		T::NativeBalance::mint_into(&caller0, caller_balance)?;
		T::NativeBalance::mint_into(&caller1, caller_balance)?;
		T::NativeBalance::mint_into(&caller2, caller_balance)?;
		T::NativeBalance::mint_into(&account0, caller_balance)?;
		add_whitelisted_project::<T>(r, caller0.clone())?;
		let pot = setup_pot_account::<T>();
		assert_eq!(T::NativeBalance::balance(&pot) > Zero::zero(), true);

		ensure!(
			WhiteListedProjectAccounts::<T>::contains_key(account0.clone()),
			"Project_id not set up correctly."
		);

		ensure!(VotingRounds::<T>::get(0).is_some(), "Round not created!");
		let caller_balance = T::NativeBalance::minimum_balance() * 100000000u32.into();

		let _ = T::NativeBalance::mint_into(&caller0, caller_balance);
		let _ = T::NativeBalance::mint_into(&caller1, caller_balance);
		let _ = T::NativeBalance::mint_into(&caller2, caller_balance);
		let value: BalanceOf<T> = T::NativeBalance::minimum_balance()
			.saturating_mul(1000u32.into())
			.saturating_mul(r.into());
		let value1: BalanceOf<T> = T::NativeBalance::minimum_balance()
			.saturating_mul(100u32.into())
			.saturating_mul(r.into());

		Opf::<T>::vote(
			RawOrigin::Signed(caller0.clone()).into(),
			account0.clone(),
			value,
			true,
			Conviction::Locked1x,
		)?;
		Opf::<T>::vote(
			RawOrigin::Signed(caller1.clone()).into(),
			account0.clone(),
			value1,
			true,
			Conviction::Locked1x,
		)?;
		Opf::<T>::vote(
			RawOrigin::Signed(caller2.clone()).into(),
			account0.clone(),
			value1,
			true,
			Conviction::Locked1x,
		)?;
		let round = VotingRounds::<T>::get(0).unwrap();
		let round_end = round.round_ending_block;
		// go to end of the round
		let now = T::BlockNumberProvider::current_block_number();
		while now < round_end {
			run_to_block::<T>();
		}
		
		assert_eq!(T::Governance::referendum_count(), r.into(), "referenda not created");

		#[block]
		{
			Opf::<T>::on_idle(frame_system::Pallet::<T>::block_number(), Weight::MAX);
		}

		// go to claiming period
		let when = round_end.saturating_add(<T as Config>::EnactmentPeriod::get());
		T::BlockNumberProvider::set_block_number(when);
		let origin = RawOrigin::Root.into();

		assert_ok!(Opf::<T>::on_registration(
			origin,
			account0.clone()
		));
		assert_eq!(Spends::<T>::contains_key(&account0), true);
		Ok(())
	}

	#[benchmark]
	fn claim_reward_for(r: Linear<1, { T::MaxProjects::get() }>) -> Result<(), BenchmarkError> {
		let caller0: T::AccountId = account("caller", 1, SEED);
		let caller1: T::AccountId = account("caller", 2, SEED);
		let caller2: T::AccountId = account("caller", 3, SEED);
		let account0: T::AccountId = account("project", r, SEED);
		add_whitelisted_project::<T>(r, caller0.clone())?;
		let pot = setup_pot_account::<T>();
		assert_eq!(T::NativeBalance::balance(&pot) > Zero::zero(), true);

		ensure!(
			WhiteListedProjectAccounts::<T>::contains_key(account0.clone()),
			"Project_id not set up correctly."
		);

		ensure!(VotingRounds::<T>::get(0).is_some(), "Round not created!");
		let caller_balance = T::NativeBalance::minimum_balance() * 100000000u32.into();

		let _ = T::NativeBalance::mint_into(&caller0, caller_balance);
		let _ = T::NativeBalance::mint_into(&caller1, caller_balance);
		let _ = T::NativeBalance::mint_into(&caller2, caller_balance);
		let value: BalanceOf<T> = T::NativeBalance::minimum_balance()
			.saturating_mul(1000u32.into())
			.saturating_mul(r.into());
		let value1: BalanceOf<T> = T::NativeBalance::minimum_balance()
			.saturating_mul(100u32.into())
			.saturating_mul(r.into());

		Opf::<T>::vote(
			RawOrigin::Signed(caller0.clone()).into(),
			account0.clone(),
			value,
			true,
			Conviction::Locked1x,
		)?;
		Opf::<T>::vote(
			RawOrigin::Signed(caller1.clone()).into(),
			account0.clone(),
			value1,
			true,
			Conviction::Locked1x,
		)?;
		Opf::<T>::vote(
			RawOrigin::Signed(caller2.clone()).into(),
			account0.clone(),
			value1,
			true,
			Conviction::Locked1x,
		)?;
		let round = VotingRounds::<T>::get(0).unwrap();
		let round_end = round.round_ending_block;
		// go to end of the round
		let now = T::BlockNumberProvider::current_block_number();
		while now < round_end {
			run_to_block::<T>();
		}
		assert_eq!(T::Governance::referendum_count(), r.into(), "referenda not created");

		#[block]
		{
			Opf::<T>::on_idle(frame_system::Pallet::<T>::block_number(), Weight::MAX);
		}

		// go to claiming period
		let when = round_end.saturating_add(<T as Config>::EnactmentPeriod::get());
		T::BlockNumberProvider::set_block_number(when);

		assert_ok!(Opf::<T>::on_registration(
			RawOrigin::Signed(caller0.clone()).into(),
			account0.clone()
		));
		assert_eq!(Spends::<T>::contains_key(&account0), true);
		let claim = Spends::<T>::get(&account0).unwrap();
		assert_eq!(claim.claimed, false);
		let now = T::BlockNumberProvider::current_block_number();
		assert_eq!(claim.expire > now, true);
		assert_eq!(claim.valid_from == now, true);
		assert_eq!(WhiteListedProjectAccounts::<T>::contains_key(&account0), true);
		assert_ok!(Opf::<T>::claim_reward_for(
			RawOrigin::Signed(caller0.clone()).into(),
			account0.clone()
		));
		//Reward properly claimed
		let claim_after = Spends::<T>::get(&account0).unwrap();
		assert_eq!(WhiteListedProjectAccounts::<T>::contains_key(&account0), false);
		assert_eq!(claim_after.claimed, true);

		Ok(())
	}

	impl_benchmark_test_suite!(Opf, crate::mock::new_test_ext(), crate::mock::Test);
}
