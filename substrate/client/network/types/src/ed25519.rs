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

//! Ed25519 keys.

use crate::PeerId;
use core::{cmp, fmt, hash};
use ed25519_dalek::{self as ed25519, Signer as _, Verifier as _};
use libp2p_identity::ed25519 as libp2p_ed25519;
use litep2p::crypto::ed25519 as litep2p_ed25519;
use zeroize::Zeroize;

/// An Ed25519 keypair.
#[derive(Clone)]
pub struct Keypair(ed25519::SigningKey);

impl Keypair {
	/// Generate a new random Ed25519 keypair.
	pub fn generate() -> Keypair {
		Keypair::from(SecretKey::generate())
	}

	/// Convert the keypair into a byte array by concatenating the bytes
	/// of the secret scalar and the compressed public point,
	/// an informal standard for encoding Ed25519 keypairs.
	pub fn to_bytes(&self) -> [u8; 64] {
		self.0.to_keypair_bytes()
	}

	/// Try to parse a keypair from the [binary format](https://datatracker.ietf.org/doc/html/rfc8032#section-5.1.5)
	/// produced by [`Keypair::to_bytes`], zeroing the input on success.
	///
	/// Note that this binary format is the same as `ed25519_dalek`'s and `ed25519_zebra`'s.
	pub fn try_from_bytes(kp: &mut [u8]) -> Result<Keypair, DecodingError> {
		let bytes = <[u8; 64]>::try_from(&*kp)
			.map_err(|e| DecodingError::KeypairParseError(Box::new(e)))?;

		ed25519::SigningKey::from_keypair_bytes(&bytes)
			.map(|k| {
				kp.zeroize();
				Keypair(k)
			})
			.map_err(|e| DecodingError::KeypairParseError(Box::new(e)))
	}

	/// Sign a message using the private key of this keypair.
	pub fn sign(&self, msg: &[u8]) -> Vec<u8> {
		self.0.sign(msg).to_bytes().to_vec()
	}

	/// Get the public key of this keypair.
	pub fn public(&self) -> PublicKey {
		PublicKey(self.0.verifying_key())
	}

	/// Get the secret key of this keypair.
	pub fn secret(&self) -> SecretKey {
		SecretKey(self.0.to_bytes())
	}
}

impl fmt::Debug for Keypair {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("Keypair").field("public", &self.0.verifying_key()).finish()
	}
}

impl From<litep2p_ed25519::Keypair> for Keypair {
	fn from(kp: litep2p_ed25519::Keypair) -> Self {
		Self::try_from_bytes(&mut kp.encode())
			.expect("ed25519_dalek in substrate & litep2p to use the same format")
	}
}

impl From<Keypair> for litep2p_ed25519::Keypair {
	fn from(kp: Keypair) -> Self {
		Self::decode(&mut kp.to_bytes())
			.expect("ed25519_dalek in substrate & litep2p to use the same format")
	}
}

impl From<libp2p_ed25519::Keypair> for Keypair {
	fn from(kp: libp2p_ed25519::Keypair) -> Self {
		Self::try_from_bytes(&mut kp.to_bytes())
			.expect("ed25519_dalek in substrate & libp2p to use the same format")
	}
}

impl From<Keypair> for libp2p_ed25519::Keypair {
	fn from(kp: Keypair) -> Self {
		Self::try_from_bytes(&mut kp.to_bytes())
			.expect("ed25519_dalek in substrate & libp2p to use the same format")
	}
}

/// Demote an Ed25519 keypair to a secret key.
impl From<Keypair> for SecretKey {
	fn from(kp: Keypair) -> SecretKey {
		SecretKey(kp.0.to_bytes())
	}
}

/// Promote an Ed25519 secret key into a keypair.
impl From<SecretKey> for Keypair {
	fn from(sk: SecretKey) -> Keypair {
		let signing = ed25519::SigningKey::from_bytes(&sk.0);
		Keypair(signing)
	}
}

/// An Ed25519 public key.
#[derive(Eq, Clone)]
pub struct PublicKey(ed25519::VerifyingKey);

impl fmt::Debug for PublicKey {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str("PublicKey(compressed): ")?;
		for byte in self.0.as_bytes() {
			write!(f, "{byte:x}")?;
		}
		Ok(())
	}
}

impl cmp::PartialEq for PublicKey {
	fn eq(&self, other: &Self) -> bool {
		self.0.as_bytes().eq(other.0.as_bytes())
	}
}

impl hash::Hash for PublicKey {
	fn hash<H: hash::Hasher>(&self, state: &mut H) {
		self.0.as_bytes().hash(state);
	}
}

impl cmp::PartialOrd for PublicKey {
	fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
		Some(self.cmp(other))
	}
}

impl cmp::Ord for PublicKey {
	fn cmp(&self, other: &Self) -> cmp::Ordering {
		self.0.as_bytes().cmp(other.0.as_bytes())
	}
}

impl PublicKey {
	/// Verify the Ed25519 signature on a message using the public key.
	pub fn verify(&self, msg: &[u8], sig: &[u8]) -> bool {
		ed25519::Signature::try_from(sig).and_then(|s| self.0.verify(msg, &s)).is_ok()
	}

	/// Convert the public key to a byte array in compressed form, i.e.
	/// where one coordinate is represented by a single bit.
	pub fn to_bytes(&self) -> [u8; 32] {
		self.0.to_bytes()
	}

	/// Try to parse a public key from a byte array containing the actual key as produced by
	/// `to_bytes`.
	pub fn try_from_bytes(k: &[u8]) -> Result<PublicKey, DecodingError> {
		let k =
			<[u8; 32]>::try_from(k).map_err(|e| DecodingError::PublicKeyParseError(Box::new(e)))?;
		ed25519::VerifyingKey::from_bytes(&k)
			.map_err(|e| DecodingError::PublicKeyParseError(Box::new(e)))
			.map(PublicKey)
	}

	/// Convert public key to `PeerId`.
	pub fn to_peer_id(&self) -> PeerId {
		litep2p::PeerId::from(litep2p::crypto::PublicKey::Ed25519(self.clone().into())).into()
	}
}

impl From<litep2p_ed25519::PublicKey> for PublicKey {
	fn from(k: litep2p_ed25519::PublicKey) -> Self {
		Self::try_from_bytes(&k.encode())
			.expect("ed25519_dalek in substrate & litep2p to use the same format")
	}
}

impl From<PublicKey> for litep2p_ed25519::PublicKey {
	fn from(k: PublicKey) -> Self {
		Self::decode(&k.to_bytes())
			.expect("ed25519_dalek in substrate & litep2p to use the same format")
	}
}

impl From<libp2p_ed25519::PublicKey> for PublicKey {
	fn from(k: libp2p_ed25519::PublicKey) -> Self {
		Self::try_from_bytes(&k.to_bytes())
			.expect("ed25519_dalek in substrate & libp2p to use the same format")
	}
}

impl From<PublicKey> for libp2p_ed25519::PublicKey {
	fn from(k: PublicKey) -> Self {
		Self::try_from_bytes(&k.to_bytes())
			.expect("ed25519_dalek in substrate & libp2p to use the same format")
	}
}

/// An Ed25519 secret key.
#[derive(Clone)]
pub struct SecretKey(ed25519::SecretKey);

/// View the bytes of the secret key.
impl AsRef<[u8]> for SecretKey {
	fn as_ref(&self) -> &[u8] {
		&self.0[..]
	}
}

impl fmt::Debug for SecretKey {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "SecretKey")
	}
}

impl SecretKey {
	/// Generate a new Ed25519 secret key.
	pub fn generate() -> SecretKey {
		let signing = ed25519::SigningKey::generate(&mut rand::rngs::OsRng);
		SecretKey(signing.to_bytes())
	}

	/// Try to parse an Ed25519 secret key from a byte slice
	/// containing the actual key, zeroing the input on success.
	/// If the bytes do not constitute a valid Ed25519 secret key, an error is
	/// returned.
	pub fn try_from_bytes(mut sk_bytes: impl AsMut<[u8]>) -> Result<SecretKey, DecodingError> {
		let sk_bytes = sk_bytes.as_mut();
		let secret = <[u8; 32]>::try_from(&*sk_bytes)
			.map_err(|e| DecodingError::SecretKeyParseError(Box::new(e)))?;
		sk_bytes.zeroize();
		Ok(SecretKey(secret))
	}

	pub fn to_bytes(&self) -> [u8; 32] {
		self.0
	}
}

impl Drop for SecretKey {
	fn drop(&mut self) {
		self.0.zeroize();
	}
}

impl From<litep2p_ed25519::SecretKey> for SecretKey {
	fn from(sk: litep2p_ed25519::SecretKey) -> Self {
		Self::try_from_bytes(&mut sk.to_bytes()).expect("Ed25519 key to be 32 bytes length")
	}
}

impl From<SecretKey> for litep2p_ed25519::SecretKey {
	fn from(sk: SecretKey) -> Self {
		Self::from_bytes(&mut sk.to_bytes())
			.expect("litep2p `SecretKey` to accept 32 bytes as Ed25519 key")
	}
}

impl From<libp2p_ed25519::SecretKey> for SecretKey {
	fn from(sk: libp2p_ed25519::SecretKey) -> Self {
		Self::try_from_bytes(&mut sk.as_ref().to_owned())
			.expect("Ed25519 key to be 32 bytes length")
	}
}

impl From<SecretKey> for libp2p_ed25519::SecretKey {
	fn from(sk: SecretKey) -> Self {
		Self::try_from_bytes(&mut sk.to_bytes())
			.expect("libp2p `SecretKey` to accept 32 bytes as Ed25519 key")
	}
}

/// Error when decoding `ed25519`-related types.
#[derive(Debug, thiserror::Error)]
pub enum DecodingError {
	#[error("failed to parse Ed25519 keypair: {0}")]
	KeypairParseError(Box<dyn std::error::Error + Send + Sync>),
	#[error("failed to parse Ed25519 secret key: {0}")]
	SecretKeyParseError(Box<dyn std::error::Error + Send + Sync>),
	#[error("failed to parse Ed25519 public key: {0}")]
	PublicKeyParseError(Box<dyn std::error::Error + Send + Sync>),
}

#[cfg(test)]
mod tests {
	use super::*;
	use quickcheck::*;

	fn eq_keypairs(kp1: &Keypair, kp2: &Keypair) -> bool {
		kp1.public() == kp2.public() && kp1.0.to_bytes() == kp2.0.to_bytes()
	}

	#[test]
	fn ed25519_keypair_encode_decode() {
		fn prop() -> bool {
			let kp1 = Keypair::generate();
			let mut kp1_enc = kp1.to_bytes();
			let kp2 = Keypair::try_from_bytes(&mut kp1_enc).unwrap();
			eq_keypairs(&kp1, &kp2) && kp1_enc.iter().all(|b| *b == 0)
		}
		QuickCheck::new().tests(10).quickcheck(prop as fn() -> _);
	}

	#[test]
	fn ed25519_keypair_from_secret() {
		fn prop() -> bool {
			let kp1 = Keypair::generate();
			let mut sk = kp1.0.to_bytes();
			let kp2 = Keypair::from(SecretKey::try_from_bytes(&mut sk).unwrap());
			eq_keypairs(&kp1, &kp2) && sk == [0u8; 32]
		}
		QuickCheck::new().tests(10).quickcheck(prop as fn() -> _);
	}

	#[test]
	fn ed25519_signature() {
		let kp = Keypair::generate();
		let pk = kp.public();

		let msg = "hello world".as_bytes();
		let sig = kp.sign(msg);
		assert!(pk.verify(msg, &sig));

		let mut invalid_sig = sig.clone();
		invalid_sig[3..6].copy_from_slice(&[10, 23, 42]);
		assert!(!pk.verify(msg, &invalid_sig));

		let invalid_msg = "h3ll0 w0rld".as_bytes();
		assert!(!pk.verify(invalid_msg, &sig));
	}

	#[test]
	fn substrate_kp_to_libs() {
		let kp = Keypair::generate();
		let kp_bytes = kp.to_bytes();
		let kp1: libp2p_ed25519::Keypair = kp.clone().into();
		let kp2: litep2p_ed25519::Keypair = kp.clone().into();
		let kp3 = libp2p_ed25519::Keypair::try_from_bytes(&mut kp_bytes.clone()).unwrap();
		let kp4 = litep2p_ed25519::Keypair::decode(&mut kp_bytes.clone()).unwrap();

		assert_eq!(kp_bytes, kp1.to_bytes());
		assert_eq!(kp_bytes, kp2.encode());

		let msg = "hello world".as_bytes();
		let sig = kp.sign(msg);
		let sig1 = kp1.sign(msg);
		let sig2 = kp2.sign(msg);
		let sig3 = kp3.sign(msg);
		let sig4 = kp4.sign(msg);

		assert_eq!(sig, sig1);
		assert_eq!(sig, sig2);
		assert_eq!(sig, sig3);
		assert_eq!(sig, sig4);

		let pk1 = kp1.public();
		let pk2 = kp2.public();
		let pk3 = kp3.public();
		let pk4 = kp4.public();

		assert!(pk1.verify(msg, &sig));
		assert!(pk2.verify(msg, &sig));
		assert!(pk3.verify(msg, &sig));
		assert!(pk4.verify(msg, &sig));
	}

	#[test]
	fn litep2p_kp_to_substrate_kp() {
		let kp = litep2p_ed25519::Keypair::generate();
		let kp1: Keypair = kp.clone().into();
		let kp2 = Keypair::try_from_bytes(&mut kp.encode()).unwrap();

		assert_eq!(kp.encode(), kp1.to_bytes());

		let msg = "hello world".as_bytes();
		let sig = kp.sign(msg);
		let sig1 = kp1.sign(msg);
		let sig2 = kp2.sign(msg);

		assert_eq!(sig, sig1);
		assert_eq!(sig, sig2);

		let pk1 = kp1.public();
		let pk2 = kp2.public();

		assert!(pk1.verify(msg, &sig));
		assert!(pk2.verify(msg, &sig));
	}

	#[test]
	fn libp2p_kp_to_substrate_kp() {
		let kp = libp2p_ed25519::Keypair::generate();
		let kp1: Keypair = kp.clone().into();
		let kp2 = Keypair::try_from_bytes(&mut kp.to_bytes()).unwrap();

		assert_eq!(kp.to_bytes(), kp1.to_bytes());

		let msg = "hello world".as_bytes();
		let sig = kp.sign(msg);
		let sig1 = kp1.sign(msg);
		let sig2 = kp2.sign(msg);

		assert_eq!(sig, sig1);
		assert_eq!(sig, sig2);

		let pk1 = kp1.public();
		let pk2 = kp2.public();

		assert!(pk1.verify(msg, &sig));
		assert!(pk2.verify(msg, &sig));
	}

	#[test]
	fn substrate_pk_to_libs() {
		let kp = Keypair::generate();
		let pk = kp.public();
		let pk_bytes = pk.to_bytes();
		let pk1: libp2p_ed25519::PublicKey = pk.clone().into();
		let pk2: litep2p_ed25519::PublicKey = pk.clone().into();
		let pk3 = libp2p_ed25519::PublicKey::try_from_bytes(&pk_bytes).unwrap();
		let pk4 = litep2p_ed25519::PublicKey::decode(&pk_bytes).unwrap();

		assert_eq!(pk_bytes, pk1.to_bytes());
		assert_eq!(pk_bytes, pk2.encode());

		let msg = "hello world".as_bytes();
		let sig = kp.sign(msg);

		assert!(pk.verify(msg, &sig));
		assert!(pk1.verify(msg, &sig));
		assert!(pk2.verify(msg, &sig));
		assert!(pk3.verify(msg, &sig));
		assert!(pk4.verify(msg, &sig));
	}

	#[test]
	fn litep2p_pk_to_substrate_pk() {
		let kp = litep2p_ed25519::Keypair::generate();
		let pk = kp.public();
		let pk_bytes = pk.clone().encode();
		let pk1: PublicKey = pk.clone().into();
		let pk2 = PublicKey::try_from_bytes(&pk_bytes).unwrap();

		assert_eq!(pk_bytes, pk1.to_bytes());

		let msg = "hello world".as_bytes();
		let sig = kp.sign(msg);

		assert!(pk.verify(msg, &sig));
		assert!(pk1.verify(msg, &sig));
		assert!(pk2.verify(msg, &sig));
	}

	#[test]
	fn libp2p_pk_to_substrate_pk() {
		let kp = libp2p_ed25519::Keypair::generate();
		let pk = kp.public();
		let pk_bytes = pk.clone().to_bytes();
		let pk1: PublicKey = pk.clone().into();
		let pk2 = PublicKey::try_from_bytes(&pk_bytes).unwrap();

		assert_eq!(pk_bytes, pk1.to_bytes());

		let msg = "hello world".as_bytes();
		let sig = kp.sign(msg);

		assert!(pk.verify(msg, &sig));
		assert!(pk1.verify(msg, &sig));
		assert!(pk2.verify(msg, &sig));
	}

	#[test]
	fn substrate_sk_to_libs() {
		let sk = SecretKey::generate();
		let sk_bytes = sk.to_bytes();
		let sk1: libp2p_ed25519::SecretKey = sk.clone().into();
		let sk2: litep2p_ed25519::SecretKey = sk.clone().into();
		let sk3 = libp2p_ed25519::SecretKey::try_from_bytes(&mut sk_bytes.clone()).unwrap();
		let sk4 = litep2p_ed25519::SecretKey::from_bytes(&mut sk_bytes.clone()).unwrap();

		let kp: Keypair = sk.into();
		let kp1: libp2p_ed25519::Keypair = sk1.into();
		let kp2: litep2p_ed25519::Keypair = sk2.into();
		let kp3: libp2p_ed25519::Keypair = sk3.into();
		let kp4: litep2p_ed25519::Keypair = sk4.into();

		let msg = "hello world".as_bytes();
		let sig = kp.sign(msg);

		assert_eq!(sig, kp1.sign(msg));
		assert_eq!(sig, kp2.sign(msg));
		assert_eq!(sig, kp3.sign(msg));
		assert_eq!(sig, kp4.sign(msg));
	}

	#[test]
	fn litep2p_sk_to_substrate_sk() {
		let sk = litep2p_ed25519::SecretKey::generate();
		let sk1: SecretKey = sk.clone().into();
		let sk2 = SecretKey::try_from_bytes(&mut sk.to_bytes()).unwrap();

		let kp: litep2p_ed25519::Keypair = sk.into();
		let kp1: Keypair = sk1.into();
		let kp2: Keypair = sk2.into();

		let msg = "hello world".as_bytes();
		let sig = kp.sign(msg);

		assert_eq!(sig, kp1.sign(msg));
		assert_eq!(sig, kp2.sign(msg));
	}

	#[test]
	fn libp2p_sk_to_substrate_sk() {
		let sk = libp2p_ed25519::SecretKey::generate();
		let sk_bytes = sk.as_ref().to_owned();
		let sk1: SecretKey = sk.clone().into();
		let sk2 = SecretKey::try_from_bytes(sk_bytes).unwrap();

		let kp: libp2p_ed25519::Keypair = sk.into();
		let kp1: Keypair = sk1.into();
		let kp2: Keypair = sk2.into();

		let msg = "hello world".as_bytes();
		let sig = kp.sign(msg);

		assert_eq!(sig, kp1.sign(msg));
		assert_eq!(sig, kp2.sign(msg));
	}
}
