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

use super::super::*;
use alloc::{boxed::Box, vec::Vec};

/// Migrate the locks and vote stake on accounts (as specified with param `to_migrate`) that have
/// more than their free balance locked.
///
/// This migration addresses a bug were a voter could lock up to their reserved balance + free
/// balance. Since locks are only designed to operate on free balance, this put those affected in a
/// situation where they could increase their free balance but still not be able to use their funds
/// because they were less than the lock.
pub fn migrate<T: Config>(to_migrate: Vec<T::AccountId>) -> Weight
where
	<<T as pallet::Config>::Currency as frame_support::traits::fungible::InspectFreeze<
		<T as frame_system::Config>::AccountId,
	>>::Id: From<pallet::FreezeReason>,
{
	let mut weight = Weight::zero();

	for who in to_migrate.iter() {
		if let Ok(mut voter) = Voting::<T>::try_get(who) {
			let balance = T::Currency::balance(who);

			weight = weight.saturating_add(T::DbWeight::get().reads(2));

			if voter.stake > balance {
				voter.stake = balance;
				Voting::<T>::insert(&who, voter);

				let _ = T::Currency::set_freeze(
					&FreezeReason::PhragmenLockedBond.into(),
					who,
					balance,
				);

				weight = weight.saturating_add(T::DbWeight::get().writes(2));
			}
		}
	}

	weight
}

/// Given the list of voters to migrate return a function that does some checks and information
/// prior to migration. This can be linked to [`frame_support::traits::OnRuntimeUpgrade::
/// pre_upgrade`] for further testing.
pub fn pre_migrate_fn<T: Config>(to_migrate: Vec<T::AccountId>) -> Box<dyn Fn() -> ()> {
	Box::new(move || {
		for who in to_migrate.iter() {
			if let Ok(voter) = Voting::<T>::try_get(who) {
				let free_balance = T::Currency::balance(who);

				if voter.stake > free_balance {
					// all good
				} else {
					log::warn!("pre-migrate elections-phragmen: voter={:?} has less stake then free balance", who);
				}
			} else {
				log::warn!("pre-migrate elections-phragmen: cannot find voter={:?}", who);
			}
		}
		log::info!("pre-migrate elections-phragmen complete");
	})
}

/// Some checks for after migration. This can be linked to
/// `frame_support::traits::OnRuntimeUpgrade::post_upgrade` for further testing.
///
/// Panics if anything goes wrong.
pub fn post_migrate<T: crate::Config>() {
	for (who, voter) in Voting::<T>::iter() {
		let balance = T::Currency::balance(&who);

		assert!(voter.stake <= balance, "migration should have made locked <= balance");
		// Ideally we would also check that the locks and AccountData.misc_frozen where correctly
		// updated, but since both of those are generic we can't do that without further bounding T.
	}

	log::info!("post-migrate elections-phragmen complete");
}
