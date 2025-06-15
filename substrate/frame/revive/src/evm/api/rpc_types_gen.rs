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
use codec::{Decode, Encode};
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
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
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
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, From, TryInto, Eq, PartialEq)]
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
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
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
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
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
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
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

/// Legacy transaction.
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, From, TryInto, Eq, PartialEq)]
#[serde(untagged)]
pub enum TransactionSigned {
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
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
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
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
)]
pub struct AccessListEntry {
	pub address: Address,
	#[serde(rename = "storageKeys")]
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

/// Signed 1559 Transaction
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
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
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
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
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
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
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
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
