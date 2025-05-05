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
//! Utility impl for the RPC types.
use super::*;
use alloc::vec::Vec;
use sp_core::{H160, U256};

impl From<BlockNumberOrTag> for BlockNumberOrTagOrHash {
	fn from(b: BlockNumberOrTag) -> Self {
		match b {
			BlockNumberOrTag::U256(n) => BlockNumberOrTagOrHash::U256(n),
			BlockNumberOrTag::BlockTag(t) => BlockNumberOrTagOrHash::BlockTag(t),
		}
	}
}

impl From<TransactionSigned> for TransactionUnsigned {
	fn from(tx: TransactionSigned) -> Self {
		use TransactionSigned::*;
		match tx {
			Transaction4844Signed(tx) => tx.transaction_4844_unsigned.into(),
			Transaction1559Signed(tx) => tx.transaction_1559_unsigned.into(),
			Transaction2930Signed(tx) => tx.transaction_2930_unsigned.into(),
			TransactionLegacySigned(tx) => tx.transaction_legacy_unsigned.into(),
		}
	}
}

impl TransactionInfo {
	/// Create a new [`TransactionInfo`] from a receipt and a signed transaction.
	pub fn new(receipt: ReceiptInfo, transaction_signed: TransactionSigned) -> Self {
		Self {
			block_hash: receipt.block_hash,
			block_number: receipt.block_number,
			from: receipt.from,
			hash: receipt.transaction_hash,
			transaction_index: receipt.transaction_index,
			transaction_signed,
		}
	}
}

impl ReceiptInfo {
	/// Initialize a new Receipt
	pub fn new(
		block_hash: H256,
		block_number: U256,
		contract_address: Option<Address>,
		from: Address,
		logs: Vec<Log>,
		to: Option<Address>,
		effective_gas_price: U256,
		gas_used: U256,
		success: bool,
		transaction_hash: H256,
		transaction_index: U256,
		r#type: Byte,
	) -> Self {
		let logs_bloom = Self::logs_bloom(&logs);
		ReceiptInfo {
			block_hash,
			block_number,
			contract_address,
			from,
			logs,
			logs_bloom,
			to,
			effective_gas_price,
			gas_used,
			status: Some(if success { U256::one() } else { U256::zero() }),
			transaction_hash,
			transaction_index,
			r#type: Some(r#type),
			..Default::default()
		}
	}

	/// Returns `true` if the transaction was successful.
	pub fn is_success(&self) -> bool {
		self.status.map_or(false, |status| status == U256::one())
	}

	/// Calculate receipt logs bloom.
	fn logs_bloom(logs: &[Log]) -> Bytes256 {
		let mut bloom = [0u8; 256];
		for log in logs {
			m3_2048(&mut bloom, &log.address.as_ref());
			for topic in &log.topics {
				m3_2048(&mut bloom, topic.as_ref());
			}
		}
		bloom.into()
	}
}
/// Specialised Bloom filter that sets three bits out of 2048, given an
/// arbitrary byte sequence.
///
/// See Section 4.4.1 "Transaction Receipt" of the [Ethereum Yellow Paper][ref].
///
/// [ref]: https://ethereum.github.io/yellowpaper/paper.pdf
fn m3_2048(bloom: &mut [u8; 256], bytes: &[u8]) {
	let hash = sp_core::keccak_256(bytes);
	for i in [0, 2, 4] {
		let bit = (hash[i + 1] as usize + ((hash[i] as usize) << 8)) & 0x7FF;
		bloom[256 - 1 - bit / 8] |= 1 << (bit % 8);
	}
}

#[test]
fn can_deserialize_input_or_data_field_from_generic_transaction() {
	let cases = [
		("with input", r#"{"input": "0x01"}"#),
		("with data", r#"{"data": "0x01"}"#),
		("with both", r#"{"data": "0x01", "input": "0x01"}"#),
	];

	for (name, json) in cases {
		let tx = serde_json::from_str::<GenericTransaction>(json).unwrap();
		assert_eq!(tx.input.to_vec(), vec![1u8], "{}", name);
	}

	let err = serde_json::from_str::<GenericTransaction>(r#"{"data": "0x02", "input": "0x01"}"#)
		.unwrap_err();
	assert!(
		err.to_string().starts_with(
		"Both \"data\" and \"input\" are set and not equal. Please use \"input\" to pass transaction call data"
		)
	);
}

#[test]
fn logs_bloom_works() {
	let receipt: ReceiptInfo = serde_json::from_str(
		r#"
		{
			"blockHash": "0x835ee379aaabf4802a22a93ad8164c02bbdde2cc03d4552d5c642faf4e09d1f3",
			"blockNumber": "0x2",
			"contractAddress": null,
			"cumulativeGasUsed": "0x5d92",
			"effectiveGasPrice": "0x2dcd5c2d",
			"from": "0xb4f1f9ecfe5a28633a27f57300bda217e99b8969",
			"gasUsed": "0x5d92",
			"logs": [
				{
				"address": "0x82bdb002b9b1f36c42df15fbdc6886abcb2ab31d",
				"topics": [
					"0x1585375487296ff2f0370daeec4214074a032b31af827c12622fa9a58c16c7d0",
					"0x000000000000000000000000b4f1f9ecfe5a28633a27f57300bda217e99b8969"
				],
				"data": "0x00000000000000000000000000000000000000000000000000000000000030390000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000000b48656c6c6f20776f726c64000000000000000000000000000000000000000000",
				"blockNumber": "0x2",
				"transactionHash": "0xad0075127962bdf73d787f2944bdb5f351876f23c35e6a48c1f5b6463a100af4",
				"transactionIndex": "0x0",
				"blockHash": "0x835ee379aaabf4802a22a93ad8164c02bbdde2cc03d4552d5c642faf4e09d1f3",
				"logIndex": "0x0",
				"removed": false
				}
			],
			"logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000400000008000000000000000000000000000000000000000000000000800000000040000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000004000000000000000800000000000000000080000000000000000000000000000000000000000000",
			"status": "0x1",
			"to": "0x82bdb002b9b1f36c42df15fbdc6886abcb2ab31d",
			"transactionHash": "0xad0075127962bdf73d787f2944bdb5f351876f23c35e6a48c1f5b6463a100af4",
			"transactionIndex": "0x0",
			"type": "0x2"
		}
		"#,
	)
	.unwrap();
	assert_eq!(receipt.logs_bloom, ReceiptInfo::logs_bloom(&receipt.logs));
}

impl GenericTransaction {
	/// Create a new [`GenericTransaction`] from a signed transaction.
	pub fn from_signed(tx: TransactionSigned, base_gas_price: U256, from: Option<H160>) -> Self {
		Self::from_unsigned(tx.into(), base_gas_price, from)
	}

	/// Create a new [`GenericTransaction`] from a unsigned transaction.
	pub fn from_unsigned(
		tx: TransactionUnsigned,
		base_gas_price: U256,
		from: Option<H160>,
	) -> Self {
		use TransactionUnsigned::*;
		match tx {
			TransactionLegacyUnsigned(tx) => GenericTransaction {
				from,
				r#type: Some(tx.r#type.as_byte()),
				chain_id: tx.chain_id,
				input: tx.input.into(),
				nonce: Some(tx.nonce),
				value: Some(tx.value),
				to: tx.to,
				gas: Some(tx.gas),
				gas_price: Some(tx.gas_price),
				..Default::default()
			},
			Transaction4844Unsigned(tx) => GenericTransaction {
				from,
				r#type: Some(tx.r#type.as_byte()),
				chain_id: Some(tx.chain_id),
				input: tx.input.into(),
				nonce: Some(tx.nonce),
				value: Some(tx.value),
				to: Some(tx.to),
				gas: Some(tx.gas),
				gas_price: Some(
					base_gas_price
						.saturating_add(tx.max_priority_fee_per_gas)
						.min(tx.max_fee_per_blob_gas),
				),
				access_list: Some(tx.access_list),
				blob_versioned_hashes: tx.blob_versioned_hashes,
				max_fee_per_blob_gas: Some(tx.max_fee_per_blob_gas),
				max_fee_per_gas: Some(tx.max_fee_per_gas),
				max_priority_fee_per_gas: Some(tx.max_priority_fee_per_gas),
				..Default::default()
			},
			Transaction1559Unsigned(tx) => GenericTransaction {
				from,
				r#type: Some(tx.r#type.as_byte()),
				chain_id: Some(tx.chain_id),
				input: tx.input.into(),
				nonce: Some(tx.nonce),
				value: Some(tx.value),
				to: tx.to,
				gas: Some(tx.gas),
				gas_price: Some(
					base_gas_price
						.saturating_add(tx.max_priority_fee_per_gas)
						.min(tx.max_fee_per_gas),
				),
				access_list: Some(tx.access_list),
				max_fee_per_gas: Some(tx.max_fee_per_gas),
				max_priority_fee_per_gas: Some(tx.max_priority_fee_per_gas),
				..Default::default()
			},
			Transaction2930Unsigned(tx) => GenericTransaction {
				from,
				r#type: Some(tx.r#type.as_byte()),
				chain_id: Some(tx.chain_id),
				input: tx.input.into(),
				nonce: Some(tx.nonce),
				value: Some(tx.value),
				to: tx.to,
				gas: Some(tx.gas),
				gas_price: Some(tx.gas_price),
				access_list: Some(tx.access_list),
				..Default::default()
			},
		}
	}

	/// Convert to a [`TransactionUnsigned`].
	pub fn try_into_unsigned(self) -> Result<TransactionUnsigned, ()> {
		match self.r#type.unwrap_or_default().0 {
			TYPE_LEGACY => Ok(TransactionLegacyUnsigned {
				r#type: TypeLegacy {},
				chain_id: self.chain_id,
				input: self.input.to_bytes(),
				nonce: self.nonce.unwrap_or_default(),
				value: self.value.unwrap_or_default(),
				to: self.to,
				gas: self.gas.unwrap_or_default(),
				gas_price: self.gas_price.unwrap_or_default(),
			}
			.into()),
			TYPE_EIP1559 => Ok(Transaction1559Unsigned {
				r#type: TypeEip1559 {},
				chain_id: self.chain_id.unwrap_or_default(),
				input: self.input.to_bytes(),
				nonce: self.nonce.unwrap_or_default(),
				value: self.value.unwrap_or_default(),
				to: self.to,
				gas: self.gas.unwrap_or_default(),
				gas_price: self.max_fee_per_gas.unwrap_or_default(),
				access_list: self.access_list.unwrap_or_default(),
				max_fee_per_gas: self.max_fee_per_gas.unwrap_or_default(),
				max_priority_fee_per_gas: self.max_priority_fee_per_gas.unwrap_or_default(),
			}
			.into()),
			TYPE_EIP2930 => Ok(Transaction2930Unsigned {
				r#type: TypeEip2930 {},
				chain_id: self.chain_id.unwrap_or_default(),
				input: self.input.to_bytes(),
				nonce: self.nonce.unwrap_or_default(),
				value: self.value.unwrap_or_default(),
				to: self.to,
				gas: self.gas.unwrap_or_default(),
				gas_price: self.gas_price.unwrap_or_default(),
				access_list: self.access_list.unwrap_or_default(),
			}
			.into()),
			TYPE_EIP4844 => Ok(Transaction4844Unsigned {
				r#type: TypeEip4844 {},
				chain_id: self.chain_id.unwrap_or_default(),
				input: self.input.to_bytes(),
				nonce: self.nonce.unwrap_or_default(),
				value: self.value.unwrap_or_default(),
				to: self.to.unwrap_or_default(),
				gas: self.gas.unwrap_or_default(),
				max_fee_per_gas: self.max_fee_per_gas.unwrap_or_default(),
				max_fee_per_blob_gas: self.max_fee_per_blob_gas.unwrap_or_default(),
				max_priority_fee_per_gas: self.max_priority_fee_per_gas.unwrap_or_default(),
				access_list: self.access_list.unwrap_or_default(),
				blob_versioned_hashes: self.blob_versioned_hashes,
			}
			.into()),
			_ => Err(()),
		}
	}
}

#[test]
fn from_unsigned_works_for_legacy() {
	let base_gas_price = U256::from(10);
	let tx = TransactionUnsigned::from(TransactionLegacyUnsigned {
		chain_id: Some(U256::from(1)),
		input: Bytes::from(vec![1u8]),
		nonce: U256::from(1),
		value: U256::from(1),
		to: Some(H160::zero()),
		gas: U256::from(1),
		gas_price: U256::from(11),
		..Default::default()
	});

	let generic = GenericTransaction::from_unsigned(tx.clone(), base_gas_price, None);
	assert_eq!(generic.gas_price, Some(U256::from(11)));

	let tx2 = generic.try_into_unsigned().unwrap();
	assert_eq!(tx, tx2);
}

#[test]
fn from_unsigned_works_for_1559() {
	let base_gas_price = U256::from(10);
	let tx = TransactionUnsigned::from(Transaction1559Unsigned {
		chain_id: U256::from(1),
		input: Bytes::from(vec![1u8]),
		nonce: U256::from(1),
		value: U256::from(1),
		to: Some(H160::zero()),
		gas: U256::from(1),
		gas_price: U256::from(20),
		max_fee_per_gas: U256::from(20),
		max_priority_fee_per_gas: U256::from(1),
		..Default::default()
	});

	let generic = GenericTransaction::from_unsigned(tx.clone(), base_gas_price, None);
	assert_eq!(generic.gas_price, Some(U256::from(11)));

	let tx2 = generic.try_into_unsigned().unwrap();
	assert_eq!(tx, tx2);
}
