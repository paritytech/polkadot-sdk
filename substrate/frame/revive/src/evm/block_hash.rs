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

use crate::{
	evm::{Block as EthBlock, Bytes256, TransactionSigned},
	Event, HashesOrTransactionInfos, LOG_TARGET,
};

use alloc::vec::Vec;
use alloy_consensus::RlpEncodableReceipt;
use alloy_core::primitives::bytes::BufMut;
use codec::{Decode, Encode};
use frame_support::weights::Weight;
use scale_info::TypeInfo;
use sp_core::{keccak_256, H160, H256, U256};

/// The transaction details captured by the revive pallet.
pub type TransactionDetails<T> = (Vec<u8>, u32, Vec<Event<T>>, bool, Weight);

/// Details needed to reconstruct the receipt info in the RPC
/// layer without losing accuracy.
#[derive(Encode, Decode, TypeInfo)]
pub struct ReconstructReceiptInfo {
	/// The actual value per gas deducted from the sender's account. Before EIP-1559, this
	/// is equal to the transaction's gas price. After, it is equal to baseFeePerGas +
	/// min(maxFeePerGas - baseFeePerGas, maxPriorityFeePerGas).
	///
	/// Note: Since there's a runtime API to extract the base gas fee (`fn gas_price()`)
	/// and we have access to the `TransactionSigned` struct, we can compute the effective gas
	/// price in the RPC layer.
	effective_gas_price: U256,

	/// The amount of gas used for this specific transaction alone.
	gas_used: U256,
}

/// Builder of the ETH block.
pub struct EthBlockBuilder {
	/// Current block number.
	block_number: U256,
	/// Parent block hash.
	parent_hash: H256,
	/// The base gas price of the block.
	base_gas_price: U256,
	/// The timestamp of the block.
	timestamp: U256,
	/// The author of the block.
	block_author: H160,
	/// The gas limit of the block.
	gas_limit: U256,

	/// Logs bloom of the receipts.
	logs_bloom: Bytes256,
	/// Total gas used by transactions in the block.
	total_gas_used: U256,
	/// The transaction hashes that will be placed in the ETH block.
	tx_hashes: Vec<H256>,
	/// The data needed to reconstruct the receipt info.
	receipt_data: Vec<ReconstructReceiptInfo>,
}

impl EthBlockBuilder {
	/// Constructs a new [`EthBlockBuilder`].
	///
	/// # Note
	///
	/// Obtaining some of the fields from the pallet's storage must be accounted.
	pub fn new(
		block_number: U256,
		parent_hash: H256,
		base_gas_price: U256,
		timestamp: U256,
		block_author: H160,
		gas_limit: U256,
	) -> Self {
		Self {
			block_number,
			parent_hash,
			base_gas_price,
			timestamp,
			block_author,
			gas_limit,
			// The following fields are populated by `process_transaction_details`.
			tx_hashes: Vec::new(),
			total_gas_used: U256::zero(),
			logs_bloom: Bytes256::default(),
			receipt_data: Vec::new(),
		}
	}

	/// Build the Ethereum block.
	///
	/// # Note
	///
	/// This is an expensive operation.
	///
	/// (I) For each transaction captured (with the unbounded number of events):
	/// - transaction is RLP decoded into a `TransactionSigned`
	/// - transaction hash is computed using `keccak256`
	/// - transaction is 2718 RLP encoded
	/// - the receipt is constructed and contains all the logs emitted by the transaction
	///   - This includes computing the bloom filter for the logs (O(N) to compute)
	///   - The receipt is 2718 RLP encoded: the cost is O(N) to encode due to the number of logs.
	///
	/// (II) Transaction trie root and receipt trie root are computed.
	///
	/// (III) Block hash is computed from the provided information.
	pub fn build<T>(
		mut self,
		details: impl IntoIterator<Item = TransactionDetails<T>>,
	) -> (H256, EthBlock, Vec<ReconstructReceiptInfo>)
	where
		T: crate::pallet::Config,
	{
		let (signed_tx, receipt): (Vec<_>, Vec<_>) = details
			.into_iter()
			.filter_map(|detail| self.process_transaction_details(detail))
			.unzip();

		// Compute expensive trie roots.
		let transactions_root = Self::compute_trie_root(&signed_tx);
		let receipts_root = Self::compute_trie_root(&receipt);

		// Compute the ETH header hash.
		let block_hash = self.header_hash(transactions_root, receipts_root);

		let block = EthBlock {
			state_root: transactions_root.0.into(),
			transactions_root: transactions_root.0.into(),
			receipts_root: receipts_root.0.into(),

			parent_hash: self.parent_hash.into(),
			miner: self.block_author.into(),
			logs_bloom: self.logs_bloom,
			total_difficulty: Some(U256::zero()),
			number: self.block_number.into(),
			gas_limit: self.gas_limit,
			gas_used: self.total_gas_used,
			timestamp: self.timestamp,

			transactions: HashesOrTransactionInfos::Hashes(self.tx_hashes),

			..Default::default()
		};

		(block_hash, block, self.receipt_data)
	}

	/// Returns a tuple of the RLP encoded transaction and receipt.
	///
	/// Internally collects the total gas used, the log blooms and the transaction hashes.
	fn process_transaction_details<T>(
		&mut self,
		detail: TransactionDetails<T>,
	) -> Option<(Vec<u8>, Vec<u8>)>
	where
		T: crate::pallet::Config,
	{
		let (payload, transaction_index, events, success, gas) = detail;
		let signed_tx = TransactionSigned::decode(&mut &payload[..]).inspect_err(|err| {
            log::error!(target: LOG_TARGET, "Failed to decode transaction at index {transaction_index}: {err:?}");
        }).ok()?;

		let transaction_hash = H256(keccak_256(&payload));
		self.tx_hashes.push(transaction_hash);

		self.receipt_data.push(ReconstructReceiptInfo {
			effective_gas_price: signed_tx.effective_gas_price(self.base_gas_price),
			gas_used: gas.ref_time().into(),
		});

		let logs = events
			.into_iter()
			.filter_map(|event| {
				if let Event::ContractEmitted { contract, data, topics } = event {
					Some(alloy_primitives::Log::new_unchecked(
						contract.0.into(),
						topics
							.into_iter()
							.map(|h| alloy_primitives::FixedBytes::from(h.0))
							.collect::<Vec<_>>(),
						alloy_primitives::Bytes::from(data),
					))
				} else {
					None
				}
			})
			.collect();

		self.total_gas_used += gas.ref_time().into();

		let receipt = alloy_consensus::Receipt {
			status: success.into(),
			cumulative_gas_used: self.total_gas_used.as_u64(),
			logs,
		};

		let receipt_bloom = receipt.bloom_slow();
		self.logs_bloom.combine(&(*receipt_bloom.0).into());

		let mut encoded_receipt =
			Vec::with_capacity(receipt.rlp_encoded_length_with_bloom(&receipt_bloom));
		receipt.rlp_encode_with_bloom(&receipt_bloom, &mut encoded_receipt);

		Some((signed_tx.encode_2718(), encoded_receipt))
	}

	/// Compute the trie root using the `(rlp(index), encoded(item))` pairs.
	fn compute_trie_root(items: &[Vec<u8>]) -> alloy_primitives::B256 {
		alloy_consensus::proofs::ordered_trie_root_with_encoder(items, |item, buf| {
			buf.put_slice(item)
		})
	}

	/// Compute the ETH header hash.
	fn header_hash(
		&self,
		transactions_root: alloy_primitives::B256,
		receipts_root: alloy_primitives::B256,
	) -> H256 {
		let alloy_header = alloy_consensus::Header {
			state_root: transactions_root,
			transactions_root,
			receipts_root,

			parent_hash: self.parent_hash.0.into(),
			beneficiary: self.block_author.0.into(),
			number: self.block_number.as_u64(),
			logs_bloom: self.logs_bloom.0.into(),
			gas_limit: self.gas_limit.as_u64(),
			gas_used: self.total_gas_used.as_u64(),
			timestamp: self.timestamp.as_u64(),

			..alloy_consensus::Header::default()
		};

		alloy_header.hash_slow().0.into()
	}
}
