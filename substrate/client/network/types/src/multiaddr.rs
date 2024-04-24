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

// Re-export the macro under shorter name under `multiaddr`.
pub use crate::build_multiaddr as multiaddr;

/// [`Multiaddr`] type used in Substrate. Converted to libp2p's `Multiaddr`
/// or litep2p's `Multiaddr` when passed to the corresponding network backend.

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct Multiaddr {
	multiaddr: LiteP2pMultiaddr,
}

impl Multiaddr {
	/// Create a new, empty multiaddress.
	pub fn empty() -> Self {
		Self { multiaddr: LiteP2pMultiaddr::empty() }
	}

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

	/// Returns the components of this multiaddress.
	pub fn iter(&self) -> Iter<'_> {
		self.multiaddr.iter().into()
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

impl<'a> From<LiteP2pIter<'a>> for Iter<'a> {
	fn from(iter: LiteP2pIter<'a>) -> Self {
		Self(iter)
	}
}

impl<'a> IntoIterator for &'a Multiaddr {
	type Item = Protocol<'a>;
	type IntoIter = Iter<'a>;

	fn into_iter(self) -> Iter<'a> {
		self.multiaddr.into_iter().into()
	}
}

impl<'a> FromIterator<Protocol<'a>> for Multiaddr {
	fn from_iter<T>(iter: T) -> Self
	where
		T: IntoIterator<Item = Protocol<'a>>,
	{
		LiteP2pMultiaddr::from_iter(iter.into_iter().map(Into::into)).into()
	}
}

/// Easy way for a user to create a `Multiaddr`.
///
/// Example:
///
/// ```rust
/// # use multiaddr::multiaddr;
/// let addr = build_multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10500u16));
/// ```
///
/// Each element passed to `multiaddr!` should be a variant of the `Protocol` enum. The
/// optional parameter is turned into the proper type with the `Into` trait.
///
/// For example, `Ip4([127, 0, 0, 1])` works because `Ipv4Addr` implements `From<[u8; 4]>`.
#[macro_export]
macro_rules! build_multiaddr {
    ($($comp:ident $(($param:expr))*),+) => {
        {
            use std::iter;
            let elem = iter::empty::<$crate::multiaddr::Protocol>();
            $(
                let elem = {
                    let cmp = $crate::multiaddr::Protocol::$comp $(( $param.into() ))*;
                    elem.chain(iter::once(cmp))
                };
            )+
            elem.collect::<$crate::multiaddr::Multiaddr>()
        }
    }
}
