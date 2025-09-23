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
	traits::{Get, ConstU32},
};
use frame_system::pallet_prelude::BlockNumberFor;
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

		/// Toggle subscription to a publisher's data.
		fn toggle_subscription(subscriber: ParaId, publisher: ParaId) -> DispatchResult;
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

		/// Maximum number of publishers a subscriber can subscribe to
		#[pallet::constant]
		type MaxSubscriptions: Get<u32>;
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

	/// Tracks subscriptions: subscriber -> list of publishers.
	///
	/// Maps subscriber ParaId to a bounded vector of publisher ParaIds.
	/// Empty vec means no subscriptions.
	#[pallet::storage]
	pub type Subscriptions<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		ParaId,  // Subscriber
		BoundedVec<ParaId, T::MaxSubscriptions>,  // List of publishers
		ValueQuery,
	>;

	/// Aggregated child trie roots for all publishers.
	///
	/// Contains (ParaId, child_trie_root) pairs for all parachains that have published data.
	/// This is used in relay chain storage proofs to efficiently provide all publisher roots.
	#[pallet::storage]
	pub type PublishedDataRoots<T: Config> = StorageValue<
		_,
		BoundedVec<(ParaId, BoundedVec<u8, ConstU32<32>>), ConstU32<1000>>,
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
		/// Too many subscriptions for this subscriber
		TooManySubscriptions,
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
			log::info!(
				target: "broadcaster::publish",
				"📡 Publishing data from parachain {:?}: {} items",
				origin_para_id,
				data.len()
			);

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
				log::debug!(
					target: "broadcaster::publish",
					"📝 Storing key {:?} ({}B) -> value ({}B) for parachain {:?}",
					key,
					key.len(),
					value.len(),
					origin_para_id
				);

				frame_support::storage::child::put(&child_info, &key, &value);

				// Track the key for enumeration (convert to BoundedVec)
				if let Ok(bounded_key) = BoundedVec::try_from(key) {
					let _ = published_keys.try_insert(bounded_key);
				}
			}

			// Update the published keys storage
			PublishedKeys::<T>::insert(origin_para_id, published_keys);

			// Calculate and update the child trie root for this publisher
			let child_root = frame_support::storage::child::root(&child_info,
				sp_runtime::StateVersion::V1);

			// Update the aggregated roots storage
			let mut roots = PublishedDataRoots::<T>::get();

			// Find and update existing entry or add new one
			let mut found = false;
			for (para_id, root_hash) in roots.iter_mut() {
				if *para_id == origin_para_id {
					if let Ok(bounded_root) = BoundedVec::try_from(child_root.clone()) {
						*root_hash = bounded_root;
					}
					found = true;
					break;
				}
			}

			// If not found, add new entry
			if !found {
				if let Ok(bounded_root) = BoundedVec::try_from(child_root.clone()) {
					let _ = roots.try_push((origin_para_id, bounded_root));
				}
			}

			PublishedDataRoots::<T>::put(roots);

			log::info!(
				target: "broadcaster::publish",
				"✅ Successfully published data for parachain {:?}, total keys: {}, root: {:?}",
				origin_para_id,
				PublishedKeys::<T>::get(origin_para_id).len(),
				child_root
			);

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
			log::debug!(
				target: "broadcaster::query",
				"🔍 get_all_published_data() called for parachain {:?}",
				para_id
			);

			if !PublisherExists::<T>::get(para_id) {
				log::debug!(
					target: "broadcaster::query",
					"❌ Parachain {:?} has no published data",
					para_id
				);
				return Vec::new();
			}

			let child_info = Self::derive_child_info(para_id);
			let published_keys = PublishedKeys::<T>::get(para_id);

			let data: Vec<(Vec<u8>, Vec<u8>)> = published_keys
				.into_iter()
				.filter_map(|bounded_key| {
					let key: Vec<u8> = bounded_key.into();
					frame_support::storage::child::get(&child_info, &key)
						.map(|value| (key, value))
				})
				.collect();

			log::debug!(
				target: "broadcaster::query",
				"📦 Returning {} data items for parachain {:?}",
				data.len(),
				para_id
			);

			data
		}

		/// Get list of all parachains that have published data.
		pub fn get_all_publishers() -> Vec<ParaId> {
			let publishers: Vec<ParaId> = PublisherExists::<T>::iter()
				.filter_map(|(para_id, exists)| if exists { Some(para_id) } else { None })
				.collect();

			log::debug!(
				target: "broadcaster::query",
				"🔍 get_all_publishers() returning {} publishers: {:?}",
				publishers.len(),
				publishers
			);

			publishers
		}

		/// Toggle subscription: subscribe if not subscribed, unsubscribe if subscribed.
		pub fn handle_subscribe_toggle(
			subscriber: ParaId,
			publisher: ParaId,
		) -> DispatchResult {
			log::info!(
				target: "broadcaster::subscribe",
				"🔄 Toggle subscription for parachain {:?} to publisher {:?}",
				subscriber,
				publisher
			);

			let mut subscriptions = Subscriptions::<T>::get(subscriber);

			// Check if already subscribed
			if let Some(pos) = subscriptions.iter().position(|&p| p == publisher) {
				// Already subscribed -> unsubscribe
				subscriptions.swap_remove(pos);
				log::debug!(
					target: "broadcaster::subscribe",
					"❌ Unsubscribed: {:?} from {:?}",
					subscriber,
					publisher
				);
			} else {
				// Not subscribed -> subscribe
				subscriptions.try_push(publisher).map_err(|_| Error::<T>::TooManySubscriptions)?;
				log::debug!(
					target: "broadcaster::subscribe",
					"✅ Subscribed: {:?} to {:?}",
					subscriber,
					publisher
				);
			}

			Subscriptions::<T>::insert(subscriber, subscriptions);
			Ok(())
		}

		/// Get all subscriptions for a parachain.
		pub fn get_subscriptions(subscriber: ParaId) -> Vec<ParaId> {
			Subscriptions::<T>::get(subscriber).into_inner()
		}

		/// Check if a parachain is subscribed to a publisher.
		pub fn is_subscribed(subscriber: ParaId, publisher: ParaId) -> bool {
			Subscriptions::<T>::get(subscriber).contains(&publisher)
		}
	}
}

// Implement publish::Publish trait
impl<T: Config> publish::Publish for Pallet<T> {
	fn publish_data(publisher: ParaId, data: Vec<(Vec<u8>, Vec<u8>)>) -> DispatchResult {
		Self::handle_publish(publisher, data)
	}

	fn toggle_subscription(subscriber: ParaId, publisher: ParaId) -> DispatchResult {
		Self::handle_subscribe_toggle(subscriber, publisher)
	}
}