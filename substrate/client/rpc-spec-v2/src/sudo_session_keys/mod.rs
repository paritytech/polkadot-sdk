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

//! Substrate sudo sessions key API.

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

pub mod api;
pub mod sudo_session_keys;

/// The result of an RPC method.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum MethodResult {
	/// Method generated a result.
	Ok(MethodResultOk),
	/// Method ecountered an error.
	Err(MethodResultErr),
}

/// The succesful result of an RPC method.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodResultOk {
	/// The result of the method.
	pub result: String,
}

/// The error result of an RPC method.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodResultErr {
	/// The error of the method.
	pub error: String,
}
