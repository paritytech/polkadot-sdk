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
