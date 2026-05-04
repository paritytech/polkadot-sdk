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

//! Types, and traits to integrate pallet-revive with EVM.
#![warn(missing_docs)]

mod receipt;
pub use receipt::{AccumulateReceipt, LogsBloom};

mod hash_builder;
pub use hash_builder::{BuilderPhase, IncrementalHashBuilder, IncrementalHashBuilderIR};

mod block_builder;
pub use block_builder::{EthereumBlockBuilder, EthereumBlockBuilderIR};

use crate::evm::Block;

use alloc::vec::Vec;
use alloy_core::primitives::{B256, bytes::BufMut};

use codec::{Decode, Encode};
use pallet_revive_types::storage as storage_types;
use scale_info::TypeInfo;
use sp_core::{H256, U256};

/// Details needed to reconstruct the receipt info in the RPC
/// layer without losing accuracy.
#[derive(Encode, Decode, TypeInfo, Clone, Debug, Default, PartialEq, Eq)]
pub struct ReceiptGasInfo {
	/// The amount of gas used for this specific transaction alone.
	pub gas_used: U256,

	/// The effective gas price for this transaction.
	pub effective_gas_price: U256,
}

impl From<ReceiptGasInfo> for storage_types::ReceiptGasInfoV1 {
	fn from(value: ReceiptGasInfo) -> Self {
		Self { gas_used: value.gas_used, effective_gas_price: value.effective_gas_price }
	}
}

impl From<storage_types::ReceiptGasInfoV1> for ReceiptGasInfo {
	fn from(value: storage_types::ReceiptGasInfoV1) -> Self {
		Self { gas_used: value.gas_used, effective_gas_price: value.effective_gas_price }
	}
}

/// Execution-facing receipt-gas list stored by `ReceiptInfoData`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ReceiptInfoDataValue {
	/// Gas metadata for each receipt in the stored Ethereum block.
	receipt_data: Vec<ReceiptGasInfo>,
}

impl From<Vec<ReceiptGasInfo>> for ReceiptInfoDataValue {
	fn from(receipt_data: Vec<ReceiptGasInfo>) -> Self {
		Self { receipt_data }
	}
}

impl From<ReceiptInfoDataValue> for Vec<ReceiptGasInfo> {
	fn from(value: ReceiptInfoDataValue) -> Self {
		value.receipt_data
	}
}

impl From<ReceiptInfoDataValue> for storage_types::ReceiptInfoDataV1 {
	fn from(value: ReceiptInfoDataValue) -> Self {
		Self(value.receipt_data.into_iter().map(Into::into).collect())
	}
}

impl From<storage_types::ReceiptInfoDataV1> for ReceiptInfoDataValue {
	fn from(value: storage_types::ReceiptInfoDataV1) -> Self {
		Self { receipt_data: value.0.into_iter().map(Into::into).collect() }
	}
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
