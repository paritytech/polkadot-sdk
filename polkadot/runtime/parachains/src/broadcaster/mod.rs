// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! A pallet for managing parachain data publishing and subscription.
//!
//! This pallet provides a publish-subscribe mechanism for parachains to share data
//! efficiently through the relay chain storage using child tries per publisher.
//!
//! ## Storage Lifecycle
//!
//! Note: This pallet does not currently implement publisher removal or cleanup mechanisms.
//! Once a parachain publishes data, it remains in storage. Publishers can update their data
//! by publishing again, but there is no explicit removal path. 

use alloc::{collections::BTreeMap, vec::Vec};
use frame_support::{
	pallet_prelude::*,
	storage::child::ChildInfo,
	traits::{defensive_prelude::*, Get, ConstU32},
};
use frame_system::pallet_prelude::BlockNumberFor;
use polkadot_primitives::Id as ParaId;

pub use pallet::*;

pub mod runtime_api;
mod traits;
pub use traits::Publish;

#[cfg(test)]
mod tests;


#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Maximum number of items that can be published in one operation.
		/// Must not exceed `xcm::v5::MaxPublishItems`.
		#[pallet::constant]
		type MaxPublishItems: Get<u32>;

		/// Maximum length of a key in bytes.
		/// Must not exceed `xcm::v5::MaxPublishKeyLength`.
		#[pallet::constant]
		type MaxKeyLength: Get<u32>;

		/// Maximum length of a value in bytes.
		/// Must not exceed `xcm::v5::MaxPublishValueLength`.
		#[pallet::constant]
		type MaxValueLength: Get<u32>;

		/// Maximum number of unique keys a publisher can have stored across all publishes.
		#[pallet::constant]
		type MaxStoredKeys: Get<u32>;

		/// Maximum number of publishers that can have published data.
		#[pallet::constant]
		type MaxPublishers: Get<u32>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Data published by a parachain.
		DataPublished { publisher: ParaId, items_count: u32 },
	}

	/// Tracks which parachains have published data.
	///
	/// Maps parachain ID to a boolean indicating whether they have a child trie.
	/// The actual child trie info is derived deterministically from the ParaId.
	#[pallet::storage]
	pub type PublisherExists<T: Config> = StorageMap<
		_,
		Twox64Concat,
		ParaId,
		bool,
		ValueQuery,
	>;

	/// Tracks all published keys per parachain.
	#[pallet::storage]
	pub type PublishedKeys<T: Config> = StorageMap<
		_,
		Twox64Concat,
		ParaId,
		BoundedBTreeSet<BoundedVec<u8, T::MaxKeyLength>, T::MaxStoredKeys>,
		ValueQuery,
	>;

	/// Aggregated child trie roots for all publishers.
	///
	/// Contains (ParaId, child_trie_root) pairs for all parachains that have published data.
	/// This is used in relay chain storage proofs to efficiently provide all publisher roots.
	#[pallet::storage]
	pub type PublishedDataRoots<T: Config> = StorageValue<
		_,
		BoundedVec<(ParaId, BoundedVec<u8, ConstU32<32>>), T::MaxPublishers>,
		ValueQuery,
	>;

	#[pallet::error]
	pub enum Error<T> {
		/// Too many items in a single publish operation.
		TooManyPublishItems,
		/// Key length exceeds maximum allowed.
		KeyTooLong,
		/// Value length exceeds maximum allowed.
		ValueTooLong,
		/// Too many unique keys stored for this publisher.
		TooManyStoredKeys,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			assert_eq!(
				&PublishedDataRoots::<T>::hashed_key(),
				polkadot_primitives::well_known_keys::BROADCASTER_PUBLISHED_DATA_ROOTS,
				"`well_known_keys::BROADCASTER_PUBLISHED_DATA_ROOTS` doesn't match key of `PublishedDataRoots`! \
				Make sure that the name of the broadcaster pallet is `Broadcaster` in the runtime!",
			);

			assert!(
				T::MaxPublishItems::get() <= xcm::v5::MaxPublishItems::get(),
				"Broadcaster MaxPublishItems exceeds XCM MaxPublishItems upper bound"
			);
			assert!(
				T::MaxKeyLength::get() <= xcm::v5::MaxPublishKeyLength::get(),
				"Broadcaster MaxKeyLength exceeds XCM MaxPublishKeyLength upper bound"
			);
			assert!(
				T::MaxValueLength::get() <= xcm::v5::MaxPublishValueLength::get(),
				"Broadcaster MaxValueLength exceeds XCM MaxPublishValueLength upper bound"
			);
		}
	}

	impl<T: Config> Pallet<T> {
		/// Process a publish operation from a parachain.
		///
		/// Stores the provided key-value pairs in the publisher's child trie.
		pub fn handle_publish(
			origin_para_id: ParaId,
			data: Vec<(Vec<u8>, Vec<u8>)>,
		) -> DispatchResult {
			let items_count = data.len() as u32;

			// Validate input limits first before making any changes
			ensure!(
				data.len() <= T::MaxPublishItems::get() as usize,
				Error::<T>::TooManyPublishItems
			);

			// Validate all keys and values before creating publisher entry
			for (key, value) in &data {
				ensure!(
					key.len() <= T::MaxKeyLength::get() as usize,
					Error::<T>::KeyTooLong
				);
				ensure!(
					value.len() <= T::MaxValueLength::get() as usize,
					Error::<T>::ValueTooLong
				);
			}

			// All validation passed, now get or create child trie info for this publisher
			let child_info = Self::get_or_create_publisher_child_info(origin_para_id);

			// Get current published keys set for tracking
			let mut published_keys = PublishedKeys::<T>::get(origin_para_id);

			// Check if adding new keys would exceed MaxStoredKeys limit
			// Count how many unique new keys we're adding
			let mut new_keys_count = 0u32;
			for (key, _) in &data {
				if let Ok(bounded_key) = BoundedVec::try_from(key.clone()) {
					if !published_keys.contains(&bounded_key) {
						new_keys_count += 1;
					}
				}
			}

			// Ensure we won't exceed the total stored keys limit
			let current_keys_count = published_keys.len() as u32;
			ensure!(
				current_keys_count.saturating_add(new_keys_count) <= T::MaxStoredKeys::get(),
				Error::<T>::TooManyStoredKeys
			);

			// Store each key-value pair in the child trie and track the key
			for (key, value) in data {
				frame_support::storage::child::put(&child_info, &key, &value);

				// Track the key for enumeration (convert to BoundedVec)
				if let Ok(bounded_key) = BoundedVec::try_from(key) {
					// This should never fail now since we checked the limit above
					published_keys.try_insert(bounded_key).defensive_ok();
				}
			}

			// Update the published keys storage
			PublishedKeys::<T>::insert(origin_para_id, published_keys);

			// Calculate and update the child trie root for this publisher
			let child_root = frame_support::storage::child::root(&child_info,
				sp_runtime::StateVersion::V1);

			// Update the aggregated roots storage
			let mut roots = PublishedDataRoots::<T>::get();

			// Convert child_root once
			if let Ok(bounded_root) = BoundedVec::try_from(child_root) {
				// Find and update existing entry or add new one
				if let Some((_, root_hash)) = roots.iter_mut().find(|(para_id, _)| *para_id == origin_para_id) {
					*root_hash = bounded_root;
				} else {
					// Not found, add new entry
					roots.try_push((origin_para_id, bounded_root)).defensive_ok();
				}
			}

			PublishedDataRoots::<T>::put(roots);

			Self::deposit_event(Event::DataPublished { publisher: origin_para_id, items_count });

			Ok(())
		}

		/// Get the child trie root hash for a specific publisher.
		///
		/// This root is always included in PersistedValidationData to prove
		/// the current state of the publisher's data.
		pub fn get_publisher_child_root(para_id: ParaId) -> Option<Vec<u8>> {
			PublisherExists::<T>::get(para_id).then(|| {
				let child_info = Self::derive_child_info(para_id);
				frame_support::storage::child::root(&child_info, sp_runtime::StateVersion::V1)
			})
		}

		/// Get or create child trie info for a publisher.
		fn get_or_create_publisher_child_info(para_id: ParaId) -> ChildInfo {
			if !PublisherExists::<T>::contains_key(para_id) {
				PublisherExists::<T>::insert(para_id, true);
			}
			Self::derive_child_info(para_id)
		}

		/// Derive a deterministic child trie identifier from parachain ID.
		pub fn derive_child_info(para_id: ParaId) -> ChildInfo {
			const PREFIX: &[u8] = b"pubsub";
			let encoded = para_id.encode();
			let mut key = Vec::with_capacity(PREFIX.len() + encoded.len());
			key.extend_from_slice(PREFIX);
			key.extend_from_slice(&encoded);

			ChildInfo::new_default(&key)
		}

		/// Retrieve a value from a publisher's child trie.
		///
		/// Returns None if the publisher doesn't exist or the key is not found.
		pub fn get_published_value(para_id: ParaId, key: &[u8]) -> Option<Vec<u8>> {
			PublisherExists::<T>::get(para_id).then(|| {
				let child_info = Self::derive_child_info(para_id);
				frame_support::storage::child::get(&child_info, key)
			})?
		}

		/// Get all published data for a parachain.
		pub fn get_all_published_data(para_id: ParaId) -> Vec<(Vec<u8>, Vec<u8>)> {
			if !PublisherExists::<T>::get(para_id) {
				return Vec::new();
			}

			let child_info = Self::derive_child_info(para_id);
			let published_keys = PublishedKeys::<T>::get(para_id);

			published_keys
				.into_iter()
				.filter_map(|bounded_key| {
					let key: Vec<u8> = bounded_key.into();
					frame_support::storage::child::get(&child_info, &key)
						.map(|value| (key, value))
				})
				.collect()
		}

		/// Get list of all parachains that have published data.
		pub fn get_all_publishers() -> Vec<ParaId> {
			PublisherExists::<T>::iter_keys().collect()
		}
	}
}

// Implement Publish trait
impl<T: Config> Publish for Pallet<T> {
	fn publish_data(publisher: ParaId, data: Vec<(Vec<u8>, Vec<u8>)>) -> DispatchResult {
		Self::handle_publish(publisher, data)
	}
}