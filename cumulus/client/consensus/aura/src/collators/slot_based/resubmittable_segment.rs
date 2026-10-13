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

//! A [`ResubmittableSegment`] of V3 parablock headers, hydrated into [`CollationParts`] from
//! local state only.

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

const LOG_TARGET: &str = "aura::resubmittable_segment";

/// Why a bundle could not be hydrated. Recoverable: [`ResubmittableSegment::hydrate`] skips the
/// bundle.
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
	#[error("bundle starts at index {0}, not 0 (skipped index or partial bundle)")]
	MalformedBundleStart(u8),
}

/// A core's resubmittable parablocks (oldest first), ready to be hydrated back into
/// [`CollationParts`]. The headers are guaranteed to end on a bundle-ender: `find_parent` trims a
/// mid-bundle tail before the segment reaches here.
pub(super) struct ResubmittableSegment<Block: BlockT>(Vec<Block::Header>);

impl<Block: BlockT> ResubmittableSegment<Block> {
	pub(super) fn new(headers: Vec<Block::Header>) -> Self {
		Self(headers)
	}

	/// Regroup the (oldest-first) headers into their original bundles: a run of consecutive
	/// `BlockBundleInfo` indices from 0. Keyed off the index, not `is_last`, which a truncated
	/// bundle lacks.
	///
	/// A bundle must start at index 0 (or be a digest-less standalone); a non-zero start is a
	/// skipped index or a partial leading bundle, i.e. a corrupt sequence, and errors. Completeness
	/// is not checked: `find_parent` already trims a mid-bundle tail, so the trailing bundle is
	/// whole.
	fn into_bundles(self) -> Result<Vec<Vec<Block::Header>>, HydrateError> {
		let mut bundles = Vec::new();
		let mut current = Vec::new();
		let mut prev_index: Option<u8> = None;
		for header in self.0 {
			let index =
				CumulusDigestItem::find_block_bundle_info(header.digest()).map(|info| info.index);
			let continues = matches!((prev_index, index), (Some(prev), Some(idx)) if prev.checked_add(1) == Some(idx));
			if !continues {
				if let Some(idx) = index {
					if idx != 0 {
						return Err(HydrateError::MalformedBundleStart(idx));
					}
				}
				if !current.is_empty() {
					bundles.push(core::mem::take(&mut current));
				}
			}
			prev_index = index;
			current.push(header);
		}
		if !current.is_empty() {
			bundles.push(current);
		}
		Ok(bundles)
	}

	/// Hydrate into [`CollationParts`], one per original bundle. Bundles that fail to hydrate are
	/// skipped.
	pub(super) fn hydrate<B, CHP>(
		self,
		para_backend: &B,
		code_hash_provider: &CHP,
		store: &ResubmissionStore<Block, B>,
	) -> Vec<CollationParts<Block>>
	where
		B: Backend<Block>,
		CHP: ValidationCodeHashProvider<Block::Hash>,
	{
		let num_headers = self.0.len();
		let bundles = match self.into_bundles() {
			Ok(bundles) => bundles,
			Err(err) => {
				tracing::warn!(target: LOG_TARGET, %err, "Malformed resubmittable segment; skipping.");
				return Vec::new();
			},
		};
		let num_bundles = bundles.len();
		let mut parts = Vec::with_capacity(num_bundles);
		for bundle in bundles {
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
}

/// Rebuild [`CollationParts`] for one bundle: merge the per-block store proofs into one PoV,
/// anchored on the bundle's first block.
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
	let bundle_parent_hash = *bundle[0].parent_hash();

	let parent_header = para_backend
		.blockchain()
		.header(bundle_parent_hash)
		.map_err(HydrateError::ParentHeaderBackend)?
		.ok_or(HydrateError::ParentHeaderMissing)?;

	let validation_code_hash = code_hash_provider
		.code_hash_at(bundle_parent_hash)
		.ok_or(HydrateError::NoValidationCodeHash)?;

	// Anchor on the first block; its store row carries the shared relay parent, session and header.
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
		proof: StorageProof::merge(proofs),
		validation_code_hash,
		validation_data,
		relay_parent_session: anchor.relay_parent_session,
		relay_parent_storage_root: *relay_parent_header.state_root(),
		relay_parent_number: *relay_parent_header.number(),
	})
}

/// Read a parablock's [`PersistedValidationData`] from its `ParachainSystem::ValidationData`
/// post-state.
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

#[cfg(test)]
mod tests {
	use super::*;
	use cumulus_primitives_core::BlockBundleInfo;
	use cumulus_test_client::runtime::Block as TestBlock;

	type TestHeader = <TestBlock as BlockT>::Header;

	/// A header at `number` with a `BlockBundleInfo` `index` when `Some`.
	fn header(number: u32, index: Option<u8>) -> TestHeader {
		header_with(number, index, false)
	}

	fn header_with(number: u32, index: Option<u8>, is_last: bool) -> TestHeader {
		let mut header = TestHeader::new(
			number,
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		);
		if let Some(index) = index {
			header.digest_mut().push(BlockBundleInfo { index, is_last }.to_digest_item());
		}
		header
	}

	/// The block numbers in each grouped bundle.
	fn grouped_numbers(headers: Vec<TestHeader>) -> Vec<Vec<u32>> {
		ResubmittableSegment::<TestBlock>::new(headers)
			.into_bundles()
			.expect("valid bundle sequence; qed")
			.into_iter()
			.map(|bundle| bundle.iter().map(|h| *h.number()).collect())
			.collect()
	}

	#[test]
	fn empty_input_yields_no_bundles() {
		assert!(grouped_numbers(vec![]).is_empty());
	}

	#[test]
	fn consecutive_indices_from_zero_are_one_bundle() {
		let headers = vec![header(1, Some(0)), header(2, Some(1)), header(3, Some(2))];
		assert_eq!(grouped_numbers(headers), vec![vec![1, 2, 3]]);
	}

	#[test]
	fn a_restart_at_zero_opens_a_new_bundle() {
		let headers =
			vec![header(1, Some(0)), header(2, Some(1)), header(3, Some(0)), header(4, Some(1))];
		assert_eq!(grouped_numbers(headers), vec![vec![1, 2], vec![3, 4]]);
	}

	#[test]
	fn a_gap_in_the_index_is_rejected() {
		// 0 then 2 (skip 1) would make the second bundle start at index 2 — a corrupt sequence.
		let headers = vec![header(1, Some(0)), header(2, Some(2))];
		assert!(matches!(
			ResubmittableSegment::<TestBlock>::new(headers).into_bundles(),
			Err(HydrateError::MalformedBundleStart(2))
		));
	}

	#[test]
	fn a_block_without_a_digest_is_a_standalone_bundle() {
		// A missing digest is always standalone.
		let headers = vec![header(1, Some(0)), header(2, None), header(3, Some(0))];
		assert_eq!(grouped_numbers(headers), vec![vec![1], vec![2], vec![3]]);
	}

	#[test]
	fn is_last_does_not_affect_grouping() {
		// `is_last` is ignored: setting it anywhere gives the same grouping.
		let headers = vec![
			header_with(1, Some(0), true), // `is_last` mid-run: ignored, the run continues.
			header_with(2, Some(1), false),
			header_with(3, Some(0), true), // `is_last` at a run end: still just a new run.
			header_with(4, Some(1), false),
		];
		assert_eq!(grouped_numbers(headers), vec![vec![1, 2], vec![3, 4]]);
	}
}
