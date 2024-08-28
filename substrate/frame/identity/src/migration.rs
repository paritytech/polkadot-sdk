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

//! Storage migrations for the Identity pallet.

use super::*;
use frame_support::{
	migrations::VersionedMigration, pallet_prelude::*, traits::UncheckedOnRuntimeUpgrade,
};

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

pub const PALLET_MIGRATIONS_ID: &[u8; 15] = b"pallet-identity";

pub mod versioned {
	use super::*;

	pub type V0ToV1<T, const KL: u64> = VersionedMigration<
		0,
		1,
		v1::VersionUncheckedMigrateV0ToV1<T, KL>,
		crate::pallet::Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}

pub mod v1 {
	use super::*;
	use frame_support::storage_alias;

	/// The log target.
	const TARGET: &'static str = "runtime::identity::migration::v1";

	/// The old identity type, useful in pre-upgrade.
	mod v0 {
		use super::*;

		#[storage_alias]
		pub type IdentityOf<T: Config> = StorageMap<
			Pallet<T>,
			Twox64Concat,
			<T as frame_system::Config>::AccountId,
			Registration<
				BalanceOf<T>,
				<T as pallet::Config>::MaxRegistrars,
				<T as pallet::Config>::IdentityInformation,
			>,
			OptionQuery,
		>;
	}

	mod vx {
		use super::*;

		#[storage_alias]
		pub type IdentityOf<T: Config> = StorageMap<
			Pallet<T>,
			Twox64Concat,
			<T as frame_system::Config>::AccountId,
			(
				Registration<
					BalanceOf<T>,
					<T as pallet::Config>::MaxRegistrars,
					<T as pallet::Config>::IdentityInformation,
				>,
				Option<Username<T>>,
			),
			OptionQuery,
		>;
	}

	/// Migration to add usernames to Identity info.
	///
	/// `T` is the runtime and `KL` is the key limit to migrate. This is just a safety guard to
	/// prevent stalling a parachain by accumulating too much weight in the migration. To have an
	/// unlimited migration (e.g. in a chain without PoV limits), set this to `u64::MAX`.
	pub struct VersionUncheckedMigrateV0ToV1<T, const KL: u64>(PhantomData<T>);
	impl<T: Config, const KL: u64> UncheckedOnRuntimeUpgrade for VersionUncheckedMigrateV0ToV1<T, KL> {
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			let identities = v0::IdentityOf::<T>::iter().count();
			log::info!(
				target: TARGET,
				"pre-upgrade state contains '{}' identities.",
				identities
			);
			ensure!((identities as u64) < KL, "too many identities to migrate");
			Ok((identities as u64).encode())
		}

		fn on_runtime_upgrade() -> Weight {
			log::info!(
				target: TARGET,
				"running storage migration from version 0 to version 1."
			);

			let mut weight = T::DbWeight::get().reads(1);
			let mut translated: u64 = 0;
			let mut interrupted = false;

			for (account, registration) in v0::IdentityOf::<T>::iter() {
				vx::IdentityOf::<T>::insert(account, (registration, None::<Username<T>>));
				translated.saturating_inc();
				if translated >= KL {
					log::warn!(
						"Incomplete! Migration limit reached. Only {} identities migrated.",
						translated
					);
					interrupted = true;
					break
				}
			}
			if !interrupted {
				log::info!("all {} identities migrated", translated);
			}

			weight.saturating_accrue(T::DbWeight::get().reads_writes(translated, translated));
			weight.saturating_accrue(T::DbWeight::get().writes(1));
			weight
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
			let identities_to_migrate: u64 = Decode::decode(&mut &state[..])
				.expect("failed to decode the state from pre-upgrade.");
			let identities = IdentityOf::<T>::iter().count() as u64;
			log::info!("post-upgrade expects '{}' identities to have been migrated.", identities);
			ensure!(identities_to_migrate == identities, "must migrate all identities.");
			log::info!(target: TARGET, "migrated all identities.");
			Ok(())
		}
	}
}

pub mod v2 {
	use super::*;
	use frame_support::{
		migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
		storage_alias,
		weights::WeightMeter,
	};

	mod v1 {
		use super::*;

		#[storage_alias]
		pub type IdentityOf<T: Config> = StorageMap<
			Pallet<T>,
			Twox64Concat,
			<T as frame_system::Config>::AccountId,
			(
				Registration<
					BalanceOf<T>,
					<T as pallet::Config>::MaxRegistrars,
					<T as pallet::Config>::IdentityInformation,
				>,
				Option<Username<T>>,
			),
			OptionQuery,
		>;

		#[storage_alias]
		pub type UsernameAuthorities<T: Config> = StorageMap<
			Pallet<T>,
			Twox64Concat,
			<T as frame_system::Config>::AccountId,
			AuthorityProperties<Suffix<T>>,
			OptionQuery,
		>;

		#[storage_alias]
		pub type AccountOfUsername<T: Config> = StorageMap<
			Pallet<T>,
			Blake2_128Concat,
			Username<T>,
			<T as frame_system::Config>::AccountId,
			OptionQuery,
		>;

		#[storage_alias]
		pub type PendingUsernames<T: Config> = StorageMap<
			Pallet<T>,
			Blake2_128Concat,
			Username<T>,
			(<T as frame_system::Config>::AccountId, BlockNumberFor<T>),
			OptionQuery,
		>;
	}

	#[derive(Decode, Encode, MaxEncodedLen, Eq, PartialEq)]
	pub enum MigrationState<A, U> {
		Authority(A),
		FinishedAuthorities,
		// TODO: All identities
		Username(U),
		Finished,
	}

	pub struct LazyMigrationV2<T: Config, W: weights::WeightInfo>(PhantomData<(T, W)>);
	impl<T: Config, W: weights::WeightInfo> SteppedMigration for LazyMigrationV2<T, W> {
		type Cursor = MigrationState<T::AccountId, Username<T>>;
		type Identifier = MigrationId<15>;

		fn id() -> Self::Identifier {
			MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 1, version_to: 2 }
		}

		fn step(
			mut cursor: Option<Self::Cursor>,
			meter: &mut WeightMeter,
		) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
			let required = match &cursor {
				Some(state) => Self::required_weight(&state),
				None => W::migration_v2_authority_step(),
			};

			if meter.remaining().any_lt(required) {
				return Err(SteppedMigrationError::InsufficientWeight { required });
			}

			loop {
				let required = match &cursor {
					Some(state) => Self::required_weight(&state),
					None => W::migration_v2_authority_step(),
				};

				if meter.try_consume(required).is_err() {
					break;
				}

				let next = match &cursor {
					None => Self::authority_step(None),
					Some(MigrationState::Authority(maybe_last_authority)) =>
						Self::authority_step(Some(maybe_last_authority)),
					Some(MigrationState::FinishedAuthorities) => Self::username_step(None),
					Some(MigrationState::Username(maybe_last_username)) =>
						Self::username_step(Some(maybe_last_username)),
					Some(MigrationState::Finished) => return Ok(None),
				};

				cursor = Some(next);
			}

			Ok(cursor)
		}
	}

	impl<T: Config, W: weights::WeightInfo> LazyMigrationV2<T, W> {
		fn required_weight(step: &MigrationState<T::AccountId, Username<T>>) -> Weight {
			match step {
				MigrationState::Authority(_) => W::migration_v2_authority_step(),
				MigrationState::FinishedAuthorities |
				MigrationState::Username(_) |
				MigrationState::Finished => W::migration_v2_username_step(),
			}
		}

		fn username_step(
			maybe_last_key: Option<&Username<T>>,
		) -> MigrationState<T::AccountId, Username<T>> {
			let mut iter = if let Some(last_key) = maybe_last_key {
				v1::AccountOfUsername::<T>::iter_from(v1::AccountOfUsername::<T>::hashed_key_for(
					last_key,
				))
			} else {
				v1::AccountOfUsername::<T>::iter()
			};

			if let Some((username, owner_account)) = iter.next() {
				match v1::IdentityOf::<T>::take(&owner_account) {
					Some((identity, Some(primary_username))) => {
						if identity.deposit > BalanceOf::<T>::zero() {
							IdentityOf::<T>::insert(&owner_account, identity);
						}
						UsernameOf::<T>::insert(&owner_account, primary_username);
					},
					_ => {},
				}
				let username_info =
					UsernameInformation { owner: owner_account, provider: Provider::Governance };
				UsernameInfoOf::<T>::insert(&username, username_info);

				MigrationState::Username(username)
			} else {
				MigrationState::Finished
			}
		}

		fn authority_step(
			maybe_last_key: Option<&T::AccountId>,
		) -> MigrationState<T::AccountId, Username<T>> {
			let mut iter = if let Some(last_key) = maybe_last_key {
				v1::UsernameAuthorities::<T>::iter_from(
					v1::UsernameAuthorities::<T>::hashed_key_for(last_key),
				)
			} else {
				v1::UsernameAuthorities::<T>::iter()
			};
			if let Some((authority_account, properties)) = iter.next() {
				let suffix = properties.account_id;
				let allocation = properties.allocation;
				let new_properties =
					AuthorityProperties { account_id: authority_account.clone(), allocation };
				UsernameAuthorities::<T>::insert(&suffix, new_properties);
				MigrationState::Authority(authority_account)
			} else {
				MigrationState::FinishedAuthorities
			}
		}
	}
}
