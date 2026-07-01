// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![allow(missing_docs)]

use super::{
	Byte, Bytes, TYPE_EIP1559, TYPE_EIP2930, TYPE_EIP4844, TYPE_EIP7702, TYPE_LEGACY, TypeEip1559,
	TypeEip2930, TypeEip4844, TypeEip7702, TypeLegacy,
};
use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode};
use derive_more::{From, TryInto};
use ethereum_types::*;
use scale_info::TypeInfo;
use serde::{Deserialize, Deserializer, Serialize};

/// Transaction object generic to all types
#[derive(
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
)]
#[serde(rename_all = "camelCase")]
pub struct GenericTransaction {
	/// accessList
	/// EIP-2930 access list
	#[serde(skip_serializing_if = "Option::is_none")]
	pub access_list: Option<AccessList>,
	/// authorizationList
	/// List of account code authorizations (EIP-7702)
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub authorization_list: Vec<AuthorizationListEntry>,
	/// blobVersionedHashes
	/// List of versioned blob hashes associated with the transaction's EIP-4844 data blobs.
	#[serde(default)]
	pub blob_versioned_hashes: Vec<H256>,
	/// blobs
	/// Raw blob data.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub blobs: Vec<Bytes>,
	/// chainId
	/// Chain ID that this transaction is valid on.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub chain_id: Option<U256>,
	/// from address
	#[serde(skip_serializing_if = "Option::is_none")]
	pub from: Option<Address>,
	/// gas limit
	#[serde(skip_serializing_if = "Option::is_none")]
	pub gas: Option<U256>,
	/// gas price
	/// The gas price willing to be paid by the sender in wei
	#[serde(skip_serializing_if = "Option::is_none")]
	pub gas_price: Option<U256>,
	/// input data
	#[serde(flatten, deserialize_with = "deserialize_input_or_data")]
	pub input: InputOrData,
	/// max fee per blob gas
	/// The maximum total fee per gas the sender is willing to pay for blob gas in wei
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_fee_per_blob_gas: Option<U256>,
	/// max fee per gas
	/// The maximum total fee per gas the sender is willing to pay (includes the network / base fee
	/// and miner / priority fee) in wei
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_fee_per_gas: Option<U256>,
	/// max priority fee per gas
	/// Maximum fee per gas the sender is willing to pay to miners in wei
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_priority_fee_per_gas: Option<U256>,
	/// nonce
	#[serde(skip_serializing_if = "Option::is_none")]
	pub nonce: Option<U256>,
	/// to address
	pub to: Option<Address>,
	/// type
	#[serde(skip_serializing_if = "Option::is_none")]
	pub r#type: Option<Byte>,
	/// value
	#[serde(skip_serializing_if = "Option::is_none")]
	pub value: Option<U256>,
}

impl GenericTransaction {
	/// Create a new [`GenericTransaction`] from a signed transaction.
	pub fn from_signed(tx: TransactionSigned, base_gas_price: U256, from: Option<H160>) -> Self {
		Self::from_unsigned(tx.into(), base_gas_price, from)
	}

	/// Returns `true` when the transaction's payload fields look like those of a simple value
	/// transfer: empty calldata, no access list, no EIP-7702 authorization list, no EIP-4844 blob
	/// payload, and no blob gas fee. The destination address is validated separately by the caller.
	pub fn has_simple_transfer_fields(&self) -> bool {
		self.input.is_empty() &&
			self.access_list.as_ref().is_none_or(|list| list.is_empty()) &&
			self.authorization_list.is_empty() &&
			self.blob_versioned_hashes.is_empty() &&
			self.blobs.is_empty() &&
			self.max_fee_per_blob_gas.is_none()
	}

	/// The gas price that is actually paid (including priority fee).
	pub fn effective_gas_price(&self, base_gas_price: U256) -> Option<U256> {
		let effective_gas_price = if let Some(prio_price) = self.max_priority_fee_per_gas {
			let max_price = self.max_fee_per_gas?;
			Some(max_price.min(base_gas_price.saturating_add(prio_price)))
		} else {
			self.gas_price
		};

		// we do not implement priority fee as it does not map to tip well
		// hence the effective gas price cannot be higher than the base price
		effective_gas_price.map(|e| e.min(base_gas_price))
	}

	/// Create a new [`GenericTransaction`] from a unsigned transaction.
	pub fn from_unsigned(
		tx: TransactionUnsigned,
		base_gas_price: U256,
		from: Option<H160>,
	) -> Self {
		use TransactionUnsigned::*;
		let mut tx = match tx {
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
			Transaction7702Unsigned(tx) => GenericTransaction {
				from,
				r#type: Some(tx.r#type.as_byte()),
				chain_id: Some(tx.chain_id),
				input: tx.input.into(),
				nonce: Some(tx.nonce),
				value: Some(tx.value),
				to: Some(tx.to),
				gas: Some(tx.gas),
				access_list: Some(tx.access_list),
				authorization_list: tx.authorization_list,
				max_fee_per_gas: Some(tx.max_fee_per_gas),
				max_priority_fee_per_gas: Some(tx.max_priority_fee_per_gas),
				..Default::default()
			},
		};
		tx.gas_price = tx.effective_gas_price(base_gas_price);
		tx
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
			TYPE_EIP7702 => Ok(Transaction7702Unsigned {
				r#type: TypeEip7702 {},
				chain_id: self.chain_id.unwrap_or_default(),
				input: self.input.to_bytes(),
				nonce: self.nonce.unwrap_or_default(),
				value: self.value.unwrap_or_default(),
				to: self.to.unwrap_or_default(),
				gas: self.gas.unwrap_or_default(),
				max_fee_per_gas: self.max_fee_per_gas.unwrap_or_default(),
				max_priority_fee_per_gas: self.max_priority_fee_per_gas.unwrap_or_default(),
				access_list: self.access_list.unwrap_or_default(),
				authorization_list: self.authorization_list,
			}
			.into()),
			_ => Err(()),
		}
	}
}

/// Transaction information
#[derive(Debug, Default, Clone, Serialize, Eq, PartialEq, TypeInfo, Encode, Decode)]
#[serde(rename_all = "camelCase")]
pub struct TransactionInfo {
	/// block hash
	pub block_hash: H256,
	/// block number
	pub block_number: U256,
	/// from address
	pub from: Address,
	/// transaction hash
	pub hash: H256,
	/// transaction index
	pub transaction_index: U256,
	#[serde(flatten)]
	pub transaction_signed: TransactionSigned,
}

// Custom deserializer to work around serde's limitation with flatten + untagged enums from Value
// See: https://github.com/serde-rs/serde/issues/1183
impl<'de> Deserialize<'de> for TransactionInfo {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		use alloc::{collections::BTreeMap, string::String};
		use serde::de::Error;

		// First try deserializing to a map
		let mut map = <BTreeMap<String, serde_json::Value>>::deserialize(deserializer)?;

		// Extract the TransactionInfo-specific fields
		let block_hash =
			map.remove("blockHash").ok_or_else(|| D::Error::missing_field("blockHash"))?;
		let block_number = map
			.remove("blockNumber")
			.ok_or_else(|| D::Error::missing_field("blockNumber"))?;
		let from = map.remove("from").ok_or_else(|| D::Error::missing_field("from"))?;
		let hash = map.remove("hash").ok_or_else(|| D::Error::missing_field("hash"))?;
		let transaction_index = map
			.remove("transactionIndex")
			.ok_or_else(|| D::Error::missing_field("transactionIndex"))?;

		// The remaining fields should be for TransactionSigned
		// Convert back to JSON and deserialize
		let remaining = serde_json::Value::Object(map.into_iter().collect());
		let json_str = serde_json::to_string(&remaining).map_err(D::Error::custom)?;
		let transaction_signed: TransactionSigned =
			serde_json::from_str(&json_str).map_err(D::Error::custom)?;

		Ok(Self {
			block_hash: serde_json::from_value(block_hash).map_err(D::Error::custom)?,
			block_number: serde_json::from_value(block_number).map_err(D::Error::custom)?,
			from: serde_json::from_value(from).map_err(D::Error::custom)?,
			hash: serde_json::from_value(hash).map_err(D::Error::custom)?,
			transaction_index: serde_json::from_value(transaction_index)
				.map_err(D::Error::custom)?,
			transaction_signed,
		})
	}
}

#[derive(
	Debug,
	Clone,
	Serialize,
	Deserialize,
	From,
	TryInto,
	Eq,
	PartialEq,
	TypeInfo,
	Encode,
	Decode,
	DecodeWithMemTracking,
)]
#[serde(untagged)]
pub enum TransactionSigned {
	Transaction7702Signed(Transaction7702Signed),
	Transaction4844Signed(Transaction4844Signed),
	Transaction1559Signed(Transaction1559Signed),
	Transaction2930Signed(Transaction2930Signed),
	TransactionLegacySigned(TransactionLegacySigned),
}

impl Default for TransactionSigned {
	fn default() -> Self {
		TransactionSigned::TransactionLegacySigned(Default::default())
	}
}

#[derive(Debug, Clone, Serialize, Deserialize, From, TryInto, Eq, PartialEq)]
#[serde(untagged)]
pub enum TransactionUnsigned {
	Transaction7702Unsigned(Transaction7702Unsigned),
	Transaction4844Unsigned(Transaction4844Unsigned),
	Transaction1559Unsigned(Transaction1559Unsigned),
	Transaction2930Unsigned(Transaction2930Unsigned),
	TransactionLegacyUnsigned(TransactionLegacyUnsigned),
}

impl Default for TransactionUnsigned {
	fn default() -> Self {
		TransactionUnsigned::TransactionLegacyUnsigned(Default::default())
	}
}

impl From<TransactionSigned> for TransactionUnsigned {
	fn from(tx: TransactionSigned) -> Self {
		use TransactionSigned::*;
		match tx {
			Transaction7702Signed(tx) => tx.transaction_7702_unsigned.into(),
			Transaction4844Signed(tx) => tx.transaction_4844_unsigned.into(),
			Transaction1559Signed(tx) => tx.transaction_1559_unsigned.into(),
			Transaction2930Signed(tx) => tx.transaction_2930_unsigned.into(),
			TransactionLegacySigned(tx) => tx.transaction_legacy_unsigned.into(),
		}
	}
}

/// EIP-1559 transaction.
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
	DecodeWithMemTracking,
)]
#[serde(rename_all = "camelCase")]
pub struct Transaction1559Unsigned {
	/// accessList
	/// EIP-2930 access list
	pub access_list: AccessList,
	/// chainId
	/// Chain ID that this transaction is valid on.
	pub chain_id: U256,
	/// gas limit
	pub gas: U256,
	/// gas price
	/// The effective gas price paid by the sender in wei. For transactions not yet included in a
	/// block, this value should be set equal to the max fee per gas. This field is DEPRECATED,
	/// please transition to using effectiveGasPrice in the receipt object going forward.
	pub gas_price: U256,
	/// input data
	pub input: Bytes,
	/// max fee per gas
	/// The maximum total fee per gas the sender is willing to pay (includes the network / base fee
	/// and miner / priority fee) in wei
	pub max_fee_per_gas: U256,
	/// max priority fee per gas
	/// Maximum fee per gas the sender is willing to pay to miners in wei
	pub max_priority_fee_per_gas: U256,
	/// nonce
	pub nonce: U256,
	/// to address
	pub to: Option<Address>,
	/// type
	pub r#type: TypeEip1559,
	/// value
	pub value: U256,
}
/// EIP-2930 transaction.
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
	DecodeWithMemTracking,
)]
#[serde(rename_all = "camelCase")]
pub struct Transaction2930Unsigned {
	/// accessList
	/// EIP-2930 access list
	pub access_list: AccessList,
	/// chainId
	/// Chain ID that this transaction is valid on.
	pub chain_id: U256,
	/// gas limit
	pub gas: U256,
	/// gas price
	/// The gas price willing to be paid by the sender in wei
	pub gas_price: U256,
	/// input data
	pub input: Bytes,
	/// nonce
	pub nonce: U256,
	/// to address
	pub to: Option<Address>,
	/// type
	pub r#type: TypeEip2930,
	/// value
	pub value: U256,
}
/// EIP-4844 transaction.
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
	DecodeWithMemTracking,
)]
#[serde(rename_all = "camelCase")]
pub struct Transaction4844Unsigned {
	/// accessList
	/// EIP-2930 access list
	pub access_list: AccessList,
	/// blobVersionedHashes
	/// List of versioned blob hashes associated with the transaction's EIP-4844 data blobs.
	pub blob_versioned_hashes: Vec<H256>,
	/// chainId
	/// Chain ID that this transaction is valid on.
	pub chain_id: U256,
	/// gas limit
	pub gas: U256,
	/// input data
	pub input: Bytes,
	/// max fee per blob gas
	/// The maximum total fee per gas the sender is willing to pay for blob gas in wei
	pub max_fee_per_blob_gas: U256,
	/// max fee per gas
	/// The maximum total fee per gas the sender is willing to pay (includes the network / base fee
	/// and miner / priority fee) in wei
	pub max_fee_per_gas: U256,
	/// max priority fee per gas
	/// Maximum fee per gas the sender is willing to pay to miners in wei
	pub max_priority_fee_per_gas: U256,
	/// nonce
	pub nonce: U256,
	/// to address
	pub to: Address,
	/// type
	pub r#type: TypeEip4844,
	/// value
	pub value: U256,
}
/// Legacy transaction.
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
	DecodeWithMemTracking,
)]
#[serde(rename_all = "camelCase")]
pub struct TransactionLegacyUnsigned {
	/// chainId
	/// Chain ID that this transaction is valid on.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub chain_id: Option<U256>,
	/// gas limit
	pub gas: U256,
	/// gas price
	/// The gas price willing to be paid by the sender in wei
	pub gas_price: U256,
	/// input data
	pub input: Bytes,
	/// nonce
	pub nonce: U256,
	/// to address
	pub to: Option<Address>,
	/// type
	pub r#type: TypeLegacy,
	/// value
	pub value: U256,
}
/// EIP-7702 transaction.
#[derive(
	Debug,
	Clone,
	Serialize,
	Deserialize,
	Default,
	From,
	Eq,
	PartialEq,
	TypeInfo,
	Encode,
	Decode,
	DecodeWithMemTracking,
)]
#[serde(rename_all = "camelCase")]
pub struct Transaction7702Unsigned {
	/// accessList
	/// EIP-2930 access list
	pub access_list: AccessList,
	/// authorizationList
	/// List of account code authorizations
	pub authorization_list: Vec<AuthorizationListEntry>,
	/// chainId
	/// Chain ID that this transaction is valid on.
	pub chain_id: U256,
	/// gas limit
	pub gas: U256,
	/// input data
	pub input: Bytes,
	/// max fee per gas
	/// The maximum total fee per gas the sender is willing to pay (includes the network / base fee
	/// and miner / priority fee) in wei
	pub max_fee_per_gas: U256,
	/// max priority fee per gas
	/// Maximum fee per gas the sender is willing to pay to miners in wei
	pub max_priority_fee_per_gas: U256,
	/// nonce
	pub nonce: U256,
	/// to address
	///
	/// # Note
	///
	/// Extracted from eip-7702: `Note, this implies a null destination is not valid.`
	pub to: Address,
	/// type
	pub r#type: TypeEip7702,
	/// value
	pub value: U256,
}
/// Signed 7702 Transaction
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
	DecodeWithMemTracking,
)]
#[serde(rename_all = "camelCase")]
pub struct Transaction7702Signed {
	#[serde(flatten)]
	pub transaction_7702_unsigned: Transaction7702Unsigned,
	/// r
	pub r: U256,
	/// s
	pub s: U256,
	/// v
	/// For backwards compatibility, `v` is optionally provided as an alternative to `yParity`.
	/// This field is DEPRECATED and all use of it should migrate to `yParity`.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub v: Option<U256>,
	/// yParity
	/// The parity (0 for even, 1 for odd) of the y-value of the secp256k1 signature.
	pub y_parity: U256,
}
/// Signed 1559 Transaction
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
	DecodeWithMemTracking,
)]
#[serde(rename_all = "camelCase")]
pub struct Transaction1559Signed {
	#[serde(flatten)]
	pub transaction_1559_unsigned: Transaction1559Unsigned,
	/// r
	pub r: U256,
	/// s
	pub s: U256,
	/// v
	/// For backwards compatibility, `v` is optionally provided as an alternative to `yParity`.
	/// This field is DEPRECATED and all use of it should migrate to `yParity`.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub v: Option<U256>,
	/// yParity
	/// The parity (0 for even, 1 for odd) of the y-value of the secp256k1 signature.
	pub y_parity: U256,
}
/// Signed 2930 Transaction
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
	DecodeWithMemTracking,
)]
#[serde(rename_all = "camelCase")]
pub struct Transaction2930Signed {
	#[serde(flatten)]
	pub transaction_2930_unsigned: Transaction2930Unsigned,
	/// r
	pub r: U256,
	/// s
	pub s: U256,
	/// v
	/// For backwards compatibility, `v` is optionally provided as an alternative to `yParity`.
	/// This field is DEPRECATED and all use of it should migrate to `yParity`.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub v: Option<U256>,
	/// yParity
	/// The parity (0 for even, 1 for odd) of the y-value of the secp256k1 signature.
	pub y_parity: U256,
}
/// Signed 4844 Transaction
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
	DecodeWithMemTracking,
)]
#[serde(rename_all = "camelCase")]
pub struct Transaction4844Signed {
	#[serde(flatten)]
	pub transaction_4844_unsigned: Transaction4844Unsigned,
	/// r
	pub r: U256,
	/// s
	pub s: U256,
	/// yParity
	/// The parity (0 for even, 1 for odd) of the y-value of the secp256k1 signature.
	pub y_parity: U256,
}
/// Signed Legacy Transaction
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
	DecodeWithMemTracking,
)]
#[serde(rename_all = "camelCase")]
pub struct TransactionLegacySigned {
	#[serde(flatten)]
	pub transaction_legacy_unsigned: TransactionLegacyUnsigned,
	/// r
	pub r: U256,
	/// s
	pub s: U256,
	/// v
	pub v: U256,
}

/// Access list
pub type AccessList = Vec<AccessListEntry>;

/// Access list entry
#[derive(
	Debug,
	Default,
	Clone,
	Encode,
	Decode,
	TypeInfo,
	Serialize,
	Deserialize,
	Eq,
	PartialEq,
	DecodeWithMemTracking,
)]
#[serde(rename_all = "camelCase")]
pub struct AccessListEntry {
	pub address: Address,
	pub storage_keys: Vec<H256>,
}

/// Authorization list entry for EIP-7702
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
	DecodeWithMemTracking,
)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizationListEntry {
	/// Chain ID that this authorization is valid on
	pub chain_id: U256,
	/// Address to authorize
	pub address: Address,
	/// Nonce of the authorization
	pub nonce: U256,
	/// y-parity of the signature
	pub y_parity: U256,
	/// r component of signature
	pub r: U256,
	/// s component of signature
	pub s: U256,
}

#[derive(
	Debug, Clone, Serialize, Deserialize, From, TryInto, Eq, PartialEq, TypeInfo, Encode, Decode,
)]
#[serde(untagged)]
pub enum HashesOrTransactionInfos {
	/// Transaction hashes
	Hashes(Vec<H256>),
	/// Full transactions
	TransactionInfos(Vec<TransactionInfo>),
}

impl Default for HashesOrTransactionInfos {
	fn default() -> Self {
		HashesOrTransactionInfos::Hashes(Default::default())
	}
}

impl HashesOrTransactionInfos {
	pub fn push_hash(&mut self, hash: H256) {
		match self {
			HashesOrTransactionInfos::Hashes(hashes) => hashes.push(hash),
			_ => {},
		}
	}

	pub fn len(&self) -> usize {
		match self {
			HashesOrTransactionInfos::Hashes(v) => v.len(),
			HashesOrTransactionInfos::TransactionInfos(v) => v.len(),
		}
	}

	pub fn is_empty(&self) -> bool {
		self.len() == 0
	}

	pub fn contains_tx(&self, hash: H256) -> bool {
		match self {
			HashesOrTransactionInfos::Hashes(hashes) => hashes.iter().any(|h256| *h256 == hash),
			HashesOrTransactionInfos::TransactionInfos(transaction_infos) => {
				transaction_infos.iter().any(|ti| ti.hash == hash)
			},
		}
	}
}

/// Input of a `GenericTransaction`
#[derive(
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
)]
pub struct InputOrData {
	#[serde(skip_serializing_if = "Option::is_none")]
	input: Option<Bytes>,
	#[serde(skip_serializing_if = "Option::is_none")]
	data: Option<Bytes>,
}

impl From<Bytes> for InputOrData {
	fn from(value: Bytes) -> Self {
		InputOrData { input: Some(value), data: None }
	}
}

impl From<Vec<u8>> for InputOrData {
	fn from(value: Vec<u8>) -> Self {
		InputOrData { input: Some(Bytes(value)), data: None }
	}
}

impl InputOrData {
	/// Get the input as `Bytes`.
	pub fn to_bytes(self) -> Bytes {
		match self {
			InputOrData { input: Some(input), data: _ } => input,
			InputOrData { input: None, data: Some(data) } => data,
			_ => Default::default(),
		}
	}

	/// Get the input as `Vec<u8>`.
	pub fn to_vec(self) -> Vec<u8> {
		self.to_bytes().0
	}

	/// Returns the input as a byte slice, preferring `input` over `data`.
	pub fn as_slice(&self) -> &[u8] {
		self.input
			.as_ref()
			.or(self.data.as_ref())
			.map(|bytes| bytes.0.as_slice())
			.unwrap_or_default()
	}

	/// Returns true if the input carries no bytes.
	pub fn is_empty(&self) -> bool {
		self.as_slice().is_empty()
	}
}

fn deserialize_input_or_data<'d, D: Deserializer<'d>>(d: D) -> Result<InputOrData, D::Error> {
	let value = InputOrData::deserialize(d)?;
	match &value {
		InputOrData { input: Some(input), data: Some(data) } if input != data => {
			Err(serde::de::Error::custom(
				"Both \"data\" and \"input\" are set and not equal. Please use \"input\" to pass transaction call data",
			))
		},
		_ => Ok(value),
	}
}

#[cfg(test)]
mod tests {
	use crate::evm::*;

	#[test]
	fn test_transaction_info_deserialize_from_value() {
		// This tests the custom deserializer for TransactionInfo
		// which works around serde's limitation with flatten + untagged enums from Value
		let tx_info_expected = serde_json::json!({
			"blockHash": "0xfb8c980d1da1a75e68c2ea4d55cb88d62dedbbb5eaf69df8fe337e9f6922b73a",
			"blockNumber": "0x161bd0f",
			"from": "0x4838b106fce9647bdf1e7877bf73ce8b0bad5f97",
			"hash": "0x2c522d01183e9ed70caaf75c940ba9908d573cfc9996b3e7adc90313798279c8",
			"transactionIndex": "0x7a",
			"chainId": "0x1",
			"gas": "0x565f",
			"gasPrice": "0x23cf3fd4",
			"input": "0x",
			"nonce": "0x2c5ce1",
			"r": "0x4a5703e4d8daf045f021cb32897a25b17d61b9ab629a59f0731ef4cce63f93d6",
			"s": "0x711812237c1fed6aaf08e9f47fc47e547fdaceba9ab7507e62af29a945354fb6",
			"to": "0x388c818ca8b9251b393131c08a736a67ccb19297",
			"type": "0x0",
			"v": "0x1",
			"value": "0x12bf92aae0c2e70"
		});

		// Test deserializing from Value (this was failing before the custom deserializer) with
		// below error:
		// ```
		// Failed to deserialize from Value: Some(Error("data did not match any variant of untagged enum TransactionSigned", line: 0, column: 0))
		// ```
		let tx_info_from_value: Result<TransactionInfo, serde_json::Error> =
			serde_json::from_value(tx_info_expected.clone());
		assert!(
			tx_info_from_value.is_ok(),
			"Failed to deserialize from Value: {:?}",
			tx_info_from_value.err()
		);

		// Test deserializing from string (this was always working)
		let json_str = serde_json::to_string(&tx_info_expected).unwrap();
		let tx_info_from_str: Result<TransactionInfo, serde_json::Error> =
			serde_json::from_str(&json_str);
		assert!(
			tx_info_from_str.is_ok(),
			"Failed to deserialize from string: {:?}",
			tx_info_from_str.err()
		);

		// Verify both methods produce the same result
		let tx_info_from_value = tx_info_from_value.unwrap();
		let tx_info_from_str = tx_info_from_str.unwrap();
		assert_eq!(
			tx_info_from_value, tx_info_from_str,
			"Value and string deserialization should match"
		);

		// Serialize it back to JSON
		let tx_info_serialized = serde_json::to_value(&tx_info_from_value);
		assert!(
			tx_info_serialized.is_ok(),
			"Failed to serialize to value: {:?}",
			tx_info_serialized.err()
		);
		let tx_info_serialized = tx_info_serialized.unwrap();

		// Verify that deserializing and serializing leads to the same result
		assert_eq!(tx_info_serialized, tx_info_expected);
	}

	#[test]
	fn test_transaction_hashes_deserialization() {
		let json = r#"["0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"]"#;
		let result: HashesOrTransactionInfos = serde_json::from_str(json).unwrap();
		assert!(matches!(result, HashesOrTransactionInfos::Hashes(_)));

		let json = r#"[]"#;
		let result: HashesOrTransactionInfos = serde_json::from_str(json).unwrap();
		assert!(matches!(result, HashesOrTransactionInfos::Hashes(_)));

		let json = r#"[{"invalid": "data"}]"#;
		let result: Result<HashesOrTransactionInfos, _> = serde_json::from_str(json);
		assert!(result.is_err());
	}

	#[test]
	fn test_transaction_infos_deserialization() {
		let json = r#"[{
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
			}]
		"#;
		let result: HashesOrTransactionInfos = serde_json::from_str(json).unwrap();
		assert!(matches!(result, HashesOrTransactionInfos::TransactionInfos(_)));
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

		let err =
			serde_json::from_str::<GenericTransaction>(r#"{"data": "0x02", "input": "0x01"}"#)
				.unwrap_err();
		assert!(
			err.to_string().starts_with(
			"Both \"data\" and \"input\" are set and not equal. Please use \"input\" to pass transaction call data"
			)
		);
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
			gas_price: U256::from(10),
			..Default::default()
		});

		let generic = GenericTransaction::from_unsigned(tx.clone(), base_gas_price, None);
		assert_eq!(generic.gas_price, Some(U256::from(10)));

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
		assert_eq!(generic.gas_price, Some(U256::from(10)));

		let tx2 = generic.try_into_unsigned().unwrap();
		assert_eq!(tx, tx2);
	}

	#[test]
	fn from_unsigned_works_for_7702() {
		let base_gas_price = U256::from(10);
		let tx = TransactionUnsigned::from(Transaction7702Unsigned {
			chain_id: U256::from(1),
			input: Bytes::from(vec![1u8]),
			nonce: U256::from(1),
			value: U256::from(1),
			to: H160::zero(),
			gas: U256::from(1),
			max_fee_per_gas: U256::from(20),
			max_priority_fee_per_gas: U256::from(1),
			authorization_list: vec![AuthorizationListEntry {
				chain_id: U256::from(1),
				address: H160::from_low_u64_be(42),
				nonce: U256::from(0),
				y_parity: U256::from(1),
				r: U256::from(1),
				s: U256::from(2),
			}],
			..Default::default()
		});

		let generic = GenericTransaction::from_unsigned(tx.clone(), base_gas_price, None);
		assert_eq!(generic.gas_price, Some(U256::from(10)));

		let tx2 = generic.try_into_unsigned().unwrap();
		assert_eq!(tx, tx2);
	}
}
