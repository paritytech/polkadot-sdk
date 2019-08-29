// Copyright 2017-2019 Parity Technologies (UK) Ltd.
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

//! System RPC module errors.

use crate::system::helpers::Health;
use jsonrpc_core as rpc;

/// System RPC Result type.
pub type Result<T> = std::result::Result<T, Error>;

/// System RPC errors.
#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum Error {
	/// Provided block range couldn't be resolved to a list of blocks.
	#[display(fmt = "Node is not fully functional: {}", _0)]
	NotHealthy(Health),
}

impl std::error::Error for Error {}

/// Base code for all system errors.
const BASE_ERROR: i64 = 2000;

impl From<Error> for rpc::Error {
	fn from(e: Error) -> Self {
		match e {
			Error::NotHealthy(ref h) => rpc::Error {
				code: rpc::ErrorCode::ServerError(BASE_ERROR + 1),
				message: format!("{}", e),
				data: serde_json::to_value(h).ok(),
			},
		}
	}
}
