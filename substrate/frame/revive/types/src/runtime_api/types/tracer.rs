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
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

use crate::common::*;

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
