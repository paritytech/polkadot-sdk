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

//! Storage migrations for pallet-conviction-voting.

use super::*;
use frame_support::{
	pallet_prelude::*,
	traits::{InspectLockableCurrency, LockIdentifier, LockableCurrency},
};

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

/// The lock identifier this pallet used before v1. Only migration vocabulary now: v1 replaced
/// the `LockableCurrency` lock with a `fungible` freeze under [`FreezeReason::Vote`].
const CONVICTION_VOTING_ID: LockIdentifier = *b"pyconvot";

pub mod v1 {
	use super::*;

	/// Move every voter's `*b"pyconvot"` `LockableCurrency` lock over to a
	/// `fungible::MutateFreeze` freeze under [`FreezeReason::Vote`].
	///
	/// `OldCurrency` is the `LockableCurrency` implementation the pallet was previously wired
	/// to (in practice: the same `pallet-balances` instance as `T::Currency`); the pallet
	/// itself no longer knows the legacy trait exists, so the migration takes it explicitly.
	///
	/// The set of accounts to migrate is exactly the key set of [`ClassLocksFor`]: the pallet
	/// has always kept the on-chain lock equal to the max over that list (and removed the lock
	/// when the list emptied), so no other account can hold a `"pyconvot"` lock.
	///
	/// NOTE: iterates all voters in one block. Fine for small/test networks; chains with large
	/// voter counts must lift the per-account body into a stepped (multi-block) migration
	/// before adopting this.
	pub struct VoteLockToFreeze<T, OldCurrency, I = ()>(PhantomData<(T, OldCurrency, I)>);

	impl<T, OldCurrency, I> frame_support::traits::UncheckedOnRuntimeUpgrade
		for VoteLockToFreeze<T, OldCurrency, I>
	where
		T: Config<I>,
		I: 'static,
		OldCurrency: LockableCurrency<T::AccountId>
			+ InspectLockableCurrency<T::AccountId, Balance = BalanceOf<T, I>>,
	{
		fn on_runtime_upgrade() -> Weight {
			let mut migrated = 0u64;
			let mut iterated = 0u64;
			for (who, class_locks) in ClassLocksFor::<T, I>::iter() {
				iterated.saturating_inc();
				// The invariant the pallet has always enforced: on-chain encumbrance = max
				// over the per-class requirements.
				let amount =
					class_locks.iter().map(|x| x.1).max().unwrap_or_else(Zero::zero);
				if amount.is_zero() {
					// Empty entry: `update_lock` already removed the legacy lock for it.
					continue;
				}
				// Freeze first, remove the lock only on success: the account must never pass
				// through a state where the balance backing a live vote is transferable.
				match T::Currency::set_freeze(&FreezeReason::Vote.into(), &who, amount) {
					Ok(()) => {
						OldCurrency::remove_lock(CONVICTION_VOTING_ID, &who);
						migrated.saturating_inc();
					},
					Err(e) => {
						// Keeping the old lock keeps the funds encumbered; nothing is lost,
						// but this needs investigating (`MaxFreezes` too small?).
						log::error!(
							target: "runtime::conviction-voting",
							"failed to freeze for {who:?}: {e:?}; legacy lock kept in place",
						);
					},
				}
			}
			log::info!(
				target: "runtime::conviction-voting",
				"migrated {migrated} of {iterated} conviction voting locks to freezes",
			);
			// Per account: 1 read (iteration) + freeze (account + freezes r/w) + lock removal
			// (account + locks r/w).
			T::DbWeight::get().reads_writes(
				iterated.saturating_mul(3),
				migrated.saturating_mul(4),
			)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			let expected: Vec<(T::AccountId, BalanceOf<T, I>)> = ClassLocksFor::<T, I>::iter()
				.map(|(who, class_locks)| {
					let amount =
						class_locks.iter().map(|x| x.1).max().unwrap_or_else(Zero::zero);
					(who, amount)
				})
				.collect();
			Ok(expected.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
			use frame_support::traits::fungible::InspectFreeze;
			let expected: Vec<(T::AccountId, BalanceOf<T, I>)> =
				Decode::decode(&mut state.as_slice())
					.map_err(|_| TryRuntimeError::Other("pre_upgrade state decodes"))?;
			for (who, amount) in expected {
				ensure!(
					T::Currency::balance_frozen(&FreezeReason::Vote.into(), &who) == amount,
					TryRuntimeError::Other("freeze amount != pre-upgrade lock requirement"),
				);
				ensure!(
					OldCurrency::balance_locked(CONVICTION_VOTING_ID, &who).is_zero(),
					TryRuntimeError::Other("legacy pyconvot lock still in place"),
				);
			}
			Ok(())
		}
	}
}

/// v0 → v1: [`v1::VoteLockToFreeze`], gated on the pallet's storage version so it runs exactly
/// once. Wire as `MigrateV0ToV1<Runtime, Balances>` (or the instanced equivalent).
pub type MigrateV0ToV1<T, OldCurrency, I = ()> = frame_support::migrations::VersionedMigration<
	0,
	1,
	v1::VoteLockToFreeze<T, OldCurrency, I>,
	Pallet<T, I>,
	<T as frame_system::Config>::DbWeight,
>;

#[cfg(test)]
mod tests {
	use super::*;
	use crate::tests::{new_test_ext, Balances, Test};
	use frame_support::traits::{fungible::InspectFreeze, OnRuntimeUpgrade, WithdrawReasons};

	#[test]
	fn vote_lock_to_freeze_migration_works() {
		new_test_ext().execute_with(|| {
			// Set up two voters exactly as the v0 pallet would have left them: a legacy
			// `pyconvot` lock equal to the max of their `ClassLocksFor` entries.
			ClassLocksFor::<Test>::insert(
				1,
				BoundedVec::truncate_from(vec![(0u8, 5u64), (1u8, 8u64)]),
			);
			Balances::set_lock(CONVICTION_VOTING_ID, &1, 8, WithdrawReasons::all());
			ClassLocksFor::<Test>::insert(2, BoundedVec::truncate_from(vec![(0u8, 20u64)]));
			Balances::set_lock(CONVICTION_VOTING_ID, &2, 20, WithdrawReasons::all());
			// On-chain storage version 0, as on any live chain today.
			StorageVersion::new(0).put::<Pallet<Test>>();

			assert_eq!(Balances::usable_balance(1), 2);
			assert_eq!(Balances::usable_balance(2), 0);

			MigrateV0ToV1::<Test, Balances>::on_runtime_upgrade();

			// The encumbrance moved from the Locks storage to the Freezes storage...
			assert_eq!(Balances::balance_locked(CONVICTION_VOTING_ID, &1), 0);
			assert_eq!(Balances::balance_locked(CONVICTION_VOTING_ID, &2), 0);
			assert_eq!(Balances::balance_frozen(&FreezeReason::Vote.into(), &1), 8);
			assert_eq!(Balances::balance_frozen(&FreezeReason::Vote.into(), &2), 20);
			// ...with no observable balance change for the voters...
			assert_eq!(Balances::usable_balance(1), 2);
			assert_eq!(Balances::usable_balance(2), 0);
			// ...and the storage version is bumped so the migration is one-shot.
			assert_eq!(StorageVersion::get::<Pallet<Test>>(), 1);
		});
	}

	#[test]
	fn migration_is_noop_when_already_v1() {
		new_test_ext().execute_with(|| {
			// An already-migrated chain (on-chain version 1) must not be touched again, even
			// if a stray legacy lock exists.
			StorageVersion::new(1).put::<Pallet<Test>>();
			Balances::set_lock(CONVICTION_VOTING_ID, &1, 8, WithdrawReasons::all());
			ClassLocksFor::<Test>::insert(1, BoundedVec::truncate_from(vec![(0u8, 8u64)]));

			MigrateV0ToV1::<Test, Balances>::on_runtime_upgrade();

			assert_eq!(Balances::balance_locked(CONVICTION_VOTING_ID, &1), 8);
			assert_eq!(Balances::balance_frozen(&FreezeReason::Vote.into(), &1), 0);
		});
	}
}
