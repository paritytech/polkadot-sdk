// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use alloc::vec::Vec;
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::U256;

#[derive(Encode, Decode, TypeInfo, Clone, Debug, Default, PartialEq, Eq)]
pub struct ReceiptGasInfoV1 {
	pub gas_used: U256,
	pub effective_gas_price: U256,
}

/// What a block committed to its synthetic transaction, the one carrying the logs emitted outside
/// any ethereum transaction.
#[derive(TypeInfo, Debug, Clone, Encode, Decode, PartialEq)]
pub struct SyntheticTransactionV1 {
	/// Its receipt gas entry.
	pub gas_info: ReceiptGasInfoV1,
	/// The `frame_system` event index of each log that went into it, in receipt order.
	///
	/// These are the logs the block's `logs_bloom` and `receipts_root` commit to. A consumer that
	/// rebuilds them from block events must select by these indices: the block's `ContractEmitted`
	/// events hold more than the header accounts for, since a contract log outside an ethereum
	/// transaction is deposited but not buffered, and a runtime bounds the buffer and deposits the
	/// event whether or not the log fitted.
	pub log_event_indices: Vec<u32>,
}
