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

use super::tracing_common::CallTypeV1;
use crate::common::Bytes;
use alloc::{collections::BTreeMap, string::String, vec::Vec};
use codec::{Decode, Encode};
use ethereum_types::{H160, H256, U256};
use pallet_revive_proc_macro::define_versioned_type;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_weights::Weight;

define_versioned_type! {
	/// Version 1 of an indexed trace returned while tracing a block.
	#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode)]
	#[serde(rename_all = "camelCase")]
	pub struct IndexedTraceV1 {
		/// Index of the traced transaction inside the block.
		pub tx_index: u32,
		/// Trace produced by executing the transaction.
		pub trace: TraceV1,
	}
}

define_versioned_type! {
	/// Version 1 of a trace result.
	#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode)]
	#[serde(untagged)]
	pub enum TraceV1 {
		/// A nested call trace.
		Call(CallTraceV1),
		/// A prestate or diff-mode trace.
		Prestate(PrestateTraceV1),
		/// An opcode and syscall execution trace.
		Execution(ExecutionTraceV1),
	}
}

define_versioned_type! {
	/// Version 1 of a prestate trace.
	#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode)]
	#[serde(untagged)]
	pub enum PrestateTraceV1 {
		/// Accounts required to execute a transaction.
		Prestate(BTreeMap<H160, PrestateTraceInfoV1>),
		/// Account state before and after transaction execution.
		DiffMode {
			/// Account state before execution.
			pre: BTreeMap<H160, PrestateTraceInfoV1>,
			/// Account state after execution.
			post: BTreeMap<H160, PrestateTraceInfoV1>,
		},
	}
}

define_versioned_type! {
	/// Version 1 of account data returned by prestate tracing.
	#[derive(
		Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
	)]
	pub struct PrestateTraceInfoV1 {
		/// Account balance.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub balance: Option<U256>,
		/// Account nonce.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub nonce: Option<u32>,
		/// Account code.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub code: Option<Bytes>,
		/// Account storage slots.
		#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
		pub storage: BTreeMap<Bytes, Option<Bytes>>,
	}
}

define_versioned_type! {
	/// Version 1 of an opcode and syscall execution trace.
	#[derive(
		Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
	)]
	#[serde(default, rename_all = "camelCase")]
	pub struct ExecutionTraceV1 {
		/// Total gas used by the transaction.
		pub gas: u64,
		/// Weight consumed by the transaction meter.
		pub weight_consumed: Weight,
		/// Base call weight of the transaction.
		pub base_call_weight: Weight,
		/// Whether transaction execution failed.
		pub failed: bool,
		/// Value returned by transaction execution.
		pub return_value: Bytes,
		/// Execution steps captured by the tracer.
		pub struct_logs: Vec<ExecutionStepV1>,
	}
}

define_versioned_type! {
	/// Version 1 of one opcode or syscall execution step.
	#[derive(
		Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
	)]
	#[serde(default, rename_all = "camelCase")]
	pub struct ExecutionStepV1 {
		/// Remaining gas before executing this step.
		#[codec(compact)]
		pub gas: u64,
		/// Gas cost of executing this step.
		#[codec(compact)]
		pub gas_cost: u64,
		/// Weight cost of executing this step.
		pub weight_cost: Weight,
		/// Current call depth.
		pub depth: u16,
		/// Return data from the last frame output.
		#[serde(skip_serializing_if = "Bytes::is_empty")]
		pub return_data: Bytes,
		/// Error that occurred during execution.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub error: Option<String>,
		/// Opcode or syscall step details.
		#[serde(flatten)]
		pub kind: ExecutionStepKindV1,
	}
}

define_versioned_type! {
	/// Version 1 of opcode or syscall step details.
	#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode)]
	#[serde(untagged)]
	pub enum ExecutionStepKindV1 {
		/// An EVM opcode execution.
		EvmOpcode {
			/// Program counter.
			#[codec(compact)]
			pc: u32,
			/// Raw opcode byte.
			op: u8,
			/// EVM stack contents.
			#[serde(default)]
			stack: Vec<Bytes>,
			/// EVM memory contents.
			#[serde(default, skip_serializing_if = "Vec::is_empty")]
			memory: Vec<Bytes>,
			/// Contract storage changes.
			#[serde(default, skip_serializing_if = "Option::is_none")]
			storage: Option<BTreeMap<Bytes, Bytes>>,
		},
		/// A PVM syscall execution.
		PvmSyscall {
			/// Raw syscall index.
			op: u8,
			/// Syscall arguments.
			#[serde(default, skip_serializing_if = "Vec::is_empty")]
			args: Vec<u64>,
			/// Syscall return value.
			#[serde(default, skip_serializing_if = "Option::is_none")]
			returned: Option<u64>,
		},
	}
}

impl Default for ExecutionStepKindV1 {
	fn default() -> Self {
		Self::EvmOpcode { pc: 0, op: 0, stack: Vec::new(), memory: Vec::new(), storage: None }
	}
}

define_versioned_type! {
	/// Version 1 of a smart contract execution call trace.
	#[derive(
		Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
	)]
	#[serde(default, rename_all = "camelCase")]
	pub struct CallTraceV1 {
		/// Address of the sender.
		pub from: H160,
		/// Amount of gas provided for the call.
		pub gas: u64,
		/// Amount of gas used.
		pub gas_used: u64,
		/// Address of the receiver.
		pub to: H160,
		/// Call input data.
		pub input: Bytes,
		/// Return data.
		#[serde(skip_serializing_if = "Bytes::is_empty")]
		pub output: Bytes,
		/// Error message if the call failed.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub error: Option<String>,
		/// Revert reason if the call reverted.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub revert_reason: Option<String>,
		/// Nested calls.
		#[serde(skip_serializing_if = "Vec::is_empty")]
		pub calls: Vec<CallTraceV1>,
		/// Logs emitted during the call.
		#[serde(skip_serializing_if = "Vec::is_empty")]
		pub logs: Vec<CallLogV1>,
		/// Amount of value transferred.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub value: Option<U256>,
		/// Type of call.
		#[serde(rename = "type")]
		pub call_type: CallTypeV1,
	}
}

define_versioned_type! {
	/// Version 1 of a log emitted during a call trace.
	#[derive(
		Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
	)]
	pub struct CallLogV1 {
		/// Address of the contract that emitted the log.
		pub address: H160,
		/// Topics used to index the log.
		#[serde(default, skip_serializing_if = "Vec::is_empty")]
		pub topics: Vec<H256>,
		/// Log data.
		pub data: Bytes,
		/// Position of the log relative to subcalls within the same trace.
		pub position: u32,
	}
}
