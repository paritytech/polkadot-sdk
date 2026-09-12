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

use crate::*;
use alloy_primitives::{Address as AlloyAddress, B256, LogData};
use serde::{Deserialize, Serialize};
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

impl From<BlockV1> for BlockHeader {
	fn from(block: BlockV1) -> Self {
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
///
/// Wraps alloy's [`Filter`] so that address and topic matching uses the same implementation the
/// rest of the Ethereum ecosystem does, and so `eth_subscribe` and the polling filter API — which
/// is handed an `eth_newFilter` [`Filter`] directly — cannot drift apart.
#[derive(Clone, Debug, PartialEq, Eq, derive_more::From)]
pub struct LogsSubscriptionFilter(Filter);

impl LogsSubscriptionFilter {
	/// Constructs a new logs filter.
	///
	/// An absent address or topic leaves the corresponding [`Filter`] set empty, which alloy treats
	/// as a wildcard.
	pub fn new(
		address: Option<BoundedOneOrMany<Address, 1000>>,
		topics: Option<BoundedVec<Option<BoundedOneOrMany<H256, 1000>>, ConstU32<4>>>,
	) -> Self {
		let mut filter = Filter::new();
		if let Some(address) = address {
			filter.address =
				address.into_iter().map(|address| AlloyAddress::from(address.0)).collect();
		}
		for (index, topic) in topics.into_iter().flatten().enumerate() {
			if let Some(topic) = topic {
				filter.topics[index] = topic.into_iter().map(|topic| B256::from(topic.0)).collect();
			}
		}
		Self(filter)
	}

	/// Checks if a certain log matches this filter.
	pub fn matches(&self, log: &Log) -> bool {
		self.0.matches(&alloy_primitives::Log {
			address: AlloyAddress::from(log.address.0),
			data: LogData::new_unchecked(
				log.topics.iter().map(|topic| B256::from(topic.0)).collect(),
				log.data.as_ref().map(|data| data.0.clone().into()).unwrap_or_default(),
			),
		})
	}
}

impl AsRef<Filter> for LogsSubscriptionFilter {
	fn as_ref(&self) -> &Filter {
		&self.0
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
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
