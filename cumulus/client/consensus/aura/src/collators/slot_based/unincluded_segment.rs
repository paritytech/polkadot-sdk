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
use cumulus_primitives_core::CumulusDigestItem;
use sc_client_api::Backend;
use sp_api::StorageProof;
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

/// Hydrate a list of unincluded-segment headers (oldest first) into [`CollatorSegmentEntry`]s.
///
/// Headers are first regrouped into the original PoV bundles they were built in — a bundle is a run
/// of consecutive blocks ending at [`BlockBundleInfo::is_last`] (a header without the digest is a
/// standalone single-block bundle) — and each bundle is hydrated into one entry by
/// [`build_bundle_entry`], mirroring how the block builder packs a multi-block PoV. All blocks of a
/// bundle share the same `CoreInfo.selector`, so core-affinity bucketing never splits a bundle
/// across callers. Bundles that fail to hydrate locally are skipped; the rest are preserved.
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
	// Group the (oldest-first) headers into their original bundles by `BlockBundleInfo` index. A
	// bundle is a run of consecutive indices starting at 0 — the same boundary the proof compaction
	// (`block_import::get_ignored_nodes`) uses, so merging the per-block proofs below reconstructs
	// exactly one PoV per bundle. Start a new bundle whenever the index doesn't continue the run
	// (a fresh bundle opens at 0) or the digest is absent (a standalone single-block bundle). We
	// intentionally key off the index rather than `is_last`, which a `UseFullCore` early-break can
	// leave unset on the real last block.
	let mut bundles: Vec<Vec<Block::Header>> = Vec::new();
	let mut current: Vec<Block::Header> = Vec::new();
	let mut prev_index: Option<u8> = None;
	for header in headers {
		let index =
			CumulusDigestItem::find_block_bundle_info(header.digest()).map(|info| info.index);
		let continues = match (prev_index, index) {
			(Some(prev), Some(idx)) => prev.checked_add(1) == Some(idx),
			_ => false,
		};
		if !continues && !current.is_empty() {
			bundles.push(core::mem::take(&mut current));
		}
		prev_index = index;
		current.push(header);
	}
	if !current.is_empty() {
		bundles.push(current);
	}

	let mut entries = Vec::with_capacity(bundles.len());
	for bundle in bundles {
		// Attribute a failure to the bundle's first block for logging.
		let block_number = *bundle[0].number();
		let block_hash = bundle[0].hash();
		match build_bundle_entry(bundle, para_backend, code_hash_provider, store) {
			Ok(entry) => entries.push(entry),
			Err(err) => tracing::warn!(
				target: LOG_TARGET,
				?block_number,
				?block_hash,
				%err,
				"Skipping unincluded-segment bundle.",
			),
		}
	}
	entries
}

/// Rebuild a [`CollatorSegmentEntry`] for one PoV bundle (a run of consecutive parablocks).
///
/// Each block's [`ResubmissionStore`] row is the anchor: it carries the block's (bundle-compacted)
/// proof, the relay-parent header, and the persisted-validation-data captured at build/import time.
/// The bundle's blocks are collected in order and their per-block proofs merged back into the full
/// bundle proof via [`StorageProof::merge`] — the same shape the block builder produced for the
/// fresh multi-block PoV. The bundle is anchored on its first block: relay parent, PVD (whose
/// `parent_head` is the bundle's parent), parent header, and validation-code hash all come from it.
///
/// Returns a [`HydrateError`] identifying *which* lookup failed; the caller logs it with the
/// failing bundle's first block number/hash. All variants are recoverable at the segment level.
fn build_bundle_entry<Block, B, CHP>(
	bundle: Vec<Block::Header>,
	para_backend: &B,
	code_hash_provider: &CHP,
	store: &ResubmissionStore<Block, B>,
) -> Result<CollatorSegmentEntry<Block>, HydrateError>
where
	Block: BlockT,
	B: Backend<Block>,
	CHP: ValidationCodeHashProvider<Block::Hash>,
{
	// The bundle's parent is the parent of its first block; all bundle blocks share the same relay
	// parent and PVD context, so anchor on the first block's stored row.
	let bundle_parent_hash = *bundle[0].parent_hash();

	let parent_header = para_backend
		.blockchain()
		.header(bundle_parent_hash)
		.map_err(HydrateError::ParentHeaderBackend)?
		.ok_or(HydrateError::ParentHeaderMissing)?;

	let validation_code_hash = code_hash_provider
		.code_hash_at(bundle_parent_hash)
		.ok_or(HydrateError::NoValidationCodeHash)?;

	let mut blocks = Vec::with_capacity(bundle.len());
	let mut proofs = Vec::with_capacity(bundle.len());
	let mut anchor = None;
	for header in bundle {
		let block_hash = header.hash();
		// The store row keyed by the block's hash carries its proof and PVD. If it isn't there we
		// can't reassemble the bundle.
		let stored = store
			.load(block_hash)
			.map_err(HydrateError::StoreLoad)?
			.ok_or(HydrateError::StoredEntryMissing)?;

		let body = para_backend
			.blockchain()
			.body(block_hash)
			.map_err(HydrateError::BodyBackend)?
			.ok_or(HydrateError::BodyMissing)?;

		if anchor.is_none() {
			// The stored PVD already carries the correct `parent_head` (the bundle's parent); the
			// block builder and the resubmission backfill both anchor it at write time.
			anchor = Some((stored.relay_parent_header.hash(), stored.persisted_validation_data));
		}
		blocks.push(Block::new(header, body));
		proofs.push((*stored.proof).clone());
	}

	// Non-empty by construction (`hydrate_segment` never pushes an empty bundle).
	let (relay_parent, validation_data) = anchor.expect("bundle is non-empty; qed");

	Ok(CollatorSegmentEntry {
		relay_parent,
		parent_header,
		blocks,
		// Merge the per-block (bundle-compacted) proofs back into the full bundle proof, matching
		// how the block builder assembled the fresh collation's PoV.
		proof: StorageProof::merge(proofs),
		validation_code_hash,
		validation_data,
	})
}
