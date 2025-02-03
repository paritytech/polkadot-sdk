// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use crate::{
	pallet::CurrentMigration, Config, CurrentXcmVersion, Pallet, VersionMigrationStage,
	VersionNotifyTargets,
};
use frame_support::{
	pallet_prelude::*,
	traits::{OnRuntimeUpgrade, StorageVersion, UncheckedOnRuntimeUpgrade},
	weights::Weight,
};

const DEFAULT_PROOF_SIZE: u64 = 64 * 1024;

/// Utilities for handling XCM version migration for the relevant data.
pub mod data {
	use crate::*;

	/// A trait for handling XCM versioned data migration for the requested `XcmVersion`.
	pub(crate) trait NeedsMigration {
		type MigratedData;

		/// Returns true if data does not match `minimal_allowed_xcm_version`.
		fn needs_migration(&self, minimal_allowed_xcm_version: XcmVersion) -> bool;

		/// Attempts to migrate data. `Ok(None)` means no migration is needed.
		/// `Ok(Some(Self::MigratedData))` should contain the migrated data.
		fn try_migrate(self, to_xcm_version: XcmVersion) -> Result<Option<Self::MigratedData>, ()>;
	}

	/// Implementation of `NeedsMigration` for `LockedFungibles` data.
	impl<B, M> NeedsMigration for BoundedVec<(B, VersionedLocation), M> {
		type MigratedData = Self;

		fn needs_migration(&self, minimal_allowed_xcm_version: XcmVersion) -> bool {
			self.iter()
				.any(|(_, unlocker)| unlocker.identify_version() < minimal_allowed_xcm_version)
		}

		fn try_migrate(
			mut self,
			to_xcm_version: XcmVersion,
		) -> Result<Option<Self::MigratedData>, ()> {
			let mut was_modified = false;
			for locked in self.iter_mut() {
				if locked.1.identify_version() < to_xcm_version {
					let Ok(new_unlocker) = locked.1.clone().into_version(to_xcm_version) else {
						return Err(())
					};
					locked.1 = new_unlocker;
					was_modified = true;
				}
			}

			if was_modified {
				Ok(Some(self))
			} else {
				Ok(None)
			}
		}
	}

	/// Implementation of `NeedsMigration` for `Queries` data.
	impl<BlockNumber> NeedsMigration for QueryStatus<BlockNumber> {
		type MigratedData = Self;

		fn needs_migration(&self, minimal_allowed_xcm_version: XcmVersion) -> bool {
			match &self {
				QueryStatus::Pending { responder, maybe_match_querier, .. } =>
					responder.identify_version() < minimal_allowed_xcm_version ||
						maybe_match_querier
							.as_ref()
							.map(|v| v.identify_version() < minimal_allowed_xcm_version)
							.unwrap_or(false),
				QueryStatus::VersionNotifier { origin, .. } =>
					origin.identify_version() < minimal_allowed_xcm_version,
				QueryStatus::Ready { response, .. } =>
					response.identify_version() < minimal_allowed_xcm_version,
			}
		}

		fn try_migrate(self, to_xcm_version: XcmVersion) -> Result<Option<Self::MigratedData>, ()> {
			if !self.needs_migration(to_xcm_version) {
				return Ok(None)
			}

			// do migration
			match self {
				QueryStatus::Pending { responder, maybe_match_querier, maybe_notify, timeout } => {
					let Ok(responder) = responder.into_version(to_xcm_version) else {
						return Err(())
					};
					let Ok(maybe_match_querier) =
						maybe_match_querier.map(|mmq| mmq.into_version(to_xcm_version)).transpose()
					else {
						return Err(())
					};
					Ok(Some(QueryStatus::Pending {
						responder,
						maybe_match_querier,
						maybe_notify,
						timeout,
					}))
				},
				QueryStatus::VersionNotifier { origin, is_active } => origin
					.into_version(to_xcm_version)
					.map(|origin| Some(QueryStatus::VersionNotifier { origin, is_active })),
				QueryStatus::Ready { response, at } => response
					.into_version(to_xcm_version)
					.map(|response| Some(QueryStatus::Ready { response, at })),
			}
		}
	}

	/// Implementation of `NeedsMigration` for `RemoteLockedFungibles` key type.
	impl<A> NeedsMigration for (XcmVersion, A, VersionedAssetId) {
		type MigratedData = Self;

		fn needs_migration(&self, minimal_allowed_xcm_version: XcmVersion) -> bool {
			self.0 < minimal_allowed_xcm_version ||
				self.2.identify_version() < minimal_allowed_xcm_version
		}

		fn try_migrate(self, to_xcm_version: XcmVersion) -> Result<Option<Self::MigratedData>, ()> {
			if !self.needs_migration(to_xcm_version) {
				return Ok(None)
			}

			let Ok(asset_id) = self.2.into_version(to_xcm_version) else { return Err(()) };
			Ok(Some((to_xcm_version, self.1, asset_id)))
		}
	}

	/// Implementation of `NeedsMigration` for `RemoteLockedFungibles` data.
	impl<ConsumerIdentifier, MaxConsumers: Get<u32>> NeedsMigration
		for RemoteLockedFungibleRecord<ConsumerIdentifier, MaxConsumers>
	{
		type MigratedData = Self;

		fn needs_migration(&self, minimal_allowed_xcm_version: XcmVersion) -> bool {
			self.owner.identify_version() < minimal_allowed_xcm_version ||
				self.locker.identify_version() < minimal_allowed_xcm_version
		}

		fn try_migrate(self, to_xcm_version: XcmVersion) -> Result<Option<Self::MigratedData>, ()> {
			if !self.needs_migration(to_xcm_version) {
				return Ok(None)
			}

			let RemoteLockedFungibleRecord { amount, owner, locker, consumers } = self;

			let Ok(owner) = owner.into_version(to_xcm_version) else { return Err(()) };
			let Ok(locker) = locker.into_version(to_xcm_version) else { return Err(()) };

			Ok(Some(RemoteLockedFungibleRecord { amount, owner, locker, consumers }))
		}
	}

	impl<T: Config> Pallet<T> {
		/// Migrates relevant data to the `required_xcm_version`.
		pub(crate) fn migrate_data_to_xcm_version(
			weight: &mut Weight,
			required_xcm_version: XcmVersion,
		) {
			const LOG_TARGET: &str = "runtime::xcm::pallet_xcm::migrate_data_to_xcm_version";

			// check and migrate `Queries`
			let queries_to_migrate = Queries::<T>::iter().filter_map(|(id, data)| {
				weight.saturating_add(T::DbWeight::get().reads(1));
				match data.try_migrate(required_xcm_version) {
					Ok(Some(new_data)) => Some((id, new_data)),
					Ok(None) => None,
					Err(_) => {
						tracing::error!(
							target: LOG_TARGET,
							?id,
							?required_xcm_version,
							"`Queries` cannot be migrated!"
						);
						None
					},
				}
			});
			for (id, new_data) in queries_to_migrate {
				tracing::info!(
					target: LOG_TARGET,
					query_id = ?id,
					?new_data,
					"Migrating `Queries`"
				);
				Queries::<T>::insert(id, new_data);
				weight.saturating_add(T::DbWeight::get().writes(1));
			}

			// check and migrate `LockedFungibles`
			let locked_fungibles_to_migrate =
				LockedFungibles::<T>::iter().filter_map(|(id, data)| {
					weight.saturating_add(T::DbWeight::get().reads(1));
					match data.try_migrate(required_xcm_version) {
						Ok(Some(new_data)) => Some((id, new_data)),
						Ok(None) => None,
						Err(_) => {
							tracing::error!(
								target: LOG_TARGET,
								?id,
								?required_xcm_version,
								"`LockedFungibles` cannot be migrated!"
							);
							None
						},
					}
				});
			for (id, new_data) in locked_fungibles_to_migrate {
				tracing::info!(
					target: LOG_TARGET,
					account_id = ?id,
					?new_data,
					"Migrating `LockedFungibles`"
				);
				LockedFungibles::<T>::insert(id, new_data);
				weight.saturating_add(T::DbWeight::get().writes(1));
			}

			// check and migrate `RemoteLockedFungibles` - 1. step - just data
			let remote_locked_fungibles_to_migrate =
				RemoteLockedFungibles::<T>::iter().filter_map(|(id, data)| {
					weight.saturating_add(T::DbWeight::get().reads(1));
					match data.try_migrate(required_xcm_version) {
						Ok(Some(new_data)) => Some((id, new_data)),
						Ok(None) => None,
						Err(_) => {
							tracing::error!(
								target: LOG_TARGET,
								?id,
								?required_xcm_version,
								"`RemoteLockedFungibles` data cannot be migrated!"
							);
							None
						},
					}
				});
			for (id, new_data) in remote_locked_fungibles_to_migrate {
				tracing::info!(
					target: LOG_TARGET,
					key = ?id,
					amount = ?new_data.amount,
					locker = ?new_data.locker,
					owner = ?new_data.owner,
					consumers_count = ?new_data.consumers.len(),
					"Migrating `RemoteLockedFungibles` data"
				);
				RemoteLockedFungibles::<T>::insert(id, new_data);
				weight.saturating_add(T::DbWeight::get().writes(1));
			}

			// check and migrate `RemoteLockedFungibles` - 2. step - key
			let remote_locked_fungibles_keys_to_migrate = RemoteLockedFungibles::<T>::iter_keys()
				.filter_map(|key| {
					if key.needs_migration(required_xcm_version) {
						let old_key = key.clone();
						match key.try_migrate(required_xcm_version) {
							Ok(Some(new_key)) => Some((old_key, new_key)),
							Ok(None) => None,
							Err(_) => {
								tracing::error!(
									target: LOG_TARGET,
									id = ?old_key,
									?required_xcm_version,
									"`RemoteLockedFungibles` key cannot be migrated!"
								);
								None
							},
						}
					} else {
						None
					}
				});
			for (old_key, new_key) in remote_locked_fungibles_keys_to_migrate {
				weight.saturating_add(T::DbWeight::get().reads(1));
				// make sure, that we don't override accidentally other data
				if RemoteLockedFungibles::<T>::get(&new_key).is_some() {
					tracing::error!(
						target: LOG_TARGET,
						?old_key,
						?new_key,
						"`RemoteLockedFungibles` already contains data for a `new_key`!"
					);
					// let's just skip for now, could be potentially caused with missing this
					// migration before (manual clean-up?).
					continue;
				}

				tracing::info!(
					target: LOG_TARGET,
					?old_key,
					?new_key,
					"Migrating `RemoteLockedFungibles` key"
				);

				// now we can swap the keys
				RemoteLockedFungibles::<T>::swap::<
					(
						NMapKey<Twox64Concat, XcmVersion>,
						NMapKey<Blake2_128Concat, T::AccountId>,
						NMapKey<Blake2_128Concat, VersionedAssetId>,
					),
					_,
					_,
				>(&old_key, &new_key);
				weight.saturating_add(T::DbWeight::get().writes(1));
			}
		}
	}
}

pub mod v1 {
	use super::*;
	use crate::{CurrentMigration, VersionMigrationStage};

	/// Named with the 'VersionUnchecked'-prefix because although this implements some version
	/// checking, the version checking is not complete as it will begin failing after the upgrade is
	/// enacted on-chain.
	///
	/// Use experimental [`MigrateToV1`] instead.
	pub struct VersionUncheckedMigrateToV1<T>(core::marker::PhantomData<T>);
	impl<T: Config> UncheckedOnRuntimeUpgrade for VersionUncheckedMigrateToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = T::DbWeight::get().reads(1);

			if StorageVersion::get::<Pallet<T>>() != 0 {
				tracing::warn!("skipping v1, should be removed");
				return weight
			}

			weight.saturating_accrue(T::DbWeight::get().writes(1));
			CurrentMigration::<T>::put(VersionMigrationStage::default());

			let translate = |pre: (u64, u64, u32)| -> Option<(u64, Weight, u32)> {
				weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
				let translated = (pre.0, Weight::from_parts(pre.1, DEFAULT_PROOF_SIZE), pre.2);
				tracing::info!("Migrated VersionNotifyTarget {:?} to {:?}", pre, translated);
				Some(translated)
			};

			VersionNotifyTargets::<T>::translate_values(translate);

			tracing::info!("v1 applied successfully");
			weight.saturating_accrue(T::DbWeight::get().writes(1));
			StorageVersion::new(1).put::<Pallet<T>>();
			weight
		}
	}

	/// Version checked migration to v1.
	///
	/// Wrapped in [`frame_support::migrations::VersionedMigration`] so the pre/post checks don't
	/// begin failing after the upgrade is enacted on-chain.
	pub type MigrateToV1<T> = frame_support::migrations::VersionedMigration<
		0,
		1,
		VersionUncheckedMigrateToV1<T>,
		crate::pallet::Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}

/// When adding a new XCM version, we need to run this migration for `pallet_xcm` to ensure that all
/// previously stored data with subkey prefix `XCM_VERSION-1` (and below) are migrated to the
/// `XCM_VERSION`.
///
/// NOTE: This migration can be permanently added to the runtime migrations.
pub struct MigrateToLatestXcmVersion<T>(core::marker::PhantomData<T>);
impl<T: Config> OnRuntimeUpgrade for MigrateToLatestXcmVersion<T> {
	fn on_runtime_upgrade() -> Weight {
		let mut weight = T::DbWeight::get().reads(1);

		// trigger expensive/lazy migration (kind of multi-block)
		CurrentMigration::<T>::put(VersionMigrationStage::default());
		weight.saturating_accrue(T::DbWeight::get().writes(1));

		// migrate other operational data to the latest XCM version in-place
		let latest = CurrentXcmVersion::get();
		Pallet::<T>::migrate_data_to_xcm_version(&mut weight, latest);

		weight
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use data::NeedsMigration;
		const LOG_TARGET: &str = "runtime::xcm::pallet_xcm::migrate_to_latest";

		let latest = CurrentXcmVersion::get();

		let number_of_queries_to_migrate = crate::Queries::<T>::iter()
			.filter(|(id, data)| {
				let needs_migration = data.needs_migration(latest);
				if needs_migration {
					tracing::warn!(
						target: LOG_TARGET,
						query_id = ?id,
						query = ?data,
						"Query was not migrated!"
					)
				}
				needs_migration
			})
			.count();

		let number_of_locked_fungibles_to_migrate = crate::LockedFungibles::<T>::iter()
			.filter_map(|(id, data)| {
				if data.needs_migration(latest) {
					tracing::warn!(
						target: LOG_TARGET,
						?id,
						?data,
						"LockedFungibles item was not migrated!"
					);
					Some(true)
				} else {
					None
				}
			})
			.count();

		let number_of_remote_locked_fungibles_to_migrate =
			crate::RemoteLockedFungibles::<T>::iter()
				.filter_map(|(key, data)| {
					if key.needs_migration(latest) || data.needs_migration(latest) {
						tracing::warn!(
							target: LOG_TARGET,
							?key,
							"RemoteLockedFungibles item was not migrated!"
						);
						Some(true)
					} else {
						None
					}
				})
				.count();

		ensure!(number_of_queries_to_migrate == 0, "must migrate all `Queries`.");
		ensure!(number_of_locked_fungibles_to_migrate == 0, "must migrate all `LockedFungibles`.");
		ensure!(
			number_of_remote_locked_fungibles_to_migrate == 0,
			"must migrate all `RemoteLockedFungibles`."
		);

		Ok(())
	}
}
