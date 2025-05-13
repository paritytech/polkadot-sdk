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

/// An entry of the `RequestStatusFor` storage map.
#[derive(Encode, Decode, TypeInfo, Clone, MaxEncodedLen, RuntimeDebug, PartialEq, Eq)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub struct RcPreimageRequestStatus<AccountId, Ticket> {
	/// The hash of the original preimage.
	pub hash: H256,
	/// The request status of the original preimage.
	pub request_status: alias::RequestStatus<AccountId, Ticket>,
}

pub type RcPreimageRequestStatusOf<T> =
	RcPreimageRequestStatus<<T as frame_system::Config>::AccountId, super::alias::TicketOf<T>>;

pub struct PreimageRequestStatusMigrator<T: pallet_preimage::Config> {
	_phantom: PhantomData<T>,
}

impl<T: Config> PalletMigration for PreimageRequestStatusMigrator<T> {
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
				.any_lt(T::AhWeightInfo::receive_preimage_request_status((batch.len() + 1) as u32))
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

			let Some(request_status) = alias::RequestStatusFor::<T>::get(next_key_inner) else {
				defensive!("Storage corruption");
				next_key = Self::next_key(Some(next_key_inner));
				continue;
			};

			batch.push(RcPreimageRequestStatus { hash: next_key_inner, request_status });
			log::debug!(target: LOG_TARGET, "Exported preimage request status for: {:?}", next_key_inner);

			next_key = Self::next_key(Some(next_key_inner));
			// Remove the migrated key from the relay chain
			alias::RequestStatusFor::<T>::remove(next_key_inner);

			if next_key.is_none() {
				break next_key;
			}
		};

		if !batch.is_empty() {
			Pallet::<T>::send_chunked_xcm_and_track(
				batch,
				|batch| types::AhMigratorCall::<T>::ReceivePreimageRequestStatus {
					request_status: batch,
				},
				|len| T::AhWeightInfo::receive_preimage_request_status(len),
			)?;
		}

		Ok(new_next_key)
	}
}

impl<T: Config> PreimageRequestStatusMigrator<T> {
	/// Get the next key after the given one or the first one for `None`.
	pub fn next_key(key: Option<H256>) -> Option<H256> {
		match key {
			None => alias::RequestStatusFor::<T>::iter_keys().next(),
			Some(key) => alias::RequestStatusFor::<T>::iter_keys_from(
				alias::RequestStatusFor::<T>::hashed_key_for(key),
			)
			.next(),
		}
	}
}

impl<T: Config> RcMigrationCheck for PreimageRequestStatusMigrator<T> {
	type RcPrePayload = Vec<(H256, bool)>;

	fn pre_check() -> Self::RcPrePayload {
		alias::RequestStatusFor::<T>::iter()
			.filter(|(hash, _)| {
				alias::PreimageFor::<T>::iter_keys().any(|(key_hash, _)| key_hash == *hash)
			})
			.map(|(hash, request_status)| {
				(
					hash,
					match request_status {
						alias::RequestStatus::Requested { .. } => true,
						_ => false,
					},
				)
			})
			.collect()
	}

	fn post_check(_rc_pre_payload: Self::RcPrePayload) {
		// "Assert storage 'Preimage::RequestStatusFor::rc_post::empty'"
		assert_eq!(
			alias::RequestStatusFor::<T>::iter().count(),
			0,
			"Preimage::RequestStatusFor must be empty on the relay chain after migration"
		);
	}
}
