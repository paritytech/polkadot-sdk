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
	IterableStorageMap,
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

	type HashedKey = BoundedVec<u8, ConstU32<256>>;

	mod v1 {
		use super::*;

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
	pub enum MigrationState<A, U, S> {
		Authority(A),
		FinishedAuthorities,
		Identity(HashedKey),
		FinishedIdentities,
		Username(U),
		FinishedUsernames,
		PendingUsername(U),
		FinishedPendingUsernames,
		CleanupAuthorities(S),
		FinishedCleanupAuthorities,
		CleanupUsernames(U),
		FinishedCleanupUsernames,
		CleanupPendingUsernames(U),
		Finished,
	}

	pub struct LazyMigrationV2<T: Config, W: weights::WeightInfo>(PhantomData<(T, W)>);
	impl<T: Config, W: weights::WeightInfo> SteppedMigration for LazyMigrationV2<T, W> {
		type Cursor = MigrationState<T::AccountId, Username<T>, Suffix<T>>;
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
					Some(MigrationState::FinishedUsernames) => Self::identity_step(None),
					Some(MigrationState::Identity(last_key)) =>
						Self::identity_step(Some(last_key.clone())),
					Some(MigrationState::FinishedIdentities) => Self::pending_username_step(None),
					Some(MigrationState::PendingUsername(maybe_last_username)) =>
						Self::pending_username_step(Some(maybe_last_username)),
					Some(MigrationState::FinishedPendingUsernames) =>
						Self::cleanup_authority_step(None),
					Some(MigrationState::CleanupAuthorities(maybe_last_username)) =>
						Self::cleanup_authority_step(Some(maybe_last_username)),
					Some(MigrationState::FinishedCleanupAuthorities) =>
						Self::cleanup_username_step(None),
					Some(MigrationState::CleanupUsernames(maybe_last_username)) =>
						Self::cleanup_username_step(Some(maybe_last_username)),
					Some(MigrationState::FinishedCleanupUsernames) =>
						Self::cleanup_pending_username_step(None),
					Some(MigrationState::CleanupPendingUsernames(maybe_last_username)) =>
						Self::cleanup_pending_username_step(Some(maybe_last_username)),
					Some(MigrationState::Finished) => return Ok(None),
				};

				cursor = Some(next);
			}

			Ok(cursor)
		}
	}

	impl<T: Config, W: weights::WeightInfo> LazyMigrationV2<T, W> {
		#[allow(unused)]
		fn pretty_username(username: &Username<T>) -> String {
			String::from_utf8(username.to_vec()).unwrap()
		}

		pub(crate) fn required_weight(
			step: &MigrationState<T::AccountId, Username<T>, Suffix<T>>,
		) -> Weight {
			match step {
				MigrationState::Authority(_) => W::migration_v2_authority_step(),
				MigrationState::FinishedAuthorities | MigrationState::Identity(_) =>
					W::migration_v2_identity_step(),
				MigrationState::FinishedIdentities | MigrationState::Username(_) =>
					W::migration_v2_username_step(),
				MigrationState::FinishedUsernames | MigrationState::PendingUsername(_) =>
					W::migration_v2_pending_username_step(),
				MigrationState::FinishedPendingUsernames |
				MigrationState::CleanupAuthorities(_) => W::migration_v2_cleanup_authority_step(),
				MigrationState::FinishedCleanupAuthorities |
				MigrationState::CleanupUsernames(_) => W::migration_v2_cleanup_username_step(),
				MigrationState::FinishedCleanupUsernames |
				MigrationState::CleanupPendingUsernames(_) => W::migration_v2_cleanup_pending_username_step(),
				MigrationState::Finished => Weight::zero(),
			}
		}

		pub(crate) fn authority_step(
			maybe_last_key: Option<&T::AccountId>,
		) -> MigrationState<T::AccountId, Username<T>, Suffix<T>> {
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
				AuthorityOf::<T>::insert(&suffix, new_properties);
				MigrationState::Authority(authority_account)
			} else {
				MigrationState::FinishedAuthorities
			}
		}

		pub(crate) fn username_step(
			maybe_last_key: Option<&Username<T>>,
		) -> MigrationState<T::AccountId, Username<T>, Suffix<T>> {
			let mut iter = if let Some(last_key) = maybe_last_key {
				v1::AccountOfUsername::<T>::iter_from(v1::AccountOfUsername::<T>::hashed_key_for(
					last_key,
				))
			} else {
				v1::AccountOfUsername::<T>::iter()
			};

			if let Some((username, owner_account)) = iter.next() {
				let username_info =
					UsernameInformation { owner: owner_account, provider: Provider::Governance };
				UsernameInfoOf::<T>::insert(&username, username_info);

				MigrationState::Username(username)
			} else {
				MigrationState::FinishedUsernames
			}
		}

		pub(crate) fn identity_step(
			maybe_last_key: Option<HashedKey>,
		) -> MigrationState<T::AccountId, Username<T>, Suffix<T>> {
			if let Some(last_key) =
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
				MigrationState::Identity(last_key.try_into().unwrap())
			} else {
				MigrationState::FinishedIdentities
			}
		}

		pub(crate) fn pending_username_step(
			maybe_last_key: Option<&Username<T>>,
		) -> MigrationState<T::AccountId, Username<T>, Suffix<T>> {
			let mut iter = if let Some(last_key) = maybe_last_key {
				v1::PendingUsernames::<T>::iter_from(v1::PendingUsernames::<T>::hashed_key_for(
					last_key,
				))
			} else {
				v1::PendingUsernames::<T>::iter()
			};

			if let Some((username, (owner_account, since))) = iter.next() {
				PendingAcceptance::<T>::insert(
					&username,
					(owner_account, since, Provider::Governance),
				);
				MigrationState::PendingUsername(username)
			} else {
				MigrationState::FinishedPendingUsernames
			}
		}

		pub(crate) fn cleanup_authority_step(
			maybe_last_key: Option<&Suffix<T>>,
		) -> MigrationState<T::AccountId, Username<T>, Suffix<T>> {
			let mut iter = if let Some(last_key) = maybe_last_key {
				AuthorityOf::<T>::iter_from(AuthorityOf::<T>::hashed_key_for(last_key))
			} else {
				AuthorityOf::<T>::iter()
			};

			if let Some((suffix, properties)) = iter.next() {
				let _ = v1::UsernameAuthorities::<T>::take(&properties.account_id);
				MigrationState::CleanupAuthorities(suffix)
			} else {
				MigrationState::FinishedCleanupAuthorities
			}
		}

		pub(crate) fn cleanup_username_step(
			maybe_last_key: Option<&Username<T>>,
		) -> MigrationState<T::AccountId, Username<T>, Suffix<T>> {
			let mut iter = if let Some(last_key) = maybe_last_key {
				UsernameInfoOf::<T>::iter_from(UsernameInfoOf::<T>::hashed_key_for(last_key))
			} else {
				UsernameInfoOf::<T>::iter()
			};

			if let Some((username, _)) = iter.next() {
				let _ = v1::AccountOfUsername::<T>::take(&username);
				MigrationState::CleanupUsernames(username)
			} else {
				MigrationState::FinishedCleanupUsernames
			}
		}

		pub(crate) fn cleanup_pending_username_step(
			maybe_last_key: Option<&Username<T>>,
		) -> MigrationState<T::AccountId, Username<T>, Suffix<T>> {
			let mut iter = if let Some(last_key) = maybe_last_key {
				PendingAcceptance::<T>::iter_from(PendingAcceptance::<T>::hashed_key_for(last_key))
			} else {
				PendingAcceptance::<T>::iter()
			};

			if let Some((username, _)) = iter.next() {
				let _ = v1::PendingUsernames::<T>::take(&username);

				MigrationState::CleanupPendingUsernames(username)
			} else {
				MigrationState::Finished
			}
		}

		#[cfg(feature = "runtime-benchmarks")]
		pub(crate) fn setup_benchmark_env() {
			use frame_benchmarking::account;
			use frame_support::Hashable;
			let suffix: Suffix<T> = b"bench".to_vec().try_into().unwrap();
			let authority: T::AccountId = account("authority", 0, 0);
			let account_with_username: T::AccountId = account("account", 1, 0);
			let account_without_username: T::AccountId = account("account", 2, 0);

			let prop: AuthorityProperties<Suffix<T>> =
				AuthorityProperties { account_id: suffix.clone(), allocation: 10 };
			v1::UsernameAuthorities::<T>::insert(&authority, &prop);

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
				&account_with_username.twox_64_concat(),
				(&registration, Some(username.clone())),
			);
			frame_support::migration::put_storage_value(
				b"Identity",
				b"IdentityOf",
				&account_without_username.twox_64_concat(),
				(&registration, None::<Username<T>>),
			);
			v1::AccountOfUsername::<T>::insert(&username, &account_with_username);
			v1::PendingUsernames::<T>::insert(&username, &(account_with_username, 0u32.into()));
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
				let authority_1 = account_from_u8(151);
				let suffix_1: Suffix<Test> = b"evn".to_vec().try_into().unwrap();
				let prop = AuthorityProperties { account_id: suffix_1.clone(), allocation: 10 };
				v1::UsernameAuthorities::<Test>::insert(&authority_1, &prop);

				let authority_2 = account_from_u8(152);
				let suffix_2: Suffix<Test> = b"odd".to_vec().try_into().unwrap();
				let prop = AuthorityProperties { account_id: suffix_2.clone(), allocation: 10 };
				v1::UsernameAuthorities::<Test>::insert(&authority_2, &prop);

				let mut usernames = vec![];
				for i in 0u8..10u8 {
					let account_id = account_from_u8(i);
					let bare_username = format!("acc{}.", i).as_bytes().to_vec();
					let mut username_1 = bare_username.clone();
					username_1.extend(suffix_1.iter());
					let username_1: Username<Test> = username_1.try_into().unwrap();
					v1::AccountOfUsername::<Test>::insert(&username_1, &account_id);

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
						v1::AccountOfUsername::<Test>::insert(&username_2, &account_id);
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

				let mut pending = vec![];
				for i in 20u8..25u8 {
					let account_id = account_from_u8(i);
					let mut bare_username = format!("acc{}.", i).as_bytes().to_vec();
					bare_username.extend(suffix_1.iter());
					let username: Username<Test> = bare_username.try_into().unwrap();
					let since: BlockNumberFor<Test> = i.into();
					v1::PendingUsernames::<Test>::insert(&username, (account_id.clone(), since));
					pending.push((username, account_id, since));
				}

				let mut identity_only = vec![];
				for i in 30u8..35u8 {
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

				let mut weight_meter = WeightMeter::new();
				let mut cursor = None;
				while let Some(new_cursor) =
					LazyMigrationV2::<Test, <Test as Config>::WeightInfo>::step(
						cursor,
						&mut weight_meter,
					)
					.unwrap()
				{
					cursor = Some(new_cursor);
				}

				let expected_prop =
					AuthorityProperties { account_id: authority_1.clone(), allocation: 10 };
				assert_eq!(AuthorityOf::<Test>::get(&suffix_1), Some(expected_prop));

				let expected_prop =
					AuthorityProperties { account_id: authority_2.clone(), allocation: 10 };
				assert_eq!(AuthorityOf::<Test>::get(&suffix_2), Some(expected_prop));

				for (owner, primary, maybe_secondary, has_identity) in usernames.iter() {
					let username_info = UsernameInfoOf::<Test>::get(primary).unwrap();
					assert_eq!(&username_info.owner, owner);
					let actual_primary = UsernameOf::<Test>::get(owner).unwrap();
					assert_eq!(primary, &actual_primary);
					assert_eq!(IdentityOf::<Test>::contains_key(owner), *has_identity);
					if let Some(secondary) = maybe_secondary {
						let expected_info = UsernameInformation {
							owner: owner.clone(),
							provider: Provider::Governance,
						};
						assert_eq!(UsernameInfoOf::<Test>::get(secondary), Some(expected_info));
					}
				}

				let pending_count = PendingAcceptance::<Test>::iter().count();
				assert_eq!(pending_count, 5);
				for (username, owner, since) in pending.iter() {
					let expected_pending = (owner.clone(), *since, Provider::Governance);
					assert_eq!(PendingAcceptance::<Test>::get(username), Some(expected_pending));
				}

				for id in identity_only.iter() {
					let expected_reg = registration(true);
					assert_eq!(IdentityOf::<Test>::get(id), Some(expected_reg));
					assert!(!UsernameOf::<Test>::contains_key(id));
				}

				assert_eq!(v1::AccountOfUsername::<Test>::iter().count(), 0);
				assert_eq!(v1::PendingUsernames::<Test>::iter().count(), 0);
				assert_eq!(v1::UsernameAuthorities::<Test>::iter().count(), 0);
			});
		}
	}
}
