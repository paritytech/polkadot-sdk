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

//! Assets vesting pallet benchmarking.

use crate::*;
use frame::benchmarking::prelude::*;

const SEED: u32 = 0;

fn create_asset<T: Config<I>, I: 'static>() -> Result<AssetIdOf<T, I>, DispatchError> {
	let id = T::BenchmarkHelper::asset_id();
	let admin = account::<AccountIdOf<T>>("admin", 0, SEED);

	T::Assets::create(id.clone(), admin, true, 1u32.into())?;
	Ok(id)
}

fn initialize_asset_account_with_balance<T: Config<I>, I: 'static>(
	id: AssetIdOf<T, I>,
	who: &AccountIdOf<T>,
	balance: BalanceOf<T, I>,
) -> BalanceOf<T, I> {
	T::Assets::set_balance(id, &who, balance)
}

fn initialize_asset_account<T: Config<I>, I: 'static>(
	id: AssetIdOf<T, I>,
	who: &AccountIdOf<T>,
) -> BalanceOf<T, I> {
	let min_balance = T::Assets::minimum_balance(id.clone());
	initialize_asset_account_with_balance::<T, I>(id, who, min_balance)
}

fn add_vesting_schedules<T: Config<I>, I: 'static>(
	id: AssetIdOf<T, I>,
	target: &AccountIdOf<T>,
	n: u32,
) -> Result<BalanceOf<T, I>, &'static str> {
	let min_balance = T::Assets::minimum_balance(id.clone());
	let min_transfer = T::MinVestedTransfer::get().max(min_balance);
	let locked = min_transfer.checked_mul(&20_u32.into()).unwrap();

	let source = account::<AccountIdOf<T>>("source", 0, SEED);
	initialize_asset_account_with_balance::<T, I>(
		id.clone(),
		&source,
		min_balance + locked.checked_mul(&n.into()).unwrap(),
	);

	// Schedule has a duration of 20.
	let per_block = min_transfer;
	let starting_block = 1_u32;

	T::BlockNumberProvider::set_block_number(BlockNumberFor::<T>::zero());

	let mut total_locked: BalanceOf<T, I> = Zero::zero();
	for _ in 0..n {
		total_locked += locked;

		let schedule = VestingInfo::new(locked, per_block, starting_block.into());
		Pallet::<T, I>::do_vested_transfer(
			id.clone(),
			&source,
			target,
			schedule,
			Preservation::Expendable,
		)?;
	}

	Ok(total_locked)
}

#[instance_benchmarks]
mod benchmarks {
	use super::*;
	use frame::traits::tokens::Preservation::Preserve;

	#[benchmark]
	fn vest_locked(s: Linear<1, T::MAX_VESTING_SCHEDULES>) -> Result<(), BenchmarkError> {
		let id = create_asset::<T, I>()?;

		// Initialize `caller` and add vesting schedules.
		let caller = whitelisted_caller();
		initialize_asset_account::<T, I>(id.clone(), &caller);
		let expected_balance = add_vesting_schedules::<T, I>(id.clone(), &caller, s)?;

		// At block zero, everything is vested.
		assert_eq!(frame_system::Pallet::<T>::block_number(), BlockNumberFor::<T>::zero());
		assert_eq!(
			Pallet::<T, I>::vesting_balance(id.clone(), &caller),
			Some(expected_balance),
			"Vesting schedule not added",
		);

		#[extrinsic_call]
		vest(RawOrigin::Signed(caller.clone()), id.clone());

		// Nothing happened since everything is still vested.
		assert_eq!(
			Pallet::<T, I>::vesting_balance(id.clone(), &caller),
			Some(expected_balance),
			"Vesting schedule was removed",
		);

		Ok(())
	}

	#[benchmark]
	fn vest_unlocked(s: Linear<1, T::MAX_VESTING_SCHEDULES>) -> Result<(), BenchmarkError> {
		let id = create_asset::<T, I>()?;

		// Initialize `caller` and add vesting schedules.
		let caller = whitelisted_caller();
		initialize_asset_account::<T, I>(id.clone(), &caller);
		add_vesting_schedules::<T, I>(id.clone(), &caller, s)?;

		// At block 21, everything is unlocked.
		T::BlockNumberProvider::set_block_number(21_u32.into());
		assert_eq!(
			Pallet::<T, I>::vesting_balance(id.clone(), &caller),
			Some(BalanceOf::<T, I>::zero()),
			"Vesting schedule still active",
		);

		#[extrinsic_call]
		vest(RawOrigin::Signed(caller.clone()), id.clone());

		// Vesting schedule is removed!
		assert_eq!(
			Pallet::<T, I>::vesting_balance(id.clone(), &caller),
			None,
			"Vesting schedule was not removed",
		);

		Ok(())
	}

	#[benchmark]
	fn vest_other_locked(s: Linear<1, T::MAX_VESTING_SCHEDULES>) -> Result<(), BenchmarkError> {
		let id = create_asset::<T, I>()?;

		// Initialize `other` and add vesting schedules.
		let other = account::<AccountIdOf<T>>("other", 0, SEED);
		initialize_asset_account::<T, I>(id.clone(), &other);
		let expected_balance = add_vesting_schedules::<T, I>(id.clone(), &other, s)?;

		// At block zero, everything is vested.
		assert_eq!(frame_system::Pallet::<T>::block_number(), BlockNumberFor::<T>::zero());
		assert_eq!(
			Pallet::<T, I>::vesting_balance(id.clone(), &other),
			Some(expected_balance),
			"Vesting schedule not added",
		);

		let caller = whitelisted_caller::<AccountIdOf<T>>();
		let other_lookup = T::Lookup::unlookup(other.clone());

		#[extrinsic_call]
		vest_other(RawOrigin::Signed(caller.clone()), id.clone(), other_lookup);

		// Nothing happened since everything is still vested.
		assert_eq!(
			Pallet::<T, I>::vesting_balance(id.clone(), &other),
			Some(expected_balance),
			"Vesting schedule was removed",
		);

		Ok(())
	}

	#[benchmark]
	fn vest_other_unlocked(s: Linear<1, T::MAX_VESTING_SCHEDULES>) -> Result<(), BenchmarkError> {
		let id = create_asset::<T, I>()?;

		// Initialize `other` and add vesting schedules.
		let other = account::<AccountIdOf<T>>("other", 0, SEED);
		initialize_asset_account::<T, I>(id.clone(), &other);
		add_vesting_schedules::<T, I>(id.clone(), &other, s)?;

		// At block 21 everything is unlocked.
		T::BlockNumberProvider::set_block_number(21_u32.into());
		assert_eq!(
			Pallet::<T, I>::vesting_balance(id.clone(), &other),
			Some(BalanceOf::<T, I>::zero()),
			"Vesting schedule still active",
		);

		let caller = whitelisted_caller::<T::AccountId>();
		let other_lookup = T::Lookup::unlookup(other.clone());

		#[extrinsic_call]
		vest_other(RawOrigin::Signed(caller.clone()), id.clone(), other_lookup);

		// Vesting schedule is removed.
		assert_eq!(
			Pallet::<T, I>::vesting_balance(id.clone(), &other),
			None,
			"Vesting schedule was not removed",
		);

		Ok(())
	}

	#[benchmark]
	fn force_vested_transfer(
		s: Linear<0, { T::MAX_VESTING_SCHEDULES - 1 }>,
	) -> Result<(), BenchmarkError> {
		let id = create_asset::<T, I>()?;

		// Prepare schedule vested transfer of `MinVestedTransfer` across 20 blocks.
		// Note: MinVestedTransfer might be 1
		let transfer_amount = T::MinVestedTransfer::get() * 20_u32.into();

		// Initialize `source` with max balance.
		let source = account::<AccountIdOf<T>>("transfer_source", 0, SEED);
		initialize_asset_account_with_balance::<T, I>(
			id.clone(),
			&source,
			T::Assets::minimum_balance(id.clone()) + transfer_amount,
		);

		// Initialize `target`.
		let target = account::<AccountIdOf<T>>("target", 0, SEED);
		initialize_asset_account::<T, I>(id.clone(), &target);

		// Add one less than max vesting schedules.
		let orig_balance = T::Assets::total_balance(id.clone(), &target);
		let mut expected_balance = add_vesting_schedules::<T, I>(id.clone(), &target, s)?;

		let per_block = transfer_amount.checked_div(&20_u32.into()).unwrap();
		expected_balance += transfer_amount;

		let source_lookup = T::Lookup::unlookup(source.clone());
		let target_lookup = T::Lookup::unlookup(target.clone());
		let vesting_schedule = VestingInfo::new(transfer_amount, per_block, 1_u32.into());

		#[extrinsic_call]
		_(RawOrigin::Root, id.clone(), source_lookup, target_lookup, vesting_schedule);

		assert_eq!(
			orig_balance + expected_balance,
			T::Assets::total_balance(id.clone(), &target),
			"Transfer didn't happen",
		);
		assert_eq!(
			Pallet::<T, I>::vesting_balance(id.clone(), &target),
			Some(expected_balance),
			"Lock not correctly updated",
		);

		Ok(())
	}

	#[benchmark]
	fn not_unlocking_merge_schedules(
		s: Linear<2, { T::MAX_VESTING_SCHEDULES }>,
	) -> Result<(), BenchmarkError> {
		let id = create_asset::<T, I>()?;

		// Initialize `caller` and add vesting schedules.
		let caller = whitelisted_caller::<AccountIdOf<T>>();
		initialize_asset_account::<T, I>(id.clone(), &caller);
		let expected_balance = add_vesting_schedules::<T, I>(id.clone(), &caller, s)?;

		// Schedules are not vesting at block 0.
		assert_eq!(frame_system::Pallet::<T>::block_number(), BlockNumberFor::<T>::zero());
		assert_eq!(
			Pallet::<T, I>::vesting_balance(id.clone(), &caller),
			Some(expected_balance),
			"Vesting balance should equal sum locked of all schedules",
		);
		assert_eq!(
			Vesting::<T, I>::get(id.clone(), &caller).unwrap().len(),
			s as usize,
			"There should be exactly max vesting schedules"
		);

		#[extrinsic_call]
		merge_schedules(RawOrigin::Signed(caller.clone()), id.clone(), 0, s - 1);

		let expected_schedule = VestingInfo::new(
			T::MinVestedTransfer::get() * 20_u32.into() * 2_u32.into(),
			T::MinVestedTransfer::get() * 2_u32.into(),
			1_u32.into(),
		);
		let expected_index = (s - 2) as usize;
		assert_eq!(
			Vesting::<T, I>::get(id.clone(), &caller).unwrap()[expected_index],
			expected_schedule
		);
		assert_eq!(
			Pallet::<T, I>::vesting_balance(id.clone(), &caller),
			Some(expected_balance),
			"Vesting balance should equal total locked of all schedules",
		);
		assert_eq!(
			Vesting::<T, I>::get(id.clone(), &caller).unwrap().len(),
			(s - 1) as usize,
			"Schedule count should reduce by 1"
		);

		Ok(())
	}

	#[benchmark]
	fn unlocking_merge_schedules(
		s: Linear<2, { T::MAX_VESTING_SCHEDULES }>,
	) -> Result<(), BenchmarkError> {
		let id = create_asset::<T, I>()?;

		// Destination used just for transfers in asserts.
		let test_dest: AccountIdOf<T> = account("test_dest", 0, SEED);

		// Initialize `caller` and add vesting schedules.
		let caller = whitelisted_caller::<AccountIdOf<T>>();
		initialize_asset_account::<T, I>(id.clone(), &caller);
		let total_transferred = add_vesting_schedules::<T, I>(id.clone(), &caller, s)?;

		// Go to about halfway through all the schedules' duration. (They all start at 1, and have a
		// duration of 20 or 21).
		T::BlockNumberProvider::set_block_number(11_u32.into());
		// We expect half the original locked balance (+ any remainder that vests on the last
		// block).
		let expected_balance = total_transferred / 2_u32.into();

		assert_eq!(
			Pallet::<T, I>::vesting_balance(id.clone(), &caller),
			Some(expected_balance),
			"Vesting balance should reflect that we are half way through all schedules duration",
		);
		assert_eq!(
			Vesting::<T, I>::get(id.clone(), &caller).unwrap().len(),
			s as usize,
			"There should be exactly max vesting schedules"
		);

		// The balance is not actually transferable because it has not been unlocked.
		assert!(T::Assets::transfer(id.clone(), &caller, &test_dest, expected_balance, Preserve)
			.is_err());

		#[extrinsic_call]
		merge_schedules(RawOrigin::Signed(caller.clone()), id.clone(), 0, s - 1);

		let expected_schedule = VestingInfo::new(
			T::MinVestedTransfer::get() * 2_u32.into() * 10_u32.into(),
			T::MinVestedTransfer::get() * 2_u32.into(),
			11_u32.into(),
		);
		let expected_index = (s - 2) as usize;
		assert_eq!(
			Vesting::<T, I>::get(id.clone(), &caller).unwrap()[expected_index],
			expected_schedule,
			"New schedule is properly created and placed"
		);
		assert_eq!(
			Pallet::<T, I>::vesting_balance(id.clone(), &caller),
			Some(expected_balance),
			"Vesting balance should equal half total locked of all schedules",
		);
		assert_eq!(
			Vesting::<T, I>::get(id.clone(), &caller).unwrap().len(),
			(s - 1) as usize,
			"Schedule count should reduce by 1"
		);
		// Since merge unlocks all schedules we can now transfer the balance.
		T::Assets::transfer(id, &caller, &test_dest, expected_balance, Preserve)?;

		Ok(())
	}

	#[benchmark]
	fn force_remove_vesting_schedule(
		s: Linear<2, { T::MAX_VESTING_SCHEDULES }>,
	) -> Result<(), BenchmarkError> {
		let id = create_asset::<T, I>()?;

		// Initialize `caller` and add vesting schedules.
		let target = account::<AccountIdOf<T>>("target", 0, SEED);
		initialize_asset_account::<T, I>(id.clone(), &target);
		add_vesting_schedules::<T, I>(id.clone(), &target, s)?;

		// The last vesting schedule.
		let schedule_index = s - 1;

		let target_lookup = T::Lookup::unlookup(target.clone());

		#[extrinsic_call]
		_(RawOrigin::Root, id.clone(), target_lookup, schedule_index);

		assert_eq!(
			Vesting::<T, I>::get(id.clone(), &target).unwrap().len(),
			schedule_index as usize,
			"Schedule count should reduce by 1"
		);

		Ok(())
	}

	impl_benchmark_test_suite! {
		Pallet,
		frame::testing_prelude::TestExternalities::default(),
		mock::Test
	}
}
