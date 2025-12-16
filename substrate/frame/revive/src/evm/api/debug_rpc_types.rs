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
	de::{Deserializer, Error, MapAccess, Visitor},
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

	/// A tracer that traces opcodes and syscalls.
	ExecutionTracer(Option<ExecutionTracerConfig>),
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

impl From<ExecutionTracerConfig> for TracerType {
	fn from(config: ExecutionTracerConfig) -> Self {
		TracerType::ExecutionTracer(Some(config))
	}
}

impl Default for TracerType {
	fn default() -> Self {
		TracerType::ExecutionTracer(Some(ExecutionTracerConfig::default()))
	}
}

/// Tracer configuration used to trace calls.
#[derive(TypeInfo, Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "std", derive(Serialize), serde(rename_all = "camelCase"))]
pub struct TracerConfig {
	/// The tracer type.
	#[cfg_attr(feature = "std", serde(flatten, default))]
	pub config: TracerType,

	/// Timeout for the tracer.
	#[cfg_attr(feature = "std", serde(with = "humantime_serde", default))]
	pub timeout: Option<core::time::Duration>,
}

#[cfg(feature = "std")]
impl<'de> Deserialize<'de> for TracerConfig {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		#[derive(Deserialize)]
		#[serde(rename_all = "camelCase")]
		struct TracerConfigWithType {
			#[serde(flatten)]
			config: TracerType,
			#[serde(with = "humantime_serde", default)]
			timeout: Option<core::time::Duration>,
		}

		#[derive(Deserialize)]
		#[serde(rename_all = "camelCase")]
		struct TracerConfigInline {
			#[serde(flatten, default)]
			execution_tracer_config: ExecutionTracerConfig,
			#[serde(with = "humantime_serde", default)]
			timeout: Option<core::time::Duration>,
		}

		#[derive(Deserialize)]
		#[serde(untagged)]
		enum TracerConfigHelper {
			WithType(TracerConfigWithType),
			Inline(TracerConfigInline),
		}

		match TracerConfigHelper::deserialize(deserializer)? {
			TracerConfigHelper::WithType(cfg) =>
				Ok(TracerConfig { config: cfg.config, timeout: cfg.timeout }),
			TracerConfigHelper::Inline(cfg) => Ok(TracerConfig {
				config: TracerType::ExecutionTracer(Some(cfg.execution_tracer_config)),
				timeout: cfg.timeout,
			}),
		}
	}
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

fn zero_to_none<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
	D: Deserializer<'de>,
{
	let opt = Option::<u64>::deserialize(deserializer)?;
	Ok(match opt {
		Some(0) => None,
		other => other,
	})
}

/// The configuration for the execution tracer.
#[derive(Clone, Debug, Decode, Serialize, Deserialize, Encode, PartialEq, TypeInfo)]
#[serde(default, rename_all = "camelCase")]
pub struct ExecutionTracerConfig {
	/// Whether to enable memory capture
	pub enable_memory: bool,

	/// Whether to disable stack capture
	pub disable_stack: bool,

	/// Whether to disable storage capture
	pub disable_storage: bool,

	/// Whether to enable return data capture
	pub enable_return_data: bool,

	/// Limit number of steps captured
	#[serde(skip_serializing_if = "Option::is_none", deserialize_with = "zero_to_none")]
	pub limit: Option<u64>,

	/// Maximum number of memory words to capture per step (default: 16)
	pub memory_word_limit: u32,
}

impl Default for ExecutionTracerConfig {
	fn default() -> Self {
		Self {
			enable_memory: false,
			disable_stack: false,
			disable_storage: false,
			enable_return_data: false,
			limit: None,
			memory_word_limit: 16,
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
///
/// By default if not specified the tracer is an ExecutionTracer, and it's config is passed inline
///
/// ```json
/// { "tracer": null,  "enableMemory": true, "disableStack": false, "disableStorage": false, "enableReturnData": true  }
/// ```
#[test]
fn test_tracer_config_serialization() {
	let tracers = vec![
		(
			r#"{ "enableMemory": true, "disableStack": false, "disableStorage": false, "enableReturnData": true }"#,
			TracerConfig {
				config: TracerType::ExecutionTracer(Some(ExecutionTracerConfig {
					enable_memory: true,
					disable_stack: false,
					disable_storage: false,
					enable_return_data: true,
					limit: None,
					memory_word_limit: 16,
				})),
				timeout: None,
			},
		),
		(
			r#"{  }"#,
			TracerConfig {
				config: TracerType::ExecutionTracer(Some(ExecutionTracerConfig::default())),
				timeout: None,
			},
		),
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
			r#"{"tracer": "ExecutionTracer"}"#,
			TracerConfig { config: TracerType::ExecutionTracer(None), timeout: None },
		),
		(
			r#"{"tracer": "ExecutionTracer", "tracerConfig": { "enableMemory": true }}"#,
			TracerConfig {
				config: ExecutionTracerConfig { enable_memory: true, ..Default::default() }.into(),
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
	/// A selfdestruct call.
	Selfdestruct,
}

/// A Trace
#[derive(TypeInfo, Deserialize, Serialize, From, Encode, Decode, Clone, Debug, Eq, PartialEq)]
#[serde(untagged)]
pub enum Trace {
	/// A call trace.
	Call(CallTrace),
	/// A prestate trace.
	Prestate(PrestateTrace),
	/// An execution trace (opcodes and syscalls).
	Execution(ExecutionTrace),
}

/// A prestate Trace
#[derive(TypeInfo, Encode, Serialize, Decode, Clone, Debug, Eq, PartialEq)]
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

impl<'de> Deserialize<'de> for PrestateTrace {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		struct PrestateTraceVisitor;

		impl<'de> Visitor<'de> for PrestateTraceVisitor {
			type Value = PrestateTrace;

			fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
				formatter.write_str("a map representing either Prestate or DiffMode")
			}

			fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
			where
				A: MapAccess<'de>,
			{
				let mut pre_map = None;
				let mut post_map = None;
				let mut account_map = BTreeMap::new();

				while let Some(key) = map.next_key::<String>()? {
					match key.as_str() {
						"pre" => {
							if pre_map.is_some() {
								return Err(Error::duplicate_field("pre"));
							}
							pre_map = Some(map.next_value::<BTreeMap<H160, PrestateTraceInfo>>()?);
						},
						"post" => {
							if post_map.is_some() {
								return Err(Error::duplicate_field("post"));
							}
							post_map = Some(map.next_value::<BTreeMap<H160, PrestateTraceInfo>>()?);
						},
						_ => {
							let addr: H160 =
								key.parse().map_err(|_| Error::custom("Invalid address"))?;
							let info = map.next_value::<PrestateTraceInfo>()?;
							account_map.insert(addr, info);
						},
					}
				}

				match (pre_map, post_map) {
					(Some(pre), Some(post)) => {
						if !account_map.is_empty() {
							return Err(Error::custom("Mixed diff and prestate mode"));
						}
						Ok(PrestateTrace::DiffMode { pre, post })
					},
					(None, None) => Ok(PrestateTrace::Prestate(account_map)),
					_ => Err(Error::custom("diff mode: must have both 'pre' and 'post'")),
				}
			}
		}

		deserializer.deserialize_map(PrestateTraceVisitor)
	}
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
	#[serde(default, skip_serializing_if = "is_empty", serialize_with = "serialize_map_skip_none")]
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

/// An execution trace containing the step-by-step execution of EVM opcodes and PVM syscalls.
/// This matches Geth's structLogger output format.
#[derive(
	Default, TypeInfo, Encode, Decode, Serialize, Deserialize, Clone, Debug, Eq, PartialEq,
)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionTrace {
	/// Total gas used by the transaction.
	pub gas: u64,
	/// Whether the transaction failed.
	pub failed: bool,
	/// The return value of the transaction.
	pub return_value: Bytes,
	/// The list of execution steps (structLogs in Geth).
	pub struct_logs: Vec<ExecutionStep>,
}

/// An execution step which can be either an EVM opcode or a PVM syscall.
#[derive(TypeInfo, Encode, Decode, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionStep {
	/// Remaining gas before executing this step.
	pub gas: u64,
	/// Cost of executing this step.
	pub gas_cost: u64,
	/// Current call depth.
	pub depth: u32,
	/// Return data from last frame output.
	#[serde(skip_serializing_if = "Bytes::is_empty")]
	pub return_data: Bytes,
	/// Any error that occurred during execution.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
	/// The kind of execution step (EVM opcode or PVM syscall).
	#[serde(flatten)]
	pub kind: ExecutionStepKind,
}

/// The kind of execution step.
#[derive(TypeInfo, Encode, Decode, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
#[serde(untagged)]
pub enum ExecutionStepKind {
	/// An EVM opcode execution.
	EVMOpcode {
		/// The program counter.
		pc: u64,
		/// The opcode being executed.
		#[serde(serialize_with = "serialize_opcode", deserialize_with = "deserialize_opcode")]
		op: u8,
		/// EVM stack contents.
		#[serde(serialize_with = "serialize_stack_minimal")]
		stack: Vec<Bytes>,
		/// EVM memory contents.
		#[serde(
			skip_serializing_if = "Vec::is_empty",
			serialize_with = "serialize_memory_no_prefix"
		)]
		memory: Vec<Bytes>,
		/// Contract storage changes.
		#[serde(
			skip_serializing_if = "Option::is_none",
			serialize_with = "serialize_storage_no_prefix"
		)]
		storage: Option<alloc::collections::BTreeMap<Bytes, Bytes>>,
	},
	/// A PVM syscall execution.
	PVMSyscall {
		/// The executed syscall.
		#[serde(serialize_with = "serialize_syscall_op")]
		op: u32,
	},
}

macro_rules! define_opcode_functions {
	($($op:ident),* $(,)?) => {
		/// Get opcode name from byte value using opcode names
		fn get_opcode_name(opcode: u8) -> &'static str {
			use revm::bytecode::opcode::*;
			match opcode {
				$(
					$op => stringify!($op),
				)*
				_ => "INVALID",
			}
		}

		/// Get opcode byte from name string
		fn get_opcode_byte(name: &str) -> Option<u8> {
			use revm::bytecode::opcode::*;
			match name {
				$(
					stringify!($op) => Some($op),
				)*
				_ => None,
			}
		}
	};
}

define_opcode_functions!(
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
	SELFDESTRUCT,
);

/// Serialize opcode as string using REVM opcode names
fn serialize_opcode<S>(opcode: &u8, serializer: S) -> Result<S::Ok, S::Error>
where
	S: serde::Serializer,
{
	let name = get_opcode_name(*opcode);
	serializer.serialize_str(name)
}

/// Serialize a syscall index to its name
fn serialize_syscall_op<S>(idx: &u32, serializer: S) -> Result<S::Ok, S::Error>
where
	S: serde::Serializer,
{
	use crate::vm::pvm::env::all_syscalls;
	let Some(syscall_name_bytes) = all_syscalls().get(*idx as usize) else {
		return Err(serde::ser::Error::custom(alloc::format!("Unknown syscall: {idx}")))
	};
	let name = core::str::from_utf8(syscall_name_bytes).unwrap_or_default();
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
pub struct CallTrace {
	/// Address of the sender.
	pub from: H160,
	/// Amount of gas provided for the call.
	#[serde(with = "super::hex_serde")]
	pub gas: u64,
	/// Amount of gas used.
	#[serde(with = "super::hex_serde")]
	pub gas_used: u64,
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
	pub calls: Vec<CallTrace>,
	/// List of logs emitted during the call.
	#[serde(skip_serializing_if = "Vec::is_empty")]
	pub logs: Vec<CallLog>,
	/// Amount of value transferred.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub value: Option<U256>,
	/// Type of call.
	#[serde(rename = "type")]
	pub call_type: CallType,
	/// Number of child calls entered (for log position calculation)
	#[serde(skip)]
	pub child_call_count: u32,
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

/// Serialize stack values using minimal hex format (like Geth)
fn serialize_stack_minimal<S>(stack: &Vec<Bytes>, serializer: S) -> Result<S::Ok, S::Error>
where
	S: serde::Serializer,
{
	let minimal_values: Vec<String> = stack.iter().map(|bytes| bytes.to_short_hex()).collect();
	minimal_values.serialize(serializer)
}

/// Serialize memory values without "0x" prefix (like Geth)
fn serialize_memory_no_prefix<S>(memory: &Vec<Bytes>, serializer: S) -> Result<S::Ok, S::Error>
where
	S: serde::Serializer,
{
	let hex_values: Vec<String> = memory.iter().map(|bytes| bytes.to_hex_no_prefix()).collect();
	hex_values.serialize(serializer)
}

/// Serialize storage map without "0x" prefix (like Geth)
fn serialize_storage_no_prefix<S>(
	storage: &Option<alloc::collections::BTreeMap<Bytes, Bytes>>,
	serializer: S,
) -> Result<S::Ok, S::Error>
where
	S: serde::Serializer,
{
	match storage {
		None => serializer.serialize_none(),
		Some(map) => {
			let mut ser_map = serializer.serialize_map(Some(map.len()))?;
			for (key, value) in map {
				ser_map.serialize_entry(&key.to_hex_no_prefix(), &value.to_hex_no_prefix())?;
			}
			ser_map.end()
		},
	}
}
