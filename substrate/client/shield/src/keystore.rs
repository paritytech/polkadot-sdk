use ml_kem::{
    EncodedSizeUser, KemCore, MlKem768, MlKem768Params,
    kem::{DecapsulationKey, EncapsulationKey},
};
use rand::rngs::OsRng;
use std::sync::RwLock;
use stp_shield::{Error as TraitError, Result as TraitResult, ShieldKeystore};

/// An memory-based keystore for the MEV-Shield.
pub struct MemoryShieldKeystore(RwLock<ShieldKeystoreInner>);

impl MemoryShieldKeystore {
    pub fn new() -> Self {
        Self(RwLock::new(ShieldKeystoreInner::new()))
    }
}

impl ShieldKeystore for MemoryShieldKeystore {
    fn roll_for_next_slot(&self) -> TraitResult<()> {
        self.0
            .write()
            .map_err(|_| TraitError::Unavailable)?
            .roll_for_next_slot()
    }

    fn next_enc_key(&self) -> TraitResult<Vec<u8>> {
        self.0
            .read()
            .map_err(|_| TraitError::Unavailable)?
            .next_enc_key()
    }

    fn current_dec_key(&self) -> TraitResult<Vec<u8>> {
        self.0
            .read()
            .map_err(|_| TraitError::Unavailable)?
            .current_dec_key()
    }
}

/// Holds the current/next ML‑KEM keypairs in-memory for a single author.
pub struct ShieldKeystoreInner {
    current_pair: ShieldKeyPair,
    next_pair: ShieldKeyPair,
}

impl ShieldKeystoreInner {
    fn new() -> Self {
        Self {
            current_pair: ShieldKeyPair::generate(),
            next_pair: ShieldKeyPair::generate(),
        }
    }

    fn roll_for_next_slot(&mut self) -> TraitResult<()> {
        self.current_pair = std::mem::replace(&mut self.next_pair, ShieldKeyPair::generate());
        Ok(())
    }

    fn next_enc_key(&self) -> TraitResult<Vec<u8>> {
        Ok(self.next_pair.enc_key.as_bytes().to_vec())
    }

    fn current_dec_key(&self) -> TraitResult<Vec<u8>> {
        Ok(self.current_pair.dec_key.as_bytes().to_vec())
    }
}

/// A pair of ML‑KEM‑768 decapsulation and encapsulation keys.
#[derive(Debug, Clone)]
struct ShieldKeyPair {
    dec_key: DecapsulationKey<MlKem768Params>,
    enc_key: EncapsulationKey<MlKem768Params>,
}

impl ShieldKeyPair {
    fn generate() -> Self {
        let (dec_key, enc_key) = MlKem768::generate(&mut OsRng);
        Self { dec_key, enc_key }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ml_kem::kem::{Decapsulate, Encapsulate};
    use stp_shield::ShieldKeystore;

    const MLKEM768_ENC_KEY_LEN: usize = 1184;
    const MLKEM768_DEC_KEY_LEN: usize = 2400;

    #[test]
    fn next_enc_key_returns_valid_mlkem768_key() {
        let ks = MemoryShieldKeystore::new();
        let ek = ks.next_enc_key().unwrap();
        assert_eq!(ek.len(), MLKEM768_ENC_KEY_LEN);
    }

    #[test]
    fn next_enc_key_is_stable_without_roll() {
        let ks = MemoryShieldKeystore::new();
        let ek1 = ks.next_enc_key().unwrap();
        let ek2 = ks.next_enc_key().unwrap();
        assert_eq!(ek1, ek2);
    }

    #[test]
    fn roll_changes_next_enc_key() {
        let ks = MemoryShieldKeystore::new();
        let before = ks.next_enc_key().unwrap();
        ks.roll_for_next_slot().unwrap();
        let after = ks.next_enc_key().unwrap();
        assert_ne!(before, after);
    }

    #[test]
    fn current_dec_key_returns_valid_mlkem768_key() {
        let ks = MemoryShieldKeystore::new();
        ks.roll_for_next_slot().unwrap();
        let dk = ks.current_dec_key().unwrap();
        assert_eq!(dk.len(), MLKEM768_DEC_KEY_LEN);
        // Verify the bytes can be parsed back into a valid DecapsulationKey.
        DecapsulationKey::<MlKem768Params>::from_bytes(dk.as_slice().try_into().unwrap());
    }

    #[test]
    fn current_dec_key_matches_previous_enc_key() {
        let ks = MemoryShieldKeystore::new();
        // Encapsulate to the "next" enc key.
        let ek_bytes = ks.next_enc_key().unwrap();
        let enc_key = EncapsulationKey::<MlKem768Params>::from_bytes(
            ek_bytes.as_slice().try_into().unwrap(),
        );
        let (kem_ct, shared_secret_enc) = enc_key.encapsulate(&mut OsRng).unwrap();

        // Roll: next becomes current.
        ks.roll_for_next_slot().unwrap();
        let dk_bytes = ks.current_dec_key().unwrap();
        let dec_key = DecapsulationKey::<MlKem768Params>::from_bytes(
            dk_bytes.as_slice().try_into().unwrap(),
        );

        // Decapsulate with current dec key should yield the same shared secret.
        let ct = ml_kem::Ciphertext::<MlKem768>::try_from(kem_ct.as_slice()).unwrap();
        let shared_secret_dec = dec_key.decapsulate(&ct).unwrap();
        assert_eq!(shared_secret_enc.as_slice(), shared_secret_dec.as_slice());
    }
}
