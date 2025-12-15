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

//! Benchmarking setup for cumulus-pallet-subscriber

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as Subscriber;
use cumulus_pallet_parachain_system::RelayChainStateProof;
use cumulus_primitives_core::ParaId;
use frame_benchmarking::v2::*;
use frame_support::traits::Get;
use frame_system::RawOrigin;
use sp_trie::StorageProof;

/// Create test subscriptions for benchmarking.
fn create_subscriptions(n: u32, keys_per_publisher: u32) -> Vec<(ParaId, Vec<Vec<u8>>)> {
	(0..n)
		.map(|i| {
			let para_id = ParaId::from(1000 + i);
			let keys: Vec<Vec<u8>> = if keys_per_publisher == 0 {
				vec![vec![i as u8], vec![i as u8, i as u8]]
			} else {
				(0..keys_per_publisher).map(|j| vec![i as u8, j as u8]).collect()
			};
			(para_id, keys)
		})
		.collect()
}

/// Create a relay chain state proof for benchmarking.
///
/// Currently returns an empty proof because of no available way of creating proof on no_std.
///
/// TODO: Replace with values or value generating function.
fn benchmark_relay_proof() -> RelayChainStateProof {
	use sp_runtime::traits::BlakeTwo256;
	use sp_trie::{empty_trie_root, LayoutV1};

	let proof = StorageProof::empty();
	let root = empty_trie_root::<LayoutV1<BlakeTwo256>>();
	RelayChainStateProof::new(ParaId::from(100), root.into(), proof).expect("valid proof")
}

#[benchmarks]
mod benchmarks {
	use super::*;

	/// Benchmark collecting publisher roots from the relay state proof.
	///
	/// Cost scales with the number of publishers `n`.
	#[benchmark]
	fn collect_publisher_roots(
		n: Linear<1, { T::MaxPublishers::get() }>,
	) {
		let subscriptions = create_subscriptions(n, 1);
		let _publishers: Vec<_> = (0..n)
			.map(|i| (ParaId::from(1000 + i), vec![(vec![i as u8], vec![25u8])]))
			.collect();
		// TODO: Use _publishers data to build proof once we have values
		let proof = benchmark_relay_proof();
		let roots;
		#[block]
		{
			roots = Subscriber::<T>::collect_publisher_roots(&proof, &subscriptions);
		}
		// TODO: Update assertion once proof contains actual data
		//assert!(roots.len() <= n as usize);
	}

	/// Benchmark processing published data from the relay proof.
	///
	/// Worst case: all publishers have updated data requiring processing.
	///
	/// Parameters:
	/// - `n`: Number of publishers with updated data
	/// - `k`: Number of keys per publisher
	/// - `s`: Total encoded bytes per publisher (max 2KiB)
	#[benchmark]
	fn process_published_data(
		n: Linear<1, { T::MaxPublishers::get() }>,
		k: Linear<1, 10>,
		s: Linear<1, 2048>,
	) {
		let subscriptions = create_subscriptions(n, k);
		// SCALE encoding overhead (1-4 bytes) ignored as negligible compared to data benchmark ranges
		let value_size_per_key = (s / k.max(1)) as usize;
		let _publishers: Vec<_> = (0..n)
			.map(|i| {
				let para_id = ParaId::from(1000 + i);
				let child_data: Vec<(Vec<u8>, Vec<u8>)> = (0..k)
					.map(|j| {
						let value = vec![25u8; value_size_per_key];
						let encoded_value = value.encode();
						(vec![i as u8, j as u8], encoded_value)
					})
					.collect();
				(para_id, child_data)
			})
			.collect();
		// TODO: Use _publishers data to build proof once we have values
		let proof = benchmark_relay_proof();
		let current_roots = Subscriber::<T>::collect_publisher_roots(&proof, &subscriptions);

		#[block]
		{
			let _ = Subscriber::<T>::process_published_data(&proof, &current_roots, &subscriptions);
		}
		// TODO: Update assertion once proof contains actual data
		//assert!(PreviousPublishedDataRoots::<T>::get().len() <= n as usize);
	}

	#[benchmark]
	fn clear_stored_roots() {
		let publisher = ParaId::from(1000);
		let root = BoundedVec::try_from(vec![0u8; 32]).unwrap();
		PreviousPublishedDataRoots::<T>::mutate(|roots| {
			let _ = roots.try_insert(publisher, root);
		});

		#[extrinsic_call]
		_(RawOrigin::Root, publisher);

		assert!(!PreviousPublishedDataRoots::<T>::get().contains_key(&publisher));
	}

	impl_benchmark_test_suite! {
		Subscriber,
		crate::mock::new_test_ext(),
		crate::mock::Test
	}
}
