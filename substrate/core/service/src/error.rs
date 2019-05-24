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

//! Errors that can occur during the service operation.

use client;
use network;
use keystore;
use consensus_common;

/// Service Result typedef.
pub type Result<T> = std::result::Result<T, Error>;

/// Service errors.
#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum Error {
	/// Client error.
	Client(client::error::Error),
	/// IO error.
	Io(std::io::Error),
	/// Consensus error.
	Consensus(consensus_common::Error),
	/// Network error.
	Network(network::error::Error),
	/// Keystore error.
	Keystore(keystore::Error),
	/// Best chain selection strategy is missing.
	#[display(fmt="Best chain selection strategy (SelectChain) is not provided.")]
	SelectChainRequired,
	/// Other error.
	Other(String),
}

impl<'a> From<&'a str> for Error {
	fn from(s: &'a str) -> Self {
		Error::Other(s.into())
	}
}

impl std::error::Error for Error {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			Error::Client(ref err) => Some(err),
			Error::Io(ref err) => Some(err),
			Error::Consensus(ref err) => Some(err),
			Error::Network(ref err) => Some(err),
			Error::Keystore(ref err) => Some(err),
			_ => None,
		}
	}
}
