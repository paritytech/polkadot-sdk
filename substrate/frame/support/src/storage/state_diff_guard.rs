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

// Feature gated since it can panic.
#![cfg(any(feature = "std", feature = "runtime-benchmarks", feature = "try-runtime", test))]

//! # Motivation
//!
//! In migrations and tests it is sometimes desirable to know and restrict which storage keys
//! change. It is also helpful to get the state difference, to ensure that there are no unexpected
//! storage changes. This module provides a guard that asserts every storage entry that has been
//! mutated is whitelisted.
//!
//! # How it works
//!
//! When the guard is instantiated, it takes the current storage state snapshot. When the guard is
//! dropped, it reads every storage entry and compares it with the snapshot to collect any storage
//! entry has been changed, removed or added. It then asserts that those mutations have the
//! one of the whitelisted prefixes.
//!
//! # Example
//!
//! Use the `StateDiffGuard` in a migration:
//!
//! ```rust
//! 
//! use frame_support::storage::generator::{StorageMap, StorageValue};
//! use frame_support::storage::StateDiffGuard;
//! use frame_support::traits::Get;
//! use sp_io::TestExternalities;
//!
//! #[frame_support::pallet]
//! mod pallet {
//! 	use frame_support::pallet_prelude::*;
//!
//! 	#[pallet::pallet]
//! 	pub struct Pallet<T>(_);
//!
//! 	#[pallet::call]
//! 	impl<T: Config> Pallet<T> {
//! 		#[pallet::weight(0)]
//! 		pub fn set_value(origin: OriginFor<T>, value: u32) -> DispatchResult {
//! 			<Value<T>>::put(value);
//! 			Ok(())
//! 		}
//! 	}
//!
//! 	#[pallet::storage]
//! 	pub type Value<T> = StorageValue<_, u32>;
//!     
//! 	#[pallet::storage]
//! 	pub type SomeMap<T> = StorageMap<_, Twox64Concat, u32, u32>;
//!     
//!     #[pallet::storage]
//! 	pub type SomeDoubleMap<T> = StorageMap<_, Twox64Concat, u32, u32>;
//! }
//!
//! mod migrations {
//! 	use super::*;
//!
//!     pub struct UncheckedMigrateToV1<T: crate::Config>(sp_std::marker::PhantomData<T>);
//!
//!     impl<T: crate::Config> frame_support::traits::UncheckedOnRuntimeUpgrade for UncheckedMigrateToV1<T> {
//!         fn on_runtime_upgrade() -> frame_support::weights::Weight {
//! 		   // migration logic here
//!            Weight::default()
//!         }
//!         
//!         #[cfg(feature = "try-runtime")]
//!         fn try_on_runtime_upgrade() -> Result<frame_support::weights::Weight, &'static str> {
//! 		   // migration logic here
//!           let guard = StateDiffGuard::builder()
//! 			.must_change(StoragePrefix {
//! 				pallet_name: SomeMap::<T>::pallet_prefix(),
//! 				storage_name: SomeMap::<T>::storage_prefix(),
//! 			})
//! 			.must_change(StoragePrefix {
//! 				pallet_name: SomeDoubleMap::<T>::pallet_prefix(),
//! 				storage_name: SomeDoubleMap::<T>::storage_prefix(),
//! 			})
//! 			.build();
//!
//!           // try runtime upgrade logic here
//! 		  let weight = Self::on_runtime_upgrade();
//!           
//!           // any other logic
//! 		  Ok(weight)
//! 	   }
//!     }
//!
//!     pub type MigrateToV1<T> = frame_support::migrations::VersionedMigration<
//!     	1,
//!      	2,
//!      	UncheckedMigrateToV1<T>,
//!      	Pallet<T>,
//! 		<T as frame_system::Config>::DbWeight
//!     >;
//! }
//! ```
//!
//! When the guard is dropped, it will assert that there are no unexpected storage changes.
//! Unexpected storage changes are the ones that are not whitelisted in the guard. In the example
//! above, the guard will panic if any storage entry that doesn't match `SomeMap` or `SomeDoubleMap`
//! prefixes is changed.

use core::fmt::{Debug, Formatter};

use sp_state_machine::{StorageKey, StorageValue};
use sp_std::collections::{btree_map::BTreeMap, btree_set::BTreeSet};

use super::storage_prefix;

/// Storage prefix: pallet name and storage name.
#[derive(Clone, Copy, Ord, PartialOrd, Eq, PartialEq)]
pub struct StoragePrefix {
	/// Name of the pallet.
	pub pallet_name: &'static [u8],
	/// Name of the storage.
	pub storage_name: &'static [u8],
}

impl StoragePrefix {
	/// Computes the storage prefix.
	pub fn storage_prefix(&self) -> [u8; 32] {
		storage_prefix(self.pallet_name, self.storage_name)
	}
}

impl Debug for StoragePrefix {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		write!(
			f,
			"{}::{}",
			sp_std::str::from_utf8(self.pallet_name).unwrap_or(""),
			sp_std::str::from_utf8(self.storage_name).unwrap_or("")
		)
	}
}

/// A guard that asserts that a specific storage prefix has been mutated or not.
#[derive(Default, Debug)]
pub struct StateDiffGuard {
	// Storage prefixes that are expected to change.
	whitelisted_prefixes: BTreeSet<StoragePrefix>,
	// Snapshot of the storage state at the beginning of the guard.
	initial_state: BTreeMap<StorageKey, StorageValue>,
}

impl StateDiffGuard {
	pub fn builder() -> StateDiffGuardBuilder {
		StateDiffGuardBuilder { whitelisted_prefixes: BTreeSet::new() }
	}

	/// Take a snapshot of the current storage state.
	fn read_state(&self) -> BTreeMap<StorageKey, StorageValue> {
		let mut state = BTreeMap::new();

		let mut previous_key = vec![];
		while let Some(next) = sp_io::storage::next_key(&previous_key) {
			// Ensure we are iterating through the correct prefix
			if !next.starts_with(&vec![]) {
				break;
			}
			if let Some(value) = sp_io::storage::get(&next) {
				state.insert(next.clone(), value.to_vec());
			}

			previous_key = next;
		}
		state
	}

	/// Reads state difference.
	///
	/// This includes:
	/// - all entries where the value has changed
	/// - new entries
	/// - removed entries
	fn read_difference(&self) -> BTreeMap<StorageKey, StorageValue> {
		let mut diff = BTreeMap::new();

		// start with an empty key
		let mut previous_key = vec![];
		let mut initial_state = self.initial_state.clone();

		while let Some(next) = sp_io::storage::next_key(&previous_key) {
			// Ensure we are iterating through the correct prefix
			if !next.starts_with(&vec![]) {
				break;
			}

			if let Some(value) = sp_io::storage::get(&next) {
				if let Some(old_value) = initial_state.remove(&next) {
					if value != old_value {
						diff.insert(next.clone(), value.to_vec());
					}
				} else {
					diff.insert(next.clone(), value.to_vec());
				}
			}

			previous_key = next;
		}

		// Add all remaining initial state to the diff
		for (key, value) in initial_state.iter() {
			diff.insert(key.clone(), value.to_vec());
		}

		diff
	}
}

/// Builder for the `StateDiffGuard`.
pub struct StateDiffGuardBuilder {
	whitelisted_prefixes: BTreeSet<StoragePrefix>,
}

impl StateDiffGuardBuilder {
	/// Add a storage prefix that should change.
	pub fn must_change(mut self, prefix: StoragePrefix) -> Self {
		// only add if a pallet level prefix is not already added, prevents from double iterating
		// storage
		self.whitelisted_prefixes.insert(prefix);

		self
	}

	/// Build the guard
	pub fn build(self) -> StateDiffGuard {
		let mut state_diff_guard = StateDiffGuard {
			whitelisted_prefixes: BTreeSet::new(),
			initial_state: BTreeMap::new(),
		};
		state_diff_guard.whitelisted_prefixes = self.whitelisted_prefixes;
		state_diff_guard.initial_state = state_diff_guard.read_state();

		sp_io::storage::start_transaction();

		state_diff_guard
	}
}

impl Drop for StateDiffGuard {
	fn drop(&mut self) {
		// this ensures that we read only difference from `initial_state`
		let diff = self.read_difference();
		let mut check_passed = true;
		for (key, value) in diff.iter() {
			let prefix_whitelisted = match key.get(0..32) {
				Some(key_prefix) => self
					.whitelisted_prefixes
					.iter()
					.any(|prefix| prefix.storage_prefix() == key_prefix),
				None => false,
			};

			if !prefix_whitelisted {
				check_passed = false;
				if let Some(old_value) = self.initial_state.get(key) {
					println!("+ {:?}:{:?}", key, value);
					println!("- {:?}:{:?}", key, old_value);
				} else if sp_io::storage::exists(&key) {
					println!("++ {:?}:{:?}", key, value);
				} else {
					println!("-- {:?}:{:?}", key, value);
				}
			}
		}

		// No need to double panic, eg. inside a test assertion failure.
		if sp_std::thread::panicking() {
			return
		}

		assert!(check_passed, "`StateDiffGuard` detected an unexpected storage change");
	}
}

#[cfg(test)]
mod tests {
	use crate::{Blake2_128Concat, Twox128};

	use super::*;
	use crate::storage::generator::{StorageDoubleMap, StorageMap, StorageValue};
	use frame_support_procedural::storage_alias;
	use sp_io::TestExternalities;

	#[test]
	#[should_panic(expected = "`StateDiffGuard` detected an unexpected storage change")]
	fn test_diff_guard_panic_works() {
		let mut ext = TestExternalities::default();
		ext.execute_with(|| {
			#[storage_alias]
			type TestMap = StorageMap<TestModule, Twox128, u32, u32>;
			#[storage_alias]
			type TestDoubleMapBlake2 =
				StorageDoubleMap<TestModule, Blake2_128Concat, u32, Blake2_128Concat, u32, u32>;
			#[storage_alias]
			type TestStorageValue = StorageValue<TestModule, u32>;

			TestMap::insert(1, 1);
			TestDoubleMapBlake2::insert(1, 1, 1);
			TestDoubleMapBlake2::insert(1, 2, 1);
			TestStorageValue::put(1);

			let guard = StateDiffGuard::builder()
				.must_change(StoragePrefix {
					pallet_name: TestDoubleMapBlake2::pallet_prefix(),
					storage_name: TestDoubleMapBlake2::storage_prefix(),
				})
				.build();

			TestDoubleMapBlake2::remove(1, 1);
			TestStorageValue::put(2);
			frame_support::storage::unhashed::put(b":CODE", b"value");
			TestMap::insert(2, 2);

			// this will panic because the storage keys are expected to change
			sp_std::mem::drop(guard);
		});
	}

	#[test]
	fn test_diff_guard_no_panic_works() {
		let mut ext = TestExternalities::default();
		ext.execute_with(|| {
			#[storage_alias]
			type TestMap = StorageMap<TestModule, Twox128, u32, u32>;
			#[storage_alias]
			type TestDoubleMapBlake2 =
				StorageDoubleMap<TestModule, Blake2_128Concat, u32, Blake2_128Concat, u32, u32>;
			#[storage_alias]
			type TestStorageValue = StorageValue<TestModule, u32>;

			TestMap::insert(1, 1);
			TestDoubleMapBlake2::insert(1, 1, 1);
			TestDoubleMapBlake2::insert(1, 2, 1);
			TestStorageValue::put(1);

			let guard = StateDiffGuard::builder()
				.must_change(StoragePrefix {
					pallet_name: TestDoubleMapBlake2::pallet_prefix(),
					storage_name: TestDoubleMapBlake2::storage_prefix(),
				})
				.build();

			TestDoubleMapBlake2::remove(1, 1);

			// this will not panic because the non-whitelisted storage keys are not expected to
			// change
			sp_std::mem::drop(guard);

			let _guard = StateDiffGuard::builder()
				.must_change(StoragePrefix {
					pallet_name: TestDoubleMapBlake2::pallet_prefix(),
					storage_name: TestDoubleMapBlake2::storage_prefix(),
				})
				.must_change(StoragePrefix {
					pallet_name: TestMap::pallet_prefix(),
					storage_name: TestMap::storage_prefix(),
				})
				.must_change(StoragePrefix {
					pallet_name: TestStorageValue::pallet_prefix(),
					storage_name: TestStorageValue::storage_prefix(),
				})
				.build();

			TestDoubleMapBlake2::remove(1, 2);
			TestStorageValue::put(2);
			TestMap::mutate(1, |v| *v = Some(2));
		});
	}
}
