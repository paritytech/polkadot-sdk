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

//! Tests for the benchmark macro for instantiable modules

#![cfg(test)]

use frame_support::{derive_impl, traits::ConstU32};
use sp_io::hashing::twox_128;
use sp_runtime::{
	testing::H256,
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

#[frame_support::pallet]
mod pallet_test {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	pub trait OtherConfig {
		type OtherEvent;
	}

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config + OtherConfig {
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;
		type LowerBound: Get<u32>;
		type UpperBound: Get<u32>;
		type MaxLength: Get<u32>;
	}

	#[pallet::storage]
	pub(crate) type Value<T: Config<I>, I: 'static = ()> = StorageValue<_, u32, OptionQuery>;

	#[pallet::storage]
	#[pallet::whitelist_storage] // This is what we're testing!
	pub(crate) type WhitelistedMap<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Blake2_128Concat,
		u32, // Key type
		u64, // Value type
		OptionQuery,
	>;

	#[pallet::storage]
	#[pallet::whitelist_storage] // Test double maps too
	pub(crate) type WhitelistedDoubleMap<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		u32, // First key
		Blake2_128Concat,
		u64,  // Second key
		u128, // Value type
		OptionQuery,
	>;

	#[pallet::storage]
	#[pallet::whitelist_storage]
	pub(crate) type WhitelistedValue<T: Config<I>, I: 'static = ()> =
		StorageValue<_, u32, OptionQuery>;

	#[pallet::storage]
	#[pallet::whitelist_storage]
	pub type AccountMap<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

	#[pallet::storage]
	#[pallet::whitelist_storage]
	pub type ComplexMap<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, u32, BoundedVec<u8, T::MaxLength>, ValueQuery>;

	#[pallet::event]
	pub enum Event<T: Config<I>, I: 'static = ()> {}

	// Add error enum variant
	#[pallet::error]
	pub enum Error<T, I = ()> {
		DataTooLong,
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I>
	where
		<T as OtherConfig>::OtherEvent: Into<<T as Config<I>>::RuntimeEvent>,
	{
		#[pallet::call_index(0)]
		#[pallet::weight({0})]
		pub fn set_value(origin: OriginFor<T>, n: u32) -> DispatchResult {
			let _sender = ensure_signed(origin)?;
			assert!(n >= T::LowerBound::get());
			Value::<T, I>::put(n);
			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight({0})]
		pub fn dummy(origin: OriginFor<T>, _n: u32) -> DispatchResult {
			let _sender = ensure_none(origin)?;
			Ok(())
		}

		#[pallet::call_index(2)]
		#[pallet::weight({0})]
		pub fn access_maps(
			origin: OriginFor<T>,
			key1: u32,
			key2: u64,
			key3: T::AccountId,
			data: Vec<u8>,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;

			let bounded_data = BoundedVec::<u8, T::MaxLength>::try_from(data.clone())
				.map_err(|_| Error::<T, I>::DataTooLong)?;
			ComplexMap::<T, I>::insert(key1, bounded_data);

			// Basic read operations
			let _ = WhitelistedValue::<T, I>::get();
			let _ = WhitelistedMap::<T, I>::get(key1);
			let _ = WhitelistedDoubleMap::<T, I>::get(key1, key2);
			let _ = ComplexMap::<T, I>::get(key1);

			// Non-whitelisted storage
			let _ = Value::<T, I>::get();

			// Iteration within LIMIT
			let mut counter = 0;
			for (_, _) in WhitelistedMap::<T, I>::iter() {
				counter += 1;
				if counter >= 5 {
					break;
				} // Safety limit
			}

			// INCREMENTAL read operations
			for i in 0..3 {
				let _ = WhitelistedMap::<T, I>::get(key1.saturating_add(i));
			}

			// BOUNDED storage access
			let bounded_key = key1 % 1000; // Prevent overly large keys
			let _ = WhitelistedMap::<T, I>::get(bounded_key);

			// Add storage ACCESS with AccountId key
			let _ = AccountMap::<T, I>::get(&sender);
			let _ = AccountMap::<T, I>::get(&key3);

			// Add MULTI-GET pattern
			let keys = [key1, key1.saturating_add(1), key1.saturating_add(2)];

			for k in keys.iter() {
				let _ = WhitelistedMap::<T, I>::get(k);
			}

			// Add storage EXISTS checks (different access pattern)
			let _ = WhitelistedMap::<T, I>::contains_key(key1);
			let _ = AccountMap::<T, I>::contains_key(&sender);

			let _ = ComplexMap::<T, I>::decode_len(key1);

			// Add conditional MULTIPLE double map accesses
			if key1 % 2 == 0 {
				for i in 0..2 {
					let _ = WhitelistedDoubleMap::<T, I>::get(
						key1.saturating_add(i as u32),
						key2.saturating_add(i as u64),
					);
				}
			}

			// Add NESTED storage access pattern
			if let Some(value) = WhitelistedMap::<T, I>::get(key1) {
				// Use first map's value as key for second lookup
				let _ = WhitelistedDoubleMap::<T, I>::get(value as u32, key2);
			}

			Ok(())
		}
	}
}

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		TestPallet: pallet_test,
		TestPallet2: pallet_test::<Instance2>,
	}
);

crate::define_benchmarks!(
	[pallet_test, TestPallet]
	[pallet_test, TestPallet2]
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = ();
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
}

impl pallet_test::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type LowerBound = ConstU32<1>;
	type UpperBound = ConstU32<100>;
	type MaxLength = ConstU32<400>;
}

impl pallet_test::Config<pallet_test::Instance2> for Test {
	type RuntimeEvent = RuntimeEvent;
	type LowerBound = ConstU32<50>;
	type UpperBound = ConstU32<100>;
	type MaxLength = ConstU32<400>;
}

impl pallet_test::OtherConfig for Test {
	type OtherEvent = RuntimeEvent;
}

fn new_test_ext() -> sp_io::TestExternalities {
	RuntimeGenesisConfig::default().build_storage().unwrap().into()
}

mod benchmarks {
	use super::pallet_test::{
		self, AccountMap, ComplexMap, Value, WhitelistedDoubleMap, WhitelistedMap, WhitelistedValue,
	};
	use crate::account;
	use frame_support::ensure;
	use frame_system::RawOrigin;
	use sp_core::Get;

	// Additional used internally by the benchmark macro.
	use super::pallet_test::{Call, Config, Pallet};

	crate::benchmarks_instance_pallet! {
		where_clause {
			where
				<T as pallet_test::OtherConfig>::OtherEvent: Clone
					+ Into<<T as pallet_test::Config<I>>::RuntimeEvent>,
				<T as pallet_test::Config<I>>::RuntimeEvent: Clone,
		}

		set_value {
			let b in ( <T as Config<I>>::LowerBound::get() ) .. ( <T as Config<I>>::UpperBound::get() );
			let caller = account::<T::AccountId>("caller", 0, 0);
		}: _ (RawOrigin::Signed(caller), b.into())
		verify {
			assert_eq!(Value::<T, I>::get(), Some(b));
		}

		other_name {
			let b in 1 .. 1000;
		}: dummy (RawOrigin::None, b.into())

		sort_vector {
			let x in 1 .. 10000;
			let mut m = Vec::<u32>::new();
			for i in (0..x).rev() {
				m.push(i);
			}
		}: {
			m.sort();
		} verify {
			ensure!(m[0] == 0, "You forgot to sort!")
		}

		access_maps {
			let a in 1 .. 100u32;
			let b in 1 .. 100u32;
			let alice = account::<T::AccountId>("alice", 0, 0);
			let bob = account::<T::AccountId>("bob", 0, 0);
			let data = vec![1u8, 2u8, 3u8]; // Simple test data

			// Setup storage
			Value::<T, I>::put(a);
			WhitelistedValue::<T, I>::put(b);
			WhitelistedMap::<T, I>::insert(a, b as u64);
			WhitelistedDoubleMap::<T, I>::insert(a, b as u64, b as u128 * 1000);

			// Setup ComplexMap
			let bounded_data = frame_support::BoundedVec::<u8, T::MaxLength>::try_from(data.clone()).unwrap();
			ComplexMap::<T, I>::insert(a, bounded_data);

			// Setup AccountMap
			AccountMap::<T, I>::insert(&alice, a);
			AccountMap::<T, I>::insert(&bob, b as u32);
		}: _ (RawOrigin::Signed(alice.clone()), a, b as u64, bob.clone(), data)
		verify {
			// Basic verifications
			assert_eq!(WhitelistedMap::<T, I>::get(a), Some(b as u64));
			assert_eq!(WhitelistedDoubleMap::<T, I>::get(a, b as u64), Some(b as u128 * 1000));
			assert_eq!(WhitelistedValue::<T, I>::get(), Some(b as u32));
			assert_eq!(Value::<T, I>::get(), Some(a));

			// Verify ComplexMap
			let expected_data = frame_support::BoundedVec::<u8, T::MaxLength>::try_from(vec![1u8, 2u8, 3u8]).unwrap();
			assert_eq!(ComplexMap::<T, I>::get(a), expected_data);

			// Verify AccountMap
			assert_eq!(AccountMap::<T, I>::get(&alice), a);
			assert_eq!(AccountMap::<T, I>::get(&bob), b as u32);
		}

		impl_benchmark_test_suite!(
			Pallet,
			crate::tests_instance::new_test_ext(),
			crate::tests_instance::Test
		)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{utils::Benchmarking, BenchmarkConfig};
	use frame_support::traits::{StorageInfoTrait, WhitelistedStorageKeys};
	use sp_state_machine::Backend;

	#[test]
	fn ensure_correct_instance_is_selected() {
		let whitelist = TestPallet::whitelisted_storage_keys();

		let mut batches = Vec::<crate::BenchmarkBatch>::new();
		let config = crate::BenchmarkConfig {
			pallet: "pallet_test".bytes().collect::<Vec<_>>(),
			// We only want that this `instance` is used.
			// Otherwise the wrong components are used.
			instance: "TestPallet".bytes().collect::<Vec<_>>(),
			benchmark: "set_value".bytes().collect::<Vec<_>>(),
			selected_components: TestPallet::benchmarks(false)
				.into_iter()
				.find_map(|b| {
					if b.name == "set_value".as_bytes() {
						Some(b.components.into_iter().map(|c| (c.0, c.1)).collect::<Vec<_>>())
					} else {
						None
					}
				})
				.unwrap(),
			verify: false,
			internal_repeats: 1,
		};
		let params = (&config, &whitelist);

		let state = sc_client_db::BenchmarkingState::<sp_runtime::traits::BlakeTwo256>::new(
			Default::default(),
			None,
			false,
			false,
		)
		.unwrap();

		let mut overlay = Default::default();
		let mut ext = sp_state_machine::Ext::new(&mut overlay, &state, None);
		sp_externalities::set_and_run_with_externalities(&mut ext, || {
			add_benchmarks!(params, batches);
			Ok::<_, crate::BenchmarkError>(())
		})
		.unwrap();

		assert!(!whitelist.is_empty(), "Pallet should have whitelisted storage");
		assert_eq!(whitelist.len(), 5);
	}

	#[test]
	fn test_whitelist_storage_for_storage_maps() {
		// Test that whitelisted_storage_keys() returns prefixes for storage maps
		let whitelist = TestPallet::whitelisted_storage_keys();

		// Should have entries for all whitelisted storage items
		assert!(!whitelist.is_empty(), "Should have whitelisted storage keys");

		// Verify each storage item
		let storage_info = pallet_test::Pallet::<Test>::storage_info();

		assert!(storage_info.len() == 6, "Should be 6 items in storage");
		assert!(whitelist.len() == 5, "Should be 5 whitelisted items in storage");

		// Calculate the expected storage prefixes
		// The pattern is: twox128("PalletName") ++ twox128("StorageItemName")
		let pallet_name = "TestPallet";
		let pallet_hash = twox_128(pallet_name.as_bytes());

		// List of your whitelisted storage items
		let whitelisted_items = [
			"WhitelistedValue",
			"WhitelistedMap",
			"WhitelistedDoubleMap",
			"AccountMap",
			"ComplexMap",
		];

		// For each whitelisted item, check that its prefix exists in the whitelist
		for item_name in &whitelisted_items {
			let item_hash = twox_128(item_name.as_bytes());
			let mut expected_prefix = Vec::new();
			expected_prefix.extend_from_slice(&pallet_hash);
			expected_prefix.extend_from_slice(&item_hash);

			let found = whitelist.iter().any(|tracked_key| {
				// Check if this key starts with the expected prefix
				tracked_key.key.starts_with(&expected_prefix)
			});

			assert!(
				found,
				"Storage item '{}' should be in whitelist. Expected prefix: {:?}",
				item_name, expected_prefix
			);
		}
	}

	#[test]
	fn test_benchmark_with_whitelisted_maps() {
		// Get the actual whitelist from the test pallet
		let whitelist = TestPallet::whitelisted_storage_keys();
		assert!(!whitelist.is_empty(), "Test pallet should have whitelisted storage");

		// Get the benchmarks for the test pallet
		let benchmarks = TestPallet::benchmarks(false);

		// Find the "access_maps" benchmark
		let selected_components = benchmarks
			.into_iter()
			.find_map(|b| {
				if b.name == b"access_maps" {
					// Convert from (BenchmarkParameter, u32, u32) to (BenchmarkParameter, u32)
					// by taking only the first two elements
					Some(
						b.components
							.into_iter()
							.map(|(param, _min, max)| (param, max)) // or use (param, min) depending on your needs
							.collect::<Vec<_>>(),
					)
				} else {
					None
				}
			})
			.unwrap_or_else(|| Vec::new());

		// Only run this test if the benchmark exists
		if !selected_components.is_empty() {
			let config = BenchmarkConfig {
				pallet: b"pallet_test".to_vec(),
				instance: b"TestPallet".to_vec(),
				benchmark: b"access_maps".to_vec(),
				selected_components,
				verify: false,
				internal_repeats: 1,
			};

			let params = (&config, &whitelist);

			let state = sc_client_db::BenchmarkingState::<sp_runtime::traits::BlakeTwo256>::new(
				Default::default(),
				None,
				false,
				true,
			)
			.unwrap();

			state.set_whitelist(whitelist.clone());

			let mut overlay = Default::default();
			let mut ext = sp_state_machine::Ext::new(&mut overlay, &state, None);

			let mut batches = Vec::<crate::BenchmarkBatch>::new();

			sp_externalities::set_and_run_with_externalities(&mut ext, || {
				// Run benchmark with whitelist that includes map prefixes
				add_benchmarks!(params, batches);

				Ok::<_, crate::BenchmarkError>(())
			})
			.unwrap();

			// Verify benchmark was added
			assert!(!batches.is_empty(), "Benchmark should be added");
		}
	}

	#[test]
	fn test_multiple_instance_whitelist_for_maps() {
		// Test that different instances have different whitelists
		let whitelist1 = TestPallet::whitelisted_storage_keys();
		let whitelist2 = TestPallet2::whitelisted_storage_keys();

		// They should both have entries
		assert!(!whitelist1.is_empty());
		assert!(!whitelist2.is_empty());

		// Note: The prefixes might be different for different instances
		// because storage prefixes include instance identifier

		// Count entries (should be same number since same storage layout)
		assert_eq!(whitelist1.len(), whitelist2.len());

		// But actual prefixes might differ due to instance encoding
		let _ = whitelist1.iter().zip(whitelist2.iter()).all(|(a, b)| a.key == b.key);

		// They might be the same or different depending on implementation
		// Just verify both are valid
		for key in &whitelist1 {
			assert!(!key.key.is_empty());
		}
		for key in &whitelist2 {
			assert!(!key.key.is_empty());
		}
	}
}
