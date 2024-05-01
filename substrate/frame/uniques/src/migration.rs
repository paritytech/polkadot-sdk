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

//! Various pieces of common functionality.
use super::*;
use core::marker::PhantomData;
use frame_support::traits::{Get, UncheckedOnRuntimeUpgrade};

mod v1 {
	use super::*;

	/// Actual implementation of the storage migration.
	pub struct UncheckedMigrateToV1Impl<T, I>(PhantomData<(T, I)>);

	impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for UncheckedMigrateToV1Impl<T, I> {
		fn on_runtime_upgrade() -> frame_support::weights::Weight {
			let mut count = 0;
			for (collection, detail) in Collection::<T, I>::iter() {
				CollectionAccount::<T, I>::insert(&detail.owner, &collection, ());
				count += 1;
			}

			log::info!(
				target: LOG_TARGET,
				"Storage migration v1 for uniques finished.",
			);

			// calculate and return migration weights
			T::DbWeight::get().reads_writes(count as u64 + 1, count as u64 + 1)
		}
	}
}

/// Migrate the pallet storage from `0` to `1`.
pub type MigrateV0ToV1<T, I> = frame_support::migrations::VersionedMigration<
	0,
	1,
	v1::UncheckedMigrateToV1Impl<T, I>,
	Pallet<T, I>,
	<T as frame_system::Config>::DbWeight,
>;
