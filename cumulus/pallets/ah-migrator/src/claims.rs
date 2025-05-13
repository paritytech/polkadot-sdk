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
use pallet_rc_migrator::claims::{alias, ClaimsMigrator, RcClaimsMessage, RcClaimsMessageOf};

impl<T: Config> Pallet<T> {
	pub fn do_receive_claims(messages: Vec<RcClaimsMessageOf<T>>) -> Result<(), Error<T>> {
		log::info!(target: LOG_TARGET, "Integrating {} claims", messages.len());
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::Claims,
			count: messages.len() as u32,
		});
		let (mut count_good, mut count_bad) = (0, 0);

		for message in messages {
			match Self::do_process_claims(message) {
				Ok(()) => count_good += 1,
				Err(e) => {
					count_bad += 1;
					log::error!(target: LOG_TARGET, "Error while integrating claims: {:?}", e);
				},
			}
		}
		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::Claims,
			count_good,
			count_bad,
		});

		Ok(())
	}

	pub fn do_process_claims(message: RcClaimsMessageOf<T>) -> Result<(), Error<T>> {
		match message {
			RcClaimsMessage::StorageValues { total } => {
				if pallet_claims::Total::<T>::exists() {
					return Err(Error::<T>::InsertConflict);
				}
				log::debug!(target: LOG_TARGET, "Processing claims message: total {:?}", total);
				pallet_claims::Total::<T>::put(total);
			},
			RcClaimsMessage::Claims((who, amount)) => {
				if alias::Claims::<T>::contains_key(&who) {
					return Err(Error::<T>::InsertConflict);
				}
				log::debug!(target: LOG_TARGET, "Processing claims message: claims {:?}", who);
				alias::Claims::<T>::insert(who, amount);
			},
			RcClaimsMessage::Vesting { who, schedule } => {
				if alias::Vesting::<T>::contains_key(&who) {
					return Err(Error::<T>::InsertConflict);
				}
				log::debug!(target: LOG_TARGET, "Processing claims message: vesting {:?}", who);
				alias::Vesting::<T>::insert(who, schedule);
			},
			RcClaimsMessage::Signing((who, statement_kind)) => {
				if alias::Signing::<T>::contains_key(&who) {
					return Err(Error::<T>::InsertConflict);
				}
				log::debug!(target: LOG_TARGET, "Processing claims message: signing {:?}", who);
				alias::Signing::<T>::insert(who, statement_kind);
			},
			RcClaimsMessage::Preclaims((who, address)) => {
				if alias::Preclaims::<T>::contains_key(&who) {
					return Err(Error::<T>::InsertConflict);
				}
				log::debug!(target: LOG_TARGET, "Processing claims message: preclaims {:?}", who);
				alias::Preclaims::<T>::insert(who, address);
			},
		}
		Ok(())
	}
}

#[cfg(feature = "std")]
impl<T: Config> crate::types::AhMigrationCheck for ClaimsMigrator<T> {
	type RcPrePayload = Vec<RcClaimsMessageOf<T>>;
	type AhPrePayload = ();

	fn pre_check(_: Self::RcPrePayload) -> Self::AhPrePayload {
		// "Assert storage 'Claims::Total::ah_pre::empty'"
		assert!(
			!pallet_claims::Total::<T>::exists(),
			"Assert storage 'Claims::Total::ah_pre::empty'"
		);
		// "Assert storage 'Claims::Claims::ah_pre::empty'"
		assert!(
			alias::Claims::<T>::iter().next().is_none(),
			"Assert storage 'Claims::Claims::ah_pre::empty'"
		);
		// "Assert storage 'Claims::Vesting::ah_pre::empty'"
		assert!(
			alias::Vesting::<T>::iter().next().is_none(),
			"Assert storage 'Claims::Vesting::ah_pre::empty'"
		);
		// "Assert storage 'Claims::Signing::ah_pre::empty'"
		assert!(
			alias::Signing::<T>::iter().next().is_none(),
			"Assert storage 'Claims::Signing::ah_pre::empty'"
		);
		// "Assert storage 'Claims::Preclaims::ah_pre::empty'"
		assert!(
			alias::Preclaims::<T>::iter().next().is_none(),
			"Assert storage 'Claims::Preclaims::ah_pre::empty'"
		);
	}

	fn post_check(rc_pre_payload: Self::RcPrePayload, _: Self::AhPrePayload) {
		let mut ah_messages = Vec::new();

		// Collect current state
		let total = pallet_claims::Total::<T>::get();
		// "Assert storage 'Claims::Total::ah_post::correct'"
		ah_messages.push(RcClaimsMessage::StorageValues { total });

		for (address, amount) in alias::Claims::<T>::iter() {
			// "Assert storage 'Claims::Claims::ah_post::correct'"
			ah_messages.push(RcClaimsMessage::Claims((address, amount)));
		}

		for (address, schedule) in alias::Vesting::<T>::iter() {
			// "Assert storage 'Claims::Vesting::ah_post::correct'"
			ah_messages.push(RcClaimsMessage::Vesting { who: address, schedule });
		}

		for (address, statement) in alias::Signing::<T>::iter() {
			// "Assert storage 'Claims::Signing::ah_post::correct'"
			ah_messages.push(RcClaimsMessage::Signing((address, statement)));
		}

		for (account_id, address) in alias::Preclaims::<T>::iter() {
			// "Assert storage 'Claims::Preclaims::ah_post::correct'"
			ah_messages.push(RcClaimsMessage::Preclaims((account_id, address)));
		}

		// Assert storage "Claims::Claims::ah_post::length"
		// Assert storage "Claims::Claims::ah_post::consistent"
		// Assert storage "Claims::Claims::ah_post::correct"
		// Assert storage "Claims::Vesting::ah_post::length"
		// Assert storage "Claims::Vesting::ah_post::consistent"
		// Assert storage "Claims::Vesting::ah_post::correct"
		// Assert storage "Claims::Signing::ah_post::length"
		// Assert storage "Claims::Signing::ah_post::consistent"
		// Assert storage "Claims::Signing::ah_post::correct"
		// Assert storage "Claims::Preclaims::ah_post::length"
		// Assert storage "Claims::Preclaims::ah_post::consistent"
		// Assert storage "Claims::Preclaims::ah_post::correct"
		// Assert storage "Claims::Total::ah_post::length"
		// Assert storage "Claims::Total::ah_post::consistent"
		// Assert storage "Claims::Total::ah_post::correct"
		assert_eq!(
			rc_pre_payload, ah_messages,
			"Claims data mismatch: Asset Hub schedules differ from original Relay Chain schedules"
		);
	}
}
