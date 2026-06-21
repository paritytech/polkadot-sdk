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

use crate::{Config, Pallet, Weight, LOG_TARGET};
use frame_support::{pallet_prelude::*, storage::migration, traits::OnRuntimeUpgrade};
use log;

/// The in-code storage version.
pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(3);

/// Wrapper for all migrations of this pallet.
pub fn migrate<T: Config<I>, I: 'static>() -> Weight {
	let on_chain_version = Pallet::<T, I>::on_chain_storage_version();
	let mut weight: Weight = Weight::zero();

	if on_chain_version < 1 {
		weight = weight.saturating_add(v0_to_v1::migrate::<T, I>());
	}

	if on_chain_version < 2 {
		weight = weight.saturating_add(v1_to_v2::migrate::<T, I>());
	}

	if on_chain_version < 3 {
		weight = weight.saturating_add(v2_to_v3::migrate::<T, I>());
	}

	STORAGE_VERSION.put::<Pallet<T, I>>();
	weight = weight.saturating_add(T::DbWeight::get().writes(1));

	weight
}

/// Implements `OnRuntimeUpgrade` trait.
pub struct Migration<T, I = ()>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		migrate::<T, I>()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, sp_runtime::TryRuntimeError> {
		v2_to_v3::pre_upgrade::<T, I>()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		v2_to_v3::post_upgrade::<T, I>(state)
	}
}

/// v0_to_v1: `UpForKicking` is replaced by a retirement period.
mod v0_to_v1 {
	use super::*;

	pub fn migrate<T: Config<I>, I: 'static>() -> Weight {
		log::info!(target: LOG_TARGET, "Running migration v0_to_v1.");

		let res = migration::clear_storage_prefix(
			<Pallet<T, I>>::name().as_bytes(),
			b"UpForKicking",
			b"",
			None,
			None,
		);

		log::info!(
			target: LOG_TARGET,
			"Cleared '{}' entries from 'UpForKicking' storage prefix",
			res.unique
		);

		if res.maybe_cursor.is_some() {
			log::error!(
				target: LOG_TARGET,
				"Storage prefix 'UpForKicking' is not completely cleared."
			);
		}

		T::DbWeight::get().writes(res.unique.into())
	}
}

/// v1_to_v2: `Members` storage map collapses `Founder` and `Fellow` keys into one `Fellow`.
/// Total number of `Founder`s and `Fellow`s must not be higher than `T::MaxMembersCount`.
pub(crate) mod v1_to_v2 {
	use super::*;
	use crate::{MemberRole, Members};

	/// V1 Role set.
	#[derive(Copy, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
	pub enum MemberRoleV1 {
		Founder,
		Fellow,
		Ally,
		Retiring,
	}

	pub fn migrate<T: Config<I>, I: 'static>() -> Weight {
		log::info!(target: LOG_TARGET, "Running migration v1_to_v2: `Members` storage map collapses `Founder` and `Fellow` keys into one `Fellow`.");
		// fetch into the scope all members.
		let founders_vec = take_members::<T, I>(MemberRoleV1::Founder).into_inner();
		let mut fellows_vec = take_members::<T, I>(MemberRoleV1::Fellow).into_inner();
		let allies = take_members::<T, I>(MemberRoleV1::Ally);
		let retiring = take_members::<T, I>(MemberRoleV1::Retiring);
		if founders_vec
			.len()
			.saturating_add(fellows_vec.len())
			.saturating_add(allies.len())
			.saturating_add(retiring.len()) ==
			0
		{
			return T::DbWeight::get().reads(4);
		}
		log::info!(
			target: LOG_TARGET,
			"Members storage v1 contains, '{}' founders, '{}' fellows, '{}' allies, '{}' retiring members.",
			founders_vec.len(),
			fellows_vec.len(),
			allies.len(),
			retiring.len(),
		);
		// merge founders with fellows and sort.
		fellows_vec.extend(founders_vec);
		fellows_vec.sort();
		if fellows_vec.len() as u32 > T::MaxMembersCount::get() {
			log::error!(
				target: LOG_TARGET,
				"Merged list of founders and fellows do not fit into `T::MaxMembersCount` bound. Truncating the merged set into max members count."
			);
			fellows_vec.truncate(T::MaxMembersCount::get() as usize);
		}
		let fellows: BoundedVec<T::AccountId, T::MaxMembersCount> =
			fellows_vec.try_into().unwrap_or_default();
		// insert members with new storage map key.
		Members::<T, I>::insert(&MemberRole::Fellow, fellows.clone());
		Members::<T, I>::insert(&MemberRole::Ally, allies.clone());
		Members::<T, I>::insert(&MemberRole::Retiring, retiring.clone());
		log::info!(
			target: LOG_TARGET,
			"Members storage updated with, '{}' fellows, '{}' allies, '{}' retiring members.",
			fellows.len(),
			allies.len(),
			retiring.len(),
		);
		T::DbWeight::get().reads_writes(4, 4)
	}

	fn take_members<T: Config<I>, I: 'static>(
		role: MemberRoleV1,
	) -> BoundedVec<T::AccountId, T::MaxMembersCount> {
		migration::take_storage_item::<
			MemberRoleV1,
			BoundedVec<T::AccountId, T::MaxMembersCount>,
			Twox64Concat,
		>(<Pallet<T, I>>::name().as_bytes(), b"Members", role)
		.unwrap_or_default()
	}
}

/// v2_to_v3: convert legacy reserved candidacy deposits into `Consideration`-based holds, turning
/// each `DepositOf` balance into a ticket. Bounded by `MaxMembersCount`, so safe in one block.
pub mod v2_to_v3 {
	use super::*;
	use crate::{BalanceOf, DepositOf};
	use alloc::vec::Vec;
	use frame_support::traits::{Consideration, ReservableCurrency};
	use sp_runtime::traits::Zero;

	/// Read the legacy `(account, reserved balance)` pairs from the pre-migration `DepositOf` map.
	pub(crate) fn legacy_deposits<T: Config<I>, I: 'static>() -> Vec<(T::AccountId, BalanceOf<T, I>)>
	{
		migration::storage_key_iter::<T::AccountId, BalanceOf<T, I>, Blake2_128Concat>(
			<Pallet<T, I>>::name().as_bytes(),
			b"DepositOf",
		)
		.collect()
	}

	pub fn migrate<T: Config<I>, I: 'static>() -> Weight {
		let footprint = Pallet::<T, I>::deposit_footprint();
		let old_deposits = legacy_deposits::<T, I>();

		log::info!(
			target: LOG_TARGET,
			"Running migration v2_to_v3: converting {} reserved deposit(s) to holds.",
			old_deposits.len(),
		);

		let mut weight = T::DbWeight::get().reads(old_deposits.len() as u64);

		for (who, amount) in old_deposits {
			// Release the legacy reserve.
			let remaining = T::OldCurrency::unreserve(&who, amount);
			if !remaining.is_zero() {
				log::error!(
					target: LOG_TARGET,
					"Could not fully unreserve a legacy alliance deposit; some balance remained reserved.",
				);
			}

			// Place the equivalent hold via the configured `Consideration`.
			match T::Consideration::new(&who, footprint) {
				Ok(ticket) => DepositOf::<T, I>::insert(&who, ticket),
				Err(e) => {
					// Account can no longer back the deposit; drop the entry so state stays
					// consistent (the unreserved funds just remain free).
					log::error!(
						target: LOG_TARGET,
						"Failed to hold a migrated alliance deposit: {:?}. Removing the entry.",
						e,
					);
					DepositOf::<T, I>::remove(&who);
				},
			}

			weight = weight.saturating_add(T::DbWeight::get().reads_writes(2, 2));
		}

		weight
	}

	#[cfg(feature = "try-runtime")]
	pub fn pre_upgrade<T: Config<I>, I: 'static>() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		use codec::Encode;

		// Only snapshot if the migration will run.
		if Pallet::<T, I>::on_chain_storage_version() >= 3 {
			return Ok(Vec::new());
		}

		// Record accounts with a deposit; all must still have one (as a hold) afterwards.
		let accounts: Vec<T::AccountId> =
			legacy_deposits::<T, I>().into_iter().map(|(who, _)| who).collect();
		Ok(accounts.encode())
	}

	#[cfg(feature = "try-runtime")]
	pub fn post_upgrade<T: Config<I>, I: 'static>(
		state: Vec<u8>,
	) -> Result<(), sp_runtime::TryRuntimeError> {
		use codec::Decode;

		if state.is_empty() {
			return Ok(());
		}

		let accounts = Vec::<T::AccountId>::decode(&mut &state[..])
			.map_err(|_| "alliance::v2_to_v3: failed to decode pre-upgrade state")?;

		ensure!(
			Pallet::<T, I>::on_chain_storage_version() >= 3,
			"alliance::v2_to_v3: storage version was not bumped"
		);
		for who in accounts {
			ensure!(
				DepositOf::<T, I>::contains_key(&who),
				"alliance::v2_to_v3: a member's deposit was lost during migration"
			);
		}
		Ok(())
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::{mock::*, MemberRole, Members};

	#[test]
	fn migration_v1_to_v2_works() {
		new_test_ext().execute_with(|| {
			assert_ok!(Alliance::join_alliance(RuntimeOrigin::signed(4)));
			assert_eq!(Members::<Test>::get(MemberRole::Ally), vec![4]);
			assert_eq!(Members::<Test>::get(MemberRole::Fellow), vec![1, 2, 3]);
			v1_to_v2::migrate::<Test, ()>();
			assert_eq!(Members::<Test>::get(MemberRole::Fellow), vec![1, 2, 3, 4]);
			assert_eq!(Members::<Test>::get(MemberRole::Ally), vec![]);
			assert_eq!(Members::<Test>::get(MemberRole::Retiring), vec![]);
		});
	}

	#[test]
	fn migration_v2_to_v3_converts_reserve_to_hold() {
		use crate::{DepositOf, HoldReason};
		use frame_support::{
			migration::put_storage_value,
			traits::{fungible::InspectHold, Consideration, ReservableCurrency},
			Blake2_128Concat, StorageHasher,
		};

		new_test_ext().execute_with(|| {
			// Pretend the chain is still on storage version 2.
			StorageVersion::new(2).put::<Alliance>();

			let who = 4u64;
			let amount = AllyDeposit::get();
			let reason = RuntimeHoldReason::Alliance(HoldReason::AllianceDeposit);

			// Simulate a legacy ally: reserve the deposit and store it in the old `DepositOf`
			// format.
			assert_ok!(<Test as crate::Config>::OldCurrency::reserve(&who, amount));
			let key = Blake2_128Concat::hash(&who.encode());
			put_storage_value(Alliance::name().as_bytes(), b"DepositOf", &key, amount);

			// Before the migration the funds are reserved, not held.
			assert_eq!(Balances::balance_on_hold(&reason, &who), 0);

			// Run the full migration wrapper.
			Migration::<Test, ()>::on_runtime_upgrade();

			// The legacy reserve is now a hold of the same amount, tracked by a `Consideration`.
			assert_eq!(Balances::balance_on_hold(&reason, &who), amount);
			assert!(DepositOf::<Test>::contains_key(who));
			assert_eq!(Alliance::on_chain_storage_version(), 3);

			// Releasing the ticket frees exactly the migrated amount.
			let ticket = DepositOf::<Test>::take(who).unwrap();
			assert_ok!(ticket.drop(&who));
			assert_eq!(Balances::balance_on_hold(&reason, &who), 0);
		});
	}
}
