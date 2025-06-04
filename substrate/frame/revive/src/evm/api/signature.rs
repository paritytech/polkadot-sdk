// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//! Ethereum signature utilities

use super::*;
use sp_core::{H160, U256};
use sp_io::{crypto::secp256k1_ecdsa_recover, hashing::keccak_256};

impl TransactionLegacySigned {
	/// Get the recovery ID from the signed transaction.
	/// See https://eips.ethereum.org/EIPS/eip-155
	fn extract_recovery_id(&self) -> Option<u8> {
		if let Some(chain_id) = self.transaction_legacy_unsigned.chain_id {
			// self.v - chain_id * 2 - 35
			let v: u64 = self.v.try_into().ok()?;
			let chain_id: u64 = chain_id.try_into().ok()?;
			let r = v.checked_sub(chain_id.checked_mul(2)?)?.checked_sub(35)?;
			r.try_into().ok()
		} else {
			self.v.try_into().ok()
		}
	}
}

impl TransactionUnsigned {
	/// Extract the unsigned transaction from a signed transaction.
	pub fn from_signed(tx: TransactionSigned) -> Self {
		match tx {
			TransactionSigned::TransactionLegacySigned(signed) =>
				Self::TransactionLegacyUnsigned(signed.transaction_legacy_unsigned),
			TransactionSigned::Transaction4844Signed(signed) =>
				Self::Transaction4844Unsigned(signed.transaction_4844_unsigned),
			TransactionSigned::Transaction1559Signed(signed) =>
				Self::Transaction1559Unsigned(signed.transaction_1559_unsigned),
			TransactionSigned::Transaction2930Signed(signed) =>
				Self::Transaction2930Unsigned(signed.transaction_2930_unsigned),
		}
	}

	/// Create a signed transaction from an [`TransactionUnsigned`] and a signature.
	pub fn with_signature(self, signature: [u8; 65]) -> TransactionSigned {
		let r = U256::from_big_endian(&signature[..32]);
		let s = U256::from_big_endian(&signature[32..64]);
		let recovery_id = signature[64];

		match self {
			TransactionUnsigned::Transaction2930Unsigned(transaction_2930_unsigned) =>
				Transaction2930Signed {
					transaction_2930_unsigned,
					r,
					s,
					v: None,
					y_parity: U256::from(recovery_id),
				}
				.into(),
			TransactionUnsigned::Transaction1559Unsigned(transaction_1559_unsigned) =>
				Transaction1559Signed {
					transaction_1559_unsigned,
					r,
					s,
					v: None,
					y_parity: U256::from(recovery_id),
				}
				.into(),

			TransactionUnsigned::Transaction4844Unsigned(transaction_4844_unsigned) =>
				Transaction4844Signed {
					transaction_4844_unsigned,
					r,
					s,
					y_parity: U256::from(recovery_id),
				}
				.into(),

			TransactionUnsigned::TransactionLegacyUnsigned(transaction_legacy_unsigned) => {
				let v = transaction_legacy_unsigned
					.chain_id
					.map(|chain_id| {
						chain_id
							.saturating_mul(U256::from(2))
							.saturating_add(U256::from(35u32 + recovery_id as u32))
					})
					.unwrap_or_else(|| U256::from(27u32 + recovery_id as u32));

				TransactionLegacySigned { transaction_legacy_unsigned, r, s, v }.into()
			},
		}
	}
}

impl TransactionSigned {
	/// Get the raw 65 bytes signature from the signed transaction.
	pub fn raw_signature(&self) -> Result<[u8; 65], ()> {
		use TransactionSigned::*;
		let (r, s, v) = match self {
			TransactionLegacySigned(tx) => (tx.r, tx.s, tx.extract_recovery_id().ok_or(())?),
			Transaction4844Signed(tx) => (tx.r, tx.s, tx.y_parity.try_into().map_err(|_| ())?),
			Transaction1559Signed(tx) => (tx.r, tx.s, tx.y_parity.try_into().map_err(|_| ())?),
			Transaction2930Signed(tx) => (tx.r, tx.s, tx.y_parity.try_into().map_err(|_| ())?),
		};
		let mut sig = [0u8; 65];
		r.write_as_big_endian(sig[0..32].as_mut());
		s.write_as_big_endian(sig[32..64].as_mut());
		sig[64] = v;
		Ok(sig)
	}

	/// Recover the Ethereum address, from a signed transaction.
	pub fn recover_eth_address(&self) -> Result<H160, ()> {
		use TransactionSigned::*;

		let mut s = rlp::RlpStream::new();
		match self {
			TransactionLegacySigned(tx) => {
				let tx = &tx.transaction_legacy_unsigned;
				s.append(tx);
			},
			Transaction4844Signed(tx) => {
				let tx = &tx.transaction_4844_unsigned;
				s.append(&tx.r#type.value());
				s.append(tx);
			},
			Transaction1559Signed(tx) => {
				let tx = &tx.transaction_1559_unsigned;
				s.append(&tx.r#type.value());
				s.append(tx);
			},
			Transaction2930Signed(tx) => {
				let tx = &tx.transaction_2930_unsigned;
				s.append(&tx.r#type.value());
				s.append(tx);
			},
		}
		let bytes = s.out().to_vec();
		let signature = self.raw_signature()?;

		let hash = keccak_256(&bytes);
		let mut addr = H160::default();
		let pk = secp256k1_ecdsa_recover(&signature, &hash).map_err(|_| ())?;
		addr.assign_from_slice(&keccak_256(&pk[..])[12..]);
		Ok(addr)
	}
}

#[test]
fn sign_and_recover_work() {
	use crate::evm::TransactionUnsigned;
	let txs = [
		// Legacy
		"f86080808301e24194095e7baea6a6c7c4c2dfeb977efac326af552d87808026a07b2e762a17a71a46b422e60890a04512cf0d907ccf6b78b5bd6e6977efdc2bf5a01ea673d50bbe7c2236acb498ceb8346a8607c941f0b8cbcde7cf439aa9369f1f",
		//// type 1: EIP2930
		"01f89b0180808301e24194095e7baea6a6c7c4c2dfeb977efac326af552d878080f838f7940000000000000000000000000000000000000001e1a0000000000000000000000000000000000000000000000000000000000000000080a0c45a61b3d1d00169c649e7326e02857b850efb96e587db4b9aad29afc80d0752a070ae1eb47ab4097dbed2f19172ae286492621b46ac737ee6c32fb18a00c94c9c",
		// type 2: EIP1559
		"02f89c018080018301e24194095e7baea6a6c7c4c2dfeb977efac326af552d878080f838f7940000000000000000000000000000000000000001e1a0000000000000000000000000000000000000000000000000000000000000000080a055d72bbc3047d4b9d3e4b8099f187143202407746118204cc2e0cb0c85a68baea04f6ef08a1418c70450f53398d9f0f2d78d9e9d6b8a80cba886b67132c4a744f2",
		// type 3: EIP4844
		"03f8bf018002018301e24194095e7baea6a6c7c4c2dfeb977efac326af552d878080f838f7940000000000000000000000000000000000000001e1a0000000000000000000000000000000000000000000000000000000000000000080e1a0000000000000000000000000000000000000000000000000000000000000000001a0672b8bac466e2cf1be3148c030988d40d582763ecebbc07700dfc93bb070d8a4a07c635887005b11cb58964c04669ac2857fa633aa66f662685dadfd8bcacb0f21",
	];
	let account = Account::from_secret_key(hex_literal::hex!(
		"a872f6cbd25a0e04a08b1e21098017a9e6194d101d75e13111f71410c59cd57f"
	));

	for tx in txs {
		let raw_tx = alloy_core::hex::decode(tx).unwrap();
		let tx = TransactionSigned::decode(&raw_tx).unwrap();

		let address = tx.recover_eth_address();
		assert_eq!(address.unwrap(), account.address());

		let unsigned = TransactionUnsigned::from_signed(tx.clone());
		let signed = account.sign_transaction(unsigned);
		assert_eq!(tx, signed);
	}
}
