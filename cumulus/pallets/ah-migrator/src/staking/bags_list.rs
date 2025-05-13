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
use pallet_rc_migrator::staking::bags_list::{alias, BagsListMigrator, GenericBagsListMessage};

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
				debug_assert!(!alias::ListNodes::<T>::contains_key(&id));
				alias::ListNodes::<T>::insert(&id, &node);
				log::debug!(target: LOG_TARGET, "Integrating BagsListNode: {:?}", &id);
			},
			RcBagsListMessage::Bag { score, bag } => {
				debug_assert!(!alias::ListBags::<T>::contains_key(&score));
				alias::ListBags::<T>::insert(&score, &bag);
				log::debug!(target: LOG_TARGET, "Integrating BagsListBag: {:?}", &score);
			},
		}

		Ok(())
	}
}

#[cfg(feature = "std")]
impl<T: Config> crate::types::AhMigrationCheck for BagsListMigrator<T> {
	type RcPrePayload = Vec<GenericBagsListMessage<T::AccountId, T::Score>>;
	type AhPrePayload = ();

	fn pre_check(_: Self::RcPrePayload) -> Self::AhPrePayload {
		// Assert storage "VoterList::ListNodes::ah_pre::empty"
		assert!(
			alias::ListNodes::<T>::iter().next().is_none(),
			"VoterList::ListNodes::ah_pre::empty"
		);

		// Assert storage "VoterList::ListBags::ah_pre::empty"
		assert!(
			alias::ListBags::<T>::iter().next().is_none(),
			"VoterList::ListBags::ah_pre::empty"
		);
	}

	fn post_check(rc_pre_payload: Self::RcPrePayload, _: Self::AhPrePayload) {
		assert!(!rc_pre_payload.is_empty(), "RC pre-payload should not be empty during post_check");
		let mut ah_messages = Vec::new();

		// Collect current state
		for (id, node) in alias::ListNodes::<T>::iter() {
			ah_messages.push(GenericBagsListMessage::Node {
				id: id.clone(),
				node: alias::Node {
					id: node.id,
					prev: node.prev,
					next: node.next,
					bag_upper: node.bag_upper,
					score: node.score,
				},
			});
		}

		for (score, bag) in alias::ListBags::<T>::iter() {
			ah_messages.push(GenericBagsListMessage::Bag {
				score,
				bag: alias::Bag { head: bag.head, tail: bag.tail },
			});
		}

		// Assert storage "VoterList::ListBags::ah_post::length"
		// Assert storage "VoterList::ListBags::ah_post::length"
		assert_eq!(
			rc_pre_payload.len(), ah_messages.len(),
			"Bags list length mismatch: Asset Hub data length differs from original Relay Chain data"
		);

		// Assert storage "VoterList::ListNodes::ah_post::correct"
		// Assert storage "VoterList::ListNodes::ah_post::consistent"
		// Assert storage "VoterList::ListBags::ah_post::correct"
		// Assert storage "VoterList::ListBags::ah_post::consistent"
		assert_eq!(
			rc_pre_payload, ah_messages,
			"Bags list data mismatch: Asset Hub data differs from original Relay Chain data"
		);
	}
}
