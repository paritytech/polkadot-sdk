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

use super::{byte::*, Type0, Type1, Type2};
use alloc::vec::Vec;
use codec::{Decode, Encode};
use derive_more::{From, TryInto};
pub use ethereum_types::*;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

/// Block object
#[derive(
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
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
	pub transactions: H256OrTransactionInfo,
	/// Transactions root
	#[serde(rename = "transactionsRoot")]
	pub transactions_root: H256,
	/// Uncles
	pub uncles: Vec<H256>,
	/// Withdrawals
	#[serde(skip_serializing_if = "Option::is_none")]
	pub withdrawals: Option<Vec<Withdrawal>>,
	/// Withdrawals root
	#[serde(rename = "withdrawalsRoot", skip_serializing_if = "Option::is_none")]
	pub withdrawals_root: Option<H256>,
}

/// Block number or tag
#[derive(
	Debug, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, From, TryInto, Eq, PartialEq,
)]
#[serde(untagged)]
pub enum BlockNumberOrTag {
	/// Block number
	U256(U256),
	/// Block tag
	BlockTag(BlockTag),
}
impl Default for BlockNumberOrTag {
	fn default() -> Self {
		BlockNumberOrTag::U256(Default::default())
	}
}

/// Block number, tag, or block hash
#[derive(
	Debug, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, From, TryInto, Eq, PartialEq,
)]
#[serde(untagged)]
pub enum BlockNumberOrTagOrHash {
	/// Block number
	U256(U256),
	/// Block tag
	BlockTag(BlockTag),
	/// Block hash
	H256(H256),
}
impl Default for BlockNumberOrTagOrHash {
	fn default() -> Self {
		BlockNumberOrTagOrHash::U256(Default::default())
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
	#[serde(rename = "blobVersionedHashes", skip_serializing_if = "Option::is_none")]
	pub blob_versioned_hashes: Option<Vec<H256>>,
	/// blobs
	/// Raw blob data.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub blobs: Option<Vec<Bytes>>,
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
	#[serde(alias = "data", skip_serializing_if = "Option::is_none")]
	pub input: Option<Bytes>,
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
#[derive(
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
)]
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
#[derive(
	Debug, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, From, TryInto, Eq, PartialEq,
)]
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
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
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

#[derive(
	Debug, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, From, TryInto, Eq, PartialEq,
)]
#[serde(untagged)]
pub enum TransactionUnsigned {
	Transaction4844Unsigned(Transaction4844Unsigned),
	Transaction1559Unsigned(Transaction1559Unsigned),
	Transaction2930Unsigned(Transaction2930Unsigned),
	TransactionLegacyUnsigned(TransactionLegacyUnsigned),
}
impl Default for TransactionUnsigned {
	fn default() -> Self {
		TransactionUnsigned::Transaction4844Unsigned(Default::default())
	}
}

/// Access list
pub type AccessList = Vec<AccessListEntry>;

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
#[derive(
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
)]
pub enum BlockTag {
	#[serde(rename = "earliest")]
	#[default]
	Earliest,
	#[serde(rename = "finalized")]
	Finalized,
	#[serde(rename = "safe")]
	Safe,
	#[serde(rename = "latest")]
	Latest,
	#[serde(rename = "pending")]
	Pending,
}

#[derive(
	Debug, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, From, TryInto, Eq, PartialEq,
)]
#[serde(untagged)]
pub enum H256OrTransactionInfo {
	/// Transaction hashes
	H256s(Vec<H256>),
	/// Full transactions
	TransactionInfos(Vec<TransactionInfo>),
}
impl Default for H256OrTransactionInfo {
	fn default() -> Self {
		H256OrTransactionInfo::H256s(Default::default())
	}
}

/// log
#[derive(
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
)]
pub struct Log {
	/// address
	#[serde(skip_serializing_if = "Option::is_none")]
	pub address: Option<Address>,
	/// block hash
	#[serde(rename = "blockHash", skip_serializing_if = "Option::is_none")]
	pub block_hash: Option<H256>,
	/// block number
	#[serde(rename = "blockNumber", skip_serializing_if = "Option::is_none")]
	pub block_number: Option<U256>,
	/// data
	#[serde(skip_serializing_if = "Option::is_none")]
	pub data: Option<Bytes>,
	/// log index
	#[serde(rename = "logIndex", skip_serializing_if = "Option::is_none")]
	pub log_index: Option<U256>,
	/// removed
	#[serde(skip_serializing_if = "Option::is_none")]
	pub removed: Option<bool>,
	/// topics
	#[serde(skip_serializing_if = "Option::is_none")]
	pub topics: Option<Vec<H256>>,
	/// transaction hash
	#[serde(rename = "transactionHash")]
	pub transaction_hash: H256,
	/// transaction index
	#[serde(rename = "transactionIndex", skip_serializing_if = "Option::is_none")]
	pub transaction_index: Option<U256>,
}

/// Syncing progress
#[derive(
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
)]
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
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
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
	pub r#type: Type2,
	/// value
	pub value: U256,
}

/// EIP-2930 transaction.
#[derive(
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
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
	pub r#type: Type1,
	/// value
	pub value: U256,
}

/// EIP-4844 transaction.
#[derive(
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
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
	pub r#type: Byte,
	/// value
	pub value: U256,
}

/// Legacy transaction.
#[derive(
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
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
	pub r#type: Type0,
	/// value
	pub value: U256,
}

#[derive(
	Debug, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, From, TryInto, Eq, PartialEq,
)]
#[serde(untagged)]
pub enum TransactionSigned {
	Transaction4844Signed(Transaction4844Signed),
	Transaction1559Signed(Transaction1559Signed),
	Transaction2930Signed(Transaction2930Signed),
	TransactionLegacySigned(TransactionLegacySigned),
}
impl Default for TransactionSigned {
	fn default() -> Self {
		TransactionSigned::Transaction4844Signed(Default::default())
	}
}

/// Validator withdrawal
#[derive(
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
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
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
)]
pub struct AccessListEntry {
	pub address: Address,
	#[serde(rename = "storageKeys")]
	pub storage_keys: Vec<H256>,
}

/// Signed 1559 Transaction
#[derive(
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
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
	#[serde(rename = "yParity", skip_serializing_if = "Option::is_none")]
	pub y_parity: Option<U256>,
}

/// Signed 2930 Transaction
#[derive(
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
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
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
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
	#[serde(rename = "yParity", skip_serializing_if = "Option::is_none")]
	pub y_parity: Option<U256>,
}

/// Signed Legacy Transaction
#[derive(
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
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
