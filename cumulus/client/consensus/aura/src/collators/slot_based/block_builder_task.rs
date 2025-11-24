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

use super::CollatorMessage;
use crate::{
	collator::{self as collator_util, BuildBlockAndImportParams, Collator, SlotClaim},
	collators::{
		check_validation_code_or_log,
		slot_based::{
			relay_chain_data_cache::RelayChainDataCache,
			slot_timer::{SlotInfo, SlotTimer},
		},
		BackingGroupConnectionHelper, RelayParentData,
	},
	LOG_TARGET,
};
use codec::{Codec, Encode};
use cumulus_client_collator::service::ServiceInterface as CollatorServiceInterface;
use cumulus_client_consensus_common::{self as consensus_common, ParachainBlockImportMarker};
use cumulus_client_proof_size_recording::prepare_proof_size_recording_transaction;
use cumulus_primitives_aura::{AuraUnincludedSegmentApi, Slot};
use cumulus_primitives_core::{
	extract_relay_parent, rpsr_digest, BundleInfo, ClaimQueueOffset, CoreInfo, CoreSelector,
	CumulusDigestItem, PersistedValidationData, RelayParentOffsetApi, TargetBlockRate,
};
use cumulus_relay_chain_interface::RelayChainInterface;
use futures::prelude::*;
use polkadot_primitives::{
	Block as RelayBlock, CoreIndex, Hash as RelayHash, Header as RelayHeader, Id as ParaId,
	DEFAULT_CLAIM_QUEUE_OFFSET,
};
use sc_client_api::{backend::AuxStore, BlockBackend, BlockOf, UsageProvider};
use sc_consensus::BlockImport;
use sc_consensus_aura::SlotDuration;
use sc_network_types::PeerId;
use sp_api::{ProofRecorder, ProvideRuntimeApi, StorageProof};
use sp_application_crypto::AppPublic;
use sp_block_builder::BlockBuilder;
use sp_blockchain::HeaderBackend;
use sp_consensus::Environment;
use sp_consensus_aura::AuraApi;
use sp_core::crypto::Pair;
use sp_externalities::Extensions;
use sp_inherents::CreateInherentDataProviders;
use sp_keystore::KeystorePtr;
use sp_runtime::traits::{Block as BlockT, HashingFor, Header as HeaderT, Member, Zero};
use sp_trie::{
	proof_size_extension::{ProofSizeExt, RecordingProofSizeProvider},
	recorder::IgnoredNodes,
};
use std::{
	collections::VecDeque,
	sync::Arc,
	time::{Duration, Instant},
};

/// Parameters for [`run_block_builder`].
pub struct BuilderTaskParams<
	Block: BlockT,
	BI,
	CIDP,
	Client,
	Backend,
	RelayClient,
	CHP,
	Proposer,
	CS,
> {
	/// Inherent data providers. Only non-consensus inherent data should be provided, i.e.
	/// the timestamp, slot, and paras inherents should be omitted, as they are set by this
	/// collator.
	pub create_inherent_data_providers: CIDP,
	/// Used to actually import blocks.
	pub block_import: BI,
	/// The underlying para client.
	pub para_client: Arc<Client>,
	/// The para client's backend, used to access the database.
	pub para_backend: Arc<Backend>,
	/// A handle to the relay-chain client.
	pub relay_client: RelayClient,
	/// A validation code hash provider, used to get the current validation code hash.
	pub code_hash_provider: CHP,
	/// The underlying keystore, which should contain Aura consensus keys.
	pub keystore: KeystorePtr,
	/// The collator network peer id.
	pub collator_peer_id: PeerId,
	/// The para's ID.
	pub para_id: ParaId,
	/// The proposer for building blocks.
	pub proposer: Proposer,
	/// The generic collator service used to plug into this consensus engine.
	pub collator_service: CS,
	/// Channel to send built blocks to the collation task.
	pub collator_sender: sc_utils::mpsc::TracingUnboundedSender<CollatorMessage<Block>>,
	/// Slot duration of the relay chain.
	pub relay_chain_slot_duration: Duration,
	/// Offset all time operations by this duration.
	///
	/// This is a time quantity that is subtracted from the actual timestamp when computing
	/// the time left to enter a new slot. In practice, this *left-shifts* the clock time with the
	/// intent to keep our "clock" slightly behind the relay chain one and thus reducing the
	/// likelihood of encountering unfavorable notification arrival timings (i.e. we don't want to
	/// wait for relay chain notifications because we woke up too early).
	pub slot_offset: Duration,
	/// The maximum percentage of the maximum PoV size that the collator can use.
	/// It will be removed once https://github.com/paritytech/polkadot-sdk/issues/6020 is fixed.
	pub max_pov_percentage: Option<u32>,
}

/// Run block-builder.
pub fn run_block_builder<Block, P, BI, CIDP, Client, Backend, RelayClient, CHP, Proposer, CS>(
	params: BuilderTaskParams<Block, BI, CIDP, Client, Backend, RelayClient, CHP, Proposer, CS>,
) -> impl Future<Output = ()> + Send + 'static
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block>
		+ UsageProvider<Block>
		+ BlockOf
		+ AuxStore
		+ HeaderBackend<Block>
		+ BlockBackend<Block>
		+ Send
		+ Sync
		+ 'static,
	Client::Api: AuraApi<Block, P::Public>
		+ RelayParentOffsetApi<Block>
		+ AuraUnincludedSegmentApi<Block>
		+ TargetBlockRate<Block>
		+ BlockBuilder<Block>,
	Backend: sc_client_api::Backend<Block> + 'static,
	RelayClient: RelayChainInterface + Clone + 'static,
	CIDP: CreateInherentDataProviders<Block, ()> + 'static,
	CIDP::InherentDataProviders: Send,
	BI: BlockImport<Block> + ParachainBlockImportMarker + Send + Sync + 'static,
	Proposer: Environment<Block> + Send + Sync + 'static,
	CS: CollatorServiceInterface<Block> + Send + Sync + 'static,
	CHP: consensus_common::ValidationCodeHashProvider<Block::Hash> + Send + Sync + 'static,
	P: Pair + Send + Sync + 'static,
	P::Public: AppPublic + Member + Codec,
	P::Signature: TryFrom<Vec<u8>> + Member + Codec,
{
	async move {
		tracing::info!(target: LOG_TARGET, "Starting slot-based block-builder task.");
		let BuilderTaskParams {
			relay_client,
			create_inherent_data_providers,
			para_client,
			keystore,
			block_import,
			collator_peer_id,
			para_id,
			proposer,
			collator_service,
			collator_sender,
			code_hash_provider,
			relay_chain_slot_duration,
			para_backend,
			slot_offset,
			max_pov_percentage,
		} = params;

		let mut slot_timer = SlotTimer::new_with_offset(slot_offset, relay_chain_slot_duration);

		let mut collator = {
			let params = collator_util::Params {
				create_inherent_data_providers,
				block_import,
				relay_client: relay_client.clone(),
				keystore: keystore.clone(),
				collator_peer_id,
				para_id,
				proposer,
				collator_service,
			};

			collator_util::Collator::<Block, P, _, _, _, _, _>::new(params)
		};

		let mut relay_chain_data_cache = RelayChainDataCache::new(relay_client.clone(), para_id);
		let mut connection_helper = BackingGroupConnectionHelper::new(
			keystore.clone(),
			relay_client
				.overseer_handle()
				// Should never fail. If it fails, then providing collations to relay chain
				// doesn't work either. So it is fine to panic here.
				.expect("Relay chain interface must provide overseer handle."),
		);

		loop {
			// We wait here until the next slot arrives.
			let Ok(slot_time) = slot_timer.wait_until_next_slot().await else {
				tracing::error!(target: LOG_TARGET, "Unable to wait for next slot.");
				return;
			};

			let Ok(relay_best_hash) = relay_client.best_block_hash().await else {
				tracing::warn!(target: crate::LOG_TARGET, "Unable to fetch latest relay chain block hash.");
				continue
			};

			let best_hash = para_client.info().best_hash;
			let relay_parent_offset =
				para_client.runtime_api().relay_parent_offset(best_hash).unwrap_or_default();

			let Ok(para_slot_duration) = crate::slot_duration(&*para_client) else {
				tracing::error!(target: LOG_TARGET, "Failed to fetch slot duration from runtime.");
				continue;
			};

			let Ok(Some(rp_data)) = offset_relay_parent_find_descendants(
				&mut relay_chain_data_cache,
				relay_best_hash,
				relay_parent_offset,
			)
			.await
			else {
				continue
			};

			let Some(para_slot) = adjust_para_to_relay_parent_slot(
				rp_data.relay_parent(),
				relay_chain_slot_duration,
				para_slot_duration,
			) else {
				continue;
			};

			// Use the slot calculated from relay parent
			let slot_info = para_slot;

			let relay_parent = rp_data.relay_parent().hash();
			let relay_parent_header = rp_data.relay_parent().clone();

			let Some((included_header, initial_parent)) =
				crate::collators::find_parent(relay_parent, para_id, &*para_backend, &relay_client)
					.await
			else {
				continue
			};

			let Ok(max_pov_size) = relay_chain_data_cache
				.get_mut_relay_chain_data(relay_parent)
				.await
				.map(|d| d.max_pov_size)
			else {
				continue;
			};

			let allowed_pov_size = if let Some(max_pov_percentage) = max_pov_percentage {
				max_pov_size * max_pov_percentage / 100
			} else {
				// Set the block limit to 85% of the maximum PoV size.
				//
				// Once https://github.com/paritytech/polkadot-sdk/issues/6020 issue is
				// fixed, this should be removed.
				max_pov_size * 85 / 100
			} as usize;

			// We mainly call this to inform users at genesis if there is a mismatch with the
			// on-chain data.
			collator
				.collator_service()
				.check_block_status(initial_parent.hash, &initial_parent.header);

			let Ok(relay_slot) =
				sc_consensus_babe::find_pre_digest::<RelayBlock>(&relay_parent_header)
					.map(|babe_pre_digest| babe_pre_digest.slot())
			else {
				tracing::error!(target: crate::LOG_TARGET, "Relay chain does not contain babe slot. This should never happen.");
				continue;
			};

			let included_header_hash = included_header.hash();

			if let Ok(authorities) = para_client.runtime_api().authorities(initial_parent.hash) {
				connection_helper.update::<P>(slot_info.slot, &authorities).await;
			}

			let Some(slot_claim) = crate::collators::can_build_upon::<_, _, P>(
				slot_info.slot,
				relay_slot,
				slot_info.timestamp,
				initial_parent.hash,
				included_header_hash,
				&*para_client,
				&keystore,
			)
			.await
			else {
				tracing::debug!(
					target: crate::LOG_TARGET,
					unincluded_segment_len = initial_parent.depth,
					relay_parent = ?relay_parent,
					relay_parent_num = %relay_parent_header.number(),
					included_hash = ?included_header_hash,
					included_num = %included_header.number(),
					initial_parent = ?initial_parent.hash,
					slot = ?slot_info.slot,
					"Not eligible to claim slot."
				);
				continue
			};

			tracing::debug!(
				target: crate::LOG_TARGET,
				unincluded_segment_len = initial_parent.depth,
				relay_parent = ?relay_parent,
				relay_parent_num = %relay_parent_header.number(),
				relay_parent_offset,
				included_hash = ?included_header_hash,
				included_num = %included_header.number(),
				initial_parent = ?initial_parent.hash,
				slot = ?slot_info.slot,
				"Claiming slot."
			);

			let mut cores = match determine_cores(
				&mut relay_chain_data_cache,
				&relay_parent_header,
				para_id,
				relay_parent_offset,
			)
			.await
			{
				Ok(Some(core)) => core,
				Ok(None) => {
					tracing::debug!(
						target: crate::LOG_TARGET,
						relay_parent = ?relay_parent,
						"No cores scheduled."
					);
					continue;
				},
				Err(()) => {
					tracing::error!(
						target: crate::LOG_TARGET,
						relay_parent = ?relay_parent,
						"Failed to determine cores."
					);

					break;
				},
			};

			let number_of_blocks =
				match para_client.runtime_api().target_block_rate(initial_parent.hash) {
					Ok(interval) => interval,
					Err(error) => {
						tracing::debug!(
							target: crate::LOG_TARGET,
							block = ?initial_parent.hash,
							?error,
							"Failed to fetch `slot_schedule`, assuming one block with 2s"
						);
						1
					},
				};

			let blocks_per_core = (number_of_blocks / cores.total_cores()).max(1);

			tracing::debug!(
				target: crate::LOG_TARGET,
				%blocks_per_core,
				core_indices = ?cores.core_indices(),
				"Core configuration",
			);

			let mut pov_parent_header = initial_parent.header;
			let mut pov_parent_hash = initial_parent.hash;
			let block_time = Duration::from_secs(6) / number_of_blocks;

			loop {
				let time_for_core = slot_time.time_left() / cores.cores_left();

				match build_collation_for_core(
					pov_parent_header,
					pov_parent_hash,
					&relay_parent_header,
					relay_parent,
					max_pov_size,
					para_id,
					&relay_client,
					&code_hash_provider,
					&slot_claim,
					&collator_sender,
					&mut collator,
					allowed_pov_size,
					cores.core_info(),
					cores.core_index(),
					block_time,
					blocks_per_core,
					time_for_core,
					cores.is_last_core() &&
						slot_time.is_parachain_slot_ending(para_slot_duration.as_duration()),
					collator_peer_id,
					rp_data.clone(),
				)
				.await
				{
					Ok(Some(header)) => {
						pov_parent_header = header;
						pov_parent_hash = pov_parent_header.hash();
					},
					// Let's wait for the next slot
					Ok(None) => break,
					Err(()) => return,
				}

				if !cores.advance() {
					break
				}
			}
		}
	}
}

/// Build a collation for one core.
///
/// One collation can be composed of multiple blocks.
async fn build_collation_for_core<Block: BlockT, P, RelayClient, BI, CIDP, Proposer, CS>(
	pov_parent_header: Block::Header,
	pov_parent_hash: Block::Hash,
	relay_parent_header: &RelayHeader,
	relay_parent_hash: RelayHash,
	max_pov_size: u32,
	para_id: ParaId,
	relay_client: &impl RelayChainInterface,
	code_hash_provider: &impl consensus_common::ValidationCodeHashProvider<Block::Hash>,
	slot_claim: &SlotClaim<P::Public>,
	collator_sender: &sc_utils::mpsc::TracingUnboundedSender<CollatorMessage<Block>>,
	collator: &mut Collator<Block, P, BI, CIDP, RelayClient, Proposer, CS>,
	allowed_pov_size: usize,
	core_info: CoreInfo,
	core_index: CoreIndex,
	block_time: Duration,
	blocks_per_core: u32,
	slot_time_for_core: Duration,
	is_last_core_in_parachain_slot: bool,
	collator_peer_id: PeerId,
	relay_parent_data: RelayParentData,
) -> Result<Option<Block::Header>, ()>
where
	RelayClient: RelayChainInterface + 'static,
	P: Pair,
	P::Public: AppPublic + Member + Codec,
	P::Signature: TryFrom<Vec<u8>> + Member + Codec,
	CIDP: CreateInherentDataProviders<Block, ()> + 'static,
	CIDP::InherentDataProviders: Send,
	BI: BlockImport<Block> + ParachainBlockImportMarker + Send + Sync + 'static,
	Proposer: Environment<Block> + Send + Sync + 'static,
	CS: CollatorServiceInterface<Block> + Send + Sync + 'static,
{
	let core_start = Instant::now();

	let validation_data = PersistedValidationData {
		parent_head: pov_parent_header.encode().into(),
		relay_parent_number: *relay_parent_header.number(),
		relay_parent_storage_root: *relay_parent_header.state_root(),
		max_pov_size,
	};

	let Some(validation_code_hash) = code_hash_provider.code_hash_at(pov_parent_hash) else {
		tracing::error!(
			target: crate::LOG_TARGET,
			?pov_parent_hash,
			"Could not fetch validation code hash",
		);

		return Err(())
	};

	check_validation_code_or_log(&validation_code_hash, para_id, relay_client, relay_parent_hash)
		.await;

	let mut blocks = Vec::new();
	let mut proofs = Vec::new();
	let mut ignored_nodes = IgnoredNodes::default();

	let mut parent_hash = pov_parent_hash;
	let mut parent_header = pov_parent_header.clone();

	for block_index in 0..blocks_per_core {
		//TODO: Remove when transaction streaming is implemented
		// We require that the next node has imported our last block before it can start building
		// the next block. To ensure that the next node is able to do so, we are skipping the last
		// block in the parachain slot. In the future this can be removed again.
		let is_last = block_index + 1 == blocks_per_core ||
			(block_index + 2 == blocks_per_core &&
				blocks_per_core > 1 &&
				is_last_core_in_parachain_slot);
		if block_index + 1 == blocks_per_core &&
			blocks_per_core > 1 &&
			is_last_core_in_parachain_slot
		{
			tracing::debug!(
				target: LOG_TARGET,
				"Skipping block production so that the next node is able to import all blocks before its slot."
			);
			break;
		}

		let block_start = Instant::now();
		let slot_time_for_block = slot_time_for_core.saturating_sub(core_start.elapsed()) /
			(blocks_per_core - block_index) as u32;

		if slot_time_for_block <= Duration::from_millis(20) {
			tracing::error!(
				target: LOG_TARGET,
				slot_time_for_block_ms = %slot_time_for_block.as_millis(),
				blocks_left = %(blocks_per_core - block_index),
				?core_index,
				"Less than 20ms slot time left to produce blocks, stopping block production for core",
			);

			break
		}

		tracing::trace!(
			target: LOG_TARGET,
			slot_time_for_block_ms = %slot_time_for_block.as_millis(),
			%block_index,
			core_index = %core_index.0,
			"Going to build block"
		);

		// The authoring duration is either the block time returned by the runtime or the 90% of the
		// rest of the slot time for the block. We take here 90% because we still need to create the
		// inherents and need to import the block afterward.
		let authoring_duration = block_time.min(slot_time_for_block);

		let (parachain_inherent_data, other_inherent_data) = match collator
			.create_inherent_data_with_rp_offset(
				relay_parent_hash,
				&validation_data,
				parent_hash,
				slot_claim.timestamp(),
				Some(relay_parent_data.clone()),
				collator_peer_id,
			)
			.await
		{
			Err(err) => {
				tracing::error!(target: crate::LOG_TARGET, ?err, "Failed to create inherent data.");
				return Ok(None)
			},
			Ok(x) => x,
		};

		let storage_proof_recorder =
			ProofRecorder::<Block>::with_ignored_nodes(ignored_nodes.clone());

		let proof_size_recorder = RecordingProofSizeProvider::new(storage_proof_recorder.clone());

		let mut extra_extensions = Extensions::default();
		extra_extensions.register(ProofSizeExt::new(proof_size_recorder.clone()));

		let Ok(Some((built_block, mut import_block))) = collator
			.build_block(BuildBlockAndImportParams {
				parent_header: &parent_header,
				slot_claim,
				additional_pre_digest: vec![
					CumulusDigestItem::CoreInfo(core_info.clone()).to_digest_item(),
					CumulusDigestItem::BundleInfo(BundleInfo {
						index: block_index as u8,
						maybe_last: is_last,
					})
					.to_digest_item(),
				],
				parachain_inherent_data,
				extra_inherent_data: other_inherent_data,
				proposal_duration: authoring_duration,
				max_pov_size: allowed_pov_size,
				storage_proof_recorder: storage_proof_recorder.into(),
				extra_extensions,
			})
			.await
		else {
			tracing::error!(target: crate::LOG_TARGET, "Unable to build block at slot.");
			return Ok(None);
		};

		parent_hash = built_block.block.header().hash();
		parent_header = built_block.block.header().clone();

		// Extract and add proof size recordings to the import block
		let recorded_sizes = proof_size_recorder
			.recorded_estimations()
			.into_iter()
			.map(|size| size as u32)
			.collect::<Vec<u32>>();

		if !recorded_sizes.is_empty() {
			prepare_proof_size_recording_transaction(parent_hash, recorded_sizes).for_each(
				|(k, v)| {
					import_block.auxiliary.push((k, Some(v)));
				},
			);
		}

		if let Err(error) = collator.import_block(import_block).await {
			tracing::error!(target: crate::LOG_TARGET, ?error, "Failed to import built block.");
			return Ok(None);
		}

		// Announce the newly built block to our peers.
		collator.collator_service().announce_block(parent_hash, None);

		blocks.push(built_block.block);
		proofs.push(built_block.proof);

		let full_core_digest = CumulusDigestItem::contains_use_full_core(parent_header.digest());
		let runtime_upgrade_digest = parent_header
			.digest()
			.logs
			.iter()
			.any(|it| matches!(it, sp_runtime::DigestItem::RuntimeEnvironmentUpdated));

		if full_core_digest || runtime_upgrade_digest {
			tracing::trace!(
				target: crate::LOG_TARGET,
				block_hash = ?parent_hash,
				time_used_by_block_in_secs = %block_start.elapsed().as_secs_f32(),
				%full_core_digest,
				%runtime_upgrade_digest,
				"Stopping block production for core",
			);
			break
		}

		ignored_nodes.extend(IgnoredNodes::from_storage_proof::<HashingFor<Block>>(
			proofs.last().expect("We just pushed the proof into the vector; qed"),
		));
		ignored_nodes.extend(IgnoredNodes::from_memory_db(built_block.backend_transaction));

		// If there is still time left for the block in the slot, we sleep the rest of the time.
		// This ensures that we have some steady block rate.
		if let Some(sleep) = slot_time_for_block
			.checked_sub(block_start.elapsed())
			// Let's not sleep for the last block here, to send out the collation as early as
			// possible.
			.filter(|_| block_index + 1 < blocks_per_core)
		{
			tokio::time::sleep(sleep).await;
		}
	}

	let proof = StorageProof::merge(proofs);

	if let Err(err) = collator_sender.unbounded_send(CollatorMessage {
		relay_parent: relay_parent_hash,
		parent_header: pov_parent_header.clone(),
		blocks,
		proof,
		validation_code_hash,
		core_index,
		max_pov_size: validation_data.max_pov_size,
	}) {
		tracing::error!(target: crate::LOG_TARGET, ?err, "Unable to send block to collation task.");
		Err(())
	} else {
		// Now let's sleep for the rest of the core.
		if let Some(sleep) = slot_time_for_core.checked_sub(core_start.elapsed()) {
			tokio::time::sleep(sleep).await;
		}

		Ok(Some(parent_header))
	}
}

/// Translate the slot of the relay parent to the slot of the parachain.
fn adjust_para_to_relay_parent_slot(
	relay_header: &RelayHeader,
	relay_chain_slot_duration: Duration,
	para_slot_duration: SlotDuration,
) -> Option<SlotInfo> {
	let relay_slot = sc_consensus_babe::find_pre_digest::<RelayBlock>(&relay_header)
		.map(|babe_pre_digest| babe_pre_digest.slot())
		.ok()?;
	let new_slot = Slot::from_timestamp(
		relay_slot
			.timestamp(SlotDuration::from_millis(relay_chain_slot_duration.as_millis() as u64))?,
		para_slot_duration,
	);
	let para_slot = SlotInfo { slot: new_slot, timestamp: new_slot.timestamp(para_slot_duration)? };
	tracing::debug!(
		target: LOG_TARGET,
		timestamp = ?para_slot.timestamp,
		slot = ?para_slot.slot,
		"Parachain slot adjusted to relay chain.",
	);
	Some(para_slot)
}

/// Finds a relay chain parent block at a specified offset from the best block, collecting its
/// descendants.
///
/// # Returns
/// * `Ok(RelayParentData)` - Contains the target relay parent and its ordered list of descendants
/// * `Err(())` - If any relay chain block header cannot be retrieved
///
/// The function traverses backwards from the best block until it finds the block at the specified
/// offset, collecting all blocks in between to maintain the chain of ancestry.
pub async fn offset_relay_parent_find_descendants<RelayClient>(
	relay_chain_data_cache: &mut RelayChainDataCache<RelayClient>,
	relay_best_block: RelayHash,
	relay_parent_offset: u32,
) -> Result<Option<RelayParentData>, ()>
where
	RelayClient: RelayChainInterface + Clone + 'static,
{
	let Ok(mut relay_header) = relay_chain_data_cache
		.get_mut_relay_chain_data(relay_best_block)
		.await
		.map(|d| d.relay_parent_header.clone())
	else {
		tracing::error!(target: LOG_TARGET, ?relay_best_block, "Unable to fetch best relay chain block header.");
		return Err(())
	};

	if relay_parent_offset == 0 {
		return Ok(Some(RelayParentData::new(relay_header)));
	}

	if sc_consensus_babe::contains_epoch_change::<RelayBlock>(&relay_header) {
		tracing::debug!(
			target: LOG_TARGET,
			?relay_best_block,
			relay_best_block_number = relay_header.number(),
			"Relay parent is in previous session.",
		);
		return Ok(None);
	}

	let mut required_ancestors: VecDeque<RelayHeader> = Default::default();
	required_ancestors.push_front(relay_header.clone());
	while required_ancestors.len() < relay_parent_offset as usize {
		let next_header = relay_chain_data_cache
			.get_mut_relay_chain_data(*relay_header.parent_hash())
			.await?
			.relay_parent_header
			.clone();
		if sc_consensus_babe::contains_epoch_change::<RelayBlock>(&next_header) {
			tracing::debug!(
				target: LOG_TARGET,
				?relay_best_block, ancestor = %next_header.hash(),
				ancestor_block_number = next_header.number(),
				"Ancestor of best block is in previous session.",
			);

			return Ok(None);
		}
		required_ancestors.push_front(next_header.clone());
		relay_header = next_header;
	}

	let relay_parent = relay_chain_data_cache
		.get_mut_relay_chain_data(*relay_header.parent_hash())
		.await?
		.relay_parent_header
		.clone();

	tracing::debug!(
		target: LOG_TARGET,
		relay_parent_hash = %relay_parent.hash(),
		relay_parent_num = relay_parent.number(),
		num_descendants = required_ancestors.len(),
		"Relay parent descendants."
	);

	Ok(Some(RelayParentData::new_with_descendants(relay_parent, required_ancestors.into())))
}

/// Return value of [`determine_cores`].
pub struct Cores {
	selector: CoreSelector,
	claim_queue_offset: ClaimQueueOffset,
	core_indices: Vec<CoreIndex>,
}

impl Cores {
	/// Returns the current [`CoreInfo`].
	pub fn core_info(&self) -> CoreInfo {
		CoreInfo {
			selector: self.selector,
			claim_queue_offset: self.claim_queue_offset,
			number_of_cores: (self.core_indices.len() as u16).into(),
		}
	}

	/// Returns the core indices.
	fn core_indices(&self) -> &[CoreIndex] {
		&self.core_indices
	}

	/// Returns the current [`CoreIndex`].
	pub fn core_index(&self) -> CoreIndex {
		self.core_indices[self.selector.0 as usize]
	}

	/// Advance to the next available core.
	///
	/// Returns `false` if there is no core left.
	fn advance(&mut self) -> bool {
		if self.selector.0 as usize + 1 < self.core_indices.len() {
			self.selector.0 += 1;
			true
		} else {
			false
		}
	}

	/// Returns the total number of cores.
	pub fn total_cores(&self) -> u32 {
		self.core_indices.len() as u32
	}

	/// Returns the number of cores left.
	fn cores_left(&self) -> u32 {
		self.total_cores() - self.selector.0 as u32
	}

	/// Returns if the current core is the last core.
	fn is_last_core(&self) -> bool {
		self.cores_left() == 1
	}
}

/// Determine the cores for the given `para_id`.
///
/// Takes into account the `parent` core to find the next available cores.
pub async fn determine_cores<RI: RelayChainInterface + 'static>(
	relay_chain_data_cache: &mut RelayChainDataCache<RI>,
	relay_parent: &RelayHeader,
	para_id: ParaId,
	relay_parent_offset: u32,
) -> Result<Option<Cores>, ()> {
	let claim_queue = &relay_chain_data_cache
		.get_mut_relay_chain_data(relay_parent.hash())
		.await?
		.claim_queue;

	let core_indices = claim_queue
		.iter_claims_at_depth_for_para(relay_parent_offset as _, para_id)
		.collect::<Vec<_>>();

	Ok(if core_indices.is_empty() {
		None
	} else {
		Some(Cores {
			selector: CoreSelector(0),
			claim_queue_offset: ClaimQueueOffset(relay_parent_offset as u8),
			core_indices,
		})
	})
}
