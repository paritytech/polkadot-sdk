// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

#![cfg_attr(not(feature = "std"), no_std)]

//! Process child trie data from relay chain state proofs via configurable handler.
//!
//! This pallet is heavily opinionated toward a parachain-to-parachain publish-subscribe model.
//! It assumes ParaId as the identifier for each child trie and is designed specifically for
//! extracting published data from relay chain proofs in a pubsub mechanism.

extern crate alloc;

use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use codec::Decode;
use cumulus_pallet_parachain_system::relay_state_snapshot::{
	ProcessRelayProofKeys, RelayChainStateProof,
};
use cumulus_primitives_core::ParaId;
use frame_support::{
	defensive,
	pallet_prelude::*,
	storage::bounded_btree_map::BoundedBTreeMap,
	traits::{Get, StorageVersion},
};
use sp_std::vec;

pub use pallet::*;
pub use weights::{WeightInfo, SubstrateWeight};

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(any(test, feature = "runtime-benchmarks"))]
mod test_util;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;

/// Define subscriptions and handle received data.
pub trait SubscriptionHandler {
	/// List of subscriptions as (ParaId, keys) tuples.
	/// Returns (subscriptions, weight) where weight is the cost of computing the subscriptions.
	fn subscriptions() -> (Vec<(ParaId, Vec<Vec<u8>>)>, Weight);

	/// Called when subscribed data is updated.
	/// Returns the weight consumed by processing the data.
	fn on_data_updated(publisher: ParaId, key: Vec<u8>, value: Vec<u8>) -> Weight;
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Handler for defining subscriptions and processing received data.
		type SubscriptionHandler: SubscriptionHandler;
		/// Weight information for extrinsics and operations.
		type WeightInfo: WeightInfo;
		/// Maximum number of publishers that can be tracked simultaneously.
		#[pallet::constant]
		type MaxPublishers: Get<u32>;
	}

	/// Child trie roots from previous block for change detection.
	#[pallet::storage]
	pub type PreviousPublishedDataRoots<T: Config> = StorageValue<
		_,
		BoundedBTreeMap<ParaId, BoundedVec<u8, ConstU32<32>>, T::MaxPublishers>,
		ValueQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Data was received and processed from a publisher.
		DataProcessed {
			publisher: ParaId,
			key: Vec<u8>,
			value_size: u32,
		},
		/// A stored publisher root was cleared.
		PublisherRootCleared { publisher: ParaId },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Publisher root not found.
		PublisherRootNotFound,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Clear the stored root for a specific publisher.
		///
		/// This forces reprocessing of data from that publisher in the next block.
		/// Useful for recovery scenarios or when a specific publisher's data needs to be refreshed.
		///
		/// - `origin`: Must be root.
		/// - `publisher`: The ParaId of the publisher whose root should be cleared.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::clear_stored_roots())]
		pub fn clear_stored_roots(
			origin: OriginFor<T>,
			publisher: ParaId,
		) -> DispatchResult {
			ensure_root(origin)?;

			<PreviousPublishedDataRoots<T>>::try_mutate(|roots| -> DispatchResult {
				roots.remove(&publisher).ok_or(Error::<T>::PublisherRootNotFound)?;
				Ok(())
			})?;

			Self::deposit_event(Event::PublisherRootCleared { publisher });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Build relay proof requests from subscriptions.
		///
		/// Returns a `RelayProofRequest` with child trie proof requests for subscribed data.
		pub fn get_relay_proof_requests() -> cumulus_primitives_core::RelayProofRequest {
			let (subscriptions, _weight) = T::SubscriptionHandler::subscriptions();
			let storage_keys = subscriptions
				.into_iter()
				.flat_map(|(para_id, data_keys)| {
					let storage_key = Self::derive_storage_key(para_id);
				data_keys.into_iter().map(move |key| {
					cumulus_primitives_core::RelayStorageKey::Child {
						storage_key: storage_key.clone(),
						key,
					}
				})
				})
				.collect();

			cumulus_primitives_core::RelayProofRequest { keys: storage_keys }
		}

		/// Derives the child trie storage key for a publisher.
		///
		/// Uses the same encoding pattern as the broadcaster pallet:
		/// `(b"pubsub", para_id).encode()` to ensure compatibility.
		fn derive_storage_key(publisher_para_id: ParaId) -> Vec<u8> {
			use codec::Encode;
			(b"pubsub", publisher_para_id).encode()
		}

		fn derive_child_info(publisher_para_id: ParaId) -> sp_core::storage::ChildInfo {
			sp_core::storage::ChildInfo::new_default(&Self::derive_storage_key(publisher_para_id))
		}

		pub fn collect_publisher_roots(
			relay_state_proof: &RelayChainStateProof,
			subscriptions: &[(ParaId, Vec<Vec<u8>>)],
		) -> BTreeMap<ParaId, Vec<u8>> {
			subscriptions
				.iter()
				.take(T::MaxPublishers::get() as usize)
				.filter_map(|(publisher_para_id, _keys)| {
					let child_info = Self::derive_child_info(*publisher_para_id);
					let prefixed_key = child_info.prefixed_storage_key();

					relay_state_proof
						.read_optional_entry::<[u8; 32]>(&prefixed_key)
						.ok()
						.flatten()
						.map(|root_hash| (*publisher_para_id, root_hash.to_vec()))
				})
				.collect()
		}

		pub fn process_published_data(
			relay_state_proof: &RelayChainStateProof,
			current_roots: &BTreeMap<ParaId, Vec<u8>>,
			subscriptions: &[(ParaId, Vec<Vec<u8>>)],
		) -> (Weight, u32) {
			// Load roots from previous block for change detection.
			let previous_roots = <PreviousPublishedDataRoots<T>>::get();

			// Early exit if no publishers have any data.
			if current_roots.is_empty() && previous_roots.is_empty() {
				return (T::DbWeight::get().reads(1), 0);
			}

			let mut total_handler_weight = Weight::zero();
			let mut total_bytes_decoded = 0u32;

			// Process each subscription.
			for (publisher, subscription_keys) in subscriptions {
				// Check if publisher has published data in this block.
				if let Some(current_root) = current_roots.get(publisher) {
					// Detect if child trie root changed since last block.
					let should_update = previous_roots
						.get(publisher)
						.map_or(true, |prev_root| prev_root.as_slice() != current_root.as_slice());

					// Only process if data changed.
					if should_update {
						let child_info = Self::derive_child_info(*publisher);

						// Read each subscribed key from relay proof.
						for key in subscription_keys.iter() {
							match relay_state_proof.read_child_storage(&child_info, key) {
								Ok(Some(encoded_value)) => {
									let encoded_size = encoded_value.len() as u32;
									total_bytes_decoded = total_bytes_decoded.saturating_add(encoded_size);

									match Vec::<u8>::decode(&mut &encoded_value[..]) {
										Ok(value) => {
											let value_size = value.len() as u32;

											// Notify handler of new data.
											let handler_weight = T::SubscriptionHandler::on_data_updated(
												*publisher,
												key.clone(),
												value.clone(),
											);
											total_handler_weight = total_handler_weight.saturating_add(handler_weight);

											Self::deposit_event(Event::DataProcessed {
												publisher: *publisher,
												key: key.clone(),
												value_size,
											});
										},
										Err(_) => {
											defensive!("Failed to decode published data value");
										},
									}
								},
								Ok(None) => {
									// Key not published yet - expected.
								},
								Err(_) => {
									defensive!("Failed to read child storage from relay chain proof");
								},
							}
						}
					}
				}
			}

			// Store current roots for next block's comparison.
			let bounded_roots: BoundedBTreeMap<ParaId, BoundedVec<u8, ConstU32<32>>, T::MaxPublishers> =
				current_roots
					.iter()
					.filter_map(|(para_id, root)| {
						BoundedVec::try_from(root.clone()).ok().map(|bounded_root| (*para_id, bounded_root))
					})
					.collect::<BTreeMap<_, _>>()
					.try_into()
					.expect("MaxPublishers limit enforced in collect_publisher_roots; qed");
			<PreviousPublishedDataRoots<T>>::put(bounded_roots);

			(total_handler_weight, total_bytes_decoded)
		}
	}

	impl<T: Config> ProcessRelayProofKeys for Pallet<T> {
		/// Process child trie data from the relay proof.
		///
		/// Note: This implementation only processes child trie keys (pubsub data).
		/// Main trie keys in the proof are intentionally ignored.
		fn process_relay_proof_keys(verified_proof: &RelayChainStateProof) -> Weight {
			let (subscriptions, subscriptions_weight) = T::SubscriptionHandler::subscriptions();
			let num_publishers = subscriptions.len() as u32;
			let total_keys = subscriptions.iter().map(|(_, keys)| keys.len() as u32).sum();

			let current_roots = Self::collect_publisher_roots(verified_proof, &subscriptions);
			let (handler_weight, total_bytes_decoded) = Self::process_published_data(verified_proof, &current_roots, &subscriptions);

			// Return total weight for all operations
			subscriptions_weight
				.saturating_add(handler_weight)
				.saturating_add(T::WeightInfo::process_proof_excluding_handler(num_publishers, total_keys, total_bytes_decoded))
		}
	}
}
