// Copyright 2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Offchain RPC errors.

use jsonrpc_core as rpc;

/// Offchain RPC Result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Offchain RPC errors.
#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum Error {
	/// Unavailable storage kind error.
	#[display(fmt="This storage kind is not available yet.")]
	UnavailableStorageKind,
}

impl std::error::Error for Error {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		None
	}
}

/// Base error code for all offchain errors.
const BASE_ERROR: i64 = 5000;

impl From<Error> for rpc::Error {
	fn from(e: Error) -> Self {
		match e {
			Error::UnavailableStorageKind => rpc::Error {
				code: rpc::ErrorCode::ServerError(BASE_ERROR + 1),
				message: "This storage kind is not available yet" .into(),
				data: None,
			},
		}
	}
}
