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
use super::{ReceiptInfo, TransactionInfo, TransactionSigned};

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
