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

use crate::*;
use serde::{Deserialize, Serialize};

/// Tracer configuration used to trace calls.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TracerConfig {
	/// The tracer type.
	#[serde(flatten, default)]
	pub config: TracerTypeV1,

	/// Timeout for the tracer.
	#[serde(with = "humantime_serde", default)]
	pub timeout: Option<core::time::Duration>,
}

impl<'de> Deserialize<'de> for TracerConfig {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::de::Deserializer<'de>,
	{
		#[derive(Deserialize)]
		#[serde(rename_all = "camelCase")]
		struct TracerConfigWithType {
			#[serde(flatten)]
			config: TracerTypeV1,
			#[serde(with = "humantime_serde", default)]
			timeout: Option<core::time::Duration>,
		}

		#[derive(Deserialize)]
		#[serde(rename_all = "camelCase")]
		struct TracerConfigInline {
			#[serde(flatten, default)]
			execution_tracer_config: ExecutionTracerConfigV1,
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
			TracerConfigHelper::WithType(cfg) => {
				Ok(TracerConfig { config: cfg.config, timeout: cfg.timeout })
			},
			TracerConfigHelper::Inline(cfg) => Ok(TracerConfig {
				config: TracerTypeV1::ExecutionTracer(Some(cfg.execution_tracer_config)),
				timeout: cfg.timeout,
			}),
		}
	}
}

/// Configuration for `debug_traceCall`, extending [`TracerConfig`] with state overrides.
///
/// Per the [Geth specification](https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-debug#debugtracecall),
/// `debug_traceCall` accepts a config object that is a superset of the base tracer config,
/// adding `stateOverrides` (and optionally `blockOverrides` and `txIndex`, which are not yet
/// supported).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceCallConfig {
	/// The base tracer configuration (tracer type, timeout, etc.).
	#[serde(flatten)]
	pub tracer_config: TracerConfig,

	/// Optional state overrides to apply before executing the traced call.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub state_overrides: Option<StateOverrideSet>,
}

/// A transaction trace
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TransactionTrace {
	/// The transaction hash.
	pub tx_hash: H256,
	/// The trace of the transaction.
	#[serde(rename = "result")]
	pub trace: TraceV1,
}

#[cfg(test)]
mod tests {
	use super::*;
	use pallet_revive_types::runtime_api::*;

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
	/// By default if not specified the tracer is an ExecutionTracer, and it's config is passed
	/// inline
	///
	/// ```json
	/// { "tracer": null,  "enableMemory": true, "disableStack": false, "disableStorage": false, "enableReturnData": true  }
	/// ```
	#[test]
	fn test_tracer_config_serialization() {
		let tracers = vec![
			(
				r#"{ "enableMemory": true, "disableStack": false, "disableStorage": false,
		"enableReturnData": true }"#,
				TracerConfig {
					config: TracerTypeV1::ExecutionTracer(Some(ExecutionTracerConfigV1 {
						enable_memory: true,
						disable_stack: false,
						disable_storage: false,
						enable_return_data: true,
						disable_syscall_details: false,
						limit: None,
						memory_word_limit: 16,
					})),
					timeout: None,
				},
			),
			(
				r#"{  }"#,
				TracerConfig {
					config: TracerTypeV1::ExecutionTracer(Some(ExecutionTracerConfigV1::default())),
					timeout: None,
				},
			),
			(
				r#"{"tracer": "callTracer"}"#,
				TracerConfig { config: TracerTypeV1::CallTracer(None), timeout: None },
			),
			(
				r#"{"tracer": "callTracer", "tracerConfig": { "withLogs": false }}"#,
				TracerConfig {
					config: Some(CallTracerConfigV1 { with_logs: false, only_top_call: false })
						.into(),
					timeout: None,
				},
			),
			(
				r#"{"tracer": "callTracer", "tracerConfig": { "onlyTopCall": true }}"#,
				TracerConfig {
					config: Some(CallTracerConfigV1 { with_logs: true, only_top_call: true })
						.into(),
					timeout: None,
				},
			),
			(
				r#"{"tracer": "callTracer", "tracerConfig": { "onlyTopCall": true }, "timeout":
		"10ms"}"#,
				TracerConfig {
					config: Some(CallTracerConfigV1 { with_logs: true, only_top_call: true })
						.into(),
					timeout: Some(core::time::Duration::from_millis(10)),
				},
			),
			(
				r#"{"tracer": "executionTracer"}"#,
				TracerConfig { config: TracerTypeV1::ExecutionTracer(None), timeout: None },
			),
			(
				r#"{"tracer": "executionTracer", "tracerConfig": { "enableMemory": true }}"#,
				TracerConfig {
					config: Some(ExecutionTracerConfigV1 {
						enable_memory: true,
						..Default::default()
					})
					.into(),
					timeout: None,
				},
			),
			(
				r#"{ "enableMemory": true }"#,
				TracerConfig {
					config: Some(ExecutionTracerConfigV1 {
						enable_memory: true,
						..Default::default()
					})
					.into(),
					timeout: None,
				},
			),
		];

		for (json_data, expected) in tracers {
			let result: TracerConfig =
				serde_json::from_str(json_data).expect("Deserialization should succeed");
			assert_eq!(result, expected, "invalid serialization for {json_data}");
		}
	}
}
