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

//! Storage migrations for the Identity pallet to v3.

use super::*;
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	traits::{Currency, ReservableCurrency},
	weights::WeightMeter,
};
#[cfg(any(test, feature = "try-runtime"))]
use sp_runtime::ArithmeticError;

type HashedKey = BoundedVec<u8, ConstU32<256>>;

/// Progressive states of a migration. The migration starts with the first variant and ends with
/// the last.
#[derive(Decode, Encode, MaxEncodedLen, Eq, PartialEq)]
pub enum MigrationState<U> {
	Username(U),
	FinishedUsernames,
	PendingUsername(U),
	FinishedPendingUsernames,
	Identity(HashedKey),
	Finished,
}

// A step in the migration process.
type MigrationStepOf<T> = MigrationState<Username<T>>;

pub struct LazyMigrationV2ToV3<T: Config, C>(PhantomData<(T, C)>);
impl<T: Config, C> SteppedMigration for LazyMigrationV2ToV3<T, C>
where
	C: Currency<T::AccountId, Balance = BalanceOf<T>> + ReservableCurrency<T::AccountId>,
{
	type Cursor = MigrationStepOf<T>;
	type Identifier = MigrationId<15>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 2, version_to: 3 }
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
			// Worst case weight for `username_step`.
			None => T::WeightInfo::migration_v3_username_step(),
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
				None => T::WeightInfo::migration_v3_username_step(),
			};
			if !meter.can_consume(required_weight) {
				break;
			}

			let next = match &cursor {
				// At first, migrate deposits for usernames
				None => Self::username_step(None),
				// Keep migrating deposits for usernames.
				Some(MigrationState::Username(maybe_last_username)) =>
					Self::username_step(Some(maybe_last_username)),
				// After the deposit for the last username was migrated, start migrating the deposit
				// for all identities, subidentities, and pending judgements.
				Some(MigrationState::FinishedUsernames) => Self::pending_username_step(None),
				Some(MigrationState::PendingUsername(maybe_last_username)) =>
					Self::pending_username_step(Some(maybe_last_username)),
				Some(MigrationState::FinishedPendingUsernames) => Self::identity_step(None),
				// Keep migrating deposits for identities.
				Some(MigrationState::Identity(key)) => Self::identity_step(Some(key.clone())),
				// After migrating the deposits, we're all done.
				Some(MigrationState::Finished) => {
					StorageVersion::new(Self::id().version_to as u16).put::<Pallet<T>>();
					return Ok(None);
				},
			};

			cursor = Some(next);
			meter.consume(required_weight);
		}

		Ok(cursor)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		Self::do_pre_upgrade()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
		Self::do_post_upgrade(state)
	}
}

#[cfg(any(test, feature = "try-runtime"))]
#[derive(Encode, Decode)]
struct TryRuntimeState<T: Config> {
	authorities: BTreeMap<T::AccountId, BalanceOf<T>>,
	identities: BTreeMap<T::AccountId, BalanceOf<T>>,
	pending_judgements: BTreeMap<T::AccountId, BalanceOf<T>>,
	subs: BTreeMap<T::AccountId, BalanceOf<T>>,
}

#[cfg(any(test, feature = "try-runtime"))]
impl<T: Config, C> LazyMigrationV2ToV3<T, C> {
	fn do_pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		let mut authorities = BTreeMap::<T::AccountId, BalanceOf<T>>::new();
		for (username, (_, _, provider)) in PendingUsernames::<T>::iter() {
			let Provider::AuthorityDeposit(deposit) = provider else {
				continue;
			};
			let Ok(suffix) = Pallet::<T>::validate_username(&username.clone().into_inner()) else {
				continue;
			};

			let Some(AuthorityProperties::<T::AccountId> { account_id: who, .. }) =
				AuthorityOf::<T>::get(suffix)
			else {
				continue;
			};
			let deposit = authorities
				.get(&who)
				.unwrap_or(&0u16.into())
				.checked_add(&deposit)
				.ok_or(ArithmeticError::Overflow)?;

			authorities.insert(who, deposit);
		}
		for (username, info) in UsernameInfoOf::<T>::iter() {
			let Provider::AuthorityDeposit(deposit) = info.provider else {
				continue;
			};
			let Ok(suffix) = Pallet::<T>::validate_username(&username.clone().into_inner()) else {
				continue;
			};
			let Some(AuthorityProperties::<T::AccountId> { account_id: who, .. }) =
				AuthorityOf::<T>::get(suffix)
			else {
				continue;
			};
			let deposit = authorities
				.get(&who)
				.unwrap_or(&0u16.into())
				.checked_add(&deposit)
				.ok_or(ArithmeticError::Overflow)?;

			authorities.insert(who, deposit);
		}

		let mut identities = BTreeMap::new();
		let mut pending_judgements = BTreeMap::new();
		let mut subs = BTreeMap::new();
		for (who, Registration { deposit, judgements, .. }) in IdentityOf::<T>::iter() {
			identities.insert(who.clone(), deposit);

			let (deposit, _) = SubsOf::<T>::get(&who);
			subs.insert(who.clone(), deposit);

			let deposit = judgements.iter().fold::<BalanceOf<T>, _>(
				0u16.into(),
				|balance, (_, judgement)| {
					let Judgement::<BalanceOf<T>>::FeePaid(fee) = judgement else { return balance };
					balance.saturating_add(*fee)
				},
			);
			pending_judgements.insert(who, deposit);
		}

		Ok(TryRuntimeState::<T> { identities, authorities, subs, pending_judgements }.encode())
	}

	fn do_post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
		let TryRuntimeState { identities, pending_judgements, subs, authorities } =
			TryRuntimeState::<T>::decode(&mut &state[..])
				.expect("Failed to decode the previous storage state");

		for (who, deposit) in authorities {
			assert_eq!(T::Balances::balance_on_hold(&HoldReason::Username.into(), &who), deposit);
		}
		for (who, deposit) in identities {
			assert_eq!(T::Balances::balance_on_hold(&HoldReason::Identity.into(), &who), deposit);
		}
		for (who, deposit) in pending_judgements {
			assert_eq!(T::Balances::balance_on_hold(&HoldReason::Judgement.into(), &who), deposit);
		}
		for (who, deposit) in subs {
			assert_eq!(
				T::Balances::balance_on_hold(&HoldReason::SubIdentities.into(), &who),
				deposit
			);
		}

		Ok(())
	}
}

impl<T: Config, C> LazyMigrationV2ToV3<T, C>
where
	C: Currency<T::AccountId, Balance = BalanceOf<T>> + ReservableCurrency<T::AccountId>,
{
	pub(crate) fn required_weight(step: &MigrationStepOf<T>) -> Weight {
		match step {
			MigrationState::Username(_) => T::WeightInfo::migration_v3_username_step(),
			MigrationState::FinishedUsernames | MigrationState::PendingUsername(_) =>
				T::WeightInfo::migration_v3_pending_username_step(),
			MigrationState::FinishedPendingUsernames | MigrationState::Identity(_) =>
				T::WeightInfo::migration_v3_identity_step(),
			MigrationState::Finished => Weight::zero(),
		}
	}

	// Migrate deposits for usernames.
	pub(crate) fn username_step(maybe_last_key: Option<&Username<T>>) -> MigrationStepOf<T> {
		let mut iter = if let Some(last_key) = maybe_last_key {
			UsernameInfoOf::<T>::iter_from(UsernameInfoOf::<T>::hashed_key_for(last_key))
		} else {
			UsernameInfoOf::<T>::iter()
		};

		if let Some((username, info)) = iter.next() {
			let _ = Self::migrate_authority_deposit(&username, info.provider);
			MigrationState::Username(username)
		} else {
			MigrationState::FinishedUsernames
		}
	}

	// Migrate deposits for usernames.
	pub(crate) fn pending_username_step(
		maybe_last_key: Option<&Username<T>>,
	) -> MigrationStepOf<T> {
		let mut iter = if let Some(last_key) = maybe_last_key {
			PendingUsernames::<T>::iter_from(PendingUsernames::<T>::hashed_key_for(last_key))
		} else {
			PendingUsernames::<T>::iter()
		};

		if let Some((username, (_, _, provider))) = iter.next() {
			// We only need to migrate the usernames that were provided via an `AuthorityDeposit`.
			let _ = Self::migrate_authority_deposit(&username, provider);
			MigrationState::PendingUsername(username)
		} else {
			MigrationState::FinishedPendingUsernames
		}
	}

	fn migrate_authority_deposit(
		username: &Username<T>,
		provider: ProviderOf<T>,
	) -> DispatchResult {
		// We only need to migrate the usernames that were provided via an `AuthorityDeposit`.
		let Provider::AuthorityDeposit(deposit) = provider else { return Ok(()) };
		let suffix = Pallet::<T>::validate_username(&username.clone().into_inner())?;
		let AuthorityProperties { account_id: authority, .. } =
			AuthorityOf::<T>::get(suffix).ok_or(DispatchError::CannotLookup)?;

		C::unreserve(&authority, deposit);
		T::Balances::hold(&HoldReason::Username.into(), &authority, deposit)
	}

	// Migrate the balances for identities and subs from `IdentityOf`.
	pub(crate) fn identity_step(maybe_last_key: Option<HashedKey>) -> MigrationStepOf<T> {
		if let Some(mut last_key) = IdentityOf::<T>::translate_next(
			maybe_last_key.map(|b| b.to_vec()),
			|account, identity| {
				// Migrate deposit for identity
				let Registration { ref deposit, ref judgements, .. } = identity;
				let _ = C::unreserve(&account, *deposit);
				let _ = T::Balances::hold(&HoldReason::Identity.into(), &account, *deposit);

				// Migrate deposit for subs (if any)
				let (deposit, _) = SubsOf::<T>::get(&account);
				let _ = C::unreserve(&account, deposit);
				let _ = T::Balances::hold(&HoldReason::SubIdentities.into(), &account, deposit);

				// Migrate deposit for judgements (if any)
				let deposit =
					judgements.iter().fold::<BalanceOf<T>, _>(0u16.into(), |deposit, (_, j)| {
						if let Judgement::FeePaid(balance) = j {
							return deposit.saturating_add(*balance);
						}

						deposit
					});
				let _ = C::unreserve(&account, deposit);
				let _ = T::Balances::hold(&HoldReason::Judgement.into(), &account, deposit);

				Some(identity)
			},
		) {
			last_key.truncate(HashedKey::bound());
			MigrationState::Identity(
				HashedKey::try_from(last_key)
					.expect("truncated to bound so the conversion must succeed; qed"),
			)
		} else {
			MigrationState::Finished
		}
	}
}

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking {
	use super::*;
	use alloc::vec;
	use frame_benchmarking::{account, BenchmarkError};

	impl<T: Config, C> LazyMigrationV2ToV3<T, C>
	where
		C: Currency<T::AccountId, Balance = BalanceOf<T>> + ReservableCurrency<T::AccountId>,
	{
		pub(crate) fn setup_benchmark_env_for_username_deposit_migration(
			is_pending: bool,
		) -> Result<(T::AccountId, BalanceOf<T>), DispatchError> {
			let suffix: Suffix<T> = b"bench".to_vec().try_into().unwrap();
			let authority: T::AccountId = account("authority", 0, 0);
			let account_id: T::AccountId = account("account", 1, 0);

			let prop: AuthorityProperties<T::AccountId> =
				AuthorityProperties { account_id: authority.clone(), allocation: 0 };
			AuthorityOf::<T>::insert(&suffix, prop);

			let username: Username<T> = b"account.bench".to_vec().try_into().unwrap();

			if is_pending {
				let since: BlockNumberFor<T> = 0u32.into();
				PendingUsernames::<T>::insert(
					&username,
					(account_id, since, Provider::new_with_deposit(T::UsernameDeposit::get())),
				);
			} else {
				UsernameInfoOf::<T>::insert(
					&username,
					UsernameInformation {
						owner: account_id.clone(),
						provider: Provider::new_with_deposit(T::UsernameDeposit::get()),
					},
				);
			}

			let deposit = T::UsernameDeposit::get();
			C::make_free_balance_be(
				&authority,
				deposit.saturating_add(T::Balances::minimum_balance()),
			);
			C::reserve(&authority, deposit)?;

			Ok((authority, deposit))
		}

		pub(crate) fn setup_benchmark_env_for_identity_deposit_migration(
		) -> Result<(T::AccountId, BalanceOf<T>, BalanceOf<T>, BalanceOf<T>), BenchmarkError> {
			let account_id: T::AccountId = frame_benchmarking::account("account", 1, 0);
			let sub_account_id: T::AccountId = frame_benchmarking::account("sub", 1, 0);

			let info = T::IdentityInformation::create_identity_info();
			let identity_deposit = Pallet::<T>::calculate_identity_deposit(&info);
			C::make_free_balance_be(
				&account_id,
				identity_deposit.saturating_add(T::Balances::minimum_balance()),
			);
			C::reserve(&account_id, identity_deposit)?;

			let judgements = BoundedVec::truncate_from(
				(0..T::MaxRegistrars::get())
					.map(|i| (i, Judgement::FeePaid(10u16.into())))
					.collect(),
			);
			let fee: BalanceOf<T> = 10u16.into();
			let judgements_deposit = fee * T::MaxRegistrars::get().into();
			C::make_free_balance_be(
				&account_id,
				judgements_deposit.saturating_add(T::Balances::minimum_balance()),
			);
			C::reserve(&account_id, judgements_deposit)?;

			let registration: Registration<
				BalanceOf<T>,
				<T as Config>::MaxRegistrars,
				<T as Config>::IdentityInformation,
			> = Registration { judgements, deposit: identity_deposit, info };
			IdentityOf::<T>::insert(&account_id, registration);

			let subs = vec![sub_account_id; T::MaxSubAccounts::get() as usize];
			let subs_deposit = T::SubAccountDeposit::get() * T::MaxSubAccounts::get().into();
			C::make_free_balance_be(
				&account_id,
				subs_deposit.saturating_add(T::Balances::minimum_balance()),
			);
			C::reserve(&account_id, subs_deposit)?;

			SubsOf::<T>::insert(&account_id, (subs_deposit, BoundedVec::truncate_from(subs)));

			Ok((account_id, identity_deposit, judgements_deposit, subs_deposit))
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::tests::{new_test_ext, Balances, Test};
	use frame_support::{assert_ok, traits::fungible::Mutate};

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
	fn migrate_to_v3() {
		new_test_ext().execute_with(|| {
			StorageVersion::new(2).put::<Pallet<Test>>();

			// Set up the first authority
			let authority_1 = account_from_u8(111);
			let suffix_1: Suffix<Test> = b"evn".to_vec().try_into().unwrap();
			let prop = AuthorityProperties { account_id: authority_1.clone(), allocation: 0 };
			AuthorityOf::<Test>::insert(&suffix_1, prop);

			// The first authority had up to 50 allocations. Remaining usernames require a deposit.
			Balances::make_free_balance_be(&authority_1, 1000);
			assert_ok!(Balances::reserve(&authority_1, 500));

			// Set up the second authority.
			let authority_2 = account_from_u8(112);
			let suffix_2: Suffix<Test> = b"odd".to_vec().try_into().unwrap();
			let prop = AuthorityProperties { account_id: authority_2.clone(), allocation: 0 };
			AuthorityOf::<Test>::insert(&suffix_2, prop);

			// No allocations. Every username should require a deposit.
			Balances::make_free_balance_be(&authority_2, 501);
			assert_ok!(Balances::reserve(&authority_2, 500));

			// Setup accounts with usernames.
			//
			// If `has_identity` is set, this `owner_account` will have a real identity
			// associated and a non-zero deposit for it.
			for i in 0u8..100u8 {
				let account_id = account_from_u8(i);
				let bare_username = format!("acc{}.", i).as_bytes().to_vec();

				// We always register an .evn-suffixed username on behalf of an user, making it
				// the primary username
				let mut username_1 = bare_username.clone();
				username_1.extend(suffix_1.iter());
				let username_1: Username<Test> = username_1.try_into().unwrap();
				UsernameOf::<Test>::insert(&account_id, &username_1);

				match i {
					// First quarter of registrations are pending, allocated
					0..25 => PendingUsernames::<Test>::insert(
						&username_1,
						(account_id.clone(), 0, Provider::new_with_allocation()),
					),
					25..50 => UsernameInfoOf::<Test>::insert(
						&username_1,
						&UsernameInformation {
							owner: account_id.clone(),
							provider: Provider::new_with_allocation(),
						},
					),
					// Third quarter of registrations are pending, with deposit
					50..75 => PendingUsernames::<Test>::insert(
						&username_1,
						(account_id.clone(), 0, Provider::new_with_deposit(10)),
					),
					_ => UsernameInfoOf::<Test>::insert(
						&username_1,
						&UsernameInformation {
							owner: account_id.clone(),
							provider: Provider::new_with_deposit(10),
						},
					),
				}

				if i % 2 == 0 {
					// Half of even-indexed users also has an identity
					let has_identity = i % 4 == 0;
					let reg = registration(has_identity);
					IdentityOf::<Test>::insert(&account_id, &reg);
					assert_ok!(Balances::mint_into(&account_id, 1 + reg.deposit));
					assert_ok!(Balances::reserve(&account_id, reg.deposit));
				} else {
					// Also, we register an username on behalf of that user with the .odd suffix
					// for odd-indexed accounts.
					let mut username_2 = bare_username.clone();
					username_2.extend(suffix_2.iter());
					let username_2: Username<Test> = username_2.try_into().unwrap();

					UsernameInfoOf::<Test>::insert(
						&username_2,
						&UsernameInformation {
							owner: account_id.clone(),
							provider: Provider::new_with_deposit(10),
						},
					);
					UsernameOf::<Test>::insert(&account_id, &username_2);

					// Half of odd-indexed of these users also has an identity
					let has_identity = i % 3 == 0;
					let reg = registration(has_identity);
					IdentityOf::<Test>::insert(&account_id, &reg);
					assert_ok!(Balances::mint_into(&account_id, 1 + reg.deposit));
					assert_ok!(Balances::reserve(&account_id, reg.deposit));
				}
			}

			// Setup accounts that only hold identities.
			//
			// - Every three accounts, we'll add a couple subs.
			// - Every four accounts, an account would have a pending judgement
			for i in 100u8..110u8 {
				let account_id = account_from_u8(i);
				let mut reg = registration(true);
				Balances::make_free_balance_be(&account_id, 11);

				if i % 3 == 0 {
					let account_sub1 = account_from_u8(i + 100);
					let account_sub2 = account_from_u8(i + 110);

					SubsOf::<Test>::insert(
						&account_id,
						(
							20,
							BoundedVec::truncate_from(vec![
								account_sub1.clone(),
								account_sub2.clone(),
							]),
						),
					);
					assert_ok!(Balances::mint_into(&account_id, 20));
					assert_ok!(Balances::reserve(&account_id, 20));
				}

				if i % 4 == 0 {
					let _ = reg.judgements.try_push((0, Judgement::FeePaid(10)));
					assert_ok!(Balances::mint_into(&account_id, 10));
					assert_ok!(Balances::reserve(&account_id, 10));
				}

				assert_ok!(Balances::reserve(&account_id, 10));
				IdentityOf::<Test>::insert(&account_id, &reg);
			}

			// Pre-upgrade check.
			let state = LazyMigrationV2ToV3::<Test, Balances>::do_pre_upgrade()
				.expect("pre_upgrade is expected to work");

			// Run the actual migration.
			let mut weight_meter = WeightMeter::new();
			let mut cursor = None;
			while let Some(new_cursor) =
				LazyMigrationV2ToV3::<Test, Balances>::step(cursor, &mut weight_meter).unwrap()
			{
				cursor = Some(new_cursor);
			}
			assert_eq!(Pallet::<Test>::on_chain_storage_version(), 3);

			// Post-upgrade check.
			assert_ok!(LazyMigrationV2ToV3::<Test, Balances>::do_post_upgrade(state));
		});
	}
}
