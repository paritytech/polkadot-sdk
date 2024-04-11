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

//! Initialization errors.

use std::path::PathBuf;

use sp_core::crypto;

/// Result type alias for the CLI.
pub type Result<T> = std::result::Result<T, Error>;

/// Error type for the CLI.
#[derive(Debug, thiserror::Error)]
#[allow(missing_docs)]
pub enum Error {
	#[error(transparent)]
	Io(#[from] std::io::Error),

	#[error(transparent)]
	Cli(#[from] clap::Error),

	#[error(transparent)]
	Service(#[from] sc_service::Error),

	#[error(transparent)]
	Client(#[from] sp_blockchain::Error),

	#[error(transparent)]
	Codec(#[from] parity_scale_codec::Error),

	#[error("Invalid input: {0}")]
	Input(String),

	#[error("Invalid listen multiaddress")]
	InvalidListenMultiaddress,

	#[error("Invalid URI; expecting either a secret URI or a public URI.")]
	InvalidUri(crypto::PublicError),

	#[error("Signature is an invalid format.")]
	SignatureFormatInvalid,

	#[error("Key is an invalid format.")]
	KeyFormatInvalid,

	#[error("Unknown key type, must be a known 4-character sequence")]
	KeyTypeInvalid,

	#[error("Signature verification failed")]
	SignatureInvalid,

	#[error("Key store operation failed")]
	KeystoreOperation,

	#[error("Key storage issue encountered")]
	KeyStorage(#[from] sc_keystore::Error),

	#[error("Invalid hexadecimal string data, {0:?}")]
	HexDataConversion(array_bytes::Error),

	/// Application specific error chain sequence forwarder.
	#[error(transparent)]
	Application(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),

	#[error(transparent)]
	GlobalLoggerError(#[from] sc_tracing::logging::Error),

	#[error(
		"Starting an authorithy without network key in {0}.
		\n This is not a safe operation because other authorities in the network may depend on your node having a stable identity.
		\n Otherwise these other authorities may not being able to reach you.
		\n If it is the first time running your node you could use one of the following methods:
		\n 1. [Preferred] Separately generate the key with: <NODE_BINARY> key generate-node-key --base-path <YOUR_BASE_PATH>
		\n 2. [Preferred] Separately generate the key with: <NODE_BINARY> key generate-node-key --file <YOUR_PATH_TO_NODE_KEY>
		\n 3. [Preferred] Separately generate the key with: <NODE_BINARY> key generate-node-key --default-base-path
		\n 4. [Unsafe] Pass --unsafe-force-node-key-generation and make sure you remove it for subsequent node restarts"
	)]
	NetworkKeyNotFound(PathBuf),
	#[error("A network key already exists in path {0}")]
	KeyAlreadyExistsInPath(PathBuf),
}

impl From<&str> for Error {
	fn from(s: &str) -> Error {
		Error::Input(s.to_string())
	}
}

impl From<String> for Error {
	fn from(s: String) -> Error {
		Error::Input(s)
	}
}

impl From<crypto::PublicError> for Error {
	fn from(e: crypto::PublicError) -> Error {
		Error::InvalidUri(e)
	}
}

impl From<array_bytes::Error> for Error {
	fn from(e: array_bytes::Error) -> Error {
		Error::HexDataConversion(e)
	}
}
