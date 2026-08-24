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

use super::Filter;
use crate::*;
use serde::{Deserialize, Serialize};

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
	#[serde(rename = "newHeads")]
	NewBlockHeaders,
	Logs,
}

/// Options passed by the user for their subscription to make it more specific.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum SubscriptionOptions {
	/// Options passed when subscribing for logs.
	LogsOptions(Filter),
}

/// Resolved parameters for the subscription request which contains both the request type and the
/// options.
#[derive(Clone, Debug)]
pub enum SubscriptionParameters {
	NewBlockHeaders,
	Logs(Filter),
}

impl SubscriptionParameters {
	pub fn new(
		subscription_kind: SubscriptionKind,
		subscription_options: Option<SubscriptionOptions>,
	) -> Option<Self> {
		match (subscription_kind, subscription_options) {
			(SubscriptionKind::Logs, None) => Some(Self::Logs(Filter::default())),
			(SubscriptionKind::Logs, Some(SubscriptionOptions::LogsOptions(filter)))
				if filter.block_option.is_valid() =>
			{
				Some(Self::Logs(filter))
			},
			(SubscriptionKind::Logs, Some(SubscriptionOptions::LogsOptions(_))) => None,
			(SubscriptionKind::NewBlockHeaders, None) => Some(Self::NewBlockHeaders),
			(SubscriptionKind::NewBlockHeaders, Some(SubscriptionOptions::LogsOptions(_))) => None,
		}
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SubscriptionItem {
	BlockHeader(BlockHeader),
	Log(Log),
}
