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
use pallet_rc_migrator::staking::bags_list::alias;

impl<T: Config> Pallet<T> {
	pub fn do_receive_bags_list_messages(
		messages: Vec<RcBagsListMessage<T>>,
	) -> Result<(), Error<T>> {
		let (mut good, mut bad) = (0, 0);
		log::info!(target: LOG_TARGET, "Integrating {} BagsListMessages", messages.len());
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::BagsList,
			count: messages.len() as u32,
		});

		for message in messages {
			match Self::do_receive_bags_list_message(message) {
				Ok(_) => good += 1,
				Err(_) => bad += 1,
			}
		}

		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::BagsList,
			count_good: good as u32,
			count_bad: bad as u32,
		});

		Ok(())
	}

	pub fn do_receive_bags_list_message(message: RcBagsListMessage<T>) -> Result<(), Error<T>> {
		match message {
			RcBagsListMessage::Node { id, node } => {
				if alias::ListNodes::<T>::contains_key(&id) {
					return Err(Error::<T>::InsertConflict);
				}

				alias::ListNodes::<T>::insert(&id, &node);
				log::debug!(target: LOG_TARGET, "Integrating BagsListNode: {:?}", &id);
			},
			RcBagsListMessage::Bag { score, bag } => {
				if alias::ListBags::<T>::contains_key(&score) {
					return Err(Error::<T>::InsertConflict);
				}

				alias::ListBags::<T>::insert(&score, &bag);
				log::debug!(target: LOG_TARGET, "Integrating BagsListBag: {:?}", &score);
			},
		}

		Ok(())
	}
}
