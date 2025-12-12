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
use codec::Encode;
use cumulus_pallet_parachain_system::RelayChainStateProof;
use cumulus_primitives_core::ParaId;
use frame_benchmarking::v2::*;
use frame_support::traits::Get;
use frame_system::RawOrigin;
use sp_runtime::{traits::HashingFor, StateVersion};
use sp_state_machine::{Backend, TrieBackendBuilder};
use sp_trie::PrefixedMemoryDB;

/// Build a relay chain state proof with child trie data for multiple publishers.
fn build_test_proof<T: Config>(
	publishers: &[(ParaId, Vec<(Vec<u8>, Vec<u8>)>)],
) -> RelayChainStateProof {
	let (db, root) = PrefixedMemoryDB::<HashingFor<polkadot_primitives::Block>>::default_with_root();
	let state_version = StateVersion::default();
	let mut backend = TrieBackendBuilder::new(db, root).build();

	let mut all_proofs = vec![];
	let mut main_trie_updates = vec![];

	// Process each publisher
	for (publisher_para_id, child_data) in publishers {
		let child_info = sp_core::storage::ChildInfo::new_default(&(b"pubsub", *publisher_para_id).encode());

		// Insert child trie data
		let child_kv: Vec<_> = child_data.iter().map(|(k, v)| (k.clone(), Some(v.clone()))).collect();
		backend.insert(vec![(Some(child_info.clone()), child_kv)], state_version);

		// Get child trie root and prepare to insert it in main trie
		let child_root = backend.child_storage_root(&child_info, core::iter::empty(), state_version).0;
		let prefixed_key = child_info.prefixed_storage_key();
		main_trie_updates.push((prefixed_key.to_vec(), Some(child_root.encode())));

		// Prove child trie keys
		let child_keys: Vec<_> = child_data.iter().map(|(k, _)| k.clone()).collect();
		if !child_keys.is_empty() {
			let child_proof = sp_state_machine::prove_child_read_on_trie_backend(&backend, &child_info, child_keys)
				.expect("prove child read");
			all_proofs.push(child_proof);
		}
	}

	// Insert all child roots in main trie
	backend.insert(vec![(None, main_trie_updates.clone())], state_version);
	let root = *backend.root();

	// Prove all child roots in main trie
	let main_keys: Vec<_> = main_trie_updates.iter().map(|(k, _)| k.clone()).collect();
	let main_proof = sp_state_machine::prove_read_on_trie_backend(&backend, main_keys)
		.expect("prove read");
	all_proofs.push(main_proof);

	// Merge all proofs
	let proof = sp_trie::StorageProof::merge(all_proofs);

	RelayChainStateProof::new(ParaId::from(100), root, proof).expect("valid proof")
}

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

	/// Benchmark calling `SubscriptionHandler::subscriptions()`.
	///
	/// Cost scales with both number of publishers `n` and keys per publisher `k`.
	#[benchmark]
	fn get_subscriptions(
		n: Linear<1, { T::MaxPublishers::get() }>,
		k: Linear<1, 10>,
	) {
		let _subscriptions = create_subscriptions(n, k);
		#[block]
		{
			let _subs = T::SubscriptionHandler::subscriptions();
		}
	}

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
		let proof = build_test_proof::<T>(&publishers);

		#[block]
		{
			Subscriber::<T>::collect_publisher_roots(&proof, &subscriptions);
		}
	}

	/// Benchmark processing published data from the relay proof.
	///
	/// Worst case: all `n` publishers have updated data with `k` keys each that need processing.
	#[benchmark]
	fn process_published_data(
		n: Linear<1, { T::MaxPublishers::get() }>,
		k: Linear<1, 10>,
	) {
		let subscriptions = create_subscriptions(n, k);
		let publishers: Vec<_> = (0..n)
			.map(|i| {
				let para_id = ParaId::from(1000 + i);
				let child_data: Vec<(Vec<u8>, Vec<u8>)> = (0..k)
					.map(|j| {
						let value = vec![25u8; 100];
						let encoded_value = value.encode();
						(vec![i as u8, j as u8], encoded_value)
					})
					.collect();
				(para_id, child_data)
			})
			.collect();
		let proof = build_test_proof::<T>(&publishers);
		let current_roots = Subscriber::<T>::collect_publisher_roots(&proof, &subscriptions);

		#[block]
		{
			Subscriber::<T>::process_published_data(&proof, &current_roots, &subscriptions);
		}
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
