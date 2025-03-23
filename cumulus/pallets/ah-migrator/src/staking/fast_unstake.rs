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

//! Fast unstake migration logic.

use crate::*;

impl<T: Config> Pallet<T> {
	pub fn do_receive_fast_unstake_messages(
		messages: Vec<RcFastUnstakeMessage<T>>,
	) -> Result<(), Error<T>> {
		let (mut good, mut bad) = (0, 0);
		log::info!(target: LOG_TARGET, "Integrating {} FastUnstakeMessages", messages.len());
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::FastUnstake,
			count: messages.len() as u32,
		});

		for message in messages {
			match Self::do_receive_fast_unstake_message(message) {
				Ok(_) => good += 1,
				Err(_) => bad += 1,
			}
		}

		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::FastUnstake,
			count_good: good as u32,
			count_bad: bad as u32,
		});

		Ok(())
	}

	pub fn do_receive_fast_unstake_message(
		message: RcFastUnstakeMessage<T>,
	) -> Result<(), Error<T>> {
		match message {
			RcFastUnstakeMessage::StorageValues { values } => {
				FastUnstakeMigrator::<T>::put_values(values);
				log::debug!(target: LOG_TARGET, "Integrating FastUnstakeStorageValues");
			},
			RcFastUnstakeMessage::Queue { member } => {
				if pallet_fast_unstake::Queue::<T>::contains_key(&member.0) {
					return Err(Error::<T>::InsertConflict);
				}
				log::debug!(target: LOG_TARGET, "Integrating FastUnstakeQueueMember: {:?}", &member.0);
				pallet_fast_unstake::Queue::<T>::insert(member.0, member.1);
			},
		}

		Ok(())
	}
}
