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

use codec::{Decode, Encode};
use derive_more::From;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

use crate::common::*;

#[derive(TypeInfo, Debug, Clone, Encode, Decode, Serialize, Deserialize, PartialEq, From)]
#[serde(tag = "tracer", content = "tracerConfig", rename_all = "camelCase")]
pub enum TracerTypeV1 {
	CallTracer(Option<CallTracerConfigV1>),
	PrestateTracer(Option<PrestateTracerConfigV1>),
	ExecutionTracer(Option<ExecutionTracerConfigV1>),
}

impl Default for TracerTypeV1 {
	fn default() -> Self {
		TracerTypeV1::ExecutionTracer(Some(ExecutionTracerConfigV1::default()))
	}
}

/// As [`TracerTypeV1`], but the execution tracer takes [`ExecutionTracerConfigV2`].
#[derive(TypeInfo, Debug, Clone, Encode, Decode, Serialize, Deserialize, PartialEq, From)]
#[serde(tag = "tracer", content = "tracerConfig", rename_all = "camelCase")]
pub enum TracerTypeV2 {
	CallTracer(Option<CallTracerConfigV1>),
	PrestateTracer(Option<PrestateTracerConfigV1>),
	ExecutionTracer(Option<ExecutionTracerConfigV2>),
}

/// Widens a V1 selection into V2. V1 has no window, so the result captures from the first step.
impl From<TracerTypeV1> for TracerTypeV2 {
	fn from(value: TracerTypeV1) -> Self {
		match value {
			TracerTypeV1::CallTracer(config) => Self::CallTracer(config),
			TracerTypeV1::PrestateTracer(config) => Self::PrestateTracer(config),
			TracerTypeV1::ExecutionTracer(config) => Self::ExecutionTracer(config.map(Into::into)),
		}
	}
}

impl From<ExecutionTracerConfigV1> for ExecutionTracerConfigV2 {
	fn from(value: ExecutionTracerConfigV1) -> Self {
		Self {
			enable_memory: value.enable_memory,
			disable_stack: value.disable_stack,
			disable_storage: value.disable_storage,
			enable_return_data: value.enable_return_data,
			disable_syscall_details: value.disable_syscall_details,
			step_offset: 0,
			limit: value.limit,
			memory_word_limit: value.memory_word_limit,
		}
	}
}

impl Default for TracerTypeV2 {
	fn default() -> Self {
		TracerTypeV2::ExecutionTracer(Some(ExecutionTracerConfigV2::default()))
	}
}

#[derive(Clone, Debug, Decode, Serialize, Deserialize, Encode, PartialEq, TypeInfo)]
#[serde(default, rename_all = "camelCase")]
pub struct CallTracerConfigV1 {
	pub with_logs: bool,
	pub only_top_call: bool,
}

impl Default for CallTracerConfigV1 {
	fn default() -> Self {
		Self { with_logs: true, only_top_call: false }
	}
}

#[derive(Clone, Debug, Decode, Serialize, Deserialize, Encode, PartialEq, TypeInfo, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct PrestateTracerConfigV1 {
	pub diff_mode: bool,
	pub disable_storage: bool,
	pub disable_code: bool,
}

#[derive(Clone, Debug, Decode, Serialize, Deserialize, Encode, PartialEq, TypeInfo)]
#[serde(default, rename_all = "camelCase")]
pub struct ExecutionTracerConfigV1 {
	pub enable_memory: bool,
	pub disable_stack: bool,
	pub disable_storage: bool,
	pub enable_return_data: bool,
	pub disable_syscall_details: bool,
	#[serde(skip_serializing_if = "Option::is_none", deserialize_with = "zero_to_none")]
	pub limit: Option<u64>,
	pub memory_word_limit: u32,
}

impl Default for ExecutionTracerConfigV1 {
	fn default() -> Self {
		Self {
			enable_memory: false,
			disable_stack: false,
			disable_storage: false,
			enable_return_data: false,
			disable_syscall_details: false,
			limit: None,
			memory_word_limit: 16,
		}
	}
}

/// As [`ExecutionTracerConfigV1`], plus [`Self::step_offset`].
#[derive(Clone, Debug, Decode, Serialize, Deserialize, Encode, PartialEq, TypeInfo)]
#[serde(default, rename_all = "camelCase")]
pub struct ExecutionTracerConfigV2 {
	pub enable_memory: bool,
	pub disable_stack: bool,
	pub disable_storage: bool,
	pub enable_return_data: bool,
	pub disable_syscall_details: bool,
	pub step_offset: u64,
	#[serde(skip_serializing_if = "Option::is_none", deserialize_with = "zero_to_none")]
	pub limit: Option<u64>,
	pub memory_word_limit: u32,
}

impl Default for ExecutionTracerConfigV2 {
	fn default() -> Self {
		Self {
			enable_memory: false,
			disable_stack: false,
			disable_storage: false,
			enable_return_data: false,
			disable_syscall_details: false,
			step_offset: 0,
			limit: None,
			memory_word_limit: 16,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::runtime_api::*;
	use alloc::{format, vec};
	use alloy_core::hex;
	use core::fmt::Debug;

	/// `value` must encode to exactly `expected`, and `expected` must decode back to `value`.
	fn assert_pinned<T: Encode + Decode + PartialEq + Debug>(
		value: &T,
		expected: &str,
		what: &str,
	) {
		assert_eq!(hex::encode(value.encode()), expected, "the {what} changed shape");
		let bytes = hex::decode(expected).expect("the literal is hex");
		let decoded = T::decode(&mut &bytes[..]).expect("the pinned bytes decode");
		assert_eq!(&decoded, value, "the pinned {what} bytes decode to something else");
	}

	/// As [`assert_pinned`], for a versioned payload. The literal must open with the `V3`
	/// discriminant, so a literal regenerated after a variant reorder is rejected too.
	fn assert_pinned_v3<T: Encode + Decode + PartialEq + Debug>(
		value: &T,
		expected: &str,
		what: &str,
	) {
		assert!(expected.starts_with("02"), "bad version number on the {what}");
		assert_pinned(value, expected, what);
	}

	#[test]
	fn v3_trace_payloads_keep_their_wire_format() {
		// Legible little-endian: `step_offset` is the `0807060504030201` between
		// `disable_syscall_details` and `limit`; the leading `02` is `ExecutionTracer`.
		let config = TracerTypeV2::ExecutionTracer(Some(ExecutionTracerConfigV2 {
			enable_memory: false,
			disable_stack: true,
			disable_storage: true,
			enable_return_data: false,
			disable_syscall_details: false,
			step_offset: 0x0102_0304_0506_0708,
			limit: Some(0x1112_1314_1516_1718),
			memory_word_limit: 0x2122_2324,
		}));
		const TRACER: &str = "02010001010000080706050403020101181716151413121124232221";
		assert_pinned(&config, TRACER, "tracer config");

		assert_pinned_v3(
			&TraceTxVersionedInputPayload::<()>::V3(TraceTxInputPayloadV3 {
				block: (),
				tx_index: 0x3132_3334,
				config: config.clone(),
			}),
			&format!("0234333231{TRACER}"),
			"V3 trace_tx input",
		);
		assert_pinned_v3(
			&TraceBlockVersionedInputPayload::<()>::V3(TraceBlockInputPayloadV3 {
				block: (),
				config: config.clone(),
			}),
			&format!("02{TRACER}"),
			"V3 trace_block input",
		);

		let tx = GenericTransactionV1::default();
		assert_pinned_v3(
			&TraceCallVersionedInputPayload::V3(TraceCallInputPayloadV3 {
				tx: tx.clone(),
				config,
				state_overrides: None,
			}),
			&format!("02{}{TRACER}00", hex::encode(tx.encode())),
			"V3 trace_call input",
		);

		assert_pinned_v3(
			&TraceTxVersionedOutputPayload::V3(TraceTxOutputPayloadV3 { entry: None }),
			"0200",
			"V3 trace_tx output",
		);
		assert_pinned_v3(
			&TraceBlockVersionedOutputPayload::V3(TraceBlockOutputPayloadV3 { entries: vec![] }),
			"0200",
			"V3 trace_block output",
		);
		let trace = ExecutionTraceV1::default();
		assert_pinned_v3(
			&TraceCallVersionedOutputPayload::V3(TraceCallOutputPayloadV3 {
				trace: TraceV2::Execution(trace.clone()),
			}),
			&format!("0202{}", hex::encode(trace.encode())),
			"V3 trace_call output",
		);
	}
}
