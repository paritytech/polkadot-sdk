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

use super::*;
use binary_merkle_tree::MerkleProof;
use codec::Encode;
use frame_benchmarking::{account, v2::*, whitelisted_caller, BenchmarkError};
use frame_support::{
	traits::{Consideration, Get},
	BoundedVec,
};
use frame_system::{pallet_prelude::BlockNumberFor, RawOrigin};
use sp_core::{crypto::KeyTypeId, H256};
use sp_runtime::traits::{Bounded, Hash, Zero};

const BENCH_KEY_TYPE: KeyTypeId = KeyTypeId(*b"sccl");

fn ensure_collection<T: Config>(
	owner: &T::AccountId,
) -> Result<pallet_scarcity::CollectionId, BenchmarkError> {
	// Avoid overflowing an existing total issuance in small-balance benchmark runtimes.
	let funding = <pallet_scarcity::BalanceOf<T> as Bounded>::max_value() / 2u32.into();
	T::Consideration::ensure_successful(owner, funding);
	let collection = pallet_scarcity::Pallet::<T>::do_create_collection(owner.clone())?;
	let item = pallet_scarcity::Pallet::<T>::do_define_item(owner.clone(), collection, Vec::new())?;
	if item != 0 {
		return Err(BenchmarkError::Stop("first item index was not zero"));
	}
	Ok(collection)
}

/// Construct a valid left-most proof with exactly `depth` sibling hashes without allocating all
/// `2^depth` leaves. The base-2 verifier hashes the leaf with each right sibling in order.
fn proof_with_depth(
	voucher: VoucherPublic,
	credit_hash: CreditHash,
	timestamp: CreditTimestamp,
	depth: u32,
) -> (H256, u32, Vec<u8>) {
	let leaf = ((voucher, credit_hash), timestamp).encode();
	let mut current = <sp_runtime::traits::BlakeTwo256 as Hash>::hash(&leaf);
	let mut siblings = Vec::with_capacity(depth as usize);
	for index in 0..depth {
		let sibling = H256::repeat_byte(index.saturating_add(1) as u8);
		let mut pair = [0u8; 64];
		pair[..32].copy_from_slice(current.as_bytes());
		pair[32..].copy_from_slice(sibling.as_bytes());
		current = <sp_runtime::traits::BlakeTwo256 as Hash>::hash(&pair);
		siblings.push(sibling);
	}
	let number_of_leaves = 1u32.checked_shl(depth).unwrap_or(u32::MAX);
	let proof =
		MerkleProof { root: current, proof: siblings, number_of_leaves, leaf_index: 0, leaf }
			.encode();
	(current, number_of_leaves, proof)
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn ingest_root() {
		let root_id = 1;
		let root = H256::repeat_byte(0x42);

		#[extrinsic_call]
		_(RawOrigin::Root, root_id, root, 1);

		assert_eq!(
			Roots::<T>::get(root_id),
			Some(RootInfo { root, claim_count: 1, claimed_count: 0 })
		);
	}

	#[benchmark]
	fn claim(h: Linear<0, { T::MaxProofDepth::get() }>) -> Result<(), BenchmarkError> {
		let owner: T::AccountId = whitelisted_caller();
		let submitter: T::AccountId = account("submitter", 0, 0);
		let destination: T::AccountId = account("destination", 0, 0);
		let collection = ensure_collection::<T>(&owner)?;
		let voucher = sp_io::crypto::sr25519_generate(BENCH_KEY_TYPE, None);
		let credit_hash = H256::repeat_byte(0x55);
		let timestamp = 123u32;
		let (root, claim_count, proof) = proof_with_depth(voucher, credit_hash, timestamp, h);
		let proof: BoundedVec<u8, T::MaxProofLen> = proof
			.try_into()
			.map_err(|_| BenchmarkError::Stop("proof exceeded MaxProofLen"))?;
		let root_id = 1;
		Roots::<T>::insert(root_id, RootInfo { root, claim_count, claimed_count: 0 });
		LatestRootId::<T>::put(root_id);
		let genesis_hash = frame_system::Pallet::<T>::block_hash(BlockNumberFor::<T>::zero());
		let payload =
			authorization_payload(&genesis_hash, root_id, credit_hash, collection, &destination);
		let signature = sp_io::crypto::sr25519_sign(BENCH_KEY_TYPE, &voucher, &payload)
			.ok_or(BenchmarkError::Stop("benchmark voucher could not sign"))?;

		#[extrinsic_call]
		_(
			RawOrigin::Signed(submitter),
			root_id,
			voucher,
			credit_hash,
			timestamp,
			proof,
			collection,
			destination.clone(),
			signature,
		);

		assert!(pallet_scarcity::NftsByOwner::<T>::contains_key(destination));
		assert!(matches!(Claims::<T>::get(credit_hash), Some(ClaimState::Claimed { item: 0, .. })));
		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
