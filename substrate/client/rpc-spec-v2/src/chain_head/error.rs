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

//! Error helpers for `chainHead` RPC module.

use jsonrpsee::{
	core::Error as RpcError,
	types::error::{CallError, ErrorObject},
};
use sp_blockchain::Error as BlockchainError;

/// ChainHead RPC errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
	/// The provided block hash is invalid.
	#[error("Invalid block hash")]
	InvalidBlock,
	/// The follow subscription was started with `withRuntime` set to `false`.
	#[error("The `chainHead_follow` subscription was started with `withRuntime` set to `false`")]
	InvalidRuntimeCall(String),
	/// Wait-for-continue event not generated.
	#[error("Wait for continue event was not generated for the subscription")]
	InvalidContinue,
	/// Invalid parameter provided to the RPC method.
	#[error("Invalid parameter: {0}")]
	InvalidParam(String),
	/// Fetch block header error.
	#[error("Could not fetch block header: {0}")]
	FetchBlockHeader(BlockchainError),
	/// Invalid subscription ID provided by the RPC server.
	#[error("Invalid subscription ID")]
	InvalidSubscriptionID,
}

/// Errors for `chainHead` RPC module, as defined in
/// https://github.com/paritytech/json-rpc-interface-spec.
mod rpc_spec_v2 {
	/// The provided block hash is invalid.
	pub const INVALID_BLOCK_ERROR: i32 = -32801;
	/// The follow subscription was started with `withRuntime` set to `false`.
	pub const INVALID_RUNTIME_CALL: i32 = -32802;
	/// Wait-for-continue event not generated.
	pub const INVALID_CONTINUE: i32 = -32803;
}

/// General purpose errors, as defined in
/// https://www.jsonrpc.org/specification#error_object.
mod json_rpc_spec {
	/// Invalid parameter error.
	pub const INVALID_PARAM_ERROR: i32 = -32602;
	/// Fetch block header error.
	pub const FETCH_BLOCK_HEADER_ERROR: i32 = -32603;
	/// Invalid subscription ID.
	pub const INVALID_SUB_ID: i32 = -32603;
}

impl From<Error> for ErrorObject<'static> {
	fn from(e: Error) -> Self {
		let msg = e.to_string();

		match e {
			Error::InvalidBlock =>
				ErrorObject::owned(rpc_spec_v2::INVALID_BLOCK_ERROR, msg, None::<()>),
			Error::InvalidRuntimeCall(_) =>
				ErrorObject::owned(rpc_spec_v2::INVALID_RUNTIME_CALL, msg, None::<()>),
			Error::InvalidContinue =>
				ErrorObject::owned(rpc_spec_v2::INVALID_CONTINUE, msg, None::<()>),
			Error::FetchBlockHeader(_) =>
				ErrorObject::owned(json_rpc_spec::FETCH_BLOCK_HEADER_ERROR, msg, None::<()>),
			Error::InvalidParam(_) =>
				ErrorObject::owned(json_rpc_spec::INVALID_PARAM_ERROR, msg, None::<()>),
			Error::InvalidSubscriptionID =>
				ErrorObject::owned(json_rpc_spec::INVALID_SUB_ID, msg, None::<()>),
		}
	}
}

impl From<Error> for RpcError {
	fn from(e: Error) -> Self {
		CallError::Custom(e.into()).into()
	}
}
