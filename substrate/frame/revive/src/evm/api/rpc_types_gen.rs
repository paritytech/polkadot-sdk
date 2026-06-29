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

use super::{Block, Byte, Bytes, Bytes8, Bytes256};
use alloc::{boxed::Box, collections::BTreeSet, vec::Vec};
use codec::{Decode, Encode};
use derive_more::{From, TryInto};
pub use ethereum_types::*;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize, de::Error};
use sp_core::ConstU32;
use sp_runtime::BoundedVec;

/// Block header object returned by `newHeads` subscriptions.
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BlockHeader {
	/// Number
	pub number: U256,
	/// Hash
	pub hash: H256,
	/// Parent block hash
	pub parent_hash: H256,
	/// Nonce
	pub nonce: Bytes8,
	/// Ommers hash
	pub sha_3_uncles: H256,
	/// Bloom filter
	pub logs_bloom: Bytes256,
	/// Transactions root
	pub transactions_root: H256,
	/// State root
	pub state_root: H256,
	/// Receipts root
	pub receipts_root: H256,
	/// Coinbase
	pub miner: Address,
	/// Extra data
	pub extra_data: Bytes,
	/// Gas limit
	pub gas_limit: U256,
	/// Gas used
	pub gas_used: U256,
	/// Timestamp
	pub timestamp: U256,
}

impl From<Block> for BlockHeader {
	fn from(block: Block) -> Self {
		Self {
			number: block.number,
			hash: block.hash,
			parent_hash: block.parent_hash,
			nonce: block.nonce,
			sha_3_uncles: block.sha_3_uncles,
			logs_bloom: block.logs_bloom,
			transactions_root: block.transactions_root,
			state_root: block.state_root,
			receipts_root: block.receipts_root,
			miner: block.miner,
			extra_data: block.extra_data,
			gas_limit: block.gas_limit,
			gas_used: block.gas_used,
			timestamp: block.timestamp,
		}
	}
}

/// Block number or tag
#[derive(Debug, Copy, Clone, Serialize, Deserialize, From, TryInto, Eq, PartialEq)]
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
#[derive(Debug, Copy, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
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

/// log
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
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

/// The kind of subscription the user is requesting from the eth-rpc.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SubscriptionKind {
	NewBlockHeaders,
	Logs,
}

/// Options passed by the user for their subscription to make it more specific.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum SubscriptionOptions {
	/// Options passed when subscribing for logs.
	LogsOptions {
		/// An optional address to use to filter the logs.
		///
		/// If specified, then only logs where this address is the emitter will be returned in the
		/// subscription. If not specified, then it means that there's no filtering based on the
		/// address of the emitter.
		///
		/// If it's specified as a vector of addresses then all of the addresses specified in the
		/// vector pass the filter.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		address: Option<BoundedOneOrMany<Address, 1000>>,

		/// An optional set of topics to filter the logs by.
		///
		/// If not specified, then logs with any topic would match the filter. If specified, then
		/// only logs which match the specified topics pass the filter.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		topics: Option<BoundedVec<Option<BoundedOneOrMany<H256, 1000>>, ConstU32<4>>>,
	},
}

/// A type used as a filter for logs in subscriptions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogsSubscriptionFilter {
	/// Defines if the filter is configured to make use of addresses or not.
	addresses: Option<BTreeSet<H160>>,

	/// Defines if the filter is configured to filter based on the topics.
	topics: Option<[Option<BTreeSet<H256>>; 4]>,
}

impl LogsSubscriptionFilter {
	/// Constructs a new logs filter.
	pub fn new(
		address: Option<BoundedOneOrMany<Address, 1000>>,
		topics: Option<BoundedVec<Option<BoundedOneOrMany<H256, 1000>>, ConstU32<4>>>,
	) -> Self {
		Self {
			addresses: address.map(|addresses| addresses.into_iter().collect()),
			topics: topics.map(|topics| {
				let mut resolved_topics = [None, None, None, None];
				for (index, topic) in topics.into_iter().enumerate() {
					resolved_topics[index] =
						topic.map(|topic_filter| topic_filter.into_iter().collect());
				}
				resolved_topics
			}),
		}
	}

	/// Checks if a certain log matches this filter.
	pub fn matches(&self, log: &Log) -> bool {
		// Check the emitter address. If it doesn't match, then we return.
		if let Some(ref address_filter) = self.addresses &&
			!address_filter.contains(&log.address) &&
			!address_filter.is_empty()
		{
			return false;
		}

		// Check the topics filter to ensure that the log matches the topics filter.
		if let Some(ref topics_filters) = self.topics {
			let mut event_topics = log.topics.iter();
			for topics_filter in topics_filters {
				let event_topic = event_topics.next();

				match (topics_filter, event_topic) {
					// Wildcard filters.
					(None, _) => {},
					(Some(topic_filters), _) if topic_filters.is_empty() => {},
					// There's a filter but there's no topic at this index, return false at this
					// point.
					(Some(..), None) => return false,
					// There's a filter and there's also a topic at this index. So filter based on
					// it.
					(Some(topics_filter), Some(topic)) => {
						if !topics_filter.contains(topic) {
							return false;
						}
					},
				}
			}
		}

		true
	}
}

/// Resolved parameters for the subscription request which contains both the request type and the
/// options.
#[derive(Clone, Debug)]
pub enum SubscriptionParameters {
	NewBlockHeaders,
	Logs(LogsSubscriptionFilter),
}

impl SubscriptionParameters {
	pub fn new(
		subscription_kind: SubscriptionKind,
		subscription_options: Option<SubscriptionOptions>,
	) -> Option<Self> {
		match (subscription_kind, subscription_options) {
			(SubscriptionKind::Logs, None) => {
				Some(Self::Logs(LogsSubscriptionFilter::new(None, None)))
			},
			(
				SubscriptionKind::Logs,
				Some(SubscriptionOptions::LogsOptions { address, topics }),
			) => Some(Self::Logs(LogsSubscriptionFilter::new(address, topics))),
			(SubscriptionKind::NewBlockHeaders, None) => Some(Self::NewBlockHeaders),
			_ => None,
		}
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SubscriptionItem {
	BlockHeader(BlockHeader),
	Log(Log),
}

/// A helper type used when a type can be serialized and deserialized as either being one or as an
/// array.
#[derive(
	Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, TypeInfo,
)]
#[serde(untagged)]
pub enum BoundedOneOrMany<T, const BOUND: u32> {
	One(T),
	Many(BoundedVec<T, ConstU32<BOUND>>),
}

impl<T: 'static, const BOUND: u32> IntoIterator for BoundedOneOrMany<T, BOUND> {
	type IntoIter = Box<dyn Iterator<Item = T>>;
	type Item = T;

	fn into_iter(self) -> Self::IntoIter {
		match self {
			BoundedOneOrMany::One(item) => Box::new(core::iter::once(item)) as _,
			BoundedOneOrMany::Many(bounded_vec) => Box::new(bounded_vec.into_iter()) as _,
		}
	}
}
