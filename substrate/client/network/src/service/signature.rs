// This file is part of Substrate.
//
// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.
//
// If you read this, you are very thorough, congratulations.

//! Signature-related code

pub use libp2p::identity::SigningError;

/// Public key.
pub enum PublicKey {
	/// Litep2p public key.
	Libp2p(libp2p::identity::PublicKey),

	/// Libp2p public key.
	Litep2p(litep2p::crypto::PublicKey),
}

impl PublicKey {
	/// Protobuf-encode [`PublicKey`].
	pub fn encode_protobuf(&self) -> Vec<u8> {
		match self {
			Self::Libp2p(public) => public.encode_protobuf(),
			Self::Litep2p(public) => public.to_protobuf_encoding(),
		}
	}
}

/// A result of signing a message with a network identity. Since `PeerId` is potentially a hash of a
/// `PublicKey`, you need to reveal the `PublicKey` next to the signature, so the verifier can check
/// if the signature was made by the entity that controls a given `PeerId`.
pub struct Signature {
	/// The public key derived from the network identity that signed the message.
	pub public_key: PublicKey,

	/// The actual signature made for the message signed.
	pub bytes: Vec<u8>,
}

impl Signature {
	/// Create new [`Signature`].
	pub fn new(public_key: PublicKey, bytes: Vec<u8>) -> Self {
		Self { public_key, bytes }
	}

	/// Create a signature for a message with a given network identity.
	pub fn sign_message(
		message: impl AsRef<[u8]>,
		keypair: &libp2p::identity::Keypair,
	) -> Result<Self, SigningError> {
		let public_key = keypair.public();
		let bytes = keypair.sign(message.as_ref())?;

		Ok(Signature { public_key: PublicKey::Libp2p(public_key), bytes })
	}
}
