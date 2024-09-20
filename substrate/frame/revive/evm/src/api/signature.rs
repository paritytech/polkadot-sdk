//! Ethereum signature utilities
#![cfg(feature = "std")]
use super::{TransactionLegacySigned, TransactionLegacyUnsigned};
use rlp::Encodable;
use secp256k1::{
	ecdsa::{RecoverableSignature, RecoveryId},
	Message, PublicKey, Secp256k1,
};
use sp_core::{keccak_256, H160, U256};

impl TransactionLegacySigned {
	/// Create a signed transaction from an [`TransactionLegacyUnsigned`] and a
	/// [`RecoverableSignature`].
	pub fn from(
		transaction_legacy_unsigned: TransactionLegacyUnsigned,
		signature: RecoverableSignature,
	) -> TransactionLegacySigned {
		let (recovery_id, sig) = signature.serialize_compact();
		let r = U256::from_big_endian(&sig[..32]);
		let s = U256::from_big_endian(&sig[32..64]);
		let v = transaction_legacy_unsigned
			.chain_id
			.map(|chain_id| chain_id * 2 + 35 + recovery_id.to_i32() as u32)
			.unwrap_or_else(|| U256::from(27) + recovery_id.to_i32() as u32);

		TransactionLegacySigned { transaction_legacy_unsigned, r, s, v }
	}

	/// Get the [`RecoverableSignature`] from the signed transaction.
	fn recoverable_signature(&self) -> Result<RecoverableSignature, secp256k1::Error> {
		let mut s = [0u8; 64];
		self.r.to_big_endian(s[0..32].as_mut());
		self.s.to_big_endian(s[32..64].as_mut());
		RecoverableSignature::from_compact(&s[..], self.extract_recovery_id()?)
	}

	/// Get the raw 65 bytes signature from the signed transaction.
	pub fn raw_signature(&self) -> Result<[u8; 65], secp256k1::Error> {
		let mut s = [0u8; 65];
		self.r.to_big_endian(s[0..32].as_mut());
		self.s.to_big_endian(s[32..64].as_mut());
		let recovery_id = self.extract_recovery_id()?.to_i32();
		s[64] = recovery_id as _;
		Ok(s)
	}

	/// Get the recovery ID from the signed transaction.
	fn extract_recovery_id(&self) -> Result<RecoveryId, secp256k1::Error> {
		let recovery_id = if let Some(chain_id) = self.transaction_legacy_unsigned.chain_id {
			self.v - chain_id * 2 - 35
		} else {
			self.v
		};

		if self.v < i32::MAX.into() {
			RecoveryId::from_i32(recovery_id.as_u32() as _)
		} else {
			Err(secp256k1::Error::InvalidRecoveryId)
		}
	}

	/// Recover the Ethereum address from the signed transaction.
	pub fn recover_eth_address(&self) -> Result<H160, secp256k1::Error> {
		let pub_key = self.recover_pub_key()?;
		pub_key_to_address(&pub_key)
	}

	/// Recover the public key from the signed transaction.
	fn recover_pub_key(&self) -> Result<PublicKey, secp256k1::Error> {
		let sig = self.recoverable_signature()?;
		let rlp_encoded = self.transaction_legacy_unsigned.rlp_bytes();
		let tx_hash = keccak_256(&rlp_encoded);

		let msg = Message::from_digest(tx_hash);
		let secp = Secp256k1::new();
		secp.recover_ecdsa(&msg, &sig)
	}
}

/// Convert a public key to an Ethereum address.
fn pub_key_to_address(pub_key: &PublicKey) -> Result<H160, secp256k1::Error> {
	let pub_key = pub_key.serialize_uncompressed();
	if pub_key[0] != 0x04 {
		return Err(secp256k1::Error::InvalidPublicKey);
	}

	let hash = keccak_256(&pub_key[1..]);
	Ok(H160::from_slice(&hash[12..]))
}
