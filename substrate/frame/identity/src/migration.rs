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

extern crate alloc;

use super::*;
use frame_support::{
	migrations::VersionedMigration, pallet_prelude::*, storage_alias,
	traits::UncheckedOnRuntimeUpgrade, IterableStorageMap,
};

#[cfg(feature = "try-runtime")]
use alloc::collections::BTreeMap;
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

/// The old identity types in v0.
mod types_v0 {
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

/// The old identity types in v1.
mod types_v1 {
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

	#[cfg(feature = "try-runtime")]
	#[storage_alias]
	pub type PendingUsernames<T: Config> = StorageMap<
		Pallet<T>,
		Blake2_128Concat,
		Username<T>,
		(<T as frame_system::Config>::AccountId, BlockNumberFor<T>),
		OptionQuery,
	>;
}

pub mod v1 {
	use super::*;

	/// The log target.
	const TARGET: &'static str = "runtime::identity::migration::v1";
	/// Migration to add usernames to Identity info.
	///
	/// `T` is the runtime and `KL` is the key limit to migrate. This is just a safety guard to
	/// prevent stalling a parachain by accumulating too much weight in the migration. To have an
	/// unlimited migration (e.g. in a chain without PoV limits), set this to `u64::MAX`.
	pub struct VersionUncheckedMigrateV0ToV1<T, const KL: u64>(PhantomData<T>);
	impl<T: Config, const KL: u64> UncheckedOnRuntimeUpgrade for VersionUncheckedMigrateV0ToV1<T, KL> {
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			let identities = types_v0::IdentityOf::<T>::iter().count();
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

			for (account, registration) in types_v0::IdentityOf::<T>::iter() {
				types_v1::IdentityOf::<T>::insert(account, (registration, None::<Username<T>>));
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
			let identities = types_v1::IdentityOf::<T>::iter().count() as u64;
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
		weights::WeightMeter,
	};

	type HashedKey = BoundedVec<u8, ConstU32<256>>;
	// The resulting state of the step and the actual weight consumed.
	type StepResultOf<T> =
		MigrationState<<T as frame_system::Config>::AccountId, Username<T>, Suffix<T>>;

	#[cfg(feature = "runtime-benchmarks")]
	pub(crate) type BenchmarkingSetupOf<T> =
		BenchmarkingSetup<Suffix<T>, <T as frame_system::Config>::AccountId, Username<T>>;

	/// Progressive states of a migration. The migration starts with the first variant and ends with
	/// the last.
	#[derive(Decode, Encode, MaxEncodedLen, Eq, PartialEq)]
	pub enum MigrationState<A, U, S> {
		Authority(A),
		FinishedAuthorities,
		Identity(HashedKey),
		FinishedIdentities,
		Username(U),
		FinishedUsernames,
		PendingUsername(HashedKey),
		FinishedPendingUsernames,
		CleanupAuthorities(S),
		FinishedCleanupAuthorities,
		CleanupUsernames(U),
		Finished,
	}

	#[cfg(feature = "try-runtime")]
	#[derive(Encode, Decode)]
	struct TryRuntimeState<T: Config> {
		authorities: BTreeMap<Suffix<T>, (T::AccountId, u32)>,
		identities: BTreeMap<
			T::AccountId,
			Registration<
				BalanceOf<T>,
				<T as Config>::MaxRegistrars,
				<T as Config>::IdentityInformation,
			>,
		>,
		primary_usernames: BTreeMap<T::AccountId, Username<T>>,
		usernames: BTreeMap<Username<T>, T::AccountId>,
		pending_usernames: BTreeMap<Username<T>, (T::AccountId, BlockNumberFor<T>)>,
	}

	pub struct LazyMigrationV1ToV2<T: Config>(PhantomData<T>);
	impl<T: Config> SteppedMigration for LazyMigrationV1ToV2<T> {
		type Cursor = MigrationState<T::AccountId, Username<T>, Suffix<T>>;
		type Identifier = MigrationId<15>;

		fn id() -> Self::Identifier {
			MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 1, version_to: 2 }
		}

		fn step(
			mut cursor: Option<Self::Cursor>,
			meter: &mut WeightMeter,
		) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
			if Pallet::<T>::on_chain_storage_version() != Self::id().version_from as u16 {
				return Ok(None);
			}

			// Check that we have enough weight for at least the next step. If we don't, then the
			// migration cannot be complete.
			let required = match &cursor {
				Some(state) => Self::required_weight(&state),
				// Worst case weight for `authority_step`.
				None => T::WeightInfo::migration_v2_authority_step(),
			};
			if meter.remaining().any_lt(required) {
				return Err(SteppedMigrationError::InsufficientWeight { required });
			}

			loop {
				// Check that we would have enough weight to perform this step in the worst case
				// scenario.
				let required_weight = match &cursor {
					Some(state) => Self::required_weight(&state),
					// Worst case weight for `authority_step`.
					None => T::WeightInfo::migration_v2_authority_step(),
				};
				if !meter.can_consume(required_weight) {
					break;
				}

				let next = match &cursor {
					// At first, migrate any authorities.
					None => Self::authority_step(None),
					// Migrate any remaining authorities.
					Some(MigrationState::Authority(maybe_last_authority)) =>
						Self::authority_step(Some(maybe_last_authority)),
					// After the last authority was migrated, start migrating usernames from
					// the former `AccountOfUsername` into `UsernameInfoOf`.
					Some(MigrationState::FinishedAuthorities) => Self::username_step(None),
					// Keep migrating usernames.
					Some(MigrationState::Username(maybe_last_username)) =>
						Self::username_step(Some(maybe_last_username)),
					// After the last username was migrated, start migrating all identities in
					// `IdentityOf`, which currently hold the primary username of the owner account
					// as well as any associated identity. Accounts which set a username but not an
					// identity also have a zero deposit identity stored, which will be removed.
					Some(MigrationState::FinishedUsernames) => Self::identity_step(None),
					// Keep migrating identities.
					Some(MigrationState::Identity(last_key)) =>
						Self::identity_step(Some(last_key.clone())),
					// After the last identity was migrated, start migrating usernames pending
					// approval from `PendingUsernames`.
					Some(MigrationState::FinishedIdentities) => Self::pending_username_step(None),
					// Keep migrating pending usernames.
					Some(MigrationState::PendingUsername(last_key)) =>
						Self::pending_username_step(Some(last_key.clone())),
					// After the last pending username was migrated, start clearing the storage
					// previously associated with authorities in `UsernameAuthority`.
					Some(MigrationState::FinishedPendingUsernames) =>
						Self::cleanup_authority_step(None),
					// Keep clearing the obsolete authority storage.
					Some(MigrationState::CleanupAuthorities(maybe_last_username)) =>
						Self::cleanup_authority_step(Some(maybe_last_username)),
					// After the last obsolete authority was cleared from storage, start clearing
					// the storage previously associated with usernames in `AccountOfUsername`.
					Some(MigrationState::FinishedCleanupAuthorities) =>
						Self::cleanup_username_step(None),
					// Keep clearing the obsolete username storage.
					Some(MigrationState::CleanupUsernames(maybe_last_username)) =>
						Self::cleanup_username_step(Some(maybe_last_username)),
					// After the last obsolete username was cleared from storage, the migration is
					// done.
					Some(MigrationState::Finished) => {
						StorageVersion::new(Self::id().version_to as u16).put::<Pallet<T>>();
						return Ok(None)
					},
				};

				cursor = Some(next);
				meter.consume(required_weight);
			}

			Ok(cursor)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			let authorities: BTreeMap<Suffix<T>, (T::AccountId, u32)> =
				types_v1::UsernameAuthorities::<T>::iter()
					.map(|(account, authority_properties)| {
						(
							authority_properties.account_id,
							(account, authority_properties.allocation),
						)
					})
					.collect();
			let mut primary_usernames: BTreeMap<_, _> = Default::default();
			let identities = types_v1::IdentityOf::<T>::iter()
				.map(|(account, (identity, maybe_username))| {
					if let Some(username) = maybe_username {
						primary_usernames.insert(account.clone(), username);
					}
					(account, identity)
				})
				.collect::<BTreeMap<_, _>>();
			let usernames = types_v1::AccountOfUsername::<T>::iter().collect::<BTreeMap<_, _>>();
			let pending_usernames: BTreeMap<Username<T>, (T::AccountId, BlockNumberFor<T>)> =
				types_v1::PendingUsernames::<T>::iter().collect();
			let state: TryRuntimeState<T> = TryRuntimeState {
				authorities,
				identities,
				primary_usernames,
				usernames,
				pending_usernames,
			};

			Ok(state.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let mut prev_state: TryRuntimeState<T> = TryRuntimeState::<T>::decode(&mut &state[..])
				.expect("Failed to decode the previous storage state");

			for (suffix, authority_properties) in AuthorityOf::<T>::iter() {
				let (prev_account, prev_allocation) = prev_state
					.authorities
					.remove(&suffix)
					.expect("should have authority in previous state");
				assert_eq!(prev_account, authority_properties.account_id);
				assert_eq!(prev_allocation, authority_properties.allocation);
			}
			assert!(prev_state.authorities.is_empty());

			for (account, identity) in IdentityOf::<T>::iter() {
				assert!(identity.deposit > 0u32.into());
				let prev_identity = prev_state
					.identities
					.remove(&account)
					.expect("should have identity in previous state");
				assert_eq!(identity, prev_identity);
			}

			for (account, free_identity) in prev_state.identities.iter() {
				assert_eq!(free_identity.deposit, 0u32.into());
				assert!(UsernameOf::<T>::contains_key(&account));
			}
			prev_state.identities.clear();

			for (account, primary_username) in UsernameOf::<T>::iter() {
				let prev_primary_username = prev_state
					.primary_usernames
					.remove(&account)
					.expect("should have primary username in previous state");
				assert_eq!(prev_primary_username, primary_username);
			}

			for (username, username_info) in UsernameInfoOf::<T>::iter() {
				let prev_account = prev_state
					.usernames
					.remove(&username)
					.expect("should have username info in previous state");
				assert_eq!(prev_account, username_info.owner);
				assert_eq!(username_info.provider, Provider::Allocation);
			}
			assert!(prev_state.usernames.is_empty());

			for (username, (account, expiration, provider)) in PendingUsernames::<T>::iter() {
				let (prev_account, prev_expiration) = prev_state
					.pending_usernames
					.remove(&username)
					.expect("should have pending username in previous state");
				assert_eq!(prev_account, account);
				assert_eq!(prev_expiration, expiration);
				assert_eq!(provider, Provider::Allocation);
			}
			assert!(prev_state.pending_usernames.is_empty());

			Ok(())
		}
	}

	impl<T: Config> LazyMigrationV1ToV2<T> {
		pub(crate) fn required_weight(
			step: &MigrationState<T::AccountId, Username<T>, Suffix<T>>,
		) -> Weight {
			match step {
				MigrationState::Authority(_) => T::WeightInfo::migration_v2_authority_step(),
				MigrationState::FinishedAuthorities | MigrationState::Username(_) =>
					T::WeightInfo::migration_v2_username_step(),
				MigrationState::FinishedUsernames | MigrationState::Identity(_) =>
					T::WeightInfo::migration_v2_identity_step(),
				MigrationState::FinishedIdentities | MigrationState::PendingUsername(_) =>
					T::WeightInfo::migration_v2_pending_username_step(),
				MigrationState::FinishedPendingUsernames |
				MigrationState::CleanupAuthorities(_) => T::WeightInfo::migration_v2_cleanup_authority_step(),
				MigrationState::FinishedCleanupAuthorities |
				MigrationState::CleanupUsernames(_) => T::WeightInfo::migration_v2_cleanup_username_step(),
				MigrationState::Finished => Weight::zero(),
			}
		}

		// Migrate one entry from `UsernameAuthorities` to `AuthorityOf`.
		pub(crate) fn authority_step(maybe_last_key: Option<&T::AccountId>) -> StepResultOf<T> {
			let mut iter = if let Some(last_key) = maybe_last_key {
				types_v1::UsernameAuthorities::<T>::iter_from(
					types_v1::UsernameAuthorities::<T>::hashed_key_for(last_key),
				)
			} else {
				types_v1::UsernameAuthorities::<T>::iter()
			};
			if let Some((authority_account, properties)) = iter.next() {
				let suffix = properties.account_id;
				let allocation = properties.allocation;
				let new_properties =
					AuthorityProperties { account_id: authority_account.clone(), allocation };
				AuthorityOf::<T>::insert(&suffix, new_properties);
				MigrationState::Authority(authority_account)
			} else {
				MigrationState::FinishedAuthorities
			}
		}

		// Migrate one entry from `AccountOfUsername` to `UsernameInfoOf`.
		pub(crate) fn username_step(maybe_last_key: Option<&Username<T>>) -> StepResultOf<T> {
			let mut iter = if let Some(last_key) = maybe_last_key {
				types_v1::AccountOfUsername::<T>::iter_from(
					types_v1::AccountOfUsername::<T>::hashed_key_for(last_key),
				)
			} else {
				types_v1::AccountOfUsername::<T>::iter()
			};

			if let Some((username, owner_account)) = iter.next() {
				let username_info = UsernameInformation {
					owner: owner_account,
					provider: Provider::new_with_allocation(),
				};
				UsernameInfoOf::<T>::insert(&username, username_info);

				MigrationState::Username(username)
			} else {
				MigrationState::FinishedUsernames
			}
		}

		// Migrate one entry from `IdentityOf` to `UsernameOf`, if it has a username associated with
		// it. Remove the entry if there was no real identity associated with the account.
		pub(crate) fn identity_step(maybe_last_key: Option<HashedKey>) -> StepResultOf<T> {
			if let Some(mut last_key) =
				IdentityOf::<T>::translate_next::<
					(
						Registration<
							BalanceOf<T>,
							<T as pallet::Config>::MaxRegistrars,
							<T as pallet::Config>::IdentityInformation,
						>,
						Option<Username<T>>,
					),
					_,
				>(maybe_last_key.map(|b| b.to_vec()), |account, (identity, maybe_username)| {
					if let Some(primary_username) = maybe_username {
						UsernameOf::<T>::insert(&account, primary_username);
					}
					if identity.deposit > BalanceOf::<T>::zero() {
						Some(identity)
					} else {
						None
					}
				}) {
				last_key.truncate(HashedKey::bound());
				MigrationState::Identity(
					HashedKey::try_from(last_key)
						.expect("truncated to bound so the conversion must succeed; qed"),
				)
			} else {
				MigrationState::FinishedIdentities
			}
		}

		// Migrate one entry from `PendingUsernames` to contain the new `Provider` field.
		pub(crate) fn pending_username_step(maybe_last_key: Option<HashedKey>) -> StepResultOf<T> {
			if let Some(mut last_key) =
				PendingUsernames::<T>::translate_next::<(T::AccountId, BlockNumberFor<T>), _>(
					maybe_last_key.map(|b| b.to_vec()),
					|_, (owner_account, since)| {
						Some((owner_account, since, Provider::new_with_allocation()))
					},
				) {
				last_key.truncate(HashedKey::bound());
				MigrationState::PendingUsername(
					HashedKey::try_from(last_key)
						.expect("truncated to bound so the conversion must succeed; qed"),
				)
			} else {
				MigrationState::FinishedPendingUsernames
			}
		}

		// Remove one entry from `UsernameAuthorities`.
		pub(crate) fn cleanup_authority_step(
			maybe_last_key: Option<&Suffix<T>>,
		) -> StepResultOf<T> {
			let mut iter = if let Some(last_key) = maybe_last_key {
				AuthorityOf::<T>::iter_from(AuthorityOf::<T>::hashed_key_for(last_key))
			} else {
				AuthorityOf::<T>::iter()
			};

			if let Some((suffix, properties)) = iter.next() {
				let _ = types_v1::UsernameAuthorities::<T>::take(&properties.account_id);
				MigrationState::CleanupAuthorities(suffix)
			} else {
				MigrationState::FinishedCleanupAuthorities
			}
		}

		// Remove one entry from `AccountOfUsername`.
		pub(crate) fn cleanup_username_step(
			maybe_last_key: Option<&Username<T>>,
		) -> StepResultOf<T> {
			let mut iter = if let Some(last_key) = maybe_last_key {
				UsernameInfoOf::<T>::iter_from(UsernameInfoOf::<T>::hashed_key_for(last_key))
			} else {
				UsernameInfoOf::<T>::iter()
			};

			if let Some((username, _)) = iter.next() {
				let _ = types_v1::AccountOfUsername::<T>::take(&username);
				MigrationState::CleanupUsernames(username)
			} else {
				MigrationState::Finished
			}
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	pub(crate) struct BenchmarkingSetup<S, A, U> {
		pub(crate) suffix: S,
		pub(crate) authority: A,
		pub(crate) account: A,
		pub(crate) username: U,
	}

	#[cfg(feature = "runtime-benchmarks")]
	impl<T: Config> LazyMigrationV1ToV2<T> {
		pub(crate) fn setup_benchmark_env_for_migration() -> BenchmarkingSetupOf<T> {
			use frame_support::Hashable;
			let suffix: Suffix<T> = b"bench".to_vec().try_into().unwrap();
			let authority: T::AccountId = frame_benchmarking::account("authority", 0, 0);
			let account_id: T::AccountId = frame_benchmarking::account("account", 1, 0);

			let prop: AuthorityProperties<Suffix<T>> =
				AuthorityProperties { account_id: suffix.clone(), allocation: 10 };
			types_v1::UsernameAuthorities::<T>::insert(&authority, &prop);

			let username: Username<T> = b"account.bench".to_vec().try_into().unwrap();
			let info = T::IdentityInformation::create_identity_info();
			let registration: Registration<
				BalanceOf<T>,
				<T as Config>::MaxRegistrars,
				<T as Config>::IdentityInformation,
			> = Registration { judgements: Default::default(), deposit: 10u32.into(), info };
			frame_support::migration::put_storage_value(
				b"Identity",
				b"IdentityOf",
				&account_id.twox_64_concat(),
				(&registration, Some(username.clone())),
			);
			types_v1::AccountOfUsername::<T>::insert(&username, &account_id);
			let since: BlockNumberFor<T> = 0u32.into();
			frame_support::migration::put_storage_value(
				b"Identity",
				b"PendingUsernames",
				&username.blake2_128_concat(),
				(&account_id, since),
			);
			BenchmarkingSetup { suffix, authority, account: account_id, username }
		}

		pub(crate) fn setup_benchmark_env_for_cleanup() -> BenchmarkingSetupOf<T> {
			let suffix: Suffix<T> = b"bench".to_vec().try_into().unwrap();
			let authority: T::AccountId = frame_benchmarking::account("authority", 0, 0);
			let account_id: T::AccountId = frame_benchmarking::account("account", 1, 0);

			let prop: AuthorityProperties<Suffix<T>> =
				AuthorityProperties { account_id: suffix.clone(), allocation: 10 };
			types_v1::UsernameAuthorities::<T>::insert(&authority, &prop);
			let prop: AuthorityProperties<T::AccountId> =
				AuthorityProperties { account_id: authority.clone(), allocation: 10 };
			AuthorityOf::<T>::insert(&suffix, &prop);

			let username: Username<T> = b"account.bench".to_vec().try_into().unwrap();
			let info = T::IdentityInformation::create_identity_info();
			let registration: Registration<
				BalanceOf<T>,
				<T as Config>::MaxRegistrars,
				<T as Config>::IdentityInformation,
			> = Registration { judgements: Default::default(), deposit: 10u32.into(), info };
			IdentityOf::<T>::insert(&account_id, &registration);
			UsernameOf::<T>::insert(&account_id, &username);
			let username_info = UsernameInformation {
				owner: account_id.clone(),
				provider: Provider::new_with_allocation(),
			};
			UsernameInfoOf::<T>::insert(&username, username_info);
			types_v1::AccountOfUsername::<T>::insert(&username, &account_id);
			let since: BlockNumberFor<T> = 0u32.into();
			PendingUsernames::<T>::insert(
				&username,
				(&account_id, since, Provider::new_with_allocation()),
			);
			BenchmarkingSetup { suffix, authority, account: account_id, username }
		}

		pub(crate) fn check_authority_cleanup_validity(suffix: Suffix<T>, authority: T::AccountId) {
			assert_eq!(types_v1::UsernameAuthorities::<T>::iter().count(), 0);
			assert_eq!(AuthorityOf::<T>::get(&suffix).unwrap().account_id, authority);
		}

		pub(crate) fn check_username_cleanup_validity(
			username: Username<T>,
			account_id: T::AccountId,
		) {
			assert_eq!(types_v1::AccountOfUsername::<T>::iter().count(), 0);
			assert_eq!(UsernameInfoOf::<T>::get(&username).unwrap().owner, account_id);
		}
	}

	#[cfg(test)]
	mod tests {
		use frame_support::Hashable;

		use super::*;
		use crate::tests::{new_test_ext, Test};

		fn registration(
			with_deposit: bool,
		) -> Registration<
			BalanceOf<Test>,
			<Test as Config>::MaxRegistrars,
			<Test as Config>::IdentityInformation,
		> {
			Registration {
				judgements: Default::default(),
				deposit: if with_deposit { 10u32.into() } else { 0u32.into() },
				info: Default::default(),
			}
		}

		fn account_from_u8(byte: u8) -> <Test as frame_system::Config>::AccountId {
			[byte; 32].into()
		}

		#[test]
		fn migrate_to_v2() {
			new_test_ext().execute_with(|| {
				StorageVersion::new(1).put::<Pallet<Test>>();
				// Set up the first authority.
				let authority_1 = account_from_u8(151);
				let suffix_1: Suffix<Test> = b"evn".to_vec().try_into().unwrap();
				let prop = AuthorityProperties { account_id: suffix_1.clone(), allocation: 10 };
				types_v1::UsernameAuthorities::<Test>::insert(&authority_1, &prop);
				// Set up the first authority.
				let authority_2 = account_from_u8(152);
				let suffix_2: Suffix<Test> = b"odd".to_vec().try_into().unwrap();
				let prop = AuthorityProperties { account_id: suffix_2.clone(), allocation: 10 };
				types_v1::UsernameAuthorities::<Test>::insert(&authority_2, &prop);

				// (owner_account, primary_username, maybe_secondary_username, has_identity)
				// If `has_identity` is set, this `owner_account` will have a real identity
				// associated and a non-zero deposit for it.
				let mut usernames = vec![];
				for i in 0u8..100u8 {
					let account_id = account_from_u8(i);
					let bare_username = format!("acc{}.", i).as_bytes().to_vec();
					let mut username_1 = bare_username.clone();
					username_1.extend(suffix_1.iter());
					let username_1: Username<Test> = username_1.try_into().unwrap();
					types_v1::AccountOfUsername::<Test>::insert(&username_1, &account_id);

					if i % 2 == 0 {
						let has_identity = i % 4 == 0;
						let reg = registration(has_identity);
						frame_support::migration::put_storage_value(
							b"Identity",
							b"IdentityOf",
							&account_id.twox_64_concat(),
							(reg, Some(username_1.clone())),
						);
						usernames.push((account_id, username_1, None, has_identity));
					} else {
						let has_identity = i % 3 == 0;
						let mut username_2 = bare_username.clone();
						username_2.extend(suffix_2.iter());
						let username_2: Username<Test> = username_2.try_into().unwrap();
						types_v1::AccountOfUsername::<Test>::insert(&username_2, &account_id);
						let reg = registration(has_identity);
						frame_support::migration::put_storage_value(
							b"Identity",
							b"IdentityOf",
							&account_id.twox_64_concat(),
							(reg, Some(username_2.clone())),
						);
						usernames.push((account_id, username_2, Some(username_1), has_identity));
					}
				}

				// (username, owner_account, since)
				let mut pending = vec![];
				for i in 100u8..110u8 {
					let account_id = account_from_u8(i);
					let mut bare_username = format!("acc{}.", i).as_bytes().to_vec();
					bare_username.extend(suffix_1.iter());
					let username: Username<Test> = bare_username.try_into().unwrap();
					let since: BlockNumberFor<Test> = i.into();
					frame_support::migration::put_storage_value(
						b"Identity",
						b"PendingUsernames",
						&username.blake2_128_concat(),
						(&account_id, since),
					);
					pending.push((username, account_id, since));
				}

				let mut identity_only = vec![];
				for i in 120u8..130u8 {
					let account_id = account_from_u8(i);
					let reg = registration(true);
					frame_support::migration::put_storage_value(
						b"Identity",
						b"IdentityOf",
						&account_id.twox_64_concat(),
						(reg, None::<Username<Test>>),
					);
					identity_only.push(account_id);
				}

				// Run the actual migration.
				let mut weight_meter = WeightMeter::new();
				let mut cursor = None;
				while let Some(new_cursor) =
					LazyMigrationV1ToV2::<Test>::step(cursor, &mut weight_meter).unwrap()
				{
					cursor = Some(new_cursor);
				}
				assert_eq!(Pallet::<Test>::on_chain_storage_version(), 2);

				// Check that the authorities were migrated.
				let expected_prop =
					AuthorityProperties { account_id: authority_1.clone(), allocation: 10 };
				assert_eq!(AuthorityOf::<Test>::get(&suffix_1), Some(expected_prop));

				let expected_prop =
					AuthorityProperties { account_id: authority_2.clone(), allocation: 10 };
				assert_eq!(AuthorityOf::<Test>::get(&suffix_2), Some(expected_prop));

				// Check that the username information was migrated.
				let count_of_usernames_without_identities =
					usernames.iter().filter(|(_, _, _, has_id)| *has_id).count();
				assert_eq!(UsernameOf::<Test>::iter().count(), usernames.len());
				// All accounts have `evn` usernames, only half of them have `odd` usernames.
				assert_eq!(
					UsernameInfoOf::<Test>::iter().count(),
					usernames.len() + usernames.len() / 2
				);
				for (owner, primary, maybe_secondary, has_identity) in usernames.iter() {
					let username_info = UsernameInfoOf::<Test>::get(primary).unwrap();
					assert_eq!(&username_info.owner, owner);
					let actual_primary = UsernameOf::<Test>::get(owner).unwrap();
					assert_eq!(primary, &actual_primary);
					assert_eq!(IdentityOf::<Test>::contains_key(owner), *has_identity);
					if let Some(secondary) = maybe_secondary {
						let expected_info = UsernameInformation {
							owner: owner.clone(),
							provider: Provider::new_with_allocation(),
						};
						assert_eq!(UsernameInfoOf::<Test>::get(secondary), Some(expected_info));
					}
				}

				// Check that existing identities were preserved.
				for id in identity_only.iter() {
					let expected_reg = registration(true);
					assert_eq!(IdentityOf::<Test>::get(id), Some(expected_reg));
					assert!(!UsernameOf::<Test>::contains_key(id));
				}
				let identity_count = IdentityOf::<Test>::iter().count();
				assert_eq!(
					identity_count,
					count_of_usernames_without_identities + identity_only.len()
				);

				// Check that pending usernames were migrated.
				let pending_count = PendingUsernames::<Test>::iter().count();
				assert_eq!(pending_count, pending.len());
				for (username, owner, since) in pending.iter() {
					let expected_pending = (owner.clone(), *since, Provider::Allocation);
					assert_eq!(PendingUsernames::<Test>::get(username), Some(expected_pending));
				}

				// Check that obsolete storage was cleared.
				assert_eq!(types_v1::AccountOfUsername::<Test>::iter().count(), 0);
				assert_eq!(types_v1::UsernameAuthorities::<Test>::iter().count(), 0);
			});
		}
	}
}
