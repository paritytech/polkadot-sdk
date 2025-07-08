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

//! Testing utilities for pallet-staking-async internal tests.

use super::*;
use frame_benchmarking::account;
use frame_support::traits::fungible::Mutate;
use frame_system::RawOrigin;

const SEED: u32 = 0;
const STAKING_ID: frame_support::traits::LockIdentifier = *b"staking ";

/// Create a funded user.
pub fn create_funded_user<T: Config>(
	string: &'static str,
	n: u32,
	balance_factor: u32,
) -> T::AccountId {
	let user = account(string, n, SEED);
	let balance = asset::existential_deposit::<T>() * balance_factor.into();
	let _ = T::Currency::set_balance(&user, balance);
	user
}

/// Create a stash and controller pair.
pub fn create_stash_controller<T: Config>(
	n: u32,
	balance_factor: u32,
	destination: RewardDestination<T::AccountId>,
) -> Result<(T::AccountId, T::AccountId), &'static str> {
	let staker = create_funded_user::<T>("stash", n, balance_factor);
	let amount =
		asset::existential_deposit::<T>().max(1u64.into()) * (balance_factor / 10).max(1).into();
	Pallet::<T>::bond(RawOrigin::Signed(staker.clone()).into(), amount, destination)?;
	Ok((staker.clone(), staker))
}

/// Create a unique stash and controller pair.
pub fn create_unique_stash_controller<T: Config>(
	n: u32,
	balance_factor: u32,
	destination: RewardDestination<T::AccountId>,
	dead_controller: bool,
) -> Result<(T::AccountId, T::AccountId), &'static str> {
	let stash = create_funded_user::<T>("stash", n, balance_factor);
	let controller = if dead_controller {
		create_funded_user::<T>("controller", n, 0)
	} else {
		create_funded_user::<T>("controller", n, balance_factor)
	};
	let amount = asset::existential_deposit::<T>() * (balance_factor / 10).max(1).into();
	Pallet::<T>::bond(RawOrigin::Signed(stash.clone()).into(), amount, destination)?;

	// update ledger to be a *different* controller to stash
	if let Some(l) = Ledger::<T>::take(&stash) {
		<Ledger<T>>::insert(&controller, l);
	}
	// update bonded account to be unique controller
	<Bonded<T>>::insert(&stash, &controller);

	Ok((stash, controller))
}

pub fn migrate_to_old_currency<T: Config>(who: T::AccountId) {
	use frame_support::traits::LockableCurrency;
	let staked = asset::staked::<T>(&who);

	// apply locks (this also adds a consumer).
	T::OldCurrency::set_lock(
		STAKING_ID,
		&who,
		staked,
		frame_support::traits::WithdrawReasons::all(),
	);
	// remove holds.
	let _ = asset::kill_stake::<T>(&who);

	// replicate old behaviour of explicit increment of consumer.
	let _ = frame_system::Pallet::<T>::inc_consumers(&who);
}
