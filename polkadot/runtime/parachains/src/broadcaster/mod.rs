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

use alloc::vec::Vec;
use frame_support::{
	pallet_prelude::*,
	storage::child::ChildInfo,
	traits::Get,
};
use polkadot_primitives::Id as ParaId;

pub use pallet::*;

pub mod runtime_api;

/// Trait for publishing key-value data.
/// Inspired by fungibles trait.
/// TODO: Check if we need to move out of pallets into a separate crate.
/// TODO: Check if sufficient level of abstraction, for now we will leave ParaId.
pub mod publish {
	use super::*;

	/// Trait for publishing key-value data for parachains.
	pub trait Publish {
		/// Publish key-value data for a specific parachain.
		fn publish_data(publisher: ParaId, data: Vec<(Vec<u8>, Vec<u8>)>) -> DispatchResult;
	}
}

#[cfg(test)]
mod tests;


#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Maximum number of items that can be published in one operation
		#[pallet::constant]
		type MaxPublishItems: Get<u32>;

		/// Maximum length of a key in bytes
		#[pallet::constant] 
		type MaxKeyLength: Get<u32>;

		/// Maximum length of a value in bytes
		#[pallet::constant]
		type MaxValueLength: Get<u32>;
	}

	/// Tracks which parachains have published data.
	///
	/// Maps parachain ID to a boolean indicating whether they have a child trie.
	/// The actual child trie info is derived deterministically from the ParaId.
	#[pallet::storage]
	pub type PublisherExists<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		ParaId,
		bool,
		ValueQuery,
	>;

	/// Tracks all published keys per parachain.
	#[pallet::storage]
	pub type PublishedKeys<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		ParaId,
		BoundedBTreeSet<BoundedVec<u8, T::MaxKeyLength>, T::MaxPublishItems>,
		ValueQuery,
	>;

	#[pallet::error]
	pub enum Error<T> {
		/// Too many items in a single publish operation
		TooManyPublishItems,
		/// Key length exceeds maximum allowed
		KeyTooLong,
		/// Value length exceeds maximum allowed  
		ValueTooLong,
		/// Child trie operation failed
		ChildTrieError,
	}

	impl<T: Config> Pallet<T> {
		/// Process a publish operation from a parachain.
		/// 
		/// Stores the provided key-value pairs in the publisher's child trie.
		pub fn handle_publish(
			origin_para_id: ParaId,
			data: Vec<(Vec<u8>, Vec<u8>)>,
		) -> DispatchResult {
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

			// Store each key-value pair in the child trie and track the key
			for (key, value) in data {
				frame_support::storage::child::put(&child_info, &key, &value);

				// Track the key for enumeration (convert to BoundedVec)
				if let Ok(bounded_key) = BoundedVec::try_from(key) {
					let _ = published_keys.try_insert(bounded_key);
				}
			}

			// Update the published keys storage
			PublishedKeys::<T>::insert(origin_para_id, published_keys);

			Ok(())
		}

		/// Get the child trie root hash for a specific publisher.
		/// 
		/// This root is always included in PersistedValidationData to prove
		/// the current state of the publisher's data.
		pub fn get_publisher_child_root(para_id: ParaId) -> Option<Vec<u8>> {
			if PublisherExists::<T>::get(para_id) {
				let child_info = Self::derive_child_info(para_id);
				Some(frame_support::storage::child::root(&child_info, 
					sp_runtime::StateVersion::V1))
			} else {
				None
			}
		}

		/// Get or create child trie info for a publisher.
		fn get_or_create_publisher_child_info(para_id: ParaId) -> ChildInfo {
			let child_info = Self::derive_child_info(para_id);
			PublisherExists::<T>::insert(para_id, true);
			child_info
		}

		/// Derive a deterministic child trie identifier from parachain ID.
		pub fn derive_child_info(para_id: ParaId) -> ChildInfo {
			let mut key = b"pubsub".to_vec();
			key.extend_from_slice(&para_id.encode());

			ChildInfo::new_default(&key)
		}

		/// Retrieve a value from a publisher's child trie.
		///
		/// Returns None if the publisher doesn't exist or the key is not found.
		pub fn get_published_value(para_id: ParaId, key: &[u8]) -> Option<Vec<u8>> {
			if PublisherExists::<T>::get(para_id) {
				let child_info = Self::derive_child_info(para_id);
				frame_support::storage::child::get(&child_info, key)
			} else {
				None
			}
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
	}
}

// Implement publish::Publish trait
impl<T: Config> publish::Publish for Pallet<T> {
	fn publish_data(publisher: ParaId, data: Vec<(Vec<u8>, Vec<u8>)>) -> DispatchResult {
		Self::handle_publish(publisher, data)
	}
}