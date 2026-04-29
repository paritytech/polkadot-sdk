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
use codec::{Decode, Encode};
use pallet_revive_proc_macro::define_versioned_interface;
use scale_info::TypeInfo;

use crate::runtime_api::{BlockV1, IndexedTraceV1, TracerTypeV1};

define_versioned_interface! {
	/// Version 1 of the input payload that asks the runtime to trace every transaction in a block
	/// under a chosen tracer configuration. The block is supplied as its Ethereum-format wire shape
	/// so that the runtime can replay each transaction inside it under the tracer.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct TraceBlockVersionedInputPayloadV1 {
		/// Block whose transactions should be traced.
		pub block: BlockV1,
		/// Tracer configuration controlling which kind of trace is produced for each transaction in
		/// the block.
		pub config: TracerTypeV1
	}

	/// Version 1 of the output payload returned for a [`TraceBlockVersionedInputPayloadV1`]
	/// request, carrying one trace per transaction in the block.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct TraceBlockVersionedOutputPayloadV1 {
		/// Traces produced by replaying the block, paired with the transaction index they
		/// correspond to.
		pub traces: Vec<IndexedTraceV1>
	}
}
