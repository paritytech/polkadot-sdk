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

const LOG_TARGET: &str = "runtime::revive::hash_builder";

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
	/// Optional stats for testing purposes.
	#[cfg(test)]
	stats: Option<HashBuilderStats>,
}

impl Default for IncrementalHashBuilder {
	fn default() -> Self {
		Self {
			// First deserialization time from the pallet storage, is expected
			// to contain index 1.
			index: 1,
			hash_builder: HashBuilder::default(),
			first_value: None,
			#[cfg(test)]
			stats: None,
		}
	}
}

/// Accounting data for the hash builder, used for testing and analysis.
#[cfg(test)]
#[derive(Debug, Clone)]
pub struct HashBuilderStats {
	/// Total size of data fed to the hash builder via add_value.
	pub total_data_size: usize,
	/// Current size of the hash builder state.
	pub hb_current_size: usize,
	/// Maximum size the hash builder has reached: (size, index).
	pub hb_max_size: (usize, u64),
	/// Largest individual data size passed to add_value: (size, index).
	pub largest_data: (usize, u64),
}

#[cfg(test)]
impl core::fmt::Display for HashBuilderStats {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		writeln!(
			f,
			"  Total data processed: {} bytes ({:.2} MB)",
			self.total_data_size,
			self.total_data_size as f64 / 1_048_576.0
		)?;
		writeln!(
			f,
			"  Current HB size: {} bytes ({:.2} KB)",
			self.hb_current_size,
			self.hb_current_size as f64 / 1024.0
		)?;
		writeln!(
			f,
			"  Max HB size: {:?} ({:.2} KB at index {})",
			self.hb_max_size,
			self.hb_max_size.0 as f64 / 1024.0,
			self.hb_max_size.1
		)?;
		writeln!(f, "  Largest data item: {:?}", self.largest_data)?;
		write!(
			f,
			"  Memory efficiency: {:.4}% (current size vs total data)",
			(self.hb_current_size as f64 / self.total_data_size as f64) * 100.0
		)
	}
}

impl IncrementalHashBuilder {
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

		IncrementalHashBuilder {
			hash_builder,
			index: serialized.index,
			first_value: None,
			#[cfg(test)]
			stats: None,
		}
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

		#[cfg(test)]
		self.process_stats(value.len(), self.index);

		if self.index == 0x7f {
			// Pushing the previous item since we are expecting the index
			// to be index + 1 in the sorted order.

			let encoded_value = self
				.first_value
				.take()
				.expect("First value must be set when processing index 127; qed");

			log::debug!(target: LOG_TARGET, "Adding first value at index 0 while processing index 127");

			let rlp_index = rlp::encode_fixed_size(&0usize);
			self.hash_builder.add_leaf(Nibbles::unpack(&rlp_index), &encoded_value);

			// Update accounting if enabled
			#[cfg(test)]
			self.process_stats(value.len(), 0);
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
			log::debug!(target: LOG_TARGET, "Adding first value at index 0 while building the trie");

			let rlp_index = rlp::encode_fixed_size(&0usize);
			self.hash_builder.add_leaf(Nibbles::unpack(&rlp_index), &encoded_value);

			#[cfg(test)]
			self.process_stats(encoded_value.len(), 0);
		}

		self.hash_builder.root().0.into()
	}

	/// Calculate the current hash builder size without updating accounting.
	#[cfg(test)]
	fn calculate_current_size(&self) -> usize {
		// Each mask in these vectors holds a u16.
		let masks_len = (self.hash_builder.state_masks.len() +
			self.hash_builder.tree_masks.len() +
			self.hash_builder.hash_masks.len()) *
			2;

		self.hash_builder.key.len() +
			self.hash_builder.value.as_slice().len() +
			self.hash_builder.stack.len() * 33 +
			masks_len + self.hash_builder.rlp_buf.len()
	}

	/// Update accounting metrics after processing data.
	#[cfg(test)]
	fn process_stats(&mut self, data_len: usize, index: u64) {
		if self.stats.is_none() {
			return
		}

		let hb_current_size = self.calculate_current_size();
		let stats = self.stats.as_mut().unwrap();

		// Update total data size
		stats.total_data_size += data_len;

		// Update current hash builder size
		stats.hb_current_size = hb_current_size;

		// Track maximum hash builder size and its index
		if hb_current_size > stats.hb_max_size.0 {
			stats.hb_max_size = (hb_current_size, index);
		}

		// Track largest individual data size and its index
		if data_len > stats.largest_data.0 {
			stats.largest_data = (data_len, index);
		}
	}

	/// Enable stats for the hash builder (test-only).
	#[cfg(test)]
	pub fn enable_stats(&mut self) {
		let initial_size = self.calculate_current_size();
		self.stats = Some(HashBuilderStats {
			total_data_size: 0,
			hb_current_size: initial_size,
			hb_max_size: (initial_size, 0),
			largest_data: (0, 0),
		});
	}

	/// Get the accounting data if available (test-only).
	#[cfg(test)]
	pub fn get_stats(&self) -> Option<&HashBuilderStats> {
		self.stats.as_ref()
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

impl IncrementalHashBuilderIR {
	/// Calculate the total size of the IR in bytes.
	#[cfg(test)]
	pub fn calculate_size(&self) -> usize {
		// Fixed-size fields
		let fixed_size = core::mem::size_of::<u64>() + // index
			core::mem::size_of::<u8>() + // value_type
			core::mem::size_of::<bool>(); // stored_in_database

		// Variable-size fields
		let key_size = self.key.len();
		let builder_value_size = self.builder_value.len();
		let stack_size: usize = self.stack.iter().map(|item| item.len()).sum();
		let state_masks_size = self.state_masks.len() * core::mem::size_of::<u16>();
		let tree_masks_size = self.tree_masks.len() * core::mem::size_of::<u16>();
		let hash_masks_size = self.hash_masks.len() * core::mem::size_of::<u16>();
		let rlp_buf_size = self.rlp_buf.len();

		// Vector metadata overhead (capacity info, etc.)
		let vec_overhead = 8 * core::mem::size_of::<usize>(); // 8 Vec structures

		fixed_size +
			key_size + builder_value_size +
			stack_size +
			state_masks_size +
			tree_masks_size +
			hash_masks_size +
			rlp_buf_size +
			vec_overhead
	}
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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_hash_builder_stats() {
		let mut builder = IncrementalHashBuilder::default();
		builder.enable_stats();

		let stats = builder.get_stats().expect("Stats should be enabled");

		assert_eq!(stats.total_data_size, 0);
		let initial_size = stats.hb_current_size;
		assert_eq!(stats.hb_max_size, (initial_size, 0));
		assert_eq!(stats.largest_data, (0, 0));
		let _ = stats;

		// Add some test data (different sizes to test largest tracking)
		let test_data1 = vec![10; 500]; // 500 bytes
		let test_data2 = vec![20; 700]; // 700 bytes
		let test_data3 = vec![30; 300]; // 300 bytes

		builder.set_first_value(test_data1.clone());
		builder.add_value(test_data2.clone());
		builder.add_value(test_data3.clone());
		let _root = builder.finish();

		let stats = builder.get_stats().expect("Stats should be enabled");
		assert_eq!(stats.total_data_size, 1500);
		assert_eq!(stats.hb_max_size.1, 2);
		assert_eq!(stats.largest_data, (700, 1)); // (size, index)
	}

	#[test]
	fn test_hash_builder_without_stats() {
		let mut builder = IncrementalHashBuilder::default();

		// Without enabling stats
		assert!(builder.get_stats().is_none());

		// Adding values should not crash
		builder.add_value(vec![1, 2, 3]);

		// Still no stats
		assert!(builder.get_stats().is_none());
	}

	#[test]
	#[ignore]
	fn test_stats_item_count_and_sizes() {
		for (item_count, item_size) in [
			// 100k items of 1kB each
			(100 * 1024, 1024),
			// 1024 items of 1MB each
			(1024, 1024 * 1024),
			// 5 items of 512 each
			(5, 512),
		] {
			println!("\n=== Testing Hash Builder with {item_count} items ===");

			let mut builder = IncrementalHashBuilder::default();
			builder.enable_stats();

			let initial_stats = builder.get_stats().unwrap();
			println!("Initial size: {} bytes", initial_stats.hb_current_size);

			let test_data = vec![42u8; item_size];

			println!(
				"Adding {} items of {} bytes ({:.2} KB) each...",
				item_count,
				item_size,
				item_size as f64 / 1024.0
			);

			builder.set_first_value(test_data.clone());
			for _ in 0..(item_count - 1) {
				builder.add_value(test_data.clone());
			}
			let final_stats = builder.get_stats().unwrap().clone();
			println!("\nFinal Stats - {item_count} Items of {item_size} bytes each:");
			println!("{}", final_stats);

			let builder_ir = builder.to_ir();
			let ir_size = builder_ir.calculate_size();
			println!("  Builder IR size: {ir_size} bytes ({} KB)", ir_size as f64 / 1024.0);

			// Verify expected values
			let expected_data_size = if item_count > 128 {
				item_count * item_size
			} else {
				// items_count - 1, because the first value is not taken into account (index < 128)
				(item_count - 1) * item_size
			};
			assert_eq!(final_stats.total_data_size, expected_data_size);
			assert!(final_stats.hb_current_size < final_stats.total_data_size);
		}
	}

	#[test]
	fn test_ir_size_calculation() {
		println!("\n=== Testing IncrementalHashBuilderIR Size Calculation ===");

		let mut builder = IncrementalHashBuilder::default();
		builder.enable_stats();

		// Calculate initial IR size (we need to restore the builder after to_ir())
		println!("Initial builder state");
		let initial_ir = builder.to_ir();
		let initial_size = initial_ir.calculate_size();
		println!("Initial IR size: {} bytes", initial_size);
		builder = IncrementalHashBuilder::from_ir(initial_ir);

		// Add some test data and track IR size changes
		let test_data_small = vec![42u8; 100];
		let test_data_large = vec![99u8; 2048];

		builder.set_first_value(test_data_small.clone());
		let ir_after_first = builder.to_ir();
		let size_after_first = ir_after_first.calculate_size();
		println!("IR size after first value: {} bytes", size_after_first);
		builder = IncrementalHashBuilder::from_ir(ir_after_first.clone());

		builder.add_value(test_data_large.clone());
		let ir_after_add = builder.to_ir();
		let size_after_add = ir_after_add.calculate_size();
		println!("IR size after adding large value: {} bytes", size_after_add);

		// Test serialization round-trip
		let restored_builder = IncrementalHashBuilder::from_ir(ir_after_add.clone());
		let restored_ir = restored_builder.to_ir();
		let restored_size = restored_ir.calculate_size();
		println!("IR size after round-trip: {} bytes", restored_size);

		// Verify sizes make sense
		// Note: first value doesn't immediately change IR size, but adding values does
		assert!(size_after_add > size_after_first);
		assert!(size_after_add > initial_size);
		assert_eq!(size_after_add, restored_size);
		assert!(restored_size > 0);
	}
}
