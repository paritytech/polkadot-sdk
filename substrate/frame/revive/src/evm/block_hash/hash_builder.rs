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

//! Incremental hash builder for Ethereum transaction and receipt trie roots.

use alloc::vec::Vec;
use alloy_core::rlp;
use alloy_trie::{
	hash_builder::{HashBuilderValue, HashBuilderValueRef},
	nodes::RlpNode,
	HashBuilder, Nibbles, TrieMask,
};
use codec::{Decode, Encode};
use sp_core::H256;

/// The Incremental Hash Builder is designed to efficiently compute the transaction and receipt
/// trie roots in Ethereum, minimizing memory usage. This is achieved by constructing the Merkle
/// Trie incrementally, rather than storing all values in memory simultaneously.
///
/// ## ETH Trie Overview
///
/// In Ethereum, the trie calculates the hash of a node (leaf) by combining the remaining key path
/// with the RLP-encoded item, as follows:
///
/// ```ignore
/// 	hash (remaining of the key path ++ RLP (item))
/// ```
///
/// Because the hash incorporates the remaining key path, computing the trie root accurately
/// requires more than just the hash of the RLP-encoded item (hash(RLP(item))). To address this, the
/// Incremental Hash Builder leverages the internal structure of the Ethereum trie to optimize
/// memory usage.
///
/// The Ethereum trie is ordered by the RLP-encoded index of items (RLP(index)). This ordering
/// allows the trie to be built incrementally, provided the items are added in a consistent order.
/// We leverage the following property of encoding RLP indexes to avoid sorting the items (and
/// therefore, we avoid knowing the number of items in advance):
///
/// ```ignore
/// rlp(1) < rlp(2) < ... < rlp(127) < RLP (0) < rlp(128) < ... < rlp(n)
/// ```
/// For more details see:
/// <https://github.com/alloy-rs/trie/blob/3e762bcb65f25710c309e7d8cb6c9ed7e3fdada1/src/root.rs#L7-L16>
///
/// This property allows the builder to add items in the order of indices 1, 2, ..., 127, followed
/// by index 0, and then index 128 onward. In this implementation, the focus is on placing the first
/// RLP encoded value at index 128.
///
/// The primary optimization comes from computing the hash (remaining_key_path ++ RLP(item)) as
/// early as possible during the trie construction process. This approach minimizes the memory
/// required by avoiding the need to store all items simultaneously.
///
/// For transactions, from real ethereum block, we can observe the following:
///  - worst case we use 90% less space
///  - best case we use 99.5% less space
///
/// ```ignore
///  hash max 8042
///  hash min 444
///  hash total 79655
///  hash saved worst case 0.1009603916891595
///  hash saved best case 0.005574038039043374
/// ```
///
/// For receipts, from real ethereum block, we can observe the following:
/// - worst case we use 94% less space
/// - best case we use 99.3% less space
///
/// ```ignore
///  hash max 7249
///  hash min 760
///  hash total 106054
///  hash saved worst case 0.06835197163709054
///  hash saved best case 0.007166160635148132
/// ```
pub struct IncrementalHashBuilder {
	/// Hash builder.
	hash_builder: HashBuilder,
	/// The index of the current value.
	index: u64,
	/// RLP encoded value.
	first_value: Option<Vec<u8>>,
}

impl IncrementalHashBuilder {
	/// Construct the hash builder from the first value.
	pub fn new() -> Self {
		Self { hash_builder: HashBuilder::default(), index: 1, first_value: None }
	}

	/// Converts the intermediate representation back into a builder.
	pub fn from_ir(serialized: IncrementalHashBuilderIR) -> Self {
		let value = match serialized.value_type {
			0 => {
				let mut value = HashBuilderValue::new();
				value.set_bytes_owned(serialized.builder_value);
				value
			},
			1 => {
				let buffer: alloy_core::primitives::B256 = serialized.builder_value[..]
					.try_into()
					.expect("The buffer was serialized properly; qed");
				let value_ref = HashBuilderValueRef::Hash(&buffer);

				let mut value = HashBuilderValue::new();
				value.set_from_ref(value_ref);
				value
			},
			_ => panic!("Value type was serialized properly; qed"),
		};

		let hash_builder = HashBuilder {
			key: Nibbles::from_nibbles(serialized.key),
			value,
			stack: serialized
				.stack
				.into_iter()
				.map(|raw| RlpNode::from_raw(&raw).expect("RlpNode was encoded properly; qed"))
				.collect(),
			state_masks: serialized
				.state_masks
				.into_iter()
				.map(|mask| TrieMask::new(mask))
				.collect(),
			tree_masks: serialized.tree_masks.into_iter().map(|mask| TrieMask::new(mask)).collect(),
			hash_masks: serialized.hash_masks.into_iter().map(|mask| TrieMask::new(mask)).collect(),
			stored_in_database: serialized.stored_in_database,
			updated_branch_nodes: None,
			proof_retainer: None,
			rlp_buf: serialized.rlp_buf,
		};

		IncrementalHashBuilder { hash_builder, index: serialized.index, first_value: None }
	}

	/// Converts the builder into an intermediate representation.
	pub fn to_ir(self) -> IncrementalHashBuilderIR {
		IncrementalHashBuilderIR {
			key: self.hash_builder.key.to_vec(),
			value_type: match self.hash_builder.value.as_ref() {
				HashBuilderValueRef::Bytes(_) => 0,
				HashBuilderValueRef::Hash(_) => 1,
			},
			builder_value: self.hash_builder.value.as_slice().to_vec(),
			stack: self.hash_builder.stack.into_iter().map(|n| n.as_slice().to_vec()).collect(),

			state_masks: self.hash_builder.state_masks.into_iter().map(|mask| mask.get()).collect(),
			tree_masks: self.hash_builder.tree_masks.into_iter().map(|mask| mask.get()).collect(),
			hash_masks: self.hash_builder.hash_masks.into_iter().map(|mask| mask.get()).collect(),

			stored_in_database: self.hash_builder.stored_in_database,
			rlp_buf: self.hash_builder.rlp_buf,
			index: self.index,
		}
	}

	/// Add a new value to the hash builder.
	///
	/// The value is returned if it should be preserved until a later time.
	pub fn add_value(&mut self, value: Vec<u8>) {
		let rlp_index = rlp::encode_fixed_size(&self.index);
		self.hash_builder.add_leaf(Nibbles::unpack(&rlp_index), &value);

		if self.index == 0x7f {
			// Pushing the previous item since we are expecting the index
			// to be index + 1 in the sorted order.
			if let Some(encoded_value) = self.first_value.take() {
				let rlp_index = rlp::encode_fixed_size(&0usize);

				self.hash_builder.add_leaf(Nibbles::unpack(&rlp_index), &encoded_value);
			}
		}

		self.index = self.index.saturating_add(1);
	}

	/// Load the first value from storage.
	pub fn set_first_value(&mut self, value: Vec<u8>) {
		self.first_value = Some(value);
	}

	/// Check if we should load the first value from storage.
	pub fn needs_first_value(&self, phase: BuilderPhase) -> bool {
		match phase {
			BuilderPhase::ProcessingValue => self.index == 0x7f,
			BuilderPhase::Build => self.index < 0x7f,
		}
	}

	/// Build the trie root hash.
	pub fn finish(&mut self) -> H256 {
		// We have less than 0x7f items to the trie. Therefore, the
		// first value index is the last one in the sorted vector
		// by rlp encoding of the index.
		if let Some(encoded_value) = self.first_value.take() {
			let rlp_index = rlp::encode_fixed_size(&0usize);
			self.hash_builder.add_leaf(Nibbles::unpack(&rlp_index), &encoded_value);
		}

		self.hash_builder.root().0.into()
	}
}

/// The phase in which the hash builder is currently operating.
pub enum BuilderPhase {
	/// Processing a value, unknown at the moment if more values will come.
	ProcessingValue,
	/// The trie hash is being finalized, no more values will be added.
	Build,
}

/// The intermediate representation of the [`IncrementalHashBuilder`] that can be placed into the
/// pallets storage. This contains the minimum amount of data that is needed to serialize
/// and deserialize the incremental hash builder.
#[derive(Encode, Decode, scale_info::TypeInfo, Clone, PartialEq, Eq, Debug)]
pub struct IncrementalHashBuilderIR {
	/// The nibbles of the builder.
	pub key: Vec<u8>,
	/// The type of the builder value.
	/// 0 represents plain bytes.
	/// 1 represents the hash of the bytes.
	pub value_type: u8,
	/// The current value stored by the builder.
	pub builder_value: Vec<u8>,
	/// The stack of RLP nodes.
	pub stack: Vec<Vec<u8>>,
	/// State mask.
	pub state_masks: Vec<u16>,
	/// Tree mask.
	pub tree_masks: Vec<u16>,
	/// Hash mask.
	pub hash_masks: Vec<u16>,
	/// True if the buider should be stored in database.
	pub stored_in_database: bool,
	/// Current RLP buffer.
	pub rlp_buf: Vec<u8>,
	/// The index of the current value.
	pub index: u64,
}

impl Default for IncrementalHashBuilderIR {
	fn default() -> Self {
		Self {
			// First deserialization time from the pallet storage, is expected
			// to contain index 1.
			index: 1,
			key: Vec::new(),
			value_type: 0,
			builder_value: Vec::new(),
			stack: Vec::new(),
			state_masks: Vec::new(),
			tree_masks: Vec::new(),
			hash_masks: Vec::new(),
			stored_in_database: false,
			rlp_buf: Vec::new(),
		}
	}
}
