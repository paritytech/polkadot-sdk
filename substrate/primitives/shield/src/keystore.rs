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

    /// Get the next ML-KEM-768 encapsulation (public) key bytes.
    fn next_enc_key(&self) -> Result<Vec<u8>>;

    /// Get the current ML-KEM-768 decapsulation (private) key bytes.
    fn current_dec_key(&self) -> Result<Vec<u8>>;
}

impl<T: ShieldKeystore + 'static> ShieldKeystore for Arc<T> {
    fn roll_for_next_slot(&self) -> Result<()> {
        (**self).roll_for_next_slot()
    }

    fn next_enc_key(&self) -> Result<Vec<u8>> {
        (**self).next_enc_key()
    }

    fn current_dec_key(&self) -> Result<Vec<u8>> {
        (**self).current_dec_key()
    }
}

pub type ShieldKeystorePtr = Arc<dyn ShieldKeystore>;
