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

//! Error helpers for the `bitswap` RPC module.

use jsonrpsee::types::error::ErrorObject;

/// Bitswap RPC errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
	/// Invalid CID parameter.
	#[error("Invalid CID: {0}")]
	InvalidCid(String),
	/// Transaction not found.
	#[error("Transaction not found")]
	NotFound,
	/// Node is performing major sync.
	#[error("Node is performing major sync")]
	MajorSyncing,
	/// Internal error.
	#[error("Internal error: {0}")]
	Internal(String),
}

/// Invalid params error code (standard JSON-RPC).
const INVALID_PARAMS: i32 = -32602;
/// Fail error code.
const FAIL: i32 = -32810;
/// Fail with retry backoff error code.
const FAIL_RETRY_BACKOFF: i32 = -32812;

#[derive(serde::Serialize)]
struct ErrorData {
	variant: &'static str,
}

impl From<Error> for ErrorObject<'static> {
	fn from(e: Error) -> Self {
		let msg = e.to_string();

		match e {
			Error::InvalidCid(_) => ErrorObject::owned(INVALID_PARAMS, msg, None::<()>),
			Error::NotFound =>
				ErrorObject::owned(FAIL, msg, Some(ErrorData { variant: "NotFound" })),
			Error::MajorSyncing => ErrorObject::owned(
				FAIL_RETRY_BACKOFF,
				msg,
				Some(ErrorData { variant: "MajorSyncing" }),
			),
			Error::Internal(_) =>
				ErrorObject::owned(FAIL, msg, Some(ErrorData { variant: "Internal" })),
		}
	}
}
