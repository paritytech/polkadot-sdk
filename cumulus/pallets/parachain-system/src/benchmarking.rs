// This file is part of Cumulus.

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

//! Benchmarking for the parachain-system pallet.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::parachain_inherent::InboundDownwardMessages;
use cumulus_primitives_core::{relay_chain::Hash as RelayHash, InboundDownwardMessage};
use frame_benchmarking::v2::*;
use sp_runtime::traits::BlakeTwo256;

#[benchmarks]
mod benchmarks {
	use super::*;

	/// Enqueue `n` messages via `enqueue_inbound_downward_messages`.
	///
	/// The limit is set to `1000` for benchmarking purposes as the actual limit is only known at
	/// runtime. However, the limit (and default) for Dotsama are magnitudes smaller.
	#[benchmark]
	fn enqueue_inbound_downward_messages(n: Linear<0, 1000>) {
		let msg = InboundDownwardMessage {
			sent_at: n, // The block number does not matter.
			msg: vec![0u8; MaxDmpMessageLenOf::<T>::get() as usize],
		};
		let msgs = vec![msg; n as usize];
		let head = mqp_head(&msgs);

		#[block]
		{
			Pallet::<T>::enqueue_inbound_downward_messages(
				head,
				InboundDownwardMessages::new(msgs).into_abridged(&mut usize::MAX.clone()),
			);
		}

		assert_eq!(ProcessedDownwardMessages::<T>::get(), n);
		assert_eq!(LastDmqMqcHead::<T>::get().head(), head);
	}

	/// Benchmark processing published data from the broadcaster pallet.
	///
	/// - `p`: Number of publishers with changed data
	/// - `k`: Number of key-value pairs per publisher
	/// - `v`: Size of each value in bytes
	#[benchmark]
	fn process_published_data(
		p: Linear<1, 100>,
		k: Linear<1, 16>,
		v: Linear<1, 1024>,
	) {
		use alloc::collections::BTreeMap;

		// Populate storage with existing data to maximize clear_prefix cost
		for i in 0..p {
			let para_id = ParaId::from(1000 + i);
			for j in 0..k {
				PublishedData::<T>::insert(
					para_id,
					vec![j as u8; 32],
					vec![0u8; v as usize],
				);
			}
		}

		// Store initial roots
		let initial_roots: BTreeMap<ParaId, Vec<u8>> = (0..p)
			.map(|i| (ParaId::from(1000 + i), vec![0xBB; 32]))
			.collect();
		PreviousPublishedDataRoots::<T>::put(initial_roots);

		// Prepare new data with changed roots
		let mut published_data = BTreeMap::new();
		let mut current_roots = Vec::new();

		for i in 0..p {
			let para_id = ParaId::from(1000 + i);
			let entries: Vec<(Vec<u8>, Vec<u8>)> = (0..k)
				.map(|j| (vec![j as u8; 32], vec![1u8; v as usize]))
				.collect();
			published_data.insert(para_id, entries);
			current_roots.push((para_id, vec![0xAA; 32]));
		}

		#[block]
		{
			Pallet::<T>::process_published_data(&published_data, &current_roots);
		}

		// Verify storage updated
		assert_eq!(PreviousPublishedDataRoots::<T>::get().len(), p as usize);
	}

	/// Re-implements an easy version of the `MessageQueueChain` for testing purposes.
	fn mqp_head(msgs: &Vec<InboundDownwardMessage>) -> RelayHash {
		let mut head = Default::default();
		for msg in msgs.iter() {
			let msg_hash = BlakeTwo256::hash_of(&msg.msg);
			head = BlakeTwo256::hash_of(&(head, msg.sent_at, msg_hash));
		}
		head
	}

	impl_benchmark_test_suite! {
		Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Test
	}
}
