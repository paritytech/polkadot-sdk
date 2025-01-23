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

//! OPF pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]
use super::*;

use crate::{Democracy::Conviction, Pallet as Opf};
//use pallet_distribution as Distribution;
pub use frame_benchmarking::{
	v1::{account, BenchmarkError},
	v2::*,
};
use frame_support::ensure;
use frame_system::RawOrigin;
use sp_runtime::traits::One;

const SEED: u32 = 0;

fn run_to_block<T: Config>(n: BlockNumberFor<T>) {
	while frame_system::Pallet::<T>::block_number() < n {
		let b = frame_system::Pallet::<T>::block_number();
		crate::Pallet::<T>::on_finalize(b);
		frame_system::Pallet::<T>::on_finalize(b);
		frame_system::Pallet::<T>::set_block_number(b + One::one());
		frame_system::Pallet::<T>::on_initialize(b);
		crate::Pallet::<T>::on_initialize(b);
	}
}

fn on_idle_full_block<T: Config>() {
	let remaining_weight = <T as frame_system::Config>::BlockWeights::get().max_block;
	let when = frame_system::Pallet::<T>::block_number();
	frame_system::Pallet::<T>::on_idle(when, remaining_weight);
	crate::Pallet::<T>::on_idle(when, remaining_weight);
}

fn add_whitelisted_project<T: Config>(n: u32, caller: T::AccountId) -> Result<(), &'static str> {
	let mut batch: Vec<_> = Vec::new();
	for _i in 0..n {
		let project_id = account("project", n, SEED);
		batch.push(project_id);
	}
	let _ = crate::Pallet::<T>::register_projects_batch(RawOrigin::Signed(caller).into(), batch);

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
	fn vote(r: Linear<1, 1000>) -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let account0: T::AccountId = account("project", r, SEED);
		add_whitelisted_project::<T>(r, caller.clone())?;
		ensure!(
			WhiteListedProjectAccounts::<T>::contains_key(account0.clone()) == true,
			"Project_id not set up correctly."
		);

		on_idle_full_block::<T>();
		let when = T::BlockNumberProvider::current_block_number()+ One::one();
		T::BlockNumberProvider::set_block_number(when);

		ensure!(VotingRounds::<T>::get(0).is_some(), "Round not created!");
		let caller_balance = T::NativeBalance::minimum_balance() * 100000000u32.into();

		let _ = T::NativeBalance::mint_into(&caller, caller_balance);

		let value: BalanceOf<T> = T::NativeBalance::minimum_balance() * 10u32.into() * (r).into();
		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), account0, value, true, Conviction::Locked1x);

		Ok(())
	}

	#[benchmark]
	fn remove_vote(r: Linear<1, 1000>) -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let account0: T::AccountId = account("project", r, SEED);
		add_whitelisted_project::<T>(r, caller.clone())?;
		ensure!(
			WhiteListedProjectAccounts::<T>::contains_key(account0.clone()) == true,
			"Project_id not set up correctly."
		);

		on_idle_full_block::<T>();
		let when = frame_system::Pallet::<T>::block_number() + One::one();
		run_to_block::<T>(when);

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
	fn release_voter_funds(r: Linear<1, 1000>) -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let account0: T::AccountId = account("project", r, SEED);
		add_whitelisted_project::<T>(r, caller.clone())?;
		ensure!(
			WhiteListedProjectAccounts::<T>::contains_key(account0.clone()) == true,
			"Project_id not set up correctly."
		);

		on_idle_full_block::<T>();
		let mut when = T::BlockNumberProvider::current_block_number().saturating_add(One::one());
		T::BlockNumberProvider::set_block_number(when);

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

		when = Votes::<T>::get(account0.clone(), caller.clone()).unwrap().funds_unlock_block;

		T::BlockNumberProvider::set_block_number(when);
		on_idle_full_block::<T>();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), account0);

		Ok(())
	}

	#[benchmark]
	fn claim_reward_for(r: Linear<1, 1000>) -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let account0: T::AccountId = account("project", r, SEED);
		add_whitelisted_project::<T>(r, caller.clone())?;
		let _pot = setup_pot_account::<T>();

		ensure!(
			WhiteListedProjectAccounts::<T>::contains_key(account0.clone()) == true,
			"Project_id not set up correctly."
		);

		let mut when = T::BlockNumberProvider::current_block_number()+ One::one();
		T::BlockNumberProvider::set_block_number(when);
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
		let round = VotingRounds::<T>::get(0).unwrap();
		let round_end = round.round_ending_block;
		// go to end of the round
		T::BlockNumberProvider::set_block_number(round_end);
		// go to claiming period
		when = round_end.saturating_add(<T as Config>::EnactmentPeriod::get());
		let enact1 = <T as Config>::EnactmentPeriod::get();
		let enact2 = <T as Democracy::Config>::EnactmentPeriod::get();
		T::BlockNumberProvider::set_block_number(when);
		println!("round_end:{:?}\nenact2:{:?}\nnow:{:?}",round_end,enact2,when);
		//SpendCreated?
		let spend = Spends::<T>::get(&account0.clone()).ok_or("Problem with spend creation")?;
		
		#[block]
		{
			on_idle_full_block::<T>();
		}
/*
		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), account0);*/

		Ok(())
	}

	impl_benchmark_test_suite!(Opf, crate::mock::new_test_ext(), crate::mock::Test);
}
