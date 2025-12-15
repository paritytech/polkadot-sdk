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
use crate::{mock::build_sproof_with_child_data, Pallet as Subscriber};
use cumulus_primitives_core::ParaId;
use frame_benchmarking::v2::*;
use frame_support::traits::Get;
use frame_system::RawOrigin;

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
		let publishers: Vec<_> = (0..n)
			.map(|i| (ParaId::from(1000 + i), vec![(vec![i as u8], vec![25u8])]))
			.collect();
		let proof = build_sproof_with_child_data(&publishers);
		let roots;
		#[block]
		{
			roots = Subscriber::<T>::collect_publisher_roots(&proof, &subscriptions);
		}
		assert!(roots.len() == n as usize);
	}

	/// Benchmark processing published data from the relay proof.
	///
	/// Worst case: all `n` publishers have updated data with `k` keys each that need processing.
	/// Each value has size `s` bytes. Max is 2048 bytes (2KiB limit per publisher).
	#[benchmark]
	fn process_published_data(
		n: Linear<1, { T::MaxPublishers::get() }>,
		k: Linear<1, 10>,
		s: Linear<1, 2048>,
	) {
		let subscriptions = create_subscriptions(n, k);
		// Calculate per-key value size to stay within 2KiB total per publisher
		let value_size_per_key = (s / k.max(1)) as usize;
		let publishers: Vec<_> = (0..n)
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
		let proof = build_sproof_with_child_data(&publishers);
		let current_roots = Subscriber::<T>::collect_publisher_roots(&proof, &subscriptions);

		#[block]
		{
			let _ = Subscriber::<T>::process_published_data(&proof, &current_roots, &subscriptions);
		}
		assert!(PreviousPublishedDataRoots::<T>::get().len() == n as usize);
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
