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

use crate::evm::{Bytes, CallTracer};
use alloc::{fmt, string::String, vec::Vec};
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use serde::{
	de::{self, MapAccess, Visitor},
	Deserialize, Deserializer, Serialize,
};
use sp_core::{H160, H256, U256};

/// Tracer configuration used to trace calls.
#[derive(TypeInfo, Debug, Clone, Encode, Decode, Serialize, PartialEq)]
#[serde(tag = "tracer", content = "tracerConfig")]
pub enum TracerConfig {
	/// A tracer that captures call traces.
	#[serde(rename = "callTracer")]
	CallTracer {
		/// Whether or not to capture logs.
		#[serde(rename = "withLog")]
		with_logs: bool,
	},
}

impl TracerConfig {
	/// Build the tracer associated to this config.
	pub fn build<G>(self, gas_mapper: G) -> CallTracer<U256, G> {
		match self {
			Self::CallTracer { with_logs } => CallTracer::new(with_logs, gas_mapper),
		}
	}
}

/// Custom deserializer to support the following JSON format:
///
/// ```json
/// { "tracer": "callTracer", "tracerConfig": { "withLogs": false } }
/// ```
///
/// ```json
/// { "tracer": "callTracer" }
/// ```
impl<'de> Deserialize<'de> for TracerConfig {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		struct TracerConfigVisitor;

		impl<'de> Visitor<'de> for TracerConfigVisitor {
			type Value = TracerConfig;

			fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
				formatter.write_str("a map with tracer and optional tracerConfig")
			}

			fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
			where
				M: MapAccess<'de>,
			{
				let mut tracer_type: Option<String> = None;
				let mut with_logs = None;

				while let Some(key) = map.next_key::<String>()? {
					match key.as_str() {
						"tracer" => {
							tracer_type = map.next_value()?;
						},
						"tracerConfig" => {
							#[derive(Deserialize)]
							struct CallTracerConfig {
								#[serde(rename = "withLogs")]
								with_logs: Option<bool>,
							}
							let inner: CallTracerConfig = map.next_value()?;
							with_logs = inner.with_logs;
						},
						_ => {},
					}
				}

				match tracer_type.as_deref() {
					Some("callTracer") =>
						Ok(TracerConfig::CallTracer { with_logs: with_logs.unwrap_or(true) }),
					_ => Err(de::Error::custom("Unsupported or missing tracer type")),
				}
			}
		}

		deserializer.deserialize_map(TracerConfigVisitor)
	}
}

#[test]
fn test_tracer_config_serialization() {
	let tracers = vec![
		(r#"{"tracer": "callTracer"}"#, TracerConfig::CallTracer { with_logs: true }),
		(
			r#"{"tracer": "callTracer", "tracerConfig": { "withLogs": true }}"#,
			TracerConfig::CallTracer { with_logs: true },
		),
		(
			r#"{"tracer": "callTracer", "tracerConfig": { "withLogs": false }}"#,
			TracerConfig::CallTracer { with_logs: false },
		),
	];

	for (json_data, expected) in tracers {
		let result: TracerConfig =
			serde_json::from_str(json_data).expect("Deserialization should succeed");
		assert_eq!(result, expected);
	}
}

impl Default for TracerConfig {
	fn default() -> Self {
		TracerConfig::CallTracer { with_logs: false }
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
	pub trace: CallTrace,
}
