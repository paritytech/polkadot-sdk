// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Substrate JSON-RPC interface v2.
//!
//! Specification [document](https://paritytech.github.io/json-rpc-interface-spec/).

#![warn(missing_docs)]
#![deny(unused_crate_dependencies)]

use serde::{Deserialize, Serialize};
use sp_core::hexdisplay::{AsBytesRef, HexDisplay};

mod common;

pub mod archive;
pub mod chain_head;
pub mod chain_spec;
pub mod transaction;

/// Task executor that is being used by RPC subscriptions.
pub type SubscriptionTaskExecutor = std::sync::Arc<dyn sp_core::traits::SpawnNamed>;

/// The result of an RPC method.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum MethodResult {
	/// Method generated a result.
	Ok(MethodResultOk),
	/// Method encountered an error.
	Err(MethodResultErr),
}

impl MethodResult {
	/// Constructs a successful result.
	pub fn ok(result: impl Into<String>) -> MethodResult {
		MethodResult::Ok(MethodResultOk { success: true, result: result.into() })
	}

	/// Constructs an error result.
	pub fn err(error: impl Into<String>) -> MethodResult {
		MethodResult::Err(MethodResultErr { success: false, error: error.into() })
	}
}

/// The successful result of an RPC method.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodResultOk {
	/// Method was successful.
	success: bool,
	/// The result of the method.
	pub result: String,
}

/// The error result of an RPC method.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodResultErr {
	/// Method encountered an error.
	success: bool,
	/// The error of the method.
	pub error: String,
}

/// Util function to encode a value as a hex string
pub fn hex_string<Data: AsBytesRef>(data: &Data) -> String {
	format!("0x{:?}", HexDisplay::from(data))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn method_result_ok() {
		let ok = MethodResult::ok("hello");

		let ser = serde_json::to_string(&ok).unwrap();
		let exp = r#"{"success":true,"result":"hello"}"#;
		assert_eq!(ser, exp);

		let ok_dec: MethodResult = serde_json::from_str(exp).unwrap();
		assert_eq!(ok_dec, ok);
	}

	#[test]
	fn method_result_error() {
		let ok = MethodResult::err("hello");

		let ser = serde_json::to_string(&ok).unwrap();
		let exp = r#"{"success":false,"error":"hello"}"#;
		assert_eq!(ser, exp);

		let ok_dec: MethodResult = serde_json::from_str(exp).unwrap();
		assert_eq!(ok_dec, ok);
	}
}
