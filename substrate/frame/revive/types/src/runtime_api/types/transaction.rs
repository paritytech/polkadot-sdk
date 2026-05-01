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

use super::transaction_signed::TransactionSignedV1;
use crate::common::{Byte, Bytes};
use alloc::vec::Vec;
use codec::{Decode, Encode, MaxEncodedLen};
use ethereum_types::{Address, H256, U256};
use pallet_revive_proc_macro::define_versioned_type;
use scale_info::TypeInfo;
use serde::{Deserialize, Deserializer, Serialize};

define_versioned_type! {
	/// Version 1 of transaction input data accepted under either `input` or `data`.
	#[derive(
		Debug,
		Default,
		Clone,
		Serialize,
		Deserialize,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
	)]
	pub struct InputOrDataV1 {
		/// Transaction call data encoded under the preferred `input` key.
		#[serde(skip_serializing_if = "Option::is_none")]
		input: Option<Bytes>,
		/// Transaction call data encoded under the legacy `data` key.
		#[serde(skip_serializing_if = "Option::is_none")]
		data: Option<Bytes>,
	}
}

impl From<Bytes> for InputOrDataV1 {
	fn from(value: Bytes) -> Self {
		Self { input: Some(value), data: None }
	}
}

impl From<Vec<u8>> for InputOrDataV1 {
	fn from(value: Vec<u8>) -> Self {
		Self { input: Some(Bytes(value)), data: None }
	}
}

impl InputOrDataV1 {
	/// Converts the transaction call data into its hex byte wrapper.
	pub fn to_bytes(self) -> Bytes {
		match self {
			Self { input: Some(input), data: _ } => input,
			Self { input: None, data: Some(data) } => data,
			Self { input: None, data: None } => Default::default(),
		}
	}

	/// Converts the transaction call data into raw bytes.
	pub fn to_vec(self) -> Vec<u8> {
		self.to_bytes().0
	}
}

/// Deserializes `input` and `data` aliases while rejecting conflicting values.
fn deserialize_input_or_data<'de, D>(deserializer: D) -> Result<InputOrDataV1, D::Error>
where
	D: Deserializer<'de>,
{
	let value = InputOrDataV1::deserialize(deserializer)?;
	match &value {
		InputOrDataV1 { input: Some(input), data: Some(data) } if input != data => {
			Err(serde::de::Error::custom(
				"Both \"data\" and \"input\" are set and not equal. Please use \"input\" to pass \
				transaction call data",
			))
		},
		InputOrDataV1 { input: _, data: _ } => Ok(value),
	}
}

define_versioned_type! {
	/// Version 1 of a transaction object generic to all Ethereum transaction types.
	#[derive(
		Debug,
		Default,
		Clone,
		Serialize,
		Deserialize,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct GenericTransactionV1 {
		/// EIP-2930 access list.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub access_list: Option<Vec<AccessListEntryV1>>,
		/// List of account code authorizations for EIP-7702.
		#[serde(default, skip_serializing_if = "Vec::is_empty")]
		pub authorization_list: Vec<AuthorizationListEntryV1>,
		/// Versioned blob hashes for EIP-4844 data blobs.
		#[serde(default)]
		pub blob_versioned_hashes: Vec<H256>,
		/// Raw EIP-4844 blob data.
		#[serde(default, skip_serializing_if = "Vec::is_empty")]
		pub blobs: Vec<Bytes>,
		/// Chain ID that this transaction is valid on.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub chain_id: Option<U256>,
		/// Sender address.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub from: Option<Address>,
		/// Gas limit.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub gas: Option<U256>,
		/// Gas price willing to be paid by the sender.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub gas_price: Option<U256>,
		/// Transaction call data.
		#[serde(flatten, deserialize_with = "deserialize_input_or_data")]
		pub input: InputOrDataV1,
		/// Maximum total fee per blob gas.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub max_fee_per_blob_gas: Option<U256>,
		/// Maximum total fee per gas.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub max_fee_per_gas: Option<U256>,
		/// Maximum priority fee per gas.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub max_priority_fee_per_gas: Option<U256>,
		/// Transaction nonce.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub nonce: Option<U256>,
		/// Destination address.
		pub to: Option<Address>,
		/// Transaction type.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub r#type: Option<Byte>,
		/// Transferred value.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub value: Option<U256>,
	}
}

define_versioned_type! {
	/// Version 1 of block transactions represented as hashes or full transaction objects.
	#[derive(
		Debug,
		Clone,
		Serialize,
		Deserialize,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
	)]
	#[serde(untagged)]
	pub enum HashesOrTransactionInfosV1 {
		/// Transaction hashes.
		Hashes(Vec<H256>),
		/// Full transaction information objects.
		TransactionInfos(Vec<TransactionInfoV1>),
	}
}

impl Default for HashesOrTransactionInfosV1 {
	fn default() -> Self {
		HashesOrTransactionInfosV1::Hashes(Default::default())
	}
}

impl HashesOrTransactionInfosV1 {
	/// Pushes a transaction hash if this value stores hashes.
	pub fn push_hash(&mut self, hash: H256) {
		match self {
			HashesOrTransactionInfosV1::Hashes(hashes) => hashes.push(hash),
			HashesOrTransactionInfosV1::TransactionInfos(_) => {},
		}
	}

	/// Returns the number of stored transactions.
	pub fn len(&self) -> usize {
		match self {
			HashesOrTransactionInfosV1::Hashes(values) => values.len(),
			HashesOrTransactionInfosV1::TransactionInfos(values) => values.len(),
		}
	}

	/// Returns whether this value stores no transactions.
	pub fn is_empty(&self) -> bool {
		self.len() == 0
	}

	/// Returns whether the given transaction hash is present.
	pub fn contains_tx(&self, hash: H256) -> bool {
		match self {
			HashesOrTransactionInfosV1::Hashes(hashes) => hashes.iter().any(|item| *item == hash),
			HashesOrTransactionInfosV1::TransactionInfos(transaction_infos) => {
				transaction_infos.iter().any(|info| info.hash == hash)
			},
		}
	}
}

define_versioned_type! {
	/// Version 1 of transaction information embedded in full transaction block responses.
	#[derive(
		Debug,
		Default,
		Clone,
		Serialize,
		Deserialize,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct TransactionInfoV1 {
		/// Block hash.
		pub block_hash: H256,
		/// Block number.
		pub block_number: U256,
		/// Sender address.
		pub from: Address,
		/// Transaction hash.
		pub hash: H256,
		/// Transaction index.
		pub transaction_index: U256,
		/// Signed transaction payload.
		#[serde(flatten)]
		pub transaction_signed: TransactionSignedV1,
	}
}

define_versioned_type! {
	/// Version 1 of an access list entry.
	#[derive(
		Debug,
		Default,
		Clone,
		Serialize,
		Deserialize,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct AccessListEntryV1 {
		/// Address whose storage is accessed.
		pub address: Address,
		/// Storage keys accessed by the transaction.
		pub storage_keys: Vec<H256>,
	}
}

define_versioned_type! {
	/// Version 1 of an authorization list entry for EIP-7702.
	#[derive(
		Debug,
		Default,
		Clone,
		Serialize,
		Deserialize,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		MaxEncodedLen,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct AuthorizationListEntryV1 {
		/// Chain ID that this authorization is valid on.
		pub chain_id: U256,
		/// Address to authorize.
		pub address: Address,
		/// Nonce of the authorization.
		pub nonce: U256,
		/// Y-parity of the signature.
		pub y_parity: U256,
		/// Signature r component.
		pub r: U256,
		/// Signature s component.
		pub s: U256,
	}
}

#[cfg(test)]
mod tests {
	use alloc::{string::ToString, vec};

	use super::*;
	use crate::runtime_api::types::{TransactionSignedV1, TypeEip1559V1};

	#[test]
	fn generic_transaction_deserializes_input_and_data_aliases() {
		// Arrange
		let cases = [
			("with input", r#"{"input": "0x01"}"#),
			("with data", r#"{"data": "0x01"}"#),
			("with both", r#"{"data": "0x01", "input": "0x01"}"#),
		];

		// Act
		let transactions = cases
			.into_iter()
			.map(|(_, json)| serde_json::from_str::<GenericTransactionV1>(json).unwrap())
			.collect::<Vec<_>>();

		// Assert
		for transaction in transactions {
			assert_eq!(transaction.input.to_vec(), vec![1u8]);
		}
	}

	#[test]
	fn generic_transaction_rejects_conflicting_input_and_data_aliases() {
		// Arrange
		let json = r#"{"data": "0x02", "input": "0x01"}"#;

		// Act
		let error = serde_json::from_str::<GenericTransactionV1>(json).unwrap_err();

		// Assert
		assert!(error.to_string().starts_with(
			"Both \"data\" and \"input\" are set and not equal. Please use \"input\" to pass \
			 transaction call data"
		));
	}

	#[test]
	fn derived_deserialize_handles_full_transaction_infos_from_value() {
		// Arrange
		let value = serde_json::json!([{
			"accessList": [{
				"address": "0x9008d19f58aabd9ed0d60971565aa8510560ab41",
				"storageKeys": [
					"0x0000000000000000000000000000000000000000000000000000000000000001"
				]
			}],
			"blockHash": "0xfb8c980d1da1a75e68c2ea4d55cb88d62dedbbb5eaf69df8fe337e9f6922b73a",
			"blockNumber": "0x161bd0f",
			"chainId": "0x1",
			"from": "0x4838b106fce9647bdf1e7877bf73ce8b0bad5f97",
			"gas": "0x565f",
			"gasPrice": "0x23cf3fd4",
			"hash": "0x2c522d01183e9ed70caaf75c940ba9908d573cfc9996b3e7adc90313798279c8",
			"input": "0x",
			"maxFeePerGas": "0x23cf3fd4",
			"maxPriorityFeePerGas": "0x0",
			"nonce": "0x2c5ce1",
			"r": "0x4a5703e4d8daf045f021cb32897a25b17d61b9ab629a59f0731ef4cce63f93d6",
			"s": "0x711812237c1fed6aaf08e9f47fc47e547fdaceba9ab7507e62af29a945354fb6",
			"to": "0x388c818ca8b9251b393131c08a736a67ccb19297",
			"transactionIndex": "0x7a",
			"type": "0x2",
			"v": "0x0",
			"value": "0x12bf92aae0c2e70",
			"yParity": "0x0"
		}]);

		// Act
		let transactions = serde_json::from_value::<HashesOrTransactionInfosV1>(value).unwrap();

		// Assert
		match transactions {
			HashesOrTransactionInfosV1::TransactionInfos(transactions) => {
				assert_eq!(transactions.len(), 1);
				let transaction = &transactions[0];
				assert_eq!(transaction.transaction_index, U256::from(0x7au64));
				assert_eq!(
					transaction.hash,
					"0x2c522d01183e9ed70caaf75c940ba9908d573cfc9996b3e7adc90313798279c8"
						.parse()
						.unwrap()
				);
				match &transaction.transaction_signed {
					TransactionSignedV1::Transaction1559Signed(transaction) => {
						assert_eq!(transaction.transaction_1559_unsigned.r#type, TypeEip1559V1);
						assert_eq!(transaction.y_parity, U256::zero());
						assert_eq!(transaction.transaction_1559_unsigned.access_list.len(), 1);
					},
					transaction => panic!("unexpected transaction variant: {transaction:?}"),
				}
			},
			transactions => panic!("unexpected transaction payload: {transactions:?}"),
		}
	}
}
