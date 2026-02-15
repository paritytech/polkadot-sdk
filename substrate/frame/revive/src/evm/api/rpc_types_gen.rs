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
//! Generated JSON-RPC types.
#![allow(missing_docs)]

use super::{byte::*, TypeEip1559, TypeEip2930, TypeEip4844, TypeEip7702, TypeLegacy};
use alloc::{collections::BTreeMap, vec::Vec};
use codec::{Decode, DecodeWithMemTracking, Encode};
use derive_more::{From, TryInto};
pub use ethereum_types::*;
use scale_info::TypeInfo;
use serde::{de::Error, Deserialize, Deserializer, Serialize};

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
}

fn deserialize_input_or_data<'d, D: Deserializer<'d>>(d: D) -> Result<InputOrData, D::Error> {
	let value = InputOrData::deserialize(d)?;
	match &value {
        InputOrData { input: Some(input), data: Some(data) } if input != data =>
            Err(serde::de::Error::custom("Both \"data\" and \"input\" are set and not equal. Please use \"input\" to pass transaction call data")),
        _ => Ok(value),
    }
}

/// Block object
#[derive(
	Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
)]
#[serde(rename_all = "camelCase")]
pub struct Block {
	/// Base fee per gas
	pub base_fee_per_gas: U256,
	/// Blob gas used
	pub blob_gas_used: U256,
	/// Difficulty
	pub difficulty: U256,
	/// Excess blob gas
	pub excess_blob_gas: U256,
	/// Extra data
	pub extra_data: Bytes,
	/// Gas limit
	pub gas_limit: U256,
	/// Gas used
	pub gas_used: U256,
	/// Hash
	pub hash: H256,
	/// Bloom filter
	pub logs_bloom: Bytes256,
	/// Coinbase
	pub miner: Address,
	/// Mix hash
	pub mix_hash: H256,
	/// Nonce
	pub nonce: Bytes8,
	/// Number
	pub number: U256,
	/// Parent Beacon Block Root
	#[serde(skip_serializing_if = "Option::is_none")]
	pub parent_beacon_block_root: Option<H256>,
	/// Parent block hash
	pub parent_hash: H256,
	/// Receipts root
	pub receipts_root: H256,
	/// Requests root
	#[serde(skip_serializing_if = "Option::is_none")]
	pub requests_hash: Option<H256>,
	/// Ommers hash
	pub sha_3_uncles: H256,
	/// Block size
	pub size: U256,
	/// State root
	pub state_root: H256,
	/// Timestamp
	pub timestamp: U256,
	/// Total difficulty
	#[serde(skip_serializing_if = "Option::is_none")]
	pub total_difficulty: Option<U256>,
	pub transactions: HashesOrTransactionInfos,
	/// Transactions root
	pub transactions_root: H256,
	/// Uncles
	pub uncles: Vec<H256>,
	/// Withdrawals
	pub withdrawals: Vec<Withdrawal>,
	/// Withdrawals root
	pub withdrawals_root: H256,
}

/// Block number or tag
#[derive(Debug, Clone, Serialize, Deserialize, From, TryInto, Eq, PartialEq)]
#[serde(untagged)]
pub enum BlockNumberOrTag {
	/// Block number
	U256(U256),
	/// Block tag
	BlockTag(BlockTag),
}
impl Default for BlockNumberOrTag {
	fn default() -> Self {
		BlockNumberOrTag::BlockTag(Default::default())
	}
}

/// Block number, tag, or block hash
#[derive(Debug, Clone, Serialize, From, TryInto, Eq, PartialEq)]
#[serde(untagged)]
pub enum BlockNumberOrTagOrHash {
	/// Block number
	BlockNumber(U256),
	/// Block tag
	BlockTag(BlockTag),
	/// Block hash
	BlockHash(H256),
}
impl Default for BlockNumberOrTagOrHash {
	fn default() -> Self {
		BlockNumberOrTagOrHash::BlockTag(Default::default())
	}
}

// Support nested object notation as defined in  https://eips.ethereum.org/EIPS/eip-1898
impl<'a> serde::Deserialize<'a> for BlockNumberOrTagOrHash {
	fn deserialize<D>(de: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'a>,
	{
		#[derive(Deserialize)]
		#[serde(untagged)]
		pub enum BlockNumberOrTagOrHashWithAlias {
			BlockTag(BlockTag),
			BlockNumber(U64),
			NestedBlockNumber {
				#[serde(rename = "blockNumber")]
				block_number: U256,
			},
			BlockHash(H256),
			NestedBlockHash {
				#[serde(rename = "blockHash")]
				block_hash: H256,
			},
		}

		let r = BlockNumberOrTagOrHashWithAlias::deserialize(de)?;
		Ok(match r {
			BlockNumberOrTagOrHashWithAlias::BlockTag(val) => BlockNumberOrTagOrHash::BlockTag(val),
			BlockNumberOrTagOrHashWithAlias::BlockNumber(val) => {
				let val: u64 =
					val.try_into().map_err(|_| D::Error::custom("u64 conversion failed"))?;
				BlockNumberOrTagOrHash::BlockNumber(val.into())
			},

			BlockNumberOrTagOrHashWithAlias::NestedBlockNumber { block_number: val } => {
				BlockNumberOrTagOrHash::BlockNumber(val)
			},
			BlockNumberOrTagOrHashWithAlias::BlockHash(val) |
			BlockNumberOrTagOrHashWithAlias::NestedBlockHash { block_hash: val } => {
				BlockNumberOrTagOrHash::BlockHash(val)
			},
		})
	}
}

/// filter
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
	/// Address(es)
	pub address: Option<AddressOrAddresses>,
	/// from block
	#[serde(skip_serializing_if = "Option::is_none")]
	pub from_block: Option<BlockNumberOrTag>,
	/// to block
	#[serde(skip_serializing_if = "Option::is_none")]
	pub to_block: Option<BlockNumberOrTag>,
	/// Restricts the logs returned to the single block
	#[serde(skip_serializing_if = "Option::is_none")]
	pub block_hash: Option<H256>,
	/// Topics
	#[serde(skip_serializing_if = "Option::is_none")]
	pub topics: Option<FilterTopics>,
}

/// Filter results
#[derive(Debug, Clone, Serialize, Deserialize, From, TryInto, Eq, PartialEq)]
#[serde(untagged)]
pub enum FilterResults {
	/// new block or transaction hashes
	Hashes(Vec<H256>),
	/// new logs
	Logs(Vec<Log>),
}
impl Default for FilterResults {
	fn default() -> Self {
		FilterResults::Hashes(Default::default())
	}
}

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

/// Receipt information
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptInfo {
	/// blob gas price
	/// The actual value per gas deducted from the sender's account for blob gas. Only specified
	/// for blob transactions as defined by EIP-4844.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub blob_gas_price: Option<U256>,
	/// blob gas used
	/// The amount of blob gas used for this specific transaction. Only specified for blob
	/// transactions as defined by EIP-4844.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub blob_gas_used: Option<U256>,
	/// block hash
	pub block_hash: H256,
	/// block number
	pub block_number: U256,
	/// contract address
	/// The contract address created, if the transaction was a contract creation, otherwise null.
	pub contract_address: Option<Address>,
	/// cumulative gas used
	/// The sum of gas used by this transaction and all preceding transactions in the same block.
	pub cumulative_gas_used: U256,
	/// effective gas price
	/// The actual value per gas deducted from the sender's account. Before EIP-1559, this is equal
	/// to the transaction's gas price. After, it is equal to baseFeePerGas + min(maxFeePerGas -
	/// baseFeePerGas, maxPriorityFeePerGas).
	pub effective_gas_price: U256,
	/// from
	pub from: Address,
	/// gas used
	/// The amount of gas used for this specific transaction alone.
	pub gas_used: U256,
	/// logs
	pub logs: Vec<Log>,
	/// logs bloom
	pub logs_bloom: Bytes256,
	/// state root
	/// The post-transaction state root. Only specified for transactions included before the
	/// Byzantium upgrade.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub root: Option<H256>,
	/// status
	/// Either 1 (success) or 0 (failure). Only specified for transactions included after the
	/// Byzantium upgrade.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub status: Option<U256>,
	/// to
	/// Address of the receiver or null in a contract creation transaction.
	pub to: Option<Address>,
	/// transaction hash
	pub transaction_hash: H256,
	/// transaction index
	pub transaction_index: U256,
	/// type
	#[serde(skip_serializing_if = "Option::is_none")]
	pub r#type: Option<Byte>,
}

/// Syncing status
#[derive(Debug, Clone, Serialize, Deserialize, From, TryInto, Eq, PartialEq)]
#[serde(untagged)]
pub enum SyncingStatus {
	/// Syncing progress
	SyncingProgress(SyncingProgress),
	/// Not syncing
	/// Should always return false if not syncing.
	Bool(bool),
}
impl Default for SyncingStatus {
	fn default() -> Self {
		SyncingStatus::SyncingProgress(Default::default())
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

/// Access list
pub type AccessList = Vec<AccessListEntry>;

/// Address(es)
#[derive(Debug, Clone, Serialize, Deserialize, From, TryInto, Eq, PartialEq)]
#[serde(untagged)]
pub enum AddressOrAddresses {
	/// Address
	Address(Address),
	/// Addresses
	Addresses(Addresses),
}
impl Default for AddressOrAddresses {
	fn default() -> Self {
		AddressOrAddresses::Address(Default::default())
	}
}

/// hex encoded address
pub type Addresses = Vec<Address>;

/// Block tag
/// `earliest`: The lowest numbered block the client has available; `finalized`: The most recent
/// crypto-economically secure block, cannot be re-orged outside of manual intervention driven by
/// community coordination; `safe`: The most recent block that is safe from re-orgs under honest
/// majority and certain synchronicity assumptions; `latest`: The most recent block in the canonical
/// chain observed by the client, this block may be re-orged out of the canonical chain even under
/// healthy/normal conditions; `pending`: A sample next block built by the client on top of `latest`
/// and containing the set of transactions usually taken from local mempool. Before the merge
/// transition is finalized, any call querying for `finalized` or `safe` block MUST be responded to
/// with `-39001: Unknown block` error
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BlockTag {
	Earliest,
	Finalized,
	Safe,
	#[default]
	Latest,
	Pending,
}

/// Filter Topics
pub type FilterTopics = Vec<FilterTopic>;

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

/// log
#[derive(
	Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, Encode, Decode, TypeInfo,
)]
#[serde(rename_all = "camelCase")]
pub struct Log {
	/// address
	pub address: Address,
	/// block hash
	pub block_hash: H256,
	/// block number
	pub block_number: U256,
	/// data
	#[serde(skip_serializing_if = "Option::is_none")]
	pub data: Option<Bytes>,
	/// log index
	pub log_index: U256,
	/// removed
	#[serde(default)]
	pub removed: bool,
	/// topics
	#[serde(default)]
	pub topics: Vec<H256>,
	/// transaction hash
	pub transaction_hash: H256,
	/// transaction index
	pub transaction_index: U256,
}

/// Syncing progress
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SyncingProgress {
	/// Current block
	#[serde(skip_serializing_if = "Option::is_none")]
	pub current_block: Option<U256>,
	/// Highest block
	#[serde(skip_serializing_if = "Option::is_none")]
	pub highest_block: Option<U256>,
	/// Starting block
	#[serde(skip_serializing_if = "Option::is_none")]
	pub starting_block: Option<U256>,
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

/// Validator withdrawal
#[derive(
	Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
)]
#[serde(rename_all = "camelCase")]
pub struct Withdrawal {
	/// recipient address for withdrawal value
	pub address: Address,
	/// value contained in withdrawal
	pub amount: U256,
	/// index of withdrawal
	pub index: U256,
	/// index of validator that generated withdrawal
	pub validator_index: U256,
}

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

/// Filter Topic List Entry
#[derive(Debug, Clone, Serialize, Deserialize, From, TryInto, Eq, PartialEq)]
#[serde(untagged)]
pub enum FilterTopic {
	/// Single Topic Match
	Single(H256),
	/// Multiple Topic Match
	Multiple(Vec<H256>),
}
impl Default for FilterTopic {
	fn default() -> Self {
		FilterTopic::Single(Default::default())
	}
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FeeHistoryResult {
	/// Lowest number block of the returned range.
	pub oldest_block: U256,

	/// An array of block base fees per gas.
	///
	/// This includes the next block after the newest of the returned range, because this value can
	/// be derived from the newest block. Zeroes are returned for pre-EIP-1559 blocks.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub base_fee_per_gas: Vec<U256>,

	/// An array of block gas used ratios.
	/// These are calculated as the ratio of `gasUsed` and `gasLimit`.
	pub gas_used_ratio: Vec<f64>,

	/// A two-dimensional array of effective priority fees per gas at the requested block
	/// percentiles.
	///
	/// A given percentile sample of effective priority fees per gas from a single block in
	/// ascending order, weighted by gas used. Zeroes are returned if the block is empty.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub reward: Vec<Vec<U256>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[serde(rename_all = "camelCase")]
pub struct SimulationParameters {
	/// Definition of blocks that can contain calls and overrides.
	///
	/// The maximum number of entries that could exist here is 256, any requests with more blocks
	/// than this are considered invalid.
	pub block_state_calls: Vec<SimulationPayload>,

	/// Adds ETH transfers as ERC20 transfer events to the logs.
	///
	/// These transfers have emitter contract parameter set as address([0xE; 20]). This allows you
	/// to track movements of ETH in your calls.
	#[serde(default)]
	pub trace_transfers: bool,

	/// When true, the `eth_simulateV1` does all the validation that a normal EVM would do, except
	/// contract sender and signature checks. When false, `eth_simulateV1` behaves like `eth_call`.
	#[serde(default)]
	pub validation: bool,

	/// When true, the method returns full transaction objects, otherwise, just hashes are
	/// returned.
	#[serde(default)]
	pub return_full_transactions: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[serde(rename_all = "camelCase")]
pub struct SimulationPayload {
	/// Overrides fields such as block number or time in a simulated block.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub block_overrides: Option<BlockOverrides>,

	/// State overrides can be used to replace existing blockchain state with new state.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub state_overrides: Option<StateOverrides>,

	/// An array of transaction call objects.
	pub calls: Vec<GenericTransaction>,
}

#[derive(
	Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, Encode, Decode, TypeInfo,
)]
#[serde(rename_all = "camelCase")]
pub struct BlockOverrides {
	/// Block number
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub number: Option<U256>,

	/// The previous value of randomness beacon
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub prev_randao: Option<U256>,

	/// Block timestamp
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub time: Option<U256>,

	/// Gas limit
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub gas_limit: Option<U256>,

	/// Fee recipient (also known as coinbase).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub fee_recipient: Option<H160>,

	/// Withdrawals made by validators.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub withdrawals: Option<Vec<Withdrawal>>,

	/// Base fee per unit of gas (see EIP-1559).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub base_fee_per_gas: Option<U256>,

	/// Base fee per unit of blob gas (see EIP-4844).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub blob_base_fee: Option<U256>,
}

#[derive(
	Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, Encode, Decode, TypeInfo,
)]
pub struct StateOverrides(pub BTreeMap<H160, AddressStateOverride>);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[serde(rename_all = "camelCase")]
pub struct AddressStateOverride {
	/// Fake balance to set for the account before executing the call.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub balance: Option<U256>,

	/// Fake nonce to set for the account before executing the call.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub nonce: Option<U256>,

	/// Fake EVM bytecode to inject into the account before executing the call.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub code: Option<Bytes>,

	/// Storage overrides to apply.
	#[serde(flatten, default, skip_serializing_if = "Option::is_none")]
	pub storage: Option<StorageOverrides>,

	/// Moves precompile to given address.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub move_precompile_to_address: Option<H160>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[serde(untagged)]
pub enum StorageOverrides {
	/// Overrides all of the storage slots of the address. If one of the storage slots are not
	/// specified then they would be removed.
	AllStorageSlots {
		#[serde(rename = "state")]
		overrides: BTreeMap<H256, Bytes32>,
	},
	/// Overrides only the specific storage slots specified in the overrides. All other slots are
	/// kept with the same values they have.
	SpecificStorageSlots {
		#[serde(rename = "stateDiff")]
		overrides: BTreeMap<H256, Bytes32>,
	},
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct SimulationResponse<SimulationError>(pub Vec<SimulationBlock<SimulationError>>);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[serde(rename_all = "camelCase")]
pub struct SimulationBlock<SimulationError> {
	/// Base fee per gas
	pub base_fee_per_gas: U256,
	/// Blob gas used
	pub blob_gas_used: U256,
	/// Difficulty
	pub difficulty: U256,
	/// Excess blob gas
	pub excess_blob_gas: U256,
	/// Extra data
	pub extra_data: Bytes,
	/// Gas limit
	pub gas_limit: U256,
	/// Gas used
	pub gas_used: U256,
	/// Hash
	pub hash: H256,
	/// Bloom filter
	pub logs_bloom: Bytes256,
	/// Coinbase
	pub miner: Address,
	/// Mix hash
	pub mix_hash: H256,
	/// Nonce
	pub nonce: Bytes8,
	/// Number
	pub number: U256,
	/// Parent Beacon Block Root
	#[serde(skip_serializing_if = "Option::is_none")]
	pub parent_beacon_block_root: Option<H256>,
	/// Parent block hash
	pub parent_hash: H256,
	/// Receipts root
	pub receipts_root: H256,
	/// Requests root
	#[serde(skip_serializing_if = "Option::is_none")]
	pub requests_hash: Option<H256>,
	/// Ommers hash
	pub sha_3_uncles: H256,
	/// Block size
	pub size: U256,
	/// State root
	pub state_root: H256,
	/// Timestamp
	pub timestamp: U256,
	/// Total difficulty
	#[serde(skip_serializing_if = "Option::is_none")]
	pub total_difficulty: Option<U256>,
	pub transactions: HashesOrTransactionInfos,
	/// Transactions root
	pub transactions_root: H256,
	/// Uncles
	pub uncles: Vec<H256>,
	/// Withdrawals
	pub withdrawals: Vec<Withdrawal>,
	/// Withdrawals root
	pub withdrawals_root: H256,
	/// The result of the various calls in the simulation
	pub calls: Vec<SimulationCallResult<SimulationError>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "status")]
pub enum SimulationCallResult<SimulationError> {
	/// Simulation call result returned when the simulation of the call succeeds
	#[serde(rename = "0x1")]
	Success {
		/// Transactions return data
		return_data: Bytes,
		/// Gas used by the transaction
		gas_used: U256,
		/// Log events emitted during call. This includes ETH logs, if `traceTransfers` is enabled
		logs: Vec<Log>,
	},
	/// Simulation call result returned when the simulation of the call fails
	#[serde(rename = "0x0")]
	Failed {
		/// Transactions return data
		return_data: Bytes,
		/// Gas used by the transaction
		gas_used: U256,
		/// Error code, data and message
		error: SimulationError,
	},
}

impl<E> SimulationCallResult<E> {
	/// Map the error type of this result.
	pub fn map_error<F>(self, f: impl FnOnce(E) -> F) -> SimulationCallResult<F> {
		match self {
			Self::Success { return_data, gas_used, logs } => {
				SimulationCallResult::Success { return_data, gas_used, logs }
			},
			Self::Failed { return_data, gas_used, error } => {
				SimulationCallResult::Failed { return_data, gas_used, error: f(error) }
			},
		}
	}

	pub fn is_success(&self) -> bool {
		matches!(self, Self::Success { .. })
	}

	pub fn is_failure(&self) -> bool {
		!self.is_success()
	}
}

impl<E> SimulationBlock<E> {
	/// Map the error type of all call results in this block.
	pub fn map_error<F>(self, f: impl Fn(E) -> F) -> SimulationBlock<F> {
		SimulationBlock {
			base_fee_per_gas: self.base_fee_per_gas,
			blob_gas_used: self.blob_gas_used,
			difficulty: self.difficulty,
			excess_blob_gas: self.excess_blob_gas,
			extra_data: self.extra_data,
			gas_limit: self.gas_limit,
			gas_used: self.gas_used,
			hash: self.hash,
			logs_bloom: self.logs_bloom,
			miner: self.miner,
			mix_hash: self.mix_hash,
			nonce: self.nonce,
			number: self.number,
			parent_beacon_block_root: self.parent_beacon_block_root,
			parent_hash: self.parent_hash,
			receipts_root: self.receipts_root,
			requests_hash: self.requests_hash,
			sha_3_uncles: self.sha_3_uncles,
			size: self.size,
			state_root: self.state_root,
			timestamp: self.timestamp,
			total_difficulty: self.total_difficulty,
			transactions: self.transactions,
			transactions_root: self.transactions_root,
			uncles: self.uncles,
			withdrawals: self.withdrawals,
			withdrawals_root: self.withdrawals_root,
			calls: self.calls.into_iter().map(|c| c.map_error(&f)).collect(),
		}
	}
}

impl<E> SimulationResponse<E> {
	/// Map the error type of all call results in this response.
	pub fn map_error<F>(self, f: impl Fn(E) -> F) -> SimulationResponse<F> {
		SimulationResponse(self.0.into_iter().map(|b| b.map_error(&f)).collect())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

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
	fn test_block_serialization_roundtrip() {
		let json_input = r#"{
			"baseFeePerGas": "0x126f2347",
			"blobGasUsed": "0x100000",
			"difficulty": "0x0",
			"excessBlobGas": "0x0",
			"extraData": "0x546974616e2028746974616e6275696c6465722e78797a29",
			"gasLimit": "0x2aca2c9",
			"gasUsed": "0x1c06043",
			"hash": "0xe6064637def8a5a9a90c8a666005975e4a6c46acf8af57e1f2adb20dfced133a",
			"logsBloom": "0xbf7bf1afcf57ea95fbb5c6fd8db37db9dbffec27cfc6a39b3417e7786defd7e3d6fd577ecddd5676eee8bf79df8faddcefa7e169def77f7e7d6dbbfd1dfef9aebd9e707b4c4ed979fda2cdeeb96b3bfed5d5fabb68ff9e7f2dfb075eff643a93feebbc07877f0dff66fedf4ede0fbcfbf56f98a1626eaed77ed4e6be388f162f9b2deeff1eefa93bdacbf3fbbd7b6757cddb7ae5b3f9b7af9c3bbff7e7f6ddef9f2dff7f17997ea6867675c29fcbe6bf725efbffe1507589bfd47a3bf7b6f5dfde50776fd94fe772d2c7b6b58baf554de55c176f27efa6fdcff7f17689bafa7f7c7bf4fd5fb9b05c2f4ed785f17ac9779feeaf1f5bbdadfc42ebad367fdcf7ad",
			"miner": "0x4838b106fce9647bdf1e7877bf73ce8b0bad5f97",
			"mixHash": "0x7e53d2d6772895d024eb00da80213aec81fb4a15bec34a5a39403ad6162274af",
			"nonce": "0x0000000000000000",
			"number": "0x1606672",
			"parentBeaconBlockRoot": "0xd9ef51c8f4155f238ba66df0d35a4d0a6bb043c0dacb5c5dbd5a231bbd4c8a01",
			"parentHash": "0x37b527c98c86436f292d4e19fac3aba6d8c7768684ea972f50adc305fd9a1475",
			"receiptsRoot": "0x2abab67c41b350435eb34f9dc0478dd7d262f35544cecf62a85af2da075bd38d",
			"requestsHash": "0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
			"sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
			"size": "0x29e6c",
			"stateRoot": "0x5159c56472adff9a760275ac63524a71f5645ede822a5547dd9ad333586d5157",
			"timestamp": "0x6895a93f",
			"transactions": [],
			"transactionsRoot": "0xfb0b9f5b28bc927db98d82e18070d2b17434c31bd2773c5dd699e96fa76a34cd",
			"uncles": [],
			"withdrawals": [],
			"withdrawalsRoot": "0x531480435633d56a52433b33f41ac9322f51a2df3364c4c112236fc6ac583118"
		}"#;

		// Deserialize the JSON into a Block
		let block: Block = serde_json::from_str(json_input).expect("Failed to deserialize block");

		// Serialize it back to JSON
		let serialized = serde_json::to_string(&block).expect("Failed to serialize block");

		// Deserialize again to ensure roundtrip consistency
		let block_roundtrip: Block =
			serde_json::from_str(&serialized).expect("Failed to deserialize roundtrip block");

		// Verify that deserializing and serializing leads to the same result
		assert_eq!(block, block_roundtrip);
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
	fn test_simulation_parameters_minimal_request() {
		// Arrange
		let json = r#"{
			"blockStateCalls": [
				{
					"calls": []
				}
			]
		}"#;

		// Act
		let params: SimulationParameters = serde_json::from_str(json).unwrap();

		// Assert
		assert_eq!(params.block_state_calls.len(), 1);
		assert!(!params.trace_transfers);
		assert!(!params.validation);
		assert!(!params.return_full_transactions);
		assert!(params.block_state_calls[0].block_overrides.is_none());
		assert!(params.block_state_calls[0].state_overrides.is_none());
		assert!(params.block_state_calls[0].calls.is_empty());
	}

	#[test]
	fn test_simulation_parameters_all_flags_true() {
		// Arrange
		let json = r#"{
			"blockStateCalls": [{"calls": []}],
			"traceTransfers": true,
			"validation": true,
			"returnFullTransactions": true
		}"#;

		// Act
		let params: SimulationParameters = serde_json::from_str(json).unwrap();

		// Assert
		assert!(params.trace_transfers);
		assert!(params.validation);
		assert!(params.return_full_transactions);
	}

	#[test]
	fn test_simulation_parameters_roundtrip() {
		// Arrange
		let json = r#"{
			"blockStateCalls": [{"calls": []}],
			"traceTransfers": true,
			"validation": false,
			"returnFullTransactions": true
		}"#;

		// Act
		let params: SimulationParameters = serde_json::from_str(json).unwrap();
		let serialized = serde_json::to_string(&params).unwrap();
		let roundtrip: SimulationParameters = serde_json::from_str(&serialized).unwrap();

		// Assert
		assert_eq!(params, roundtrip);
	}

	#[test]
	fn test_simulation_parameters_camel_case_field_names() {
		// Arrange
		let params = SimulationParameters {
			block_state_calls: vec![SimulationPayload {
				block_overrides: None,
				state_overrides: None,
				calls: vec![],
			}],
			trace_transfers: true,
			validation: true,
			return_full_transactions: false,
		};

		// Act
		let value = serde_json::to_value(&params).unwrap();

		// Assert
		assert!(value.get("blockStateCalls").is_some());
		assert!(value.get("traceTransfers").is_some());
		assert!(value.get("validation").is_some());
		assert!(value.get("returnFullTransactions").is_some());
		assert!(value.get("block_state_calls").is_none());
		assert!(value.get("trace_transfers").is_none());
		assert!(value.get("return_full_transactions").is_none());
	}

	#[test]
	fn test_simulation_parameters_multi_block() {
		// Arrange
		let json = r#"{
			"blockStateCalls": [
				{
					"stateOverrides": {
						"0xc000000000000000000000000000000000000000": {
							"balance": "0x4a817c800"
						}
					},
					"calls": [
						{
							"from": "0xc000000000000000000000000000000000000000",
							"to": "0xc000000000000000000000000000000000000001",
							"maxFeePerGas": "0xf",
							"value": "0x1"
						}
					]
				},
				{
					"calls": [
						{
							"from": "0xc000000000000000000000000000000000000000",
							"to": "0xc000000000000000000000000000000000000002",
							"maxFeePerGas": "0xf",
							"value": "0x1"
						}
					]
				}
			]
		}"#;

		// Act
		let params: SimulationParameters = serde_json::from_str(json).unwrap();

		// Assert
		assert_eq!(params.block_state_calls.len(), 2);
		assert!(params.block_state_calls[0].state_overrides.is_some());
		assert!(params.block_state_calls[1].state_overrides.is_none());
		assert_eq!(params.block_state_calls[0].calls.len(), 1);
		assert_eq!(params.block_state_calls[1].calls.len(), 1);
	}

	#[test]
	fn test_simulation_payload_optional_fields_omitted_on_serialize() {
		// Arrange
		let payload =
			SimulationPayload { block_overrides: None, state_overrides: None, calls: vec![] };

		// Act
		let value = serde_json::to_value(&payload).unwrap();

		// Assert
		assert!(value.get("blockOverrides").is_none());
		assert!(value.get("stateOverrides").is_none());
		assert!(value.get("calls").is_some());
	}

	#[test]
	fn test_simulation_payload_with_all_fields() {
		// Arrange
		let json = r#"{
			"blockOverrides": {
				"number": "0xa"
			},
			"stateOverrides": {
				"0xc000000000000000000000000000000000000000": {
					"balance": "0x1"
				}
			},
			"calls": [
				{
					"from": "0xc000000000000000000000000000000000000000",
					"to": "0xc000000000000000000000000000000000000001",
					"value": "0x1"
				}
			]
		}"#;

		// Act
		let payload: SimulationPayload = serde_json::from_str(json).unwrap();

		// Assert
		assert!(payload.block_overrides.is_some());
		assert_eq!(payload.block_overrides.as_ref().unwrap().number, Some(U256::from(10)));
		assert!(payload.state_overrides.is_some());
		assert_eq!(payload.calls.len(), 1);
	}

	#[test]
	fn test_block_overrides_empty() {
		// Arrange
		let json = r#"{}"#;

		// Act
		let overrides: BlockOverrides = serde_json::from_str(json).unwrap();

		// Assert
		assert_eq!(overrides, BlockOverrides::default());
	}

	#[test]
	fn test_block_overrides_all_fields() {
		// Arrange
		let json = r#"{
			"number": "0xa",
			"prevRandao": "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
			"time": "0x64",
			"gasLimit": "0x1c9c380",
			"feeRecipient": "0x1234567890abcdef1234567890abcdef12345678",
			"baseFeePerGas": "0x9",
			"blobBaseFee": "0x1",
			"withdrawals": [
				{
					"index": "0x0",
					"validatorIndex": "0x1",
					"address": "0x0000000000000000000000000000000000000000",
					"amount": "0x1"
				}
			]
		}"#;

		// Act
		let overrides: BlockOverrides = serde_json::from_str(json).unwrap();

		// Assert
		assert_eq!(overrides.number, Some(U256::from(10)));
		assert!(overrides.prev_randao.is_some());
		assert_eq!(overrides.time, Some(U256::from(100)));
		assert_eq!(overrides.gas_limit, Some(U256::from(30_000_000)));
		assert!(overrides.fee_recipient.is_some());
		assert_eq!(overrides.base_fee_per_gas, Some(U256::from(9)));
		assert_eq!(overrides.blob_base_fee, Some(U256::from(1)));
		assert_eq!(overrides.withdrawals.as_ref().unwrap().len(), 1);
	}

	#[test]
	fn test_block_overrides_omits_none_on_serialize() {
		// Arrange
		let overrides = BlockOverrides { number: Some(U256::from(10)), ..Default::default() };

		// Act
		let value = serde_json::to_value(&overrides).unwrap();

		// Assert
		assert!(value.get("number").is_some());
		assert!(value.get("prevRandao").is_none());
		assert!(value.get("time").is_none());
		assert!(value.get("gasLimit").is_none());
		assert!(value.get("feeRecipient").is_none());
		assert!(value.get("baseFeePerGas").is_none());
		assert!(value.get("blobBaseFee").is_none());
		assert!(value.get("withdrawals").is_none());
	}

	#[test]
	fn test_block_overrides_roundtrip() {
		// Arrange
		let overrides = BlockOverrides {
			number: Some(U256::from(10)),
			prev_randao: Some(U256::from(42)),
			time: Some(U256::from(100)),
			gas_limit: Some(U256::from(30_000_000)),
			fee_recipient: Some(H160::from_low_u64_be(1)),
			base_fee_per_gas: Some(U256::from(9)),
			blob_base_fee: Some(U256::from(1)),
			withdrawals: Some(vec![Withdrawal {
				index: U256::from(0),
				validator_index: U256::from(1),
				address: Address::zero(),
				amount: U256::from(1),
			}]),
		};

		// Act
		let serialized = serde_json::to_string(&overrides).unwrap();
		let roundtrip: BlockOverrides = serde_json::from_str(&serialized).unwrap();

		// Assert
		assert_eq!(overrides, roundtrip);
	}

	#[test]
	fn test_state_overrides_empty_map() {
		// Arrange
		let json = r#"{}"#;

		// Act
		let overrides: StateOverrides = serde_json::from_str(json).unwrap();

		// Assert
		assert!(overrides.0.is_empty());
	}

	#[test]
	fn test_state_overrides_balance_only() {
		// Arrange
		let json = r#"{
			"0xc000000000000000000000000000000000000000": {
				"balance": "0x4a817c800"
			}
		}"#;

		// Act
		let overrides: StateOverrides = serde_json::from_str(json).unwrap();

		// Assert
		assert_eq!(overrides.0.len(), 1);
		let addr: H160 = "0xc000000000000000000000000000000000000000".parse().unwrap();
		let account = overrides.0.get(&addr).unwrap();
		assert_eq!(account.balance, Some(U256::from(20_000_000_000u64)));
	}

	#[test]
	fn test_state_overrides_all_account_fields() {
		// Arrange
		let json = r#"{
			"0xc000000000000000000000000000000000000000": {
				"balance": "0x1",
				"nonce": "0xa",
				"code": "0x6080",
				"state": {
					"0x0000000000000000000000000000000000000000000000000000000000000000": "0x0000000000000000000000000000000000000000000000000000000000000001"
				},
				"movePrecompileToAddress": "0x0000000000000000000000000000000000000123"
			}
		}"#;

		// Act
		let overrides: StateOverrides = serde_json::from_str(json).unwrap();

		// Assert
		let addr: H160 = "0xc000000000000000000000000000000000000000".parse().unwrap();
		let account = overrides.0.get(&addr).unwrap();
		assert_eq!(account.balance, Some(U256::from(1)));
		assert_eq!(account.nonce, Some(U256::from(10)));
		assert!(account.code.is_some());
		assert!(matches!(
			account.storage.as_ref(),
			Some(StorageOverrides::AllStorageSlots { overrides })
			if overrides.len() == 1
		));
		assert!(account.move_precompile_to_address.is_some());
	}

	#[test]
	fn test_state_overrides_state_diff() {
		// Arrange
		let json = r#"{
			"0xc000000000000000000000000000000000000000": {
				"stateDiff": {
					"0x0000000000000000000000000000000000000000000000000000000000000001": "0x00000000000000000000000000000000000000000000000000000000000000ff"
				}
			}
		}"#;

		// Act
		let overrides: StateOverrides = serde_json::from_str(json).unwrap();

		// Assert
		let addr: H160 = "0xc000000000000000000000000000000000000000".parse().unwrap();
		let account = overrides.0.get(&addr).unwrap();
		let Some(StorageOverrides::SpecificStorageSlots { overrides }) = account.storage.as_ref()
		else {
			panic!("Storage overrides were not a state diff")
		};
		assert_eq!(overrides.len(), 1);
	}

	#[test]
	fn test_state_overrides_multiple_accounts() {
		// Arrange
		let json = r#"{
			"0xc000000000000000000000000000000000000000": {
				"balance": "0x1"
			},
			"0xc000000000000000000000000000000000000001": {
				"balance": "0x2"
			}
		}"#;

		// Act
		let overrides: StateOverrides = serde_json::from_str(json).unwrap();

		// Assert
		assert_eq!(overrides.0.len(), 2);
	}

	#[test]
	fn test_state_overrides_roundtrip() {
		// Arrange
		let json = r#"{
			"0xc000000000000000000000000000000000000000": {
				"balance": "0x4a817c800",
				"nonce": "0x1",
				"code": "0x6080"
			}
		}"#;

		// Act
		let overrides: StateOverrides = serde_json::from_str(json).unwrap();
		let serialized = serde_json::to_string(&overrides).unwrap();
		let roundtrip: StateOverrides = serde_json::from_str(&serialized).unwrap();

		// Assert
		assert_eq!(overrides, roundtrip);
	}

	#[test]
	fn test_simulation_call_result_success() {
		// Arrange
		let json = r#"{
			"status": "0x1",
			"returnData": "0x",
			"gasUsed": "0x5208",
			"logs": []
		}"#;

		// Act
		let result: SimulationCallResult<serde_json::Value> = serde_json::from_str(json).unwrap();

		// Assert
		match &result {
			SimulationCallResult::Success { return_data, gas_used, logs } => {
				assert!(return_data.0.is_empty());
				assert_eq!(*gas_used, U256::from(21000));
				assert!(logs.is_empty());
			},
			_ => panic!("Expected Success variant"),
		}
	}

	#[test]
	fn test_simulation_call_result_success_with_logs() {
		// Arrange
		let json = r#"{
			"status": "0x1",
			"returnData": "0x",
			"gasUsed": "0x5208",
			"logs": [
				{
					"address": "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
					"topics": [
						"0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"
					],
					"data": "0x0000000000000000000000000000000000000000000000000000000000000001",
					"blockNumber": "0xa",
					"transactionHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
					"transactionIndex": "0x0",
					"blockHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
					"logIndex": "0x0",
					"removed": false
				}
			]
		}"#;

		// Act
		let result: SimulationCallResult<serde_json::Value> = serde_json::from_str(json).unwrap();

		// Assert
		match &result {
			SimulationCallResult::Success { logs, .. } => {
				assert_eq!(logs.len(), 1);
				let expected_addr = H160::from_slice(&[0xee; 20]);
				assert_eq!(logs[0].address, expected_addr);
				assert_eq!(logs[0].topics.len(), 1);
			},
			_ => panic!("Expected Success variant"),
		}
	}

	#[test]
	fn test_simulation_call_result_failed() {
		// Arrange
		let json = r#"{
			"status": "0x0",
			"returnData": "0x",
			"gasUsed": "0x5236",
			"error": {
				"message": "execution reverted",
				"code": -32000,
				"data": "0x"
			}
		}"#;

		// Act
		let result: SimulationCallResult<serde_json::Value> = serde_json::from_str(json).unwrap();

		// Assert
		match &result {
			SimulationCallResult::Failed { return_data, gas_used, error } => {
				assert!(return_data.0.is_empty());
				assert_eq!(*gas_used, U256::from(0x5236));
				assert_eq!(error.get("message").unwrap().as_str().unwrap(), "execution reverted");
				assert_eq!(error.get("code").unwrap().as_i64().unwrap(), -32000);
			},
			_ => panic!("Expected Failed variant"),
		}
	}

	#[test]
	fn test_simulation_call_result_success_serialization() {
		// Arrange
		let result: SimulationCallResult<serde_json::Value> = SimulationCallResult::Success {
			return_data: Bytes(vec![]),
			gas_used: U256::from(21000),
			logs: vec![],
		};

		// Act
		let value = serde_json::to_value(&result).unwrap();

		// Assert
		assert_eq!(value.get("status").unwrap().as_str().unwrap(), "0x1");
		assert!(value.get("returnData").is_some());
		assert!(value.get("gasUsed").is_some());
		assert!(value.get("logs").is_some());
		assert!(value.get("error").is_none());
	}

	#[test]
	fn test_simulation_call_result_failed_serialization() {
		// Arrange
		let error = serde_json::json!({
			"message": "execution reverted",
			"code": -32000
		});
		let result: SimulationCallResult<serde_json::Value> = SimulationCallResult::Failed {
			return_data: Bytes(vec![]),
			gas_used: U256::from(0x5236),
			error,
		};

		// Act
		let value = serde_json::to_value(&result).unwrap();

		// Assert
		assert_eq!(value.get("status").unwrap().as_str().unwrap(), "0x0");
		assert!(value.get("error").is_some());
		assert!(value.get("logs").is_none());
	}

	#[test]
	fn test_simulation_call_result_roundtrip_success() {
		// Arrange
		let result: SimulationCallResult<serde_json::Value> = SimulationCallResult::Success {
			return_data: Bytes(vec![0xab, 0xcd]),
			gas_used: U256::from(42000),
			logs: vec![],
		};

		// Act
		let serialized = serde_json::to_string(&result).unwrap();
		let roundtrip: SimulationCallResult<serde_json::Value> =
			serde_json::from_str(&serialized).unwrap();

		// Assert
		assert_eq!(result, roundtrip);
	}

	#[test]
	fn test_simulation_call_result_roundtrip_failed() {
		// Arrange
		let error = serde_json::json!({
			"message": "out of gas",
			"code": -32015
		});
		let result: SimulationCallResult<serde_json::Value> = SimulationCallResult::Failed {
			return_data: Bytes(vec![]),
			gas_used: U256::from(30_000_000),
			error,
		};

		// Act
		let serialized = serde_json::to_string(&result).unwrap();
		let roundtrip: SimulationCallResult<serde_json::Value> =
			serde_json::from_str(&serialized).unwrap();

		// Assert
		assert_eq!(result, roundtrip);
	}

	#[test]
	fn test_full_simulation_request_deserialization() {
		// Arrange
		let json = r#"{
			"blockStateCalls": [
				{
					"blockOverrides": {
						"number": "0xa",
						"feeRecipient": "0x1234567890abcdef1234567890abcdef12345678",
						"baseFeePerGas": "0x9"
					},
					"stateOverrides": {
						"0xc000000000000000000000000000000000000000": {
							"balance": "0x4a817c800"
						}
					},
					"calls": [
						{
							"from": "0xc000000000000000000000000000000000000000",
							"to": "0xc000000000000000000000000000000000000001",
							"maxFeePerGas": "0xf",
							"value": "0x1"
						}
					]
				}
			],
			"traceTransfers": true,
			"validation": false
		}"#;

		// Act
		let params: SimulationParameters = serde_json::from_str(json).unwrap();

		// Assert
		assert!(params.trace_transfers);
		assert!(!params.validation);
		assert!(!params.return_full_transactions);

		let block = &params.block_state_calls[0];
		let bo = block.block_overrides.as_ref().unwrap();
		assert_eq!(bo.number, Some(U256::from(10)));
		assert_eq!(bo.base_fee_per_gas, Some(U256::from(9)));
		assert!(bo.fee_recipient.is_some());

		let so = block.state_overrides.as_ref().unwrap();
		assert_eq!(so.0.len(), 1);

		let call = &block.calls[0];
		assert!(call.from.is_some());
		assert!(call.to.is_some());
		assert_eq!(call.value, Some(U256::from(1)));
		assert_eq!(call.max_fee_per_gas, Some(U256::from(15)));
	}

	#[test]
	fn test_full_simulation_response_deserialization() {
		// Arrange
		let json = r#"[{
			"baseFeePerGas": "0x9",
			"blobGasUsed": "0x0",
			"difficulty": "0x0",
			"excessBlobGas": "0x0",
			"extraData": "0x",
			"gasLimit": "0x1c9c380",
			"gasUsed": "0x5208",
			"hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
			"miner": "0x0000000000000000000000000000000000000000",
			"mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"nonce": "0x0000000000000000",
			"number": "0xa",
			"parentHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"receiptsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
			"size": "0x0",
			"stateRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"timestamp": "0x64",
			"transactions": [],
			"transactionsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"uncles": [],
			"withdrawals": [],
			"withdrawalsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"calls": [
				{
					"status": "0x1",
					"returnData": "0x",
					"gasUsed": "0x5208",
					"logs": []
				}
			]
		}]"#;
		// Act
		let response: SimulationResponse<serde_json::Value> = serde_json::from_str(json).unwrap();

		// Assert
		assert_eq!(response.0.len(), 1);
		let block = &response.0[0];
		assert_eq!(block.number, U256::from(10));
		assert_eq!(block.timestamp, U256::from(100));
		assert_eq!(block.base_fee_per_gas, U256::from(9));
		assert_eq!(block.gas_limit, U256::from(30_000_000));
		assert_eq!(block.gas_used, U256::from(21000));
		assert_eq!(block.calls.len(), 1);
		assert!(matches!(block.calls[0], SimulationCallResult::Success { .. }));
	}

	#[test]
	fn test_simulation_response_with_failed_call() {
		// Arrange
		let json = r#"[{
			"baseFeePerGas": "0x9",
			"blobGasUsed": "0x0",
			"difficulty": "0x0",
			"excessBlobGas": "0x0",
			"extraData": "0x",
			"gasLimit": "0x1c9c380",
			"gasUsed": "0x5236",
			"hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
			"miner": "0x0000000000000000000000000000000000000000",
			"mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"nonce": "0x0000000000000000",
			"number": "0xa",
			"parentHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"receiptsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
			"size": "0x0",
			"stateRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"timestamp": "0x64",
			"transactions": [],
			"transactionsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"uncles": [],
			"withdrawals": [],
			"withdrawalsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"calls": [
				{
					"status": "0x0",
					"returnData": "0x",
					"gasUsed": "0x5236",
					"error": {
						"message": "execution reverted",
						"code": -32000
					}
				}
			]
		}]"#;
		// Act
		let response: SimulationResponse<serde_json::Value> = serde_json::from_str(json).unwrap();

		// Assert
		let block = &response.0[0];
		match &block.calls[0] {
			SimulationCallResult::Failed { error, .. } => {
				assert_eq!(error.get("message").unwrap().as_str().unwrap(), "execution reverted");
				assert_eq!(error.get("code").unwrap().as_i64().unwrap(), -32000);
			},
			_ => panic!("Expected Failed variant"),
		}
	}

	#[test]
	fn test_simulation_response_with_optional_block_fields() {
		// Arrange
		let json = r#"[{
			"baseFeePerGas": "0x9",
			"blobGasUsed": "0x0",
			"difficulty": "0x0",
			"excessBlobGas": "0x0",
			"extraData": "0x",
			"gasLimit": "0x1c9c380",
			"gasUsed": "0x0",
			"hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
			"miner": "0x0000000000000000000000000000000000000000",
			"mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"nonce": "0x0000000000000000",
			"number": "0xa",
			"parentBeaconBlockRoot": "0x0000000000000000000000000000000000000000000000000000000000000001",
			"parentHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"receiptsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"requestsHash": "0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
			"sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
			"size": "0x0",
			"stateRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"timestamp": "0x64",
			"totalDifficulty": "0x0",
			"transactions": [],
			"transactionsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"uncles": [],
			"withdrawals": [],
			"withdrawalsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"calls": []
		}]"#;
		// Act
		let response: SimulationResponse<serde_json::Value> = serde_json::from_str(json).unwrap();

		// Assert
		let block = &response.0[0];
		assert!(block.parent_beacon_block_root.is_some());
		assert_eq!(block.parent_beacon_block_root.unwrap(), H256::from_low_u64_be(1));
		assert!(block.requests_hash.is_some());
		assert!(block.total_difficulty.is_some());
		assert_eq!(block.total_difficulty.unwrap(), U256::from(0));
	}

	#[test]
	fn test_simulation_response_optional_fields_omitted() {
		// Arrange
		let json = r#"[{
			"baseFeePerGas": "0x9",
			"blobGasUsed": "0x0",
			"difficulty": "0x0",
			"excessBlobGas": "0x0",
			"extraData": "0x",
			"gasLimit": "0x1c9c380",
			"gasUsed": "0x0",
			"hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
			"miner": "0x0000000000000000000000000000000000000000",
			"mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"nonce": "0x0000000000000000",
			"number": "0xa",
			"parentHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"receiptsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
			"size": "0x0",
			"stateRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"timestamp": "0x64",
			"transactions": [],
			"transactionsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"uncles": [],
			"withdrawals": [],
			"withdrawalsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"calls": []
		}]"#;
		// Act
		let response: SimulationResponse<serde_json::Value> = serde_json::from_str(json).unwrap();

		// Assert
		let block = &response.0[0];
		assert!(block.parent_beacon_block_root.is_none());
		assert!(block.requests_hash.is_none());
		assert!(block.total_difficulty.is_none());
		let value = serde_json::to_value(&response).unwrap();
		let block_value = &value[0];
		assert!(block_value.get("parentBeaconBlockRoot").is_none());
		assert!(block_value.get("requestsHash").is_none());
		assert!(block_value.get("totalDifficulty").is_none());
	}

	#[test]
	fn test_simulation_response_multi_block_multi_call() {
		// Arrange
		let json = r#"[
			{
				"baseFeePerGas": "0x9",
				"blobGasUsed": "0x0",
				"difficulty": "0x0",
				"excessBlobGas": "0x0",
				"extraData": "0x",
				"gasLimit": "0x1c9c380",
				"gasUsed": "0xa410",
				"hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
				"logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
				"miner": "0x0000000000000000000000000000000000000000",
				"mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
				"nonce": "0x0000000000000000",
				"number": "0xa",
				"parentHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
				"receiptsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
				"sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
				"size": "0x0",
				"stateRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
				"timestamp": "0x64",
				"transactions": [],
				"transactionsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
				"uncles": [],
				"withdrawals": [],
				"withdrawalsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
				"calls": [
					{
						"status": "0x1",
						"returnData": "0x",
						"gasUsed": "0x5208",
						"logs": []
					},
					{
						"status": "0x1",
						"returnData": "0x",
						"gasUsed": "0x5208",
						"logs": []
					}
				]
			},
			{
				"baseFeePerGas": "0x8",
				"blobGasUsed": "0x0",
				"difficulty": "0x0",
				"excessBlobGas": "0x0",
				"extraData": "0x",
				"gasLimit": "0x1c9c380",
				"gasUsed": "0x5208",
				"hash": "0x0000000000000000000000000000000000000000000000000000000000000001",
				"logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
				"miner": "0x0000000000000000000000000000000000000000",
				"mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
				"nonce": "0x0000000000000000",
				"number": "0xb",
				"parentHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
				"receiptsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
				"sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
				"size": "0x0",
				"stateRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
				"timestamp": "0x65",
				"transactions": [],
				"transactionsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
				"uncles": [],
				"withdrawals": [],
				"withdrawalsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
				"calls": [
					{
						"status": "0x0",
						"returnData": "0x",
						"gasUsed": "0x5208",
						"error": {
							"message": "insufficient funds",
							"code": -32000
						}
					}
				]
			}
		]"#;
		// Act
		let response: SimulationResponse<serde_json::Value> = serde_json::from_str(json).unwrap();

		// Assert
		assert_eq!(response.0.len(), 2);
		assert_eq!(response.0[0].number, U256::from(10));
		assert_eq!(response.0[0].calls.len(), 2);
		assert!(matches!(response.0[0].calls[0], SimulationCallResult::Success { .. }));
		assert!(matches!(response.0[0].calls[1], SimulationCallResult::Success { .. }));
		assert_eq!(response.0[1].number, U256::from(11));
		assert_eq!(response.0[1].calls.len(), 1);
		assert!(matches!(response.0[1].calls[0], SimulationCallResult::Failed { .. }));
	}

	#[test]
	fn test_simulation_response_roundtrip() {
		// Arrange
		let block: SimulationBlock<serde_json::Value> = SimulationBlock {
			base_fee_per_gas: U256::from(9),
			blob_gas_used: U256::zero(),
			difficulty: U256::zero(),
			excess_blob_gas: U256::zero(),
			extra_data: Bytes(vec![]),
			gas_limit: U256::from(30_000_000),
			gas_used: U256::from(21000),
			hash: H256::zero(),
			logs_bloom: Bytes256([0u8; 256]),
			miner: Address::zero(),
			mix_hash: H256::zero(),
			nonce: Bytes8([0u8; 8]),
			number: U256::from(10),
			parent_beacon_block_root: None,
			parent_hash: H256::zero(),
			receipts_root: H256::zero(),
			requests_hash: None,
			sha_3_uncles: H256::zero(),
			size: U256::zero(),
			state_root: H256::zero(),
			timestamp: U256::from(100),
			total_difficulty: None,
			transactions: HashesOrTransactionInfos::Hashes(vec![]),
			transactions_root: H256::zero(),
			uncles: vec![],
			withdrawals: vec![],
			withdrawals_root: H256::zero(),
			calls: vec![SimulationCallResult::Success {
				return_data: Bytes(vec![]),
				gas_used: U256::from(21000),
				logs: vec![],
			}],
		};
		let response = SimulationResponse(vec![block]);

		// Act
		let serialized = serde_json::to_string(&response).unwrap();
		let roundtrip: SimulationResponse<serde_json::Value> =
			serde_json::from_str(&serialized).unwrap();

		// Assert
		assert_eq!(response, roundtrip);
	}

	#[test]
	fn test_address_state_override_camel_case_fields() {
		// Arrange
		let json = r#"{
			"balance": "0x1",
			"nonce": "0x2",
			"code": "0x60",
			"stateDiff": {
				"0x0000000000000000000000000000000000000000000000000000000000000000": "0x0000000000000000000000000000000000000000000000000000000000000001"
			},
			"movePrecompileToAddress": "0x0000000000000000000000000000000000000001"
		}"#;
		// Act
		let override_account: AddressStateOverride = serde_json::from_str(json).unwrap();

		// Assert
		assert_eq!(override_account.balance, Some(U256::from(1)));
		assert_eq!(override_account.nonce, Some(U256::from(2)));
		assert!(override_account.code.is_some());
		let Some(StorageOverrides::SpecificStorageSlots { overrides }) =
			override_account.storage.as_ref()
		else {
			panic!("Storage overrides were not a state diff")
		};
		assert_eq!(overrides.len(), 1);
		assert!(override_account.move_precompile_to_address.is_some());
	}

	#[test]
	fn test_block_decode() {
		let json = r#"{
			"baseFeePerGas": "0x23cf3fd4",
			"blobGasUsed": "0x0",
			"difficulty": "0x0",
			"excessBlobGas": "0x80000",
			"extraData": "0x546974616e2028746974616e6275696c6465722e78797a29",
			"gasLimit": "0x2aea4ea",
			"gasUsed": "0xe36e2f",
			"hash": "0xfb8c980d1da1a75e68c2ea4d55cb88d62dedbbb5eaf69df8fe337e9f6922b73a",
			"logsBloom": "0xb56c514421c05ba024436428e2487b83134983e9c650686421bd10588512e0a9a55d51e8e84c868446517ed5e90609dd43aad1edcc1462b8e8f15763b3ff6e62a506d3d910d0aae829786fac994a6de34860263be47eb8300e91dd2cc3110a22ba0d60008e6a0362c5a3ffd5aa18acc8c22b6fe02c54273b12a841bc958c9ae12378bc0e5881c2d840ff677f8038243216e5c105e58819bc0cbb8c56abb7e490cf919ceb85702e5d54dece9332a00c9e6ade9cb47d42440201ecd7704088236b39037c9ff189286e3e5d6657aa389c2d482e337af5cfc45b0d25ad0e300c2b6bf599bc2007008830226612a4e7e7cae4e57c740205a809dc280825165b98559c",
			"miner": "0x4838b106fce9647bdf1e7877bf73ce8b0bad5f97",
			"mixHash": "0x11b02e97eaa48bc83cbb6f9478f32eaf7e8b67fead4edeef945822612f1854f6",
			"nonce": "0x0000000000000000",
			"number": "0x161bd0f",
			"parentBeaconBlockRoot": "0xd8266eb7bb40e4e5e3beb9caed7ccaa448ce55203a03705c87860deedcf7236d",
			"parentHash": "0x7c9625cc198af5cf677a15cdc38da3cf64d57b9729de5bd1c96b3c556a84aa7d",
			"receiptsRoot": "0x758614638725ede86a2f4c8339eb79b84ae346915319dc286643c9324e34f28a",
			"requestsHash": "0xd9267a5ab4782c4e0bdc5fcd2fefb53c91f92f91b6059c8f13343a0691ba77d1",
			"sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
			"size": "0x14068",
			"stateRoot": "0x7ed9726e3172886af5301968c2ddb7c38f8adf99c99ec10fdfaab66c610854bb",
			"timestamp": "0x68a5ce5b",
			"transactions": [
				{
				"blockHash": "0xfb8c980d1da1a75e68c2ea4d55cb88d62dedbbb5eaf69df8fe337e9f6922b73a",
				"blockNumber": "0x161bd0f",
				"from": "0x693ca5c6852a7d212dabc98b28e15257465c11f3",
				"gas": "0x70bdb",
				"gasPrice": "0x23cf3fd4",
				"maxPriorityFeePerGas": "0x0",
				"maxFeePerGas": "0x47ca802f",
				"hash": "0xf6d8b07ddcf9a9d44c99c3665fd8c78f0ccd32506350ea5a9be1a68ba08bfd1f",
				"input": "0x09c5eabe000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000002a90000cca0b86991c6218b36c1d19d4a2e9eb0ce3606eb48000000000000000000000000000000020000000000000000000000035c9618f600000000000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc20000000000000000000000002374fed200000000000000000001528fd550bc9a0000000000000000351e55bea6d51900dac17f958d2ee523a2206206994597c13d831ec7000000000000000000000000000000000000000000000000000000005c0c965e0000000000000000000000000000000000004c00000001000000000000000000000000000000000000002e24cd1d61a63f43658ed73b6ddeba00010002000100000000000000000000000000000000000000000000000039d622818daae62900006602000000000000000000002ff9e9686fa6ac00000000000000000000000000007f88ca000000000000000004caaa5ba8029c920300010000000000000000052319c661ddb06600000000000000000001528fd550bc9a0000000000000000005049606b67676100011c0c00000000000000002ff9e9686fa6ac000000000000000000000000035c16902c0000000000000000000000000000000200000000000000000000000000000002000073d53553ee552c1f2a9722e6407d43e41e19593f1cbc3d63300bfc6e48709f5b5ed98f228c70104e8c5d570b5608b47dca95ce6e371636965b6fdcab3613b6b65f061a44b7132011bb97a768bd238eacb62d7109920b000000000000000005246c56372e6d000000000000000000000000005c0c965e0000000000000000000000002374fed20000000000000000000000002374fed200011cc19621f6edbb9c02b95055b9f52eba0e2cb954c259f42aeca488551ea82b72f2504bbd310eb7145435e258751ab6854ab08b1630b89d6621dc1398c5d0c43b480000000000000000000000000000000000000000000000000000",
				"nonce": "0x40c6",
				"to": "0x0000000aa232009084bd71a5797d089aa4edfad4",
				"transactionIndex": "0x0",
				"value": "0x0",
				"type": "0x2",
				"accessList": [],
				"chainId": "0x1",
				"v": "0x1",
				"yParity": "0x1",
				"r": "0xb3e71bd95d73e965495b17647f5faaf058e13af7dd21f2af24eac16f7e9d06a1",
				"s": "0x58775b0c15075fb7f007b88e88605ae5daec1ffbac2771076e081c8c2b005c20"
				},
				{
				"blockHash": "0xfb8c980d1da1a75e68c2ea4d55cb88d62dedbbb5eaf69df8fe337e9f6922b73a",
				"blockNumber": "0x161bd0f",
				"from": "0x4791eb2224d272655e8d5da171bb07dd5a805ff6",
				"hash": "0xda8bc5dc5617758c6af0681d71642f68ce679bb92df4d8cf48493f0cfad14e20",
				"transactionIndex": "0x19",
				"gas": "0x186a0",
				"gasPrice": "0x6a5efc76",
				"maxPriorityFeePerGas": "0x6a5efc76",
				"maxFeePerGas": "0x6a5efc76",
				"input": "0x2c7bddf4",
				"nonce": "0x6233",
				"to": "0x62b53c45305d29bbe4b1bfa49dd78766b2f1e624",
				"value": "0x0",
				"type": "0x4",
				"accessList": [],
				"chainId": "0x1",
				"authorizationList": [
				],
				"v": "0x1",
				"yParity": "0x1",
				"r": "0x3b863c04d39f70e499ffb176376128a57481727116027a92a364b6e1668d13a7",
				"s": "0x39b13f0597c509de8260c7808057e64126e7d0715044dda908d1f513e1ed79ad"
				}
			],
			"transactionsRoot": "0xca2e7e6ebe1b08030fe5b9efabee82b95e62f07cff5a4298354002c46b41a216",
			"uncles": [],
			"withdrawals": [
			],
			"withdrawalsRoot": "0x7a3ad42fdb774c0e662597141f52a81210ffec9ce0db9dfcd841f747b0909010"
		}"#;

		let _result: Block = serde_json::from_str(json).unwrap();
	}
}
