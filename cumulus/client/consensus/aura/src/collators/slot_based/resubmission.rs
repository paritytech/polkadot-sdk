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

//! Resolving relay-chain data needed to record resubmission entries.
//!
//! [`resolve_session_and_pvd`] is shared by the build path (block_builder_task) and the
//! resubmission backfill task. [`run_resubmission_backfill`] is the task that records resubmission
//! entries for blocks imported at the tip (built by other collators), doing the relay-chain queries
//! off the block-import path.

use super::SlotBasedBlockImportHandle;
use cumulus_client_resubmission_store::{now_unix_ms, prepare_resubmission_aux_data};
use cumulus_primitives_core::{
	relay_chain::{BlockId, Hash as RelayHash, Header as RelayHeader, SessionIndex},
	CumulusDigestItem, PersistedValidationData, RelayBlockIdentifier,
};
use cumulus_relay_chain_interface::RelayChainInterface;
use polkadot_primitives::{Id as ParaId, OccupiedCoreAssumption};
use sc_client_api::backend::AuxStore;
use sp_api::StorageProof;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use std::sync::Arc;

const LOG_TARGET: &str = "aura::resubmission";

/// Fetch the session index and persisted validation data (with `OccupiedCoreAssumption::TimedOut`)
/// for the given relay parent and para id.
///
/// Returns `None` on any error. The returned PVD is the relay-chain's own PVD
/// as-is — its `parent_head` is the currently-included head, which is the authoritative value for
/// resubmission.
pub(crate) async fn resolve_session_and_pvd<R: RelayChainInterface + ?Sized>(
	relay_client: &R,
	relay_parent: RelayHash,
	para_id: ParaId,
) -> Option<(SessionIndex, PersistedValidationData)> {
	let session = match relay_client.session_index_for_child(relay_parent).await {
		Ok(s) => s,
		Err(err) => {
			tracing::warn!(
				target: LOG_TARGET,
				?relay_parent,
				?err,
				"Failed to fetch relay-parent session; skipping resubmission entry.",
			);
			return None;
		},
	};

	let pvd = match relay_client
		.persisted_validation_data(relay_parent, para_id, OccupiedCoreAssumption::TimedOut)
		.await
	{
		Ok(Some(pvd)) => pvd,
		Ok(None) => {
			tracing::warn!(
				target: LOG_TARGET,
				?relay_parent,
				"No persisted validation data (TimedOut); skipping resubmission entry.",
			);
			return None;
		},
		Err(err) => {
			tracing::warn!(
				target: LOG_TARGET,
				?relay_parent,
				?err,
				"Failed to fetch persisted validation data; skipping resubmission entry.",
			);
			return None;
		},
	};

	Some((session, pvd))
}

/// Resolve the relay-parent header from a [`RelayBlockIdentifier`] found in a parablock's digest.
///
/// Production slot-based blocks carry the relay parent as [`RelayBlockIdentifier::ByStorageRoot`]
/// (via the relay-parent-storage-root digest), so we look the relay block up by number and confirm
/// its state root matches — guarding against resolving the wrong fork. [`RelayBlockIdentifier::
/// ByHash`] is resolved directly. Returns `None` on any error or mismatch.
pub(crate) async fn resolve_relay_parent<R: RelayChainInterface + ?Sized>(
	relay_client: &R,
	identifier: &RelayBlockIdentifier,
) -> Option<RelayHeader> {
	match identifier {
		RelayBlockIdentifier::ByHash(relay_parent) => {
			match relay_client.header(BlockId::Hash(*relay_parent)).await {
				Ok(Some(header)) => Some(header),
				Ok(None) => {
					tracing::debug!(
						target: LOG_TARGET,
						?relay_parent,
						"Relay parent header unavailable; skipping resubmission entry.",
					);
					None
				},
				Err(err) => {
					tracing::debug!(
						target: LOG_TARGET,
						?relay_parent,
						?err,
						"Failed to fetch relay parent header; skipping resubmission entry.",
					);
					None
				},
			}
		},
		RelayBlockIdentifier::ByStorageRoot { storage_root, block_number } => {
			let header = match relay_client.header(BlockId::Number(*block_number)).await {
				Ok(Some(header)) => header,
				Ok(None) => {
					tracing::debug!(
						target: LOG_TARGET,
						?block_number,
						"Relay header at number unavailable; skipping resubmission entry.",
					);
					return None;
				},
				Err(err) => {
					tracing::debug!(
						target: LOG_TARGET,
						?block_number,
						?err,
						"Failed to fetch relay header by number; skipping resubmission entry.",
					);
					return None;
				},
			};

			// The canonical relay block at this number must match the storage root recorded in the
			// digest, otherwise we resolved a different fork.
			if header.state_root != *storage_root {
				tracing::warn!(
					target: LOG_TARGET,
					?block_number,
					"Relay storage root mismatch; skipping resubmission entry.",
				);
				return None;
			}

			Some(header)
		},
	}
}

/// Backfill resubmission entries for blocks imported at the tip.
///
/// Receives imported `(block, storage proof)` pairs from the [`SlotBasedBlockImportHandle`] and
/// records a resubmission entry for each via [`backfill_resubmission_entry`]. All relay-chain
/// queries happen here, off the block-import path, so block import is never delayed by them.
pub(crate) async fn run_resubmission_backfill<Block, RClient, Client>(
	mut block_import_handle: SlotBasedBlockImportHandle<Block>,
	relay_client: RClient,
	para_client: Arc<Client>,
	para_id: ParaId,
) where
	Block: BlockT,
	RClient: RelayChainInterface,
	Client: AuxStore + HeaderBackend<Block>,
{
	loop {
		let (block, proof) = block_import_handle.next().await;
		backfill_resubmission_entry(&relay_client, &*para_client, para_id, block, proof).await;
	}
}

/// Record a resubmission entry for a single block imported at the tip (built by another collator).
///
/// Resolves the relay-chain data the entry needs (relay-parent header, session and persisted
/// validation data) and writes the entry to the aux store. All relay-chain queries happen here,
/// outside the block-import path, so block import is never delayed by them.
///
/// Resubmission only ever concerns unfinalized blocks, so blocks that have already been finalized
/// are skipped — they are no longer part of any unincluded segment, and their entries would be
/// pruned on finality anyway. Finality is re-checked after the relay-chain queries (which may take
/// a while) to avoid resurrecting an entry the finality pruner has just removed.
pub(crate) async fn backfill_resubmission_entry<Block, R, Client>(
	relay_client: &R,
	para_client: &Client,
	para_id: ParaId,
	block: Block,
	proof: StorageProof,
) where
	Block: BlockT,
	R: RelayChainInterface + ?Sized,
	Client: AuxStore + HeaderBackend<Block>,
{
	let header = block.header();
	let block_hash = header.hash();
	let number = *header.number();

	let Some(core_info) = CumulusDigestItem::find_core_info(header.digest()) else {
		tracing::trace!(target: LOG_TARGET, ?block_hash, "Imported block has no core info digest; skipping.");
		return;
	};
	let Some(relay_block_identifier) =
		CumulusDigestItem::find_relay_block_identifier(header.digest())
	else {
		tracing::trace!(target: LOG_TARGET, ?block_hash, "Imported block has no relay block identifier; skipping.");
		return;
	};

	let Some(relay_parent_header) =
		resolve_relay_parent(relay_client, &relay_block_identifier).await
	else {
		return;
	};
	let relay_parent = relay_parent_header.hash();

	let Some((relay_parent_session, persisted_validation_data)) =
		resolve_session_and_pvd(relay_client, relay_parent, para_id).await
	else {
		return;
	};

	if number <= para_client.info().finalized_number {
		return;
	}

	let pairs: Vec<_> = prepare_resubmission_aux_data::<Block>(
		block_hash,
		now_unix_ms(),
		Arc::new(proof),
		relay_parent_header,
		relay_parent_session,
		persisted_validation_data,
		core_info.selector,
	)
	.collect();
	let refs: Vec<_> = pairs.iter().map(|(k, v)| (k.as_slice(), v.as_slice())).collect();

	match para_client.insert_aux(&refs, &[]) {
		Ok(()) => tracing::trace!(
			target: LOG_TARGET,
			?block_hash,
			"Stored resubmission entry for imported block.",
		),
		Err(err) => tracing::warn!(
			target: LOG_TARGET,
			?block_hash,
			?err,
			"Failed to store resubmission entry for imported block.",
		),
	}
}
