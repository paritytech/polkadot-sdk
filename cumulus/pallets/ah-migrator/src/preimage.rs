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
use frame_support::traits::{Consideration, Footprint};
use pallet_rc_migrator::preimage::{chunks::*, *};
use sp_runtime::traits::{BlakeTwo256, Hash};

impl<T: Config> Pallet<T> {
	pub fn do_receive_preimage_chunks(chunks: Vec<RcPreimageChunk>) -> Result<(), Error<T>> {
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::PreimageChunk,
			count: chunks.len() as u32,
		});
		let (mut count_good, mut count_bad) = (0, 0);
		log::info!(target: LOG_TARGET, "Integrating {} preimage chunks", chunks.len());

		for chunk in chunks {
			match Self::do_receive_preimage_chunk(chunk) {
				Ok(()) => count_good += 1,
				Err(e) => {
					count_bad += 1;
					log::error!(target: LOG_TARGET, "Error while integrating preimage chunk: {:?}", e);
				},
			}
		}
		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::PreimageChunk,
			count_good,
			count_bad,
		});

		Ok(())
	}

	pub fn do_receive_preimage_chunk(chunk: RcPreimageChunk) -> Result<(), Error<T>> {
		log::debug!(target: LOG_TARGET, "Integrating preimage chunk {} offset {}/{}", chunk.preimage_hash, chunk.chunk_byte_offset + chunk.chunk_bytes.len() as u32, chunk.preimage_len);
		let key = (chunk.preimage_hash, chunk.preimage_len);

		// First check that we did not miss a chunk
		let preimage = match alias::PreimageFor::<T>::get(key) {
			Some(preimage) => {
				if preimage.len() != chunk.chunk_byte_offset as usize {
					defensive!("Preimage chunk missing");
					return Err(Error::<T>::TODO);
				}

				match preimage.try_mutate(|p| {
					p.extend(chunk.chunk_bytes.clone());
				}) {
					Some(preimage) => {
						alias::PreimageFor::<T>::insert(key, &preimage);
						preimage
					},
					None => {
						defensive!("Preimage too big");
						return Err(Error::<T>::TODO);
					},
				}
			},
			None => {
				if chunk.chunk_byte_offset != 0 {
					defensive!("Preimage chunk missing");
					return Err(Error::<T>::TODO);
				}

				let preimage: BoundedVec<u8, ConstU32<{ CHUNK_SIZE }>> = chunk.chunk_bytes;
				debug_assert!(CHUNK_SIZE <= pallet_rc_migrator::preimage::alias::MAX_SIZE);
				let bounded_preimage: BoundedVec<
					u8,
					ConstU32<{ pallet_rc_migrator::preimage::alias::MAX_SIZE }>,
				> = preimage.into_inner().try_into().expect("Asserted");
				alias::PreimageFor::<T>::insert(key, &bounded_preimage);
				bounded_preimage
			},
		};

		if preimage.len() == chunk.preimage_len as usize + chunk.chunk_byte_offset as usize {
			log::debug!(target: LOG_TARGET, "Preimage complete: {}", chunk.preimage_hash);
		}

		Ok(())
	}

	pub fn do_receive_preimage_request_statuses(
		request_status: Vec<RcPreimageRequestStatusOf<T>>,
	) -> Result<(), Error<T>> {
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::PreimageRequestStatus,
			count: request_status.len() as u32,
		});
		log::info!(target: LOG_TARGET, "Integrating {} preimage request status", request_status.len());
		let (mut count_good, mut count_bad) = (0, 0);

		for request_status in request_status {
			match Self::do_receive_preimage_request_status(request_status) {
				Ok(()) => count_good += 1,
				Err(e) => {
					count_bad += 1;
					log::error!(target: LOG_TARGET, "Error while integrating preimage request status: {:?}", e);
				},
			}
		}

		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::PreimageRequestStatus,
			count_good,
			count_bad,
		});
		Ok(())
	}

	pub fn do_receive_preimage_request_status(
		request_status: RcPreimageRequestStatusOf<T>,
	) -> Result<(), Error<T>> {
		if alias::RequestStatusFor::<T>::contains_key(request_status.hash) {
			log::warn!(target: LOG_TARGET, "Request status already migrated: {:?}", request_status.hash);
			return Ok(());
		}

		if !alias::PreimageFor::<T>::iter_keys()
			.any(|(key_hash, _)| key_hash == request_status.hash)
		{
			log::error!("Missing preimage for request status hash {:?}", request_status.hash);
			return Err(Error::<T>::TODO);
		}

		let new_ticket = match request_status.request_status {
			alias::RequestStatus::Unrequested { ticket: (ref who, ref ticket), len } => {
				let fp = Footprint::from_parts(1, len as usize);
				ticket.clone().update(who, fp).ok()
			},
			alias::RequestStatus::Requested {
				maybe_ticket: Some((ref who, ref ticket)),
				maybe_len: Some(len),
				..
			} => {
				let fp = Footprint::from_parts(1, len as usize);
				ticket.clone().update(who, fp).ok()
			},
			alias::RequestStatus::Requested { maybe_ticket: Some(_), maybe_len: None, .. } => {
				defensive!("Ticket cannot be re-evaluated");
				// I think this is unreachable, but not exactly sure. Either way, nothing that we
				// could do about it.
				None
			},
			_ => None,
		};

		let new_request_status = match (new_ticket, request_status.request_status.clone()) {
			(Some(new_ticket), alias::RequestStatus::Unrequested { ticket: (who, _), len }) =>
				alias::RequestStatus::Unrequested { ticket: (who, new_ticket), len },
			(
				Some(new_ticket),
				alias::RequestStatus::Requested {
					maybe_ticket: Some((who, _)),
					maybe_len: Some(len),
					count,
				},
			) => alias::RequestStatus::Requested {
				maybe_ticket: Some((who, new_ticket)),
				maybe_len: Some(len),
				count,
			},
			_ => request_status.request_status,
		};

		alias::RequestStatusFor::<T>::insert(request_status.hash, &new_request_status);
		log::debug!(target: LOG_TARGET, "Integrating preimage request status: {:?}", new_request_status);

		Ok(())
	}

	pub fn do_receive_preimage_legacy_statuses(
		statuses: Vec<RcPreimageLegacyStatusOf<T>>,
	) -> Result<(), Error<T>> {
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::PreimageLegacyStatus,
			count: statuses.len() as u32,
		});
		log::info!(target: LOG_TARGET, "Integrating {} preimage legacy status", statuses.len());
		let (mut count_good, mut count_bad) = (0, 0);

		for status in statuses {
			match Self::do_receive_preimage_legacy_status(status) {
				Ok(()) => count_good += 1,
				Err(_) => {
					count_bad += 1;
				},
			}
		}

		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::PreimageLegacyStatus,
			count_good,
			count_bad,
		});
		Ok(())
	}

	pub fn do_receive_preimage_legacy_status(
		status: RcPreimageLegacyStatusOf<T>,
	) -> Result<(), Error<T>> {
		// Unreserve the deposit
		let missing =
			<T as pallet_preimage::Config>::Currency::unreserve(&status.depositor, status.deposit);

		if missing != Default::default() {
			log::error!(target: LOG_TARGET, "Failed to unreserve deposit for preimage legacy status {:?}, who: {}, missing {:?}", status.hash,status.depositor.to_ss58check(), missing);
			return Err(Error::<T>::FailedToUnreserveDeposit);
		}

		Ok(())
	}
}

#[cfg(feature = "std")]
impl<T: Config> crate::types::AhMigrationCheck for PreimageChunkMigrator<T> {
	type RcPrePayload = Vec<(H256, u32)>;
	type AhPrePayload = ();

	fn pre_check(_rc_pre_payload: Self::RcPrePayload) -> Self::AhPrePayload {
		// AH does not have a preimage pallet, therefore must be empty.
		assert!(
			alias::PreimageFor::<T>::iter_keys().next().is_none(),
			"Assert storage 'Preimage::PreimageFor::ah_pre::empty'"
		);
	}

	// The payload should come from the relay chain pre-check method on the same pallet
	fn post_check(rc_pre_payload: Self::RcPrePayload, _ah_pre_payload: Self::AhPrePayload) {
		// Assert storage "Preimage::PreimageFor::ah_post::consistent"
		for (key, preimage) in alias::PreimageFor::<T>::iter() {
			assert!(preimage.len() > 0, "Preimage::PreimageFor is empty");
			assert!(preimage.len() <= 4 * 1024 * 1024_usize, "Preimage::PreimageFor is too big");
			assert!(
				preimage.len() == key.1 as usize,
				"Preimage::PreimageFor is not the correct length"
			);
			assert!(
				<T as frame_system::Config>::Hashing::hash(&preimage) == key.0,
				"Preimage::PreimageFor hash mismatch"
			);
			// Assert storage "Preimage::RequestStatusFor::ah_post::consistent"
			assert!(
				alias::RequestStatusFor::<T>::contains_key(key.0),
				"Preimage::RequestStatusFor is missing"
			);
		}

		let new_preimages = alias::PreimageFor::<T>::iter_keys().count();
		// Pallet scheduler currently unrequests and deletes preimage with hash
		// 0x7ee7ea7b28e3e17353781b6d9bff255b8d00beffe8d1ed259baafe1de0c2cc2e and len 42
		if new_preimages != rc_pre_payload.len() {
			log::warn!(
				"Preimage::PreimageFor and relay chain payload have different size: {} vs {}",
				new_preimages,
				rc_pre_payload.len(),
			);
		}

		// All items have been successfully migrated from the relay chain
		// Assert storage "Preimage::PreimageFor::ah_post::correct"
		for (hash, len) in rc_pre_payload.iter() {
			// Pallet scheduler currently unrequests and deletes preimage with hash
			// 0x7ee7ea7b28e3e17353781b6d9bff255b8d00beffe8d1ed259baafe1de0c2cc2e and len 42
			if !alias::PreimageFor::<T>::contains_key((hash, len)) {
				log::warn!(
					"Relay chain Preimage::PreimageFor storage item with key {:?} {:?} is not found on assethub",
					hash,
					len,
				);
			}
		}

		// All AssetHub items came from the relay chain
		// Assert storage "Preimage::PreimageFor::ah_post::correct"
		for (hash, len) in alias::PreimageFor::<T>::iter_keys() {
			// Preimages for referendums that did not pass on the relay chain can be noted when
			// migrating to Asset Hub.
			if !rc_pre_payload.contains(&(hash, len)) {
				log::warn!("Asset Hub migrated Preimage::PreimageFor storage item with key {:?} {:?} was not present on the relay chain", hash, len);
			}
		}

		// Integrity check that all preimages have the correct hash and length
		// Assert storage "Preimage::PreimageFor::ah_post::consistent"
		for (hash, len) in alias::PreimageFor::<T>::iter_keys() {
			let preimage = alias::PreimageFor::<T>::get((hash, len)).expect("Storage corrupted");

			assert_eq!(preimage.len(), len as usize);
			assert_eq!(BlakeTwo256::hash(preimage.as_slice()), hash);
		}
	}
}

#[cfg(feature = "std")]
impl<T: Config> crate::types::AhMigrationCheck for PreimageRequestStatusMigrator<T> {
	type RcPrePayload = Vec<(H256, bool)>;
	type AhPrePayload = ();

	fn pre_check(_rc_pre_payload: Self::RcPrePayload) -> Self::AhPrePayload {
		// AH does not have a preimage pallet, therefore must be empty.
		// Assert storage "Preimage::RequestStatusFor::ah_pre::empty"
		assert!(
			alias::RequestStatusFor::<T>::iter_keys().next().is_none(),
			"Preimage::RequestStatusFor is not empty"
		);
	}

	// The payload should come from the relay chain pre-check method on the same pallet
	fn post_check(rc_pre_payload: Self::RcPrePayload, _ah_pre_payload: Self::AhPrePayload) {
		let new_requests_len = alias::RequestStatusFor::<T>::iter_keys().count();
		// Pallet scheduler currently unrequests and deletes preimage with hash
		// 0x7ee7ea7b28e3e17353781b6d9bff255b8d00beffe8d1ed259baafe1de0c2cc2e and len 42
		if new_requests_len != rc_pre_payload.len() {
			log::warn!(
				"Preimage::RequestStatusFor and relay chain payload have different size: {} vs {}",
				new_requests_len,
				rc_pre_payload.len(),
			);
		}

		for (hash, requested) in rc_pre_payload.iter() {
			// Pallet scheduler currently unrequests and deletes preimage with hash
			// 0x7ee7ea7b28e3e17353781b6d9bff255b8d00beffe8d1ed259baafe1de0c2cc2e and len 42
			// Assert storage "Preimage::RequestStatusFor::ah_post::correct"
			if !alias::RequestStatusFor::<T>::contains_key(hash) {
				log::warn!(
					"Relay chain Preimage::RequestStatusFor storage item with key {:?} is not found on assethub",
					hash
				);
			} else {
				match alias::RequestStatusFor::<T>::get(hash).unwrap() {
					alias::RequestStatus::Unrequested { len, .. } => {
						assert!(
							alias::PreimageFor::<T>::contains_key((hash, len)),
							"Preimage::RequestStatusFor is missing preimage"
						);
					},
					alias::RequestStatus::Requested { maybe_len: Some(len), .. } => {
						// TODO: preimages that store referendums calls will be unrequested since
						// the call of the preimage is mapped and a new preimage of the mapped call
						// is noted. The unrequested preimage can be deletes since not needed
						// anymore.
						//
						// assert!(
						// 	requested,
						// 	"Unrequested preimage with hash {:?} in the relay chain has become
						// requested on assetHub", 	hash
						// );
						assert!(
							alias::PreimageFor::<T>::contains_key((hash, len)),
							"Preimage::RequestStatusFor is missing preimage"
						);
					},
					alias::RequestStatus::Requested { .. } => {
						assert!(
							requested,
							"Unrequested preimage with hash {:?} in the relay chain has become requested on assetHub",
							hash
						);
					},
				}
			}
		}

		for hash in alias::RequestStatusFor::<T>::iter_keys() {
			// Preimages for referendums that did not pass on the relay chain can be noted when
			// migrating to Asset Hub.
			if !rc_pre_payload.contains(&(hash, true)) && !rc_pre_payload.contains(&(hash, false)) {
				log::warn!("Asset Hub migrated Preimage::RequestStatusFor storage item with key {:?} was not present on the relay chain", hash);
			}
		}

		// Assert storage "Preimage::PreimageFor::ah_post::consistent"
		assert_eq!(
			alias::PreimageFor::<T>::iter_keys().count(),
			alias::RequestStatusFor::<T>::iter_keys().count(),
			"Preimage::PreimageFor and Preimage::RequestStatusFor have different lengths on Asset Hub"
		);
	}
}

#[cfg(feature = "std")]
impl<T: Config> crate::types::AhMigrationCheck for PreimageLegacyRequestStatusMigrator<T> {
	type RcPrePayload = Vec<H256>;
	type AhPrePayload = ();

	fn pre_check(_rc_pre_payload: Self::RcPrePayload) -> Self::AhPrePayload {
		// AH does not have a preimage pallet, therefore must be empty.
		// Assert storage "Preimage::StatusFor::ah_pre::empty"
		assert!(
			alias::StatusFor::<T>::iter_keys().next().is_none(),
			"Preimage::StatusFor is not empty on the relay chain"
		);
	}

	fn post_check(_rc_pre_payload: Self::RcPrePayload, _ah_pre_payload: Self::AhPrePayload) {
		// All items have been deleted
		// Assert storage "Preimage::StatusFor::ah_post::correct"
		assert!(
			alias::StatusFor::<T>::iter_keys().next().is_none(),
			"Preimage::StatusFor is not empty on assetHub"
		);
	}
}
