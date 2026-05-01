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

use codec::{Decode, Encode};
use pallet_revive_proc_macro::define_versioned_interface;
use scale_info::TypeInfo;

use crate::runtime_api::{BlockV1, TraceV1, TracerTypeV1};

define_versioned_interface! {
	/// Version 1 of the input payload that asks the runtime to trace a single transaction inside a
	/// block, identified by its index in the block's transaction list. The block is supplied in
	/// Ethereum-format wire shape so the runtime can replay it deterministically.
	#[derive(
		Debug,
		Clone,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
	)]
	pub struct TraceTxVersionedInputPayloadV1 {
		/// Block containing the transaction to trace.
		pub block: BlockV1,
		/// Index of the transaction within the block's transaction list.
		pub tx_index: u32,
		/// Tracer configuration controlling which kind of trace is produced.
		pub config: TracerTypeV1
	}

	/// Version 1 of the output payload returned for a [`TraceTxVersionedInputPayloadV1`] request,
	/// carrying the trace if the transaction at the requested index exists.
	#[derive(
		Debug,
		Clone,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
	)]
	pub struct TraceTxVersionedOutputPayloadV1 {
		/// Trace produced by replaying the transaction, or `None` if the requested transaction
		/// index does not exist in the supplied block.
		pub trace: Option<TraceV1>
	}
}
