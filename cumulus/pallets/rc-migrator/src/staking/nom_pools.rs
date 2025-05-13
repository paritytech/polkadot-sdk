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
use frame_support::traits::{ConstU32, Get};
use pallet_nomination_pools::{BondedPoolInner, ClaimPermission, PoolId, PoolMember};
use sp_runtime::{Perbill, Saturating};

/// The stages of the nomination pools pallet migration.
///
/// They advance in a linear fashion.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
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
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
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
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
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
		let mut messages = XcmBatchAndMeter::new_from_config::<T>();

		loop {
			if weight_counter.try_consume(T::DbWeight::get().reads_writes(1, 1)).is_err() ||
				weight_counter.try_consume(messages.consume_weight()).is_err()
			{
				log::info!("RC weight limit reached at batch length {}, stopping", messages.len());
				if messages.is_empty() {
					return Err(Error::OutOfWeight);
				} else {
					break;
				}
			}
			if T::MaxAhWeight::get()
				.any_lt(T::AhWeightInfo::receive_nom_pools_messages((messages.len() + 1) as u32))
			{
				log::info!("AH weight limit reached at batch length {}, stopping", messages.len());
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
			Pallet::<T>::send_chunked_xcm_and_track(
				messages,
				|messages| types::AhMigratorCall::<T>::ReceiveNomPoolsMessages { messages },
				|len| T::AhWeightInfo::receive_nom_pools_messages(len),
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

#[cfg(feature = "std")]
pub mod tests {
	use super::*;
	use pallet_nomination_pools::{
		CommissionChangeRate, CommissionClaimPermission, PoolRoles, PoolState,
	};
	pub use sp_runtime::traits::{One, Zero};
	use sp_staking::EraIndex;
	use sp_std::{collections::btree_map::BTreeMap, fmt::Debug};

	#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, DebugNoBound, PartialEq, Clone)]
	pub struct GenericCommission<AccountId: Debug, BlockNumber: Debug> {
		pub current: Option<(Perbill, AccountId)>,
		pub max: Option<Perbill>,
		pub change_rate: Option<CommissionChangeRate<BlockNumber>>,
		pub throttle_from: Option<BlockNumber>,
		pub claim_permission: Option<CommissionClaimPermission<AccountId>>,
	}

	#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, DebugNoBound, PartialEq, Clone)]
	pub struct GenericBondedPoolInner<Balance: Debug, AccountId: Debug, BlockNumber: Debug> {
		pub commission: GenericCommission<AccountId, BlockNumber>,
		pub member_counter: u32,
		pub points: Balance,
		pub roles: PoolRoles<AccountId>,
		pub state: PoolState,
	}

	#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	pub struct GenericPoolMember<Balance, RewardCounter> {
		pub pool_id: PoolId,
		pub points: Balance,
		pub last_recorded_reward_counter: RewardCounter,
		pub unbonding_eras: BTreeMap<EraIndex, Balance>,
	}

	#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	pub struct GenericRewardPool<Balance, RewardCounter> {
		pub last_recorded_reward_counter: RewardCounter,
		pub last_recorded_total_payouts: Balance,
		pub total_rewards_claimed: Balance,
		pub total_commission_pending: Balance,
		pub total_commission_claimed: Balance,
	}

	#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	pub struct GenericUnbondPool<Balance> {
		pub points: Balance,
		pub balance: Balance,
	}

	#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	pub struct GenericSubPools<Balance> {
		pub no_era: GenericUnbondPool<Balance>,
		pub with_era: BTreeMap<EraIndex, GenericUnbondPool<Balance>>,
	}

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
	pub enum GenericNomPoolsMessage<
		Balance: Debug + Clone + PartialEq,
		RewardCounter: Debug + Clone + PartialEq,
		AccountId: Debug + Clone + PartialEq,
		BlockNumber: Debug + Clone + PartialEq,
	> {
		StorageValues { values: NomPoolsStorageValues<Balance> },
		PoolMembers { member: (AccountId, GenericPoolMember<Balance, RewardCounter>) },
		BondedPools { pool: (PoolId, GenericBondedPoolInner<Balance, AccountId, BlockNumber>) },
		RewardPools { rewards: (PoolId, GenericRewardPool<Balance, RewardCounter>) },
		SubPoolsStorage { sub_pools: (PoolId, GenericSubPools<Balance>) },
		Metadata { meta: (PoolId, BoundedVec<u8, ConstU32<256>>) },
		ReversePoolIdLookup { lookups: (AccountId, PoolId) },
		ClaimPermissions { perms: (AccountId, ClaimPermission) },
	}
}

pub type BalanceOf<T> = <<T as pallet_nomination_pools::Config>::Currency as frame_support::traits::fungible::Inspect<<T as frame_system::Config>::AccountId>>::Balance;

#[cfg(feature = "std")]
impl<T: Config> crate::types::RcMigrationCheck for NomPoolsMigrator<T> {
	type RcPrePayload = Vec<
		tests::GenericNomPoolsMessage<
			BalanceOf<T>,
			T::RewardCounter,
			<T as frame_system::Config>::AccountId,
			BlockNumberFor<T>,
		>,
	>;

	fn pre_check() -> Self::RcPrePayload {
		let mut messages = Vec::new();

		// Collect storage values
		let values = NomPoolsStorageValues {
			total_value_locked: pallet_nomination_pools::TotalValueLocked::<T>::get(),
			min_join_bond: pallet_nomination_pools::MinJoinBond::<T>::get(),
			min_create_bond: pallet_nomination_pools::MinCreateBond::<T>::get(),
			max_pools: pallet_nomination_pools::MaxPools::<T>::get(),
			max_pool_members: pallet_nomination_pools::MaxPoolMembers::<T>::get(),
			max_pool_members_per_pool: pallet_nomination_pools::MaxPoolMembersPerPool::<T>::get(),
			global_max_commission: pallet_nomination_pools::GlobalMaxCommission::<T>::get(),
			last_pool_id: pallet_nomination_pools::LastPoolId::<T>::get(),
		};
		messages.push(tests::GenericNomPoolsMessage::StorageValues { values });

		// Collect pool members
		for (who, member) in pallet_nomination_pools::PoolMembers::<T>::iter() {
			let generic_member = tests::GenericPoolMember {
				pool_id: member.pool_id,
				points: member.points,
				last_recorded_reward_counter: member.last_recorded_reward_counter,
				unbonding_eras: member.unbonding_eras.into_inner(),
			};
			messages
				.push(tests::GenericNomPoolsMessage::PoolMembers { member: (who, generic_member) });
		}

		// Collect bonded pools
		for (pool_id, mut pool) in pallet_nomination_pools::BondedPools::<T>::iter() {
			if let Some(ref mut change_rate) = pool.commission.change_rate.as_mut() {
				#[cfg(not(feature = "ahm-westend"))]
				{
					change_rate.min_delay = change_rate.min_delay / 2u32.into();
				}
				change_rate.min_delay = change_rate.min_delay.saturating_add(tests::One::one());
			}
			let generic_pool = tests::GenericBondedPoolInner {
				commission: tests::GenericCommission {
					current: pool.commission.current,
					max: pool.commission.max,
					change_rate: pool.commission.change_rate,
					throttle_from: None, // None to avoid discrepancies during the AH postcheck
					claim_permission: pool.commission.claim_permission,
				},
				member_counter: pool.member_counter,
				points: pool.points,
				roles: pool.roles,
				state: pool.state,
			};
			messages
				.push(tests::GenericNomPoolsMessage::BondedPools { pool: (pool_id, generic_pool) });
		}

		// Collect reward pools
		for (pool_id, rewards) in alias::RewardPools::<T>::iter() {
			let generic_rewards = tests::GenericRewardPool {
				last_recorded_reward_counter: rewards.last_recorded_reward_counter,
				last_recorded_total_payouts: rewards.last_recorded_total_payouts,
				total_rewards_claimed: rewards.total_rewards_claimed,
				total_commission_pending: rewards.total_commission_pending,
				total_commission_claimed: rewards.total_commission_claimed,
			};
			messages.push(tests::GenericNomPoolsMessage::RewardPools {
				rewards: (pool_id, generic_rewards),
			});
		}

		// Collect sub pools storage
		for (pool_id, sub_pools) in alias::SubPoolsStorage::<T>::iter() {
			let generic_sub_pools = tests::GenericSubPools {
				no_era: tests::GenericUnbondPool {
					points: sub_pools.no_era.points,
					balance: sub_pools.no_era.balance,
				},
				with_era: sub_pools
					.with_era
					.into_iter()
					.map(|(era, pool)| {
						(
							era,
							tests::GenericUnbondPool { points: pool.points, balance: pool.balance },
						)
					})
					.collect(),
			};
			messages.push(tests::GenericNomPoolsMessage::SubPoolsStorage {
				sub_pools: (pool_id, generic_sub_pools),
			});
		}

		// Collect metadata
		for (pool_id, meta) in pallet_nomination_pools::Metadata::<T>::iter() {
			let meta_inner = meta.into_inner();
			let meta_converted = BoundedVec::<u8, ConstU32<256>>::try_from(meta_inner)
				.expect("metadata length within bounds");
			messages
				.push(tests::GenericNomPoolsMessage::Metadata { meta: (pool_id, meta_converted) });
		}

		// Collect reverse pool id lookup
		for (who, pool_id) in pallet_nomination_pools::ReversePoolIdLookup::<T>::iter() {
			messages.push(tests::GenericNomPoolsMessage::ReversePoolIdLookup {
				lookups: (who, pool_id),
			});
		}

		// Collect claim permissions
		for (who, perms) in pallet_nomination_pools::ClaimPermissions::<T>::iter() {
			messages.push(tests::GenericNomPoolsMessage::ClaimPermissions { perms: (who, perms) });
		}

		messages
	}

	fn post_check(_: Self::RcPrePayload) {
		assert_eq!(
			pallet_nomination_pools::TotalValueLocked::<T>::get(),
			tests::Zero::zero(),
			"Assert storage 'NominationPools::TotalValueLocked::rc_post::empty'"
		);
		assert_eq!(
			pallet_nomination_pools::MinJoinBond::<T>::get(),
			tests::Zero::zero(),
			"Assert storage 'NominationPools::MinJoinBond::rc_post::empty'"
		);
		assert_eq!(
			pallet_nomination_pools::MinCreateBond::<T>::get(),
			tests::Zero::zero(),
			"Assert storage 'NominationPools::MinCreateBond::rc_post::empty'"
		);
		assert!(
			pallet_nomination_pools::MaxPools::<T>::get().is_none(),
			"Assert storage 'NominationPools::MaxPools::rc_post::empty'"
		);
		assert!(
			pallet_nomination_pools::MaxPoolMembers::<T>::get().is_none(),
			"Assert storage 'NominationPools::MaxPoolMembers::rc_post::empty'"
		);
		assert!(
			pallet_nomination_pools::MaxPoolMembersPerPool::<T>::get().is_none(),
			"Assert storage 'NominationPools::MaxPoolMembersPerPool::rc_post::empty'"
		);
		assert!(
			pallet_nomination_pools::GlobalMaxCommission::<T>::get().is_none(),
			"Assert storage 'NominationPools::GlobalMaxCommission::rc_post::empty'"
		);
		assert_eq!(
			pallet_nomination_pools::LastPoolId::<T>::get(),
			0,
			"Assert storage 'NominationPools::LastPoolId::rc_post::empty'"
		);
		assert!(
			pallet_nomination_pools::PoolMembers::<T>::iter().next().is_none(),
			"Assert storage 'NominationPools::PoolMembers::rc_post::empty'"
		);
		assert!(
			pallet_nomination_pools::BondedPools::<T>::iter().next().is_none(),
			"Assert storage 'NominationPools::BondedPools::rc_post::empty'"
		);
		assert!(
			alias::RewardPools::<T>::iter().next().is_none(),
			"Assert storage 'NominationPools::RewardPools::rc_post::empty'"
		);
		assert!(
			alias::SubPoolsStorage::<T>::iter().next().is_none(),
			"Assert storage 'NominationPools::SubPoolsStorage::rc_post::empty'"
		);
		assert!(
			pallet_nomination_pools::Metadata::<T>::iter().next().is_none(),
			"Assert storage 'NominationPools::Metadata::rc_post::empty'"
		);
		assert!(
			pallet_nomination_pools::ReversePoolIdLookup::<T>::iter().next().is_none(),
			"Assert storage 'NominationPools::ReversePoolIdLookup::rc_post::empty'"
		);
		assert!(
			pallet_nomination_pools::ClaimPermissions::<T>::iter().next().is_none(),
			"Assert storage 'NominationPools::ClaimPermissions::rc_post::empty'"
		);
	}
}
