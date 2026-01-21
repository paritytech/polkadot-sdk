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

pub use crate::runtime_api::StatementSource;
use crate::{Hash, Statement, Topic, MAX_ANY_TOPICS};
use sp_core::Bytes;
use std::collections::HashSet;

/// Statement store error.
#[derive(Debug, Clone, Eq, PartialEq, thiserror::Error)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Error {
	/// Database error.
	#[error("Database error: {0:?}")]
	Db(String),
	/// Error decoding statement structure.
	#[error("Error decoding statement: {0:?}")]
	Decode(String),
	/// Error making runtime call.
	#[error("Error calling into the runtime")]
	Runtime,
}

/// Filter for subscribing to statements with different topics.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub enum TopicFilter {
	/// Matches all topics.
	Any,
	/// Matches only statements including all of the given topics.
	/// Bytes are expected to be a 32-byte topic. Up to `4` topics can be provided.
	MatchAll(Vec<Bytes>),
	/// Matches statements including any of the given topics.
	/// Bytes are expected to be a 32-byte topic. Up to `128` topics can be provided.
	MatchAny(Vec<Bytes>),
}

/// Topic filter for statement subscriptions.
#[derive(Clone, Debug)]
pub enum CheckedTopicFilter {
	/// Matches all topics.
	Any,
	/// Matches only statements including all of the given topics.
	/// Bytes are expected to be a 32-byte topic. Up to `4` topics can be provided.
	MatchAll(HashSet<Topic>),
	/// Matches statements including any of the given topics.
	/// Bytes are expected to be a 32-byte topic. Up to `128` topics can be provided.
	MatchAny(HashSet<Topic>),
}

impl CheckedTopicFilter {
	/// Check if the statement matches the filter.
	pub fn matches(&self, statement: &Statement) -> bool {
		match self {
			CheckedTopicFilter::Any => true,
			CheckedTopicFilter::MatchAll(topics) =>
				statement.topics().iter().filter(|topic| topics.contains(*topic)).count() ==
					topics.len(),
			CheckedTopicFilter::MatchAny(topics) =>
				statement.topics().iter().any(|topic| topics.contains(topic)),
		}
	}
}

// Convert TopicFilter to CheckedTopicFilter, validating topic lengths.
impl TryInto<CheckedTopicFilter> for TopicFilter {
	type Error = Error;

	fn try_into(self) -> Result<CheckedTopicFilter> {
		match self {
			TopicFilter::Any => Ok(CheckedTopicFilter::Any),
			TopicFilter::MatchAll(topics) => {
				let mut parsed_topics = HashSet::with_capacity(topics.len());
				for topic in topics {
					if topic.0.len() != 32 {
						return Err(Error::Decode("Invalid topic format".into()));
					}
					let mut arr = [0u8; 32];
					arr.copy_from_slice(&topic.0);
					parsed_topics.insert(arr);
				}
				Ok(CheckedTopicFilter::MatchAll(parsed_topics))
			},
			TopicFilter::MatchAny(topics) => {
				let mut parsed_topics = HashSet::with_capacity(topics.len());
				if topics.len() > MAX_ANY_TOPICS {
					return Err(Error::Decode("Too many topics in MatchAny filter".into()));
				}
				for topic in topics {
					if topic.0.len() != 32 {
						return Err(Error::Decode("Invalid topic format".into()));
					}
					let mut arr = [0u8; 32];
					arr.copy_from_slice(&topic.0);
					parsed_topics.insert(arr);
				}
				Ok(CheckedTopicFilter::MatchAny(parsed_topics))
			},
		}
	}
}

/// Reason why a statement was rejected from the store.
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "reason", rename_all = "camelCase"))]
pub enum RejectionReason {
	/// Statement data exceeds the maximum allowed size for the account.
	DataTooLarge {
		/// The size of the submitted statement data.
		submitted_size: usize,
		/// Still available data size for the account.
		available_size: usize,
	},
	/// Attempting to replace a channel message with lower or equal expiry.
	ChannelPriorityTooLow {
		/// The expiry of the submitted statement.
		submitted_expiry: u64,
		/// The minimum expiry of the existing channel message.
		min_expiry: u64,
	},
	/// Account reached its statement limit and submitted expiry is too low to evict existing.
	AccountFull {
		/// The expiry of the submitted statement.
		submitted_expiry: u64,
		/// The minimum expiry of the existing statement.
		min_expiry: u64,
	},
	/// The global statement store is full and cannot accept new statements.
	StoreFull,
}

/// Reason why a statement failed validation.
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "reason", rename_all = "camelCase"))]
pub enum InvalidReason {
	/// Statement has no proof.
	NoProof,
	/// Proof validation failed.
	BadProof,
	/// Statement exceeds max allowed statement size.
	EncodingTooLarge {
		/// The size of the submitted statement encoding.
		submitted_size: usize,
		/// The maximum allowed size.
		max_size: usize,
	},
	/// Statement has already expired. The expiry field is in the past.
	AlreadyExpired,
}

/// Statement submission outcome
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "status", rename_all = "camelCase"))]
pub enum SubmitResult {
	/// Statement was accepted as new.
	New,
	/// Statement was already known.
	Known,
	/// Statement was already known but has expired.
	KnownExpired,
	/// Statement was rejected because the store is full or priority is too low.
	Rejected(RejectionReason),
	/// Statement failed validation.
	Invalid(InvalidReason),
	/// Internal store error.
	InternalError(Error),
}

/// Result type for `Error`
pub type Result<T> = std::result::Result<T, Error>;

/// Decision returned by the filter used in [`StatementStore::statements_by_hashes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterDecision {
	/// Skip this statement, continue to next.
	Skip,
	/// Take this statement, continue to next.
	Take,
	/// Stop iteration, return collected statements.
	Abort,
}

/// Statement store API.
pub trait StatementStore: Send + Sync {
	/// Return all statements.
	fn statements(&self) -> Result<Vec<(Hash, Statement)>>;

	/// Return recent statements and clear the internal index.
	///
	/// This consumes and clears the recently received statements,
	/// allowing new statements to be collected from this point forward.
	fn take_recent_statements(&self) -> Result<Vec<(Hash, Statement)>>;

	/// Get statement by hash.
	fn statement(&self, hash: &Hash) -> Result<Option<Statement>>;

	/// Check if statement exists in the store
	///
	/// Fast index check without accessing the DB.
	fn has_statement(&self, hash: &Hash) -> bool;

	/// Return all statement hashes.
	fn statement_hashes(&self) -> Vec<Hash>;

	/// Fetch statements by their hashes with a filter callback.
	///
	/// The callback receives (hash, encoded_bytes, decoded_statement) and returns:
	/// - `Skip`: ignore this statement, continue to next
	/// - `Take`: include this statement in the result, continue to next
	/// - `Abort`: stop iteration, return collected statements so far
	///
	/// Returns (statements, number_of_hashes_processed).
	fn statements_by_hashes(
		&self,
		hashes: &[Hash],
		filter: &mut dyn FnMut(&Hash, &[u8], &Statement) -> FilterDecision,
	) -> Result<(Vec<(Hash, Statement)>, usize)>;

	/// Return the data of all known statements which include all topics and have no `DecryptionKey`
	/// field.
	fn broadcasts(&self, match_all_topics: &[Topic]) -> Result<Vec<Vec<u8>>>;

	/// Return the data of all known statements whose decryption key is identified as `dest` (this
	/// will generally be the public key or a hash thereof for symmetric ciphers, or a hash of the
	/// private key for symmetric ciphers).
	fn posted(&self, match_all_topics: &[Topic], dest: [u8; 32]) -> Result<Vec<Vec<u8>>>;

	/// Return the decrypted data of all known statements whose decryption key is identified as
	/// `dest`. The key must be available to the client.
	fn posted_clear(&self, match_all_topics: &[Topic], dest: [u8; 32]) -> Result<Vec<Vec<u8>>>;

	/// Return all known statements which include all topics and have no `DecryptionKey`
	/// field.
	fn broadcasts_stmt(&self, match_all_topics: &[Topic]) -> Result<Vec<Vec<u8>>>;

	/// Return all known statements whose decryption key is identified as `dest` (this
	/// will generally be the public key or a hash thereof for symmetric ciphers, or a hash of the
	/// private key for symmetric ciphers).
	fn posted_stmt(&self, match_all_topics: &[Topic], dest: [u8; 32]) -> Result<Vec<Vec<u8>>>;

	/// Return the statement and the decrypted data of all known statements whose decryption key is
	/// identified as `dest`. The key must be available to the client.
	///
	/// The result is for each statement: the SCALE-encoded statement concatenated to the
	/// decrypted data.
	fn posted_clear_stmt(&self, match_all_topics: &[Topic], dest: [u8; 32])
		-> Result<Vec<Vec<u8>>>;

	/// Submit a statement.
	fn submit(&self, statement: Statement, source: StatementSource) -> SubmitResult;

	/// Remove a statement from the store.
	fn remove(&self, hash: &Hash) -> Result<()>;

	/// Remove all statements authored by `who`.
	fn remove_by(&self, who: [u8; 32]) -> Result<()>;
}
