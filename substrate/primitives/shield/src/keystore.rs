//! MEV Shield Keystore traits

extern crate alloc;

use codec::{Decode, Encode};

use alloc::{string::String, sync::Arc, vec::Vec};

#[derive(Debug, Encode, Decode)]
pub enum Error {
    /// Keystore unavailable
    Unavailable,
    /// Validation error
    ValidationError(String),
    /// Other error
    Other(String),
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Unavailable => write!(f, "Keystore unavailable"),
            Error::ValidationError(e) => write!(f, "Validation error: {}", e),
            Error::Other(e) => write!(f, "Other error: {}", e),
        }
    }
}

pub type Result<T> = core::result::Result<T, Error>;

/// Something that generates, stores and provides access to secret keys
/// and operations used by the MEV Shield.
pub trait ShieldKeystore: Send + Sync {
    /// Roll for the next slot and update the current/next keys.
    fn roll_for_next_slot(&self) -> Result<()>;

    /// Get the next public key.
    fn next_public_key(&self) -> Result<Vec<u8>>;

    /// Decapsulate a ciphertext using the ML-KEM-768 algorithm.
    fn mlkem768_decapsulate(&self, ciphertext: &[u8]) -> Result<[u8; 32]>;

    /// Decrypt a ciphertext using the XChaCha20-Poly1305 AEAD scheme.
    fn aead_decrypt(
        &self,
        key: [u8; 32],
        nonce: [u8; 24],
        msg: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>>;
}

impl<T: ShieldKeystore + 'static> ShieldKeystore for Arc<T> {
    fn roll_for_next_slot(&self) -> Result<()> {
        (**self).roll_for_next_slot()
    }

    fn next_public_key(&self) -> Result<Vec<u8>> {
        (**self).next_public_key()
    }

    fn mlkem768_decapsulate(&self, ciphertext: &[u8]) -> Result<[u8; 32]> {
        (**self).mlkem768_decapsulate(ciphertext)
    }

    fn aead_decrypt(
        &self,
        key: [u8; 32],
        nonce: [u8; 24],
        msg: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>> {
        (**self).aead_decrypt(key, nonce, msg, aad)
    }
}

pub type ShieldKeystorePtr = Arc<dyn ShieldKeystore>;

sp_externalities::decl_extension! {
    /// The shield keystore extension to register/retrieve from the externalities.
    pub struct ShieldKeystoreExt(ShieldKeystorePtr);
}

impl ShieldKeystoreExt {
    pub fn from(keystore: ShieldKeystorePtr) -> Self {
        Self(keystore)
    }
}
