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

//! [`Multihash`] implemenattion used by substrate. Currently it's a wrapper over
//! multihash used by litep2p, but it can be switched to other implementation if needed.

use litep2p::types::multihash::{
	Code as LiteP2pCode, Error as LiteP2pError, Multihash as LiteP2pMultihash,
	MultihashDigest as LiteP2pDigest,
};
use std::fmt::{self, Debug};

/// Default [`Multihash`] implementations. Only hashes used by substrate are defined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
	/// Identity hasher.
	Identity,
	/// SHA-256 (32-byte hash size).
	Sha2_256,
}

impl Code {
	/// Calculate digest using this [`Code`]'s hashing algorithm.
	pub fn digest(&self, input: &[u8]) -> Multihash {
		LiteP2pCode::from(*self).digest(input).into()
	}
}

/// Error generated when converting to [`Code`].
#[derive(Debug, thiserror::Error)]
pub enum CodeError {
	/// The multihash code is not supported.
	#[error("unsupported multihash code '{0}'")]
	UsupportedCode(u64),
	/// Catch-all for other errors emitted when converting `u64` code to enum.
	/// Never generated as of multihash-0.17.0.
	#[error("multihash code conversion error: {0}")]
	Other(Box<dyn std::error::Error + Send + Sync>),
}

impl From<Code> for LiteP2pCode {
	fn from(code: Code) -> Self {
		match code {
			Code::Identity => LiteP2pCode::Identity,
			Code::Sha2_256 => LiteP2pCode::Sha2_256,
		}
	}
}

impl TryFrom<LiteP2pCode> for Code {
	type Error = CodeError;

	fn try_from(code: LiteP2pCode) -> Result<Self, Self::Error> {
		match code {
			LiteP2pCode::Identity => Ok(Code::Identity),
			LiteP2pCode::Sha2_256 => Ok(Code::Sha2_256),
			_ => Err(CodeError::UsupportedCode(code.into())),
		}
	}
}

impl TryFrom<u64> for Code {
	type Error = CodeError;

	fn try_from(code: u64) -> Result<Self, Self::Error> {
		match LiteP2pCode::try_from(code) {
			Ok(code) => code.try_into(),
			Err(LiteP2pError::UnsupportedCode(code)) => Err(CodeError::UsupportedCode(code)),
			Err(e) => Err(CodeError::Other(Box::new(e))),
		}
	}
}

impl From<Code> for u64 {
	fn from(code: Code) -> Self {
		LiteP2pCode::from(code).into()
	}
}

#[derive(Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub struct Multihash {
	multihash: LiteP2pMultihash,
}

impl Multihash {
	/// Multihash code.
	pub fn code(&self) -> u64 {
		self.multihash.code()
	}

	/// Multihash digest.
	pub fn digest(&self) -> &[u8] {
		self.multihash.digest()
	}
}

/// Remove extra layer of nestedness by deferring to the wrapped value's [`Debug`].
impl Debug for Multihash {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		Debug::fmt(&self.multihash, f)
	}
}

impl From<LiteP2pMultihash> for Multihash {
	fn from(multihash: LiteP2pMultihash) -> Self {
		Multihash { multihash }
	}
}

impl From<Multihash> for LiteP2pMultihash {
	fn from(multihash: Multihash) -> Self {
		multihash.multihash
	}
}

// TODO: uncomment this after upgrading `multihash` crate to v0.19.1.
//
// impl From<multihash::Multihash<64>> for Multihash {
// 	fn from(generic: multihash::MultihashGeneric<64>) -> Self {
// 		LiteP2pMultihash::wrap(generic.code(), generic.digest())
// 			.expect("both have size 64; qed")
// 			.into()
// 	}
// }
//
// impl From<Multihash> for multihash::Multihash<64> {
// 	fn from(multihash: Multihash) -> Self {
// 		multihash::Multihash::<64>::wrap(multihash.code(), multihash.digest())
// 			.expect("both have size 64; qed")
// 	}
// }

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn code_from_u64() {
		assert_eq!(Code::try_from(0x00).unwrap(), Code::Identity);
		assert_eq!(Code::try_from(0x12).unwrap(), Code::Sha2_256);
		assert!(matches!(Code::try_from(0x01).unwrap_err(), CodeError::UsupportedCode(0x01)));
	}

	#[test]
	fn code_into_u64() {
		assert_eq!(u64::from(Code::Identity), 0x00);
		assert_eq!(u64::from(Code::Sha2_256), 0x12);
	}
}
