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
	evm::{Block, TransactionSigned},
	Event,
};

use alloc::vec::Vec;
use alloy_consensus::RlpEncodableReceipt;
use alloy_core::primitives::bytes::BufMut;
use codec::{Decode, Encode};
use frame_support::weights::Weight;
use scale_info::TypeInfo;
use sp_core::{keccak_256, H160, H256, U256};

const LOG_TARGET: &str = "runtime::revive::hash";

/// The transaction details captured by the revive pallet.
pub type TransactionDetails<T> = (Vec<u8>, u32, Vec<Event<T>>, bool, Weight);

/// Details needed to reconstruct the receipt info in the RPC
/// layer without losing accuracy.
#[derive(Encode, Decode, TypeInfo, Clone)]
pub struct ReceiptGasInfo {
	/// The actual value per gas deducted from the sender's account. Before EIP-1559, this
	/// is equal to the transaction's gas price. After, it is equal to baseFeePerGas +
	/// min(maxFeePerGas - baseFeePerGas, maxPriorityFeePerGas).
	///
	/// Note: Since there's a runtime API to extract the base gas fee (`fn gas_price()`)
	/// and we have access to the `TransactionSigned` struct, we can compute the effective gas
	/// price in the RPC layer.
	pub effective_gas_price: U256,

	/// The amount of gas used for this specific transaction alone.
	pub gas_used: U256,
}

impl Block {
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
		details: impl IntoIterator<Item = TransactionDetails<T>>,
		block_number: U256,
		parent_hash: H256,
		timestamp: U256,
		block_author: H160,
		gas_limit: U256,
		base_gas_price: U256,
	) -> (H256, Block, Vec<ReceiptGasInfo>)
	where
		T: crate::pallet::Config,
	{
		let mut block = Self {
			number: block_number,
			parent_hash,
			timestamp,
			miner: block_author,
			gas_limit,

			// The remaining fields are populated by `process_transaction_details`.
			..Default::default()
		};

		let transaction_details: Vec<_> = details
			.into_iter()
			.filter_map(|detail| block.process_transaction_details(detail, base_gas_price))
			.collect();

		let mut signed_tx = Vec::with_capacity(transaction_details.len());
		let mut receipts = Vec::with_capacity(transaction_details.len());
		let mut gas_infos = Vec::with_capacity(transaction_details.len());
		for (signed, receipt, gas_info) in transaction_details {
			signed_tx.push(signed);
			receipts.push(receipt);
			gas_infos.push(gas_info);
		}

		// Compute expensive trie roots.
		let transactions_root = Self::compute_trie_root(&signed_tx);
		let receipts_root = Self::compute_trie_root(&receipts);

		block.state_root = transactions_root.0.into();
		block.transactions_root = transactions_root.0.into();
		block.receipts_root = receipts_root.0.into();

		// Compute the ETH header hash.
		let block_hash = block.header_hash();

		(block_hash, block, gas_infos)
	}

	/// Returns a tuple of the RLP encoded transaction and receipt.
	///
	/// Internally collects the total gas used, the log blooms and the transaction hashes.
	fn process_transaction_details<T>(
		&mut self,
		detail: TransactionDetails<T>,
		base_gas_price: U256,
	) -> Option<(Vec<u8>, Vec<u8>, ReceiptGasInfo)>
	where
		T: crate::pallet::Config,
	{
		let (payload, transaction_index, events, success, gas) = detail;
		let signed_tx = TransactionSigned::decode(&mut &payload[..]).inspect_err(|err| {
            log::error!(target: LOG_TARGET, "Failed to decode transaction at index {transaction_index}: {err:?}");
        }).ok()?;

		let transaction_hash = H256(keccak_256(&payload));
		self.transactions.push_hash(transaction_hash);

		let gas_info = ReceiptGasInfo {
			effective_gas_price: signed_tx.effective_gas_price(base_gas_price),
			gas_used: gas.ref_time().into(),
		};

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

		self.gas_used += gas.ref_time().into();

		let receipt = alloy_consensus::Receipt {
			status: success.into(),
			cumulative_gas_used: self.gas_used.as_u64(),
			logs,
		};

		let receipt_bloom = receipt.bloom_slow();
		self.logs_bloom.combine(&(*receipt_bloom.0).into());

		let mut encoded_receipt =
			Vec::with_capacity(receipt.rlp_encoded_length_with_bloom(&receipt_bloom));
		receipt.rlp_encode_with_bloom(&receipt_bloom, &mut encoded_receipt);

		Some((signed_tx.signed_payload(), encoded_receipt, gas_info))
	}

	/// Compute the trie root using the `(rlp(index), encoded(item))` pairs.
	pub fn compute_trie_root(items: &[Vec<u8>]) -> alloy_primitives::B256 {
		alloy_consensus::proofs::ordered_trie_root_with_encoder(items, |item, buf| {
			buf.put_slice(item)
		})
	}

	/// Compute the ETH header hash.
	fn header_hash(&self) -> H256 {
		// Note: Cap the gas limit to u64::MAX.
		// In practice, it should be impossible to fill a u64::MAX gas limit
		// of an either Ethereum or Substrate block.
		let gas_limit =
			if self.gas_limit > u64::MAX.into() { u64::MAX } else { self.gas_limit.as_u64() };

		let alloy_header = alloy_consensus::Header {
			state_root: self.transactions_root.0.into(),
			transactions_root: self.transactions_root.0.into(),
			receipts_root: self.receipts_root.0.into(),

			parent_hash: self.parent_hash.0.into(),
			beneficiary: self.miner.0.into(),
			number: self.number.as_u64(),
			logs_bloom: self.logs_bloom.0.into(),
			gas_limit,
			gas_used: self.gas_used.as_u64(),
			timestamp: self.timestamp.as_u64(),

			..alloy_consensus::Header::default()
		};

		alloy_header.hash_slow().0.into()
	}
}
