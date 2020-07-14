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

use crate::ethereum_types::{EthereumHeaderId, TransactionHash as EthereumTransactionHash};
use crate::sync_types::MaybeConnectionError;

use jsonrpsee::client::RequestError;
use serde_json;

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
	Request(RequestError),
}

impl From<RpcError> for String {
	fn from(err: RpcError) -> Self {
		match err {
			RpcError::Serialization(e) => e.to_string(),
			RpcError::Ethereum(e) => e.to_string(),
			RpcError::Substrate(e) => e.to_string(),
			RpcError::Request(e) => e.to_string(),
		}
	}
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

impl From<RequestError> for RpcError {
	fn from(err: RequestError) -> Self {
		Self::Request(err)
	}
}

impl From<ethabi::Error> for RpcError {
	fn from(err: ethabi::Error) -> Self {
		Self::Ethereum(EthereumNodeError::ResponseParseFailed(format!("{}", err)))
	}
}

impl MaybeConnectionError for RpcError {
	fn is_connection_error(&self) -> bool {
		match *self {
			RpcError::Request(RequestError::TransportError(_)) => true,
			_ => false,
		}
	}
}

impl From<codec::Error> for RpcError {
	fn from(err: codec::Error) -> Self {
		Self::Substrate(SubstrateNodeError::Decoding(err))
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
	/// We have received a transaction missing a `raw` field.
	IncompleteTransaction,
	/// An invalid Substrate block number was received from
	/// an Ethereum node.
	InvalidSubstrateBlockNumber,
	/// Block includes the same transaction more than once.
	DuplicateBlockTransaction(EthereumHeaderId, EthereumTransactionHash),
	/// Block is missing transaction we believe is a part of this block.
	BlockMissingTransaction(EthereumHeaderId, EthereumTransactionHash),
}

impl ToString for EthereumNodeError {
	fn to_string(&self) -> String {
		match self {
			Self::ResponseParseFailed(e) => e.to_string(),
			Self::IncompleteHeader => {
				"Incomplete Ethereum Header Received (missing some of required fields - hash, number, logs_bloom)"
					.to_string()
			}
			Self::IncompleteReceipt => {
				"Incomplete Ethereum Receipt Recieved (missing required field - gas_used)".to_string()
			}
			Self::IncompleteTransaction => "Incomplete Ethereum Transaction (missing required field - raw)".to_string(),
			Self::InvalidSubstrateBlockNumber => "Received an invalid Substrate block from Ethereum Node".to_string(),
			Self::DuplicateBlockTransaction(header_id, tx_hash) => format!(
				"Ethereum block {}/{} includes Ethereum transaction {} more than once",
				header_id.0, header_id.1, tx_hash,
			),
			Self::BlockMissingTransaction(header_id, tx_hash) => format!(
				"Ethereum block {}/{} is missing Ethereum transaction {} which we believe is a part of this block",
				header_id.0, header_id.1, tx_hash,
			),
		}
	}
}

/// Errors that can occur only when interacting with
/// a Substrate node through RPC.
#[derive(Debug)]
pub enum SubstrateNodeError {
	/// The response from the client could not be SCALE decoded.
	Decoding(codec::Error),
}

impl ToString for SubstrateNodeError {
	fn to_string(&self) -> String {
		match self {
			Self::Decoding(e) => e.what().to_string(),
		}
	}
}
