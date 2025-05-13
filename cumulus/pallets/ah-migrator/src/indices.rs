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
use pallet_rc_migrator::indices::{IndicesMigrator, RcIndicesIndex, RcIndicesIndexOf};

impl<T: Config> Pallet<T> {
	pub fn do_receive_indices(indices: Vec<RcIndicesIndexOf<T>>) -> Result<(), Error<T>> {
		let len = indices.len() as u32;
		Self::deposit_event(Event::BatchReceived { pallet: PalletEventName::Indices, count: len });
		log::info!(target: LOG_TARGET, "Integrating batch of {} indices", len);

		for index in indices {
			Self::do_receive_index(index);
		}

		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::Indices,
			count_good: len,
			count_bad: 0,
		});
		Ok(())
	}

	pub fn do_receive_index(index: RcIndicesIndexOf<T>) {
		log::debug!(target: LOG_TARGET, "Integrating index {:?}", &index.index);
		defensive_assert!(!pallet_indices::Accounts::<T>::contains_key(&index.index));

		pallet_indices::Accounts::<T>::insert(
			&index.index,
			(index.who, index.deposit, index.frozen),
		);
	}
}

#[cfg(feature = "std")]
impl<T: Config> crate::types::AhMigrationCheck for IndicesMigrator<T> {
	type RcPrePayload = Vec<RcIndicesIndexOf<T>>;
	type AhPrePayload = Vec<RcIndicesIndexOf<T>>;

	fn pre_check(_: Self::RcPrePayload) -> Self::AhPrePayload {
		let indices = pallet_indices::Accounts::<T>::iter()
			.map(|(index, (who, deposit, frozen))| RcIndicesIndex { index, who, deposit, frozen })
			.collect::<Vec<_>>();

		assert!(indices.is_empty(), "Assert storage 'Indices::Accounts::ah_pre::empty'");

		indices
	}

	fn post_check(rc_pre_payload: Self::RcPrePayload, ah_pre_payload: Self::AhPrePayload) {
		use std::collections::BTreeMap;
		let all_pre: BTreeMap<_, _> = rc_pre_payload
			.into_iter()
			.chain(ah_pre_payload.into_iter())
			.map(|RcIndicesIndex { index, who, deposit, frozen }| (index, (who, deposit, frozen)))
			.collect();
		let all_post: BTreeMap<_, _> = pallet_indices::Accounts::<T>::iter().collect();

		// Note that by using BTreeMaps, we implicitly check the case that an AH entry is not
		// overwritten by a RC entry since we iterate over the RC entries first before the collect.
		// Assert storage "Indices::Accounts::ah_post::correct"
		// Assert storage "Indices::Accounts::ah_post::consistent"
		// Assert storage "Indices::Accounts::ah_post::length"
		assert_eq!(all_pre, all_post, "RC and AH indices are present");
	}
}
