// Copyright 2019 Parity Technologies (UK) Ltd.
// Copyright 2023 litep2p developers
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

//! Ed25519 keys.

use crate::{
    error::{Error, ParseError},
    PeerId,
};

use ed25519_dalek::{self as ed25519, Signer as _, Verifier as _};
use std::fmt;
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
    pub fn try_from_bytes(kp: &mut [u8]) -> Result<Keypair, Error> {
        let bytes = <[u8; 64]>::try_from(&*kp)
            .map_err(|e| Error::Other(format!("Failed to parse ed25519 keypair: {e}")))?;

        ed25519::SigningKey::from_keypair_bytes(&bytes)
            .map(|k| {
                kp.zeroize();
                Keypair(k)
            })
            .map_err(|e| Error::Other(format!("Failed to parse ed25519 keypair: {e}")))
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

impl PartialEq for PublicKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_bytes().eq(other.0.as_bytes())
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

    /// Get the public key as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// Try to parse a public key from a byte array containing the actual key as produced by
    /// `to_bytes`.
    pub fn try_from_bytes(k: &[u8]) -> Result<PublicKey, ParseError> {
        let k = <[u8; 32]>::try_from(k).map_err(|_| ParseError::InvalidPublicKey)?;

        // The error type of the verifying key is deliberately opaque as to avoid side-channel
        // leakage. We can't provide a more specific error type here.
        ed25519::VerifyingKey::from_bytes(&k)
            .map_err(|_| ParseError::InvalidPublicKey)
            .map(PublicKey)
    }

    /// Convert public key to `PeerId`.
    pub fn to_peer_id(&self) -> PeerId {
        crate::crypto::PublicKey::Ed25519(self.clone()).into()
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
    pub fn try_from_bytes(mut sk_bytes: impl AsMut<[u8]>) -> crate::Result<SecretKey> {
        let sk_bytes = sk_bytes.as_mut();
        let secret = <[u8; 32]>::try_from(&*sk_bytes)
            .map_err(|e| Error::Other(format!("Failed to parse ed25519 secret key: {e}")))?;
        sk_bytes.zeroize();
        Ok(SecretKey(secret))
    }

    /// Convert this secret key to a byte array.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0
    }
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
    fn secret_key() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let key = Keypair::generate();
        tracing::trace!("keypair: {:?}", key);
        tracing::trace!("secret: {:?}", key.secret());
        tracing::trace!("public: {:?}", key.public());

        let new_key = Keypair::from(key.secret());
        assert_eq!(new_key.secret().as_ref(), key.secret().as_ref());
        assert_eq!(new_key.public(), key.public());

        let new_secret = SecretKey::from(new_key.clone());
        assert_eq!(new_secret.as_ref(), new_key.secret().as_ref());

        let cloned_secret = new_secret.clone();
        assert_eq!(cloned_secret.as_ref(), new_secret.as_ref());
    }
}
