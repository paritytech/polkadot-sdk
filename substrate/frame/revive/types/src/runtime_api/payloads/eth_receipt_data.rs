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

use alloc::vec::Vec;
use codec::{Decode, Encode, MaxEncodedLen};
use pallet_revive_proc_macro::define_versioned_interface;
use scale_info::TypeInfo;

use crate::runtime_api::ReceiptGasInfoV1;

define_versioned_interface! {
	/// Version 1 of the input payload that asks the runtime for the per-transaction gas data needed
	/// to assemble Ethereum-style receipts for the current block. Off-chain clients combine this
	/// data with their own transaction list to build receipts that match what an Ethereum execution
	/// client would emit.
	#[derive(
		Debug,
		Clone,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		MaxEncodedLen,
	)]
	pub struct EthReceiptDataVersionedInputPayloadV1 {}

	/// Version 1 of the output payload returned for an [`EthReceiptDataVersionedInputPayloadV1`]
	/// request, carrying the gas data for every transaction in the current block.
	#[derive(
		Debug,
		Clone,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
	)]
	pub struct EthReceiptDataVersionedOutputPayloadV1 {
		/// One [`ReceiptGasInfoV1`] entry for each transaction in the current block, in
		/// transaction-index order.
		pub receipt_data: Vec<ReceiptGasInfoV1>
	}
}
