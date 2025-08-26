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
use alloy_consensus::RlpEncodableReceipt;
use alloy_core::primitives::{bytes::BufMut, Bloom, FixedBytes, Log, B256};
use codec::{Decode, Encode};
use frame_support::weights::Weight;
use scale_info::TypeInfo;
use sp_core::{keccak_256, H160, H256, U256};

/// The log emitted by executing the ethereum transaction.
///
/// This is needed to compute the receipt bloom hash.
#[derive(Encode, Decode, TypeInfo, Clone, Debug)]
pub struct EventLog {
	/// The contract that emitted the event.
	pub contract: H160,
	/// Data supplied by the contract. Metadata generated during contract compilation
	/// is needed to decode it.
	pub data: Vec<u8>,
	/// A list of topics used to index the event.
	pub topics: Vec<H256>,
}

/// The transaction details needed to build the ethereum block hash.
#[derive(Encode, Decode, TypeInfo, Clone, Debug)]
pub struct TransactionDetails {
	/// The RLP encoding of the signed transaction.
	pub transaction_encoded: Vec<u8>,
	/// The logs emitted by the transaction.
	pub logs: Vec<EventLog>,
	/// Whether the transaction was successful.
	pub success: bool,
	/// The accurate gas used by the transaction.
	pub gas_used: Weight,
}

/// Details needed to reconstruct the receipt info in the RPC
/// layer without losing accuracy.
#[derive(Encode, Decode, TypeInfo, Clone, Debug, PartialEq, Eq)]
pub struct ReceiptGasInfo {
	/// The amount of gas used for this specific transaction alone.
	pub gas_used: U256,
}

/// A processed transaction by `Block::process_transaction_details`.
struct TransactionProcessed {
	transaction_encoded: Vec<u8>,
	tx_hash: H256,
	gas_info: ReceiptGasInfo,
	encoded_receipt: Vec<u8>,
	receipt_bloom: Bloom,
}

impl Block {
	/// Build the Ethereum block.
	///
	/// # Note
	///
	/// This is an expensive operation.
	///
	/// (I) For each transaction captured (with the unbounded number of events):
	/// - transaction hash is computed using `keccak256`
	/// - transaction is 2718 RLP encoded
	/// - the receipt is constructed and contains all the logs emitted by the transaction
	///   - This includes computing the bloom filter for the logs (O(N) to compute)
	///   - The receipt is 2718 RLP encoded: the cost is O(N) to encode due to the number of logs.
	///
	/// (II) Transaction trie root and receipt trie root are computed.
	///
	/// (III) Block hash is computed from the provided information.
	pub fn build(
		transaction_details: impl IntoIterator<Item = TransactionDetails>,
		block_number: U256,
		parent_hash: H256,
		timestamp: U256,
		block_author: H160,
		gas_limit: U256,
	) -> (H256, Block, Vec<ReceiptGasInfo>) {
		let mut block = Self {
			number: block_number,
			parent_hash,
			timestamp,
			miner: block_author,
			gas_limit,

			// The remaining fields are populated by `process_transaction_details`.
			..Default::default()
		};

		// Needed for computing the transaction root.
		let mut signed_tx = Vec::new();
		// Needed for computing the receipt root.
		let mut receipts = Vec::new();
		// Gas info will be stored in the pallet storage under `ReceiptInfoData`
		// and is needed for reconstructing the Receipts.
		let mut gas_infos = Vec::new();
		// Transaction hashes are placed in the ETH block.
		let mut tx_hashes = Vec::new();
		// Bloom filter for the logs emitted by the transactions.
		let mut logs_bloom = Bloom::default();

		for detail in transaction_details {
			let processed = block.process_transaction_details(detail);

			signed_tx.push(processed.transaction_encoded);
			tx_hashes.push(processed.tx_hash);
			gas_infos.push(processed.gas_info);
			receipts.push(processed.encoded_receipt);
			logs_bloom.accrue_bloom(&processed.receipt_bloom);
		}

		// Compute expensive trie roots.
		let transactions_root = Self::compute_trie_root(&signed_tx);
		let receipts_root = Self::compute_trie_root(&receipts);

		// We use the transaction root as state root since the state
		// root is not yet computed by the substrate block.
		block.state_root = transactions_root.0.into();
		block.transactions_root = transactions_root.0.into();
		block.receipts_root = receipts_root.0.into();
		block.logs_bloom = (*logs_bloom.data()).into();
		block.transactions = HashesOrTransactionInfos::Hashes(tx_hashes);

		// Compute the ETH header hash.
		let block_hash = block.header_hash();

		(block_hash, block, gas_infos)
	}

	/// Returns a tuple of the RLP encoded transaction and receipt.
	///
	/// Internally collects the total gas used.
	fn process_transaction_details(&mut self, detail: TransactionDetails) -> TransactionProcessed {
		let TransactionDetails { transaction_encoded, logs, success, gas_used } = detail;

		let tx_hash = H256(keccak_256(&transaction_encoded));
		// The transaction type is the first byte from the encoded transaction,
		// when the transaction is not legacy. For legacy transactions, there's
		// no type defined. Additionally, the RLP encoding of the tx type byte
		// is identical to the tx type.
		let transaction_type = transaction_encoded
			.first()
			.cloned()
			.map(|first| match first {
				TYPE_EIP2930 | TYPE_EIP1559 | TYPE_EIP4844 | TYPE_EIP7702 => vec![first],
				_ => vec![],
			})
			.unwrap_or_default();

		let logs = logs
			.into_iter()
			.map(|log| {
				Log::new_unchecked(
					log.contract.0.into(),
					log.topics.into_iter().map(|h| FixedBytes::from(h.0)).collect::<Vec<_>>(),
					log.data.into(),
				)
			})
			.collect();

		self.gas_used = self.gas_used.saturating_add(gas_used.ref_time().into());

		let receipt = alloy_consensus::Receipt {
			status: success.into(),
			cumulative_gas_used: self.gas_used.as_u64(),
			logs,
		};

		let receipt_bloom = receipt.bloom_slow();

		// Receipt encoding must be prefixed with the rlp(transaction type).
		let mut encoded_receipt = transaction_type;
		let encoded_len = encoded_receipt
			.len()
			.saturating_add(receipt.rlp_encoded_length_with_bloom(&receipt_bloom));
		encoded_receipt.reserve(encoded_len);

		receipt.rlp_encode_with_bloom(&receipt_bloom, &mut encoded_receipt);

		TransactionProcessed {
			transaction_encoded,
			tx_hash,
			gas_info: ReceiptGasInfo { gas_used: gas_used.ref_time().into() },
			encoded_receipt,
			receipt_bloom,
		}
	}

	/// Compute the trie root using the `(rlp(index), encoded(item))` pairs.
	pub fn compute_trie_root(items: &[Vec<u8>]) -> B256 {
		alloy_consensus::proofs::ordered_trie_root_with_encoder(items, |item, buf| {
			buf.put_slice(item)
		})
	}

	/// Compute the ETH header hash.
	fn header_hash(&self) -> H256 {
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
			// <<<<<<< HEAD
			// 			base_fee_per_gas: self.base_fee_per_gas.map(|gas| gas.as_u64()),
			// 			withdrawals_root: self.withdrawals_root.map(|root| root.0.into()),
			// 			blob_gas_used: self.blob_gas_used.map(|gas| gas.as_u64()),
			// 			excess_blob_gas: self.excess_blob_gas.map(|gas| gas.as_u64()),
			// 			parent_beacon_block_root: self.parent_beacon_block_root.map(|root| root.0.into()),
			// 			requests_hash: self.requests_hash.map(|hash| hash.0.into()),

			// 			difficulty: Default::default(),
			// =======
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

#[cfg(test)]
mod test {
	use super::*;
	use crate::evm::{Block, ReceiptInfo};

	fn manual_trie_root_compute(encoded: Vec<Vec<u8>>) -> H256 {
		use alloy_consensus::private::alloy_trie::{HashBuilder, Nibbles};

		// Helper function to RLP-encode a transaction index
		fn rlp_encode_index(index: u64) -> Vec<u8> {
			let mut buffer = Vec::new();
			alloy_consensus::private::alloy_rlp::Encodable::encode(&index, &mut buffer);
			buffer
		}

		let mut hash_builder = HashBuilder::default();

		let mut pairs: Vec<_> = encoded
			.into_iter()
			.enumerate()
			.map(|(index, tx)| {
				let key = rlp_encode_index(index as u64);
				let mut nibbles = Nibbles::new();
				for byte in key.iter() {
					// Split each byte into two nibbles (high and low 4 bits)
					let high_nibble = (byte >> 4) & 0x0F;
					let low_nibble = byte & 0x0F;
					nibbles.push(high_nibble);
					nibbles.push(low_nibble);
				}

				(nibbles, tx)
			})
			.collect();
		// Sort by nibble key.
		// This gives the right lexicographical order to add leaves to the trie.
		pairs.sort_by(|x, y| x.0.cmp(&y.0));

		for (key, tx) in pairs {
			hash_builder.add_leaf(key, &tx);
		}

		let tx_root_hash = hash_builder.root();
		tx_root_hash.0.into()
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

				use crate::TransactionSigned;
				let transaction_encoded = tx_info.transaction_signed.signed_payload();

				pub fn decode(data: &[u8]) -> Result<TransactionSigned, rlp::DecoderError> {
					use crate::evm::*;

					if data.len() < 1 {
						return Err(rlp::DecoderError::RlpIsTooShort);
					}
					match data[0] {
						TYPE_EIP2930 => {
							println!(" TYPE_EIP2930");
							rlp::decode::<Transaction2930Signed>(&data[1..]).map(Into::into)
						},
						TYPE_EIP1559 => {
							println!(" TYPE_EIP1559");

							rlp::decode::<Transaction1559Signed>(&data[1..]).map(Into::into)
						},
						TYPE_EIP4844 => {
							println!(" TYPE_EIP4844");
							rlp::decode::<Transaction4844Signed>(&data[1..]).map(Into::into)
						},
						TYPE_EIP7702 => {
							println!(" TYPE_EIP7702");
							rlp::decode::<Transaction7702Signed>(&data[1..]).map(Into::into)
						},
						_ => {
							println!(" LEGACY");
							rlp::decode::<TransactionLegacySigned>(data).map(Into::into)
						},
					}
				}

				let generic_transaction = decode(&transaction_encoded).unwrap();
				println!(" Can DECODE!\n");

				TransactionDetails {
					transaction_encoded: tx_info.transaction_signed.signed_payload(),
					logs: receipt_info
						.logs
						.into_iter()
						.map(|log| EventLog {
							contract: log.address.into(),
							data: log.data.unwrap_or_default().0,
							topics: log.topics,
						})
						.collect(),
					success: receipt_info.status.unwrap_or_default() == 1.into(),
					gas_used: receipt_info.gas_used.as_u64().into(),
				}
			})
			.collect();

		// The block hash would differ here because we don't take into account
		// the ommers and other fields from the substrate perspective.
		// However, the state roots must be identical.
		let built_block = Block::build(
			transaction_details,
			block.number.into(),
			block.parent_hash.into(),
			block.timestamp.into(),
			block.miner.into(),
			Default::default(),
		)
		.1;

		assert_eq!(built_block.gas_used, block.gas_used);
		assert_eq!(built_block.logs_bloom, block.logs_bloom);
		// We are using the tx root for state root.
		assert_eq!(built_block.state_root, built_block.transactions_root);

		// Double check the receipts roots.
		assert_eq!(built_block.receipts_root, block.receipts_root);

		let manual_hash = manual_trie_root_compute(encoded_tx.clone());
		println!("Manual Hash: {:?}", manual_hash);
		println!("Built block Hash: {:?}", built_block.transactions_root);
		println!("Real Block Tx Hash: {:?}", block.transactions_root);

		// This double checks the compute logic.
		assert_eq!(manual_hash, block.transactions_root);
		// This ensures we can compute the same transaction root as Ethereum.
		assert_eq!(block.transactions_root, built_block.transactions_root);
	}
}
