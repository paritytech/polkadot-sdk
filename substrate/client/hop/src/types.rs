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

//! HOP types and data structures.

use crate::primitives::HopBlockNumber;
use codec::{Decode, Encode};
use serde::{Deserialize, Serialize};

/// Entry in the HOP data pool
#[derive(Debug, Clone, Encode, Decode)]
pub struct HopPoolEntry {
	/// The actual data blob
	pub data: Vec<u8>,
	/// Block number when this was added
	pub added_at: HopBlockNumber,
	/// Block number when this expires (added_at + retention_period)
	pub expires_at: HopBlockNumber,
	/// Size in bytes
	pub size: u64,
	/// Ephemeral ed25519 public keys of intended recipients.
	/// Each recipient claims by signing the content hash with their corresponding private key.
	pub recipients: Vec<[u8; 32]>,
	/// Tracks which recipients have claimed (by index into `recipients`).
	pub claimed: Vec<bool>,
}

impl HopPoolEntry {
	/// Create a new pool entry
	pub fn new(
		data: Vec<u8>,
		added_at: HopBlockNumber,
		retention_blocks: u32,
		recipients: Vec<[u8; 32]>,
	) -> Self {
		let size = data.len() as u64;
		let expires_at = added_at.saturating_add(retention_blocks);
		let claimed = vec![false; recipients.len()];

		Self { data, added_at, expires_at, size, recipients, claimed }
	}
}

/// Pool statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolStatus {
	/// Number of entries in the pool
	pub entry_count: usize,
	/// Total bytes used
	pub total_bytes: u64,
	/// Maximum bytes allowed
	pub max_bytes: u64,
}

/// Result of a successful `hop_submit` call
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitResult {
	/// The content hash of the submitted data
	pub hash: sp_core::Bytes,
	/// Current pool status after the submission
	pub pool_status: PoolStatus,
}

/// HOP errors
#[derive(Debug, thiserror::Error)]
pub enum HopError {
	#[error("Data too large: {0} bytes (max: {1})")]
	DataTooLarge(usize, u64),

	#[error("Pool full: {0}/{1} bytes used")]
	PoolFull(u64, u64),

	#[error("Data already exists in pool")]
	DuplicateEntry,

	#[error("Data not found")]
	NotFound,

	#[error("Invalid data: size cannot be zero")]
	EmptyData,

	#[error("Encoding error: {0}")]
	Encoding(String),

	#[error("Invalid signature")]
	InvalidSignature,

	#[error("Not an intended recipient")]
	NotRecipient,

	#[error("At least one recipient public key is required")]
	NoRecipients,

	#[error("Invalid recipient public key: expected 32 bytes, got {0}")]
	InvalidRecipientKey(usize),
}

impl From<HopError> for jsonrpsee::types::ErrorObjectOwned {
	fn from(err: HopError) -> Self {
		let code = match err {
			HopError::DataTooLarge(_, _) => 1001,
			HopError::PoolFull(_, _) => 1002,
			HopError::DuplicateEntry => 1003,
			HopError::NotFound => 1004,
			HopError::EmptyData => 1005,
			HopError::Encoding(_) => 1006,
			HopError::InvalidSignature => 1009,
			HopError::NotRecipient => 1010,
			HopError::NoRecipients => 1011,
			HopError::InvalidRecipientKey(_) => 1012,
		};

		jsonrpsee::types::ErrorObject::owned(code, err.to_string(), None::<()>)
	}
}

/// Maximum data size (64 MiB)
pub const MAX_DATA_SIZE: u64 = 64 * 1024 * 1024;

/// Default retention period in blocks (24 hours at 6 seconds per block = 14,400 blocks)
pub const DEFAULT_RETENTION_BLOCKS: u32 = 14_400;

/// Default maximum pool size in bytes (10 GiB)
pub const DEFAULT_MAX_POOL_SIZE: u64 = 10 * 1024 * 1024 * 1024;
