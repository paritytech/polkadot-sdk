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

pub type BalanceOf<T> = <<T as pallet_vesting::Config>::Currency as Currency<
	<T as frame_system::Config>::AccountId,
>>::Balance;

#[derive(
	Encode, Decode, CloneNoBound, PartialEqNoBound, EqNoBound, TypeInfo, MaxEncodedLen, DebugNoBound,
)]
#[codec(mel_bound(T: pallet_vesting::Config))]
#[scale_info(skip_type_params(T))]
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

			let mut iter = match inner_key {
				Some(who) => pallet_vesting::Vesting::<T>::iter_from_key(who),
				None => pallet_vesting::Vesting::<T>::iter(),
			};

			match iter.next() {
				Some((who, schedules)) => {
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
			Pallet::<T>::send_chunked_xcm(
				messages,
				|messages| types::AhMigratorCall::ReceiveVestingSchedules { messages },
				|_| Weight::from_all(1), // TODO
			)?;
		}

		Ok(inner_key)
	}
}
