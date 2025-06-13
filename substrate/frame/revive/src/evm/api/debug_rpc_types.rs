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
/// Only "callTracer" is supported for now.
#[derive(TypeInfo, Debug, Clone, Encode, Decode, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tracer", content = "tracerConfig", rename_all = "camelCase")]
pub enum TracerType {
	/// A tracer that traces calls.
	CallTracer(Option<CallTracerConfig>),

	/// A tracer that traces the prestate.
	PrestateTracer(Option<PrestateTracerConfig>),
}

impl From<CallTracerConfig> for TracerType {
	fn from(config: CallTracerConfig) -> Self {
		TracerType::CallTracer(Some(config))
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
}

/// A Trace
#[derive(TypeInfo, From, Encode, Decode, Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
#[serde(untagged)]
pub enum Trace {
	/// A call trace.
	Call(CallTrace),
	/// A prestate trace.
	Prestate(PrestateTrace),
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
	#[serde(
		skip_serializing_if = "BTreeMap::is_empty",
		serialize_with = "serialize_map_skip_none"
	)]
	pub storage: BTreeMap<Bytes, Option<Bytes>>,
}

/// Serializes a map of `K -> Option<V>`, omitting any entries whose value is `None`.
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

/// A smart contract execution call trace.
#[derive(
	TypeInfo, Default, Encode, Decode, Serialize, Deserialize, Clone, Debug, Eq, PartialEq,
)]
pub struct CallTrace<Gas = U256> {
	/// Address of the sender.
	pub from: H160,
	/// Amount of gas provided for the call.
	pub gas: Gas,
	/// Amount of gas used.
	#[serde(rename = "gasUsed")]
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
	#[serde(rename = "revertReason", skip_serializing_if = "Option::is_none")]
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
pub struct TransactionTrace {
	/// The transaction hash.
	#[serde(rename = "txHash")]
	pub tx_hash: H256,
	/// The trace of the transaction.
	#[serde(rename = "result")]
	pub trace: Trace,
}
