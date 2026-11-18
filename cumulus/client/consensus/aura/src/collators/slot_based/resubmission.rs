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
//! [`resolve_session`] is shared by the build path (block_builder_task) and the resubmission
//! backfill task. [`run_resubmission_backfill`] is the task that records resubmission entries for
//! unfinalized imported blocks, doing the relay-chain queries off the block-import path.

use super::SlotBasedBlockImportHandle;
use cumulus_client_resubmission_store::{
	prepare_resubmission_aux_data, prune_finalized_entries, prune_missed_finalized_entries,
};
use cumulus_primitives_core::{
	relay_chain::{
		BlockId, BlockNumber as RelayBlockNumber, Hash as RelayHash, Header as RelayHeader,
		OccupiedCoreAssumption, SessionIndex,
	},
	CumulusDigestItem, ParaId, RelayBlockIdentifier,
};
use cumulus_relay_chain_interface::{RelayChainError, RelayChainInterface};
use futures::{FutureExt, StreamExt};
use sc_client_api::{backend::AuxStore, BlockchainEvents};
use sp_api::StorageProof;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use std::sync::Arc;

const LOG_TARGET: &str = "aura::resubmission";

#[derive(Debug, thiserror::Error)]
pub(crate) enum ResubmissionError {
	#[error("relay parent header unavailable for {0:?}")]
	HeaderUnavailable(RelayBlockIdentifier),

	#[error("relay storage root mismatch at number {block_number}")]
	StorageRootMismatch { block_number: RelayBlockNumber },

	#[error("failed to fetch relay parent header: {0}")]
	RelayParentHeader(RelayChainError),

	#[error("failed to fetch relay-parent session: {0}")]
	Session(RelayChainError),
}

/// Fetch the session index for the given relay parent.
pub(crate) async fn resolve_session<R: RelayChainInterface + ?Sized>(
	relay_client: &R,
	relay_parent: RelayHash,
) -> Result<SessionIndex, ResubmissionError> {
	relay_client
		.session_index_for_child(relay_parent)
		.await
		.map_err(ResubmissionError::Session)
}

/// Resolve the relay-parent header from a [`RelayBlockIdentifier`] found in a parablock's digest.
///
/// Production slot-based blocks carry the relay parent as [`RelayBlockIdentifier::ByStorageRoot`]
/// (via the relay-parent-storage-root digest), so we look the relay block up by number and confirm
/// its state root matches — guarding against resolving the wrong fork. [`RelayBlockIdentifier::
/// ByHash`] is resolved directly.
async fn resolve_relay_parent<R: RelayChainInterface + ?Sized>(
	relay_client: &R,
	identifier: &RelayBlockIdentifier,
) -> Result<RelayHeader, ResubmissionError> {
	match identifier {
		RelayBlockIdentifier::ByHash(relay_parent) => relay_client
			.header(BlockId::Hash(*relay_parent))
			.await
			.map_err(ResubmissionError::RelayParentHeader)?
			.ok_or_else(|| ResubmissionError::HeaderUnavailable(identifier.clone())),
		RelayBlockIdentifier::ByStorageRoot { storage_root, block_number } => {
			let header = relay_client
				.header(BlockId::Number(*block_number))
				.await
				.map_err(ResubmissionError::RelayParentHeader)?
				.ok_or_else(|| ResubmissionError::HeaderUnavailable(identifier.clone()))?;

			// The canonical relay block at this number must match the storage root recorded in the
			// digest, otherwise we resolved a different fork.
			if header.state_root != *storage_root {
				return Err(ResubmissionError::StorageRootMismatch { block_number: *block_number });
			}

			Ok(header)
		},
	}
}

/// Maintain the resubmission store for unfinalized imported blocks.
///
/// Receives imported `(block, storage proof)` pairs from the [`SlotBasedBlockImportHandle`] and
/// records a resubmission entry for each via [`backfill_resubmission_entry`], and prunes entries as
/// blocks are finalized.
pub(crate) async fn run_resubmission_backfill<Block, RClient, Client>(
	mut block_import_handle: SlotBasedBlockImportHandle<Block>,
	relay_client: RClient,
	para_client: Arc<Client>,
	para_id: ParaId,
) where
	Block: BlockT,
	RClient: RelayChainInterface,
	Client: AuxStore + HeaderBackend<Block> + BlockchainEvents<Block>,
{
	// Reclaim entries for blocks that were finalized without their prune being observed (e.g. while
	// the node was down). The notification stream below only covers finalizations from now on.
	if let Err(err) = prune_missed_finalized_entries::<Block, _>(&*para_client) {
		tracing::warn!(
			target: LOG_TARGET,
			?err,
			"Failed to prune missed finalized resubmission entries at startup.",
		);
	}

	let mut finality_notifications = para_client.finality_notification_stream();

	tracing::debug!(
		target: LOG_TARGET,
		"Resubmission backfill task started; awaiting imported blocks.",
	);

	loop {
		let import_fut = block_import_handle.next().fuse();
		let notification_fut = finality_notifications.next().fuse();
		futures::pin_mut!(import_fut, notification_fut);

		futures::select! {
			maybe_notification = notification_fut => {
				let Some(notification) = maybe_notification else {
					// The finality stream ended; nothing left to prune.
					break;
				};

				if let Err(err) = prune_finalized_entries(&*para_client, &notification) {
					tracing::warn!(
						target: LOG_TARGET,
						?err,
						"Failed to prune finalized resubmission entries.",
					);
				}
			},
			(block, proof) = import_fut => {
				tracing::trace!(
					target: LOG_TARGET,
					block_hash = ?block.header().hash(),
					number = ?block.header().number(),
					"Backfilling resubmission entry for imported block.",
				);
				backfill_resubmission_entry(
					&relay_client,
					&*para_client,
					block.header(),
					proof,
					para_id,
				)
				.await;
			},
		}
	}
}

/// Resolve the relay-chain data an entry needs and write it to the aux store.
///
/// Finalized blocks are skipped
async fn backfill_resubmission_entry<Block, R, Client>(
	relay_client: &R,
	para_client: &Client,
	header: &Block::Header,
	proof: Arc<StorageProof>,
	para_id: ParaId,
) where
	Block: BlockT,
	R: RelayChainInterface + ?Sized,
	Client: AuxStore + HeaderBackend<Block> + BlockchainEvents<Block>,
{
	let block_hash = header.hash();
	let number = *header.number();

	if number <= para_client.info().finalized_number {
		tracing::trace!(
			target: LOG_TARGET,
			?block_hash,
			?number,
			"Imported block is already finalized; no resubmission entry needed.",
		);
		return;
	}

	let Some(relay_block_identifier) =
		CumulusDigestItem::find_relay_block_identifier(header.digest())
	else {
		tracing::trace!(target: LOG_TARGET, ?block_hash, "Imported block has no relay block identifier; skipping.");
		return;
	};

	let mut relay_imports = None;
	let mut finality_notifications = None;
	let relay_parent_header = loop {
		if number <= para_client.info().finalized_number {
			return;
		}
		match resolve_relay_parent(relay_client, &relay_block_identifier).await {
			Ok(header) => break header,
			Err(ResubmissionError::HeaderUnavailable(_)) => {
				if relay_imports.is_none() {
					let imports = match relay_client.import_notification_stream().await {
						Ok(imports) => imports,
						Err(err) => {
							tracing::debug!(target: LOG_TARGET, ?block_hash, ?err, "Could not subscribe to relay imports; skipping resubmission entry.");
							return;
						},
					};
					relay_imports = Some(imports);
					finality_notifications = Some(para_client.finality_notification_stream());
					// Recheck after subscribing so a concurrent relay import cannot be missed.
					continue;
				}
			},
			Err(err) => {
				tracing::debug!(
					target: LOG_TARGET,
					?block_hash,
					?err,
					"Could not resolve relay parent; skipping resubmission entry.",
				);
				return;
			},
		}

		let imports = relay_imports.as_mut().expect("subscribed before waiting; qed");
		let finalized = finality_notifications.as_mut().expect("subscribed before waiting; qed");
		futures::select! {
			import = imports.next().fuse() => if import.is_none() { return },
			finality = finalized.next().fuse() => if finality.is_none() { return },
		}
	};
	let relay_parent = relay_parent_header.hash();

	let relay_parent_session = match resolve_session(relay_client, relay_parent).await {
		Ok(session) => session,
		Err(err) => {
			tracing::debug!(
				target: LOG_TARGET,
				?block_hash,
				?err,
				"Could not resolve relay-parent session; skipping resubmission entry.",
			);
			return;
		},
	};
	if number <= para_client.info().finalized_number {
		tracing::trace!(
			target: LOG_TARGET,
			?block_hash,
			?number,
			"Block finalized while resolving relay data; dropping resubmission entry.",
		);
		return;
	}

	// Capture `max_pov_size` for the stored entry, so resubmission can rebuild the block's PVD.
	let max_pov_size = match relay_client
		.persisted_validation_data(relay_parent, para_id, OccupiedCoreAssumption::TimedOut)
		.await
	{
		Ok(Some(pvd)) => pvd.max_pov_size,
		Ok(None) | Err(_) => {
			tracing::debug!(
				target: LOG_TARGET,
				?block_hash,
				"Could not fetch max_pov_size for relay parent; skipping resubmission entry.",
			);
			return;
		},
	};

	let pairs: Vec<_> = prepare_resubmission_aux_data::<Block>(
		block_hash,
		proof,
		relay_parent_header,
		relay_parent_session,
		max_pov_size,
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

#[cfg(test)]
mod tests {
	use super::{
		super::{tests::TestRelayClient, SlotBasedBlockImport},
		*,
	};
	use codec::Encode;
	use cumulus_client_resubmission_store::ResubmissionStore;
	use cumulus_primitives_core::{
		rpsr_digest::relay_parent_storage_root_item, BlockBundleInfo, CoreInfo,
	};
	use cumulus_test_client::{
		runtime::Block, BuildBlockBuilder, Client, TestClientBuilder, TestClientBuilderExt,
	};
	use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
	use futures::poll;
	use sc_client_api::backend::Finalizer;
	use sc_consensus::{
		BlockImport, BlockImportParams, ForkChoiceStrategy, StateAction, StorageChanges,
	};
	use sp_consensus::BlockOrigin;
	use std::collections::HashMap;

	fn relay() -> (TestRelayClient, RelayHeader) {
		let old = RelayHeader::new(
			63,
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		);
		let parent = RelayHeader::new(
			64,
			Default::default(),
			RelayHash::repeat_byte(1),
			old.hash(),
			Default::default(),
		);
		let client = TestRelayClient::new_with_best(
			HashMap::from([(old.hash(), old.clone()), (parent.hash(), parent.clone())]),
			old.hash(),
		);
		(client, parent)
	}

	#[tokio::test]
	async fn backfill_retains_proof_until_relay_header_arrives() {
		let client = Arc::new(TestClientBuilder::default().build());
		let (mut relay, parent) = relay();
		let header = <Block as BlockT>::Header::new(
			1,
			Default::default(),
			Default::default(),
			client.info().genesis_hash,
			sp_runtime::Digest {
				logs: vec![relay_parent_storage_root_item(parent.state_root, parent.number)],
			},
		);
		let proof = Arc::new(StorageProof::new(vec![vec![1, 2, 3]]));
		let store = ResubmissionStore::<Block, _>::new(client.clone());
		let recovery_relay = relay.clone();
		let backfill = backfill_resubmission_entry::<Block, _, _>(
			&recovery_relay,
			&*client,
			&header,
			proof.clone(),
			2000.into(),
		);
		futures::pin_mut!(backfill);
		assert!(poll!(&mut backfill).is_pending());
		assert!(store.load(header.hash()).expect("store lookup succeeds; qed").is_none());

		let mut observer = relay
			.import_notification_stream()
			.await
			.expect("second relay subscription succeeds; qed");
		relay.set_best_hash(Some(parent.hash()));
		relay.notify_import(parent.clone());
		assert_eq!(observer.next().now_or_never(), Some(Some(parent.clone())));
		backfill.await;
		let entry = store
			.load(header.hash())
			.expect("store lookup succeeds; qed")
			.expect("proof was retained; qed");
		assert_eq!(entry.proof, proof);
		assert_eq!(entry.relay_parent_header, parent);
		assert_eq!(entry.relay_parent_session, 1);
		assert_eq!(entry.max_pov_size, 1024 * 1024);
	}

	#[tokio::test]
	async fn backfill_rejects_a_relay_header_from_another_fork() {
		let client = Arc::new(TestClientBuilder::default().build());
		let (mut relay, parent) = relay();
		relay.set_best_hash(Some(parent.hash()));
		let header = <Block as BlockT>::Header::new(
			1,
			Default::default(),
			Default::default(),
			client.info().genesis_hash,
			sp_runtime::Digest {
				logs: vec![relay_parent_storage_root_item(
					RelayHash::repeat_byte(2),
					parent.number,
				)],
			},
		);
		backfill_resubmission_entry::<Block, _, _>(
			&relay,
			&*client,
			&header,
			Arc::new(StorageProof::empty()),
			2000.into(),
		)
		.await;
		assert!(ResubmissionStore::<Block, _>::new(client)
			.load(header.hash())
			.expect("store lookup succeeds; qed")
			.is_none());
	}

	async fn imported_block(client: &Client, relay_number: RelayBlockNumber) -> Block {
		let mut sproof = RelayStateSproofBuilder::default();
		sproof.para_id = cumulus_test_client::runtime::PARACHAIN_ID.into();
		sproof.included_para_head = Some(
			client
				.header(client.info().best_hash)
				.expect("header lookup succeeds; qed")
				.expect("best block exists; qed")
				.encode()
				.into(),
		);
		let built = client
			.init_block_builder_builder()
			.with_relay_sproof_builder(sproof)
			.with_validation_data(cumulus_primitives_core::PersistedValidationData {
				relay_parent_number: relay_number,
				..Default::default()
			})
			.with_pre_digests(vec![
				CumulusDigestItem::CoreInfo(CoreInfo {
					selector: 0.into(),
					claim_queue_offset: 0.into(),
					number_of_cores: 1.into(),
				})
				.to_digest_item(),
				CumulusDigestItem::BlockBundleInfo(BlockBundleInfo { index: 0, is_last: true })
					.to_digest_item(),
			])
			.build()
			.block_builder
			.build()
			.expect("test block builds; qed");
		let block = built.block;
		let (header, body) = block.clone().deconstruct();
		let mut params = BlockImportParams::new(BlockOrigin::Own, header.clone());
		params.body = Some(body);
		params.fork_choice = Some(ForkChoiceStrategy::LongestChain);
		params.state_action =
			StateAction::ApplyChanges(StorageChanges::Changes(built.storage_changes));
		client.import_block(params).await.expect("test block imports; qed");
		block
	}

	#[tokio::test]
	async fn missing_relay_header_bounds_catch_up_and_imports_resume() {
		let author = TestClientBuilder::default().build();
		let client = Arc::new(TestClientBuilder::default().build());
		let mut parents = vec![RelayHeader::new(
			0,
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		)];
		let mut blocks = Vec::new();
		for number in 1..=3 {
			let block = imported_block(&author, number).await;
			let Some(RelayBlockIdentifier::ByStorageRoot { storage_root, block_number }) =
				CumulusDigestItem::find_relay_block_identifier(block.header.digest())
			else {
				panic!("test runtime emits the relay storage root; qed");
			};
			assert_eq!(block_number, number);
			parents.push(RelayHeader::new(
				number,
				Default::default(),
				storage_root,
				parents.last().expect("genesis is present; qed").hash(),
				Default::default(),
			));
			blocks.push(block);
		}
		let mut relay = TestRelayClient::new_with_best(
			parents.iter().map(|header| (header.hash(), header.clone())).collect(),
			parents[0].hash(),
		);
		let params = |block: Block| {
			let (header, body) = block.deconstruct();
			let mut params = BlockImportParams::new(BlockOrigin::NetworkInitialSync, header);
			params.body = Some(body);
			params.fork_choice = Some(ForkChoiceStrategy::LongestChain);
			params
		};
		let (importer, handle) = SlotBasedBlockImport::new(client.clone(), client.clone());
		let store = ResubmissionStore::<Block, _>::new(client.clone());
		let backfill = run_resubmission_backfill(
			handle,
			relay.clone(),
			client.clone(),
			cumulus_test_client::runtime::PARACHAIN_ID.into(),
		);
		futures::pin_mut!(backfill);

		importer
			.import_block(params(blocks[0].clone()))
			.await
			.expect("first import succeeds; qed");
		assert!(poll!(&mut backfill).is_pending());
		importer
			.import_block(params(blocks[1].clone()))
			.await
			.expect("second import succeeds; qed");
		assert_eq!(client.info().best_number, 2);

		// An invalid state root fails at execution, so a pending import shows it is still
		// waiting for capacity.
		let mut invalid = params(blocks[2].clone());
		invalid.header.set_state_root(Default::default());
		let blocked = importer.import_block(invalid);
		futures::pin_mut!(blocked);
		for _ in 0..3 {
			relay.notify_import(parents[0].clone());
			assert!(poll!(&mut backfill).is_pending());
			assert!(poll!(&mut blocked).is_pending());
			assert_eq!(client.info().best_number, 2);
			for block in &blocks {
				assert!(store
					.load(block.header.hash())
					.expect("store lookup succeeds; qed")
					.is_none());
			}
		}

		relay.set_best_hash(Some(parents[3].hash()));
		relay.notify_import(parents[3].clone());
		assert!(poll!(&mut backfill).is_pending());
		for block in &blocks[..2] {
			assert!(store.load(block.header.hash()).expect("store lookup succeeds; qed").is_some());
		}
		assert!(blocked.await.is_err());
		assert_eq!(client.info().best_number, 2);

		importer
			.import_block(params(blocks[2].clone()))
			.await
			.expect("catch-up resumes; qed");
		assert!(poll!(&mut backfill).is_pending());
		assert_eq!(client.info().best_number, 3);
		for (block, parent) in blocks.iter().zip(&parents[1..]) {
			let entry = store
				.load(block.header.hash())
				.expect("store lookup succeeds; qed")
				.expect("imported proof was retained; qed");
			assert!(entry.proof.iter_nodes().next().is_some());
			assert_eq!(&entry.relay_parent_header, parent);
		}
	}

	#[tokio::test]
	async fn finality_releases_a_proof_waiting_for_its_relay_header() {
		let client = Arc::new(TestClientBuilder::default().build());
		let (relay, parent) = relay();
		let header = imported_block(&client, parent.number).await.header;
		let backfill = backfill_resubmission_entry::<Block, _, _>(
			&relay,
			&*client,
			&header,
			Arc::new(StorageProof::empty()),
			2000.into(),
		);
		futures::pin_mut!(backfill);
		assert!(poll!(&mut backfill).is_pending());
		client
			.finalize_block(header.hash(), None, true)
			.expect("test block finalizes; qed");
		backfill.await;
		assert!(ResubmissionStore::<Block, _>::new(client.clone())
			.load(header.hash())
			.expect("store lookup succeeds; qed")
			.is_none());
	}
}
