// Copyright 2020 Parity Technologies (UK) Ltd.
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
//

//! Helpers related to signatures.
//!
//! Used for testing and benchmarking.

use crate::{public_to_address, rlp_encode, step_validator, Address, Header, H256, H520};

use secp256k1::{Message, PublicKey, SecretKey};

/// Utilities for signing headers.
pub trait SignHeader {
	/// Signs header by given author.
	fn sign_by(self, author: &SecretKey) -> Header;
	/// Signs header by given authors set.
	fn sign_by_set(self, authors: &[SecretKey]) -> Header;
}

impl SignHeader for Header {
	fn sign_by(mut self, author: &SecretKey) -> Self {
		self.author = secret_to_address(author);

		let message = self.seal_hash(false).unwrap();
		let signature = sign(author, message);
		self.seal[1] = rlp_encode(&signature);
		self
	}

	fn sign_by_set(self, authors: &[SecretKey]) -> Self {
		let step = self.step().unwrap();
		let author = step_validator(authors, step);
		self.sign_by(author)
	}
}

/// Return author's signature over given message.
pub fn sign(author: &SecretKey, message: H256) -> H520 {
	let (signature, recovery_id) = secp256k1::sign(&Message::parse(message.as_fixed_bytes()), author);
	let mut raw_signature = [0u8; 65];
	raw_signature[..64].copy_from_slice(&signature.serialize());
	raw_signature[64] = recovery_id.serialize();
	raw_signature.into()
}

/// Returns address corresponding to given secret key.
pub fn secret_to_address(secret: &SecretKey) -> Address {
	let public = PublicKey::from_secret_key(secret);
	let mut raw_public = [0u8; 64];
	raw_public.copy_from_slice(&public.serialize()[1..]);
	public_to_address(&raw_public)
}
