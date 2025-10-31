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
	limits,
	sp_runtime::traits::One,
	weights::WeightInfo,
	BlockHash, Config, EthBlockBuilderIR, EthereumBlock, Event, Pallet, ReceiptInfoData,
	UniqueSaturatedInto, H160, H256,
};
use alloc::vec::Vec;
use environmental::environmental;
use frame_support::{
	pallet_prelude::{DispatchError, DispatchResultWithPostInfo},
	storage::with_transaction,
	weights::Weight,
};
use sp_core::U256;
use sp_runtime::TransactionOutcome;

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
#[cfg(feature = "runtime-benchmarks")]
pub fn bench_with_ethereum_context<R>(f: impl FnOnce() -> R) -> R {
	receipt::using(&mut AccumulateReceipt::new(), f)
}

/// Execute the Ethereum call, and write the block storage transaction details.
///
/// # Parameters
/// - transaction_encoded: The RLP encoded transaction bytes.
/// - call: A closure that executes the transaction logic and returns the gas consumed and result.
pub fn with_ethereum_context<T: Config>(
	transaction_encoded: Vec<u8>,
	call: impl FnOnce() -> (Weight, DispatchResultWithPostInfo),
) -> DispatchResultWithPostInfo {
	receipt::using(&mut AccumulateReceipt::new(), || {
		let (err, gas_consumed, mut post_info) =
			with_transaction(|| -> TransactionOutcome<Result<_, DispatchError>> {
				let (gas_consumed, result) = call();
				match result {
					Ok(post_info) =>
						TransactionOutcome::Commit(Ok((None, gas_consumed, post_info))),
					Err(err) => TransactionOutcome::Rollback(Ok((
						Some(err.error),
						gas_consumed,
						err.post_info,
					))),
				}
			})?;

		if let Some(dispatch_error) = err {
			deposit_eth_extrinsic_revert_event::<T>(dispatch_error);
			crate::block_storage::process_transaction::<T>(
				transaction_encoded,
				false,
				gas_consumed,
			);
			Ok(post_info)
		} else {
			// deposit a dummy event in benchmark mode
			#[cfg(feature = "runtime-benchmarks")]
			deposit_eth_extrinsic_revert_event::<T>(crate::Error::<T>::BenchmarkingError.into());

			crate::block_storage::process_transaction::<T>(transaction_encoded, true, gas_consumed);
			post_info
				.actual_weight
				.as_mut()
				.map(|w| w.saturating_reduce(T::WeightInfo::deposit_eth_extrinsic_revert_event()));
			Ok(post_info)
		}
	})
}

fn deposit_eth_extrinsic_revert_event<T: Config>(dispatch_error: DispatchError) {
	Pallet::<T>::deposit_event(Event::<T>::EthExtrinsicRevert { dispatch_error });
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
	eth_block_base_fee: U256,
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
		eth_block_base_fee,
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

// The `EthereumBlockBuilder` builds the Ethereum-compatible block by maintaining
// two incremental hash builders. Each builder accumulates entries until the trie
// is finalized:
//  1. `transactions_root` - builds the Merkle root of transaction payloads
//  2. `receipts_root` - builds the Merkle root of transaction receipts (event logs)
//
// The `EthereumBlockBuilder` is serialized and deserialized to and from storage
// on every transaction via the `EthereumBlockBuilderIR` object. This is needed until
// the runtime exposes a better API to preserve the state between transactions (ie,
// the global `environment!` is wiped because each transaction will instantiate a new
// WASM instance).
//
// For this reason, we need to account for the memory used by the `EthereumBlockBuilder`
// and for the pallet storage consumed by the `EthereumBlockBuilderIR`.
//
// ## Memory Usage Analysis
//
// The incremental hash builder accumulates entries until the trie is finalized.
// The last added entry value is kept in memory until it can be hashed.
// The keys are always ordered and the hashing happens when the next entry is added to
// the trie. The common prefix of the current and previous keys forms the path into the
// trie, and together with the value of the previous entry, a hash of 32 bytes is
// computed.
//
// For this reason, the memory usage of the incremental hash builder is no greater
// than two entries of maximum size, plus some marginal book-keeping overhead
// (ignored to simplify calculations).
//
// `IncrementalHashBuilder = 2 * maximum size of the entry`
//
// Additionally, the block builder caches the first entry for each incremental hash.
// The entry is loaded from storage into RAM when either:
// - The block is finalized, OR
// - After 127 transactions.
// Therefore, an additional entry of maximum size is needed in memory.
//
// That gives us 3 items of maximum size per each hash builder.
//
// `EthereumBlockBuilder = 3 * (max size of transactions + max size of receipts)`
// The maximum size of a transaction is limited by
// `limits::MAX_TRANSACTION_PAYLOAD_SIZE`, while the maximum size of a receipt is
// limited by `limits::EVENT_BYTES`.
//
// Similarly, this is the amount of pallet storage consumed by the
// `EthereumBlockBuilderIR` object, plus a marginal book-keeping overhead.
pub fn block_builder_bytes_usage(max_events_size: u32) -> u32 {
	// A block builder requires 3 times the maximum size of the entry.
	const MEMORY_COEFFICIENT: u32 = 3;

	// Because events are not capped, and the builder cannot exceed the
	// number of bytes received, the actual memory usage for receipts is:
	// `receipts_hash_builder = min(events_per_tx * 3, max_events_size)`
	// where `max_events_size` can be consumed by a single transaction.
	// Since we don't know in advance the `events_per_tx`, we'll assume the
	// worst case scenario.
	let receipts_hash_builder = max_events_size;

	// `transactions_root` hash builder
	let transactions_hash_builder =
		limits::MAX_TRANSACTION_PAYLOAD_SIZE.saturating_mul(MEMORY_COEFFICIENT);

	receipts_hash_builder.saturating_add(transactions_hash_builder)
}
