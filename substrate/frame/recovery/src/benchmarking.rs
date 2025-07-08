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

#![cfg(feature = "runtime-benchmarks")]

use super::*;

use crate::Pallet;
use alloc::{boxed::Box, vec, vec::Vec};
use frame::benchmarking::prelude::*;

const SEED: u32 = 0;
const DEFAULT_DELAY: u32 = 0;

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

fn assert_has_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_has_event(generic_event.into());
}

fn get_total_deposit<T: Config>(
	bounded_friends: &FriendsOf<T>,
) -> Option<<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance>
{
	let friend_deposit = T::FriendDepositFactor::get()
		.checked_mul(&bounded_friends.len().saturated_into())
		.unwrap();

	T::ConfigDepositBase::get().checked_add(&friend_deposit)
}

fn generate_friends<T: Config>(num: u32) -> Vec<<T as frame_system::Config>::AccountId> {
	// Create friends
	let mut friends = (0..num).map(|x| account("friend", x, SEED)).collect::<Vec<_>>();
	// Sort
	friends.sort();

	for friend in 0..friends.len() {
		// Top up accounts of friends
		T::Currency::make_free_balance_be(
			&friends.get(friend).unwrap(),
			BalanceOf::<T>::max_value(),
		);
	}

	friends
}

fn add_caller_and_generate_friends<T: Config>(
	caller: T::AccountId,
	num: u32,
) -> Vec<<T as frame_system::Config>::AccountId> {
	// Create friends
	let mut friends = generate_friends::<T>(num - 1);

	T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value());

	friends.push(caller);

	// Sort
	friends.sort();

	friends
}

fn insert_recovery_config_with_max_friends<T: Config>(account: &T::AccountId) {
	T::Currency::make_free_balance_be(&account, BalanceOf::<T>::max_value());

	let n = T::MaxFriends::get();

	let friends = generate_friends::<T>(n);

	let bounded_friends: FriendsOf<T> = friends.try_into().unwrap();

	// Get deposit for recovery
	let total_deposit = get_total_deposit::<T>(&bounded_friends).unwrap();

	let recovery_config = RecoveryConfig {
		delay_period: DEFAULT_DELAY.into(),
		deposit: total_deposit,
		friends: bounded_friends,
		threshold: n as u16,
	};

	// Reserve deposit for recovery
	T::Currency::reserve(&account, total_deposit).unwrap();

	<Recoverable<T>>::insert(&account, recovery_config);
}

fn setup_active_recovery_with_max_friends<T: Config>(
	caller: &T::AccountId,
	lost_account: &T::AccountId,
) {
	insert_recovery_config_with_max_friends::<T>(&lost_account);
	let n = T::MaxFriends::get();
	let friends = generate_friends::<T>(n);
	let bounded_friends: FriendsOf<T> = friends.try_into().unwrap();

	let initial_recovery_deposit = T::RecoveryDeposit::get();
	T::Currency::reserve(caller, initial_recovery_deposit).unwrap();

	let active_recovery = ActiveRecovery {
		created: DEFAULT_DELAY.into(),
		deposit: initial_recovery_deposit,
		friends: bounded_friends,
	};
	<ActiveRecoveries<T>>::insert(lost_account, caller, active_recovery);
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn as_recovered() {
		let caller: T::AccountId = whitelisted_caller();
		let recovered_account: T::AccountId = account("recovered_account", 0, SEED);
		let recovered_account_lookup = T::Lookup::unlookup(recovered_account.clone());
		let call: <T as Config>::RuntimeCall =
			frame_system::Call::<T>::remark { remark: vec![] }.into();

		Proxy::<T>::insert(&caller, &recovered_account);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), recovered_account_lookup, Box::new(call))
	}

	#[benchmark]
	fn set_recovered() {
		let lost: T::AccountId = whitelisted_caller();
		let lost_lookup = T::Lookup::unlookup(lost.clone());
		let rescuer: T::AccountId = whitelisted_caller();
		let rescuer_lookup = T::Lookup::unlookup(rescuer.clone());

		#[extrinsic_call]
		_(RawOrigin::Root, lost_lookup, rescuer_lookup);

		assert_last_event::<T>(
			Event::AccountRecovered { lost_account: lost, rescuer_account: rescuer }.into(),
		);
	}

	#[benchmark]
	fn create_recovery(n: Linear<1, { T::MaxFriends::get() }>) {
		let caller: T::AccountId = whitelisted_caller();
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value());

		// Create friends
		let friends = generate_friends::<T>(n);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), friends, n as u16, DEFAULT_DELAY.into());

		assert_last_event::<T>(Event::RecoveryCreated { account: caller }.into());
	}

	#[benchmark]
	fn initiate_recovery() {
		let caller: T::AccountId = whitelisted_caller();
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value());

		let lost_account: T::AccountId = account("lost_account", 0, SEED);
		let lost_account_lookup = T::Lookup::unlookup(lost_account.clone());

		insert_recovery_config_with_max_friends::<T>(&lost_account);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), lost_account_lookup);

		assert_last_event::<T>(
			Event::RecoveryInitiated { lost_account, rescuer_account: caller }.into(),
		);
	}

	#[benchmark]
	fn vouch_recovery(n: Linear<1, { T::MaxFriends::get() }>) {
		let caller: T::AccountId = whitelisted_caller();
		let lost_account: T::AccountId = account("lost_account", 0, SEED);
		let lost_account_lookup = T::Lookup::unlookup(lost_account.clone());
		let rescuer_account: T::AccountId = account("rescuer_account", 0, SEED);
		let rescuer_account_lookup = T::Lookup::unlookup(rescuer_account.clone());

		// Create friends
		let friends = add_caller_and_generate_friends::<T>(caller.clone(), n);
		let bounded_friends: FriendsOf<T> = friends.try_into().unwrap();

		// Get deposit for recovery
		let total_deposit = get_total_deposit::<T>(&bounded_friends).unwrap();

		let recovery_config = RecoveryConfig {
			delay_period: DEFAULT_DELAY.into(),
			deposit: total_deposit,
			friends: bounded_friends.clone(),
			threshold: n as u16,
		};

		// Create the recovery config storage item
		<Recoverable<T>>::insert(&lost_account, recovery_config.clone());

		// Reserve deposit for recovery
		T::Currency::reserve(&caller, total_deposit).unwrap();

		// Create an active recovery status
		let recovery_status = ActiveRecovery {
			created: DEFAULT_DELAY.into(),
			deposit: total_deposit,
			friends: generate_friends::<T>(n - 1).try_into().unwrap(),
		};

		// Create the active recovery storage item
		<ActiveRecoveries<T>>::insert(&lost_account, &rescuer_account, recovery_status);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), lost_account_lookup, rescuer_account_lookup);
		assert_last_event::<T>(
			Event::RecoveryVouched { lost_account, rescuer_account, sender: caller }.into(),
		);
	}

	#[benchmark]
	fn claim_recovery(n: Linear<1, { T::MaxFriends::get() }>) {
		let caller: T::AccountId = whitelisted_caller();
		let lost_account: T::AccountId = account("lost_account", 0, SEED);
		let lost_account_lookup = T::Lookup::unlookup(lost_account.clone());

		T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value());

		// Create friends
		let friends = generate_friends::<T>(n);
		let bounded_friends: FriendsOf<T> = friends.try_into().unwrap();

		// Get deposit for recovery
		let total_deposit = get_total_deposit::<T>(&bounded_friends).unwrap();

		let recovery_config = RecoveryConfig {
			delay_period: 0u32.into(),
			deposit: total_deposit,
			friends: bounded_friends.clone(),
			threshold: n as u16,
		};

		// Create the recovery config storage item
		<Recoverable<T>>::insert(&lost_account, recovery_config.clone());

		// Reserve deposit for recovery
		T::Currency::reserve(&caller, total_deposit).unwrap();

		// Create an active recovery status
		let recovery_status = ActiveRecovery {
			created: 0u32.into(),
			deposit: total_deposit,
			friends: bounded_friends.clone(),
		};

		// Create the active recovery storage item
		<ActiveRecoveries<T>>::insert(&lost_account, &caller, recovery_status);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), lost_account_lookup);
		assert_last_event::<T>(
			Event::AccountRecovered { lost_account, rescuer_account: caller }.into(),
		);
	}

	#[benchmark]
	fn close_recovery(n: Linear<1, { T::MaxFriends::get() }>) {
		let caller: T::AccountId = whitelisted_caller();
		let rescuer_account: T::AccountId = account("rescuer_account", 0, SEED);
		let rescuer_account_lookup = T::Lookup::unlookup(rescuer_account.clone());

		T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value());
		T::Currency::make_free_balance_be(&rescuer_account, BalanceOf::<T>::max_value());

		// Create friends
		let friends = generate_friends::<T>(n);
		let bounded_friends: FriendsOf<T> = friends.try_into().unwrap();

		// Get deposit for recovery
		let total_deposit = get_total_deposit::<T>(&bounded_friends).unwrap();

		let recovery_config = RecoveryConfig {
			delay_period: DEFAULT_DELAY.into(),
			deposit: total_deposit,
			friends: bounded_friends.clone(),
			threshold: n as u16,
		};

		// Create the recovery config storage item
		<Recoverable<T>>::insert(&caller, recovery_config.clone());

		// Reserve deposit for recovery
		T::Currency::reserve(&caller, total_deposit).unwrap();

		// Create an active recovery status
		let recovery_status = ActiveRecovery {
			created: DEFAULT_DELAY.into(),
			deposit: total_deposit,
			friends: bounded_friends.clone(),
		};

		// Create the active recovery storage item
		<ActiveRecoveries<T>>::insert(&caller, &rescuer_account, recovery_status);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), rescuer_account_lookup);
		assert_last_event::<T>(
			Event::RecoveryClosed { lost_account: caller, rescuer_account }.into(),
		);
	}

	#[benchmark]
	fn remove_recovery(n: Linear<1, { T::MaxFriends::get() }>) {
		let caller: T::AccountId = whitelisted_caller();

		T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value());

		// Create friends
		let friends = generate_friends::<T>(n);
		let bounded_friends: FriendsOf<T> = friends.try_into().unwrap();

		// Get deposit for recovery
		let total_deposit = get_total_deposit::<T>(&bounded_friends).unwrap();

		let recovery_config = RecoveryConfig {
			delay_period: DEFAULT_DELAY.into(),
			deposit: total_deposit,
			friends: bounded_friends.clone(),
			threshold: n as u16,
		};

		// Create the recovery config storage item
		<Recoverable<T>>::insert(&caller, recovery_config);

		// Reserve deposit for recovery
		T::Currency::reserve(&caller, total_deposit).unwrap();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()));
		assert_last_event::<T>(Event::RecoveryRemoved { lost_account: caller }.into());
	}

	#[benchmark]
	fn cancel_recovered() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let account: T::AccountId = account("account", 0, SEED);
		let account_lookup = T::Lookup::unlookup(account.clone());

		frame_system::Pallet::<T>::inc_providers(&caller);

		frame_system::Pallet::<T>::inc_consumers(&caller)?;

		Proxy::<T>::insert(&caller, &account);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), account_lookup);

		Ok(())
	}

	#[benchmark]
	fn poke_deposit(n: Linear<1, { T::MaxFriends::get() }>) -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let lost_account: T::AccountId = account("lost_account", 0, SEED);

		// Fund caller account
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value());

		// 1. Setup recovery config for caller
		insert_recovery_config_with_max_friends::<T>(&caller);

		// 2. Setup active recovery for lost account
		setup_active_recovery_with_max_friends::<T>(&caller, &lost_account);

		// 3. Get initial deposits
		let initial_config = <Recoverable<T>>::get(&caller).unwrap();
		let initial_config_deposit = initial_config.deposit;
		let initial_recovery_deposit = T::RecoveryDeposit::get();
		assert_eq!(
			T::Currency::reserved_balance(&caller),
			initial_config_deposit.saturating_add(initial_recovery_deposit)
		);

		// 4. Artificially increase deposits
		let increased_config_deposit = initial_config_deposit.saturating_add(2u32.into());
		let increased_recovery_deposit = initial_recovery_deposit.saturating_add(2u32.into());

		<Recoverable<T>>::try_mutate(&caller, |maybe_config| -> Result<(), BenchmarkError> {
			let config = maybe_config.as_mut().unwrap();
			T::Currency::reserve(
				&caller,
				increased_config_deposit.saturating_sub(initial_config_deposit),
			)?;
			config.deposit = increased_config_deposit;
			Ok(())
		})
		.map_err(|_| BenchmarkError::Stop("Failed to mutate storage"))?;

		<ActiveRecoveries<T>>::try_mutate(
			&lost_account,
			&caller,
			|maybe_recovery| -> Result<(), BenchmarkError> {
				let recovery = maybe_recovery.as_mut().unwrap();
				T::Currency::reserve(
					&caller,
					increased_recovery_deposit.saturating_sub(initial_recovery_deposit),
				)?;
				recovery.deposit = increased_recovery_deposit;
				Ok(())
			},
		)
		.map_err(|_| BenchmarkError::Stop("Failed to mutate storage"))?;

		// 5. Verify increased deposits
		assert_eq!(
			T::Currency::reserved_balance(&caller),
			increased_config_deposit.saturating_add(increased_recovery_deposit)
		);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), Some(T::Lookup::unlookup(lost_account.clone())));

		// 6. Assert final state
		assert_eq!(
			T::Currency::reserved_balance(&caller),
			initial_config_deposit.saturating_add(initial_recovery_deposit)
		);

		// 7. Check events were emitted
		assert_has_event::<T>(
			Event::DepositPoked {
				who: caller.clone(),
				kind: DepositKind::RecoveryConfig,
				old_deposit: increased_config_deposit,
				new_deposit: initial_config_deposit,
			}
			.into(),
		);
		assert_has_event::<T>(
			Event::DepositPoked {
				who: caller,
				kind: DepositKind::ActiveRecoveryFor(lost_account),
				old_deposit: increased_recovery_deposit,
				new_deposit: initial_recovery_deposit,
			}
			.into(),
		);

		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
