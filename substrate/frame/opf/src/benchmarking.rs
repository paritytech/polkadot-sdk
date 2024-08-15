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

use crate::Pallet as Opf;
//use pallet_distribution as Distribution;
use frame_benchmarking::{
	v1::{account, BenchmarkError},
	v2::*,
};
use frame_support::ensure;
use frame_system::RawOrigin;
use sp_runtime::traits::One;

const SEED: u32 = 0;

fn run_to_block<T: Config>(n: frame_system::pallet_prelude::BlockNumberFor<T>) {
	while T::BlockNumberProvider::current_block_number() < n {
		crate::Pallet::<T>::on_finalize(T::BlockNumberProvider::current_block_number());
		frame_system::Pallet::<T>::on_finalize(T::BlockNumberProvider::current_block_number());
		frame_system::Pallet::<T>::set_block_number(
			T::BlockNumberProvider::current_block_number() + One::one(),
		);
		frame_system::Pallet::<T>::on_initialize(T::BlockNumberProvider::current_block_number());
		crate::Pallet::<T>::on_initialize(T::BlockNumberProvider::current_block_number());
	}
}

fn on_idle_full_block<T: Config>() {
	let remaining_weight = <T as frame_system::Config>::BlockWeights::get().max_block;
	let when = T::BlockNumberProvider::current_block_number();
	frame_system::Pallet::<T>::on_idle(when, remaining_weight);
	crate::Pallet::<T>::on_idle(when, remaining_weight);
}

fn add_whitelisted_project<T: Config>(n: u32) -> Result<(), &'static str> {
	for _i in 0..n {
		let project_id = account("project", n, SEED);
		WhiteListedProjectAccounts::<T>::mutate(|value| {
			let mut val = value.clone();
			let _ = val.try_push(project_id);
			*value = val;
		})
	}

	Ok(())
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn vote(r: Linear<1, { T::MaxWhitelistedProjects::get() }>) -> Result<(), BenchmarkError> {
		add_whitelisted_project::<T>(r)?;
		ensure!(
			WhiteListedProjectAccounts::<T>::get().len() as u32 == r,
			"Project_id not set up correctly."
		);

		on_idle_full_block::<T>();
		let when = T::BlockNumberProvider::current_block_number() + One::one();
		run_to_block::<T>(when);

		ensure!(VotingRounds::<T>::get(0).is_some(), "Round not created!");
		let caller_balance = T::NativeBalance::minimum_balance() * 10000u32.into();
		let caller: T::AccountId = whitelisted_caller();
		let _ = T::NativeBalance::mint_into(&caller, caller_balance);
		let account = WhiteListedProjectAccounts::<T>::get()[(r - 1) as usize].clone();
		let value: BalanceOf<T> = T::NativeBalance::minimum_balance() * 100u32.into() * (r).into();
		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), account, value, true);

		Ok(())
	}

	#[benchmark]
	fn remove_vote(
		r: Linear<1, { T::MaxWhitelistedProjects::get() }>,
	) -> Result<(), BenchmarkError> {
		add_whitelisted_project::<T>(r)?;
		ensure!(
			WhiteListedProjectAccounts::<T>::get().len() as u32 == r,
			"Project_id not set up correctly."
		);

		on_idle_full_block::<T>();
		let when = T::BlockNumberProvider::current_block_number() + One::one();
		run_to_block::<T>(when);

		ensure!(VotingRounds::<T>::get(0).is_some(), "Round not created!");
		let caller_balance = T::NativeBalance::minimum_balance() * 10000u32.into();
		let caller: T::AccountId = whitelisted_caller();
		let _ = T::NativeBalance::mint_into(&caller, caller_balance);
		let account = WhiteListedProjectAccounts::<T>::get()[(r - 1) as usize].clone();
		let value: BalanceOf<T> = T::NativeBalance::minimum_balance() * 100u32.into() * (r).into();
		Opf::<T>::vote(RawOrigin::Signed(caller.clone()).into(), account.clone(), value, true)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), account);

		Ok(())
	}

	impl_benchmark_test_suite!(Opf, crate::mock::new_test_ext(), crate::mock::Test);
}
