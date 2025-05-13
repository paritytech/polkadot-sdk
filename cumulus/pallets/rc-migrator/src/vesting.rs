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
use frame_support::traits::Currency;
use pallet_vesting::MaxVestingSchedulesGet;
use sp_std::vec::Vec;

pub type BalanceOf<T> = <<T as pallet_vesting::Config>::Currency as Currency<
	<T as frame_system::Config>::AccountId,
>>::Balance;

#[derive(
	Encode, Decode, CloneNoBound, PartialEqNoBound, EqNoBound, TypeInfo, MaxEncodedLen, DebugNoBound,
)]
#[codec(mel_bound(T: pallet_vesting::Config))]
#[scale_info(skip_type_params(T))]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub struct RcVestingSchedule<T: pallet_vesting::Config> {
	pub who: <T as frame_system::Config>::AccountId,
	pub schedules: BoundedVec<
		pallet_vesting::VestingInfo<BalanceOf<T>, BlockNumberFor<T>>,
		MaxVestingSchedulesGet<T>,
	>,
}

pub struct VestingMigrator<T> {
	_phantom: PhantomData<T>,
}

impl<T: Config> PalletMigration for VestingMigrator<T> {
	type Key = T::AccountId;
	type Error = Error<T>;

	fn migrate_many(
		current_key: Option<Self::Key>,
		weight_counter: &mut WeightMeter,
	) -> Result<Option<Self::Key>, Self::Error> {
		let mut inner_key = current_key;
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
				.any_lt(T::AhWeightInfo::receive_vesting_schedules((messages.len() + 1) as u32))
			{
				log::info!("AH weight limit reached at batch length {}, stopping", messages.len());
				if messages.is_empty() {
					return Err(Error::OutOfWeight);
				} else {
					break;
				}
			}
			let mut iter = match inner_key {
				Some(who) => pallet_vesting::Vesting::<T>::iter_from_key(who),
				None => pallet_vesting::Vesting::<T>::iter(),
			};

			match iter.next() {
				Some((who, schedules)) => {
					pallet_vesting::Vesting::<T>::remove(&who);
					messages.push(RcVestingSchedule { who: who.clone(), schedules });
					log::debug!(target: LOG_TARGET, "Migrating vesting schedules for {:?}", who);
					inner_key = Some(who);
				},
				None => {
					inner_key = None;
					break;
				},
			}
		}

		if !messages.is_empty() {
			Pallet::<T>::send_chunked_xcm_and_track(
				messages,
				|messages| types::AhMigratorCall::ReceiveVestingSchedules { messages },
				|len| T::AhWeightInfo::receive_vesting_schedules(len),
			)?;
		}

		Ok(inner_key)
	}
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GenericVestingInfo<BlockNumber, Balance> {
	pub locked: Balance,
	pub starting_block: BlockNumber,
	pub per_block: Balance,
}

#[cfg(feature = "std")]
impl<T: Config> crate::types::RcMigrationCheck for VestingMigrator<T> {
	type RcPrePayload =
		Vec<(Vec<u8>, Vec<BalanceOf<T>>, Vec<GenericVestingInfo<BlockNumberFor<T>, BalanceOf<T>>>)>;

	fn pre_check() -> Self::RcPrePayload {
		pallet_vesting::Vesting::<T>::iter()
			.map(|(who, schedules)| {
				let balances: Vec<BalanceOf<T>> = schedules.iter().map(|s| s.locked()).collect();
				let vesting_info: Vec<GenericVestingInfo<BlockNumberFor<T>, BalanceOf<T>>> =
					schedules
						.iter()
						.map(|s| GenericVestingInfo {
							locked: s.locked(),
							starting_block: s.starting_block(),
							per_block: s.per_block(),
						})
						.collect();
				(who.encode(), balances, vesting_info)
			})
			.collect()
	}

	fn post_check(_: Self::RcPrePayload) {
		// Assert storage "Vesting::Vesting::rc_post::empty"
		assert!(
			pallet_vesting::Vesting::<T>::iter().next().is_none(),
			"Vesting storage should be empty after migration"
		);
	}
}
