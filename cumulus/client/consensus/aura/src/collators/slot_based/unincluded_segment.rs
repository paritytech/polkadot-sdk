// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! Per-block hydration for the V3 unincluded segment.
//!
//! The parent search yields the unincluded segment as bare headers. [`hydrate_segment`] rebuilds
//! each into a [`CollatorSegmentEntry`] anchored on the resubmission store: the block builder and
//! block-import paths write a `StoredEntry` keyed by parablock hash alongside the block, capturing
//! the relay-parent header, session, and persisted-validation-data it was built/validated against.
//! Hydration reads the relay-parent identity and PVD straight from that entry (rather than
//! re-resolving against a relay-chain client whose blocks may have rotated out of view) and only
//! looks up the parachain-local body, parent header, and validation-code hash.

use super::CollatorSegmentEntry;
use cumulus_client_consensus_common::ValidationCodeHashProvider;
use cumulus_client_resubmission_store::ResubmissionStore;
use sc_client_api::Backend;
use sp_blockchain::{Backend as BlockchainBackend, Error as BlockchainError, HeaderBackend};
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};

const LOG_TARGET: &str = "aura::cumulus::block_builder_task";

/// Why a single unincluded-segment header could not be hydrated into a [`CollatorSegmentEntry`].
///
/// Carries only the *cause*; the caller attaches the failing block's number/hash to its log line.
/// None of these are fatal at the segment level — [`hydrate_segment`] skips the entry and
/// continues with the rest.
#[derive(Debug, thiserror::Error)]
enum HydrateError {
	/// No stored proof/relay-parent metadata in the resubmission store (pruned on finality, or
	/// never written — e.g. the block was imported before the store was wired up).
	#[error("no stored storage-proof entry (entry was pruned or never written)")]
	StoredEntryMissing,
	/// The resubmission store errored on read.
	#[error("resubmission store load failed: {0}")]
	StoreLoad(BlockchainError),
	/// The parent parablock's header is not in the local backend (pruned or never imported).
	#[error("parent header not in the parachain backend")]
	ParentHeaderMissing,
	/// The parachain backend errored while looking up the parent header.
	#[error("parachain backend errored looking up parent header: {0}")]
	ParentHeaderBackend(BlockchainError),
	/// The block's body is not in the local backend.
	#[error("block body not in the parachain backend")]
	BodyMissing,
	/// The parachain backend errored while looking up the block body.
	#[error("parachain backend errored looking up block body: {0}")]
	BodyBackend(BlockchainError),
	/// No validation-code hash known at the parent parablock.
	#[error("no validation-code hash at parent")]
	NoValidationCodeHash,
}

/// Hydrate a list of unincluded-segment headers (oldest first) into [`CollatorSegmentEntry`]s by
/// calling [`build_entry`] on each. Headers that fail to hydrate locally are skipped — the rest of
/// the segment is preserved. The specific failure cause is reported via [`HydrateError`] and
/// logged here with the failing block's number/hash.
pub(super) fn hydrate_segment<Block, B, CHP>(
	headers: Vec<Block::Header>,
	para_backend: &B,
	code_hash_provider: &CHP,
	store: &ResubmissionStore<Block, B>,
) -> Vec<CollatorSegmentEntry<Block>>
where
	Block: BlockT,
	B: Backend<Block>,
	CHP: ValidationCodeHashProvider<Block::Hash>,
{
	let mut entries = Vec::with_capacity(headers.len());
	for header in headers {
		let block_number = *header.number();
		let block_hash = header.hash();
		match build_entry(header, para_backend, code_hash_provider, store) {
			Ok(entry) => entries.push(entry),
			Err(err) => tracing::warn!(
				target: LOG_TARGET,
				?block_number,
				?block_hash,
				%err,
				"Skipping unincluded-segment entry.",
			),
		}
	}
	entries
}

/// Rebuild a [`CollatorSegmentEntry`] for one unincluded parablock.
///
/// The [`ResubmissionStore`] entry for the block's hash is the anchor: it carries the proof, the
/// relay-parent header, and the persisted-validation-data captured at build/import time. From
/// those, only the parachain-local body, parent header, and validation-code hash still need to be
/// looked up here — no relay-chain client call is made.
///
/// Returns a [`HydrateError`] identifying *which* lookup failed; the caller logs it with the
/// failing block's number/hash. All variants are recoverable at the segment level.
fn build_entry<Block, B, CHP>(
	header: Block::Header,
	para_backend: &B,
	code_hash_provider: &CHP,
	store: &ResubmissionStore<Block, B>,
) -> Result<CollatorSegmentEntry<Block>, HydrateError>
where
	Block: BlockT,
	B: Backend<Block>,
	CHP: ValidationCodeHashProvider<Block::Hash>,
{
	let block_hash = header.hash();
	let parent_hash = *header.parent_hash();

	// Anchor: the store row keyed by the block's hash carries the proof, the relay-parent header,
	// and the PVD captured at build/import time. If it isn't there we can't resubmit.
	let stored = store
		.load(block_hash)
		.map_err(HydrateError::StoreLoad)?
		.ok_or(HydrateError::StoredEntryMissing)?;

	let relay_parent = stored.relay_parent_header.hash();

	let parent_header = para_backend
		.blockchain()
		.header(parent_hash)
		.map_err(HydrateError::ParentHeaderBackend)?
		.ok_or(HydrateError::ParentHeaderMissing)?;

	let body = para_backend
		.blockchain()
		.body(block_hash)
		.map_err(HydrateError::BodyBackend)?
		.ok_or(HydrateError::BodyMissing)?;
	let block = Block::new(header, body);

	let validation_code_hash =
		code_hash_provider.code_hash_at(parent_hash).ok_or(HydrateError::NoValidationCodeHash)?;

	// The stored PVD already carries the correct `parent_head`: the block builder and the
	// resubmission backfill both anchor it on the block's actual para parent at write time.
	let validation_data = stored.persisted_validation_data;

	Ok(CollatorSegmentEntry {
		relay_parent,
		parent_header,
		blocks: vec![block],
		proof: (*stored.proof).clone(),
		validation_code_hash,
		validation_data,
	})
}
