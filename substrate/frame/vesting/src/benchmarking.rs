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

//! Vesting pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use frame_benchmarking::{v2::*, BenchmarkError};
use frame_support::assert_ok;
use frame_system::{pallet_prelude::BlockNumberFor, RawOrigin};
use sp_runtime::traits::{Bounded, CheckedDiv, CheckedMul};

use crate::*;

const SEED: u32 = 0;

type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

fn add_locks<T: Config>(who: &T::AccountId, n: u8) {
	for id in 0..n {
		let lock_id = [id; 8];
		let locked = 256_u32;
		let reasons = WithdrawReasons::TRANSFER | WithdrawReasons::RESERVE;
		T::Currency::set_lock(lock_id, who, locked.into(), reasons);
	}
}

fn add_vesting_schedules<T: Config>(
	target: &T::AccountId,
	n: u32,
) -> Result<BalanceOf<T>, &'static str> {
	let min_transfer = T::MinVestedTransfer::get();
	let locked = min_transfer.checked_mul(&20_u32.into()).unwrap();
	// Schedule has a duration of 20.
	let per_block = min_transfer;
	let starting_block = 1_u32;

	let source = account("source", 0, SEED);
	T::Currency::make_free_balance_be(&source, BalanceOf::<T>::max_value());

	T::BlockNumberProvider::set_block_number(BlockNumberFor::<T>::zero());

	let mut total_locked: BalanceOf<T> = Zero::zero();
	for _ in 0..n {
		total_locked += locked;

		let schedule = VestingInfo::new(locked, per_block, starting_block.into());
		assert_ok!(Pallet::<T>::do_vested_transfer(&source, target, schedule));

		// Top up to guarantee we can always transfer another schedule.
		T::Currency::make_free_balance_be(&source, BalanceOf::<T>::max_value());
	}

	Ok(total_locked)
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn vest_locked(
		l: Linear<0, { MaxLocksOf::<T>::get() - 1 }>,
		s: Linear<1, T::MAX_VESTING_SCHEDULES>,
	) -> Result<(), BenchmarkError> {
		let caller = whitelisted_caller();
		T::Currency::make_free_balance_be(&caller, T::Currency::minimum_balance());

		add_locks::<T>(&caller, l as u8);
		let expected_balance = add_vesting_schedules::<T>(&caller, s)?;

		// At block zero, everything is vested.
		assert_eq!(frame_system::Pallet::<T>::block_number(), BlockNumberFor::<T>::zero());
		assert_eq!(
			Pallet::<T>::vesting_balance(&caller),
			Some(expected_balance),
			"Vesting schedule not added",
		);

		#[extrinsic_call]
		vest(RawOrigin::Signed(caller.clone()));

		// Nothing happened since everything is still vested.
		assert_eq!(
			Pallet::<T>::vesting_balance(&caller),
			Some(expected_balance),
			"Vesting schedule was removed",
		);

		Ok(())
	}

	#[benchmark]
	fn vest_unlocked(
		l: Linear<0, { MaxLocksOf::<T>::get() - 1 }>,
		s: Linear<1, T::MAX_VESTING_SCHEDULES>,
	) -> Result<(), BenchmarkError> {
		let caller = whitelisted_caller();
		T::Currency::make_free_balance_be(&caller, T::Currency::minimum_balance());

		add_locks::<T>(&caller, l as u8);
		add_vesting_schedules::<T>(&caller, s)?;

		// At block 21, everything is unlocked.
		T::BlockNumberProvider::set_block_number(21_u32.into());
		assert_eq!(
			Pallet::<T>::vesting_balance(&caller),
			Some(BalanceOf::<T>::zero()),
			"Vesting schedule still active",
		);

		#[extrinsic_call]
		vest(RawOrigin::Signed(caller.clone()));

		// Vesting schedule is removed!
		assert_eq!(Pallet::<T>::vesting_balance(&caller), None, "Vesting schedule was not removed",);

		Ok(())
	}

	#[benchmark]
	fn vest_other_locked(
		l: Linear<0, { MaxLocksOf::<T>::get() - 1 }>,
		s: Linear<1, T::MAX_VESTING_SCHEDULES>,
	) -> Result<(), BenchmarkError> {
		let other = account::<T::AccountId>("other", 0, SEED);
		let other_lookup = T::Lookup::unlookup(other.clone());

		T::Currency::make_free_balance_be(&other, T::Currency::minimum_balance());
		add_locks::<T>(&other, l as u8);
		let expected_balance = add_vesting_schedules::<T>(&other, s)?;

		// At block zero, everything is vested.
		assert_eq!(frame_system::Pallet::<T>::block_number(), BlockNumberFor::<T>::zero());
		assert_eq!(
			Pallet::<T>::vesting_balance(&other),
			Some(expected_balance),
			"Vesting schedule not added",
		);

		let caller = whitelisted_caller::<T::AccountId>();

		#[extrinsic_call]
		vest_other(RawOrigin::Signed(caller.clone()), other_lookup);

		// Nothing happened since everything is still vested.
		assert_eq!(
			Pallet::<T>::vesting_balance(&other),
			Some(expected_balance),
			"Vesting schedule was removed",
		);

		Ok(())
	}

	#[benchmark]
	fn vest_other_unlocked(
		l: Linear<0, { MaxLocksOf::<T>::get() - 1 }>,
		s: Linear<1, { T::MAX_VESTING_SCHEDULES }>,
	) -> Result<(), BenchmarkError> {
		let other = account::<T::AccountId>("other", 0, SEED);
		let other_lookup = T::Lookup::unlookup(other.clone());

		T::Currency::make_free_balance_be(&other, T::Currency::minimum_balance());
		add_locks::<T>(&other, l as u8);
		add_vesting_schedules::<T>(&other, s)?;
		// At block 21 everything is unlocked.
		T::BlockNumberProvider::set_block_number(21_u32.into());

		assert_eq!(
			Pallet::<T>::vesting_balance(&other),
			Some(BalanceOf::<T>::zero()),
			"Vesting schedule still active",
		);

		let caller = whitelisted_caller::<T::AccountId>();

		#[extrinsic_call]
		vest_other(RawOrigin::Signed(caller.clone()), other_lookup);

		// Vesting schedule is removed.
		assert_eq!(Pallet::<T>::vesting_balance(&other), None, "Vesting schedule was not removed",);

		Ok(())
	}

	#[benchmark]
	fn vested_transfer(
		l: Linear<0, { MaxLocksOf::<T>::get() - 1 }>,
		s: Linear<0, { T::MAX_VESTING_SCHEDULES - 1 }>,
	) -> Result<(), BenchmarkError> {
		let caller = whitelisted_caller();
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value());

		let target = account::<T::AccountId>("target", 0, SEED);
		let target_lookup = T::Lookup::unlookup(target.clone());
		// Give target existing locks.
		T::Currency::make_free_balance_be(&target, T::Currency::minimum_balance());
		add_locks::<T>(&target, l as u8);
		// Add one vesting schedules.
		let orig_balance = T::Currency::free_balance(&target);
		let mut expected_balance = add_vesting_schedules::<T>(&target, s)?;

		let transfer_amount = T::MinVestedTransfer::get();
		let per_block = transfer_amount.checked_div(&20_u32.into()).unwrap();
		expected_balance += transfer_amount;

		let vesting_schedule = VestingInfo::new(transfer_amount, per_block, 1_u32.into());

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), target_lookup, vesting_schedule);

		assert_eq!(
			orig_balance + expected_balance,
			T::Currency::free_balance(&target),
			"Transfer didn't happen",
		);
		assert_eq!(
			Pallet::<T>::vesting_balance(&target),
			Some(expected_balance),
			"Lock not correctly updated",
		);

		Ok(())
	}

	#[benchmark]
	fn force_vested_transfer(
		l: Linear<0, { MaxLocksOf::<T>::get() - 1 }>,
		s: Linear<0, { T::MAX_VESTING_SCHEDULES - 1 }>,
	) -> Result<(), BenchmarkError> {
		let source = account::<T::AccountId>("source", 0, SEED);
		let source_lookup = T::Lookup::unlookup(source.clone());
		T::Currency::make_free_balance_be(&source, BalanceOf::<T>::max_value());

		let target = account::<T::AccountId>("target", 0, SEED);
		let target_lookup = T::Lookup::unlookup(target.clone());
		// Give target existing locks.
		T::Currency::make_free_balance_be(&target, T::Currency::minimum_balance());
		add_locks::<T>(&target, l as u8);
		// Add one less than max vesting schedules.
		let orig_balance = T::Currency::free_balance(&target);
		let mut expected_balance = add_vesting_schedules::<T>(&target, s)?;

		let transfer_amount = T::MinVestedTransfer::get();
		let per_block = transfer_amount.checked_div(&20_u32.into()).unwrap();
		expected_balance += transfer_amount;

		let vesting_schedule = VestingInfo::new(transfer_amount, per_block, 1_u32.into());

		#[extrinsic_call]
		_(RawOrigin::Root, source_lookup, target_lookup, vesting_schedule);

		assert_eq!(
			orig_balance + expected_balance,
			T::Currency::free_balance(&target),
			"Transfer didn't happen",
		);
		assert_eq!(
			Pallet::<T>::vesting_balance(&target),
			Some(expected_balance),
			"Lock not correctly updated",
		);

		Ok(())
	}

	#[benchmark]
	fn not_unlocking_merge_schedules(
		l: Linear<0, { MaxLocksOf::<T>::get() - 1 }>,
		s: Linear<2, { T::MAX_VESTING_SCHEDULES }>,
	) -> Result<(), BenchmarkError> {
		let caller = whitelisted_caller::<T::AccountId>();
		// Give target existing locks.
		T::Currency::make_free_balance_be(&caller, T::Currency::minimum_balance());
		add_locks::<T>(&caller, l as u8);
		// Add max vesting schedules.
		let expected_balance = add_vesting_schedules::<T>(&caller, s)?;

		// Schedules are not vesting at block 0.
		assert_eq!(frame_system::Pallet::<T>::block_number(), BlockNumberFor::<T>::zero());
		assert_eq!(
			Pallet::<T>::vesting_balance(&caller),
			Some(expected_balance),
			"Vesting balance should equal sum locked of all schedules",
		);
		assert_eq!(
			Vesting::<T>::get(&caller).unwrap().len(),
			s as usize,
			"There should be exactly max vesting schedules"
		);

		#[extrinsic_call]
		merge_schedules(RawOrigin::Signed(caller.clone()), 0, s - 1);

		let expected_schedule = VestingInfo::new(
			T::MinVestedTransfer::get() * 20_u32.into() * 2_u32.into(),
			T::MinVestedTransfer::get() * 2_u32.into(),
			1_u32.into(),
		);
		let expected_index = (s - 2) as usize;
		assert_eq!(Vesting::<T>::get(&caller).unwrap()[expected_index], expected_schedule);
		assert_eq!(
			Pallet::<T>::vesting_balance(&caller),
			Some(expected_balance),
			"Vesting balance should equal total locked of all schedules",
		);
		assert_eq!(
			Vesting::<T>::get(&caller).unwrap().len(),
			(s - 1) as usize,
			"Schedule count should reduce by 1"
		);

		Ok(())
	}

	#[benchmark]
	fn unlocking_merge_schedules(
		l: Linear<0, { MaxLocksOf::<T>::get() - 1 }>,
		s: Linear<2, { T::MAX_VESTING_SCHEDULES }>,
	) -> Result<(), BenchmarkError> {
		// Destination used just for currency transfers in asserts.
		let test_dest: T::AccountId = account("test_dest", 0, SEED);

		let caller = whitelisted_caller::<T::AccountId>();
		// Give target existing locks.
		T::Currency::make_free_balance_be(&caller, T::Currency::minimum_balance());
		add_locks::<T>(&caller, l as u8);
		// Add max vesting schedules.
		let total_transferred = add_vesting_schedules::<T>(&caller, s)?;

		// Go to about half way through all the schedules duration. (They all start at 1, and have a
		// duration of 20 or 21).
		T::BlockNumberProvider::set_block_number(11_u32.into());
		// We expect half the original locked balance (+ any remainder that vests on the last
		// block).
		let expected_balance = total_transferred / 2_u32.into();
		assert_eq!(
			Pallet::<T>::vesting_balance(&caller),
			Some(expected_balance),
			"Vesting balance should reflect that we are half way through all schedules duration",
		);
		assert_eq!(
			Vesting::<T>::get(&caller).unwrap().len(),
			s as usize,
			"There should be exactly max vesting schedules"
		);
		// The balance is not actually transferable because it has not been unlocked.
		assert!(T::Currency::transfer(
			&caller,
			&test_dest,
			expected_balance,
			ExistenceRequirement::AllowDeath
		)
		.is_err());

		#[extrinsic_call]
		merge_schedules(RawOrigin::Signed(caller.clone()), 0, s - 1);

		let expected_schedule = VestingInfo::new(
			T::MinVestedTransfer::get() * 2_u32.into() * 10_u32.into(),
			T::MinVestedTransfer::get() * 2_u32.into(),
			11_u32.into(),
		);
		let expected_index = (s - 2) as usize;
		assert_eq!(
			Vesting::<T>::get(&caller).unwrap()[expected_index],
			expected_schedule,
			"New schedule is properly created and placed"
		);
		assert_eq!(
			Pallet::<T>::vesting_balance(&caller),
			Some(expected_balance),
			"Vesting balance should equal half total locked of all schedules",
		);
		assert_eq!(
			Vesting::<T>::get(&caller).unwrap().len(),
			(s - 1) as usize,
			"Schedule count should reduce by 1"
		);
		// Since merge unlocks all schedules we can now transfer the balance.
		assert_ok!(T::Currency::transfer(
			&caller,
			&test_dest,
			expected_balance,
			ExistenceRequirement::AllowDeath
		));

		Ok(())
	}

	#[benchmark]
	fn force_remove_vesting_schedule(
		l: Linear<0, { MaxLocksOf::<T>::get() - 1 }>,
		s: Linear<2, { T::MAX_VESTING_SCHEDULES }>,
	) -> Result<(), BenchmarkError> {
		let source = account::<T::AccountId>("source", 0, SEED);
		T::Currency::make_free_balance_be(&source, BalanceOf::<T>::max_value());

		let target = account::<T::AccountId>("target", 0, SEED);
		let target_lookup = T::Lookup::unlookup(target.clone());
		T::Currency::make_free_balance_be(&target, T::Currency::minimum_balance());

		// Give target existing locks.
		add_locks::<T>(&target, l as u8);
		add_vesting_schedules::<T>(&target, s)?;

		// The last vesting schedule.
		let schedule_index = s - 1;

		#[extrinsic_call]
		_(RawOrigin::Root, target_lookup, schedule_index);

		assert_eq!(
			Vesting::<T>::get(&target).unwrap().len(),
			schedule_index as usize,
			"Schedule count should reduce by 1"
		);

		Ok(())
	}

	impl_benchmark_test_suite! {
		Pallet,
		mock::ExtBuilder::default().existential_deposit(256).build(),
		mock::Test
	}
}
