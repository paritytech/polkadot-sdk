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

//! Per-block hydration of the V3 resubmittable headers.
//!
//! The parent search yields bare headers; [`hydrate_segment`] rebuilds each into a
//! [`CollationParts`] from local state only (no relay-chain client): proofs and relay parent from
//! the resubmission store, validation data from the block's post-state, body/parent/code-hash from
//! the parachain backend.

use super::message::CollationParts;
use codec::Decode;
use cumulus_client_consensus_common::ValidationCodeHashProvider;
use cumulus_client_resubmission_store::ResubmissionStore;
use cumulus_primitives_core::{CumulusDigestItem, PersistedValidationData};
use sc_client_api::{Backend, TrieCacheContext};
use sp_api::StorageProof;
use sp_blockchain::{Backend as BlockchainBackend, Error as BlockchainError, HeaderBackend};
use sp_crypto_hashing::twox_128;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use sp_state_machine::Backend as StateBackend;

const LOG_TARGET: &str = "aura::cumulus::block_builder_task";

/// Why a single resubmittable header could not be hydrated into a [`CollationParts`].
///
/// Carries only the *cause*; the caller attaches the failing block's number/hash to its log line.
/// None of these are fatal at the segment level — [`hydrate_segment`] skips the entry and
/// continues with the rest.
#[derive(Debug, thiserror::Error)]
enum HydrateError {
	#[error("no stored storage-proof entry (entry was pruned or never written)")]
	StoredEntryMissing,
	#[error("resubmission store load failed: {0}")]
	StoreLoad(BlockchainError),
	#[error("parent header not in the parachain backend")]
	ParentHeaderMissing,
	#[error("parachain backend errored looking up parent header: {0}")]
	ParentHeaderBackend(BlockchainError),
	#[error("block body not in the parachain backend")]
	BodyMissing,
	#[error("parachain backend errored looking up block body: {0}")]
	BodyBackend(BlockchainError),
	#[error("no validation-code hash at parent")]
	NoValidationCodeHash,
	#[error("block state unavailable: {0}")]
	StateUnavailable(BlockchainError),
	#[error("no validation data in the block's post-state")]
	ValidationDataMissing,
}

/// Read a parablock's execution [`PersistedValidationData`] from its post-state
/// (`ParachainSystem::ValidationData`, set by the `set_validation_data` inherent). Coupled to the
/// conventional `ParachainSystem` pallet name.
fn read_validation_data<Block, B>(
	para_backend: &B,
	block_hash: Block::Hash,
) -> Result<PersistedValidationData, HydrateError>
where
	Block: BlockT,
	B: Backend<Block>,
{
	let key = [twox_128(b"ParachainSystem"), twox_128(b"ValidationData")].concat();
	let state = para_backend
		.state_at(block_hash, TrieCacheContext::Untrusted)
		.map_err(HydrateError::StateUnavailable)?;
	let raw = state
		.storage(&key)
		.map_err(|_| HydrateError::ValidationDataMissing)?
		.ok_or(HydrateError::ValidationDataMissing)?;
	PersistedValidationData::decode(&mut &raw[..]).map_err(|_| HydrateError::ValidationDataMissing)
}

/// Hydrate resubmittable headers (oldest first) into [`CollationParts`], one per original PoV
/// bundle.
///
/// Headers are regrouped into the bundles they were built in and each is rebuilt by
/// [`build_bundle_parts`], mirroring how the builder packs a multi-block PoV. All blocks of a
/// bundle share a `CoreInfo.selector`, so core-affinity bucketing never splits one. Bundles that
/// fail to hydrate are skipped.
pub(super) fn hydrate_segment<Block, B, CHP>(
	headers: Vec<Block::Header>,
	para_backend: &B,
	code_hash_provider: &CHP,
	store: &ResubmissionStore<Block, B>,
) -> Vec<CollationParts<Block>>
where
	Block: BlockT,
	B: Backend<Block>,
	CHP: ValidationCodeHashProvider<Block::Hash>,
{
	// Group the (oldest-first) headers into their original bundles by `BlockBundleInfo` index: a
	// bundle is a run of consecutive indices from 0, the same boundary the proof compaction
	// (`block_import::get_ignored_nodes`) uses, so merging the per-block proofs below reconstructs
	// one PoV per bundle. A new bundle starts when the index doesn't continue the run or the digest
	// is absent (a standalone block). Key off the index, not `is_last`: the builder can stop a
	// bundle early (`UseFullCore`, runtime upgrade), leaving no `is_last` block at all.
	let num_headers = headers.len();
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

	let num_bundles = bundles.len();
	let mut parts = Vec::with_capacity(num_bundles);
	for bundle in bundles {
		// Attribute a failure to the bundle's first block for logging.
		let block_number = *bundle[0].number();
		let block_hash = bundle[0].hash();
		let bundle_len = bundle.len();
		match build_bundle_parts(bundle, para_backend, code_hash_provider, store) {
			Ok(bundle_parts) => {
				tracing::trace!(
					target: LOG_TARGET,
					?block_number,
					?block_hash,
					bundle_len,
					"Hydrated resubmittable bundle.",
				);
				parts.push(bundle_parts)
			},
			Err(err) => tracing::warn!(
				target: LOG_TARGET,
				?block_number,
				?block_hash,
				%err,
				"Skipping resubmittable bundle.",
			),
		}
	}

	tracing::debug!(
		target: LOG_TARGET,
		headers = num_headers,
		bundles = num_bundles,
		hydrated = parts.len(),
		skipped = num_bundles.saturating_sub(parts.len()),
		"Hydrated resubmittable headers.",
	);

	parts
}

/// Rebuild a [`CollationParts`] for one PoV bundle (a run of consecutive parablocks).
///
/// The per-block proofs from each block's [`ResubmissionStore`] row are merged back into the full
/// bundle proof via [`StorageProof::merge`], matching the fresh multi-block PoV. The bundle is
/// anchored on its first block: relay parent from its store row, validation data from its
/// post-state, parent header and code hash from the bundle's parent.
///
/// The returned [`HydrateError`] names which lookup failed; all variants are recoverable at the
/// segment level.
fn build_bundle_parts<Block, B, CHP>(
	bundle: Vec<Block::Header>,
	para_backend: &B,
	code_hash_provider: &CHP,
	store: &ResubmissionStore<Block, B>,
) -> Result<CollationParts<Block>, HydrateError>
where
	Block: BlockT,
	B: Backend<Block>,
	CHP: ValidationCodeHashProvider<Block::Hash>,
{
	// All bundle blocks share a relay parent and PVD context, so anchor on the first block.
	let bundle_parent_hash = *bundle[0].parent_hash();

	let parent_header = para_backend
		.blockchain()
		.header(bundle_parent_hash)
		.map_err(HydrateError::ParentHeaderBackend)?
		.ok_or(HydrateError::ParentHeaderMissing)?;

	let validation_code_hash = code_hash_provider
		.code_hash_at(bundle_parent_hash)
		.ok_or(HydrateError::NoValidationCodeHash)?;

	// Anchor on the first block: its PVD's `parent_head` is the bundle's parent, and its store row
	// carries the shared relay parent, session, and header — reused so the collation task does not
	// re-query the relay chain.
	let first_hash = bundle[0].hash();
	let validation_data = read_validation_data(para_backend, first_hash)?;
	let anchor = store
		.load(first_hash)
		.map_err(HydrateError::StoreLoad)?
		.ok_or(HydrateError::StoredEntryMissing)?;
	let relay_parent_header = anchor.relay_parent_header;
	let relay_parent = relay_parent_header.hash();

	let mut blocks = Vec::with_capacity(bundle.len());
	let mut proofs = Vec::with_capacity(bundle.len());
	for header in bundle {
		let block_hash = header.hash();
		// The store row carries this block's proof; without it the bundle can't be reassembled.
		let stored = store
			.load(block_hash)
			.map_err(HydrateError::StoreLoad)?
			.ok_or(HydrateError::StoredEntryMissing)?;

		let body = para_backend
			.blockchain()
			.body(block_hash)
			.map_err(HydrateError::BodyBackend)?
			.ok_or(HydrateError::BodyMissing)?;

		blocks.push(Block::new(header, body));
		proofs.push((*stored.proof).clone());
	}

	Ok(CollationParts {
		relay_parent,
		parent_header,
		blocks,
		// Merge the per-block proofs back into the full bundle proof.
		proof: StorageProof::merge(proofs),
		validation_code_hash,
		validation_data,
		relay_parent_session: anchor.relay_parent_session,
		relay_parent_storage_root: *relay_parent_header.state_root(),
		relay_parent_number: *relay_parent_header.number(),
	})
}
