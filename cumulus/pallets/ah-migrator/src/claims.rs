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
use pallet_rc_migrator::claims::{alias, RcClaimsMessage, RcClaimsMessageOf};

impl<T: Config> Pallet<T> {
	pub fn do_receive_claims(messages: Vec<RcClaimsMessageOf<T>>) -> Result<(), Error<T>> {
		log::info!(target: LOG_TARGET, "Integrating {} claims", messages.len());
		Self::deposit_event(Event::ClaimsBatchReceived { count: messages.len() as u32 });
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
		Self::deposit_event(Event::ClaimsBatchProcessed { count_good, count_bad });

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
