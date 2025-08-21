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

use super::{byte::*, TypeEip1559, TypeEip2930, TypeEip4844, TypeLegacy};
use alloc::vec::Vec;
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
pub struct Block {
	/// Base fee per gas
	#[serde(rename = "baseFeePerGas", skip_serializing_if = "Option::is_none")]
	pub base_fee_per_gas: Option<U256>,
	/// Blob gas used
	#[serde(rename = "blobGasUsed", skip_serializing_if = "Option::is_none")]
	pub blob_gas_used: Option<U256>,
	/// Difficulty
	#[serde(skip_serializing_if = "Option::is_none")]
	pub difficulty: Option<U256>,
	/// Excess blob gas
	#[serde(rename = "excessBlobGas", skip_serializing_if = "Option::is_none")]
	pub excess_blob_gas: Option<U256>,
	/// Extra data
	#[serde(rename = "extraData")]
	pub extra_data: Bytes,
	/// Gas limit
	#[serde(rename = "gasLimit")]
	pub gas_limit: U256,
	/// Gas used
	#[serde(rename = "gasUsed")]
	pub gas_used: U256,
	/// Hash
	pub hash: H256,
	/// Bloom filter
	#[serde(rename = "logsBloom")]
	pub logs_bloom: Bytes256,
	/// Coinbase
	pub miner: Address,
	/// Mix hash
	#[serde(rename = "mixHash")]
	pub mix_hash: H256,
	/// Nonce
	pub nonce: Bytes8,
	/// Number
	pub number: U256,
	/// Parent Beacon Block Root
	#[serde(rename = "parentBeaconBlockRoot", skip_serializing_if = "Option::is_none")]
	pub parent_beacon_block_root: Option<H256>,
	/// Parent block hash
	#[serde(rename = "parentHash")]
	pub parent_hash: H256,
	/// Receipts root
	#[serde(rename = "receiptsRoot")]
	pub receipts_root: H256,
	/// Ommers hash
	#[serde(rename = "sha3Uncles")]
	pub sha_3_uncles: H256,
	/// Block size
	pub size: U256,
	/// State root
	#[serde(rename = "stateRoot")]
	pub state_root: H256,
	/// Timestamp
	pub timestamp: U256,
	/// Total difficulty
	#[serde(rename = "totalDifficulty", skip_serializing_if = "Option::is_none")]
	pub total_difficulty: Option<U256>,
	#[serde(rename = "transactions")]
	pub transactions: HashesOrTransactionInfos,
	/// Transactions root
	#[serde(rename = "transactionsRoot")]
	pub transactions_root: H256,
	/// Uncles
	pub uncles: Vec<H256>,
	/// Withdrawals
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub withdrawals: Vec<Withdrawal>,
	/// Withdrawals root
	#[serde(rename = "withdrawalsRoot", skip_serializing_if = "Option::is_none")]
	pub withdrawals_root: Option<H256>,
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

			BlockNumberOrTagOrHashWithAlias::NestedBlockNumber { block_number: val } =>
				BlockNumberOrTagOrHash::BlockNumber(val),
			BlockNumberOrTagOrHashWithAlias::BlockHash(val) |
			BlockNumberOrTagOrHashWithAlias::NestedBlockHash { block_hash: val } =>
				BlockNumberOrTagOrHash::BlockHash(val),
		})
	}
}

/// filter
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Filter {
	/// Address(es)
	pub address: Option<AddressOrAddresses>,
	/// from block
	#[serde(rename = "fromBlock", skip_serializing_if = "Option::is_none")]
	pub from_block: Option<BlockNumberOrTag>,
	/// to block
	#[serde(rename = "toBlock", skip_serializing_if = "Option::is_none")]
	pub to_block: Option<BlockNumberOrTag>,
	/// Restricts the logs returned to the single block
	#[serde(rename = "blockHash", skip_serializing_if = "Option::is_none")]
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
pub struct GenericTransaction {
	/// accessList
	/// EIP-2930 access list
	#[serde(rename = "accessList", skip_serializing_if = "Option::is_none")]
	pub access_list: Option<AccessList>,
	/// blobVersionedHashes
	/// List of versioned blob hashes associated with the transaction's EIP-4844 data blobs.
	#[serde(rename = "blobVersionedHashes", default, skip_serializing_if = "Vec::is_empty")]
	pub blob_versioned_hashes: Vec<H256>,
	/// blobs
	/// Raw blob data.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub blobs: Vec<Bytes>,
	/// chainId
	/// Chain ID that this transaction is valid on.
	#[serde(rename = "chainId", skip_serializing_if = "Option::is_none")]
	pub chain_id: Option<U256>,
	/// from address
	#[serde(skip_serializing_if = "Option::is_none")]
	pub from: Option<Address>,
	/// gas limit
	#[serde(skip_serializing_if = "Option::is_none")]
	pub gas: Option<U256>,
	/// gas price
	/// The gas price willing to be paid by the sender in wei
	#[serde(rename = "gasPrice", skip_serializing_if = "Option::is_none")]
	pub gas_price: Option<U256>,
	/// input data
	#[serde(flatten, deserialize_with = "deserialize_input_or_data")]
	pub input: InputOrData,
	/// max fee per blob gas
	/// The maximum total fee per gas the sender is willing to pay for blob gas in wei
	#[serde(rename = "maxFeePerBlobGas", skip_serializing_if = "Option::is_none")]
	pub max_fee_per_blob_gas: Option<U256>,
	/// max fee per gas
	/// The maximum total fee per gas the sender is willing to pay (includes the network / base fee
	/// and miner / priority fee) in wei
	#[serde(rename = "maxFeePerGas", skip_serializing_if = "Option::is_none")]
	pub max_fee_per_gas: Option<U256>,
	/// max priority fee per gas
	/// Maximum fee per gas the sender is willing to pay to miners in wei
	#[serde(rename = "maxPriorityFeePerGas", skip_serializing_if = "Option::is_none")]
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
pub struct ReceiptInfo {
	/// blob gas price
	/// The actual value per gas deducted from the sender's account for blob gas. Only specified
	/// for blob transactions as defined by EIP-4844.
	#[serde(rename = "blobGasPrice", skip_serializing_if = "Option::is_none")]
	pub blob_gas_price: Option<U256>,
	/// blob gas used
	/// The amount of blob gas used for this specific transaction. Only specified for blob
	/// transactions as defined by EIP-4844.
	#[serde(rename = "blobGasUsed", skip_serializing_if = "Option::is_none")]
	pub blob_gas_used: Option<U256>,
	/// block hash
	#[serde(rename = "blockHash")]
	pub block_hash: H256,
	/// block number
	#[serde(rename = "blockNumber")]
	pub block_number: U256,
	/// contract address
	/// The contract address created, if the transaction was a contract creation, otherwise null.
	#[serde(rename = "contractAddress")]
	pub contract_address: Option<Address>,
	/// cumulative gas used
	/// The sum of gas used by this transaction and all preceding transactions in the same block.
	#[serde(rename = "cumulativeGasUsed")]
	pub cumulative_gas_used: U256,
	/// effective gas price
	/// The actual value per gas deducted from the sender's account. Before EIP-1559, this is equal
	/// to the transaction's gas price. After, it is equal to baseFeePerGas + min(maxFeePerGas -
	/// baseFeePerGas, maxPriorityFeePerGas).
	#[serde(rename = "effectiveGasPrice")]
	pub effective_gas_price: U256,
	/// from
	pub from: Address,
	/// gas used
	/// The amount of gas used for this specific transaction alone.
	#[serde(rename = "gasUsed")]
	pub gas_used: U256,
	/// logs
	pub logs: Vec<Log>,
	/// logs bloom
	#[serde(rename = "logsBloom")]
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
	#[serde(rename = "transactionHash")]
	pub transaction_hash: H256,
	/// transaction index
	#[serde(rename = "transactionIndex")]
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
#[derive(
	Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
)]
pub struct TransactionInfo {
	/// block hash
	#[serde(rename = "blockHash")]
	pub block_hash: H256,
	/// block number
	#[serde(rename = "blockNumber")]
	pub block_number: U256,
	/// from address
	pub from: Address,
	/// transaction hash
	pub hash: H256,
	/// transaction index
	#[serde(rename = "transactionIndex")]
	pub transaction_index: U256,
	#[serde(flatten)]
	pub transaction_signed: TransactionSigned,
}

#[derive(Debug, Clone, Serialize, Deserialize, From, TryInto, Eq, PartialEq)]
#[serde(untagged)]
pub enum TransactionUnsigned {
	Transaction4844Unsigned(Transaction4844Unsigned),
	Transaction1559Unsigned(Transaction1559Unsigned),
	Transaction2930Unsigned(Transaction2930Unsigned),
	Transaction7702Unsigned(Transaction7702Unsigned),
	TransactionLegacyUnsigned(TransactionLegacyUnsigned),
}
impl Default for TransactionUnsigned {
	fn default() -> Self {
		TransactionUnsigned::TransactionLegacyUnsigned(Default::default())
	}
}

/// Authorization list
pub type AuthList = Vec<SetCodeAuthorizationEntry>;

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
pub enum BlockTag {
	#[serde(rename = "earliest")]
	Earliest,
	#[serde(rename = "finalized")]
	Finalized,
	#[serde(rename = "safe")]
	Safe,
	#[serde(rename = "latest")]
	#[default]
	Latest,
	#[serde(rename = "pending")]
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

/// log
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Log {
	/// address
	pub address: Address,
	/// block hash
	#[serde(rename = "blockHash")]
	pub block_hash: H256,
	/// block number
	#[serde(rename = "blockNumber")]
	pub block_number: U256,
	/// data
	#[serde(skip_serializing_if = "Option::is_none")]
	pub data: Option<Bytes>,
	/// log index
	#[serde(rename = "logIndex")]
	pub log_index: U256,
	/// removed
	#[serde(skip_serializing_if = "Option::is_none")]
	pub removed: Option<bool>,
	/// topics
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub topics: Vec<H256>,
	/// transaction hash
	#[serde(rename = "transactionHash")]
	pub transaction_hash: H256,
	/// transaction index
	#[serde(rename = "transactionIndex")]
	pub transaction_index: U256,
}

/// Syncing progress
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct SyncingProgress {
	/// Current block
	#[serde(rename = "currentBlock", skip_serializing_if = "Option::is_none")]
	pub current_block: Option<U256>,
	/// Highest block
	#[serde(rename = "highestBlock", skip_serializing_if = "Option::is_none")]
	pub highest_block: Option<U256>,
	/// Starting block
	#[serde(rename = "startingBlock", skip_serializing_if = "Option::is_none")]
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
pub struct Transaction1559Unsigned {
	/// accessList
	/// EIP-2930 access list
	#[serde(rename = "accessList")]
	pub access_list: AccessList,
	/// chainId
	/// Chain ID that this transaction is valid on.
	#[serde(rename = "chainId")]
	pub chain_id: U256,
	/// gas limit
	pub gas: U256,
	/// gas price
	/// The effective gas price paid by the sender in wei. For transactions not yet included in a
	/// block, this value should be set equal to the max fee per gas. This field is DEPRECATED,
	/// please transition to using effectiveGasPrice in the receipt object going forward.
	#[serde(rename = "gasPrice")]
	pub gas_price: U256,
	/// input data
	pub input: Bytes,
	/// max fee per gas
	/// The maximum total fee per gas the sender is willing to pay (includes the network / base fee
	/// and miner / priority fee) in wei
	#[serde(rename = "maxFeePerGas")]
	pub max_fee_per_gas: U256,
	/// max priority fee per gas
	/// Maximum fee per gas the sender is willing to pay to miners in wei
	#[serde(rename = "maxPriorityFeePerGas")]
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
pub struct Transaction2930Unsigned {
	/// accessList
	/// EIP-2930 access list
	#[serde(rename = "accessList")]
	pub access_list: AccessList,
	/// chainId
	/// Chain ID that this transaction is valid on.
	#[serde(rename = "chainId")]
	pub chain_id: U256,
	/// gas limit
	pub gas: U256,
	/// gas price
	/// The gas price willing to be paid by the sender in wei
	#[serde(rename = "gasPrice")]
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
pub struct Transaction4844Unsigned {
	/// accessList
	/// EIP-2930 access list
	#[serde(rename = "accessList")]
	pub access_list: AccessList,
	/// blobVersionedHashes
	/// List of versioned blob hashes associated with the transaction's EIP-4844 data blobs.
	#[serde(rename = "blobVersionedHashes")]
	pub blob_versioned_hashes: Vec<H256>,
	/// chainId
	/// Chain ID that this transaction is valid on.
	#[serde(rename = "chainId")]
	pub chain_id: U256,
	/// gas limit
	pub gas: U256,
	/// input data
	pub input: Bytes,
	/// max fee per blob gas
	/// The maximum total fee per gas the sender is willing to pay for blob gas in wei
	#[serde(rename = "maxFeePerBlobGas")]
	pub max_fee_per_blob_gas: U256,
	/// max fee per gas
	/// The maximum total fee per gas the sender is willing to pay (includes the network / base fee
	/// and miner / priority fee) in wei
	#[serde(rename = "maxFeePerGas")]
	pub max_fee_per_gas: U256,
	/// max priority fee per gas
	/// Maximum fee per gas the sender is willing to pay to miners in wei
	#[serde(rename = "maxPriorityFeePerGas")]
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

/// EIP-7702 transaction.
#[derive(
	Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
)]
pub struct Transaction7702Unsigned {
	/// chainId
	/// Chain ID that this transaction is valid on.
	#[serde(rename = "chainId")]
	pub chain_id: U256,
	/// nonce
	pub nonce: U256,
	/// gas limit
	pub gas: U256,
	/// gas price
	/// The gas price willing to be paid by the sender in wei
	#[serde(rename = "gasPrice")]
	pub gas_price: U256,
	/// gas price
	/// The gas price willing to be paid by the sender in wei
	#[serde(rename = "maxFeePerGas")]
	pub max_fee_per_gas: U256,
	/// The gas price willing to be paid by the sender in wei
	#[serde(rename = "maxPriorityFeePerGas")]
	pub max_priority_fee_per_gas: U256,
	/// to address
	pub to: Address,
	/// value
	pub value: U256,
	/// input data
	pub input: Bytes,
	/// accessList
	/// EIP-2930 access list
	#[serde(rename = "accessList")]
	pub access_list: AccessList,
	/// Authorization list.
	#[serde(rename = "authorizationList")]
	pub auth_list: Vec<SetCodeAuthorizationEntry>,
	/// type
	pub r#type: TypeEip2930,
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
pub struct TransactionLegacyUnsigned {
	/// chainId
	/// Chain ID that this transaction is valid on.
	#[serde(rename = "chainId", skip_serializing_if = "Option::is_none")]
	pub chain_id: Option<U256>,
	/// gas limit
	pub gas: U256,
	/// gas price
	/// The gas price willing to be paid by the sender in wei
	#[serde(rename = "gasPrice")]
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
	Transaction4844Signed(Transaction4844Signed),
	Transaction1559Signed(Transaction1559Signed),
	Transaction2930Signed(Transaction2930Signed),
	Transaction7702Signed(Transaction7702Signed),
	TransactionLegacySigned(TransactionLegacySigned),
}

impl Default for TransactionSigned {
	fn default() -> Self {
		TransactionSigned::TransactionLegacySigned(Default::default())
	}
}

impl TransactionSigned {
	/// Get the effective gas price.
	pub fn effective_gas_price(&self, base_gas_price: U256) -> U256 {
		match &self {
			TransactionSigned::TransactionLegacySigned(tx) =>
				tx.transaction_legacy_unsigned.gas_price,
			TransactionSigned::Transaction4844Signed(tx) => base_gas_price
				.saturating_add(tx.transaction_4844_unsigned.max_priority_fee_per_gas)
				.min(tx.transaction_4844_unsigned.max_fee_per_blob_gas),
			TransactionSigned::Transaction1559Signed(tx) => base_gas_price
				.saturating_add(tx.transaction_1559_unsigned.max_priority_fee_per_gas)
				.min(tx.transaction_1559_unsigned.max_fee_per_gas),
			TransactionSigned::Transaction2930Signed(tx) => tx.transaction_2930_unsigned.gas_price,
			TransactionSigned::Transaction7702Signed(tx) => tx.transaction_7702_unsigned.gas_price,
		}
	}
}

/// Validator withdrawal
#[derive(
	Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
)]
pub struct Withdrawal {
	/// recipient address for withdrawal value
	pub address: Address,
	/// value contained in withdrawal
	pub amount: U256,
	/// index of withdrawal
	pub index: U256,
	/// index of validator that generated withdrawal
	#[serde(rename = "validatorIndex")]
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
pub struct AccessListEntry {
	pub address: Address,
	#[serde(rename = "storageKeys")]
	pub storage_keys: Vec<H256>,
}

/// Set code authorization entry
#[derive(
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
)]
pub struct SetCodeAuthorizationEntry {
	/// chainId
	/// Chain ID that this transaction is valid on.
	#[serde(rename = "chainId")]
	pub chain_id: U256,
	/// Address.
	pub address: Address,
	/// nonce
	pub nonce: U256,
	/// r
	pub r: U256,
	/// s
	pub s: U256,
	/// yParity
	/// The parity (0 for even, 1 for odd) of the y-value of the secp256k1 signature.
	#[serde(rename = "yParity")]
	pub y_parity: U256,
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
	#[serde(rename = "yParity")]
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
	#[serde(rename = "yParity")]
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
pub struct Transaction4844Signed {
	#[serde(flatten)]
	pub transaction_4844_unsigned: Transaction4844Unsigned,
	/// r
	pub r: U256,
	/// s
	pub s: U256,
	/// yParity
	/// The parity (0 for even, 1 for odd) of the y-value of the secp256k1 signature.
	#[serde(rename = "yParity")]
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

/// Signed 7702 Transaction
#[derive(
	Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
)]
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
	#[serde(rename = "yParity")]
	pub y_parity: U256,
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

#[cfg(test)]

mod test {
	use super::*;

	#[test]
	fn test_hashes_or_transaction_infos_deserialization() {
		let json = r#"["0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"]"#;
		let result: HashesOrTransactionInfos = serde_json::from_str(json).unwrap();
		assert!(matches!(result, HashesOrTransactionInfos::Hashes(_)));

		let json = r#"[]"#;
		let result: HashesOrTransactionInfos = serde_json::from_str(json).unwrap();
		assert!(matches!(result, HashesOrTransactionInfos::Hashes(_)));

		let json = r#"[{"invalid": "data"}]"#;
		let result: Result<HashesOrTransactionInfos, _> = serde_json::from_str(json);
		assert!(result.is_err());

		// Real block representation.
		let json = r#"[{
			"accessList": [],
			"blockHash": "0xfb8c980d1da1a75e68c2ea4d55cb88d62dedbbb5eaf69df8fe337e9f6922b73a",
			"blockNumber": "0x161bd0f",
			"chainId": "0x1",
			"from": "0x693ca5c6852a7d212dabc98b28e15257465c11f3",
			"gas": "0x70bdb",
			"gasPrice": "0x23cf3fd4",
			"hash": "0xf6d8b07ddcf9a9d44c99c3665fd8c78f0ccd32506350ea5a9be1a68ba08bfd1f",
			"input": "0x09c5eabe000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000002a90000cca0b86991c6218b36c1d19d4a2e9eb0ce3606eb48000000000000000000000000000000020000000000000000000000035c9618f600000000000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc20000000000000000000000002374fed200000000000000000001528fd550bc9a0000000000000000351e55bea6d51900dac17f958d2ee523a2206206994597c13d831ec7000000000000000000000000000000000000000000000000000000005c0c965e0000000000000000000000000000000000004c00000001000000000000000000000000000000000000002e24cd1d61a63f43658ed73b6ddeba00010002000100000000000000000000000000000000000000000000000039d622818daae62900006602000000000000000000002ff9e9686fa6ac00000000000000000000000000007f88ca000000000000000004caaa5ba8029c920300010000000000000000052319c661ddb06600000000000000000001528fd550bc9a0000000000000000005049606b67676100011c0c00000000000000002ff9e9686fa6ac000000000000000000000000035c16902c0000000000000000000000000000000200000000000000000000000000000002000073d53553ee552c1f2a9722e6407d43e41e19593f1cbc3d63300bfc6e48709f5b5ed98f228c70104e8c5d570b5608b47dca95ce6e371636965b6fdcab3613b6b65f061a44b7132011bb97a768bd238eacb62d7109920b000000000000000005246c56372e6d000000000000000000000000005c0c965e0000000000000000000000002374fed20000000000000000000000002374fed200011cc19621f6edbb9c02b95055b9f52eba0e2cb954c259f42aeca488551ea82b72f2504bbd310eb7145435e258751ab6854ab08b1630b89d6621dc1398c5d0c43b480000000000000000000000000000000000000000000000000000",
			"maxFeePerGas": "0x47ca802f",
			"maxPriorityFeePerGas": "0x0",
			"nonce": "0x40c6",
			"r": "0xb3e71bd95d73e965495b17647f5faaf058e13af7dd21f2af24eac16f7e9d06a1",
			"s": "0x58775b0c15075fb7f007b88e88605ae5daec1ffbac2771076e081c8c2b005c20",
			"to": "0x0000000aa232009084bd71a5797d089aa4edfad4",
			"transactionIndex": "0x0",
			"type": "0x2",
			"v": "0x1",
			"value": "0x0",
			"yParity": "0x1"
    	}]
		"#;
		let result: HashesOrTransactionInfos = serde_json::from_str(json).unwrap();
		assert!(matches!(result, HashesOrTransactionInfos::TransactionInfos(_)));

		// Complex real block representation.
		let json = r#"[{
			"accessList": [
				{
				"address": "0x9008d19f58aabd9ed0d60971565aa8510560ab41",
				"storageKeys": [
					"0x0000000000000000000000000000000000000000000000000000000000000001",
					"0x650f69dead2eeee68214ac0bc29f23bc7e2f82c89293ef4b23dc1591bc737c67"
				]
				},
				{
				"address": "0x2c4c28ddbdac9c5e7055b4c863b72ea0149d8afe",
				"storageKeys": [
					"0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc",
					"0x88d075c869ce192f20da9bfc0d2db81b73b4aa4af2ce17e52384cb021d06bd06"
				]
				},
				{
				"address": "0x9e7ae8bdba9aa346739792d219a808884996db67",
				"storageKeys": []
				},
				{
				"address": "0x800c32eaa2a6c93cf4cb51794450ed77fbfbb172",
				"storageKeys": []
				},
				{
				"address": "0x366aa56191e89d219ac36b33406fce85da1e7554",
				"storageKeys": []
				},
				{
				"address": "0xc92e8bdf79f0507f65a392b0ab4667716bfe0110",
				"storageKeys": []
				},
				{
				"address": "0xbbbbbbb520d69a9775e85b458c58c648259fad5f",
				"storageKeys": [
					"0xa3b7a258ccc3c19490a94edab51a442dd2eeac4318bddede8cd899595d08f28a"
				]
				},
				{
				"address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
				"storageKeys": [
					"0xb84fbcd09e20fa700ddef111765a21785d2290b3c7c8719a27e4b60b59126522",
					"0xda591c30fe54b3edd3bcb5d0d916c5c472e3ad81645d0312a5e73f3708e8708b",
					"0x0bc90c98aed598fd15d9075ded522981aeb2ee369c8117e46bd494dc17c29999",
					"0xdbde769b5281dad4214cceeb1871ab281fb8fd2a4443141db1078642029ae248",
					"0x9d98752c354deebddd53535455198eacf8cfb934237d3523207f70386be5e3dc"
				]
				},
				{
				"address": "0x60bf78233f48ec42ee3f101b9a05ec7878728006",
				"storageKeys": []
				},
				{
				"address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
				"storageKeys": [
					"0x0000000000000000000000000000000000000000000000000000000000000001",
					"0x0000000000000000000000000000000000000000000000000000000000000004",
					"0x44624a8a323b583a84b5812478c554a5e284223b469e9f7039175023c0e54c3e",
					"0x3ad18d7747f05925bebbb1df8860daa9cd402d902420f95ce10c49792803c3d6",
					"0xcc236083e86ee3df0f3160002f381f1404bd44c4dec1322196f34d52548202f5",
					"0x88afba62e6432e4d0a3e39a2be587d39c0b93368d66cedb0531bb5292040a552",
					"0x10d6a54a4754c8869d6886b5f5d7fbfa5b4522237ea5c60d11bc4e7a1ff9390b",
					"0x5cfccd15aa8eff180914a82b4caf3b39b3c62421a17404ea7a7d0c80fe766666",
					"0x659f5b53123b3e7a886575e106645d4d7c5af120440f3f409542b3987fa1ea07",
					"0x77d5014beb071d1c3dabbdbdba61f9a5cc3ffedca11c102ef7e2fae619d04e12",
					"0x6e91f60197c982353033e86512311820683e018e0f39963c5d00c2c490bc45d3",
					"0x7050c9e0f4ca769c69bd3a8ef740bc37934f8e2c036e5a723fd8ee048ed3f8c3"
				]
				},
				{
				"address": "0x43506849d7c04f9138d1a2050bbf3a0c054402dd",
				"storageKeys": []
				},
				{
				"address": "0x68b3465833fb72a70ecdf485e0e4c7bd8665fc45",
				"storageKeys": []
				},
				{
				"address": "0x1346d1ee3fb1b65fecfcb65c149ca0702c286f53",
				"storageKeys": [
					"0x0000000000000000000000000000000000000000000000000000000000000004",
					"0x0000000000000000000000000000000000000000000000000000000000000002",
					"0xc0d1c00078410fd0164580b0bad93d8a579580d06cf45fc2696a823498097b8a",
					"0x0000000000000000000000000000000000000000000000000000000000000008",
					"0x0000000000000000000000000000000000000000000000000000000000000000"
				]
				},
				{
				"address": "0x899d774e0f8e14810d628db63e65dfacea682343",
				"storageKeys": [
					"0xd64773870f40323194fda2d1773a23183ba723843a4aa8fb90d5eaf49c342f55",
					"0xef7cf59cb40a7ae1b5e03b08af7ed07c83f41406ca13eaeed923c1f2fc8bbb2a",
					"0x70f537a6c3c5e23e6deecb5baafd173071015ed695aa4c5ab2072a13f49234e4",
					"0x8eb102192bd88c1782b7bb42421db4a5cda302102196a664e54ad03c03e19e1e"
				]
				}
			],
			"blockHash": "0xfb8c980d1da1a75e68c2ea4d55cb88d62dedbbb5eaf69df8fe337e9f6922b73a",
			"blockNumber": "0x161bd0f",
			"chainId": "0x1",
			"from": "0x6bf97afe2d2c790999cded2a8523009eb8a0823f",
			"gas": "0x15d818",
			"gasPrice": "0x65045d54",
			"hash": "0xcd1ae1806eebfe30add1509b02c6f89e85865c7243450742ac86d402507667fd",
			"input": "0x13d79a0b0000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000012000000000000000000000000000000000000000000000000000000000000001c000000000000000000000000000000000000000000000000000000000000003e00000000000000000000000000000000000000000000000000000000000000004000000000000000000000000899d774e0f8e14810d628db63e65dfacea682343000000000000000000000000a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48000000000000000000000000a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48000000000000000000000000899d774e0f8e14810d628db63e65dfacea6823430000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000a374bf76d4c42c8c8c00000000000000000000000000000000005cd4d66627e732daca892b48abb16400000000000000000000000000000000000000000000000006b06fe010314e3e0681000000000000000000000000000000000000000000000000000000000bebc2000000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000003000000000000000000000000366aa56191e89d219ac36b33406fce85da1e7554000000000000000000000000000000000000000000000000000000000bebc2000000000000000000000000000000000000000000000006ac7510475c22e2e3060000000000000000000000000000000000000000000000000000000068a5d4fb98b80e71c53f4b325f7753088b0d8ee55933f28c326277958a47f93bc54a095400000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000bebc200000000000000000000000000000000000000000000000000000000000000016000000000000000000000000000000000000000000000000000000000000000414a13957a7c51fc3c1579a62c6160cf8fdf6cbdb186688f8a71c5085ce84e5cfe6cdd79d18f7e34d99a555aaf04a2c7787f9ad58f7bab041c8a94500b5f051a201b000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000032000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002000000000000000000000000060bf78233f48ec42ee3f101b9a05ec78787280060000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000001e4760f2a0b000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000020000000000000000000000000a0b86991c6218b36c1d19d4a2e9eb0ce3606eb480000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000001388000000000000000000000000000000000000000000000000000000000000000e4d505accf000000000000000000000000366aa56191e89d219ac36b33406fce85da1e7554000000000000000000000000c92e8bdf79f0507f65a392b0ab4667716bfe0110ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff00000000000000000000000000000000000000000000000000000000720d7562000000000000000000000000000000000000000000000000000000000000001c219463f0255ddbed6266f8998f2c3d706c12eaf9de73c3b9f082b0a583fce90546a423f6fe118493aa5f9f57adfd73963e67bb89e6b20faf95821275b6b1607e0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000340000000000000000000000000bbbbbbb520d69a9775e85b458c58c648259fad5f0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000002644dcebcba0000000000000000000000000000000000000000000000000000000068a5ce680000000000000000000000009008d19f58aabd9ed0d60971565aa8510560ab4100000000000000000000000067336cec42645f55059eff241cb02ea5cc52ff860000000000000000000000000000000000000000000000002d2e61b16af396e5000000000000000000000000a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2000000000000000000000000000000000000000000000000000000000bebc20000000000000000000000000000000000000000000000000000aa03ac85e927d00000000000000000000000009008d19f58aabd9ed0d60971565aa8510560ab4100000000000000000000000000000000000000000000000000000000000000006d9aa07971bc4e6731b47ed80776c5740000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001a000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000419969dd20c05e5c0ca3d82fed5f912ae3678db7452adc4bffeb8ae098920f9e2a7804cfa5e1e42f85209c494f49914c39258c7668a992f59a01b2fe2d73d445771b000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000068b3465833fb72a70ecdf485e0e4c7bd8665fc450000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e404e45aaf000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2000000000000000000000000899d774e0f8e14810d628db63e65dfacea68234300000000000000000000000000000000000000000000000000000000000027100000000000000000000000009008d19f58aabd9ed0d60971565aa8510560ab4100000000000000000000000000000000000000000000000000aa03ac85e927d00000000000000000000000000000000000000000000006b3d96e8c277e88e06c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000ab81f1",
			"maxFeePerGas": "0x76630193",
			"maxPriorityFeePerGas": "0x41351d80",
			"nonce": "0x18733",
			"r": "0x6f71e41e8630b35dea48e57d1afd291651a5f15c338133c4976267ac00dd9e56",
			"s": "0x45689f732b0a8e6be5bdf5a45db561f33cae7e976f8e8ebdbcbe2e51dc40869c",
			"to": "0x9008d19f58aabd9ed0d60971565aa8510560ab41",
			"transactionIndex": "0x7",
			"type": "0x2",
			"v": "0x0",
			"value": "0x0",
			"yParity": "0x0"
		}]"#;
		let result: HashesOrTransactionInfos = serde_json::from_str(json).unwrap();
		assert!(matches!(result, HashesOrTransactionInfos::TransactionInfos(_)));

		let json = r#"[{
			"accessList": [],
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
}
