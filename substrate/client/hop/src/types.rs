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

use crate::primitives::{HopBlockNumber, HopHash};
use codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use sp_core::{bounded_vec::BoundedVec, hashing::blake2_256, ConstU32};
use sp_runtime::MultiSigner;

/// Sender identity derived from the account that signed the submission.
pub type SenderId = [u8; 32];

/// Metadata for a pool entry (stored in-memory index and on-disk .meta files).
#[derive(Debug, Clone, Encode, Decode)]
pub struct HopEntryMeta {
	/// Block number when this was added
	pub added_at: HopBlockNumber,
	/// Block number when this expires (added_at + retention_period)
	pub expires_at: HopBlockNumber,
	/// Size in bytes
	pub size: u64,
	/// Ephemeral public keys of intended recipients (MultiSigner: ed25519, sr25519, or ecdsa).
	///
	/// Using a `BoundedVec` means a corrupted / hostile on-disk `.meta` file with
	/// too many recipients fails to SCALE-decode and is discarded during startup
	/// recovery rather than being loaded into the in-memory index.
	pub recipients: RecipientVec,
	/// Tracks which recipients have claimed (by index into `recipients`).
	pub claimed: Vec<bool>,
	/// Account ID of the sender who submitted this entry.
	pub sender_id: SenderId,
	/// Whether this entry has been promoted to permanent on-chain storage.
	pub promoted: bool,
}

impl HopEntryMeta {
	/// Create a new entry metadata (without data blob)
	pub fn new(
		size: u64,
		added_at: HopBlockNumber,
		retention_blocks: u32,
		recipients: RecipientVec,
		sender_id: SenderId,
	) -> Self {
		let expires_at = added_at.saturating_add(retention_blocks);
		let claimed = vec![false; recipients.len()];
		Self { added_at, expires_at, size, recipients, claimed, sender_id, promoted: false }
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

	#[error("Invalid signature")]
	InvalidSignature,

	#[error("Not an intended recipient")]
	NotRecipient,

	#[error("At least one recipient public key is required")]
	NoRecipients,

	#[error("Invalid recipient: failed to SCALE-decode MultiSigner")]
	InvalidRecipientKey,

	#[error("User quota exceeded: using {used} of {limit} bytes")]
	UserQuotaExceeded { used: u64, limit: u64 },

	#[error("Account does not have a valid authorization")]
	NotAuthorized,

	#[error("Invalid signer: failed to SCALE-decode MultiSigner")]
	InvalidSigner,

	#[error("I/O error: {0}")]
	IoError(#[from] std::io::Error),

	#[error("Recipient already acknowledged, data may have been deleted")]
	AlreadyClaimed,

	#[error("Invalid hash length: expected 32 bytes, got {0}")]
	InvalidHashLength(usize),

	#[error("Runtime API error: {0}")]
	RuntimeApiError(#[from] sp_api::ApiError),

	#[error("Too many recipients: {provided} (max {limit})")]
	TooManyRecipients { provided: usize, limit: usize },

	#[error("Duplicate recipient in list")]
	DuplicateRecipient,

	#[error("Rate limited: retry after {retry_after_secs}s")]
	RateLimited { retry_after_secs: u64 },
}

impl From<HopError> for jsonrpsee::types::ErrorObjectOwned {
	fn from(err: HopError) -> Self {
		let code = match err {
			HopError::DataTooLarge(_, _) => 1001,
			HopError::PoolFull(_, _) => 1002,
			HopError::DuplicateEntry => 1003,
			HopError::NotFound => 1004,
			HopError::EmptyData => 1005,
			HopError::InvalidSignature => 1007,
			HopError::NotRecipient => 1008,
			HopError::NoRecipients => 1009,
			HopError::InvalidRecipientKey => 1010,
			HopError::UserQuotaExceeded { .. } => 1011,
			HopError::NotAuthorized => 1012,
			HopError::IoError(_) => 1013,
			HopError::InvalidSigner => 1014,
			HopError::AlreadyClaimed => 1015,
			HopError::InvalidHashLength(_) => 1016,
			HopError::RuntimeApiError(_) => 1017,
			HopError::TooManyRecipients { .. } => 1018,
			HopError::DuplicateRecipient => 1019,
			HopError::RateLimited { .. } => 1020,
		};

		jsonrpsee::types::ErrorObject::owned(code, err.to_string(), None::<()>)
	}
}

/// Crate-level upper bound on a HOP entry's data size (8 MiB). The effective
/// cap is whatever `HopApi::max_promotion_size()` reports on the current
/// runtime; this constant just anchors the worst case.
pub const MAX_DATA_SIZE: u64 = 8 * 1024 * 1024;

/// Default retention period in blocks (24 hours at 6 seconds per block = 14,400 blocks)
pub const DEFAULT_RETENTION_BLOCKS: u32 = 14_400;

/// Default maximum pool size in bytes (10 GiB)
pub const DEFAULT_MAX_POOL_SIZE: u64 = 10 * 1024 * 1024 * 1024;

/// Default maximum pool size in MiB (10 GiB = 10240 MiB)
pub const DEFAULT_MAX_POOL_SIZE_MIB: u64 = DEFAULT_MAX_POOL_SIZE / (1024 * 1024);

/// Default maintenance interval in seconds (1 hour)
pub const DEFAULT_CHECK_INTERVAL_SECS: u64 = 3600;

/// Maximum number of recipients allowed per submission.
///
/// Caps the fan-out so that per-entry metadata (both RAM and disk) is bounded
/// and `find_recipient`'s signature-verification scan is bounded.
pub const MAX_RECIPIENTS: u32 = 256;

/// A `Vec<MultiSigner>` that SCALE-decode rejects if it exceeds `MAX_RECIPIENTS`,
/// enforcing the fan-out cap at the type level instead of via scattered runtime checks.
pub type RecipientVec = BoundedVec<MultiSigner, ConstU32<MAX_RECIPIENTS>>;

/// Default per-user quota in MiB (1 GiB). Hard cap, not scaled by active users.
pub const DEFAULT_MAX_USER_SIZE_MIB: u64 = 1024;

/// Default buffer before expiry at which to start promoting entries on-chain
/// (600 blocks ≈ 1 h at 6 s per block).
pub const DEFAULT_PROMOTION_BUFFER_BLOCKS: u32 = 600;

/// Default sustained submit rate per account (requests per minute).
pub const DEFAULT_SUBMIT_RATE_PER_MIN: u32 = 60;

/// Default submit burst per account (requests).
pub const DEFAULT_SUBMIT_BURST: u32 = 120;

/// Default sustained bandwidth per account in MiB per minute.
pub const DEFAULT_BANDWIDTH_PER_MIN_MIB: u64 = 256;

/// Default bandwidth burst per account in MiB.
pub const DEFAULT_BANDWIDTH_BURST_MIB: u64 = 512;

/// Domain-separator prefix for `hop_submit` signatures.
pub const HOP_SUBMIT_CONTEXT: &[u8] = b"hop-submit-v1:";

/// Domain-separator prefix for `hop_claim` signatures.
pub const HOP_CLAIM_CONTEXT: &[u8] = b"hop-claim-v1:";

/// Domain-separator prefix for `hop_ack` signatures.
pub const HOP_ACK_CONTEXT: &[u8] = b"hop-ack-v1:";

/// Compute the 32-byte payload that HOP recipients / submitters sign for a given
/// operation. This is `blake2_256(context || hash)` and ensures signatures from
/// one operation cannot be replayed in another.
pub fn signing_payload(context: &[u8], hash: &HopHash) -> [u8; 32] {
	let mut buf = Vec::with_capacity(context.len() + 32);
	buf.extend_from_slice(context);
	buf.extend_from_slice(hash.as_bytes());
	blake2_256(&buf)
}

/// Per-recipient overhead charged against pool capacity and per-user quota, in bytes.
/// Covers the in-memory `MultiSigner` variant plus its parallel `bool` in `claimed`.
/// Kept as a small constant that over-approximates `size_of::<MultiSigner>() + 1`.
pub const METADATA_COST_PER_RECIPIENT: u64 = 40;

/// Total bytes an entry charges against pool capacity: the blob plus bounded
/// per-recipient metadata overhead.
pub fn entry_accounted_size(data_size: u64, num_recipients: usize) -> u64 {
	data_size.saturating_add((num_recipients as u64).saturating_mul(METADATA_COST_PER_RECIPIENT))
}
