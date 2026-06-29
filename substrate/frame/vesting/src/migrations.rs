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

//! Storage migrations for the vesting pallet.

use super::*;
use alloc::vec;

// Legacy V0→V1 migration, retained for downstream API stability. The body now writes V2
// storage shape (forced by the new storage type) but `post_migrate` still asserts V1 and
// the version is not bumped — effectively a no-op. No in-tree runtime references it.
// TODO: remove in a follow-up PR.
pub mod v1 {
	use super::*;

	#[cfg(feature = "try-runtime")]
	pub fn pre_migrate<T: Config>() -> Result<(), &'static str> {
		assert!(StorageVersion::<T>::get() == Releases::V0, "Storage version too high.");

		log::debug!(
			target: "runtime::vesting",
			"migration: Vesting storage version v1 PRE migration checks successful!"
		);

		Ok(())
	}

	/// Migrate from single schedule (V0) to multi-schedule + kind-tagged storage (V2).
	/// WARNING: This migration will delete schedules if `MaxVestingSchedules < 1`.
	pub fn migrate<T: Config>() -> Weight {
		let mut reads_writes = 0;

		Vesting::<T>::translate::<VestingInfo<BalanceOf<T>, BlockNumberFor<T>>, _>(
			|_key, vesting_info| {
				reads_writes += 1;
				let v: Option<
					BoundedVec<
						(VestingInfo<BalanceOf<T>, BlockNumberFor<T>>, VestingKind),
						MaxVestingSchedulesGet<T>,
					>,
				> = vec![(vesting_info, VestingKind::Public)].try_into().ok();

				if v.is_none() {
					log::warn!(
						target: "runtime::vesting",
						"migration: Failed to move a vesting schedule into a BoundedVec"
					);
				}

				v
			},
		);

		T::DbWeight::get().reads_writes(reads_writes, reads_writes)
	}

	#[cfg(feature = "try-runtime")]
	pub fn post_migrate<T: Config>() -> Result<(), &'static str> {
		assert_eq!(StorageVersion::<T>::get(), Releases::V1);

		for (_key, schedules) in Vesting::<T>::iter() {
			assert!(
				schedules.len() >= 1,
				"A bounded vec with incorrect count of items was created."
			);

			for (s, _kind) in schedules {
				// It is ok if this does not pass, but ideally pre-existing schedules would pass
				// this validation logic so we can be more confident about edge cases.
				if !s.is_valid() {
					log::warn!(
						target: "runtime::vesting",
						"migration: A schedule does not pass new validation logic.",
					)
				}
			}
		}

		log::debug!(
			target: "runtime::vesting",
			"migration: Vesting storage version v1 POST migration checks successful!"
		);
		Ok(())
	}
}

/// Migration from multi-schedule (`BoundedVec<VestingInfo>`, V1) to kind-tagged schedules
/// (`BoundedVec<(VestingInfo, VestingKind)>`, V2).
///
/// All pre-existing V1 schedules are tagged `VestingKind::Public`. Historical
/// `force_vested_transfer` schedules are indistinguishable from public ones in V1 storage, so
/// they also receive the `Public` tag. Future root-issued schedules will be stored with `System`.
pub mod v2 {
	use super::*;
	use frame_support::traits::OnRuntimeUpgrade;

	pub struct Migration<T>(PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for Migration<T> {
		fn on_runtime_upgrade() -> Weight {
			if StorageVersion::<T>::get() != Releases::V1 {
				return T::DbWeight::get().reads(1);
			}
			let mut reads = 1u64;
			let mut writes = 0u64;

			Vesting::<T>::translate::<
				BoundedVec<VestingInfo<BalanceOf<T>, BlockNumberFor<T>>, MaxVestingSchedulesGet<T>>,
				_,
			>(|who, old| {
				reads += 1;
				writes += 1;
				let tagged: alloc::vec::Vec<_> =
					old.into_iter().map(|vi| (vi, VestingKind::Public)).collect();

				// Warn if this account exceeds the public vesting schedule capacity, as this will
				// cause new schedules to be rejected until some existing ones vest out.
				let count = tagged.len() as u32;
				if count >= T::MAX_PUBLIC_VESTING_SCHEDULES {
					log::warn!(
						target: "runtime::vesting",
						"Migration: account {:?} has {} public schedules which exceed the limit of {}; \
						 new public schedules will be rejected until some existing schedules vest out.",
						who,
						count,
						T::MAX_PUBLIC_VESTING_SCHEDULES,
					);
				}

				let new: BoundedVec<_, MaxVestingSchedulesGet<T>> =
					tagged.try_into().expect("same capacity; qed");
				Some(new)
			});

			StorageVersion::<T>::put(Releases::V2);
			T::DbWeight::get().reads_writes(reads, writes)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, sp_runtime::TryRuntimeError> {
			// Mirror the `on_runtime_upgrade` guard: when not at V1 the migration is a no-op.
			if StorageVersion::<T>::get() != Releases::V1 {
				return Ok(alloc::vec::Vec::new());
			}
			let count = Vesting::<T>::iter().count() as u64;
			Ok(count.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			// Empty state occurs if the chain was not at V1 before the upgrade.
			if state.is_empty() {
				return Ok(());
			}
			assert_eq!(StorageVersion::<T>::get(), Releases::V2, "Storage version must be V2");
			let pre_count = u64::decode(&mut &state[..]).expect("pre_upgrade encoded a u64; qed");
			let post_count = Vesting::<T>::iter().count() as u64;
			assert_eq!(pre_count, post_count, "Account count must be preserved");
			for (_who, schedules) in Vesting::<T>::iter() {
				for (_vi, kind) in &schedules {
					assert_eq!(
						*kind,
						VestingKind::Public,
						"All migrated schedules must be tagged Public"
					);
				}
			}
			Ok(())
		}
	}
}
