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

use crate::{
	evm::block_hash::{AccumulateReceipt, EthereumBlockBuilder, LogsBloom},
	sp_runtime::traits::One,
	BlockHash, Config, EthBlockBuilderIR, EthereumBlock, ReceiptInfoData, UniqueSaturatedInto,
	H160, H256,
};

use frame_support::weights::Weight;
pub use sp_core::U256;

use alloc::vec::Vec;
use environmental::environmental;

/// The maximum number of block hashes to keep in the history.
///
/// Note: This might be made configurable in the future.
pub const BLOCK_HASH_COUNT: u32 = 256;

// Accumulates the receipt's events (logs) for the current transaction
// that are needed to construct the final transaction receipt.
environmental!(receipt: AccumulateReceipt);

/// Capture the Ethereum log for the current transaction.
///
/// This method does nothing if called from outside of the ethereum context.
pub fn capture_ethereum_log(contract: &H160, data: &[u8], topics: &[H256]) {
	receipt::with(|receipt| {
		receipt.add_log(contract, data, topics);
	});
}

/// Get the receipt details of the current transaction.
///
/// This method returns `None` if and only if the function is called
/// from outside of the ethereum context.
pub fn get_receipt_details() -> Option<(Vec<u8>, LogsBloom)> {
	receipt::with(|receipt| {
		let encoding = core::mem::take(&mut receipt.encoding);
		let bloom = core::mem::take(&mut receipt.bloom);
		(encoding, bloom)
	})
}

/// Capture the receipt events emitted from the current ethereum
/// transaction. The transaction must be signed by an eth-compatible
/// wallet.
pub fn with_ethereum_context<R>(f: impl FnOnce() -> R) -> R {
	receipt::using(&mut AccumulateReceipt::new(), f)
}

/// Clear the storage used to capture the block hash related data.
pub fn on_initialize<T: Config>() {
	ReceiptInfoData::<T>::kill();
	EthereumBlock::<T>::kill();
}

/// Build the ethereum block and store it into the pallet storage.
pub fn on_finalize_build_eth_block<T: Config>(
	block_author: H160,
	eth_block_num: U256,
	gas_limit: U256,
	timestamp: U256,
) {
	let parent_hash = if eth_block_num > U256::zero() {
		BlockHash::<T>::get(eth_block_num - 1)
	} else {
		H256::default()
	};

	let block_builder_ir = EthBlockBuilderIR::<T>::get();
	EthBlockBuilderIR::<T>::kill();

	// Load the first values if not already loaded.
	let (block, receipt_data) = EthereumBlockBuilder::<T>::from_ir(block_builder_ir).build(
		eth_block_num,
		parent_hash,
		timestamp,
		block_author,
		gas_limit,
	);

	// Put the block hash into storage.
	BlockHash::<T>::insert(eth_block_num, block.hash);

	// Prune older block hashes.
	let block_hash_count = BLOCK_HASH_COUNT;
	let to_remove =
		eth_block_num.saturating_sub(block_hash_count.into()).saturating_sub(One::one());
	if !to_remove.is_zero() {
		<BlockHash<T>>::remove(U256::from(UniqueSaturatedInto::<u32>::unique_saturated_into(
			to_remove,
		)));
	}
	// Store the ETH block into the last block.
	EthereumBlock::<T>::put(block);
	// Store the receipt info data for offchain reconstruction.
	ReceiptInfoData::<T>::put(receipt_data);
}

/// Process a transaction payload with extra details.
/// This stores the RLP encoded transaction and receipt details into storage.
///
/// The data is used during the `on_finalize` hook to reconstruct the ETH block.
pub fn process_transaction<T: Config>(
	transaction_encoded: Vec<u8>,
	success: bool,
	gas_used: Weight,
) {
	// Method returns `None` only when called from outside of the ethereum context.
	// This is not the case here, since this is called from within the
	// ethereum context.
	let (encoded_logs, bloom) = get_receipt_details().unwrap_or_default();

	let block_builder_ir = EthBlockBuilderIR::<T>::get();
	let mut block_builder = EthereumBlockBuilder::<T>::from_ir(block_builder_ir);

	block_builder.process_transaction(transaction_encoded, success, gas_used, encoded_logs, bloom);

	EthBlockBuilderIR::<T>::put(block_builder.to_ir());
}
