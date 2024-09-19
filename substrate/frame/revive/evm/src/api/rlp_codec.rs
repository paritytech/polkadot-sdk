//! RLP encoding and decoding for Ethereum transactions.
//! See <https://eth.wiki/fundamentals/rlp> for more information about RLP encoding.

use super::*;
use rlp::{Decodable, Encodable};
use sp_core::{keccak_256, H160};
use sp_io::crypto::secp256k1_ecdsa_recover;

/// See <https://eips.ethereum.org/EIPS/eip-155>
impl Encodable for TransactionLegacyUnsigned {
	fn rlp_append(&self, s: &mut rlp::RlpStream) {
		if let Some(chain_id) = self.chain_id {
			s.begin_list(9);
			s.append(&self.nonce);
			s.append(&self.gas_price);
			s.append(&self.gas);
			match self.to {
				Some(ref to) => s.append(to),
				None => s.append_empty_data(),
			};
			s.append(&self.value);
			s.append(&self.input.0);
			s.append(&chain_id);
			s.append(&0_u8);
			s.append(&0_u8);
		} else {
			s.begin_list(6);
			s.append(&self.nonce);
			s.append(&self.gas_price);
			s.append(&self.gas);
			match self.to {
				Some(ref to) => s.append(to),
				None => s.append_empty_data(),
			};
			s.append(&self.value);
			s.append(&self.input.0);
		}
	}
}

/// See <https://eips.ethereum.org/EIPS/eip-155>
impl Decodable for TransactionLegacyUnsigned {
	fn decode(rlp: &rlp::Rlp) -> Result<Self, rlp::DecoderError> {
		Ok(TransactionLegacyUnsigned {
			nonce: rlp.val_at(0)?,
			gas_price: rlp.val_at(1)?,
			gas: rlp.val_at(2)?,
			to: {
				let to = rlp.at(3)?;
				if to.is_empty() {
					None
				} else {
					Some(to.as_val()?)
				}
			},
			value: rlp.val_at(4)?,
			input: Bytes(rlp.val_at(5)?),
			chain_id: {
				if let Ok(chain_id) = rlp.val_at(6) {
					Some(chain_id)
				} else {
					None
				}
			},
			..Default::default()
		})
	}
}

impl Encodable for TransactionLegacySigned {
	fn rlp_append(&self, s: &mut rlp::RlpStream) {
		s.begin_list(9);
		s.append(&self.transaction_legacy_unsigned.nonce);
		s.append(&self.transaction_legacy_unsigned.gas_price);
		s.append(&self.transaction_legacy_unsigned.gas);
		match self.transaction_legacy_unsigned.to {
			Some(ref to) => s.append(to),
			None => s.append_empty_data(),
		};
		s.append(&self.transaction_legacy_unsigned.value);
		s.append(&self.transaction_legacy_unsigned.input.0);

		s.append(&self.v);
		s.append(&self.r);
		s.append(&self.s);
	}
}

/// See <https://eips.ethereum.org/EIPS/eip-155>
impl Decodable for TransactionLegacySigned {
	fn decode(rlp: &rlp::Rlp) -> Result<Self, rlp::DecoderError> {
		let v: U256 = rlp.val_at(6)?;
		let extract_chain_id = |v: u64| {
			if v >= 35 {
				Some((v - 35) / 2)
			} else {
				None
			}
		};

		Ok(TransactionLegacySigned {
			transaction_legacy_unsigned: {
				TransactionLegacyUnsigned {
					nonce: rlp.val_at(0)?,
					gas_price: rlp.val_at(1)?,
					gas: rlp.val_at(2)?,
					to: {
						let to = rlp.at(3)?;
						if to.is_empty() {
							None
						} else {
							Some(to.as_val()?)
						}
					},
					value: rlp.val_at(4)?,
					input: Bytes(rlp.val_at(5)?),
					chain_id: extract_chain_id(v.as_u64()).map(|v| v.into()),
					r#type: Type0 {},
				}
			},
			v,
			r: rlp.val_at(7)?,
			s: rlp.val_at(8)?,
		})
	}
}

/// Recover the signer of a transaction.
pub trait SignerRecovery {
	/// The signature type.
	type Signature;

	/// The signer address type.
	type Signer;

	/// Recover the signer of a transaction from its signature.
	fn recover_signer(&self, signature: &Self::Signature) -> Option<Self::Signer>;
}

impl SignerRecovery for TransactionLegacyUnsigned {
	type Signature = sp_core::ecdsa::Signature;
	type Signer = H160;
	fn recover_signer(&self, sig: &Self::Signature) -> Option<Self::Signer> {
		let msg = keccak_256(&rlp::encode(self));
		let pub_key = secp256k1_ecdsa_recover(sig.as_ref(), &msg).ok()?;
		let pub_key_hash = keccak_256(&pub_key);
		Some(H160::from_slice(&pub_key_hash[12..]))
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use core::str::FromStr;
	use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};

	struct Account {
		sk: SecretKey,
	}

	impl Default for Account {
		fn default() -> Self {
			Account {
				sk: SecretKey::from_str(
					"a872f6cbd25a0e04a08b1e21098017a9e6194d101d75e13111f71410c59cd57f",
				)
				.unwrap(),
			}
		}
	}

	impl Account {
		fn address(&self) -> H160 {
			let pub_key =
				PublicKey::from_secret_key(&Secp256k1::new(), &self.sk).serialize_uncompressed();
			let hash = keccak_256(&pub_key[1..]);
			H160::from_slice(&hash[12..])
		}

		fn sign_transaction(&self, tx: TransactionLegacyUnsigned) -> TransactionLegacySigned {
			let rlp_encoded = tx.rlp_bytes();
			let tx_hash = keccak_256(&rlp_encoded);
			let secp = Secp256k1::new();
			let msg = Message::from_digest(tx_hash);
			let sig = secp.sign_ecdsa_recoverable(&msg, &self.sk);
			TransactionLegacySigned::from(tx, sig)
		}
	}

	#[test]
	fn encode_decode_legacy_transaction_works() {
		let tx = TransactionLegacyUnsigned {
			chain_id: Some(596.into()),
			gas: U256::from(21000),
			nonce: U256::from(1),
			gas_price: U256::from("0x640000006a"),
			to: Some(H160::from_str("0x1111111111222222222233333333334444444444").unwrap()),
			value: U256::from(123123),
			input: Bytes(vec![]),
			r#type: Type0,
		};

		let rlp_bytes = rlp::encode(&tx);
		let decoded = rlp::decode::<TransactionLegacyUnsigned>(&rlp_bytes).unwrap();
		dbg!(&decoded);
	}

	#[test]
	fn recover_address_works() {
		let account = Account::default();

		let unsigned_tx = TransactionLegacyUnsigned {
			value: 200_000_000_000_000_000_000u128.into(),
			gas_price: 100_000_000_200u64.into(),
			gas: 100_107u32.into(),
			nonce: 3.into(),
			to: Some(H160::from_str("75e480db528101a381ce68544611c169ad7eb342").unwrap()),
			chain_id: Some(596.into()),
			..Default::default()
		};

		let tx = account.sign_transaction(unsigned_tx.clone());
		let recovered_address = tx.recover_eth_address().unwrap();

		assert_eq!(account.address(), recovered_address);
	}
}
