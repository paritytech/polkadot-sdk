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
//!Types, and traits to integrate pallet-revive with EVM.
#![warn(missing_docs)]

mod api;
pub use api::*;
mod tracing;
pub use tracing::*;
mod gas_encoder;
pub use gas_encoder::*;
pub mod runtime;

use crate::alloc::{format, string::*};

/// Extract the revert message from a revert("msg") solidity statement.
pub fn extract_revert_message(exec_data: &[u8]) -> Option<String> {
	let error_selector = exec_data.get(0..4)?;

	match error_selector {
		// assert(false)
		[0x4E, 0x48, 0x7B, 0x71] => {
			let panic_code: u32 = U256::from_big_endian(exec_data.get(4..36)?).try_into().ok()?;

			// See https://docs.soliditylang.org/en/latest/control-structures.html#panic-via-assert-and-error-via-require
			let msg = match panic_code {
				0x00 => "generic panic",
				0x01 => "assert(false)",
				0x11 => "arithmetic underflow or overflow",
				0x12 => "division or modulo by zero",
				0x21 => "enum overflow",
				0x22 => "invalid encoded storage byte array accessed",
				0x31 => "out-of-bounds array access; popping on an empty array",
				0x32 => "out-of-bounds access of an array or bytesN",
				0x41 => "out of memory",
				0x51 => "uninitialized function",
				code => return Some(format!("execution reverted: unknown panic code: {code:#x}")),
			};

			Some(format!("execution reverted: {msg}"))
		},
		// revert(string)
		[0x08, 0xC3, 0x79, 0xA0] => {
			let decoded = ethabi::decode(&[ethabi::ParamKind::String], &exec_data[4..]).ok()?;
			if let Some(ethabi::Token::String(msg)) = decoded.first() {
				return Some(format!("execution reverted: {}", String::from_utf8_lossy(msg)))
			}
			Some("execution reverted".to_string())
		},
		_ => {
			log::debug!(target: crate::LOG_TARGET, "Unknown revert function selector: {error_selector:?}");
			Some("execution reverted".to_string())
		},
	}
}
