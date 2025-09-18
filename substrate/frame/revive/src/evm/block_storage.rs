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
	evm::block_hash::{AccumulateReceipt, LogsBloom},
	H160, H256,
};
use alloc::vec::Vec;
use environmental::environmental;

/// The maximum number of block hashes to keep in the history.
pub const BLOCK_HASH_COUNT: u32 = 256;

// The events emitted by this pallet while executing the current inflight transaction.
//
// The events are needed to reconstruct the receipt root hash, as they represent the
// logs emitted by the contract. The events are consumed when the transaction is
// completed. To minimize the amount of used memory, the events are RLP encoded directly.
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
