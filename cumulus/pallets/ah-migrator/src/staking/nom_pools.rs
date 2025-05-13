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

use crate::*;
use frame_support::traits::DefensiveSaturating;
use pallet_nomination_pools::BondedPoolInner;
#[cfg(feature = "std")]
use pallet_rc_migrator::staking::nom_pools::tests;
use pallet_rc_migrator::staking::nom_pools::{BalanceOf, NomPoolsMigrator};
use sp_runtime::{
	traits::{CheckedSub, One},
	Saturating,
};

impl<T: Config> Pallet<T> {
	pub fn do_receive_nom_pools_messages(
		messages: Vec<RcNomPoolsMessage<T>>,
	) -> Result<(), Error<T>> {
		let mut good = 0;
		log::info!(target: LOG_TARGET, "Integrating {} NomPoolsMessages", messages.len());
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::NomPools,
			count: messages.len() as u32,
		});

		for message in messages {
			Self::do_receive_nom_pools_message(message);
			good += 1;
		}

		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::NomPools,
			count_good: good as u32,
			count_bad: 0,
		});
		Ok(())
	}

	pub fn do_receive_nom_pools_message(message: RcNomPoolsMessage<T>) {
		use RcNomPoolsMessage::*;
		match message {
			StorageValues { values } => {
				pallet_rc_migrator::staking::nom_pools::NomPoolsMigrator::<T>::put_values(values);
				log::debug!(target: LOG_TARGET, "Integrating NomPoolsStorageValues");
			},
			PoolMembers { member } => {
				debug_assert!(!pallet_nomination_pools::PoolMembers::<T>::contains_key(&member.0));
				log::debug!(target: LOG_TARGET, "Integrating NomPoolsPoolMember: {:?}", &member.0);
				pallet_nomination_pools::PoolMembers::<T>::insert(member.0, member.1);
			},
			BondedPools { pool } => {
				debug_assert!(!pallet_nomination_pools::BondedPools::<T>::contains_key(pool.0));
				log::debug!(target: LOG_TARGET, "Integrating NomPoolsBondedPool: {}", &pool.0);
				pallet_nomination_pools::BondedPools::<T>::insert(
					pool.0,
					Self::rc_to_ah_bonded_pool(pool.1),
				);
			},
			RewardPools { rewards } => {
				log::debug!(target: LOG_TARGET, "Integrating NomPoolsRewardPool: {:?}", &rewards.0);
				// Not sure if it is the best to use the alias here, but it is the easiest...
				pallet_rc_migrator::staking::nom_pools_alias::RewardPools::<T>::insert(
					rewards.0, rewards.1,
				);
			},
			SubPoolsStorage { sub_pools } => {
				log::debug!(target: LOG_TARGET, "Integrating NomPoolsSubPoolsStorage: {:?}", &sub_pools.0);
				pallet_rc_migrator::staking::nom_pools_alias::SubPoolsStorage::<T>::insert(
					sub_pools.0,
					sub_pools.1,
				);
			},
			Metadata { meta } => {
				log::debug!(target: LOG_TARGET, "Integrating NomPoolsMetadata: {:?}", &meta.0);
				pallet_nomination_pools::Metadata::<T>::insert(meta.0, meta.1);
			},
			ReversePoolIdLookup { lookups } => {
				log::debug!(target: LOG_TARGET, "Integrating NomPoolsReversePoolIdLookup: {:?}", &lookups.0);
				pallet_nomination_pools::ReversePoolIdLookup::<T>::insert(lookups.0, lookups.1);
			},
			ClaimPermissions { perms } => {
				log::debug!(target: LOG_TARGET, "Integrating NomPoolsClaimPermissions: {:?}", &perms.0);
				pallet_nomination_pools::ClaimPermissions::<T>::insert(perms.0, perms.1);
			},
		}
	}

	/// Translate a bonded RC pool to an AH one.
	pub fn rc_to_ah_bonded_pool(mut pool: BondedPoolInner<T>) -> BondedPoolInner<T> {
		if let Some(ref mut throttle_from) = pool.commission.throttle_from {
			// Plus one here to be safe for the pool member just in case that the pool operator
			// would like to enact commission rate changes immediately.
			*throttle_from = Self::rc_to_ah_timepoint(*throttle_from).saturating_add(One::one());
		}
		if let Some(ref mut change_rate) = pool.commission.change_rate {
			// We cannot assume how this conversion works, but adding one will ensure that we err on
			// the side of pool-member safety in case of rounding.
			change_rate.min_delay =
				T::RcToAhDelay::convert(change_rate.min_delay).saturating_add(One::one());
		}

		pool
	}

	/// Convert an absolute RC time point to an AH one.
	///
	/// This works by re-anchoring the time point to. For example:
	///
	/// - RC now: 100
	/// - AH now: 75
	/// - RC time point: 50
	/// - Result: 75 - (100 - 50) / 2 = 50
	///
	/// Other example:
	///
	/// - RC now: 100
	/// - AH now: 75
	/// - RC time point: 150
	/// - Result: 75 + (150 - 100) / 2 = 100
	pub fn rc_to_ah_timepoint(rc_timepoint: BlockNumberFor<T>) -> BlockNumberFor<T> {
		let rc_now = <T as crate::Config>::RcBlockNumberProvider::current_block_number();
		let ah_now = frame_system::Pallet::<T>::block_number();

		if let Some(rc_since) = rc_now.checked_sub(&rc_timepoint) {
			ah_now.saturating_sub(T::RcToAhDelay::convert(rc_since)) // TODO rename
		} else {
			ah_now.saturating_add(T::RcToAhDelay::convert(
				rc_timepoint.defensive_saturating_sub(rc_now),
			))
		}
	}
}

#[cfg(feature = "std")]
impl<T: Config> crate::types::AhMigrationCheck for NomPoolsMigrator<T> {
	type RcPrePayload = Vec<
		tests::GenericNomPoolsMessage<
			BalanceOf<T>,
			T::RewardCounter,
			<T as frame_system::Config>::AccountId,
			BlockNumberFor<T>,
		>,
	>;
	type AhPrePayload = ();

	fn pre_check(_: Self::RcPrePayload) -> Self::AhPrePayload {
		assert!(
			pallet_nomination_pools::TotalValueLocked::<T>::get().is_zero(),
			"Assert storage 'NominationPools::TotalValueLocked::ah_pre::empty'"
		);
		assert!(
			pallet_nomination_pools::MinJoinBond::<T>::get().is_zero(),
			"Assert storage 'NominationPools::MinJoinBond::ah_pre::empty'"
		);
		assert!(
			pallet_nomination_pools::MinCreateBond::<T>::get().is_zero(),
			"Assert storage 'NominationPools::MinCreateBond::ah_pre::empty'"
		);
		assert!(
			pallet_nomination_pools::MaxPools::<T>::get().is_none(),
			"Assert storage 'NominationPools::MaxPools::ah_pre::empty'"
		);
		assert!(
			pallet_nomination_pools::MaxPoolMembers::<T>::get().is_none(),
			"Assert storage 'NominationPools::MaxPoolMembers::ah_pre::empty'"
		);
		assert!(
			pallet_nomination_pools::MaxPoolMembersPerPool::<T>::get().is_none(),
			"Assert storage 'NominationPools::MaxPoolMembersPerPool::ah_pre::empty'"
		);
		assert!(
			pallet_nomination_pools::GlobalMaxCommission::<T>::get().is_none(),
			"Assert storage 'NominationPools::GlobalMaxCommission::ah_pre::empty'"
		);
		assert!(
			pallet_nomination_pools::LastPoolId::<T>::get().is_zero(),
			"Assert storage 'NominationPools::LastPoolId::ah_pre::empty'"
		);
		assert!(
			pallet_nomination_pools::PoolMembers::<T>::iter().next().is_none(),
			"Assert storage 'NominationPools::PoolMembers::ah_pre::empty'"
		);
		assert!(
			pallet_nomination_pools::BondedPools::<T>::iter().next().is_none(),
			"Assert storage 'NominationPools::BondedPools::ah_pre::empty'"
		);
		assert!(
			pallet_rc_migrator::staking::nom_pools_alias::RewardPools::<T>::iter()
				.next()
				.is_none(),
			"Assert storage 'NominationPools::RewardPools::ah_pre::empty'"
		);
		assert!(
			pallet_rc_migrator::staking::nom_pools_alias::SubPoolsStorage::<T>::iter()
				.next()
				.is_none(),
			"Assert storage 'NominationPools::SubPoolsStorage::ah_pre::empty'"
		);
		assert!(
			pallet_nomination_pools::Metadata::<T>::iter().next().is_none(),
			"Assert storage 'NominationPools::Metadata::ah_pre::empty'"
		);
		assert!(
			pallet_nomination_pools::ReversePoolIdLookup::<T>::iter().next().is_none(),
			"Assert storage 'NominationPools::ReversePoolIdLookup::ah_pre::empty'"
		);
		assert!(
			pallet_nomination_pools::ClaimPermissions::<T>::iter().next().is_none(),
			"Assert storage 'NominationPools::ClaimPermissions::ah_pre::empty'"
		);
	}

	fn post_check(rc_pre_payload: Self::RcPrePayload, _: Self::AhPrePayload) {
		let mut ah_messages = Vec::new();

		// Collect storage values from AH
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
		ah_messages.push(tests::GenericNomPoolsMessage::StorageValues { values });

		// Collect all other storage items from AH
		for (who, member) in pallet_nomination_pools::PoolMembers::<T>::iter() {
			let generic_member = tests::GenericPoolMember {
				pool_id: member.pool_id,
				points: member.points,
				last_recorded_reward_counter: member.last_recorded_reward_counter,
				unbonding_eras: member.unbonding_eras.into_inner(),
			};
			ah_messages
				.push(tests::GenericNomPoolsMessage::PoolMembers { member: (who, generic_member) });
		}

		for (pool_id, pool) in pallet_nomination_pools::BondedPools::<T>::iter() {
			let generic_pool = tests::GenericBondedPoolInner {
				commission: tests::GenericCommission {
					current: pool.commission.current,
					max: pool.commission.max,
					change_rate: pool.commission.change_rate,
					throttle_from: pool.commission.throttle_from,
					claim_permission: pool.commission.claim_permission,
				},
				member_counter: pool.member_counter,
				points: pool.points,
				roles: pool.roles,
				state: pool.state,
			};
			ah_messages
				.push(tests::GenericNomPoolsMessage::BondedPools { pool: (pool_id, generic_pool) });
		}

		for (pool_id, rewards) in
			pallet_rc_migrator::staking::nom_pools_alias::RewardPools::<T>::iter()
		{
			let generic_rewards = tests::GenericRewardPool {
				last_recorded_reward_counter: rewards.last_recorded_reward_counter,
				last_recorded_total_payouts: rewards.last_recorded_total_payouts,
				total_rewards_claimed: rewards.total_rewards_claimed,
				total_commission_pending: rewards.total_commission_pending,
				total_commission_claimed: rewards.total_commission_claimed,
			};
			ah_messages.push(tests::GenericNomPoolsMessage::RewardPools {
				rewards: (pool_id, generic_rewards),
			});
		}

		for (pool_id, sub_pools) in
			pallet_rc_migrator::staking::nom_pools_alias::SubPoolsStorage::<T>::iter()
		{
			let generic_sub_pools = tests::GenericSubPools {
				no_era: tests::GenericUnbondPool {
					points: sub_pools.no_era.points,
					balance: sub_pools.no_era.balance,
				},
				with_era: sub_pools
					.with_era
					.into_inner()
					.into_iter()
					.map(|(era, pool)| {
						(
							era,
							tests::GenericUnbondPool { points: pool.points, balance: pool.balance },
						)
					})
					.collect(),
			};
			ah_messages.push(tests::GenericNomPoolsMessage::SubPoolsStorage {
				sub_pools: (pool_id, generic_sub_pools),
			});
		}

		for (pool_id, meta) in pallet_nomination_pools::Metadata::<T>::iter() {
			let meta_converted = BoundedVec::<u8, ConstU32<256>>::try_from(meta.into_inner())
				.expect("Metadata length is known to be within bounds; qed");
			ah_messages
				.push(tests::GenericNomPoolsMessage::Metadata { meta: (pool_id, meta_converted) });
		}

		for (who, pool_id) in pallet_nomination_pools::ReversePoolIdLookup::<T>::iter() {
			ah_messages.push(tests::GenericNomPoolsMessage::ReversePoolIdLookup {
				lookups: (who, pool_id),
			});
		}

		for (who, perms) in pallet_nomination_pools::ClaimPermissions::<T>::iter() {
			ah_messages
				.push(tests::GenericNomPoolsMessage::ClaimPermissions { perms: (who, perms) });
		}

		let ah_filtered: Vec<_> = ah_messages
			.into_iter()
			.map(|msg| match msg {
				// If the message is of type BondedPools, we remove the throttle_from value
				// from the commission. This is necessary because in AssetHub, the value of
				// throttle_from is calculated dynamically using block_number(),
				// and it cannot be correctly obtained during the postcheck. By removing
				// this value, we avoid conflicts and ensure that the AH system functions as
				// intended.
				tests::GenericNomPoolsMessage::BondedPools { pool: (id, mut inner) } => {
					inner.commission.throttle_from = None;
					tests::GenericNomPoolsMessage::BondedPools { pool: (id, inner) }
				},
				other => other,
			})
			.collect();

		// Assert storage "NominationPools::TotalValueLocked::ah_post::correct"
		// Assert storage "NominationPools::TotalValueLocked::ah_post::consistent"
		// Assert storage "NominationPools::MinJoinBond::ah_post::correct"
		// Assert storage "NominationPools::MinJoinBond::ah_post::consistent"
		// Assert storage "NominationPools::MinCreateBond::ah_post::correct"
		// Assert storage "NominationPools::MinCreateBond::ah_post::consistent"
		// Assert storage "NominationPools::MaxPools::ah_post::correct"
		// Assert storage "NominationPools::MaxPools::ah_post::consistent"
		// Assert storage "NominationPools::MaxPoolMembers::ah_post::correct"
		// Assert storage "NominationPools::MaxPoolMembers::ah_post::consistent"
		// Assert storage "NominationPools::MaxPoolMembersPerPool::ah_post::correct"
		// Assert storage "NominationPools::MaxPoolMembersPerPool::ah_post::consistent"
		// Assert storage "NominationPools::GlobalMaxCommission::ah_post::correct"
		// Assert storage "NominationPools::GlobalMaxCommission::ah_post::consistent"
		// Assert storage "NominationPools::LastPoolId::ah_post::correct"
		// Assert storage "NominationPools::LastPoolId::ah_post::consistent"
		// Assert storage "NominationPools::PoolMembers::ah_post::correct"
		// Assert storage "NominationPools::PoolMembers::ah_post::consistent"
		// Assert storage "NominationPools::BondedPools::ah_post::correct"
		// Assert storage "NominationPools::BondedPools::ah_post::consistent"
		// Assert storage "NominationPools::RewardPools::ah_post::correct"
		// Assert storage "NominationPools::RewardPools::ah_post::consistent"
		// Assert storage "NominationPools::SubPoolsStorage::ah_post::correct"
		// Assert storage "NominationPools::SubPoolsStorage::ah_post::consistent"
		// Assert storage "NominationPools::Metadata::ah_post::correct"
		// Assert storage "NominationPools::Metadata::ah_post::consistent"
		// Assert storage "NominationPools::ReversePoolIdLookup::ah_post::correct"
		// Assert storage "NominationPools::ReversePoolIdLookup::ah_post::consistent"
		// Assert storage "NominationPools::ClaimPermissions::ah_post::correct"
		// Assert storage "NominationPools::ClaimPermissions::ah_post::consistent"
		assert_eq!(
			rc_pre_payload, ah_filtered,
			"Assert storage 'NominationPools::Metadata::ah_post::correct'"
		);
	}
}
