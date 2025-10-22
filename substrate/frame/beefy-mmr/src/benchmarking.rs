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
		pallet_mmr::UseLocalStorage::<T>::set(true);

		for block_num in 1..=n {
			init_block::<T>(block_num);
		}
		let proof = Mmr::<T>::generate_mock_ancestry_proof().unwrap();
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

	/// Generate ancestry proofs with `n` nodes and benchmark the verification logic.
	/// These proofs are inflated, containing all the leafs, so we won't read any peak during
	/// the verification. We need to account for the peaks separately.
	#[benchmark]
	fn n_items_proof_is_non_canonical(n: Linear<2, 512>) {
		pallet_mmr::UseLocalStorage::<T>::set(true);

		for block_num in 1..=n {
			init_block::<T>(block_num);
		}
		let proof = Mmr::<T>::generate_mock_ancestry_proof().unwrap();
		assert_eq!(proof.items.len(), n as usize);

		let is_non_canonical;
		#[block]
		{
			is_non_canonical = <BeefyMmr<T> as AncestryHelper<HeaderFor<T>>>::is_non_canonical(
				&Commitment {
					payload: Payload::from_single_entry(
						known_payloads::MMR_ROOT_ID,
						MerkleRootOf::<T>::default().encode(),
					),
					block_number: n.into(),
					validator_set_id: 0,
				},
				proof,
				Mmr::<T>::mmr_root(),
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
