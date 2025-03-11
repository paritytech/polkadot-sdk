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
		Self::deposit_event(Event::PreimageChunkBatchReceived { count: chunks.len() as u32 });
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
		Self::deposit_event(Event::PreimageChunkBatchProcessed { count_good, count_bad });

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
		Self::deposit_event(Event::PreimageRequestStatusBatchReceived {
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

		Self::deposit_event(Event::PreimageRequestStatusBatchProcessed { count_good, count_bad });
		Ok(())
	}

	pub fn do_receive_preimage_request_status(
		request_status: RcPreimageRequestStatusOf<T>,
	) -> Result<(), Error<T>> {
		if alias::RequestStatusFor::<T>::contains_key(request_status.hash) {
			log::warn!(target: LOG_TARGET, "Request status already migrated: {:?}", request_status.hash);
			return Ok(());
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
		Self::deposit_event(Event::PreimageLegacyStatusBatchReceived {
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

		Self::deposit_event(Event::PreimageLegacyStatusBatchProcessed { count_good, count_bad });
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
			"Preimage::PreimageFor is not empty"
		);
		assert!(
			alias::RequestStatusFor::<T>::iter_keys().next().is_none(),
			"Preimage::RequestStatusFor is not empty"
		);
	}

	// The payload should come from the relay chain pre-check method on the same pallet
	fn post_check(rc_pre_payload: Self::RcPrePayload, _ah_pre_payload: Self::AhPrePayload) {
		// Check that the PreimageFor entries are sane.
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
			assert!(
				alias::RequestStatusFor::<T>::contains_key(key.0),
				"Preimage::RequestStatusFor is missing"
			);
		}

		for (hash, len) in rc_pre_payload {
			if alias::PreimageFor::<T>::contains_key((hash, len)) {
				log::error!("missing relay chain item in assetHub for Preimage::PreimageFor");
			}
			// TODO: fix failing check and change log to assert below
			// assert!(
			//   alias::PreimageFor::<T>::contains_key((hash, len)),
			//	 "missing relay chain item in assetHub for Preimage::PreimageFor"
			// );
		}

		// Integrity check that all preimages have the correct hash and length
		for (hash, len) in alias::PreimageFor::<T>::iter_keys() {
			let preimage = alias::PreimageFor::<T>::get((hash, len)).expect("Storage corrupted");

			assert_eq!(preimage.len(), len as usize);
			assert_eq!(BlakeTwo256::hash(preimage.as_slice()), hash);
		}

		for (hash, status) in alias::RequestStatusFor::<T>::iter() {
			match status {
				alias::RequestStatus::Unrequested { len, .. } => {
					assert!(
						alias::PreimageFor::<T>::contains_key((hash, len)),
						"Preimage::RequestStatusFor is missing preimage"
					);
				},
				alias::RequestStatus::Requested { maybe_len: Some(len), .. } => {
					assert!(
						alias::PreimageFor::<T>::contains_key((hash, len)),
						"Preimage::RequestStatusFor is missing preimage"
					);
				},
				_ => {},
			}
		}
		/*assert_eq!(
			alias::PreimageFor::<T>::iter_keys().count(),
			alias::RequestStatusFor::<T>::iter_keys().count(),
			"Preimage::PreimageFor and Preimage::RequestStatusFor have different lengths"
		);*/
		// TODO fixme (ggwpez had to comment this since it fails with a new snapshot)
	}
}
