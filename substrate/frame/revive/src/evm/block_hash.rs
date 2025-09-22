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
//!Types, and traits to integrate pallet-revive with EVM.
#![warn(missing_docs)]

use crate::evm::{
	Block, HashesOrTransactionInfos, TYPE_EIP1559, TYPE_EIP2930, TYPE_EIP4844, TYPE_EIP7702,
};

use alloc::{vec, vec::Vec};
use alloy_core::{
	primitives::{bytes::BufMut, B256},
	rlp,
};
use alloy_trie::{
	hash_builder::{HashBuilderValue, HashBuilderValueRef},
	nodes::RlpNode,
	HashBuilder, Nibbles, TrieMask,
};
use codec::{Decode, Encode};
use frame_support::weights::Weight;
use scale_info::TypeInfo;
use sp_core::{keccak_256, H160, H256, U256};

/// Details needed to reconstruct the receipt info in the RPC
/// layer without losing accuracy.
#[derive(Encode, Decode, TypeInfo, Clone, Debug, PartialEq, Eq)]
pub struct ReceiptGasInfo {
	/// The amount of gas used for this specific transaction alone.
	pub gas_used: U256,
}

impl Block {
	/// Compute the trie root using the `(rlp(index), encoded(item))` pairs.
	pub fn compute_trie_root(items: &[Vec<u8>]) -> B256 {
		alloy_consensus::proofs::ordered_trie_root_with_encoder(items, |item, buf| {
			buf.put_slice(item)
		})
	}

	/// Compute the ETH header hash.
	pub fn header_hash(&self) -> H256 {
		// Note: Cap the gas limit to u64::MAX.
		// In practice, it should be impossible to fill a u64::MAX gas limit
		// of an either Ethereum or Substrate block.
		let gas_limit = self.gas_limit.try_into().unwrap_or(u64::MAX);

		let alloy_header = alloy_consensus::Header {
			state_root: self.state_root.0.into(),
			transactions_root: self.transactions_root.0.into(),
			receipts_root: self.receipts_root.0.into(),

			parent_hash: self.parent_hash.0.into(),
			beneficiary: self.miner.0.into(),
			number: self.number.as_u64(),
			logs_bloom: self.logs_bloom.0.into(),
			gas_limit,
			gas_used: self.gas_used.as_u64(),
			timestamp: self.timestamp.as_u64(),

			ommers_hash: self.sha_3_uncles.0.into(),
			extra_data: self.extra_data.clone().0.into(),
			mix_hash: self.mix_hash.0.into(),
			nonce: self.nonce.0.into(),

			base_fee_per_gas: Some(self.base_fee_per_gas.as_u64()),
			withdrawals_root: Some(self.withdrawals_root.0.into()),
			blob_gas_used: Some(self.blob_gas_used.as_u64()),
			excess_blob_gas: Some(self.excess_blob_gas.as_u64()),
			parent_beacon_block_root: self.parent_beacon_block_root.map(|root| root.0.into()),
			requests_hash: self.requests_hash.map(|hash| hash.0.into()),

			..Default::default()
		};

		alloy_header.hash_slow().0.into()
	}
}

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
	/// RLP encoded value.
	pub first_value: Option<Vec<u8>>,
}

impl IncrementalHashBuilder {
	/// Construct the hash builder from the first value.
	pub fn new(first_value: Vec<u8>) -> Self {
		Self { hash_builder: HashBuilder::default(), index: 1, first_value: Some(first_value) }
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

		IncrementalHashBuilder {
			hash_builder,
			index: serialized.index,
			first_value: serialized.first_value,
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
			first_value: self.first_value,
		}
	}

	/// Add a new value to the hash builder.
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

		self.index += 1;
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

/// Accumulate events (logs) into a stream of RLP encoded bytes.
/// This is a very straight forward implementation that RLP encodes logs as they are added.
///
/// The main goal is to generate the RLP-encoded representation of receipts
/// which is required to compute the receipt root hash, without storing the full receipt
/// data in memory.
///
/// One approach is to store the full receipt in memory, together with the RLP encoding
/// of the receipt.
///
/// However, since we only care about the RLP encoding of the receipt, we can optimize the memory
/// usage by only storing the RLP encoded value and the logs directly. This effectively saves
/// the need to store the full receipt (which can grow unboundedly due to the number of logs), and
/// builds the RLP encoding incrementally as logs are added.
///
/// The implementation leverages the RLP encoding details of the receipt:
///
/// ```ignore
/// // Memory representation of the RLP encoded receipt:
/// [
/// 	ReceiptHeader ++ rlp(status) ++ rlp(gas) ++ rlp(bloom)
/// 			++ LogsHeader ++ rlp(log1) ++ rlp(log2) ++ ... ++ rlp(logN)
/// ]
/// ```
///
/// The optimization comes from the fact that `rlp(log1) ++ rlp(log2) ++ ... ++ rlp(logN)`
/// can be built incrementally.
///
/// On average, from the real ethereum block, this implementation reduces the memory usage by 30%.
///  `EncodedReceipt Space optimization (on average): 0.6995642434146292`
pub struct AccumulateReceipt {
	/// The RLP bytes where the logs are accumulated.
	pub encoding: Vec<u8>,
	/// The bloom filter collected from accumulating logs.
	pub bloom: LogsBloom,
}

/// Bloom log filter compatible with Ethereum implementation.
///
/// This structure avoids conversions between substrate to alloy types
/// to optimally compute the bloom.
#[derive(Clone, Copy)]
pub struct LogsBloom {
	/// The bloom bytes used to store logs.
	pub bloom: [u8; BLOOM_SIZE_BYTES],
}

impl Default for LogsBloom {
	fn default() -> Self {
		Self::new()
	}
}

impl LogsBloom {
	/// Constructs a new [`LogsBloom`].
	pub const fn new() -> Self {
		Self { bloom: [0u8; BLOOM_SIZE_BYTES] }
	}

	/// Ingests a raw log (event) into the bloom filter.
	pub fn accrue_log(&mut self, contract: &H160, topics: &[H256]) {
		Self::m3_2048(&mut self.bloom, contract.as_ref());

		for topic in topics {
			Self::m3_2048(&mut self.bloom, topic.as_ref());
		}
	}

	/// Accrues the input into the bloom filter.
	pub fn accrue_bloom(&mut self, other: &Self) {
		for i in 0..BLOOM_SIZE_BYTES {
			self.bloom[i] |= other.bloom[i];
		}
	}

	/// Specialized Bloom filter that sets three bits out of 2048, given an
	/// arbitrary byte sequence.
	///
	/// See Section 4.3.1 "Transaction Receipt" of the
	/// [Ethereum Yellow Paper][ref] (page 6).
	///
	/// [ref]: https://ethereum.github.io/yellowpaper/paper.pdf
	fn m3_2048(bloom: &mut [u8; 256], bytes: &[u8]) {
		let hash = keccak_256(bytes);
		for i in [0, 2, 4] {
			let bit = (hash[i + 1] as usize + ((hash[i] as usize) << 8)) & 0x7FF;
			bloom[256 - 1 - bit / 8] |= 1 << (bit % 8);
		}
	}
}

impl AccumulateReceipt {
	/// Constructs a new [`AccumulateReceipt`].
	pub const fn new() -> Self {
		Self { encoding: Vec::new(), bloom: LogsBloom::new() }
	}

	/// Add the log into the accumulated receipt.
	///
	/// This accrues the log bloom and keeps track of the RLP encoding of the log.
	pub fn add_log(&mut self, contract: &H160, data: &[u8], topics: &[H256]) {
		// Accrue the log bloom.
		self.bloom.accrue_log(contract, topics);

		// Determine the length of the log RLP encoding.
		let mut topics_len = 0;
		for topic in topics {
			// Topics are represented by 32 bytes. However, their encoding
			// can produce different lengths depending on their value.
			topics_len += rlp::Encodable::length(&topic.0);
		}
		// Account for the size of the list header.
		let topics_list_header_length = topics_len + rlp::length_of_length(topics_len);
		// Compute the total payload length of the log.
		let payload_length = rlp::Encodable::length(&contract.0) +
			rlp::Encodable::length(&data) +
			topics_list_header_length;

		let header = rlp::Header { list: true, payload_length };
		header.encode(&mut self.encoding);
		rlp::Encodable::encode(&contract.0, &mut self.encoding);
		// Encode the topics as a list
		rlp::Header { list: true, payload_length: topics_len }.encode(&mut self.encoding);
		for topic in topics {
			rlp::Encodable::encode(&topic.0, &mut self.encoding);
		}
		rlp::Encodable::encode(&data, &mut self.encoding);
	}

	/// Finalize the accumulated receipt and return the RLP encoded bytes.
	pub fn encoded_receipt(
		encoded_logs: Vec<u8>,
		bloom: LogsBloom,
		status: bool,
		gas: u64,
		transaction_type: Vec<u8>,
	) -> Vec<u8> {
		let logs_length = encoded_logs.len();
		let list_header_length = logs_length + rlp::length_of_length(logs_length);

		let header = rlp::Header {
			list: true,
			payload_length: rlp::Encodable::length(&status) +
				rlp::Encodable::length(&gas) +
				rlp::Encodable::length(&bloom.bloom) +
				list_header_length,
		};

		let mut encoded = transaction_type;
		header.encode(&mut encoded);
		rlp::Encodable::encode(&status, &mut encoded);
		rlp::Encodable::encode(&gas, &mut encoded);
		rlp::Encodable::encode(&bloom.bloom, &mut encoded);

		let logs_header = rlp::Header { list: true, payload_length: logs_length };
		logs_header.encode(&mut encoded);

		encoded.extend(encoded_logs);

		encoded
	}
}

/// Number of bytes that a bloom stores.
const BLOOM_SIZE_BYTES: usize = 256;

/// The intermediate representation of the Ethereum block builder.
#[derive(Encode, Decode, TypeInfo)]
pub struct EthereumBlockBuilderIR {
	transaction_root_builder: Option<IncrementalHashBuilderIR>,
	receipts_root_builder: Option<IncrementalHashBuilderIR>,

	gas_used: U256,
	pub(crate) tx_hashes: Vec<H256>,

	logs_bloom: [u8; BLOOM_SIZE_BYTES],
	pub(crate) gas_info: Vec<ReceiptGasInfo>,
}

impl Default for EthereumBlockBuilderIR {
	fn default() -> Self {
		Self {
			transaction_root_builder: None,
			receipts_root_builder: None,
			gas_used: U256::zero(),
			tx_hashes: Vec::new(),
			logs_bloom: [0; BLOOM_SIZE_BYTES],
			gas_info: Vec::new(),
		}
	}
}

/// Ethereum block builder.
pub struct EthereumBlockBuilder {
	pub(crate) transaction_root_builder: Option<IncrementalHashBuilder>,
	pub(crate) receipts_root_builder: Option<IncrementalHashBuilder>,

	gas_used: U256,
	pub(crate) tx_hashes: Vec<H256>,

	logs_bloom: LogsBloom,
	gas_info: Vec<ReceiptGasInfo>,
}

impl EthereumBlockBuilder {
	/// Constructs a new [`EthereumBlockBuilder`].
	pub const fn new() -> Self {
		Self {
			transaction_root_builder: None,
			receipts_root_builder: None,
			gas_used: U256::zero(),
			tx_hashes: Vec::new(),
			logs_bloom: LogsBloom::new(),
			gas_info: Vec::new(),
		}
	}

	/// Converts the builder into an intermediate representation.
	pub fn to_ir(self) -> EthereumBlockBuilderIR {
		EthereumBlockBuilderIR {
			transaction_root_builder: self.transaction_root_builder.map(|b| b.to_ir()),
			receipts_root_builder: self.receipts_root_builder.map(|b| b.to_ir()),
			gas_used: self.gas_used,
			tx_hashes: self.tx_hashes,
			logs_bloom: self.logs_bloom.bloom,
			gas_info: self.gas_info,
		}
	}

	/// Converts the intermediate representation back into a builder.
	pub fn from_ir(ir: EthereumBlockBuilderIR) -> Self {
		Self {
			transaction_root_builder: ir
				.transaction_root_builder
				.map(|b| IncrementalHashBuilder::from_ir(b)),
			receipts_root_builder: ir
				.receipts_root_builder
				.map(|b| IncrementalHashBuilder::from_ir(b)),
			gas_used: ir.gas_used,
			tx_hashes: ir.tx_hashes,
			logs_bloom: LogsBloom { bloom: ir.logs_bloom },
			gas_info: ir.gas_info,
		}
	}

	/// Reset the state of the block builder to accommodate for the next block.
	pub fn reset(&mut self) {
		*self = Self::new();
	}

	/// Process a single transaction at a time.
	pub fn process_transaction(
		&mut self,
		transaction_encoded: Vec<u8>,
		success: bool,
		gas_used: Weight,
		encoded_logs: Vec<u8>,
		receipt_bloom: LogsBloom,
	) {
		let tx_hash = H256(keccak_256(&transaction_encoded));
		self.tx_hashes.push(tx_hash);

		// Update the transaction trie.
		let transaction_type = Self::extract_transaction_type(transaction_encoded.as_slice());
		Self::add_builder_value(&mut self.transaction_root_builder, transaction_encoded);

		// Update gas and logs bloom.
		self.gas_used = self.gas_used.saturating_add(gas_used.ref_time().into());
		self.logs_bloom.accrue_bloom(&receipt_bloom);

		// Update the receipt trie.
		let encoded_receipt = AccumulateReceipt::encoded_receipt(
			encoded_logs,
			receipt_bloom,
			success,
			self.gas_used.as_u64(),
			transaction_type,
		);
		Self::add_builder_value(&mut self.receipts_root_builder, encoded_receipt);

		self.gas_info.push(ReceiptGasInfo { gas_used: gas_used.ref_time().into() });
	}

	/// Build the ethereum block from provided data.
	pub fn build(
		&mut self,
		block_number: U256,
		parent_hash: H256,
		timestamp: U256,
		block_author: H160,
		gas_limit: U256,
	) -> (Block, Vec<ReceiptGasInfo>) {
		let transactions_root = Self::compute_trie_root(&mut self.transaction_root_builder);
		let receipts_root = Self::compute_trie_root(&mut self.receipts_root_builder);

		let tx_hashes = core::mem::replace(&mut self.tx_hashes, Vec::new());
		let gas_info = core::mem::replace(&mut self.gas_info, Vec::new());

		let mut block = Block {
			number: block_number,
			parent_hash,
			timestamp,
			miner: block_author,
			gas_limit,

			state_root: transactions_root,
			transactions_root,
			receipts_root,

			gas_used: self.gas_used,

			logs_bloom: self.logs_bloom.bloom.into(),
			transactions: HashesOrTransactionInfos::Hashes(tx_hashes),

			..Default::default()
		};

		let block_hash = block.header_hash();
		block.hash = block_hash;

		(block, gas_info)
	}

	fn compute_trie_root(builder: &mut Option<IncrementalHashBuilder>) -> H256 {
		match builder {
			Some(builder) => builder.finish(),
			None => HashBuilder::default().root().0.into(),
		}
	}

	fn add_builder_value(builder: &mut Option<IncrementalHashBuilder>, value: Vec<u8>) {
		match builder {
			Some(builder) => builder.add_value(value),
			None => *builder = Some(IncrementalHashBuilder::new(value)),
		}
	}

	fn extract_transaction_type(transaction_encoded: &[u8]) -> Vec<u8> {
		// The transaction type is the first byte from the encoded transaction,
		// when the transaction is not legacy. For legacy transactions, there's
		// no type defined. Additionally, the RLP encoding of the tx type byte
		// is identical to the tx type.
		transaction_encoded
			.first()
			.cloned()
			.map(|first| match first {
				TYPE_EIP2930 | TYPE_EIP1559 | TYPE_EIP4844 | TYPE_EIP7702 => vec![first],
				_ => vec![],
			})
			.unwrap_or_default()
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::evm::{Block, ReceiptInfo};
	use alloy_trie::{HashBuilder, Nibbles};

	/// Manual implementation of the Ethereum trie root computation.
	///
	/// Given the RLP encoded values, the implementation adjusts the
	/// index to account for RLP encoding rules.
	fn manual_trie_root_compute(encoded: Vec<Vec<u8>>) -> H256 {
		const fn adjust_index_for_rlp(i: usize, len: usize) -> usize {
			if i > 0x7f {
				i
			} else if i == 0x7f || i + 1 == len {
				0
			} else {
				i + 1
			}
		}

		let mut hb = HashBuilder::default();

		let items_len = encoded.len();
		for i in 0..items_len {
			let index = adjust_index_for_rlp(i, items_len);

			let index_buffer = rlp::encode_fixed_size(&index);
			hb.add_leaf(Nibbles::unpack(&index_buffer), &encoded[index]);

			// Each mask in these vectors holds a u16.
			let masks_len = (hb.state_masks.len() + hb.tree_masks.len() + hb.hash_masks.len()) * 2;
			let _size = hb.key.len() +
				hb.value.as_slice().len() +
				hb.stack.len() * 33 +
				masks_len + hb.rlp_buf.len();
		}

		hb.root().0.into()
	}

	/// The test compares three hashing options:
	/// - Block::compute_trie_root: this uses the consensus proofs crate
	/// - manual_trie_root_compute: this ensures the keys are added in the correct order
	/// - IncrementalHashBuilder: this offers the most compact storage option
	///
	/// The above hashes must be identical. While at it, the incremental hash
	/// builder is serialized and deserialized to ensure consistency.
	#[test]
	fn incremental_hasher() {
		const UPPER_BOUND: usize = 256;
		const RLP_VALUE_SIZE: usize = 128;

		let mut rlp_values = Vec::with_capacity(UPPER_BOUND);

		for i in 0..UPPER_BOUND {
			// Simulate an RLP value repeated for `i`.
			let rlp_value = vec![i as u8; RLP_VALUE_SIZE];

			rlp_values.push(rlp_value);

			let block_hash: H256 = Block::compute_trie_root(&rlp_values).0.into();
			let manual_hash = manual_trie_root_compute(rlp_values.clone());

			let mut builder = IncrementalHashBuilder::new(rlp_values[0].clone());
			for rlp_value in rlp_values.iter().skip(1) {
				builder.add_value(rlp_value.clone());

				let ir_builder = builder.to_ir();
				builder = IncrementalHashBuilder::from_ir(ir_builder);
			}
			let incremental_hash = builder.finish();

			assert_eq!(block_hash, manual_hash);
			assert_eq!(block_hash, incremental_hash);
		}
	}

	#[test]
	fn test_alloy_rlp_ordering_compatibility() {
		let zero_encoded = rlp::encode_fixed_size(&0usize);
		let max_single_byte = rlp::encode_fixed_size(&127usize);
		let first_multi_byte = rlp::encode_fixed_size(&128usize);

		// Document the exact bytes we expect
		assert_eq!(zero_encoded.as_slice(), &[0x80]); // RLP encoding of 0
		assert_eq!(max_single_byte.as_slice(), &[0x7f]); // RLP encoding of 127
		assert_eq!(first_multi_byte.as_slice(), &[0x81, 0x80]); // RLP encoding of 128

		// Verify ordering
		assert!(max_single_byte < zero_encoded);
		assert!(zero_encoded < first_multi_byte);
	}

	#[test]
	fn ensure_identical_hashes() {
		// curl -X POST --data '{"jsonrpc":"2.0","method":"eth_getBlockByNumber","params":["0x161bd0f", true],"id":1}' -H "Content-Type: application/json" https://ethereum-rpc.publicnode.com | jq .result
		const BLOCK_PATH: &str = "./test-assets/eth_block.json";
		// curl -X POST --data '{"jsonrpc":"2.0","method":"eth_getBlockReceipts","params":["0x161bd0f"],"id":1}' -H "Content-Type: application/json" https://ethereum-rpc.publicnode.com | jq .result
		const BLOCK_RECEIPTS: &str = "./test-assets/eth_receipts.json";

		let json = std::fs::read_to_string(BLOCK_PATH).unwrap();
		let block: Block = serde_json::from_str(&json).unwrap();

		let json = std::fs::read_to_string(BLOCK_RECEIPTS).unwrap();
		let receipts: Vec<ReceiptInfo> = serde_json::from_str(&json).unwrap();

		assert_eq!(block.header_hash(), receipts[0].block_hash);

		let tx = match &block.transactions {
			HashesOrTransactionInfos::TransactionInfos(infos) => infos.clone(),
			_ => panic!("Expected full tx body"),
		};

		let encoded_tx: Vec<_> = tx
			.clone()
			.into_iter()
			.map(|tx| tx.transaction_signed.signed_payload())
			.collect();

		let transaction_details: Vec<_> = tx
			.into_iter()
			.zip(receipts.into_iter())
			.map(|(tx_info, receipt_info)| {
				if tx_info.transaction_index != receipt_info.transaction_index {
					panic!("Transaction and receipt index do not match");
				}

				let logs: Vec<_> = receipt_info
					.logs
					.into_iter()
					.map(|log| (log.address, log.data.unwrap_or_default().0, log.topics))
					.collect();

				(
					tx_info.transaction_signed.signed_payload(),
					logs,
					receipt_info.status.unwrap_or_default() == 1.into(),
					receipt_info.gas_used.as_u64(),
				)
			})
			.collect();

		// Build the ethereum block incrementally.
		let mut incremental_block = EthereumBlockBuilder::new();
		for (signed, logs, success, gas_used) in transaction_details {
			let mut log_size = 0;

			let mut accumulate_receipt = AccumulateReceipt::new();
			for (address, data, topics) in &logs {
				let current_size = data.len() + topics.len() * 32 + 20;
				log_size += current_size;
				accumulate_receipt.add_log(address, data, topics);
			}

			incremental_block.process_transaction(
				signed,
				success,
				gas_used.into(),
				accumulate_receipt.encoding,
				accumulate_receipt.bloom,
			);

			let ir = incremental_block.to_ir();
			incremental_block = EthereumBlockBuilder::from_ir(ir);

			println!(" Otherwise size {:?}", log_size);
		}

		// The block hash would differ here because we don't take into account
		// the ommers and other fields from the substrate perspective.
		// However, the state roots must be identical.
		let built_block = incremental_block
			.build(
				block.number,
				block.parent_hash,
				block.timestamp,
				block.miner,
				Default::default(),
			)
			.0;

		assert_eq!(built_block.gas_used, block.gas_used);
		assert_eq!(built_block.logs_bloom, block.logs_bloom);
		// We are using the tx root for state root.
		assert_eq!(built_block.state_root, built_block.transactions_root);

		// Double check the receipts roots.
		assert_eq!(built_block.receipts_root, block.receipts_root);

		let manual_hash = manual_trie_root_compute(encoded_tx.clone());

		let mut total_size = 0;
		for enc in &encoded_tx {
			total_size += enc.len();
		}
		println!("Total size used by transactions: {:?}", total_size);

		let mut builder = IncrementalHashBuilder::new(encoded_tx[0].clone());
		for tx in encoded_tx.iter().skip(1) {
			builder.add_value(tx.clone())
		}
		let incremental_hash = builder.finish();

		println!("Incremental hash: {:?}", incremental_hash);
		println!("Manual Hash: {:?}", manual_hash);
		println!("Built block Hash: {:?}", built_block.transactions_root);
		println!("Real Block Tx Hash: {:?}", block.transactions_root);

		assert_eq!(incremental_hash, block.transactions_root);

		// This double checks the compute logic.
		assert_eq!(manual_hash, block.transactions_root);
		// This ensures we can compute the same transaction root as Ethereum.
		assert_eq!(block.transactions_root, built_block.transactions_root);
	}
}
