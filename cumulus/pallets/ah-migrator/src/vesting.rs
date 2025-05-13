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

use super::*;
use pallet_rc_migrator::vesting::{
	BalanceOf, GenericVestingInfo, RcVestingSchedule, VestingMigrator,
};

impl<T: Config> Pallet<T> {
	pub fn do_receive_vesting_schedules(
		messages: Vec<RcVestingSchedule<T>>,
	) -> Result<(), Error<T>> {
		alias::StorageVersion::<T>::put(alias::Releases::V1);
		log::info!(target: LOG_TARGET, "Integrating {} vesting schedules", messages.len());
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::Vesting,
			count: messages.len() as u32,
		});
		let (mut count_good, mut count_bad) = (0, 0);

		for message in messages {
			match Self::do_process_vesting_schedule(message) {
				Ok(()) => count_good += 1,
				Err(e) => {
					count_bad += 1;
					log::error!(target: LOG_TARGET, "Error while integrating vesting: {:?}", e);
				},
			}
		}

		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::Vesting,
			count_good,
			count_bad,
		});

		Ok(())
	}

	/// Integrate vesting schedules.
	pub fn do_process_vesting_schedule(message: RcVestingSchedule<T>) -> Result<(), Error<T>> {
		let mut ah_schedules = pallet_vesting::Vesting::<T>::get(&message.who).unwrap_or_default();

		if !ah_schedules.is_empty() {
			defensive!("We disabled vesting, looks like someone used it. Manually verify this and then remove this defensive assert.");
		}

		for schedule in message.schedules {
			ah_schedules
				.try_push(schedule)
				.defensive()
				.map_err(|_| Error::<T>::FailedToIntegrateVestingSchedule)?;
		}

		pallet_vesting::Vesting::<T>::insert(&message.who, &ah_schedules);
		log::debug!(target: LOG_TARGET, "Integrated vesting schedule for {:?}, len {}", message.who, ah_schedules.len());

		Ok(())
	}
}

pub mod alias {
	use super::*;

	#[frame_support::storage_alias(pallet_name)]
	pub type StorageVersion<T: pallet_vesting::Config> =
		StorageValue<pallet_vesting::Pallet<T>, Releases, ValueQuery>;

	#[derive(
		Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, MaxEncodedLen, Default, TypeInfo,
	)]
	pub enum Releases {
		#[default]
		V0,
		V1,
	}
}

#[cfg(feature = "std")]
impl<T: Config> crate::types::AhMigrationCheck for VestingMigrator<T> {
	type RcPrePayload =
		Vec<(Vec<u8>, Vec<BalanceOf<T>>, Vec<GenericVestingInfo<BlockNumberFor<T>, BalanceOf<T>>>)>;

	type AhPrePayload = ();

	fn pre_check(_: Self::RcPrePayload) -> Self::AhPrePayload {
		let vesting_schedules: Vec<_> = pallet_vesting::Vesting::<T>::iter().collect();
		assert!(vesting_schedules.is_empty(), "Assert storage 'Vesting::Vesting::ah_pre::empty'");
		assert_eq!(
			alias::StorageVersion::<T>::get(),
			alias::Releases::V0,
			"Vesting::StorageVersion::ah_post::empty"
		)
	}

	fn post_check(rc_pre_payload: Self::RcPrePayload, _: Self::AhPrePayload) {
		use std::collections::BTreeMap;

		let rc_pre: BTreeMap<
			Vec<u8>,
			(Vec<BalanceOf<T>>, Vec<GenericVestingInfo<BlockNumberFor<T>, BalanceOf<T>>>),
		> = rc_pre_payload
			.into_iter()
			.map(|(who, balances, vesting_info)| (who, (balances, vesting_info)))
			.collect();

		let all_post: BTreeMap<
			Vec<u8>,
			(Vec<BalanceOf<T>>, Vec<GenericVestingInfo<BlockNumberFor<T>, BalanceOf<T>>>),
		> = pallet_vesting::Vesting::<T>::iter()
			.map(|(who, schedules)| {
				let mut balances: Vec<BalanceOf<T>> = Vec::new();
				let mut vesting_info: Vec<GenericVestingInfo<BlockNumberFor<T>, BalanceOf<T>>> =
					Vec::new();

				for s in schedules.iter() {
					balances.push(s.locked());
					vesting_info.push(GenericVestingInfo {
						locked: s.locked(),
						starting_block: s.starting_block(),
						per_block: s.per_block(),
					});
				}
				(who.encode(), (balances, vesting_info))
			})
			.collect();

		// Assert storage "Vesting::Vesting::ah_post::correct"
		// Assert storage "Vesting::Vesting::ah_post::consistent"
		assert_eq!(
			rc_pre,
			all_post,
			"Vesting schedules mismatch: Asset Hub schedules differ from original Relay Chain schedules"
		);

		// Assert storage "Vesting::Vesting::ah_post::length"
		assert_eq!(
			rc_pre.len(),
			all_post.len(),
			"Vesting schedules mismatch: Asset Hub schedules differ from original Relay Chain schedules"
		);

		// Assert storage "Vesting::StorageVersion::ah_post::correct"
		// Assert storage "Vesting::StorageVersion::ah_post::consistent"
		assert_eq!(alias::StorageVersion::<T>::get(), alias::Releases::V1, "Vesting StorageVersion mismatch: Asset Hub schedules differ from original Relay Chain schedules")
	}
}
