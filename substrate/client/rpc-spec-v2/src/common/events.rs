// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Common events for RPC-V2 spec.

use serde::{Deserialize, Serialize};

/// The storage item to query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageQuery<Key> {
	/// The provided key.
	pub key: Key,
	/// The type of the storage query.
	#[serde(rename = "type")]
	pub query_type: StorageQueryType,
}

/// The storage item to query with pagination.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedStorageQuery<Key> {
	/// The provided key.
	pub key: Key,
	/// The type of the storage query.
	#[serde(rename = "type")]
	pub query_type: StorageQueryType,
	/// The pagination key from which the iteration should resume.
	#[serde(skip_serializing_if = "Option::is_none")]
	#[serde(default)]
	pub pagination_start_key: Option<Key>,
}

/// The type of the storage query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StorageQueryType {
	/// Fetch the value of the provided key.
	Value,
	/// Fetch the hash of the value of the provided key.
	Hash,
	/// Fetch the closest descendant merkle value.
	ClosestDescendantMerkleValue,
	/// Fetch the values of all descendants of they provided key.
	DescendantsValues,
	/// Fetch the hashes of the values of all descendants of they provided key.
	DescendantsHashes,
}

impl StorageQueryType {
	/// Returns `true` if the query is a descendant query.
	pub fn is_descendant_query(&self) -> bool {
		matches!(self, Self::DescendantsValues | Self::DescendantsHashes)
	}
}

/// The storage result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageResult {
	/// The hex-encoded key of the result.
	pub key: String,
	/// The result of the query.
	#[serde(flatten)]
	pub result: StorageResultType,
	/// The child trie key if provided.
	#[serde(skip_serializing_if = "Option::is_none")]
	#[serde(default)]
	pub child_trie_key: Option<String>,
}

/// The type of the storage query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StorageResultType {
	/// Fetch the value of the provided key.
	Value(String),
	/// Fetch the hash of the value of the provided key.
	Hash(String),
	/// Fetch the closest descendant merkle value.
	ClosestDescendantMerkleValue(String),
}

/// The error of a storage call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageResultErr {
	/// The hex-encoded key of the result.
	pub key: String,
	/// The result of the query.
	#[serde(flatten)]
	pub error: StorageResultType,
}

/// The result of a storage call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "event")]
pub enum ArchiveStorageEvent {
	/// Query generated a result.
	Storage(StorageResult),
	/// Query encountered an error.
	StorageError(ArchiveStorageMethodErr),
	/// Operation storage is done.
	StorageDone,
}

impl ArchiveStorageEvent {
	/// Create a new `ArchiveStorageEvent::StorageErr` event.
	pub fn err(error: String) -> Self {
		Self::StorageError(ArchiveStorageMethodErr { error })
	}

	/// Create a new `ArchiveStorageEvent::StorageResult` event.
	pub fn result(result: StorageResult) -> Self {
		Self::Storage(result)
	}

	/// Checks if the event is a `StorageDone` event.
	pub fn is_done(&self) -> bool {
		matches!(self, Self::StorageDone)
	}

	/// Checks if the event is a `StorageErr` event.
	pub fn is_err(&self) -> bool {
		matches!(self, Self::StorageError(_))
	}

	/// Checks if the event is a `StorageResult` event.
	pub fn is_result(&self) -> bool {
		matches!(self, Self::Storage(_))
	}
}

/// The result of a storage call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveStorageMethodOk {
	/// Reported results.
	pub result: Vec<StorageResult>,
	/// Number of discarded items.
	pub discarded_items: usize,
}

/// The error of a storage call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveStorageMethodErr {
	/// Reported error.
	pub error: String,
}

/// The type of the archive storage difference query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArchiveStorageDiffType {
	/// The result is provided as value of the key.
	Value,
	/// The result the hash of the value of the key.
	Hash,
}

/// The storage item to query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveStorageDiffItem<Key> {
	/// The provided key.
	pub key: Key,
	/// The type of the storage query.
	pub return_type: ArchiveStorageDiffType,
	/// The child trie key if provided.
	#[serde(skip_serializing_if = "Option::is_none")]
	#[serde(default)]
	pub child_trie_key: Option<Key>,
}

/// The result of a storage difference call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveStorageDiffMethodResult {
	/// Reported results.
	pub result: Vec<ArchiveStorageDiffResult>,
}

/// The result of a storage difference call operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArchiveStorageDiffOperationType {
	/// The key is added.
	Added,
	/// The key is modified.
	Modified,
	/// The key is removed.
	Deleted,
}

/// The result of an individual storage difference key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveStorageDiffResult {
	/// The hex-encoded key of the result.
	pub key: String,
	/// The result of the query.
	#[serde(flatten)]
	pub result: StorageResultType,
	/// The operation type.
	#[serde(rename = "type")]
	pub operation_type: ArchiveStorageDiffOperationType,
	/// The child trie key if provided.
	#[serde(skip_serializing_if = "Option::is_none")]
	#[serde(default)]
	pub child_trie_key: Option<String>,
}

/// The event generated by the `archive_storageDiff` method.
///
/// The `archive_storageDiff` can generate the following events:
///  - `storageDiff` event - generated when a `ArchiveStorageDiffResult` is produced.
///  - `storageDiffError` event - generated when an error is produced.
///  - `storageDiffDone` event - generated when the `archive_storageDiff` method completed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "event")]
pub enum ArchiveStorageDiffEvent {
	/// The `storageDiff` event.
	StorageDiff(ArchiveStorageDiffResult),
	/// The `storageDiffError` event.
	StorageDiffError(ArchiveStorageMethodErr),
	/// The `storageDiffDone` event.
	StorageDiffDone,
}

impl ArchiveStorageDiffEvent {
	/// Create a new `ArchiveStorageDiffEvent::StorageDiffError` event.
	pub fn err(error: String) -> Self {
		Self::StorageDiffError(ArchiveStorageMethodErr { error })
	}

	/// Checks if the event is a `StorageDiffDone` event.
	pub fn is_done(&self) -> bool {
		matches!(self, Self::StorageDiffDone)
	}

	/// Checks if the event is a `StorageDiffError` event.
	pub fn is_err(&self) -> bool {
		matches!(self, Self::StorageDiffError(_))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn archive_diff_input() {
		// Item with Value.
		let item = ArchiveStorageDiffItem {
			key: "0x1",
			return_type: ArchiveStorageDiffType::Value,
			child_trie_key: None,
		};
		// Encode
		let ser = serde_json::to_string(&item).unwrap();
		let exp = r#"{"key":"0x1","returnType":"value"}"#;
		assert_eq!(ser, exp);
		// Decode
		let dec: ArchiveStorageDiffItem<&str> = serde_json::from_str(exp).unwrap();
		assert_eq!(dec, item);

		// Item with Hash.
		let item = ArchiveStorageDiffItem {
			key: "0x1",
			return_type: ArchiveStorageDiffType::Hash,
			child_trie_key: None,
		};
		// Encode
		let ser = serde_json::to_string(&item).unwrap();
		let exp = r#"{"key":"0x1","returnType":"hash"}"#;
		assert_eq!(ser, exp);
		// Decode
		let dec: ArchiveStorageDiffItem<&str> = serde_json::from_str(exp).unwrap();
		assert_eq!(dec, item);

		// Item with Value and child trie key.
		let item = ArchiveStorageDiffItem {
			key: "0x1",
			return_type: ArchiveStorageDiffType::Value,
			child_trie_key: Some("0x2"),
		};
		// Encode
		let ser = serde_json::to_string(&item).unwrap();
		let exp = r#"{"key":"0x1","returnType":"value","childTrieKey":"0x2"}"#;
		assert_eq!(ser, exp);
		// Decode
		let dec: ArchiveStorageDiffItem<&str> = serde_json::from_str(exp).unwrap();
		assert_eq!(dec, item);

		// Item with Hash and child trie key.
		let item = ArchiveStorageDiffItem {
			key: "0x1",
			return_type: ArchiveStorageDiffType::Hash,
			child_trie_key: Some("0x2"),
		};
		// Encode
		let ser = serde_json::to_string(&item).unwrap();
		let exp = r#"{"key":"0x1","returnType":"hash","childTrieKey":"0x2"}"#;
		assert_eq!(ser, exp);
		// Decode
		let dec: ArchiveStorageDiffItem<&str> = serde_json::from_str(exp).unwrap();
		assert_eq!(dec, item);
	}

	#[test]
	fn archive_diff_output() {
		// Item with Value.
		let item = ArchiveStorageDiffResult {
			key: "0x1".into(),
			result: StorageResultType::Value("res".into()),
			operation_type: ArchiveStorageDiffOperationType::Added,
			child_trie_key: None,
		};
		// Encode
		let ser = serde_json::to_string(&item).unwrap();
		let exp = r#"{"key":"0x1","value":"res","type":"added"}"#;
		assert_eq!(ser, exp);
		// Decode
		let dec: ArchiveStorageDiffResult = serde_json::from_str(exp).unwrap();
		assert_eq!(dec, item);

		// Item with Hash.
		let item = ArchiveStorageDiffResult {
			key: "0x1".into(),
			result: StorageResultType::Hash("res".into()),
			operation_type: ArchiveStorageDiffOperationType::Modified,
			child_trie_key: None,
		};
		// Encode
		let ser = serde_json::to_string(&item).unwrap();
		let exp = r#"{"key":"0x1","hash":"res","type":"modified"}"#;
		assert_eq!(ser, exp);
		// Decode
		let dec: ArchiveStorageDiffResult = serde_json::from_str(exp).unwrap();
		assert_eq!(dec, item);

		// Item with Hash, child trie key and removed.
		let item = ArchiveStorageDiffResult {
			key: "0x1".into(),
			result: StorageResultType::Hash("res".into()),
			operation_type: ArchiveStorageDiffOperationType::Deleted,
			child_trie_key: Some("0x2".into()),
		};
		// Encode
		let ser = serde_json::to_string(&item).unwrap();
		let exp = r#"{"key":"0x1","hash":"res","type":"deleted","childTrieKey":"0x2"}"#;
		assert_eq!(ser, exp);
		// Decode
		let dec: ArchiveStorageDiffResult = serde_json::from_str(exp).unwrap();
		assert_eq!(dec, item);
	}

	#[test]
	fn storage_result() {
		// Item with Value.
		let item = StorageResult {
			key: "0x1".into(),
			result: StorageResultType::Value("res".into()),
			child_trie_key: None,
		};
		// Encode
		let ser = serde_json::to_string(&item).unwrap();
		let exp = r#"{"key":"0x1","value":"res"}"#;
		assert_eq!(ser, exp);
		// Decode
		let dec: StorageResult = serde_json::from_str(exp).unwrap();
		assert_eq!(dec, item);

		// Item with Hash.
		let item = StorageResult {
			key: "0x1".into(),
			result: StorageResultType::Hash("res".into()),
			child_trie_key: None,
		};
		// Encode
		let ser = serde_json::to_string(&item).unwrap();
		let exp = r#"{"key":"0x1","hash":"res"}"#;
		assert_eq!(ser, exp);
		// Decode
		let dec: StorageResult = serde_json::from_str(exp).unwrap();
		assert_eq!(dec, item);

		// Item with DescendantsValues.
		let item = StorageResult {
			key: "0x1".into(),
			result: StorageResultType::ClosestDescendantMerkleValue("res".into()),
			child_trie_key: None,
		};
		// Encode
		let ser = serde_json::to_string(&item).unwrap();
		let exp = r#"{"key":"0x1","closestDescendantMerkleValue":"res"}"#;
		assert_eq!(ser, exp);
		// Decode
		let dec: StorageResult = serde_json::from_str(exp).unwrap();
		assert_eq!(dec, item);
	}

	#[test]
	fn storage_query() {
		// Item with Value.
		let item = StorageQuery { key: "0x1", query_type: StorageQueryType::Value };
		// Encode
		let ser = serde_json::to_string(&item).unwrap();
		let exp = r#"{"key":"0x1","type":"value"}"#;
		assert_eq!(ser, exp);
		// Decode
		let dec: StorageQuery<&str> = serde_json::from_str(exp).unwrap();
		assert_eq!(dec, item);

		// Item with Hash.
		let item = StorageQuery { key: "0x1", query_type: StorageQueryType::Hash };
		// Encode
		let ser = serde_json::to_string(&item).unwrap();
		let exp = r#"{"key":"0x1","type":"hash"}"#;
		assert_eq!(ser, exp);
		// Decode
		let dec: StorageQuery<&str> = serde_json::from_str(exp).unwrap();
		assert_eq!(dec, item);

		// Item with DescendantsValues.
		let item = StorageQuery { key: "0x1", query_type: StorageQueryType::DescendantsValues };
		// Encode
		let ser = serde_json::to_string(&item).unwrap();
		let exp = r#"{"key":"0x1","type":"descendantsValues"}"#;
		assert_eq!(ser, exp);
		// Decode
		let dec: StorageQuery<&str> = serde_json::from_str(exp).unwrap();
		assert_eq!(dec, item);

		// Item with DescendantsHashes.
		let item = StorageQuery { key: "0x1", query_type: StorageQueryType::DescendantsHashes };
		// Encode
		let ser = serde_json::to_string(&item).unwrap();
		let exp = r#"{"key":"0x1","type":"descendantsHashes"}"#;
		assert_eq!(ser, exp);
		// Decode
		let dec: StorageQuery<&str> = serde_json::from_str(exp).unwrap();
		assert_eq!(dec, item);

		// Item with Merkle.
		let item =
			StorageQuery { key: "0x1", query_type: StorageQueryType::ClosestDescendantMerkleValue };
		// Encode
		let ser = serde_json::to_string(&item).unwrap();
		let exp = r#"{"key":"0x1","type":"closestDescendantMerkleValue"}"#;
		assert_eq!(ser, exp);
		// Decode
		let dec: StorageQuery<&str> = serde_json::from_str(exp).unwrap();
		assert_eq!(dec, item);
	}

	#[test]
	fn storage_query_paginated() {
		let item = PaginatedStorageQuery {
			key: "0x1",
			query_type: StorageQueryType::Value,
			pagination_start_key: None,
		};
		// Encode
		let ser = serde_json::to_string(&item).unwrap();
		let exp = r#"{"key":"0x1","type":"value"}"#;
		assert_eq!(ser, exp);
		// Decode
		let dec: StorageQuery<&str> = serde_json::from_str(exp).unwrap();
		assert_eq!(dec.key, item.key);
		assert_eq!(dec.query_type, item.query_type);
		let dec: PaginatedStorageQuery<&str> = serde_json::from_str(exp).unwrap();
		assert_eq!(dec, item);

		let item = PaginatedStorageQuery {
			key: "0x1",
			query_type: StorageQueryType::Value,
			pagination_start_key: Some("0x2"),
		};
		// Encode
		let ser = serde_json::to_string(&item).unwrap();
		let exp = r#"{"key":"0x1","type":"value","paginationStartKey":"0x2"}"#;
		assert_eq!(ser, exp);
		// Decode
		let dec: PaginatedStorageQuery<&str> = serde_json::from_str(exp).unwrap();
		assert_eq!(dec, item);
	}
}
