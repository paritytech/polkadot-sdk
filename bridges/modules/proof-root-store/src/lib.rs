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

		/// The origin allowed submitting head updates.
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

	/// Current ring buffer position.
	#[pallet::storage]
	pub(super) type RootKeysPointer<T: Config<I>, I: 'static = ()> =
		StorageValue<_, u32, ValueQuery>;

	/// A ring buffer of imported keys. Ordered by the insertion time.
	#[pallet::storage]
	pub(super) type RootKeys<T: Config<I>, I: 'static = ()> = StorageMap<
		Hasher = Identity,
		Key = u32,
		Value = T::Key,
		QueryKind = OptionQuery,
		OnEmpty = GetDefault,
		MaxValues = MaybeRootsToKeep<T, I>,
	>;

	/// Storage for root-related k-v data, bounded by `RootKeysPointer+RootKeys` ring buffer.
	#[pallet::storage]
	pub type Roots<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128Concat, T::Key, T::Value, OptionQuery>;

	/// Adapter for using `Config::RootsToKeep` as `MaxValues` bound in our storage maps.
	pub struct MaybeRootsToKeep<T, I>(PhantomData<(T, I)>);
	impl<T: Config<I>, I: 'static> Get<Option<u32>> for MaybeRootsToKeep<T, I> {
		fn get() -> Option<u32> {
			Some(T::RootsToKeep::get())
		}
	}

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
			// Check that the ring buffer is aligned with `Roots`.
			ensure!(
				RootKeys::<T, I>::iter_values().count() == Roots::<T, I>::iter_keys().count(),
				"`RootIndex` contains different keys than `Roots`"
			);
			for key in RootKeys::<T, I>::iter_values() {
				ensure!(
					Roots::<T, I>::get(key).is_some(),
					"`Roots` does not contain the key from `RootKeys`!"
				);
			}

			Ok(())
		}
	}

	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Stores root values.
		pub fn do_note_new_roots(roots: BoundedVec<(T::Key, T::Value), T::RootsToKeep>) {
			// Insert `roots` to the `Roots` bounded by `RootKeysPointer+RootKeys`.
			for (key, value) in roots {
				let index = <RootKeysPointer<T, I>>::get();
				let pruning = <RootKeys<T, I>>::try_get(index);

				<Roots<T, I>>::insert(&key, value);
				<RootKeys<T, I>>::insert(index, key);

				// Update ring buffer pointer and remove old root.
				<RootKeysPointer<T, I>>::put((index + 1) % T::RootsToKeep::get());
				if let Ok(key_to_prune) = pruning {
					// log::debug!(target: LOG_TARGET, "Pruning old header: {:?}.", key_to_prune);
					<Roots<T, I>>::remove(key_to_prune);
				}
			}
		}

		/// Returns the stored value for the given key.
		pub fn get_root(key: &T::Key) -> Option<T::Value> {
			Roots::<T, I>::get(key)
		}

		/// Returns the stored root keys.
		#[cfg(any(feature = "std", feature = "runtime-benchmarks", test))]
		pub fn get_root_keys() -> Vec<T::Key> {
			Roots::<T, I>::iter_keys().collect()
		}
	}
}
