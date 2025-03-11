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

//! Nomination pools data migrator module.

use super::nom_pools_alias as alias;
use crate::{types::*, *};
use alias::{RewardPool, SubPools};
use pallet_nomination_pools::{BondedPoolInner, ClaimPermission, PoolId, PoolMember};
use sp_runtime::Perbill;

/// The stages of the nomination pools pallet migration.
///
/// They advance in a linear fashion.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum NomPoolsStage<AccountId> {
	/// Migrate the storage values.
	StorageValues,
	/// Migrate the `PoolMembers` storage map.
	PoolMembers(Option<AccountId>),
	/// Migrate the `BondedPools` storage map.
	BondedPools(Option<PoolId>),
	/// Migrate the `RewardPools` storage map.
	RewardPools(Option<PoolId>),
	/// Migrate the `SubPoolsStorage` storage map.
	SubPoolsStorage(Option<PoolId>),
	/// Migrate the `Metadata` storage map.
	Metadata(Option<PoolId>),
	/// Migrate the `ReversePoolIdLookup` storage map.
	ReversePoolIdLookup(Option<AccountId>),
	/// Migrate the `ClaimPermissions` storage map.
	ClaimPermissions(Option<AccountId>),
	/// All done.
	Finished,
}

/// All the `StorageValues` from the nominations pools pallet.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct NomPoolsStorageValues<Balance> {
	pub total_value_locked: Balance,
	pub min_join_bond: Balance,
	pub min_create_bond: Balance,
	pub max_pools: Option<u32>,
	pub max_pool_members: Option<u32>,
	pub max_pool_members_per_pool: Option<u32>,
	pub global_max_commission: Option<Perbill>,
	pub last_pool_id: u32,
}

/// A message from RC to AH to migrate some nomination pools data.
#[derive(
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	RuntimeDebugNoBound,
	CloneNoBound,
	PartialEqNoBound,
	EqNoBound,
)]
#[codec(mel_bound(T: Config))]
#[scale_info(skip_type_params(T))]
pub enum RcNomPoolsMessage<T: pallet_nomination_pools::Config> {
	/// All `StorageValues` that can be migrated at once.
	StorageValues { values: NomPoolsStorageValuesOf<T> },
	/// Entry of the `PoolMembers` map.
	PoolMembers { member: (T::AccountId, PoolMember<T>) },
	/// Entry of the `BondedPools` map.
	BondedPools { pool: (PoolId, BondedPoolInner<T>) },
	/// Entry of the `RewardPools` map.
	RewardPools { rewards: (PoolId, RewardPool<T>) },
	/// Entry of the `SubPoolsStorage` map.
	SubPoolsStorage { sub_pools: (PoolId, SubPools<T>) },
	/// Entry of the `Metadata` map.
	Metadata { meta: (PoolId, BoundedVec<u8, T::MaxMetadataLen>) },
	/// Entry of the `ReversePoolIdLookup` map.
	// TODO check if inserting None into an option map is the same as deleting the key
	ReversePoolIdLookup { lookups: (T::AccountId, PoolId) },
	/// Entry of the `ClaimPermissions` map.
	ClaimPermissions { perms: (T::AccountId, ClaimPermission) },
}

/// Migrate the nomination pools pallet.
pub struct NomPoolsMigrator<T> {
	_phantom: PhantomData<T>,
}

impl<T: Config> PalletMigration for NomPoolsMigrator<T> {
	type Key = NomPoolsStage<T::AccountId>;
	type Error = Error<T>;

	fn migrate_many(
		current_key: Option<Self::Key>,
		weight_counter: &mut WeightMeter,
	) -> Result<Option<Self::Key>, Self::Error> {
		let mut inner_key = current_key.unwrap_or(NomPoolsStage::StorageValues);
		let mut messages = Vec::new();

		loop {
			if weight_counter
				.try_consume(<T as frame_system::Config>::DbWeight::get().reads_writes(1, 1))
				.is_err()
			{
				if messages.is_empty() {
					return Err(Error::OutOfWeight);
				} else {
					break;
				}
			}
			if messages.len() > 10_000 {
				log::warn!("Weight allowed very big batch, stopping");
				break;
			}

			inner_key = match inner_key {
				NomPoolsStage::StorageValues => {
					let values = Self::take_values();
					messages.push(RcNomPoolsMessage::StorageValues { values });
					NomPoolsStage::<T::AccountId>::PoolMembers(None)
				},
				// Bunch of copy & paste code
				NomPoolsStage::PoolMembers(pool_iter) => {
					let mut new_pool_iter = match pool_iter.clone() {
						Some(pool_iter) => pallet_nomination_pools::PoolMembers::<T>::iter_from(
							pallet_nomination_pools::PoolMembers::<T>::hashed_key_for(pool_iter),
						),
						None => pallet_nomination_pools::PoolMembers::<T>::iter(),
					};

					match new_pool_iter.next() {
						Some((key, member)) => {
							pallet_nomination_pools::PoolMembers::<T>::remove(&key);
							messages.push(RcNomPoolsMessage::PoolMembers {
								member: (key.clone(), member),
							});
							NomPoolsStage::PoolMembers(Some(key))
						},
						None => NomPoolsStage::BondedPools(None),
					}
				},
				NomPoolsStage::BondedPools(pool_iter) => {
					let mut new_pool_iter = match pool_iter {
						Some(pool_iter) => pallet_nomination_pools::BondedPools::<T>::iter_from(
							pallet_nomination_pools::BondedPools::<T>::hashed_key_for(pool_iter),
						),
						None => pallet_nomination_pools::BondedPools::<T>::iter(),
					};

					match new_pool_iter.next() {
						Some((key, pool)) => {
							pallet_nomination_pools::BondedPools::<T>::remove(key);
							messages.push(RcNomPoolsMessage::BondedPools { pool: (key, pool) });
							NomPoolsStage::BondedPools(Some(key))
						},
						None => NomPoolsStage::RewardPools(None),
					}
				},
				NomPoolsStage::RewardPools(pool_iter) => {
					let mut new_pool_iter = match pool_iter {
						Some(pool_iter) => alias::RewardPools::<T>::iter_from(
							alias::RewardPools::<T>::hashed_key_for(pool_iter),
						),
						None => alias::RewardPools::<T>::iter(),
					};

					match new_pool_iter.next() {
						Some((key, rewards)) => {
							alias::RewardPools::<T>::remove(key);
							messages
								.push(RcNomPoolsMessage::RewardPools { rewards: (key, rewards) });
							NomPoolsStage::RewardPools(Some(key))
						},
						None => NomPoolsStage::SubPoolsStorage(None),
					}
				},
				NomPoolsStage::SubPoolsStorage(pool_iter) => {
					let mut new_pool_iter = match pool_iter {
						Some(pool_iter) => alias::SubPoolsStorage::<T>::iter_from(
							alias::SubPoolsStorage::<T>::hashed_key_for(pool_iter),
						),
						None => alias::SubPoolsStorage::<T>::iter(),
					};

					match new_pool_iter.next() {
						Some((key, sub_pools)) => {
							alias::SubPoolsStorage::<T>::remove(key);
							messages.push(RcNomPoolsMessage::SubPoolsStorage {
								sub_pools: (key, sub_pools),
							});
							NomPoolsStage::SubPoolsStorage(Some(key))
						},
						None => NomPoolsStage::Metadata(None),
					}
				},
				NomPoolsStage::Metadata(pool_iter) => {
					let mut new_pool_iter = match pool_iter {
						Some(pool_iter) => pallet_nomination_pools::Metadata::<T>::iter_from(
							pallet_nomination_pools::Metadata::<T>::hashed_key_for(pool_iter),
						),
						None => pallet_nomination_pools::Metadata::<T>::iter(),
					};

					match new_pool_iter.next() {
						Some((key, meta)) => {
							pallet_nomination_pools::Metadata::<T>::remove(key);
							messages.push(RcNomPoolsMessage::Metadata { meta: (key, meta) });
							NomPoolsStage::Metadata(Some(key))
						},
						None => NomPoolsStage::ReversePoolIdLookup(None),
					}
				},
				NomPoolsStage::ReversePoolIdLookup(pool_iter) => {
					let mut new_pool_iter = match pool_iter.clone() {
						Some(pool_iter) =>
							pallet_nomination_pools::ReversePoolIdLookup::<T>::iter_from(
								pallet_nomination_pools::ReversePoolIdLookup::<T>::hashed_key_for(
									pool_iter,
								),
							),
						None => pallet_nomination_pools::ReversePoolIdLookup::<T>::iter(),
					};

					match new_pool_iter.next() {
						Some((key, lookup)) => {
							pallet_nomination_pools::ReversePoolIdLookup::<T>::remove(&key);
							messages.push(RcNomPoolsMessage::ReversePoolIdLookup {
								lookups: (key.clone(), lookup),
							});
							NomPoolsStage::ReversePoolIdLookup(Some(key))
						},
						None => NomPoolsStage::ClaimPermissions(None),
					}
				},
				NomPoolsStage::ClaimPermissions(pool_iter) => {
					let mut new_pool_iter = match pool_iter.clone() {
						Some(pool_iter) =>
							pallet_nomination_pools::ClaimPermissions::<T>::iter_from(
								pallet_nomination_pools::ClaimPermissions::<T>::hashed_key_for(
									pool_iter,
								),
							),
						None => pallet_nomination_pools::ClaimPermissions::<T>::iter(),
					};

					match new_pool_iter.next() {
						Some((key, perm)) => {
							pallet_nomination_pools::ClaimPermissions::<T>::remove(&key);
							messages.push(RcNomPoolsMessage::ClaimPermissions {
								perms: (key.clone(), perm),
							});
							NomPoolsStage::ClaimPermissions(Some(key))
						},
						None => NomPoolsStage::Finished,
					}
				},
				NomPoolsStage::Finished => {
					break;
				},
			};
		}

		if !messages.is_empty() {
			Pallet::<T>::send_chunked_xcm(
				messages,
				|messages| types::AhMigratorCall::<T>::ReceiveNomPoolsMessages { messages },
				|_| Weight::from_all(1), // TODO
			)?;
		}

		if inner_key == NomPoolsStage::Finished {
			Ok(None)
		} else {
			Ok(Some(inner_key))
		}
	}
}

pub type NomPoolsStorageValuesOf<T> = NomPoolsStorageValues<pallet_nomination_pools::BalanceOf<T>>;

impl<T: pallet_nomination_pools::Config> NomPoolsMigrator<T> {
	/// Return and remove all `StorageValues` from the nomination pools pallet.
	///
	/// Called by the relay chain.
	fn take_values() -> NomPoolsStorageValuesOf<T> {
		use pallet_nomination_pools::*;

		NomPoolsStorageValues {
			total_value_locked: TotalValueLocked::<T>::take(),
			min_join_bond: MinJoinBond::<T>::take(),
			min_create_bond: MinCreateBond::<T>::take(),
			max_pools: MaxPools::<T>::take(),
			max_pool_members: MaxPoolMembers::<T>::take(),
			max_pool_members_per_pool: MaxPoolMembersPerPool::<T>::take(),
			global_max_commission: GlobalMaxCommission::<T>::take(),
			last_pool_id: LastPoolId::<T>::take(),
		}
	}

	/// Put all `StorageValues` into storage.
	///
	/// Called by Asset Hub after receiving the values.
	pub fn put_values(values: NomPoolsStorageValuesOf<T>) {
		use pallet_nomination_pools::*;

		TotalValueLocked::<T>::put(values.total_value_locked);
		MinJoinBond::<T>::put(values.min_join_bond);
		MinCreateBond::<T>::put(values.min_create_bond);
		MaxPools::<T>::set(values.max_pools);
		MaxPoolMembers::<T>::set(values.max_pool_members);
		MaxPoolMembersPerPool::<T>::set(values.max_pool_members_per_pool);
		GlobalMaxCommission::<T>::set(values.global_max_commission);
		LastPoolId::<T>::put(values.last_pool_id);
	}
}
