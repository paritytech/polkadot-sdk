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
use pallet_revive_types::runtime_api::*;

use crate::{ReceiptGasInfo, evm::block_hash::SyntheticTransactionInfo};

pub struct ReceiptDataInputPayload;

impl From<ReceiptDataVersionedInputPayload> for ReceiptDataInputPayload {
	fn from(value: ReceiptDataVersionedInputPayload) -> Self {
		match value {
			ReceiptDataVersionedInputPayload::V1(payload) => payload.into(),
			ReceiptDataVersionedInputPayload::V2(payload) => payload.into(),
		}
	}
}

impl From<ReceiptDataInputPayloadV1> for ReceiptDataInputPayload {
	fn from(_value: ReceiptDataInputPayloadV1) -> Self {
		Self
	}
}

impl From<ReceiptDataInputPayloadV2> for ReceiptDataInputPayload {
	fn from(_value: ReceiptDataInputPayloadV2) -> Self {
		Self
	}
}

pub struct ReceiptDataOutputPayload {
	pub receipt_data: Vec<ReceiptGasInfo>,
	pub synthetic: Option<SyntheticTransactionInfo>,
}

impl From<ReceiptDataOutputPayload> for ReceiptDataOutputPayloadV1 {
	/// Drops `synthetic`: V1 promises one entry per ethereum transaction, and the deployed
	/// consumers of that version reject a longer vec outright.
	fn from(value: ReceiptDataOutputPayload) -> Self {
		Self { receipt_data: value.receipt_data.into_iter().map(Into::into).collect() }
	}
}

impl From<ReceiptDataOutputPayload> for ReceiptDataOutputPayloadV2 {
	fn from(value: ReceiptDataOutputPayload) -> Self {
		Self {
			receipt_data: value.receipt_data.into_iter().map(Into::into).collect(),
			synthetic: value.synthetic.map(Into::into),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn gas_info(gas_used: u64) -> ReceiptGasInfo {
		ReceiptGasInfo { gas_used: gas_used.into(), effective_gas_price: 1u64.into() }
	}

	fn output() -> ReceiptDataOutputPayload {
		ReceiptDataOutputPayload {
			receipt_data: vec![gas_info(1)],
			synthetic: Some(SyntheticTransactionInfo { gas_info: gas_info(2), log_count: 3 }),
		}
	}

	#[test]
	fn v1_drops_the_synthetic_transaction() {
		// V1's contract is one entry per ethereum transaction, and the consumers of that version
		// predate the synthetic transaction: a longer vec is what they reject.
		let v1 = ReceiptDataOutputPayloadV1::from(output());

		assert_eq!(v1.receipt_data.len(), 1);
		assert_eq!(v1.receipt_data[0].gas_used, 1u64.into());
	}

	#[test]
	fn v2_reports_the_synthetic_transaction_apart() {
		let v2 = ReceiptDataOutputPayloadV2::from(output());

		assert_eq!(v2.receipt_data.len(), 1, "still one entry per ethereum transaction");
		let synthetic = v2.synthetic.expect("reported separately");
		assert_eq!(synthetic.gas_info.gas_used, 2u64.into());
		assert_eq!(synthetic.log_count, 3, "the committed log count reaches the consumer");
	}
}
