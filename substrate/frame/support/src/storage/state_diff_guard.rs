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

//! WARNING: This code is experimental and its API might change.
//!
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
//! use frame_support::storage::generator::{StorageMap, StorageValue};
//! use frame_support::storage::state_diff_guard::StateDiffGuard;
//! use frame_support::traits::Get;
//! use frame_support::pallet_prelude::*;
//! use sp_io::TestExternalities;
//!
//! pub use pallet::*;
//!
//! #[frame_support::pallet]
//! pub mod pallet {
//! 	use frame_support::pallet_prelude::*;
//! 	use frame_system::pallet_prelude::*;
//!
//! 	#[pallet::config]
//! 	pub trait Config: frame_system::Config {}
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
//! pub struct UncheckedMigrateToV1<T: Config>(sp_std::marker::PhantomData<T>);
//!
//! impl<T: Config> frame_support::traits::UncheckedOnRuntimeUpgrade for UncheckedMigrateToV1<T> {
//!     fn on_runtime_upgrade() -> frame_support::weights::Weight {
//!         // migration logic here
//!         Weight::default()
//!     }
//!
//!     #[cfg(feature = "try-runtime")]
//!     fn try_on_runtime_upgrade() -> Result<frame_support::weights::Weight, &'static str> {
//!         // migration logic here
//!         let _guard = StateDiffGuard::builder()
//!             .must_change_if_exists(SomeMap::<T>::storage_info())
//!             .must_not_change(SomeDoubleMap::<T>::storage_info())
//! 			.can_not_change(GuardSubject::AnythingElse)
//!             .build();
//!
//!         // try runtime upgrade logic here
//!         let weight = Self::on_runtime_upgrade();
//!
//!         // any other logic
//!         Ok(weight)
//!     }
//! }
//!
//! pub type MigrateToV1<T> = frame_support::migrations::VersionedMigration<
//!     1,
//!     2,
//!     UncheckedMigrateToV1<T>,
//!     Pallet<T>,
//!     <T as frame_system::Config>::DbWeight
//! >;
//! ```
//!
//! In the example above, the guard will panic if any storage entry that doesn't match `SomeMap` or
//! `SomeDoubleMap` prefixes is changed.

use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use array_bytes::bytes2hex;
use core::fmt::Debug;

use crate::traits::StorageInfo;

/// Raw storage key.
type StorageKey = Vec<u8>;
/// Raw storage value.
type StorageValue = Vec<u8>;
/// Raw storage state.
/// BTreeMap is used to ensure that the keys are sorted.
type State = BTreeMap<StorageKey, StorageValue>;

/// Mutation policy for a storage prefix.
#[derive(Debug)]
pub enum MutationPolicy {
	/// The storage prefix is expected to change.
	CanChange,
	/// The storage prefix must change if it already existed prior to the guard.
	MustChangeIfExists,
	/// The storage prefix must not change.
	MustNotChange,
}

/// Guard subject.
#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Clone)]
pub enum GuardSubject {
	/// Explicit storage prefix to guard.
	Prefix(Vec<u8>),
	/// Any storage prefix.
	AnythingElse,
}

impl GuardSubject {
	/// Get the raw prefix.
	fn raw_prefix(&self) -> Option<&Vec<u8>> {
		match self {
			GuardSubject::Prefix(prefix) => Some(prefix.as_ref()),
			GuardSubject::AnythingElse => None,
		}
	}
}

/// Wrapper around a `Vec<GuardSubject>` so that we can implement conversion traits on it.
pub struct GuardSubjectCollection(pub Vec<GuardSubject>);

impl From<Vec<StorageInfo>> for GuardSubjectCollection {
	fn from(storage_info: Vec<StorageInfo>) -> Self {
		GuardSubjectCollection(
			storage_info.into_iter().map(|s| GuardSubject::Prefix(s.prefix)).collect(),
		)
	}
}

impl From<GuardSubject> for GuardSubjectCollection {
	fn from(subject: GuardSubject) -> Self {
		GuardSubjectCollection(vec![subject])
	}
}

/// A guard that asserts that a specific storage prefix has been mutated or not.
#[derive(Default, Debug)]
pub struct StateDiffGuard {
	/// Storage prefixes that are expected to change.
	/// Default for `AnythingElse` is `Must(NotChange)`.
	mutation_policy: BTreeMap<GuardSubject, MutationPolicy>,
	/// Snapshot of the storage state when the guard is created.
	initial_state: State,
}

impl StateDiffGuard {
	pub fn builder() -> StateDiffGuardBuilder {
		StateDiffGuardBuilder { mutation_policy: BTreeMap::new() }
	}

	/// Take a snapshot of the current storage state.
	fn read_state(&self) -> State {
		let mut state = BTreeMap::new();

		let mut previous_key = Vec::new();
		while let Some(next) = sp_io::storage::next_key(&previous_key) {
			if let Some(value) = sp_io::storage::get(&next) {
				state.insert(next.clone(), value.to_vec());
			}

			previous_key = next;
		}
		state
	}

	/// Given the storage key, get it's prefix mutation policy.
	fn prefix_mutation_policy(&self, key: &StorageKey) -> Option<&MutationPolicy> {
		let guard_anything_else = self.mutation_policy.get(&GuardSubject::AnythingElse);
		self.mutation_policy
			.iter()
			.find(|(info, _)| info.raw_prefix().map_or(false, |prefix| key.starts_with(prefix)))
			.map_or(guard_anything_else, |(_, policy)| Some(policy))
	}

	/// Apply the mutation policy to the current storage state.
	///
	/// Returns `true` if all the mutation policies are satisfied, `false` otherwise.
	///
	/// The first iteration compares initial and current value of the storage keys by taking
	/// the initial value from the initial state. This way, once the iteration is done, the
	/// initial state will only contain the keys that were removed.
	///
	/// Then for each removed key, it applies the mutation policy again.
	fn apply_mutation_policy(&mut self) -> bool {
		let mut previous_key = Vec::new();

		// check has passed
		let mut check_passed = true;

		while let Some(next) = sp_io::storage::next_key(&previous_key) {
			previous_key = next.clone();
			let Some(value) = sp_io::storage::get(&next) else { continue };
			let (maybe_old_value, value) = (self.initial_state.remove(&next), value);

			let Some(policy) = self.prefix_mutation_policy(&next) else {
				continue;
			};

			match policy {
				MutationPolicy::CanChange => {
					// expected to change, no need to check anything
				},
				MutationPolicy::MustNotChange =>
					if maybe_old_value != Some(value.to_vec()) {
						check_passed = false;
						log::error!(
							"Storage value for key must not have been changed, but it is {:?} -> {:?}",
							bytes2hex("0x", &next),
							bytes2hex("0x", value),
						);
					},
				MutationPolicy::MustChangeIfExists =>
					if maybe_old_value == Some(value.to_vec()) {
						check_passed = false;
						log::error!(
							"Storage value for key must have been changed, but it is not {:?} -> {:?}",
							bytes2hex("0x", &next),
							bytes2hex("0x", value),
						);
					},
			}
		}

		// if there are any keys left in initial state, it means that they were removed
		for (key, _) in self.initial_state.iter() {
			let Some(policy) = self.prefix_mutation_policy(key) else { continue };

			match policy {
				MutationPolicy::CanChange | MutationPolicy::MustChangeIfExists => {
					// expected to change, no need to check anything
				},
				MutationPolicy::MustNotChange => {
					check_passed = false;
					log::error!(
						"Storage key must not have been removed, but it is {:?}",
						bytes2hex("0x", key)
					);
				},
			}
		}

		check_passed
	}
}

/// Builder for the `StateDiffGuard`.
pub struct StateDiffGuardBuilder {
	mutation_policy: BTreeMap<GuardSubject, MutationPolicy>,
}

impl StateDiffGuardBuilder {
	/// Add a storage prefix that must change if it already exited prior to the guard.
	pub fn must_change_if_exists<S: Into<GuardSubjectCollection>>(mut self, prefixes: S) -> Self {
		for prefix in prefixes.into().0 {
			self.mutation_policy.insert(prefix, MutationPolicy::MustChangeIfExists);
		}

		self
	}

	/// Add a storage prefix that must not change.
	pub fn must_not_change<S: Into<GuardSubjectCollection>>(mut self, prefixes: S) -> Self {
		for prefix in prefixes.into().0 {
			self.mutation_policy.insert(prefix, MutationPolicy::MustNotChange);
		}

		self
	}

	/// Add a storage prefix that can change.
	pub fn can_change<S: Into<GuardSubjectCollection>>(mut self, prefixes: S) -> Self {
		for prefix in prefixes.into().0 {
			self.mutation_policy.insert(prefix, MutationPolicy::CanChange);
		}

		self
	}

	/// Build the guard
	pub fn build(self) -> StateDiffGuard {
		let mut state_diff_guard = StateDiffGuard {
			mutation_policy: self.mutation_policy,
			initial_state: BTreeMap::new(),
		};
		state_diff_guard.initial_state = state_diff_guard.read_state();

		state_diff_guard
	}
}

impl Drop for StateDiffGuard {
	fn drop(&mut self) {
		let check_passed = self.apply_mutation_policy();

		// No need to double panic, eg. inside a test assertion failure.
		#[cfg(feature = "std")]
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
	#[allow(unused_imports)]
	use crate::storage::generator::{StorageDoubleMap, StorageMap, StorageValue};
	use frame_support::traits::StorageInfoTrait;
	use frame_support_procedural::storage_alias;
	use sp_io::TestExternalities;

	#[storage_alias]
	type TestMap = StorageMap<TestModule, Twox128, u32, u32>;
	#[storage_alias]
	type TestDoubleMapBlake2 =
		StorageDoubleMap<TestModule, Blake2_128Concat, u32, Blake2_128Concat, u32, u32>;
	#[storage_alias]
	type TestStorageValue = StorageValue<TestModule, u32>;

	#[test]
	fn diff_guard_default_works() {
		TestExternalities::default().execute_with(|| {
			let _guard = StateDiffGuard::builder().build();

			TestMap::insert(1, 1);
		});
	}

	#[test]
	#[should_panic(expected = "`StateDiffGuard` detected an unexpected storage change")]
	fn diff_guard_anything_else_works() {
		TestExternalities::default().execute_with(|| {
			TestMap::insert(1, 1);
			TestDoubleMapBlake2::insert(1, 1, 1);
			TestStorageValue::put(1);

			let _guard =
				StateDiffGuard::builder().must_not_change(GuardSubject::AnythingElse).build();

			TestMap::insert(1, 2);
			TestDoubleMapBlake2::insert(1, 1, 2);
			TestStorageValue::put(2);
		});
	}

	#[test]
	#[should_panic(expected = "`StateDiffGuard` detected an unexpected storage change")]
	fn guard_storage_key_types_works() {
		TestExternalities::default().execute_with(|| {
			let _guard = StateDiffGuard::builder()
				.must_not_change(TestDoubleMapBlake2::storage_info())
				.build();

			mod v2 {
				use super::*;

				#[storage_alias]
				pub type TestMap = StorageMap<TestModule, Twox128, u128, u32>;
				#[storage_alias]
				pub type TestDoubleMapBlake2 = StorageDoubleMap<
					TestModule,
					Blake2_128Concat,
					u128,
					Blake2_128Concat,
					u64,
					u32,
				>;
			}

			v2::TestMap::insert(1, 1);
			v2::TestDoubleMapBlake2::insert(1, 1, 12);
		});
	}

	#[test]
	#[should_panic(expected = "`StateDiffGuard` detected an unexpected storage change")]
	fn diff_guard_basic_panic_works() {
		TestExternalities::default().execute_with(|| {
			TestDoubleMapBlake2::insert(1, 1, 1);
			TestDoubleMapBlake2::insert(1, 2, 1);
			TestStorageValue::put(1);

			let _guard = StateDiffGuard::builder()
				.can_change(TestDoubleMapBlake2::storage_info())
				.must_not_change(TestMap::storage_info())
				.build();

			TestDoubleMapBlake2::remove(1, 1);
			TestMap::insert(1, 1);

			// this will panic because by default any other whitelisted prefix is not expected to
		});
	}

	#[test]
	#[should_panic(expected = "`StateDiffGuard` detected an unexpected storage change")]
	fn diff_guard_basic_must_change_if_exists() {
		TestExternalities::default().execute_with(|| {
			TestDoubleMapBlake2::insert(1, 1, 1);
			TestDoubleMapBlake2::insert(1, 1, 2);
			TestStorageValue::put(1);

			// must change all entries of `TestDoubleMapBlake2`
			let _guard = StateDiffGuard::builder()
				.must_change_if_exists(TestDoubleMapBlake2::storage_info())
				.can_change(TestMap::storage_info())
				.build();

			TestDoubleMapBlake2::remove(1, 2);
			TestMap::insert(1, 1);
		});
	}

	#[test]
	fn test_diff_guard_no_panic_works() {
		let mut ext = TestExternalities::default();
		ext.execute_with(|| {
			TestMap::insert(1, 1);
			TestDoubleMapBlake2::insert(1, 1, 1);
			TestDoubleMapBlake2::insert(1, 2, 1);
			TestStorageValue::put(1);

			let _guard = StateDiffGuard::builder()
				.must_change_if_exists(TestDoubleMapBlake2::storage_info());
		});
	}

	#[test]
	#[should_panic(expected = "`StateDiffGuard` detected an unexpected storage change")]
	fn test_diff_guard_must_change_if_existed_errors() {
		let mut ext = TestExternalities::default();
		ext.execute_with(|| {
			TestStorageValue::put(1);
			let _guard = StateDiffGuard::builder()
				.must_change_if_exists(TestStorageValue::storage_info())
				.build();
		});
	}

	#[test]
	fn test_diff_guard_must_change_if_existed_works_on_change() {
		let mut ext = TestExternalities::default();
		ext.execute_with(|| {
			TestStorageValue::put(1);
			let _guard = StateDiffGuard::builder()
				.must_change_if_exists(TestStorageValue::storage_info())
				.build();

			TestStorageValue::put(2);
		});
	}

	#[test]
	fn test_diff_guard_must_change_if_existed_works_if_not_existed() {
		let mut ext = TestExternalities::default();
		ext.execute_with(|| {
			let _guard = StateDiffGuard::builder()
				.must_change_if_exists(TestStorageValue::storage_info())
				.build();

			TestStorageValue::put(2);
		});
	}

	#[test]
	fn test_diff_guard_must_change_if_existed_works_if_not_existed_or_crated() {
		let mut ext = TestExternalities::default();
		ext.execute_with(|| {
			let _guard = StateDiffGuard::builder()
				.must_change_if_exists(TestStorageValue::storage_info())
				.build();
		});
	}

	#[test]
	fn test_diff_guard_can_change_works() {
		let mut ext = TestExternalities::default();
		ext.execute_with(|| {
			let _guard =
				StateDiffGuard::builder().can_change(TestStorageValue::storage_info()).build();
		});
	}

	#[test]
	fn test_diff_guard_can_change_works_on_change() {
		let mut ext = TestExternalities::default();
		ext.execute_with(|| {
			TestStorageValue::put(1);

			let _guard =
				StateDiffGuard::builder().can_change(TestStorageValue::storage_info()).build();
		});
	}

	#[test]
	fn test_diff_guard_can_change_works_on_change_if_exited() {
		let mut ext = TestExternalities::default();
		ext.execute_with(|| {
			TestStorageValue::put(1);

			let _guard =
				StateDiffGuard::builder().can_change(TestStorageValue::storage_info()).build();

			TestStorageValue::put(2);
		});
	}

	#[test]
	fn test_diff_guard_can_change_works_if_not_existed() {
		let mut ext = TestExternalities::default();
		ext.execute_with(|| {
			let _guard =
				StateDiffGuard::builder().can_change(TestStorageValue::storage_info()).build();

			TestStorageValue::put(2);
		});
	}

	#[test]
	fn test_diff_guard_must_not_change_works() {
		let mut ext = TestExternalities::default();
		ext.execute_with(|| {
			let _guard = StateDiffGuard::builder()
				.must_not_change(TestStorageValue::storage_info())
				.build();
		});
	}

	#[test]
	fn test_diff_guard_must_not_change_errors() {
		let mut ext = TestExternalities::default();
		ext.execute_with(|| {
			TestStorageValue::put(1);
			let _guard = StateDiffGuard::builder()
				.must_not_change(TestStorageValue::storage_info())
				.build();
		});
	}

	#[test]
	#[should_panic(expected = "`StateDiffGuard` detected an unexpected storage change")]
	fn test_diff_guard_must_not_change_on_change() {
		let mut ext = TestExternalities::default();
		ext.execute_with(|| {
			TestStorageValue::put(1);

			let _guard = StateDiffGuard::builder()
				.must_not_change(TestStorageValue::storage_info())
				.build();

			TestStorageValue::put(2);
		});
	}

	#[test]
	#[should_panic(expected = "`StateDiffGuard` detected an unexpected storage change")]
	fn test_diff_guard_must_not_change_if_not_existed() {
		let mut ext = TestExternalities::default();
		ext.execute_with(|| {
			let _guard = StateDiffGuard::builder()
				.must_not_change(TestStorageValue::storage_info())
				.build();

			TestStorageValue::put(2);
		});
	}
}
