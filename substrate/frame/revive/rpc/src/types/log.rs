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
use alloy_primitives::BlockHash;
use serde::{Deserialize, Serialize};
use serde_with::{DefaultOnNull, OneOrMany, serde_as};
use sp_core::ConstU32;
use sp_runtime::{BoundedBTreeSet, BoundedVec};
use std::collections::BTreeSet;
use thiserror::Error;

/// Filter results
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
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

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "FilterRepr", into = "FilterRepr")]
pub struct Filter {
	pub block_option: FilterBlockOption,
	pub address: BTreeSet<H160>,
	pub topics: BoundedVec<BoundedBTreeSet<H256, ConstU32<1000>>, ConstU32<4>>,
}

impl Filter {
	pub fn matches(&self, log: &Log) -> bool {
		(self.address.is_empty() || self.address.contains(&log.address)) &&
			self.topics.len() <= log.topics.len() &&
			self.topics
				.iter()
				.zip(&log.topics)
				.all(|(set, topic)| set.is_empty() || set.contains(topic))
	}
}

#[cfg(test)]
impl Filter {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn from_block(mut self, block: impl Into<BlockNumberOrTag>) -> Self {
		let to_block = match self.block_option {
			FilterBlockOption::Range { to_block, .. } => to_block,
			FilterBlockOption::AtBlock { .. } => BlockNumberOrTag::Latest,
		};
		self.block_option = FilterBlockOption::Range { from_block: block.into(), to_block };
		self
	}

	pub fn to_block(mut self, block: impl Into<BlockNumberOrTag>) -> Self {
		let from_block = match self.block_option {
			FilterBlockOption::Range { from_block, .. } => from_block,
			FilterBlockOption::AtBlock { .. } => BlockNumberOrTag::Latest,
		};
		self.block_option = FilterBlockOption::Range { from_block, to_block: block.into() };
		self
	}

	pub fn at_block_hash(mut self, block_hash: H256) -> Self {
		self.block_option =
			FilterBlockOption::AtBlock { block_hash: BlockHash::from(block_hash.0) };
		self
	}

	pub fn address(mut self, addresses: impl IntoIterator<Item = H160>) -> Self {
		self.address = addresses.into_iter().collect();
		self
	}

	pub fn event_signature(self, topics: impl IntoIterator<Item = H256>) -> Self {
		self.topic(0, topics)
	}

	pub fn topic1(self, topics: impl IntoIterator<Item = H256>) -> Self {
		self.topic(1, topics)
	}

	fn topic(mut self, position: usize, topics: impl IntoIterator<Item = H256>) -> Self {
		let mut positions = self.topics.into_inner();
		if positions.len() <= position {
			positions.resize(position + 1, BoundedBTreeSet::new());
		}
		positions[position] = topics
			.into_iter()
			.collect::<BTreeSet<_>>()
			.try_into()
			.expect("test topic alternatives are within bounds");
		self.topics = positions.try_into().expect("test topic positions are within bounds");
		self
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterBlockOption {
	AtBlock { block_hash: BlockHash },
	Range { from_block: BlockNumberOrTag, to_block: BlockNumberOrTag },
}

impl FilterBlockOption {
	pub fn is_valid_for_subscription(&self) -> bool {
		match self {
			Self::AtBlock { .. } => true,
			Self::Range {
				from_block: BlockNumberOrTag::Latest,
				to_block: BlockNumberOrTag::Latest,
			} => true,
			Self::Range { from_block, to_block: BlockNumberOrTag::Latest } => {
				Self::concrete_bound(from_block).is_some()
			},
			Self::Range { from_block, to_block } => matches!(
				(Self::concrete_bound(from_block), Self::concrete_bound(to_block)),
				(Some(from), Some(to)) if from <= to
			),
		}
	}

	pub fn window(&self, block_number: U256) -> LogWindow {
		match self {
			Self::AtBlock { .. } => LogWindow::Open,
			Self::Range { from_block, to_block } => {
				match (Self::concrete_bound(from_block), Self::concrete_bound(to_block)) {
					(_, Some(to)) if block_number > U256::from(to) => LogWindow::Closed,
					(Some(from), _) if block_number < U256::from(from) => LogWindow::NotYetOpen,
					_ => LogWindow::Open,
				}
			},
		}
	}

	fn concrete_bound(bound: &BlockNumberOrTag) -> Option<u64> {
		match bound {
			BlockNumberOrTag::Earliest => Some(0),
			BlockNumberOrTag::Number(number) => Some(*number),
			BlockNumberOrTag::Latest |
			BlockNumberOrTag::Finalized |
			BlockNumberOrTag::Safe |
			BlockNumberOrTag::Pending => None,
		}
	}
}

impl Default for FilterBlockOption {
	fn default() -> Self {
		Self::Range { from_block: BlockNumberOrTag::Latest, to_block: BlockNumberOrTag::Latest }
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogWindow {
	NotYetOpen,
	Open,
	Closed,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FilterRepr {
	#[serde(skip_serializing_if = "Option::is_none")]
	block_hash: Option<BlockHash>,
	#[serde(skip_serializing_if = "Option::is_none")]
	from_block: Option<BlockNumberOrTag>,
	#[serde(skip_serializing_if = "Option::is_none")]
	to_block: Option<BlockNumberOrTag>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	#[serde_as(as = "DefaultOnNull<OneOrMany<_>>")]
	address: Vec<H160>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	#[serde_as(as = "DefaultOnNull<Vec<DefaultOnNull<OneOrMany<_>>>>")]
	topics: Vec<Vec<Option<H256>>>,
}

impl TryFrom<FilterRepr> for Filter {
	type Error = FilterError;

	fn try_from(repr: FilterRepr) -> Result<Self, Self::Error> {
		let FilterRepr { block_hash, from_block, to_block, address, topics } = repr;

		let block_option = match (block_hash, from_block, to_block) {
			(Some(block_hash), None, None) => FilterBlockOption::AtBlock { block_hash },
			(Some(_), _, _) => return Err(FilterError::BlockHashCombinedWithRange),
			(None, from_block, to_block) => FilterBlockOption::Range {
				from_block: from_block.unwrap_or(BlockNumberOrTag::Latest),
				to_block: to_block.unwrap_or(BlockNumberOrTag::Latest),
			},
		};

		let topics = topics
			.into_iter()
			.map(|alternatives| {
				// A `null` alternative makes the whole position a wildcard, mirroring geth.
				alternatives
					.into_iter()
					.collect::<Option<BTreeSet<_>>>()
					.unwrap_or_default()
					.try_into()
					.map_err(|_| FilterError::ExceedMaxTopics)
			})
			.collect::<Result<Vec<_>, _>>()?;
		let topics = BoundedVec::try_from(topics).map_err(|_| FilterError::ExceedMaxTopics)?;

		Ok(Self { block_option, address: address.into_iter().collect(), topics })
	}
}

impl From<Filter> for FilterRepr {
	fn from(filter: Filter) -> Self {
		let (block_hash, from_block, to_block) = match filter.block_option {
			FilterBlockOption::AtBlock { block_hash } => (Some(block_hash), None, None),
			FilterBlockOption::Range { from_block, to_block } => {
				(None, Some(from_block), Some(to_block))
			},
		};

		Self {
			block_hash,
			from_block,
			to_block,
			address: filter.address.into_iter().collect(),
			topics: filter
				.topics
				.into_iter()
				.map(|position| position.into_iter().map(Some).collect())
				.collect(),
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FilterError {
	#[error("cannot specify both BlockHash and FromBlock/ToBlock, choose one or the other")]
	BlockHashCombinedWithRange,
	#[error("exceed max topics")]
	ExceedMaxTopics,
}

#[cfg(test)]
mod tests {
	use super::*;

	fn bounded_set(
		topics: impl IntoIterator<Item = H256>,
	) -> BoundedBTreeSet<H256, ConstU32<1000>> {
		BoundedBTreeSet::try_from(topics.into_iter().collect::<BTreeSet<_>>()).unwrap()
	}

	#[test]
	fn block_hash_conflicts_only_with_non_null_range_bounds() {
		// Arrange
		let block_hash = BlockHash::repeat_byte(0xab);
		let with_null_bounds = serde_json::json!({
			"blockHash": block_hash,
			"fromBlock": null,
			"toBlock": null,
		});
		let with_non_null_bound = serde_json::json!({
			"blockHash": block_hash,
			"fromBlock": "latest",
		});

		// Act
		let with_null_bounds = serde_json::from_value::<Filter>(with_null_bounds).unwrap();
		let error = serde_json::from_value::<Filter>(with_non_null_bound).unwrap_err();

		// Assert
		assert_eq!(with_null_bounds.block_option, FilterBlockOption::AtBlock { block_hash });
		assert_eq!(
			error.to_string(),
			"cannot specify both BlockHash and FromBlock/ToBlock, choose one or the other",
		);
	}

	#[test]
	fn missing_and_null_range_bounds_default_to_the_latest_block() {
		// Arrange
		let missing = serde_json::json!({});
		let null = serde_json::json!({ "fromBlock": null, "toBlock": null });
		let expected = FilterBlockOption::Range {
			from_block: BlockNumberOrTag::Latest,
			to_block: BlockNumberOrTag::Latest,
		};

		// Act
		let missing = serde_json::from_value::<Filter>(missing).unwrap();
		let null = serde_json::from_value::<Filter>(null).unwrap();

		// Assert
		assert_eq!(missing.block_option, expected);
		assert_eq!(null.block_option, expected);
	}

	#[test]
	fn malformed_block_hash_cannot_fall_through_to_a_range_filter() {
		// Arrange
		let representation = serde_json::json!({ "blockHash": "invalid" });

		// Act
		let error = serde_json::from_value::<Filter>(representation).unwrap_err();

		// Assert
		assert_eq!(error.to_string(), "odd number of digits");
	}

	#[test]
	fn missing_null_and_empty_address_and_topics_are_wildcards() {
		// Arrange
		let missing = serde_json::json!({});
		let null = serde_json::json!({ "address": null, "topics": null });
		let empty = serde_json::json!({ "address": [], "topics": [] });

		// Act
		let missing = serde_json::from_value::<Filter>(missing).unwrap();
		let null = serde_json::from_value::<Filter>(null).unwrap();
		let empty = serde_json::from_value::<Filter>(empty).unwrap();

		// Assert
		for filter in [missing, null, empty] {
			assert!(filter.address.is_empty());
			assert!(filter.topics.is_empty());
		}
	}

	#[test]
	fn scalar_and_array_addresses_deserialize_as_matching_sets() {
		// Arrange
		let first_address = H160::repeat_byte(0xab);
		let second_address = H160::repeat_byte(0xcd);
		let scalar = serde_json::json!({ "address": first_address });
		let array = serde_json::json!({ "address": [first_address, second_address] });

		// Act
		let scalar = serde_json::from_value::<Filter>(scalar).unwrap();
		let array = serde_json::from_value::<Filter>(array).unwrap();

		// Assert
		assert_eq!(scalar.address, BTreeSet::from([first_address]));
		assert_eq!(array.address, BTreeSet::from([first_address, second_address]));
	}

	#[test]
	fn null_topic_alternative_makes_the_position_a_wildcard() {
		// Arrange
		let representation = serde_json::json!({
			"topics": [[H256::repeat_byte(0xab), null, H256::repeat_byte(0xcd)]],
		});

		// Act
		let filter = serde_json::from_value::<Filter>(representation).unwrap();

		// Assert
		assert_eq!(filter.topics, vec![BoundedBTreeSet::new()]);
	}

	#[test]
	fn trailing_empty_array_position_requires_the_log_to_have_a_third_topic() {
		// Arrange
		let event_signature = H256::repeat_byte(0xab);
		let second_topic = H256::repeat_byte(0xcd);
		let alternative_topic = H256::repeat_byte(0x11);
		let filter = serde_json::from_value::<Filter>(serde_json::json!({
			"topics": [event_signature, [second_topic, alternative_topic], []],
		}))
		.unwrap();
		let two_topic_log =
			Log { topics: vec![event_signature, second_topic], ..Default::default() };
		let three_topic_log = Log {
			topics: vec![event_signature, second_topic, H256::repeat_byte(0xef)],
			..Default::default()
		};

		// Act
		let two_topic_log_matches = filter.matches(&two_topic_log);
		let three_topic_log_matches = filter.matches(&three_topic_log);

		// Assert
		assert_eq!(
			filter.topics,
			vec![
				bounded_set([event_signature]),
				bounded_set([second_topic, alternative_topic]),
				BoundedBTreeSet::new(),
			],
		);
		assert!(!two_topic_log_matches);
		assert!(three_topic_log_matches);
	}
}
