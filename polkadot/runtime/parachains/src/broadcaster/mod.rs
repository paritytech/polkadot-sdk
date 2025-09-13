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
			// Validate input limits
			ensure!(
				data.len() <= T::MaxPublishItems::get() as usize,
				Error::<T>::TooManyPublishItems
			);

			// Get or create child trie info for this publisher
			let child_info = Self::get_or_create_publisher_child_info(origin_para_id);

			// Store each key-value pair in the child trie
			for (key, value) in data {
				ensure!(
					key.len() <= T::MaxKeyLength::get() as usize,
					Error::<T>::KeyTooLong
				);
				ensure!(
					value.len() <= T::MaxValueLength::get() as usize,
					Error::<T>::ValueTooLong
				);

				// Store in child trie
				frame_support::storage::child::put(&child_info, &key, &value);
			}

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
	}
}