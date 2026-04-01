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

//! DAP pallet migrations.

use super::*;
use frame_support::traits::UncheckedOnRuntimeUpgrade;

/// V1 to V2 migration: initializes `LastIssuanceTimestamp` and seeds `BudgetAllocation`.
///
/// - `T`: DAP pallet config
/// - `P`: `Get<u64>` providing the initial timestamp (e.g. active era start from staking)
/// - `B`: `Get<BudgetAllocationMap>` providing the initial budget allocation
///
/// NOTE: This migration should be applied when staking changes are integrated to support
/// budget drip. The storage version bump (V1 → V2) happens at that point.
pub type MigrateV1ToV2<T, P, B> = frame_support::migrations::VersionedMigration<
	1,
	2,
	InnerMigrateV1ToV2<T, P, B>,
	pallet::Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

/// Inner (unversioned) migration logic. Use [`MigrateV1ToV2`] instead.
pub struct InnerMigrateV1ToV2<T, P, B>(core::marker::PhantomData<(T, P, B)>);

impl<T: Config, P: Get<u64>, B: Get<BudgetAllocationMap>> UncheckedOnRuntimeUpgrade
	for InnerMigrateV1ToV2<T, P, B>
{
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let mut weight = T::DbWeight::get().reads(2);

		// Seed LastIssuanceTimestamp (idempotent).
		let current_ts = LastIssuanceTimestamp::<T>::get();
		if current_ts == 0 {
			let ts = P::get();
			LastIssuanceTimestamp::<T>::put(ts);
			weight = weight.saturating_add(T::DbWeight::get().writes(1));
			log::info!(target: LOG_TARGET, "Initialized LastIssuanceTimestamp to {ts}");
		}

		// Seed BudgetAllocation (idempotent).
		let current_budget = BudgetAllocation::<T>::get();
		if current_budget.is_empty() {
			BudgetAllocation::<T>::put(B::get());
			weight = weight.saturating_add(T::DbWeight::get().writes(1));
			log::info!(target: LOG_TARGET, "Initialized BudgetAllocation with default budget");
		}

		weight
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, sp_runtime::TryRuntimeError> {
		frame_support::ensure!(
			LastIssuanceTimestamp::<T>::get() == 0 || BudgetAllocation::<T>::get().is_empty(),
			"Migration not needed: LastIssuanceTimestamp and BudgetAllocation already set"
		);
		Ok(alloc::vec::Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		frame_support::ensure!(
			LastIssuanceTimestamp::<T>::get() != 0,
			"LastIssuanceTimestamp should be non-zero after migration"
		);

		let budget = BudgetAllocation::<T>::get();
		frame_support::ensure!(!budget.is_empty(), "BudgetAllocation should be non-empty");

		let total: u64 = budget.values().map(|p| p.deconstruct() as u64).sum();
		frame_support::ensure!(
			total == Perbill::one().deconstruct() as u64,
			"BudgetAllocation must sum to 100%"
		);

		Ok(())
	}
}
