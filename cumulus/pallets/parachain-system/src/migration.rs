// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

use crate::{
	Config, LastProcessedHrmpMessage, Pallet, ReservedDmpWeightOverride, ReservedXcmpWeightOverride,
};
use frame_support::{
	pallet_prelude::*,
	traits::{Get, OnRuntimeUpgrade, StorageVersion},
	weights::Weight,
};

/// The in-code storage version.
pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(4);

pub use v4::MigrateV3ToV4;

// V4: Extend the `InboundMessageId` to also contain the sender.
pub mod v4 {
	use super::*;
	use crate::parachain_inherent::{InboundHrmpMessageId, InboundMessageId};
	use frame_support::traits::UncheckedOnRuntimeUpgrade;

	mod unversioned {
		use super::*;

		pub struct UncheckedMigrateV3ToV4<T: Config>(PhantomData<T>);
	}

	impl<T: Config> UncheckedOnRuntimeUpgrade for unversioned::UncheckedMigrateV3ToV4<T> {
		fn on_runtime_upgrade() -> frame_support::weights::Weight {
			let result =
				LastProcessedHrmpMessage::<T>::translate(|maybe_pre: Option<InboundMessageId>| {
					maybe_pre.map(|pre| InboundHrmpMessageId::Generic(pre))
				});
			if result.is_err() {
				log::error!(
					target: "parachain_system",
					"unexpected error when performing translation of the LastProcessedHrmpMessage to the new InboundMessageId type"
				);
			}

			T::DbWeight::get().reads_writes(1, 1)
		}
	}

	/// [`VersionedMigration`](frame_support::migrations::VersionedMigration), that is performed
	/// only when the on-chain version is 3.
	pub type MigrateV3ToV4<T> = frame_support::migrations::VersionedMigration<
		3,
		4,
		unversioned::UncheckedMigrateV3ToV4<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}

pub mod v3 {
	use super::*;
	use crate::parachain_inherent::InboundMessageId;

	#[frame_support::storage_alias]
	pub type LastProcessedHrmpMessage<T: Config> = StorageValue<Pallet<T>, InboundMessageId>;

	/// Migrates the pallet storage to the most recent version.
	pub struct Migration<T: Config>(PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for Migration<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight: Weight = T::DbWeight::get().reads(2);

			if StorageVersion::get::<Pallet<T>>() == 0 {
				weight = weight
					.saturating_add(v1::migrate::<T>())
					.saturating_add(T::DbWeight::get().writes(1));
				StorageVersion::new(1).put::<Pallet<T>>();
			}

			if StorageVersion::get::<Pallet<T>>() == 1 {
				weight = weight
					.saturating_add(v2::migrate::<T>())
					.saturating_add(T::DbWeight::get().writes(1));
				StorageVersion::new(2).put::<Pallet<T>>();
			}

			if StorageVersion::get::<Pallet<T>>() == 2 {
				// Runtime upgrades are in their own PoV so there is no issue with killing this.
				crate::PoVMessagesTracker::<T>::kill();
				weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));
				StorageVersion::new(3).put::<Pallet<T>>();
			}

			weight
		}
	}
}

/// V2: Migrate to 2D weights for ReservedXcmpWeightOverride and ReservedDmpWeightOverride.
mod v2 {
	use super::*;
	const DEFAULT_POV_SIZE: u64 = 64 * 1024; // 64 KB

	pub fn migrate<T: Config>() -> Weight {
		let translate = |pre: u64| -> Weight { Weight::from_parts(pre, DEFAULT_POV_SIZE) };

		if ReservedXcmpWeightOverride::<T>::translate(|pre| pre.map(translate)).is_err() {
			log::error!(
				target: "parachain_system",
				"unexpected error when performing translation of the ReservedXcmpWeightOverride type during storage upgrade to v2"
			);
		}

		if ReservedDmpWeightOverride::<T>::translate(|pre| pre.map(translate)).is_err() {
			log::error!(
				target: "parachain_system",
				"unexpected error when performing translation of the ReservedDmpWeightOverride type during storage upgrade to v2"
			);
		}

		T::DbWeight::get().reads_writes(2, 2)
	}
}

/// V1: `LastUpgrade` block number is removed from the storage since the upgrade
/// mechanism now uses signals instead of block offsets.
mod v1 {
	use crate::{Config, Pallet};
	use frame_support::{migration::clear_storage_prefix, pallet_prelude::*};

	pub fn migrate<T: Config>() -> Weight {
		let _ =
			clear_storage_prefix(<Pallet<T>>::name().as_bytes(), b"LastUpgrade", b"", None, None);
		T::DbWeight::get().writes(1)
	}
}

#[cfg(all(feature = "try-runtime", test))]
mod tests {
	use super::*;
	use crate::{
		mock::{new_test_ext, Test},
		parachain_inherent::{InboundHrmpMessageId, InboundMessageId},
	};
	use frame_support::traits::OnRuntimeUpgrade;

	#[test]
	#[allow(deprecated)]
	fn test_migrate_v3_to_v4() {
		new_test_ext().execute_with(|| {
			// None
			let storage_version = StorageVersion::new(3);
			storage_version.put::<Pallet<Test>>();
			frame_support::storage::unhashed::kill(
				&v3::LastProcessedHrmpMessage::<Test>::hashed_key(),
			);
			let bytes = v4::MigrateV3ToV4::<Test>::pre_upgrade();
			assert!(bytes.is_ok());
			v4::MigrateV3ToV4::<Test>::on_runtime_upgrade();
			assert!(v4::MigrateV3ToV4::<Test>::post_upgrade(bytes.unwrap()).is_ok());
			let post = crate::LastProcessedHrmpMessage::<Test>::get();
			assert_eq!(post, None);

			// Some
			let storage_version = StorageVersion::new(3);
			storage_version.put::<Pallet<Test>>();
			let pre = InboundMessageId { sent_at: 321, reverse_idx: 123 };
			frame_support::storage::unhashed::put_raw(
				&v3::LastProcessedHrmpMessage::<Test>::hashed_key(),
				&pre.encode(),
			);
			let bytes = v4::MigrateV3ToV4::<Test>::pre_upgrade();
			assert!(bytes.is_ok());
			v4::MigrateV3ToV4::<Test>::on_runtime_upgrade();
			assert!(v4::MigrateV3ToV4::<Test>::post_upgrade(bytes.unwrap()).is_ok());
			let post = crate::LastProcessedHrmpMessage::<Test>::get();
			assert_eq!(post, Some(InboundHrmpMessageId::Generic(pre)));
		});
	}
}
