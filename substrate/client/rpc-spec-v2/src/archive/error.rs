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

//! Error helpers for `archive` RPC module.

use jsonrpsee::types::error::ErrorObject;

/// ChainHead RPC errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
	/// Invalid parameter provided to the RPC method.
	#[error("Invalid parameter: {0}")]
	InvalidParam(String),
	/// Runtime call failed.
	#[error("Runtime call: {0}")]
	RuntimeCall(String),
	/// Failed to fetch leaves.
	#[error("Failed to fetch leaves of the chain: {0}")]
	FetchLeaves(String),
}

// Base code for all `archive` errors.
const BASE_ERROR: i32 = 3000;
/// Invalid parameter error.
const INVALID_PARAM_ERROR: i32 = BASE_ERROR + 1;
/// Runtime call error.
const RUNTIME_CALL_ERROR: i32 = BASE_ERROR + 2;
/// Failed to fetch leaves.
const FETCH_LEAVES_ERROR: i32 = BASE_ERROR + 3;

impl From<Error> for ErrorObject<'static> {
	fn from(e: Error) -> Self {
		let msg = e.to_string();

		match e {
			Error::InvalidParam(_) => ErrorObject::owned(INVALID_PARAM_ERROR, msg, None::<()>),
			Error::RuntimeCall(_) => ErrorObject::owned(RUNTIME_CALL_ERROR, msg, None::<()>),
			Error::FetchLeaves(_) => ErrorObject::owned(FETCH_LEAVES_ERROR, msg, None::<()>),
		}
		.into()
	}
}
