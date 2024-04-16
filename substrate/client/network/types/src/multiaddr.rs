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

use litep2p::types::multiaddr::{
	Error as LiteP2pError, Iter as LiteP2pIter, Multiaddr as LiteP2pMultiaddr,
};
use std::{
	fmt::{self, Debug, Display},
	str::FromStr,
};

mod protocol;
pub use protocol::Protocol;

/// [`Multiaddr`] type used in Substrate. Converted to libp2p's `Multiaddr`
/// or litep2p's `Multiaddr` when passed to the corresponding network backend.

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct Multiaddr {
	multiaddr: LiteP2pMultiaddr,
}

impl Multiaddr {
	/// Adds an address component to the end of this multiaddr.
	pub fn push(&mut self, p: Protocol<'_>) {
		self.multiaddr.push(p.into())
	}

	/// Pops the last `Protocol` of this multiaddr, or `None` if the multiaddr is empty.
	pub fn pop<'a>(&mut self) -> Option<Protocol<'a>> {
		self.multiaddr.pop().map(Into::into)
	}

	/// Like [`Multiaddr::push`] but consumes `self`.
	pub fn with(self, p: Protocol<'_>) -> Self {
		self.multiaddr.with(p.into()).into()
	}
}

impl Display for Multiaddr {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		Display::fmt(&self.multiaddr, f)
	}
}

/// Remove an extra layer of nestedness by deferring to the wrapped value's [`Debug`].
impl Debug for Multiaddr {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		Debug::fmt(&self.multiaddr, f)
	}
}

impl AsRef<[u8]> for Multiaddr {
	fn as_ref(&self) -> &[u8] {
		self.multiaddr.as_ref()
	}
}

impl From<LiteP2pMultiaddr> for Multiaddr {
	fn from(multiaddr: LiteP2pMultiaddr) -> Self {
		Self { multiaddr }
	}
}

impl From<Multiaddr> for LiteP2pMultiaddr {
	fn from(multiaddr: Multiaddr) -> Self {
		multiaddr.multiaddr
	}
}

/// Error when parsing a [`Multiaddr`] from string.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
	/// Invalid multiaddress.
	#[error("invalid multiaddress")]
	InvalidMultiaddr,
	/// Invalid protocol specification.
	#[error("invalid protocol string")]
	InvalidProtocolString,
	/// Unknown protocol identifier.
	#[error("unknown protocol '{0}'")]
	UnknownProtocol(String),
	/// Other error emitted when parsing into the wrapped type.
	/// Never generated as of multiaddr-0.17.0.
	#[error("multiaddr parsing error: {0}")]
	Other(Box<dyn std::error::Error + Send + Sync>),
}

impl From<LiteP2pError> for ParseError {
	fn from(error: LiteP2pError) -> Self {
		match error {
			LiteP2pError::InvalidMultiaddr => ParseError::InvalidMultiaddr,
			LiteP2pError::InvalidProtocolString => ParseError::InvalidProtocolString,
			LiteP2pError::UnknownProtocolString(s) => ParseError::UnknownProtocol(s),
			error @ _ => ParseError::Other(Box::new(error)),
		}
	}
}

impl FromStr for Multiaddr {
	type Err = ParseError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let multiaddr = LiteP2pMultiaddr::from_str(s)?;
		Ok(Self { multiaddr })
	}
}

/// Iterator over `Multiaddr` [`Protocol`]s.
pub struct Iter<'a>(LiteP2pIter<'a>);

impl<'a> Iterator for Iter<'a> {
	type Item = Protocol<'a>;

	fn next(&mut self) -> Option<Self::Item> {
		self.0.next().map(Into::into)
	}
}
