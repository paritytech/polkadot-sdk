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

//! Storage migrations for the staking-async pallet.

use crate::{log, reward::EraRewardManager, Config, DisableMintingGuard, RewardKind, RewardPot};
use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible::{Inspect, Mutate},
		tokens::Preservation,
		Get, OnRuntimeUpgrade,
	},
	PalletId,
};
use sp_runtime::{traits::AccountIdConversion, Saturating};
use sp_staking::EraIndex;

/// One-shot migration relocating already-funded era pots after the seed-derivation
/// change (#11930) so existing rewards stay claimable. For runtimes that activated
/// DAP before the slot-based rotation of era pot accounts landed.
///
/// Migrates a single [`RewardKind`] per instance — list it twice in `Migrations`
/// if both kinds need migrating.
///
/// Idempotent: skips eras whose old account has no balance.
///
/// Generic params:
/// - `T`: pallet config.
/// - `S`: same `Get<PalletId>` used by [`crate::Seed`] to derive pot accounts.
/// - `K`: which [`RewardKind`] to migrate.
pub struct MigrateEraPotsToPool<T, S, K>(core::marker::PhantomData<(T, S, K)>);

impl<T: Config, S: Get<PalletId>, K: Get<RewardKind>> MigrateEraPotsToPool<T, S, K> {
	/// Reproduces the historical seed derivation used before the slot-based
	/// rotation, needed to locate pre-migration balances.
	fn old_pot_account(era: EraIndex) -> T::AccountId {
		S::get().into_sub_account_truncating(RewardPot::Era(era, K::get()))
	}
}

impl<T: Config, S: Get<PalletId>, K: Get<RewardKind>> OnRuntimeUpgrade
	for MigrateEraPotsToPool<T, S, K>
{
	fn on_runtime_upgrade() -> Weight {
		let mut weight = T::DbWeight::get().reads(2);

		let Some(guard_era) = DisableMintingGuard::<T>::get() else {
			log!(info, "EraPotsToPool: guard unset, nothing to migrate");
			return weight;
		};

		let active_era_idx = crate::session_rotation::Rotator::<T>::active_era();
		debug_assert!(
			active_era_idx >= guard_era,
			"active_era should always be past DisableMintingGuard once set"
		);
		if active_era_idx <= guard_era {
			return weight;
		}

		// Anything older than `HistoryDepth` was already cleaned up via the
		// normal payout flow.
		let oldest = active_era_idx.saturating_sub(T::HistoryDepth::get()).max(guard_era);

		let kind = K::get();
		let mut migrated = 0u32;
		for era in oldest..active_era_idx {
			let old = Self::old_pot_account(era);
			weight.saturating_accrue(T::DbWeight::get().reads(1));
			if frame_system::Pallet::<T>::providers(&old) == 0 {
				continue;
			}

			// `create` is idempotent: increments the provider on the new
			// slot account only if not already provided.
			let new = EraRewardManager::<T>::create(era, kind);
			weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));

			let balance = T::Currency::balance(&old);
			weight.saturating_accrue(T::DbWeight::get().reads(1));
			if !balance.is_zero() {
				if let Err(e) = T::Currency::transfer(&old, &new, balance, Preservation::Expendable)
				{
					log!(
						error,
						"EraPotsToPool: era {} kind {:?}: transfer failed: {:?}",
						era,
						kind,
						e,
					);
					// Keep providers on the old account; balance is still there
					// and the account remains queryable for manual recovery.
					continue;
				}
				weight.saturating_accrue(T::DbWeight::get().reads_writes(2, 2));
			}

			// Try to release the old drained account so it can be reaped.
			let _ = frame_system::Pallet::<T>::dec_providers(&old);
			weight.saturating_accrue(T::DbWeight::get().writes(1));
			migrated.saturating_accrue(1);
		}

		log!(
			info,
			"EraPotsToPool: migrated {} eras of kind {:?} from guard {} to active {}",
			migrated,
			kind,
			guard_era,
			active_era_idx,
		);
		weight
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, sp_runtime::TryRuntimeError> {
		use crate::{BalanceOf, PotAccountProvider};
		use codec::Encode;
		use sp_runtime::traits::Zero;

		let kind = K::get();
		let mut total_old: BalanceOf<T> = Zero::zero();
		let mut total_new_pre: BalanceOf<T> = Zero::zero();
		for era in Self::migrated_eras() {
			let old = Self::old_pot_account(era);
			total_old.saturating_accrue(T::Currency::balance(&old));
			let new = T::RewardPots::pot_account(RewardPot::Era(era, kind));
			total_new_pre.saturating_accrue(T::Currency::balance(&new));
		}
		Ok((total_old, total_new_pre).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use crate::{BalanceOf, PotAccountProvider};
		use codec::Decode;
		use sp_runtime::traits::Zero;

		let (total_old, total_new_pre): (BalanceOf<T>, BalanceOf<T>) =
			Decode::decode(&mut &state[..]).map_err(|_| "decode pre_upgrade state")?;

		let kind = K::get();
		let mut remaining_old: BalanceOf<T> = Zero::zero();
		let mut total_new_post: BalanceOf<T> = Zero::zero();
		for era in Self::migrated_eras() {
			let old = Self::old_pot_account(era);
			remaining_old.saturating_accrue(T::Currency::balance(&old));
			let new = T::RewardPots::pot_account(RewardPot::Era(era, kind));
			total_new_post.saturating_accrue(T::Currency::balance(&new));
		}

		frame_support::ensure!(
			remaining_old.is_zero(),
			"old pot accounts still hold balance after migration"
		);
		// Funds must have landed in the new pots, accounting for whatever was
		// already there pre-migration (if anything).
		frame_support::ensure!(
			total_new_post.saturating_sub(total_new_pre) == total_old,
			"new pot balances did not increase by total_old after migration"
		);
		Ok(())
	}
}

#[cfg(feature = "try-runtime")]
impl<T: Config, S: Get<PalletId>, K: Get<RewardKind>> MigrateEraPotsToPool<T, S, K> {
	/// Returns the eras the migration touches. Only used for pre/post state checks.
	fn migrated_eras() -> core::ops::Range<EraIndex> {
		let active = crate::session_rotation::Rotator::<T>::active_era();
		match DisableMintingGuard::<T>::get() {
			Some(guard) if active > guard => {
				let oldest = active.saturating_sub(T::HistoryDepth::get()).max(guard);
				oldest..active
			},
			_ => 0..0,
		}
	}
}
