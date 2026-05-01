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

use codec::{Decode, Encode, MaxEncodedLen};
use pallet_revive_proc_macro::define_versioned_type;
use scale_info::TypeInfo;
use serde::{Deserialize, Deserializer, Serialize};

use crate::runtime_api::StateOverrideSetV1;

define_versioned_type! {
	/// Version 1 of the tracer selection used by tracing runtime APIs.
	#[derive(
		Debug,
		Clone,
		Serialize,
		Deserialize,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		MaxEncodedLen,
	)]
	#[serde(tag = "tracer", content = "tracerConfig", rename_all = "camelCase")]
	pub enum TracerTypeV1 {
		/// Collects nested call traces.
		CallTracer(Option<CallTracerConfigV1>),
		/// Collects account prestate or state-diff traces.
		PrestateTracer(Option<PrestateTracerConfigV1>),
		/// Collects opcode and syscall execution steps.
		ExecutionTracer(Option<ExecutionTracerConfigV1>),
	}
}

impl Default for TracerTypeV1 {
	fn default() -> Self {
		Self::ExecutionTracer(Some(ExecutionTracerConfigV1::default()))
	}
}

define_versioned_type! {
	/// Version 1 of call tracer configuration.
	#[derive(
		Debug,
		Clone,
		Serialize,
		Deserialize,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		MaxEncodedLen,
	)]
	#[serde(default, rename_all = "camelCase")]
	pub struct CallTracerConfigV1 {
		/// Whether emitted logs are included in call traces.
		pub with_logs: bool,
		/// Whether only the top-level call is included in the trace.
		pub only_top_call: bool,
	}
}

impl Default for CallTracerConfigV1 {
	fn default() -> Self {
		Self { with_logs: true, only_top_call: false }
	}
}

define_versioned_type! {
	/// Version 1 of prestate tracer configuration.
	#[derive(
		Debug,
		Default,
		Clone,
		Serialize,
		Deserialize,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		MaxEncodedLen,
	)]
	#[serde(default, rename_all = "camelCase")]
	pub struct PrestateTracerConfigV1 {
		/// Whether the tracer returns state differences instead of full prestate.
		pub diff_mode: bool,
		/// Whether account storage is omitted from the trace.
		pub disable_storage: bool,
		/// Whether account code is omitted from the trace.
		pub disable_code: bool,
	}
}

define_versioned_type! {
	/// Version 1 of execution tracer configuration.
	#[derive(
		Debug,
		Clone,
		Serialize,
		Deserialize,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		MaxEncodedLen,
	)]
	#[serde(default, rename_all = "camelCase")]
	pub struct ExecutionTracerConfigV1 {
		/// Whether EVM memory capture is enabled.
		pub enable_memory: bool,
		/// Whether EVM stack capture is disabled.
		pub disable_stack: bool,
		/// Whether EVM storage capture is disabled.
		pub disable_storage: bool,
		/// Whether return data capture is enabled.
		pub enable_return_data: bool,
		/// Whether PVM syscall arguments and return values are omitted.
		pub disable_syscall_details: bool,
		/// Maximum number of execution steps to capture.
		#[serde(skip_serializing_if = "Option::is_none", deserialize_with = "zero_to_none")]
		pub limit: Option<u64>,
		/// Maximum number of memory words to capture per step.
		pub memory_word_limit: u32,
	}
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

define_versioned_type! {
	/// Version 1 of tracing execution configuration.
	#[derive(
		Debug,
		Default,
		Clone,
		Serialize,
		Deserialize,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
	)]
	#[serde(default, rename_all = "camelCase")]
	pub struct TracingConfigV1 {
		/// Optional state overrides to apply before executing the traced call.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub state_overrides: Option<StateOverrideSetV1>,
	}
}

/// Deserializes zero execution-step limits as an absent limit.
fn zero_to_none<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
	D: Deserializer<'de>,
{
	let value = Option::<u64>::deserialize(deserializer)?;
	Ok(match value {
		Some(0) => None,
		other => other,
	})
}
