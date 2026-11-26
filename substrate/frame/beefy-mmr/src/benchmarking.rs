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

//! Beefy pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as BeefyMmr;
use codec::Encode;
use frame_benchmarking::v2::*;
use frame_support::traits::Hooks;
use frame_system::{Config as SystemConfig, Pallet as System};
use pallet_mmr::{Nodes, Pallet as Mmr};
use sp_consensus_beefy::Payload;
use sp_runtime::traits::One;

pub trait Config:
	pallet_mmr::Config<Hashing = sp_consensus_beefy::MmrHashing> + crate::Config
{
}

impl<T> Config for T where
	T: pallet_mmr::Config<Hashing = sp_consensus_beefy::MmrHashing> + crate::Config
{
}

fn init_block<T: Config>(block_num: u32) {
	let block_num = block_num.into();
	System::<T>::initialize(&block_num, &<T as SystemConfig>::Hash::default(), &Default::default());
	Mmr::<T>::on_initialize(block_num);
}

#[benchmarks]
mod benchmarks {
	use super::*;

	/// Generate ancestry proofs with `n` leafs and benchmark the logic that checks
	/// if the proof is optimal.
	#[benchmark]
	fn n_leafs_proof_is_optimal(n: Linear<2, 512>) {
		let (proof, _) =
			Mmr::<T>::generate_synthetic_ancestry_proof((n - 1).into(), n.into()).unwrap();
		assert_eq!(proof.leaf_count, n as u64);

		#[block]
		{
			<BeefyMmr<T> as AncestryHelper<HeaderFor<T>>>::is_proof_optimal(&proof);
		};
	}

	#[benchmark]
	fn extract_validation_context() {
		pallet_mmr::UseLocalStorage::<T>::set(true);

		init_block::<T>(1);
		let header = System::<T>::finalize();
		frame_system::BlockHash::<T>::insert(BlockNumberFor::<T>::one(), header.hash());

		let validation_context;
		#[block]
		{
			validation_context =
				<BeefyMmr<T> as AncestryHelper<HeaderFor<T>>>::extract_validation_context(header);
		}

		assert!(validation_context.is_some());
	}

	#[benchmark]
	fn read_peak() {
		pallet_mmr::UseLocalStorage::<T>::set(true);

		init_block::<T>(1);

		let peak;
		#[block]
		{
			peak = Nodes::<T>::get(0)
		}

		assert!(peak.is_some());
	}

	/// Generate optimal ancestry proofs with `n` nodes (items + previous peaks) and benchmark the
	/// verification logic.
	///
	/// For `prev_leaf_count = 2^(n - 2) - 1` and `leaf_count = 2^(n - 1) - 1`, the optimal proof
	/// has exactly `n` nodes. This is the split that needs the most hashing for a given `n`.
	/// The upper bound covers all the proofs possible with 32-bit block numbers.
	#[benchmark]
	fn n_items_proof_is_non_canonical(n: Linear<3, 33>) {
		let prev_leaf_count = (1u64 << (n - 2)) - 1;
		let leaf_count = (1u64 << (n - 1)) - 1;
		let (proof, mmr_root) =
			Mmr::<T>::generate_synthetic_ancestry_proof(prev_leaf_count, leaf_count).unwrap();
		assert!(<BeefyMmr<T> as AncestryHelper<HeaderFor<T>>>::is_proof_optimal(&proof));
		assert_eq!(proof.items.len() + proof.prev_peaks.len(), n as usize);

		// Set up the chain so that leaf `i` was added by block `i + 1`.
		let leaf_count_to_block_num = |leaf_count: u64| -> BlockNumberFor<T> {
			u32::try_from(leaf_count).expect("leaf_count < 2^32 since n <= 33; qed").into()
		};
		System::<T>::set_block_number(leaf_count_to_block_num(leaf_count));
		pallet_mmr::NumberOfLeaves::<T>::put(leaf_count);

		let commitment = Commitment {
			payload: Payload::from_single_entry(
				known_payloads::MMR_ROOT_ID,
				MerkleRootOf::<T>::default().encode(),
			),
			block_number: leaf_count_to_block_num(prev_leaf_count),
			validator_set_id: 0,
		};
		let context = AncestryValidationContext { leaf_count, mmr_root };

		let is_non_canonical;
		#[block]
		{
			is_non_canonical = <BeefyMmr<T> as AncestryHelper<HeaderFor<T>>>::is_non_canonical(
				&commitment,
				proof,
				context,
			);
		};

		assert_eq!(is_non_canonical, true);
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(Default::default()),
		crate::mock::Test
	);
}
