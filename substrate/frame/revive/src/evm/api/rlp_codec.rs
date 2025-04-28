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

impl TransactionUnsigned {
	/// Return the bytes to be signed by the private key.
	pub fn unsigned_payload(&self) -> Vec<u8> {
		use TransactionUnsigned::*;
		let mut s = rlp::RlpStream::new();
		match self {
			Transaction2930Unsigned(ref tx) => {
				s.append(&tx.r#type.value());
				s.append(tx);
			},
			Transaction1559Unsigned(ref tx) => {
				s.append(&tx.r#type.value());
				s.append(tx);
			},
			Transaction4844Unsigned(ref tx) => {
				s.append(&tx.r#type.value());
				s.append(tx);
			},
			TransactionLegacyUnsigned(ref tx) => {
				s.append(tx);
			},
		}

		s.out().to_vec()
	}
}

impl TransactionSigned {
	/// Extract the unsigned transaction from a signed transaction.
	pub fn unsigned(self) -> TransactionUnsigned {
		use TransactionSigned::*;
		use TransactionUnsigned::*;
		match self {
			Transaction2930Signed(tx) => Transaction2930Unsigned(tx.transaction_2930_unsigned),
			Transaction1559Signed(tx) => Transaction1559Unsigned(tx.transaction_1559_unsigned),
			Transaction4844Signed(tx) => Transaction4844Unsigned(tx.transaction_4844_unsigned),
			TransactionLegacySigned(tx) =>
				TransactionLegacyUnsigned(tx.transaction_legacy_unsigned),
		}
	}

	/// Encode the Ethereum transaction into bytes.
	pub fn signed_payload(&self) -> Vec<u8> {
		use TransactionSigned::*;
		let mut s = rlp::RlpStream::new();
		match self {
			Transaction2930Signed(ref tx) => {
				s.append(&tx.transaction_2930_unsigned.r#type.value());
				s.append(tx);
			},
			Transaction1559Signed(ref tx) => {
				s.append(&tx.transaction_1559_unsigned.r#type.value());
				s.append(tx);
			},
			Transaction4844Signed(ref tx) => {
				s.append(&tx.transaction_4844_unsigned.r#type.value());
				s.append(tx);
			},
			TransactionLegacySigned(ref tx) => {
				s.append(tx);
			},
		}

		s.out().to_vec()
	}

	/// Decode the Ethereum transaction from bytes.
	pub fn decode(data: &[u8]) -> Result<Self, rlp::DecoderError> {
		if data.len() < 1 {
			return Err(rlp::DecoderError::RlpIsTooShort);
		}
		match data[0] {
			TYPE_EIP2930 => rlp::decode::<Transaction2930Signed>(&data[1..]).map(Into::into),
			TYPE_EIP1559 => rlp::decode::<Transaction1559Signed>(&data[1..]).map(Into::into),
			TYPE_EIP4844 => rlp::decode::<Transaction4844Signed>(&data[1..]).map(Into::into),
			_ => rlp::decode::<TransactionLegacySigned>(data).map(Into::into),
		}
	}
}

impl TransactionUnsigned {
	/// Get a signed transaction payload with a dummy 65 bytes signature.
	pub fn dummy_signed_payload(self) -> Vec<u8> {
		const DUMMY_SIGNATURE: [u8; 65] = [1u8; 65];
		self.with_signature(DUMMY_SIGNATURE).signed_payload()
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
			s.append(&0u8);
			s.append(&0u8);
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
			chain_id: rlp.val_at(6).ok(),
			..Default::default()
		})
	}
}

impl Encodable for TransactionLegacySigned {
	fn rlp_append(&self, s: &mut rlp::RlpStream) {
		let tx = &self.transaction_legacy_unsigned;

		s.begin_list(9);
		s.append(&tx.nonce);
		s.append(&tx.gas_price);
		s.append(&tx.gas);
		match tx.to {
			Some(ref to) => s.append(to),
			None => s.append_empty_data(),
		};
		s.append(&tx.value);
		s.append(&tx.input.0);

		s.append(&self.v);
		s.append(&self.r);
		s.append(&self.s);
	}
}

impl Encodable for AccessListEntry {
	fn rlp_append(&self, s: &mut rlp::RlpStream) {
		s.begin_list(2);
		s.append(&self.address);
		s.append_list(&self.storage_keys);
	}
}

impl Decodable for AccessListEntry {
	fn decode(rlp: &rlp::Rlp) -> Result<Self, rlp::DecoderError> {
		Ok(AccessListEntry { address: rlp.val_at(0)?, storage_keys: rlp.list_at(1)? })
	}
}

/// See <https://eips.ethereum.org/EIPS/eip-1559>
impl Encodable for Transaction1559Unsigned {
	fn rlp_append(&self, s: &mut rlp::RlpStream) {
		s.begin_list(9);
		s.append(&self.chain_id);
		s.append(&self.nonce);
		s.append(&self.max_priority_fee_per_gas);
		s.append(&self.max_fee_per_gas);
		s.append(&self.gas);
		match self.to {
			Some(ref to) => s.append(to),
			None => s.append_empty_data(),
		};
		s.append(&self.value);
		s.append(&self.input.0);
		s.append_list(&self.access_list);
	}
}

/// See <https://eips.ethereum.org/EIPS/eip-1559>
impl Encodable for Transaction1559Signed {
	fn rlp_append(&self, s: &mut rlp::RlpStream) {
		let tx = &self.transaction_1559_unsigned;
		s.begin_list(12);
		s.append(&tx.chain_id);
		s.append(&tx.nonce);
		s.append(&tx.max_priority_fee_per_gas);
		s.append(&tx.max_fee_per_gas);
		s.append(&tx.gas);
		match tx.to {
			Some(ref to) => s.append(to),
			None => s.append_empty_data(),
		};
		s.append(&tx.value);
		s.append(&tx.input.0);
		s.append_list(&tx.access_list);

		s.append(&self.y_parity);
		s.append(&self.r);
		s.append(&self.s);
	}
}

impl Decodable for Transaction1559Signed {
	fn decode(rlp: &rlp::Rlp) -> Result<Self, rlp::DecoderError> {
		Ok(Transaction1559Signed {
			transaction_1559_unsigned: {
				Transaction1559Unsigned {
					chain_id: rlp.val_at(0)?,
					nonce: rlp.val_at(1)?,
					max_priority_fee_per_gas: rlp.val_at(2)?,
					max_fee_per_gas: rlp.val_at(3)?,
					gas: rlp.val_at(4)?,
					to: {
						let to = rlp.at(5)?;
						if to.is_empty() {
							None
						} else {
							Some(to.as_val()?)
						}
					},
					value: rlp.val_at(6)?,
					input: Bytes(rlp.val_at(7)?),
					access_list: rlp.list_at(8)?,
					..Default::default()
				}
			},
			y_parity: rlp.val_at(9)?,
			r: rlp.val_at(10)?,
			s: rlp.val_at(11)?,
			..Default::default()
		})
	}
}

//See https://eips.ethereum.org/EIPS/eip-2930
impl Encodable for Transaction2930Unsigned {
	fn rlp_append(&self, s: &mut rlp::RlpStream) {
		s.begin_list(8);
		s.append(&self.chain_id);
		s.append(&self.nonce);
		s.append(&self.gas_price);
		s.append(&self.gas);
		match self.to {
			Some(ref to) => s.append(to),
			None => s.append_empty_data(),
		};
		s.append(&self.value);
		s.append(&self.input.0);
		s.append_list(&self.access_list);
	}
}

//See https://eips.ethereum.org/EIPS/eip-2930
impl Encodable for Transaction2930Signed {
	fn rlp_append(&self, s: &mut rlp::RlpStream) {
		let tx = &self.transaction_2930_unsigned;
		s.begin_list(11);
		s.append(&tx.chain_id);
		s.append(&tx.nonce);
		s.append(&tx.gas_price);
		s.append(&tx.gas);
		match tx.to {
			Some(ref to) => s.append(to),
			None => s.append_empty_data(),
		};
		s.append(&tx.value);
		s.append(&tx.input.0);
		s.append_list(&tx.access_list);
		s.append(&self.y_parity);
		s.append(&self.r);
		s.append(&self.s);
	}
}

impl Decodable for Transaction2930Signed {
	fn decode(rlp: &rlp::Rlp) -> Result<Self, rlp::DecoderError> {
		Ok(Transaction2930Signed {
			transaction_2930_unsigned: {
				Transaction2930Unsigned {
					chain_id: rlp.val_at(0)?,
					nonce: rlp.val_at(1)?,
					gas_price: rlp.val_at(2)?,
					gas: rlp.val_at(3)?,
					to: {
						let to = rlp.at(4)?;
						if to.is_empty() {
							None
						} else {
							Some(to.as_val()?)
						}
					},
					value: rlp.val_at(5)?,
					input: Bytes(rlp.val_at(6)?),
					access_list: rlp.list_at(7)?,
					..Default::default()
				}
			},
			y_parity: rlp.val_at(8)?,
			r: rlp.val_at(9)?,
			s: rlp.val_at(10)?,
			..Default::default()
		})
	}
}

//See https://eips.ethereum.org/EIPS/eip-4844
impl Encodable for Transaction4844Unsigned {
	fn rlp_append(&self, s: &mut rlp::RlpStream) {
		s.begin_list(11);
		s.append(&self.chain_id);
		s.append(&self.nonce);
		s.append(&self.max_priority_fee_per_gas);
		s.append(&self.max_fee_per_gas);
		s.append(&self.gas);
		s.append(&self.to);
		s.append(&self.value);
		s.append(&self.input.0);
		s.append_list(&self.access_list);
		s.append(&self.max_fee_per_blob_gas);
		s.append_list(&self.blob_versioned_hashes);
	}
}

//See https://eips.ethereum.org/EIPS/eip-4844
impl Encodable for Transaction4844Signed {
	fn rlp_append(&self, s: &mut rlp::RlpStream) {
		let tx = &self.transaction_4844_unsigned;
		s.begin_list(14);
		s.append(&tx.chain_id);
		s.append(&tx.nonce);
		s.append(&tx.max_priority_fee_per_gas);
		s.append(&tx.max_fee_per_gas);
		s.append(&tx.gas);
		s.append(&tx.to);
		s.append(&tx.value);
		s.append(&tx.input.0);
		s.append_list(&tx.access_list);
		s.append(&tx.max_fee_per_blob_gas);
		s.append_list(&tx.blob_versioned_hashes);
		s.append(&self.y_parity);
		s.append(&self.r);
		s.append(&self.s);
	}
}

impl Decodable for Transaction4844Signed {
	fn decode(rlp: &rlp::Rlp) -> Result<Self, rlp::DecoderError> {
		Ok(Transaction4844Signed {
			transaction_4844_unsigned: {
				Transaction4844Unsigned {
					chain_id: rlp.val_at(0)?,
					nonce: rlp.val_at(1)?,
					max_priority_fee_per_gas: rlp.val_at(2)?,
					max_fee_per_gas: rlp.val_at(3)?,
					gas: rlp.val_at(4)?,
					to: rlp.val_at(5)?,
					value: rlp.val_at(6)?,
					input: Bytes(rlp.val_at(7)?),
					access_list: rlp.list_at(8)?,
					max_fee_per_blob_gas: rlp.val_at(9)?,
					blob_versioned_hashes: rlp.list_at(10)?,
					..Default::default()
				}
			},
			y_parity: rlp.val_at(11)?,
			r: rlp.val_at(12)?,
			s: rlp.val_at(13)?,
		})
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
					r#type: TypeLegacy {},
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
	fn encode_decode_tx_works() {
		let txs = [
			// Legacy
			(
				"f86080808301e24194095e7baea6a6c7c4c2dfeb977efac326af552d87808025a0fe38ca4e44a30002ac54af7cf922a6ac2ba11b7d22f548e8ecb3f51f41cb31b0a06de6a5cbae13c0c856e33acf021b51819636cfc009d39eafb9f606d546e305a8",
				r#"
				{
					"chainId": "0x1",
					"gas": "0x1e241",
					"gasPrice": "0x0",
					"input": "0x",
					"nonce": "0x0",
					"to": "0x095e7baea6a6c7c4c2dfeb977efac326af552d87",
					"type": "0x0",
					"value": "0x0",
					"r": "0xfe38ca4e44a30002ac54af7cf922a6ac2ba11b7d22f548e8ecb3f51f41cb31b0",
					"s": "0x6de6a5cbae13c0c856e33acf021b51819636cfc009d39eafb9f606d546e305a8",
					"v": "0x25"
				}
				"#
			),
			// type 1: EIP2930
			(
				"01f89b0180808301e24194095e7baea6a6c7c4c2dfeb977efac326af552d878080f838f7940000000000000000000000000000000000000001e1a0000000000000000000000000000000000000000000000000000000000000000080a0fe38ca4e44a30002ac54af7cf922a6ac2ba11b7d22f548e8ecb3f51f41cb31b0a06de6a5cbae13c0c856e33acf021b51819636cfc009d39eafb9f606d546e305a8",
				r#"
				{
					"accessList": [
						{
						"address": "0x0000000000000000000000000000000000000001",
						"storageKeys": ["0x0000000000000000000000000000000000000000000000000000000000000000"]
						}
					],
					"chainId": "0x1",
					"gas": "0x1e241",
					"gasPrice": "0x0",
					"input": "0x",
					"nonce": "0x0",
					"to": "0x095e7baea6a6c7c4c2dfeb977efac326af552d87",
					"type": "0x1",
					"value": "0x0",
					"r": "0xfe38ca4e44a30002ac54af7cf922a6ac2ba11b7d22f548e8ecb3f51f41cb31b0",
					"s": "0x6de6a5cbae13c0c856e33acf021b51819636cfc009d39eafb9f606d546e305a8",
					"yParity": "0x0"
				}
				"#
			),
			// type 2: EIP1559
			(
				"02f89c018080018301e24194095e7baea6a6c7c4c2dfeb977efac326af552d878080f838f7940000000000000000000000000000000000000001e1a0000000000000000000000000000000000000000000000000000000000000000080a0fe38ca4e44a30002ac54af7cf922a6ac2ba11b7d22f548e8ecb3f51f41cb31b0a06de6a5cbae13c0c856e33acf021b51819636cfc009d39eafb9f606d546e305a8",
				r#"
				{
					"accessList": [
						{
							"address": "0x0000000000000000000000000000000000000001",
							"storageKeys": ["0x0000000000000000000000000000000000000000000000000000000000000000"]
						}
					],
					"chainId": "0x1",
					"gas": "0x1e241",
					"gasPrice": "0x0",
					"input": "0x",
					"maxFeePerGas": "0x1",
					"maxPriorityFeePerGas": "0x0",
					"nonce": "0x0",
					"to": "0x095e7baea6a6c7c4c2dfeb977efac326af552d87",
					"type": "0x2",
					"value": "0x0",
					"r": "0xfe38ca4e44a30002ac54af7cf922a6ac2ba11b7d22f548e8ecb3f51f41cb31b0",
					"s": "0x6de6a5cbae13c0c856e33acf021b51819636cfc009d39eafb9f606d546e305a8",
					"yParity": "0x0"

				}
				"#
			),
			// type 3: EIP4844
			(

				"03f8bf018002018301e24194095e7baea6a6c7c4c2dfeb977efac326af552d878080f838f7940000000000000000000000000000000000000001e1a0000000000000000000000000000000000000000000000000000000000000000080e1a0000000000000000000000000000000000000000000000000000000000000000080a0fe38ca4e44a30002ac54af7cf922a6ac2ba11b7d22f548e8ecb3f51f41cb31b0a06de6a5cbae13c0c856e33acf021b51819636cfc009d39eafb9f606d546e305a8",
				r#"
				{
					"accessList": [
						{
						"address": "0x0000000000000000000000000000000000000001",
						"storageKeys": ["0x0000000000000000000000000000000000000000000000000000000000000000"]
						}
					],
					"blobVersionedHashes": ["0x0000000000000000000000000000000000000000000000000000000000000000"],
					"chainId": "0x1",
					"gas": "0x1e241",
					"input": "0x",
					"maxFeePerBlobGas": "0x0",
					"maxFeePerGas": "0x1",
					"maxPriorityFeePerGas": "0x2",
					"nonce": "0x0",
					"to": "0x095e7baea6a6c7c4c2dfeb977efac326af552d87",
					"type": "0x3",
					"value": "0x0",
					"r": "0xfe38ca4e44a30002ac54af7cf922a6ac2ba11b7d22f548e8ecb3f51f41cb31b0",
					"s": "0x6de6a5cbae13c0c856e33acf021b51819636cfc009d39eafb9f606d546e305a8",
					"yParity": "0x0"
				}
				"#
			)
		];

		for (tx, json) in txs {
			let raw_tx = alloy_core::hex::decode(tx).unwrap();
			let tx = TransactionSigned::decode(&raw_tx).unwrap();
			assert_eq!(tx.signed_payload(), raw_tx);
			let expected_tx = serde_json::from_str(json).unwrap();
			assert_eq!(tx, expected_tx);
		}
	}

	#[test]
	fn dummy_signed_payload_works() {
		let tx: TransactionUnsigned = TransactionLegacyUnsigned {
			chain_id: Some(596.into()),
			gas: U256::from(21000),
			nonce: U256::from(1),
			gas_price: U256::from("0x640000006a"),
			to: Some(Account::from(subxt_signer::eth::dev::baltathar()).address()),
			value: U256::from(123123),
			input: Bytes(vec![]),
			r#type: TypeLegacy,
		}
		.into();

		let dummy_signed_payload = tx.clone().dummy_signed_payload();
		let payload = Account::default().sign_transaction(tx).signed_payload();
		assert_eq!(dummy_signed_payload.len(), payload.len());
	}
}
