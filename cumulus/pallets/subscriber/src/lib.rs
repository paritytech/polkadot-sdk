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

//! # Subscriber Pallet
//!
//! Extracts and processes published data from relay chain child tries.
//!
//! ## Overview
//!
//! This pallet provides infrastructure for subscribing to data published by other parachains
//! via the relay chain broadcaster pallet. It:
//!
//! - Extracts child trie data from verified relay chain state proofs
//! - Tracks child trie roots for change detection optimization
//! - Delegates to consumer-defined handlers for processing received data
//!
//! The pallet is intentionally agnostic about:
//! - Which parachains to subscribe to
//! - Which keys to read
//! - What to do with the data
//!
//! These decisions are delegated to the `SubscriptionHandler` implementation provided via Config.
//!
//! ## Usage Example
//!
//! ```rust,ignore
//! // 1. Define your subscription handler
//! pub struct MyPriceOracleHandler;
//! impl cumulus_pallet_subscriber::SubscriptionHandler for MyPriceOracleHandler {
//!     fn subscriptions() -> Vec<(ParaId, Vec<Vec<u8>>)> {
//!         vec![(ParaId::from(2000), vec![b"btc_price".to_vec()])]
//!     }
//!
//!     fn on_data_updated(publisher: ParaId, key: Vec<u8>, value: Vec<u8>) {
//!         // Store price in your pallet
//!         PriceStorage::insert(key, value);
//!     }
//! }
//!
//! // 2. Configure pallet-subscriber
//! impl cumulus_pallet_subscriber::Config for Runtime {
//!     type SubscriptionHandler = MyPriceOracleHandler;
//!     type WeightInfo = ();
//! }
//!
//! // 3. Configure parachain-system to use subscriber
//! impl cumulus_pallet_parachain_system::Config for Runtime {
//!     // ... other config ...
//!     type ChildTrieProcessor = Subscriber;
//! }
//!
//! // 4. Implement runtime API
//! impl cumulus_primitives_core::KeyToIncludeInRelayProofApi<Block> for Runtime {
//!     fn child_trie_keys_to_prove() -> Vec<cumulus_primitives_core::ChildTrieProofRequest> {
//!         Subscriber::get_child_trie_proof_requests()
//!     }
//! }
//! ```

extern crate alloc;

use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use codec::Decode;
use cumulus_pallet_parachain_system::relay_state_snapshot::RelayChainStateProof;
use cumulus_primitives_core::ParaId;
use frame_support::{
	defensive,
	pallet_prelude::*,
	storage::bounded_btree_map::BoundedBTreeMap,
	traits::Get,
};
use frame_system::pallet_prelude::*;
use sp_std::vec;

pub use pallet::*;

// Re-export the trait from parachain-system for convenience
pub use cumulus_pallet_parachain_system::relay_state_snapshot::ProcessChildTrieData;

/// Trait for defining subscriptions and handling received data.
///
/// Consumer pallets implement this trait to specify:
/// - Which parachains and keys to subscribe to
/// - How to process received data
pub trait SubscriptionHandler {
	/// Returns the list of subscriptions.
	///
	/// Each tuple contains:
	/// - `ParaId`: The publisher parachain to subscribe to
	/// - `Vec<Vec<u8>>`: The keys to subscribe to (empty means all keys)
	fn subscriptions() -> Vec<(ParaId, Vec<Vec<u8>>)>;

	/// Called when subscribed data is updated.
	///
	/// # Parameters
	/// - `publisher`: The ParaId that published the data
	/// - `key`: The storage key that was updated
	/// - `value`: The new value (already decoded from SCALE)
	fn on_data_updated(publisher: ParaId, key: Vec<u8>, value: Vec<u8>);
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Handler that defines subscriptions and processes received data.
		type SubscriptionHandler: SubscriptionHandler;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	/// Child trie root hashes from the previous block, used for change detection.
	///
	/// Maps publisher ParaId to their child trie root hash (32 bytes).
	/// Only roots for subscribed publishers are tracked.
	/// Limited to 100 publishers max.
	#[pallet::storage]
	pub type PreviousPublishedDataRoots<T: Config> = StorageValue<
		_,
		BoundedBTreeMap<ParaId, BoundedVec<u8, ConstU32<32>>, ConstU32<100>>,
		ValueQuery,
	>;

	impl<T: Config> Pallet<T> {
		/// Get child trie proof requests for the collator.
		///
		/// This should be called from the runtime API `child_trie_keys_to_prove` implementation.
		/// It queries the subscription handler to build proof requests.
		pub fn get_child_trie_proof_requests(
		) -> Vec<cumulus_primitives_core::ChildTrieProofRequest> {
			T::SubscriptionHandler::subscriptions()
				.into_iter()
				.map(|(para_id, keys)| cumulus_primitives_core::ChildTrieProofRequest {
					child_trie_identifier: Self::derive_child_info(para_id)
						.storage_key()
						.to_vec(),
					data_keys: keys,
				})
				.collect()
		}

		/// Derives the child trie info for a publisher parachain.
		///
		/// Must match the broadcaster pallet's child trie structure.
		/// Uses the pubsub convention: `encode(b"pubsub", ParaId)`
		fn derive_child_info(publisher_para_id: ParaId) -> sp_storage::ChildInfo {
			use codec::Encode;
			sp_storage::ChildInfo::new_default(&(b"pubsub", publisher_para_id).encode())
		}

		/// Collects child trie root hashes for subscribed publishers from the relay chain state
		/// proof.
		///
		/// Returns pairs of (ParaId, root_hash) for change detection optimization.
		fn collect_publisher_roots(
			relay_state_proof: &RelayChainStateProof,
		) -> Vec<(ParaId, Vec<u8>)> {
			let subscriptions = T::SubscriptionHandler::subscriptions();

			subscriptions
				.into_iter()
				.filter_map(|(publisher_para_id, _keys)| {
					let child_info = Self::derive_child_info(publisher_para_id);
					let prefixed_key = child_info.prefixed_storage_key();

					relay_state_proof
						.read_optional_entry::<[u8; 32]>(&*prefixed_key)
						.ok()
						.flatten()
						.map(|root_hash| (publisher_para_id, root_hash.to_vec()))
				})
				.collect()
		}

		/// Process published data from the broadcaster pallet.
		///
		/// Reads data from child tries in the relay chain storage proof for subscribed publishers
		/// whose roots have changed. Uses child trie roots to detect changes between blocks.
		///
		/// Returns the weight consumed.
		fn process_published_data(
			relay_state_proof: &RelayChainStateProof,
			current_roots: &Vec<(ParaId, Vec<u8>)>,
		) -> Weight {
			let previous_roots = <PreviousPublishedDataRoots<T>>::get();

			if current_roots.is_empty() && previous_roots.is_empty() {
				return T::DbWeight::get().reads(1);
			}

			let mut p = 0u32;
			let mut k = 0u32;
			let mut v = 0u32;

			let current_roots_map: BTreeMap<ParaId, Vec<u8>> =
				current_roots.iter().map(|(para_id, root)| (*para_id, root.clone())).collect();

			let subscriptions = T::SubscriptionHandler::subscriptions();

			for (publisher, subscription_keys) in subscriptions {
				let should_update = match previous_roots.get(&publisher) {
					Some(prev_root) => match current_roots_map.get(&publisher) {
						Some(curr_root) if prev_root == curr_root => false,
						_ => true,
					},
					None => true,
				};

				if should_update && current_roots_map.contains_key(&publisher) {
					let child_info = Self::derive_child_info(publisher);

					for key in subscription_keys.iter() {
						match relay_state_proof.read_child_storage(&child_info, key) {
							Ok(Some(encoded_value)) => {
								match Vec::<u8>::decode(&mut &encoded_value[..]) {
									Ok(value) => {
										T::SubscriptionHandler::on_data_updated(
											publisher,
											key.clone(),
											value.clone(),
										);
										v = v.max(value.len() as u32);
										k += 1;
									},
									Err(_) => {
										defensive!("Failed to decode published data value");
									},
								}
							},
							Ok(None) => {
								// Key not published yet - expected
							},
							Err(_) => {
								defensive!("Failed to read child storage from relay chain proof");
							},
						}
					}

					p += 1;
				}
			}

			// Convert to BoundedBTreeMap with BoundedVec values
			let bounded_roots: BoundedBTreeMap<ParaId, BoundedVec<u8, ConstU32<32>>, ConstU32<100>> =
				current_roots_map
					.into_iter()
					.filter_map(|(para_id, root)| {
						BoundedVec::try_from(root).ok().map(|bounded_root| (para_id, bounded_root))
					})
					.collect::<BTreeMap<_, _>>()
					.try_into()
					.unwrap_or_default();
			<PreviousPublishedDataRoots<T>>::put(bounded_roots);

			T::WeightInfo::process_published_data(p, k, v)
		}
	}

	impl<T: Config> ProcessChildTrieData for Pallet<T> {
		fn process_child_trie_data(verified_proof: &RelayChainStateProof) -> Weight {
			let current_roots = Self::collect_publisher_roots(verified_proof);
			Self::process_published_data(verified_proof, &current_roots)
		}
	}
}

/// Weight functions needed for this pallet.
pub trait WeightInfo {
	fn process_published_data(p: u32, k: u32, v: u32) -> Weight;
}

/// Default weights implementation (placeholder).
impl WeightInfo for () {
	fn process_published_data(_p: u32, k: u32, v: u32) -> Weight {
		// Rough estimate based on operations:
		// - Reading previous roots: 1 read
		// - Reading child storage: k reads
		// - Writing updated roots: 1 write
		// - Decoding and processing: computation based on value size
		Weight::from_parts(10_000_000, 0)
			.saturating_add(Weight::from_parts(5_000 * k as u64, 0))
			.saturating_add(Weight::from_parts(100 * v as u64, 0))
			.saturating_add(frame_support::weights::constants::RocksDbWeight::get().reads(1 + k as u64))
			.saturating_add(frame_support::weights::constants::RocksDbWeight::get().writes(1))
	}
}
