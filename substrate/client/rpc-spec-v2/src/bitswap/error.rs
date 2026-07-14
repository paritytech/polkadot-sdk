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
	#[error("Node is major syncing")]
	MajorSyncing,
	/// Internal error. Never emitted in practice.
	///
	/// Do not render the wrapped error to not expose the internal state to the remote caller.
	#[error("Internal error")]
	Internal(#[from] sp_blockchain::Error),
	/// Caller passed more CIDs than `bitswap_unstable_stream` will accept.
	#[error("Too many CIDs: max {max}, got {got}")]
	TooManyCids {
		/// Maximum number of CIDs accepted in a single request.
		max: usize,
		/// Number of CIDs the caller passed.
		got: usize,
	},
	/// Caller passed an empty `cids` array.
	#[error("Input cids array is empty")]
	EmptyCids,
	/// Caller passed the same CID twice (string-equal or decoding to the same digest).
	#[error("Input contains duplicate CIDs")]
	DuplicateCids,
}

/// Bitswap JSON-RPC error categories, according to the spec.
///
/// Note: `-32811 FailRetry` is part of the per-CID error matrix in the spec but is not emitted by
/// this implementation, which only distinguishes permanent failure (`Fail`) and backoff-eligible
/// transient failure (`FailRetryBackoff`).
#[derive(Debug)]
enum ErrorCode {
	/// Standard JSON-RPC invalid-params. Used per-CID for malformed/unsupported CIDs.
	InvalidParams = -32602,
	/// Top-level: `cids` length exceeds implementation maximum.
	TooManyCids = -32801,
	/// Top-level: `cids` array is empty.
	EmptyCids = -32802,
	/// Top-level: `cids` contains duplicates (string-equal or digest-equal).
	DuplicateCids = -32803,
	/// Per-CID permanent failure (e.g. data not found). Must not retry.
	Fail = -32810,
	/// Per-CID transient failure; can retry with a backoff of 1-5 seconds.
	FailRetryBackoff = -32812,
}

impl From<Error> for ErrorObject<'static> {
	fn from(e: Error) -> Self {
		let msg = e.to_string();
		let code = match e {
			Error::InvalidCid(_) => ErrorCode::InvalidParams,
			Error::NotFound => ErrorCode::Fail,
			Error::MajorSyncing => ErrorCode::FailRetryBackoff,
			// This error is never emitted in practice and is only needed to cover all
			// compile-time variants that `BlockBackend::indexed_transaction` returns.
			// It is unclear what error category to use in case of internal errors, let's use
			// `FailRetryBackoff`.
			Error::Internal(_) => ErrorCode::FailRetryBackoff,
			Error::TooManyCids { .. } => ErrorCode::TooManyCids,
			Error::EmptyCids => ErrorCode::EmptyCids,
			Error::DuplicateCids => ErrorCode::DuplicateCids,
		};
		ErrorObject::owned(code as i32, msg, None::<()>)
	}
}
