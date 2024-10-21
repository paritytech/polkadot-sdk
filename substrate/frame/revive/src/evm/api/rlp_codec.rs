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
//! RLP encoding and decoding for Ethereum transactions.
//! See <https://eth.wiki/fundamentals/rlp> for more information about RLP encoding.

use super::*;
use alloc::vec::Vec;
use rlp::{Decodable, Encodable};

impl TransactionLegacyUnsigned {
	/// Get the rlp encoded bytes of a signed transaction with a dummy 65 bytes signature.
	pub fn dummy_signed_payload(&self) -> Vec<u8> {
		let mut s = rlp::RlpStream::new();
		s.append(self);
		const DUMMY_SIGNATURE: [u8; 65] = [0u8; 65];
		s.append_raw(&DUMMY_SIGNATURE.as_ref(), 1);
		s.out().to_vec()
	}
}

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

		let extract_chain_id = |v: U256| {
			if v.ge(&35u32.into()) {
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
					chain_id: extract_chain_id(v).map(|v| v.into()),
					r#type: Type0 {},
				}
			},
			v,
			r: rlp.val_at(7)?,
			s: rlp.val_at(8)?,
		})
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn encode_decode_legacy_transaction_works() {
		let tx = TransactionLegacyUnsigned {
			chain_id: Some(596.into()),
			gas: U256::from(21000),
			nonce: U256::from(1),
			gas_price: U256::from("0x640000006a"),
			to: Some(Account::from(subxt_signer::eth::dev::baltathar()).address()),
			value: U256::from(123123),
			input: Bytes(vec![]),
			r#type: Type0,
		};

		let rlp_bytes = rlp::encode(&tx);
		let decoded = rlp::decode::<TransactionLegacyUnsigned>(&rlp_bytes).unwrap();
		assert_eq!(&tx, &decoded);

		let tx = Account::default().sign_transaction(tx);
		let rlp_bytes = rlp::encode(&tx);
		let decoded = rlp::decode::<TransactionLegacySigned>(&rlp_bytes).unwrap();
		assert_eq!(&tx, &decoded);
	}

	#[test]
	fn dummy_signed_payload_works() {
		let tx = TransactionLegacyUnsigned {
			chain_id: Some(596.into()),
			gas: U256::from(21000),
			nonce: U256::from(1),
			gas_price: U256::from("0x640000006a"),
			to: Some(Account::from(subxt_signer::eth::dev::baltathar()).address()),
			value: U256::from(123123),
			input: Bytes(vec![]),
			r#type: Type0,
		};

		let signed_tx = Account::default().sign_transaction(tx.clone());
		let rlp_bytes = rlp::encode(&signed_tx);
		assert_eq!(tx.dummy_signed_payload().len(), rlp_bytes.len());
	}

	#[test]
	fn recover_address_works() {
		let account = Account::default();

		let unsigned_tx = TransactionLegacyUnsigned {
			value: 200_000_000_000_000_000_000u128.into(),
			gas_price: 100_000_000_200u64.into(),
			gas: 100_107u32.into(),
			nonce: 3.into(),
			to: Some(Account::from(subxt_signer::eth::dev::baltathar()).address()),
			chain_id: Some(596.into()),
			..Default::default()
		};

		let tx = account.sign_transaction(unsigned_tx.clone());
		let recovered_address = tx.recover_eth_address().unwrap();

		assert_eq!(account.address(), recovered_address);
	}
}
