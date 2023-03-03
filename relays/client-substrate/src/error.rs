// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

//! Substrate node RPC errors.

use bp_polkadot_core::parachains::ParaId;
use jsonrpsee::core::Error as RpcError;
use relay_utils::MaybeConnectionError;
use sc_rpc_api::system::Health;
use sp_core::storage::StorageKey;
use sp_runtime::transaction_validity::TransactionValidityError;
use thiserror::Error;

/// Result type used by Substrate client.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur only when interacting with
/// a Substrate node through RPC.
#[derive(Error, Debug)]
pub enum Error {
	/// IO error.
	#[error("IO error: {0}")]
	Io(#[from] std::io::Error),
	/// An error that can occur when making a request to
	/// an JSON-RPC server.
	#[error("RPC error: {0}")]
	RpcError(#[from] RpcError),
	/// The response from the server could not be SCALE decoded.
	#[error("Response parse failed: {0}")]
	ResponseParseFailed(#[from] codec::Error),
	/// Account does not exist on the chain.
	#[error("Account does not exist on the chain.")]
	AccountDoesNotExist,
	/// Runtime storage is missing some mandatory value.
	#[error("Mandatory storage value is missing from the runtime storage.")]
	MissingMandatoryStorageValue,
	/// Required parachain head is not present at the relay chain.
	#[error("Parachain {0:?} head {1} is missing from the relay chain storage.")]
	MissingRequiredParachainHead(ParaId, u64),
	/// Failed to find finality proof for the given header.
	#[error("Failed to find finality proof for header {0}.")]
	FinalityProofNotFound(u64),
	/// The client we're connected to is not synced, so we can't rely on its state.
	#[error("Substrate client is not synced {0}.")]
	ClientNotSynced(Health),
	/// Failed to read best finalized header hash from given chain.
	#[error("Failed to read best finalized header hash of {chain}: {error:?}.")]
	FailedToReadBestFinalizedHeaderHash {
		/// Name of the chain where the error has happened.
		chain: String,
		/// Underlying error.
		error: Box<Error>,
	},
	/// Failed to read best finalized header from given chain.
	#[error("Failed to read best header of {chain}: {error:?}.")]
	FailedToReadBestHeader {
		/// Name of the chain where the error has happened.
		chain: String,
		/// Underlying error.
		error: Box<Error>,
	},
	/// Failed to read header by hash from given chain.
	#[error("Failed to read header {hash} of {chain}: {error:?}.")]
	FailedToReadHeaderByHash {
		/// Name of the chain where the error has happened.
		chain: String,
		/// Hash of the header we've tried to read.
		hash: String,
		/// Underlying error.
		error: Box<Error>,
	},
	/// Failed to execute runtime call at given chain.
	#[error("Failed to execute runtime call {method} at {chain}: {error:?}.")]
	ErrorExecutingRuntimeCall {
		/// Name of the chain where the error has happened.
		chain: String,
		/// Runtime method name.
		method: String,
		/// Underlying error.
		error: Box<Error>,
	},
	/// Failed to read sotrage value at given chain.
	#[error("Failed to read storage value {key:?} at {chain}: {error:?}.")]
	FailedToReadRuntimeStorageValue {
		/// Name of the chain where the error has happened.
		chain: String,
		/// Runtime storage key
		key: StorageKey,
		/// Underlying error.
		error: Box<Error>,
	},
	/// The bridge pallet is halted and all transactions will be rejected.
	#[error("Bridge pallet is halted.")]
	BridgePalletIsHalted,
	/// The bridge pallet is not yet initialized and all transactions will be rejected.
	#[error("Bridge pallet is not initialized.")]
	BridgePalletIsNotInitialized,
	/// There's no best head of the parachain at the `pallet-bridge-parachains` at the target side.
	#[error("No head of the ParaId({0}) at the bridge parachains pallet at {1}.")]
	NoParachainHeadAtTarget(u32, String),
	/// An error has happened when we have tried to parse storage proof.
	#[error("Error when parsing storage proof: {0:?}.")]
	StorageProofError(bp_runtime::StorageProofError),
	/// The Substrate transaction is invalid.
	#[error("Substrate transaction is invalid: {0:?}")]
	TransactionInvalid(#[from] TransactionValidityError),
	/// Custom logic error.
	#[error("{0}")]
	Custom(String),
}

impl From<tokio::task::JoinError> for Error {
	fn from(error: tokio::task::JoinError) -> Self {
		Error::Custom(format!("Failed to wait tokio task: {error}"))
	}
}

impl Error {
	/// Box the error.
	pub fn boxed(self) -> Box<Self> {
		Box::new(self)
	}
}

impl MaybeConnectionError for Error {
	fn is_connection_error(&self) -> bool {
		match *self {
			Error::RpcError(RpcError::Transport(_))
				// right now if connection to the ws server is dropped (after it is already established),
				// we're getting this error
				| Error::RpcError(RpcError::Internal(_))
				| Error::RpcError(RpcError::RestartNeeded(_))
				| Error::ClientNotSynced(_) => true,
			Error::FailedToReadBestFinalizedHeaderHash { ref error, .. } => error.is_connection_error(),
			Error::FailedToReadBestHeader { ref error, .. } => error.is_connection_error(),
			Error::FailedToReadHeaderByHash { ref error, .. } => error.is_connection_error(),
			Error::ErrorExecutingRuntimeCall { ref error, .. } => error.is_connection_error(),
			Error::FailedToReadRuntimeStorageValue { ref error, .. } => error.is_connection_error(),
			_ => false,
		}
	}
}
