use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use ml_kem::{
    Ciphertext, EncodedSizeUser, KemCore, MlKem768, MlKem768Params,
    kem::{Decapsulate, DecapsulationKey, EncapsulationKey},
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

    fn next_public_key(&self) -> TraitResult<Vec<u8>> {
        self.0
            .read()
            .map_err(|_| TraitError::Unavailable)?
            .next_public_key()
    }

    fn mlkem768_decapsulate(&self, ciphertext: &[u8]) -> TraitResult<[u8; 32]> {
        self.0
            .read()
            .map_err(|_| TraitError::Unavailable)?
            .mlkem768_decapsulate(ciphertext)
    }

    fn aead_decrypt(
        &self,
        key: [u8; 32],
        nonce: [u8; 24],
        msg: &[u8],
        aad: &[u8],
    ) -> TraitResult<Vec<u8>> {
        self.0
            .read()
            .map_err(|_| TraitError::Unavailable)?
            .aead_decrypt(key, nonce, msg, aad)
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

    fn next_public_key(&self) -> TraitResult<Vec<u8>> {
        Ok(self.next_pair.enc_key.as_bytes().to_vec())
    }

    fn mlkem768_decapsulate(&self, ciphertext: &[u8]) -> TraitResult<[u8; 32]> {
        let ciphertext = Ciphertext::<MlKem768>::try_from(ciphertext)
            .map_err(|e| TraitError::ValidationError(e.to_string()))?;
        let shared_secret = self
            .current_pair
            .dec_key
            .decapsulate(&ciphertext)
            .map_err(|_| {
                TraitError::Other("Failed to decapsulate ciphertext using ML-KEM 768".into())
            })?;

        Ok(shared_secret.into())
    }

    fn aead_decrypt(
        &self,
        key: [u8; 32],
        nonce: [u8; 24],
        msg: &[u8],
        aad: &[u8],
    ) -> TraitResult<Vec<u8>> {
        let aead = XChaCha20Poly1305::new((&key).into());
        let nonce = XNonce::from_slice(&nonce);
        let payload = Payload { msg, aad };
        let decrypted = aead
            .decrypt(nonce, payload)
            .map_err(|_| TraitError::Other("Failed to decrypt message using AEAD".into()))?;

        Ok(decrypted)
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
    use ml_kem::kem::Encapsulate;
    use stp_shield::ShieldKeystore;

    const MLKEM768_PK_LEN: usize = 1184;
    const MLKEM768_CT_LEN: usize = 1088;

    #[test]
    fn next_public_key_returns_valid_mlkem768_key() {
        let ks = MemoryShieldKeystore::new();
        let pk = ks.next_public_key().unwrap();
        assert_eq!(pk.len(), MLKEM768_PK_LEN);
    }

    #[test]
    fn next_public_key_is_stable_without_roll() {
        let ks = MemoryShieldKeystore::new();
        let pk1 = ks.next_public_key().unwrap();
        let pk2 = ks.next_public_key().unwrap();
        assert_eq!(pk1, pk2);
    }

    #[test]
    fn roll_changes_next_public_key() {
        let ks = MemoryShieldKeystore::new();
        let before = ks.next_public_key().unwrap();
        ks.roll_for_next_slot().unwrap();
        let after = ks.next_public_key().unwrap();
        assert_ne!(before, after);
    }

    #[test]
    fn decapsulate_with_current_key_after_roll() {
        let ks = MemoryShieldKeystore::new();

        // The "next" public key is what will be announced in the inherent.
        // After a roll, it becomes the "current" key used for decapsulation.
        let pk_bytes = ks.next_public_key().unwrap();
        let enc_key =
            EncapsulationKey::<MlKem768Params>::from_bytes(pk_bytes.as_slice().try_into().unwrap());

        let (ct, ss_sender) = enc_key.encapsulate(&mut OsRng).unwrap();

        // Roll so that next → current.
        ks.roll_for_next_slot().unwrap();

        // Decapsulate uses `current_pair`, which is now the old `next_pair`.
        let ss_receiver = ks.mlkem768_decapsulate(ct.as_slice()).unwrap();
        assert_eq!(ss_sender.as_slice(), &ss_receiver);
    }

    #[test]
    fn decapsulate_fails_with_wrong_ciphertext() {
        let ks = MemoryShieldKeystore::new();
        let garbage = vec![0u8; MLKEM768_CT_LEN];
        // Decapsulation with garbage should still produce a result (ML-KEM implicit reject),
        // but let's just verify it doesn't panic.
        let _ = ks.mlkem768_decapsulate(&garbage);
    }

    #[test]
    fn decapsulate_fails_with_wrong_length() {
        let ks = MemoryShieldKeystore::new();
        let short = vec![0u8; 32];
        assert!(ks.mlkem768_decapsulate(&short).is_err());
    }

    #[test]
    fn aead_encrypt_decrypt_roundtrip() {
        let ks = MemoryShieldKeystore::new();
        let key = [42u8; 32];
        let nonce = [7u8; 24];
        let plaintext = b"hello mev shield";
        let aad = b"extra data";

        // Encrypt with chacha20poly1305 directly.
        let cipher = XChaCha20Poly1305::new((&key).into());
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .unwrap();

        // Decrypt via keystore.
        let decrypted = ks.aead_decrypt(key, nonce, &ciphertext, aad).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn aead_decrypt_fails_with_wrong_key() {
        let ks = MemoryShieldKeystore::new();
        let key = [42u8; 32];
        let wrong_key = [99u8; 32];
        let nonce = [7u8; 24];
        let plaintext = b"secret";

        let cipher = XChaCha20Poly1305::new((&key).into());
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext.as_slice(),
                    aad: &[],
                },
            )
            .unwrap();

        assert!(ks.aead_decrypt(wrong_key, nonce, &ciphertext, &[]).is_err());
    }

    #[test]
    fn aead_decrypt_fails_with_wrong_aad() {
        let ks = MemoryShieldKeystore::new();
        let key = [42u8; 32];
        let nonce = [7u8; 24];
        let plaintext = b"secret";

        let cipher = XChaCha20Poly1305::new((&key).into());
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext.as_slice(),
                    aad: b"correct aad",
                },
            )
            .unwrap();

        assert!(
            ks.aead_decrypt(key, nonce, &ciphertext, b"wrong aad")
                .is_err()
        );
    }

    #[test]
    fn full_encrypt_decrypt_roundtrip() {
        // Simulates the full client → block author flow:
        // 1. Client reads next_public_key, encapsulates, encrypts with AEAD
        // 2. Author rolls, decapsulates with current key, decrypts AEAD
        let ks = MemoryShieldKeystore::new();

        // Client side: read the announced public key and encrypt.
        let pk_bytes = ks.next_public_key().unwrap();
        let enc_key =
            EncapsulationKey::<MlKem768Params>::from_bytes(pk_bytes.as_slice().try_into().unwrap());
        let (kem_ct, shared_secret) = enc_key.encapsulate(&mut OsRng).unwrap();

        let nonce = [13u8; 24];
        let plaintext = b"signed_extrinsic_bytes_here";
        let cipher = XChaCha20Poly1305::new(shared_secret.as_slice().into());
        let aead_ct = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext.as_slice(),
                    aad: &[],
                },
            )
            .unwrap();

        // Author side: roll (next → current), then decrypt.
        ks.roll_for_next_slot().unwrap();

        let recovered_ss = ks.mlkem768_decapsulate(kem_ct.as_slice()).unwrap();
        let decrypted = ks.aead_decrypt(recovered_ss, nonce, &aead_ct, &[]).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decapsulate_before_roll_uses_different_key() {
        // Without rolling, decapsulate uses the initial `current_pair`,
        // not the `next_pair` whose public key we encapsulated with.
        let ks = MemoryShieldKeystore::new();

        let pk_bytes = ks.next_public_key().unwrap();
        let enc_key =
            EncapsulationKey::<MlKem768Params>::from_bytes(pk_bytes.as_slice().try_into().unwrap());
        let (kem_ct, ss_sender) = enc_key.encapsulate(&mut OsRng).unwrap();

        // DO NOT roll — decapsulate uses initial current_pair, not next_pair.
        let ss_receiver = ks.mlkem768_decapsulate(kem_ct.as_slice()).unwrap();

        // Shared secrets should NOT match (different keypairs).
        assert_ne!(ss_sender.as_slice(), &ss_receiver);
    }
}
