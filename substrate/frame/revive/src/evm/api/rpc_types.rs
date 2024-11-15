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
//! Utility impl for the RPC types.
use super::{GenericTransaction, ReceiptInfo, TransactionInfo, TransactionSigned};
use sp_core::{H160, U256};

impl TransactionInfo {
	/// Create a new [`TransactionInfo`] from a receipt and a signed transaction.
	pub fn new(receipt: ReceiptInfo, transaction_signed: TransactionSigned) -> Self {
		Self {
			block_hash: receipt.block_hash,
			block_number: receipt.block_number,
			from: receipt.from,
			hash: receipt.transaction_hash,
			transaction_index: receipt.transaction_index,
			transaction_signed,
		}
	}
}

impl ReceiptInfo {
	/// Returns `true` if the transaction was successful.
	pub fn is_success(&self) -> bool {
		self.status.map_or(false, |status| status == U256::one())
	}
}

macro_rules! impl_into_generic_transaction {
    ($tx: ident, $from: ident, { $($field:ident: $mapping:expr),* }) => {
			GenericTransaction {
				from: $from,
				input: Some($tx.input),
				nonce: Some($tx.nonce),
				r#type: Some($tx.r#type.as_byte()),
				value: Some($tx.value),
				$($field: $mapping,)*
				..Default::default()
			}
    }
}

impl GenericTransaction {
	/// Create a new [`GenericTransaction`] from a signed transaction.
	pub fn from_signed(tx: TransactionSigned, from: Option<H160>) -> Self {
		use TransactionSigned::*;
		match tx {
			TransactionLegacySigned(tx) => {
				let tx = tx.transaction_legacy_unsigned;
				impl_into_generic_transaction!(tx, from, {
					chain_id: tx.chain_id,
					gas: Some(tx.gas),
					gas_price: Some(tx.gas_price),
					to: tx.to
				})
			},
			Transaction4844Signed(tx) => {
				let tx = tx.transaction_4844_unsigned;
				impl_into_generic_transaction!(tx, from, {
					access_list: Some(tx.access_list),
					blob_versioned_hashes: Some(tx.blob_versioned_hashes),
					max_fee_per_blob_gas: Some(tx.max_fee_per_blob_gas),
					max_fee_per_gas: Some(tx.max_fee_per_gas),
					max_priority_fee_per_gas: Some(tx.max_priority_fee_per_gas),
					chain_id: Some(tx.chain_id),
					gas: Some(tx.gas),
					gas_price: Some(tx.max_fee_per_blob_gas),
					to: Some(tx.to)
				})
			},
			Transaction1559Signed(tx) => {
				let tx = tx.transaction_1559_unsigned;
				impl_into_generic_transaction!(tx, from, {
					access_list: Some(tx.access_list),
					max_fee_per_gas: Some(tx.max_fee_per_gas),
					max_priority_fee_per_gas: Some(tx.max_priority_fee_per_gas),
					chain_id: Some(tx.chain_id),
					gas: Some(tx.gas),
					gas_price: Some(tx.gas_price),
					to: tx.to
				})
			},
			Transaction2930Signed(tx) => {
				let tx = tx.transaction_2930_unsigned;
				impl_into_generic_transaction!(tx, from, {
					access_list: Some(tx.access_list),
					chain_id: Some(tx.chain_id),
					gas: Some(tx.gas),
					gas_price: Some(tx.gas_price),
					to: tx.to
				})
			},
		}
	}
}
