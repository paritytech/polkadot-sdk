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

use alloc::vec::Vec;

use sp_core::{storage::StateVersion, H256};

#[cfg(not(substrate_runtime))]
use sp_trie::{LayoutV0, LayoutV1, TrieConfiguration};

use sp_runtime_interface::{
	pass_by::{
		AllocateAndReturnPointer, PassAs, PassFatPointerAndDecode, PassFatPointerAndDecodeSlice,
		PassFatPointerAndRead, PassPointerAndReadCopy, PassPointerAndWrite,
	},
	runtime_interface,
};

use crate::*;

/// Interface that provides trie related functionality.
#[runtime_interface]
pub trait Trie {
	/// A trie root formed from the iterated items.
	fn blake2_256_root(
		input: PassFatPointerAndDecode<Vec<(Vec<u8>, Vec<u8>)>>,
	) -> AllocateAndReturnPointer<H256, 32> {
		LayoutV0::<sp_core::Blake2Hasher>::trie_root(input)
	}

	/// A trie root formed from the iterated items.
	#[version(2)]
	fn blake2_256_root(
		input: PassFatPointerAndDecode<Vec<(Vec<u8>, Vec<u8>)>>,
		version: PassAs<StateVersion, u8>,
	) -> AllocateAndReturnPointer<H256, 32> {
		match version {
			StateVersion::V0 => LayoutV0::<sp_core::Blake2Hasher>::trie_root(input),
			StateVersion::V1 => LayoutV1::<sp_core::Blake2Hasher>::trie_root(input),
		}
	}

	/// A trie root formed from the iterated items.
	#[version(3)]
	#[raw_api]
	fn blake2_256_root(
		input: PassFatPointerAndDecode<Vec<(Vec<u8>, Vec<u8>)>>,
		version: PassAs<StateVersion, u8>,
		out: PassPointerAndWrite<&mut H256, 32>,
	) {
		let root = match version {
			StateVersion::V0 => LayoutV0::<sp_core::Blake2Hasher>::trie_root(input),
			StateVersion::V1 => LayoutV1::<sp_core::Blake2Hasher>::trie_root(input),
		};
		out.0.copy_from_slice(&root.0);
	}

	/// A convenience wrapper providing a developer-friendly interface for the `blake2_256_root`
	/// host function.
	#[wrapper]
	fn blake2_256_root(data: Vec<(Vec<u8>, Vec<u8>)>, state_version: StateVersion) -> H256 {
		let mut root = H256::default();
		blake2_256_root__raw(data, state_version, &mut root);
		root
	}
	/// A trie root formed from the enumerated items.
	fn blake2_256_ordered_root(
		input: PassFatPointerAndDecode<Vec<Vec<u8>>>,
	) -> AllocateAndReturnPointer<H256, 32> {
		LayoutV0::<sp_core::Blake2Hasher>::ordered_trie_root(input)
	}

	/// A trie root formed from the enumerated items.
	#[version(2)]
	fn blake2_256_ordered_root(
		input: PassFatPointerAndDecode<Vec<Vec<u8>>>,
		version: PassAs<StateVersion, u8>,
	) -> AllocateAndReturnPointer<H256, 32> {
		match version {
			StateVersion::V0 => LayoutV0::<sp_core::Blake2Hasher>::ordered_trie_root(input),
			StateVersion::V1 => LayoutV1::<sp_core::Blake2Hasher>::ordered_trie_root(input),
		}
	}

	/// A trie root formed from the enumerated items.
	#[version(3)]
	#[raw_api]
	fn blake2_256_ordered_root(
		input: PassFatPointerAndDecode<Vec<Vec<u8>>>,
		version: PassAs<StateVersion, u8>,
		out: PassPointerAndWrite<&mut H256, 32>,
	) {
		let root = match version {
			StateVersion::V0 => LayoutV0::<sp_core::Blake2Hasher>::ordered_trie_root(input),
			StateVersion::V1 => LayoutV1::<sp_core::Blake2Hasher>::ordered_trie_root(input),
		};
		out.0.copy_from_slice(&root.0);
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `blake2_256_ordered_root` host function.
	#[wrapper]
	fn blake2_256_ordered_root(data: Vec<Vec<u8>>, state_version: StateVersion) -> H256 {
		let mut root = H256::default();
		blake2_256_ordered_root__raw(data, state_version, &mut root);
		root
	}

	/// A trie root formed from the iterated items.
	fn keccak_256_root(
		input: PassFatPointerAndDecode<Vec<(Vec<u8>, Vec<u8>)>>,
	) -> AllocateAndReturnPointer<H256, 32> {
		LayoutV0::<sp_core::KeccakHasher>::trie_root(input)
	}

	/// A trie root formed from the iterated items.
	#[version(2)]
	fn keccak_256_root(
		input: PassFatPointerAndDecode<Vec<(Vec<u8>, Vec<u8>)>>,
		version: PassAs<StateVersion, u8>,
	) -> AllocateAndReturnPointer<H256, 32> {
		match version {
			StateVersion::V0 => LayoutV0::<sp_core::KeccakHasher>::trie_root(input),
			StateVersion::V1 => LayoutV1::<sp_core::KeccakHasher>::trie_root(input),
		}
	}

	/// A trie root formed from the iterated items.
	#[version(3)]
	#[raw_api]
	fn keccak_256_root(
		input: PassFatPointerAndDecode<Vec<(Vec<u8>, Vec<u8>)>>,
		version: PassAs<StateVersion, u8>,
		out: PassPointerAndWrite<&mut H256, 32>,
	) {
		let root = match version {
			StateVersion::V0 => LayoutV0::<sp_core::KeccakHasher>::trie_root(input),
			StateVersion::V1 => LayoutV1::<sp_core::KeccakHasher>::trie_root(input),
		};
		out.0.copy_from_slice(&root.0);
	}

	/// A convenience wrapper providing a developer-friendly interface for the `keccak_256_root`
	/// host function.
	#[wrapper]
	fn keccak_256_root(data: Vec<(Vec<u8>, Vec<u8>)>, state_version: StateVersion) -> H256 {
		let mut root = H256::default();
		keccak_256_root__raw(data, state_version, &mut root);
		root
	}

	/// A trie root formed from the enumerated items.
	fn keccak_256_ordered_root(
		input: PassFatPointerAndDecode<Vec<Vec<u8>>>,
	) -> AllocateAndReturnPointer<H256, 32> {
		LayoutV0::<sp_core::KeccakHasher>::ordered_trie_root(input)
	}

	/// A trie root formed from the enumerated items.
	#[version(2)]
	fn keccak_256_ordered_root(
		input: PassFatPointerAndDecode<Vec<Vec<u8>>>,
		version: PassAs<StateVersion, u8>,
	) -> AllocateAndReturnPointer<H256, 32> {
		match version {
			StateVersion::V0 => LayoutV0::<sp_core::KeccakHasher>::ordered_trie_root(input),
			StateVersion::V1 => LayoutV1::<sp_core::KeccakHasher>::ordered_trie_root(input),
		}
	}

	/// A trie root formed from the enumerated items.
	#[version(3)]
	#[raw_api]
	fn keccak_256_ordered_root(
		input: PassFatPointerAndDecode<Vec<Vec<u8>>>,
		version: PassAs<StateVersion, u8>,
		out: PassPointerAndWrite<&mut H256, 32>,
	) {
		let root = match version {
			StateVersion::V0 => LayoutV0::<sp_core::KeccakHasher>::ordered_trie_root(input),
			StateVersion::V1 => LayoutV1::<sp_core::KeccakHasher>::ordered_trie_root(input),
		};
		out.0.copy_from_slice(&root.0);
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `keccak_256_ordered_root` host function.
	#[wrapper]
	fn keccak_256_ordered_root(data: Vec<Vec<u8>>, state_version: StateVersion) -> H256 {
		let mut root = H256::default();
		keccak_256_ordered_root__raw(data, state_version, &mut root);
		root
	}

	/// Verify trie proof
	fn blake2_256_verify_proof(
		root: PassPointerAndReadCopy<H256, 32>,
		proof: PassFatPointerAndDecodeSlice<&[Vec<u8>]>,
		key: PassFatPointerAndRead<&[u8]>,
		value: PassFatPointerAndRead<&[u8]>,
	) -> bool {
		sp_trie::verify_trie_proof::<LayoutV0<sp_core::Blake2Hasher>, _, _, _>(
			&root,
			proof,
			&[(key, Some(value))],
		)
		.is_ok()
	}

	/// Verify trie proof
	#[version(2)]
	fn blake2_256_verify_proof(
		root: PassPointerAndReadCopy<H256, 32>,
		proof: PassFatPointerAndDecodeSlice<&[Vec<u8>]>,
		key: PassFatPointerAndRead<&[u8]>,
		value: PassFatPointerAndRead<&[u8]>,
		version: PassAs<StateVersion, u8>,
	) -> bool {
		match version {
			StateVersion::V0 => sp_trie::verify_trie_proof::<
				LayoutV0<sp_core::Blake2Hasher>,
				_,
				_,
				_,
			>(&root, proof, &[(key, Some(value))])
			.is_ok(),
			StateVersion::V1 => sp_trie::verify_trie_proof::<
				LayoutV1<sp_core::Blake2Hasher>,
				_,
				_,
				_,
			>(&root, proof, &[(key, Some(value))])
			.is_ok(),
		}
	}

	/// Verify trie proof
	fn keccak_256_verify_proof(
		root: PassPointerAndReadCopy<H256, 32>,
		proof: PassFatPointerAndDecodeSlice<&[Vec<u8>]>,
		key: PassFatPointerAndRead<&[u8]>,
		value: PassFatPointerAndRead<&[u8]>,
	) -> bool {
		sp_trie::verify_trie_proof::<LayoutV0<sp_core::KeccakHasher>, _, _, _>(
			&root,
			proof,
			&[(key, Some(value))],
		)
		.is_ok()
	}

	/// Verify trie proof
	#[version(2)]
	fn keccak_256_verify_proof(
		root: PassPointerAndReadCopy<H256, 32>,
		proof: PassFatPointerAndDecodeSlice<&[Vec<u8>]>,
		key: PassFatPointerAndRead<&[u8]>,
		value: PassFatPointerAndRead<&[u8]>,
		version: PassAs<StateVersion, u8>,
	) -> bool {
		match version {
			StateVersion::V0 => sp_trie::verify_trie_proof::<
				LayoutV0<sp_core::KeccakHasher>,
				_,
				_,
				_,
			>(&root, proof, &[(key, Some(value))])
			.is_ok(),
			StateVersion::V1 => sp_trie::verify_trie_proof::<
				LayoutV1<sp_core::KeccakHasher>,
				_,
				_,
				_,
			>(&root, proof, &[(key, Some(value))])
			.is_ok(),
		}
	}
}
