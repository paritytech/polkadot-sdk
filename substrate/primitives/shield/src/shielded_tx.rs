//! MEV Shielded Transaction

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, Encode};
use scale_info::TypeInfo;

const KEY_HASH_LEN: usize = 16;
const NONCE_LEN: usize = 24;

#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
pub struct ShieldedTransaction {
    pub key_hash: [u8; KEY_HASH_LEN],
    pub kem_ct: Vec<u8>,
    pub aead_ct: Vec<u8>,
    pub nonce: [u8; NONCE_LEN],
}

impl ShieldedTransaction {
    pub fn parse(ciphertext: &[u8]) -> Option<Self> {
        let mut cursor: usize = 0;

        let key_hash_end = cursor.checked_add(KEY_HASH_LEN)?;
        let key_hash: [u8; KEY_HASH_LEN] = ciphertext.get(cursor..key_hash_end)?.try_into().ok()?;
        cursor = key_hash_end;

        let kem_ct_len_end = cursor.checked_add(2)?;
        let kem_ct_len = ciphertext
            .get(cursor..kem_ct_len_end)?
            .try_into()
            .map(u16::from_le_bytes)
            .ok()?
            .into();
        cursor = kem_ct_len_end;

        let kem_ct_end = cursor.checked_add(kem_ct_len)?;
        let kem_ct = ciphertext.get(cursor..kem_ct_end)?.to_vec();
        cursor = kem_ct_end;

        let nonce_end = cursor.checked_add(NONCE_LEN)?;
        let nonce = ciphertext.get(cursor..nonce_end)?.try_into().ok()?;
        cursor = nonce_end;

        let aead_ct = ciphertext.get(cursor..)?.to_vec();

        Some(Self {
            key_hash,
            kem_ct,
            aead_ct,
            nonce,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_ciphertext(
        key_hash: &[u8; KEY_HASH_LEN],
        kem_ct: &[u8],
        nonce: &[u8; 24],
        aead_ct: &[u8],
    ) -> Vec<u8> {
        let kem_len = (kem_ct.len() as u16).to_le_bytes();
        let mut buf = Vec::with_capacity(KEY_HASH_LEN + 2 + kem_ct.len() + 24 + aead_ct.len());
        buf.extend_from_slice(key_hash);
        buf.extend_from_slice(&kem_len);
        buf.extend_from_slice(kem_ct);
        buf.extend_from_slice(nonce);
        buf.extend_from_slice(aead_ct);
        buf
    }

    const DUMMY_KEM_CT: [u8; 1088] = [0xAA; 1088];
    const DUMMY_NONCE: [u8; 24] = [0xBB; 24];
    const DUMMY_KEY_HASH: [u8; KEY_HASH_LEN] = [0xCC; KEY_HASH_LEN];
    const DUMMY_AEAD: [u8; 64] = [0xDD; 64];

    fn valid_ciphertext() -> Vec<u8> {
        build_ciphertext(&DUMMY_KEY_HASH, &DUMMY_KEM_CT, &DUMMY_NONCE, &DUMMY_AEAD)
    }

    #[test]
    fn parse_valid_roundtrip() {
        let ct = valid_ciphertext();
        let tx = ShieldedTransaction::parse(&ct).expect("should parse");

        assert_eq!(tx.key_hash, DUMMY_KEY_HASH);
        assert_eq!(tx.kem_ct, DUMMY_KEM_CT);
        assert_eq!(tx.nonce, DUMMY_NONCE);
        assert_eq!(tx.aead_ct, DUMMY_AEAD);
    }

    #[test]
    fn parse_empty_aead_ct() {
        let ct = build_ciphertext(&DUMMY_KEY_HASH, &DUMMY_KEM_CT, &DUMMY_NONCE, &[]);
        let tx = ShieldedTransaction::parse(&ct).expect("should parse with empty aead_ct");

        assert!(tx.aead_ct.is_empty());
        assert_eq!(tx.kem_ct, DUMMY_KEM_CT);
    }

    #[test]
    fn parse_zero_length_kem_ct() {
        let ct = build_ciphertext(&DUMMY_KEY_HASH, &[], &DUMMY_NONCE, &DUMMY_AEAD);
        let tx = ShieldedTransaction::parse(&ct).expect("should parse with zero-length kem_ct");

        assert!(tx.kem_ct.is_empty());
        assert_eq!(tx.aead_ct, DUMMY_AEAD);
    }

    #[test]
    fn parse_empty_returns_none() {
        assert!(ShieldedTransaction::parse(&[]).is_none());
    }

    #[test]
    fn parse_truncated_key_hash() {
        let ct = [0u8; KEY_HASH_LEN - 1];
        assert!(ShieldedTransaction::parse(&ct).is_none());
    }

    #[test]
    fn parse_truncated_kem_len() {
        // key_hash present but only 1 byte for kem_ct_len (needs 2).
        let ct = [0u8; KEY_HASH_LEN + 1];
        assert!(ShieldedTransaction::parse(&ct).is_none());
    }

    #[test]
    fn parse_kem_ct_len_exceeds_remaining() {
        // Claim 1088 bytes of kem_ct but only provide 10.
        let mut ct = Vec::new();
        ct.extend_from_slice(&DUMMY_KEY_HASH);
        ct.extend_from_slice(&1088u16.to_le_bytes());
        ct.extend_from_slice(&[0u8; 10]);
        assert!(ShieldedTransaction::parse(&ct).is_none());
    }

    #[test]
    fn parse_truncated_nonce() {
        // key_hash + kem_len + kem_ct present, but nonce truncated.
        let mut ct = Vec::new();
        ct.extend_from_slice(&DUMMY_KEY_HASH);
        ct.extend_from_slice(&4u16.to_le_bytes());
        ct.extend_from_slice(&[0u8; 4]); // kem_ct
        ct.extend_from_slice(&[0u8; 20]); // only 20 of 24 nonce bytes
        assert!(ShieldedTransaction::parse(&ct).is_none());
    }

    #[test]
    fn parse_small_kem_ct() {
        let small_kem = [0x11; 4];
        let ct = build_ciphertext(&DUMMY_KEY_HASH, &small_kem, &DUMMY_NONCE, &DUMMY_AEAD);
        let tx = ShieldedTransaction::parse(&ct).expect("should parse");

        assert_eq!(tx.kem_ct, small_kem);
        assert_eq!(tx.aead_ct, DUMMY_AEAD);
    }
}
