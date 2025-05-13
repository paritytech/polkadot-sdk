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

use crate::{preimage::*, types::*, *};

/// An entry of the `StatusFor` storage map. Should only be used to unreserve funds on AH.
#[derive(Encode, Decode, TypeInfo, Clone, MaxEncodedLen, RuntimeDebug, PartialEq, Eq)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub struct RcPreimageLegacyStatus<AccountId, Balance> {
	/// The hash of the original preimage.
	///
	/// This is not really needed by AH, just here to make debugging easier.
	pub hash: H256,
	/// The account that made the deposit.
	pub depositor: AccountId,
	/// The amount of the storage deposit.
	pub deposit: Balance,
}

pub type RcPreimageLegacyStatusOf<T> =
	RcPreimageLegacyStatus<<T as frame_system::Config>::AccountId, super::alias::BalanceOf<T>>;

pub struct PreimageLegacyRequestStatusMigrator<T: pallet_preimage::Config> {
	_phantom: PhantomData<T>,
}

impl<T: Config> PalletMigration for PreimageLegacyRequestStatusMigrator<T> {
	type Key = H256;
	type Error = Error<T>;

	fn migrate_many(
		mut next_key: Option<Self::Key>,
		weight_counter: &mut WeightMeter,
	) -> Result<Option<Self::Key>, Self::Error> {
		let mut batch = XcmBatchAndMeter::new_from_config::<T>();

		let new_next_key = loop {
			if weight_counter.try_consume(T::DbWeight::get().reads_writes(1, 1)).is_err() ||
				weight_counter.try_consume(batch.consume_weight()).is_err()
			{
				log::info!("RC weight limit reached at batch length {}, stopping", batch.len());
				if batch.is_empty() {
					return Err(Error::OutOfWeight);
				} else {
					break next_key;
				}
			}

			if T::MaxAhWeight::get()
				.any_lt(T::AhWeightInfo::receive_preimage_legacy_status((batch.len() + 1) as u32))
			{
				log::info!("AH weight limit reached at batch length {}, stopping", batch.len());
				if batch.is_empty() {
					return Err(Error::OutOfWeight);
				} else {
					break next_key;
				}
			}

			let next_key_inner = match next_key {
				Some(key) => key,
				None => {
					let Some(key) = Self::next_key(None) else {
						break None;
					};
					key
				},
			};

			let Some(request_status) = alias::StatusFor::<T>::get(next_key_inner) else {
				defensive!("Storage corruption");
				next_key = Self::next_key(Some(next_key_inner));
				continue;
			};

			match request_status {
				alias::OldRequestStatus::Unrequested { deposit: (depositor, deposit), .. } => {
					batch.push(RcPreimageLegacyStatus { hash: next_key_inner, depositor, deposit });
				},
				alias::OldRequestStatus::Requested {
					deposit: Some((depositor, deposit)), ..
				} => {
					batch.push(RcPreimageLegacyStatus { hash: next_key_inner, depositor, deposit });
				},
				_ => {},
			}

			log::debug!(target: LOG_TARGET, "Exported legacy preimage status for: {:?}", next_key_inner);
			next_key = Self::next_key(Some(next_key_inner));
			// Remove the migrated key from the relay chain
			alias::StatusFor::<T>::remove(next_key_inner);

			if next_key.is_none() {
				break next_key;
			}
		};

		if !batch.is_empty() {
			Pallet::<T>::send_chunked_xcm_and_track(
				batch,
				|batch| types::AhMigratorCall::<T>::ReceivePreimageLegacyStatus {
					legacy_status: batch,
				},
				|len| T::AhWeightInfo::receive_preimage_legacy_status(len),
			)?;
		}

		Ok(new_next_key)
	}
}

impl<T: Config> PreimageLegacyRequestStatusMigrator<T> {
	/// Get the next key after the given one or the first one for `None`.
	pub fn next_key(key: Option<H256>) -> Option<H256> {
		match key {
			None => alias::StatusFor::<T>::iter_keys().next(),
			Some(key) =>
				alias::StatusFor::<T>::iter_keys_from(alias::StatusFor::<T>::hashed_key_for(key))
					.next(),
		}
	}
}

impl<T: Config> RcMigrationCheck for PreimageLegacyRequestStatusMigrator<T> {
	type RcPrePayload = Vec<H256>;

	fn pre_check() -> Self::RcPrePayload {
		alias::StatusFor::<T>::iter().map(|(hash, _)| hash).collect()
	}

	fn post_check(rc_pre_payload: Self::RcPrePayload) {
		for hash in rc_pre_payload {
			// "Assert storage 'Preimage::StatusFor::rc_post::empty'"
			assert!(
				!alias::StatusFor::<T>::contains_key(hash),
				"migrated key in Preimage::StatusFor must be deleted"
			);
		}
	}
}
