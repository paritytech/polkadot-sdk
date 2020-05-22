// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

#![allow(dead_code)]

use jsonrpsee::raw::client::RawClientError;
use jsonrpsee::transport::http::RequestError;
use serde_json;

type RpcHttpError = RawClientError<RequestError>;

/// Contains common errors that can occur when
/// interacting with a Substrate or Ethereum node
/// through RPC.
#[derive(Debug)]
pub enum RpcError {
	/// The arguments to the RPC method failed to serialize.
	Serialization(serde_json::Error),
	/// An error occured when interacting with an Ethereum node.
	Ethereum(EthereumNodeError),
	/// An error occured when interacting with a Substrate node.
	Substrate(SubstrateNodeError),
	/// An error that can occur when making an HTTP request to
	/// an JSON-RPC client.
	Request(RpcHttpError),
	/// The response from the client could not be SCALE decoded.
	Decoding(codec::Error),
}

impl From<serde_json::Error> for RpcError {
	fn from(err: serde_json::Error) -> Self {
		Self::Serialization(err)
	}
}

impl From<EthereumNodeError> for RpcError {
	fn from(err: EthereumNodeError) -> Self {
		Self::Ethereum(err)
	}
}

impl From<SubstrateNodeError> for RpcError {
	fn from(err: SubstrateNodeError) -> Self {
		Self::Substrate(err)
	}
}

impl From<RpcHttpError> for RpcError {
	fn from(err: RpcHttpError) -> Self {
		Self::Request(err)
	}
}

impl From<codec::Error> for RpcError {
	fn from(err: codec::Error) -> Self {
		Self::Decoding(err)
	}
}

/// Errors that can occur only when interacting with
/// an Ethereum node through RPC.
#[derive(Debug)]
pub enum EthereumNodeError {
	/// Failed to parse response.
	ResponseParseFailed(String),
	/// We have received a header with missing fields.
	IncompleteHeader,
	/// We have received a receipt missing a `gas_used` field.
	IncompleteReceipt,
	/// An invalid Substrate block number was received from
	/// an Ethereum node.
	InvalidSubstrateBlockNumber,
}

/// Errors that can occur only when interacting with
/// a Substrate node through RPC.
#[derive(Debug)]
pub enum SubstrateNodeError {
	/// Request start failed.
	StartRequestFailed(RequestError),
	/// Error serializing request.
	RequestSerialization(serde_json::Error),
	/// Failed to parse response.
	ResponseParseFailed,
}
