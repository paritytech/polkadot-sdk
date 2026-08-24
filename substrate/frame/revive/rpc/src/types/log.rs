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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterBlockOption {
	AtBlock { block_hash: BlockHash },
	Range { from_block: BlockNumberOrTag, to_block: BlockNumberOrTag },
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
	fn null_alternate_block_selectors_deserialize_as_the_non_null_selector() {
		// Arrange
		let block_hash = BlockHash::repeat_byte(0xab);
		let at_block = serde_json::json!({
			"blockHash": block_hash,
			"fromBlock": null,
			"toBlock": null,
		});
		let range = serde_json::json!({
			"blockHash": null,
			"fromBlock": "earliest",
			"toBlock": null,
		});

		// Act
		let at_block = serde_json::from_value::<Filter>(at_block).unwrap();
		let range = serde_json::from_value::<Filter>(range).unwrap();

		// Assert
		assert_eq!(at_block.block_option, FilterBlockOption::AtBlock { block_hash });
		assert_eq!(
			range.block_option,
			FilterBlockOption::Range {
				from_block: BlockNumberOrTag::Earliest,
				to_block: BlockNumberOrTag::Latest,
			},
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
	fn non_null_block_hash_and_range_bound_are_rejected() {
		// Arrange
		let representation = serde_json::json!({
			"blockHash": BlockHash::repeat_byte(0xab),
			"fromBlock": "latest",
		});

		// Act
		let error = serde_json::from_value::<Filter>(representation).unwrap_err();

		// Assert
		assert_eq!(
			error.to_string(),
			"cannot specify both BlockHash and FromBlock/ToBlock, choose one or the other",
		);
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
	fn unrelated_fields_are_ignored_when_deserializing_a_block_range() {
		// Arrange
		let representation = serde_json::json!({
			"fromBlock": "earliest",
			"unrelated": true,
		});

		// Act
		let filter = serde_json::from_value::<Filter>(representation).unwrap();

		// Assert
		assert_eq!(
			filter.block_option,
			FilterBlockOption::Range {
				from_block: BlockNumberOrTag::Earliest,
				to_block: BlockNumberOrTag::Latest,
			},
		);
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
	fn null_address_elements_are_rejected() {
		// Arrange
		let representation = serde_json::json!({ "address": [H160::repeat_byte(0xab), null] });

		// Act
		let result = serde_json::from_value::<Filter>(representation);

		// Assert
		assert!(result.is_err());
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
	fn trailing_wildcard_preserves_third_topic_position() {
		// Arrange
		let event_signature = H256::repeat_byte(0xab);
		let first_set_topic = H256::repeat_byte(0xcd);
		let second_set_topic = H256::repeat_byte(0xef);
		let representation = serde_json::json!({
			"topics": [event_signature, [first_set_topic, second_set_topic], null],
		});

		// Act
		let filter = serde_json::from_value::<Filter>(representation).unwrap();

		// Assert
		assert_eq!(
			filter.topics,
			vec![
				bounded_set([event_signature]),
				bounded_set([first_set_topic, second_set_topic]),
				BoundedBTreeSet::new(),
			],
		);
	}

	#[test]
	fn trailing_empty_array_position_requires_the_log_to_have_a_third_topic() {
		// Arrange
		let event_signature = H256::repeat_byte(0xab);
		let second_topic = H256::repeat_byte(0xcd);
		let filter = serde_json::from_value::<Filter>(serde_json::json!({
			"topics": [event_signature, [second_topic], []],
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
				bounded_set([second_topic]),
				BoundedBTreeSet::new(),
			],
		);
		assert!(!two_topic_log_matches);
		assert!(three_topic_log_matches);
	}

	#[test]
	fn logs_with_fewer_topics_than_filter_positions_do_not_match() {
		// Arrange
		let event_signature = H256::repeat_byte(0xab);
		let filter = serde_json::from_value::<Filter>(serde_json::json!({
			"topics": [event_signature, null],
		}))
		.unwrap();
		let short_log = Log { topics: vec![event_signature], ..Default::default() };
		let matching_log =
			Log { topics: vec![event_signature, H256::repeat_byte(0xcd)], ..Default::default() };

		// Act
		let short_log_matches = filter.matches(&short_log);
		let matching_log_matches = filter.matches(&matching_log);

		// Assert
		assert!(!short_log_matches);
		assert!(matching_log_matches);
	}

	#[test]
	fn single_alternatives_serialize_as_scalars_and_wildcards_as_empty_arrays() {
		// Arrange
		let address = H160::repeat_byte(0xab);
		let event_signature = H256::repeat_byte(0xcd);
		let other_topic = H256::repeat_byte(0xef);
		let filter = serde_json::from_value::<Filter>(serde_json::json!({
			"address": [address],
			"topics": [event_signature, null, [event_signature, other_topic]],
		}))
		.unwrap();

		// Act
		let serialized = serde_json::to_value(&filter).unwrap();

		// Assert
		assert_eq!(
			serialized,
			serde_json::json!({
				"fromBlock": "latest",
				"toBlock": "latest",
				"address": address,
				"topics": [event_signature, [], [event_signature, other_topic]],
			}),
		);
	}
}
