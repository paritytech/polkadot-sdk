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

//! This pallet provides mechanisms for tracking and updating data related to the state root of an
//! external chain.
//!
//! The `T::Value` type represents state root-related data—for example, a `state_root` or
//! an entire `HeadData` structure.
//! The `T::Key` type serves as the identifier in the map where we store `T::Value`.
//! For example, it could be a `block_hash` or `block_number`.
//!
//! Example use cases:
//! 1. Store a `block_hash` → `state_root` mapping.
//! 2. Store a `block_number` → `HeadData` mapping.
//!
//! Root data is stored in a ring buffer, respecting the `T::RootsToKeep` limit.
//! When the limit is reached, the oldest entries are removed.
//!
//! There are two approaches for storing data:
//! 1. Send root data between chains using the dedicated extrinsic `fn note_new_roots(...)`.
//! 2. Implement an adapter (e.g., using the `OnSystemEvent` callback) that triggers
//!    `pallet_bridge_proof_root_store::Pallet::<T, I>::do_note_new_roots(...)`.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::collections::VecDeque;

pub use pallet::*;
pub mod weights;
pub use weights::WeightInfo;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking {
	// TODO: FAIL-CI
}
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	/// Configuration trait for the pallet.
	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// The origin allowed to submit head updates.
		type SubmitOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The key type used to identify stored values of type `T::Value`.
		type Key: Parameter + MaxEncodedLen;

		/// The type of the root value.
		type Value: Parameter + MaxEncodedLen;

		/// Maximum number of roots to retain in storage.
		/// This setting prevents unbounded growth of the on-chain state.
		#[pallet::constant]
		type RootsToKeep: Get<u32>;
	}

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	/// Storage for root-related data, bounded by `RootIndex` and `T::RootsToKeep`.
	#[pallet::storage]
	pub type Roots<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128Concat, T::Key, T::Value, OptionQuery>;

	/// Storage tracking the insertion order of roots for `T::RootsToKeep` (implemented as a simple ring buffer).
	#[pallet::storage]
	#[pallet::unbounded]
	pub type RootIndex<T: Config<I>, I: 'static = ()> =
		StorageValue<_, VecDeque<T::Key>, ValueQuery>;

	#[pallet::call(weight(<T as Config<I>>::WeightInfo))]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Records a new root data.
		#[pallet::call_index(0)]
		pub fn note_new_roots(
			origin: OriginFor<T>,
			roots: BoundedVec<(T::Key, T::Value), T::RootsToKeep>,
		) -> DispatchResult {
			let _ = T::SubmitOrigin::ensure_origin(origin);
			Self::do_note_new_roots(roots);
			Ok(())
		}
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		#[cfg(feature = "try-runtime")]
		fn try_state(
			_n: BlockNumberFor<T>,
		) -> Result<(), frame_support::sp_runtime::TryRuntimeError> {
			Self::do_try_state()
		}
	}

	#[cfg(any(feature = "try-runtime", test))]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Ensure the correctness of the state of this pallet.
		pub fn do_try_state() -> Result<(), frame_support::sp_runtime::TryRuntimeError> {
			// Check that `RootIndex` is aligned with `Roots`.
			let index = RootIndex::<T, I>::get();
			ensure!(
				index.len() == Roots::<T, I>::iter_keys().count(),
				"`RootIndex` contains different keys than `Roots`"
			);
			for key in index {
				ensure!(
					Roots::<T, I>::get(key).is_some(),
					"`Roots` does not contain key from `RootIndex`!"
				);
			}

			Ok(())
		}
	}

	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Stores root values.
		pub fn do_note_new_roots(roots: BoundedVec<(T::Key, T::Value), T::RootsToKeep>) {
			// Update `RootIndex` ring buffer.
			let (to_add, to_remove) = RootIndex::<T, I>::mutate(|index| {
				let mut to_add = alloc::vec::Vec::with_capacity(roots.len());
				let mut to_remove = alloc::vec::Vec::with_capacity(roots.len());
				let max = T::RootsToKeep::get();

				// Add all at the end.
				for (key, value) in roots {
					if !index.contains(&key) {
						to_add.push((key.clone(), value));
						index.push_back(key);
					}
				}

				// Remove from the front up to the `T::RootsToKeep` limit.
				while index.len() > (max as usize) {
					if let Some(key_to_remove) = index.pop_front() {
						to_remove.push(key_to_remove);
					}
				}

				(to_add, to_remove)
			});

			// Add new ones to the `Roots` (aligned with `RootIndex`).
			for (key, value) in to_add {
				Roots::<T, I>::insert(key, value);
			}

			// Remove from `Roots` (aligned with `RootIndex`).
			for key in to_remove {
				Roots::<T, I>::remove(key);
			}
		}

		/// Returns the stored value for the given key.
		pub fn get_root(key: &T::Key) -> Option<T::Value> {
			Roots::<T, I>::get(key)
		}

		/// Returns the stored `RootIndex` data.
		pub fn get_root_index() -> VecDeque<T::Key> {
			RootIndex::<T, I>::get()
		}
	}
}
