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

use crate::evm::Bytes;
use alloc::{collections::BTreeMap, string::String, vec::Vec};
use codec::{Decode, Encode};
use derive_more::From;
use scale_info::TypeInfo;
use serde::{
	ser::{SerializeMap, Serializer},
	Deserialize, Serialize,
};
use sp_core::{H160, H256, U256};

/// The type of tracer to use.
#[derive(TypeInfo, Debug, Clone, Encode, Decode, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tracer", content = "tracerConfig", rename_all = "camelCase")]
pub enum TracerType {
	/// A tracer that traces calls.
	CallTracer(Option<CallTracerConfig>),

	/// A tracer that traces the prestate.
	PrestateTracer(Option<PrestateTracerConfig>),

	/// A tracer that traces opcodes.
	StructLogger(Option<OpcodeTracerConfig>),
}

impl From<CallTracerConfig> for TracerType {
	fn from(config: CallTracerConfig) -> Self {
		TracerType::CallTracer(Some(config))
	}
}

impl From<PrestateTracerConfig> for TracerType {
	fn from(config: PrestateTracerConfig) -> Self {
		TracerType::PrestateTracer(Some(config))
	}
}

impl From<OpcodeTracerConfig> for TracerType {
	fn from(config: OpcodeTracerConfig) -> Self {
		TracerType::StructLogger(Some(config))
	}
}

impl Default for TracerType {
	fn default() -> Self {
		TracerType::CallTracer(Some(CallTracerConfig::default()))
	}
}

/// Tracer configuration used to trace calls.
#[derive(TypeInfo, Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "std", derive(Deserialize, Serialize), serde(rename_all = "camelCase"))]
pub struct TracerConfig {
	/// The tracer type.
	#[cfg_attr(feature = "std", serde(flatten, default))]
	pub config: TracerType,

	/// Timeout for the tracer.
	#[cfg_attr(feature = "std", serde(with = "humantime_serde", default))]
	pub timeout: Option<core::time::Duration>,
}

/// The configuration for the call tracer.
#[derive(Clone, Debug, Decode, Serialize, Deserialize, Encode, PartialEq, TypeInfo)]
#[serde(default, rename_all = "camelCase")]
pub struct CallTracerConfig {
	/// Whether to include logs in the trace.
	pub with_logs: bool,

	/// Whether to only include the top-level calls in the trace.
	pub only_top_call: bool,
}

impl Default for CallTracerConfig {
	fn default() -> Self {
		Self { with_logs: true, only_top_call: false }
	}
}

/// The configuration for the prestate tracer.
#[derive(Clone, Debug, Decode, Serialize, Deserialize, Encode, PartialEq, TypeInfo)]
#[serde(default, rename_all = "camelCase")]
pub struct PrestateTracerConfig {
	/// Whether to include the diff mode in the trace.
	pub diff_mode: bool,

	/// Whether to include storage in the trace.
	pub disable_storage: bool,

	/// Whether to include code in the trace.
	pub disable_code: bool,
}

impl Default for PrestateTracerConfig {
	fn default() -> Self {
		Self { diff_mode: false, disable_storage: false, disable_code: false }
	}
}

/// The configuration for the opcode tracer.
#[derive(Clone, Debug, Decode, Serialize, Deserialize, Encode, PartialEq, TypeInfo)]
#[serde(default, rename_all = "camelCase")]
pub struct OpcodeTracerConfig {
	/// Whether to enable memory capture (default: false)
	pub enable_memory: bool,

	/// Whether to disable stack capture (default: false)
	pub disable_stack: bool,

	/// Whether to disable storage capture (default: false)
	pub disable_storage: bool,

	/// Whether to enable return data capture (default: false)
	pub enable_return_data: bool,

	/// Limit number of steps captured (default: 0, no limit)
	pub limit: u64,
}

impl Default for OpcodeTracerConfig {
	fn default() -> Self {
		Self {
			enable_memory: false,
			disable_stack: false,
			disable_storage: false,
			enable_return_data: false,
			limit: 0,
		}
	}
}

/// Serialization should support the following JSON format:
///
/// ```json
/// { "tracer": "callTracer", "tracerConfig": { "withLogs": false } }
/// ```
///
/// ```json
/// { "tracer": "callTracer" }
/// ```
#[test]
fn test_tracer_config_serialization() {
	let tracers = vec![
		(
			r#"{"tracer": "callTracer"}"#,
			TracerConfig { config: TracerType::CallTracer(None), timeout: None },
		),
		(
			r#"{"tracer": "callTracer", "tracerConfig": { "withLogs": false }}"#,
			TracerConfig {
				config: CallTracerConfig { with_logs: false, only_top_call: false }.into(),
				timeout: None,
			},
		),
		(
			r#"{"tracer": "callTracer", "tracerConfig": { "onlyTopCall": true }}"#,
			TracerConfig {
				config: CallTracerConfig { with_logs: true, only_top_call: true }.into(),
				timeout: None,
			},
		),
		(
			r#"{"tracer": "callTracer", "tracerConfig": { "onlyTopCall": true }, "timeout": "10ms"}"#,
			TracerConfig {
				config: CallTracerConfig { with_logs: true, only_top_call: true }.into(),
				timeout: Some(core::time::Duration::from_millis(10)),
			},
		),
		(
			r#"{"tracer": "structLogger"}"#,
			TracerConfig { config: TracerType::StructLogger(None), timeout: None },
		),
		(
			r#"{"tracer": "structLogger", "tracerConfig": { "enableMemory": true }}"#,
			TracerConfig {
				config: OpcodeTracerConfig { enable_memory: true, ..Default::default() }.into(),
				timeout: None,
			},
		),
	];

	for (json_data, expected) in tracers {
		let result: TracerConfig =
			serde_json::from_str(json_data).expect("Deserialization should succeed");
		assert_eq!(result, expected);
	}
}

/// The type of call that was executed.
#[derive(
	Default, TypeInfo, Encode, Decode, Serialize, Deserialize, Eq, PartialEq, Clone, Debug,
)]
#[serde(rename_all = "UPPERCASE")]
pub enum CallType {
	/// A regular call.
	#[default]
	Call,
	/// A read-only call.
	StaticCall,
	/// A delegate call.
	DelegateCall,
	/// A create call.
	Create,
	/// A create2 call.
	Create2,
}

/// A Trace
#[derive(TypeInfo, From, Encode, Decode, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
#[serde(untagged)]
pub enum Trace {
	/// A call trace.
	Call(CallTrace),
	/// A prestate trace.
	Prestate(PrestateTrace),
	/// An opcode trace.
	Opcode(OpcodeTrace),
}

/// A prestate Trace
#[derive(TypeInfo, Encode, Decode, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
#[serde(untagged)]
pub enum PrestateTrace {
	/// The Prestate mode returns the accounts necessary to execute a given transaction
	Prestate(BTreeMap<H160, PrestateTraceInfo>),

	/// The diff mode returns the differences between the transaction's pre and post-state
	/// The result only contains the accounts that were modified by the transaction
	DiffMode {
		/// The state before the call.
		///  The accounts in the `pre` field will contain all of their basic fields, even if those
		/// fields have not been modified. For `storage` however, only non-empty slots that have
		/// been modified will be included
		pre: BTreeMap<H160, PrestateTraceInfo>,
		/// The state after the call.
		/// It only contains the specific fields that were actually modified during the transaction
		post: BTreeMap<H160, PrestateTraceInfo>,
	},
}

impl PrestateTrace {
	/// Returns the pre and post trace info.
	pub fn state_mut(
		&mut self,
	) -> (&mut BTreeMap<H160, PrestateTraceInfo>, Option<&mut BTreeMap<H160, PrestateTraceInfo>>) {
		match self {
			PrestateTrace::Prestate(pre) => (pre, None),
			PrestateTrace::DiffMode { pre, post } => (pre, Some(post)),
		}
	}
}

/// The info of a prestate trace.
#[derive(
	TypeInfo, Default, Encode, Decode, Serialize, Deserialize, Clone, Debug, Eq, PartialEq,
)]
pub struct PrestateTraceInfo {
	/// The balance of the account.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub balance: Option<U256>,
	/// The nonce of the account.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub nonce: Option<u32>,
	/// The code of the contract account.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub code: Option<Bytes>,
	/// The storage of the contract account.
	#[serde(skip_serializing_if = "is_empty", serialize_with = "serialize_map_skip_none")]
	pub storage: BTreeMap<Bytes, Option<Bytes>>,
}

/// Returns true if the map has no `Some` element
pub fn is_empty<K, V>(map: &BTreeMap<K, Option<V>>) -> bool {
	!map.values().any(|v| v.is_some())
}

/// Serializes a map, skipping `None` values.
pub fn serialize_map_skip_none<S, K, V>(
	map: &BTreeMap<K, Option<V>>,
	serializer: S,
) -> Result<S::Ok, S::Error>
where
	S: Serializer,
	K: serde::Serialize,
	V: serde::Serialize,
{
	let len = map.values().filter(|v| v.is_some()).count();
	let mut ser_map = serializer.serialize_map(Some(len))?;

	for (key, opt_val) in map {
		if let Some(val) = opt_val {
			ser_map.serialize_entry(key, val)?;
		}
	}

	ser_map.end()
}

/// An opcode trace containing the step-by-step execution of EVM instructions.
/// This matches Geth's structLogger output format.
#[derive(TypeInfo, Encode, Decode, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpcodeTrace {
	/// Total gas used by the transaction.
	pub gas: u64,
	/// Whether the transaction failed.
	pub failed: bool,
	/// The return value of the transaction.
	pub return_value: Bytes,
	/// The list of opcode execution steps (structLogs in Geth).
	pub struct_logs: Vec<OpcodeStep>,
}

impl Default for OpcodeTrace {
	fn default() -> Self {
		Self { gas: 0, failed: false, return_value: Bytes::default(), struct_logs: Vec::new() }
	}
}

/// A single opcode execution step.
/// This matches Geth's structLog format exactly.
#[derive(TypeInfo, Encode, Decode, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpcodeStep {
	/// The program counter.
	pub pc: u64,
	/// The opcode being executed.
	#[serde(serialize_with = "serialize_opcode", deserialize_with = "deserialize_opcode")]
	pub op: u8,
	/// Remaining gas before executing this opcode.
	pub gas: u64,
	/// Cost of executing this opcode.
	pub gas_cost: u64,
	/// Current call depth.
	pub depth: u32,
	/// EVM stack contents (optional based on config).
	#[serde(skip_serializing_if = "Option::is_none")]
	pub stack: Option<Vec<Bytes>>,
	/// EVM memory contents (optional based on config).
	#[serde(skip_serializing_if = "Option::is_none")]
	pub memory: Option<Vec<Bytes>>,
	/// Contract storage changes (optional based on config).
	#[serde(skip_serializing_if = "Option::is_none")]
	pub storage: Option<alloc::collections::BTreeMap<Bytes, Bytes>>,
	/// Any error that occurred during opcode execution.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
}

/// Get opcode name from byte value using REVM opcode names
fn get_opcode_name(opcode: u8) -> &'static str {
	use revm::bytecode::opcode::*;

	macro_rules! opcode_match {
        ($($op:ident),*) => {
            match opcode {
                $(
                    $op => stringify!($op),
                )*
                _ => "INVALID",
            }
        };
    }

	opcode_match!(
		STOP,
		ADD,
		MUL,
		SUB,
		DIV,
		SDIV,
		MOD,
		SMOD,
		ADDMOD,
		MULMOD,
		EXP,
		SIGNEXTEND,
		LT,
		GT,
		SLT,
		SGT,
		EQ,
		ISZERO,
		AND,
		OR,
		XOR,
		NOT,
		BYTE,
		SHL,
		SHR,
		SAR,
		KECCAK256,
		ADDRESS,
		BALANCE,
		ORIGIN,
		CALLER,
		CALLVALUE,
		CALLDATALOAD,
		CALLDATASIZE,
		CALLDATACOPY,
		CODESIZE,
		CODECOPY,
		GASPRICE,
		EXTCODESIZE,
		EXTCODECOPY,
		RETURNDATASIZE,
		RETURNDATACOPY,
		EXTCODEHASH,
		BLOCKHASH,
		COINBASE,
		TIMESTAMP,
		NUMBER,
		DIFFICULTY,
		GASLIMIT,
		CHAINID,
		SELFBALANCE,
		BASEFEE,
		BLOBHASH,
		BLOBBASEFEE,
		POP,
		MLOAD,
		MSTORE,
		MSTORE8,
		SLOAD,
		SSTORE,
		JUMP,
		JUMPI,
		PC,
		MSIZE,
		GAS,
		JUMPDEST,
		TLOAD,
		TSTORE,
		MCOPY,
		PUSH0,
		PUSH1,
		PUSH2,
		PUSH3,
		PUSH4,
		PUSH5,
		PUSH6,
		PUSH7,
		PUSH8,
		PUSH9,
		PUSH10,
		PUSH11,
		PUSH12,
		PUSH13,
		PUSH14,
		PUSH15,
		PUSH16,
		PUSH17,
		PUSH18,
		PUSH19,
		PUSH20,
		PUSH21,
		PUSH22,
		PUSH23,
		PUSH24,
		PUSH25,
		PUSH26,
		PUSH27,
		PUSH28,
		PUSH29,
		PUSH30,
		PUSH31,
		PUSH32,
		DUP1,
		DUP2,
		DUP3,
		DUP4,
		DUP5,
		DUP6,
		DUP7,
		DUP8,
		DUP9,
		DUP10,
		DUP11,
		DUP12,
		DUP13,
		DUP14,
		DUP15,
		DUP16,
		SWAP1,
		SWAP2,
		SWAP3,
		SWAP4,
		SWAP5,
		SWAP6,
		SWAP7,
		SWAP8,
		SWAP9,
		SWAP10,
		SWAP11,
		SWAP12,
		SWAP13,
		SWAP14,
		SWAP15,
		SWAP16,
		LOG0,
		LOG1,
		LOG2,
		LOG3,
		LOG4,
		CREATE,
		CALL,
		CALLCODE,
		RETURN,
		DELEGATECALL,
		CREATE2,
		STATICCALL,
		REVERT,
		INVALID,
		SELFDESTRUCT
	)
}

/// Get opcode byte from name string
fn get_opcode_byte(name: &str) -> Option<u8> {
	match name {
		// Arithmetic operations
		"STOP" => Some(revm::bytecode::opcode::STOP),
		"ADD" => Some(revm::bytecode::opcode::ADD),
		"MUL" => Some(revm::bytecode::opcode::MUL),
		"SUB" => Some(revm::bytecode::opcode::SUB),
		"DIV" => Some(revm::bytecode::opcode::DIV),
		"SDIV" => Some(revm::bytecode::opcode::SDIV),
		"MOD" => Some(revm::bytecode::opcode::MOD),
		"SMOD" => Some(revm::bytecode::opcode::SMOD),
		"ADDMOD" => Some(revm::bytecode::opcode::ADDMOD),
		"MULMOD" => Some(revm::bytecode::opcode::MULMOD),
		"EXP" => Some(revm::bytecode::opcode::EXP),
		"SIGNEXTEND" => Some(revm::bytecode::opcode::SIGNEXTEND),
		// Comparison operations
		"LT" => Some(revm::bytecode::opcode::LT),
		"GT" => Some(revm::bytecode::opcode::GT),
		"SLT" => Some(revm::bytecode::opcode::SLT),
		"SGT" => Some(revm::bytecode::opcode::SGT),
		"EQ" => Some(revm::bytecode::opcode::EQ),
		"ISZERO" => Some(revm::bytecode::opcode::ISZERO),
		// Bitwise operations
		"AND" => Some(revm::bytecode::opcode::AND),
		"OR" => Some(revm::bytecode::opcode::OR),
		"XOR" => Some(revm::bytecode::opcode::XOR),
		"NOT" => Some(revm::bytecode::opcode::NOT),
		"BYTE" => Some(revm::bytecode::opcode::BYTE),
		"SHL" => Some(revm::bytecode::opcode::SHL),
		"SHR" => Some(revm::bytecode::opcode::SHR),
		"SAR" => Some(revm::bytecode::opcode::SAR),
		// Hash operations
		"KECCAK256" => Some(revm::bytecode::opcode::KECCAK256),
		// Environment information
		"ADDRESS" => Some(revm::bytecode::opcode::ADDRESS),
		"BALANCE" => Some(revm::bytecode::opcode::BALANCE),
		"ORIGIN" => Some(revm::bytecode::opcode::ORIGIN),
		"CALLER" => Some(revm::bytecode::opcode::CALLER),
		"CALLVALUE" => Some(revm::bytecode::opcode::CALLVALUE),
		"CALLDATALOAD" => Some(revm::bytecode::opcode::CALLDATALOAD),
		"CALLDATASIZE" => Some(revm::bytecode::opcode::CALLDATASIZE),
		"CALLDATACOPY" => Some(revm::bytecode::opcode::CALLDATACOPY),
		"CODESIZE" => Some(revm::bytecode::opcode::CODESIZE),
		"CODECOPY" => Some(revm::bytecode::opcode::CODECOPY),
		"GASPRICE" => Some(revm::bytecode::opcode::GASPRICE),
		"EXTCODESIZE" => Some(revm::bytecode::opcode::EXTCODESIZE),
		"EXTCODECOPY" => Some(revm::bytecode::opcode::EXTCODECOPY),
		"RETURNDATASIZE" => Some(revm::bytecode::opcode::RETURNDATASIZE),
		"RETURNDATACOPY" => Some(revm::bytecode::opcode::RETURNDATACOPY),
		"EXTCODEHASH" => Some(revm::bytecode::opcode::EXTCODEHASH),
		// Block information
		"BLOCKHASH" => Some(revm::bytecode::opcode::BLOCKHASH),
		"COINBASE" => Some(revm::bytecode::opcode::COINBASE),
		"TIMESTAMP" => Some(revm::bytecode::opcode::TIMESTAMP),
		"NUMBER" => Some(revm::bytecode::opcode::NUMBER),
		"DIFFICULTY" => Some(revm::bytecode::opcode::DIFFICULTY),
		"GASLIMIT" => Some(revm::bytecode::opcode::GASLIMIT),
		"CHAINID" => Some(revm::bytecode::opcode::CHAINID),
		"SELFBALANCE" => Some(revm::bytecode::opcode::SELFBALANCE),
		"BASEFEE" => Some(revm::bytecode::opcode::BASEFEE),
		"BLOBHASH" => Some(revm::bytecode::opcode::BLOBHASH),
		"BLOBBASEFEE" => Some(revm::bytecode::opcode::BLOBBASEFEE),
		// Storage and memory operations
		"POP" => Some(revm::bytecode::opcode::POP),
		"MLOAD" => Some(revm::bytecode::opcode::MLOAD),
		"MSTORE" => Some(revm::bytecode::opcode::MSTORE),
		"MSTORE8" => Some(revm::bytecode::opcode::MSTORE8),
		"SLOAD" => Some(revm::bytecode::opcode::SLOAD),
		"SSTORE" => Some(revm::bytecode::opcode::SSTORE),
		"JUMP" => Some(revm::bytecode::opcode::JUMP),
		"JUMPI" => Some(revm::bytecode::opcode::JUMPI),
		"PC" => Some(revm::bytecode::opcode::PC),
		"MSIZE" => Some(revm::bytecode::opcode::MSIZE),
		"GAS" => Some(revm::bytecode::opcode::GAS),
		"JUMPDEST" => Some(revm::bytecode::opcode::JUMPDEST),
		"TLOAD" => Some(revm::bytecode::opcode::TLOAD),
		"TSTORE" => Some(revm::bytecode::opcode::TSTORE),
		"MCOPY" => Some(revm::bytecode::opcode::MCOPY),
		// Push operations
		"PUSH0" => Some(revm::bytecode::opcode::PUSH0),
		"PUSH1" => Some(revm::bytecode::opcode::PUSH1),
		"PUSH2" => Some(revm::bytecode::opcode::PUSH2),
		"PUSH3" => Some(revm::bytecode::opcode::PUSH3),
		"PUSH4" => Some(revm::bytecode::opcode::PUSH4),
		"PUSH5" => Some(revm::bytecode::opcode::PUSH5),
		"PUSH6" => Some(revm::bytecode::opcode::PUSH6),
		"PUSH7" => Some(revm::bytecode::opcode::PUSH7),
		"PUSH8" => Some(revm::bytecode::opcode::PUSH8),
		"PUSH9" => Some(revm::bytecode::opcode::PUSH9),
		"PUSH10" => Some(revm::bytecode::opcode::PUSH10),
		"PUSH11" => Some(revm::bytecode::opcode::PUSH11),
		"PUSH12" => Some(revm::bytecode::opcode::PUSH12),
		"PUSH13" => Some(revm::bytecode::opcode::PUSH13),
		"PUSH14" => Some(revm::bytecode::opcode::PUSH14),
		"PUSH15" => Some(revm::bytecode::opcode::PUSH15),
		"PUSH16" => Some(revm::bytecode::opcode::PUSH16),
		"PUSH17" => Some(revm::bytecode::opcode::PUSH17),
		"PUSH18" => Some(revm::bytecode::opcode::PUSH18),
		"PUSH19" => Some(revm::bytecode::opcode::PUSH19),
		"PUSH20" => Some(revm::bytecode::opcode::PUSH20),
		"PUSH21" => Some(revm::bytecode::opcode::PUSH21),
		"PUSH22" => Some(revm::bytecode::opcode::PUSH22),
		"PUSH23" => Some(revm::bytecode::opcode::PUSH23),
		"PUSH24" => Some(revm::bytecode::opcode::PUSH24),
		"PUSH25" => Some(revm::bytecode::opcode::PUSH25),
		"PUSH26" => Some(revm::bytecode::opcode::PUSH26),
		"PUSH27" => Some(revm::bytecode::opcode::PUSH27),
		"PUSH28" => Some(revm::bytecode::opcode::PUSH28),
		"PUSH29" => Some(revm::bytecode::opcode::PUSH29),
		"PUSH30" => Some(revm::bytecode::opcode::PUSH30),
		"PUSH31" => Some(revm::bytecode::opcode::PUSH31),
		"PUSH32" => Some(revm::bytecode::opcode::PUSH32),
		// Dup operations
		"DUP1" => Some(revm::bytecode::opcode::DUP1),
		"DUP2" => Some(revm::bytecode::opcode::DUP2),
		"DUP3" => Some(revm::bytecode::opcode::DUP3),
		"DUP4" => Some(revm::bytecode::opcode::DUP4),
		"DUP5" => Some(revm::bytecode::opcode::DUP5),
		"DUP6" => Some(revm::bytecode::opcode::DUP6),
		"DUP7" => Some(revm::bytecode::opcode::DUP7),
		"DUP8" => Some(revm::bytecode::opcode::DUP8),
		"DUP9" => Some(revm::bytecode::opcode::DUP9),
		"DUP10" => Some(revm::bytecode::opcode::DUP10),
		"DUP11" => Some(revm::bytecode::opcode::DUP11),
		"DUP12" => Some(revm::bytecode::opcode::DUP12),
		"DUP13" => Some(revm::bytecode::opcode::DUP13),
		"DUP14" => Some(revm::bytecode::opcode::DUP14),
		"DUP15" => Some(revm::bytecode::opcode::DUP15),
		"DUP16" => Some(revm::bytecode::opcode::DUP16),
		// Swap operations
		"SWAP1" => Some(revm::bytecode::opcode::SWAP1),
		"SWAP2" => Some(revm::bytecode::opcode::SWAP2),
		"SWAP3" => Some(revm::bytecode::opcode::SWAP3),
		"SWAP4" => Some(revm::bytecode::opcode::SWAP4),
		"SWAP5" => Some(revm::bytecode::opcode::SWAP5),
		"SWAP6" => Some(revm::bytecode::opcode::SWAP6),
		"SWAP7" => Some(revm::bytecode::opcode::SWAP7),
		"SWAP8" => Some(revm::bytecode::opcode::SWAP8),
		"SWAP9" => Some(revm::bytecode::opcode::SWAP9),
		"SWAP10" => Some(revm::bytecode::opcode::SWAP10),
		"SWAP11" => Some(revm::bytecode::opcode::SWAP11),
		"SWAP12" => Some(revm::bytecode::opcode::SWAP12),
		"SWAP13" => Some(revm::bytecode::opcode::SWAP13),
		"SWAP14" => Some(revm::bytecode::opcode::SWAP14),
		"SWAP15" => Some(revm::bytecode::opcode::SWAP15),
		"SWAP16" => Some(revm::bytecode::opcode::SWAP16),
		// Log operations
		"LOG0" => Some(revm::bytecode::opcode::LOG0),
		"LOG1" => Some(revm::bytecode::opcode::LOG1),
		"LOG2" => Some(revm::bytecode::opcode::LOG2),
		"LOG3" => Some(revm::bytecode::opcode::LOG3),
		"LOG4" => Some(revm::bytecode::opcode::LOG4),
		// System operations
		"CREATE" => Some(revm::bytecode::opcode::CREATE),
		"CALL" => Some(revm::bytecode::opcode::CALL),
		"CALLCODE" => Some(revm::bytecode::opcode::CALLCODE),
		"RETURN" => Some(revm::bytecode::opcode::RETURN),
		"DELEGATECALL" => Some(revm::bytecode::opcode::DELEGATECALL),
		"CREATE2" => Some(revm::bytecode::opcode::CREATE2),
		"STATICCALL" => Some(revm::bytecode::opcode::STATICCALL),
		"REVERT" => Some(revm::bytecode::opcode::REVERT),
		"INVALID" => Some(revm::bytecode::opcode::INVALID),
		"SELFDESTRUCT" => Some(revm::bytecode::opcode::SELFDESTRUCT),
		_ => None,
	}
}

/// Serialize opcode as string using REVM opcode names
fn serialize_opcode<S>(opcode: &u8, serializer: S) -> Result<S::Ok, S::Error>
where
	S: serde::Serializer,
{
	let name = get_opcode_name(*opcode);
	serializer.serialize_str(name)
}

/// Deserialize opcode from string using reverse lookup table
fn deserialize_opcode<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
	D: serde::Deserializer<'de>,
{
	let s = String::deserialize(deserializer)?;
	get_opcode_byte(&s)
		.ok_or_else(|| serde::de::Error::custom(alloc::format!("Unknown opcode: {}", s)))
}

/// A smart contract execution call trace.
#[derive(
	TypeInfo, Default, Encode, Decode, Serialize, Deserialize, Clone, Debug, Eq, PartialEq,
)]
#[serde(rename_all = "camelCase")]
pub struct CallTrace<Gas = U256> {
	/// Address of the sender.
	pub from: H160,
	/// Amount of gas provided for the call.
	pub gas: Gas,
	/// Amount of gas used.
	pub gas_used: Gas,
	/// Address of the receiver.
	pub to: H160,
	/// Call input data.
	pub input: Bytes,
	/// Return data.
	#[serde(skip_serializing_if = "Bytes::is_empty")]
	pub output: Bytes,
	/// The error message if the call failed.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
	/// The revert reason, if the call reverted.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub revert_reason: Option<String>,
	/// List of sub-calls.
	#[serde(skip_serializing_if = "Vec::is_empty")]
	pub calls: Vec<CallTrace<Gas>>,
	/// List of logs emitted during the call.
	#[serde(skip_serializing_if = "Vec::is_empty")]
	pub logs: Vec<CallLog>,
	/// Amount of value transferred.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub value: Option<U256>,
	/// Type of call.
	#[serde(rename = "type")]
	pub call_type: CallType,
}

/// A log emitted during a call.
#[derive(
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
)]
pub struct CallLog {
	/// The address of the contract that emitted the log.
	pub address: H160,
	/// The topics used to index the log.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub topics: Vec<H256>,
	/// The log's data.
	pub data: Bytes,
	/// Position of the log relative to subcalls within the same trace
	/// See <https://github.com/ethereum/go-ethereum/pull/28389> for details
	#[serde(with = "super::hex_serde")]
	pub position: u32,
}

/// A transaction trace
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TransactionTrace {
	/// The transaction hash.
	pub tx_hash: H256,
	/// The trace of the transaction.
	#[serde(rename = "result")]
	pub trace: Trace,
}
