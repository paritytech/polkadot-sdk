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
		BackingGroupConnectionHelper, RelayHash, RelayParentData,
	},
	LOG_TARGET,
};
use codec::{Codec, Encode};
use cumulus_client_collator::service::ServiceInterface as CollatorServiceInterface;
use cumulus_client_consensus_common::{self as consensus_common, ParachainBlockImportMarker};
use cumulus_client_proof_size_recording::prepare_proof_size_recording_aux_data;
use cumulus_primitives_aura::{AuraUnincludedSegmentApi, Slot};
use cumulus_primitives_core::{
	BlockBundleInfo, ClaimQueueOffset, CoreInfo, CoreSelector, CumulusDigestItem,
	PersistedValidationData, RelayParentOffsetApi, TargetBlockRate,
};
use cumulus_relay_chain_interface::RelayChainInterface;
use futures::prelude::*;
use polkadot_primitives::{Block as RelayBlock, CoreIndex, Header as RelayHeader, Id as ParaId};
use sc_client_api::{backend::AuxStore, BlockBackend, BlockOf, UsageProvider};
use sc_consensus::BlockImport;
use sc_consensus_aura::SlotDuration;
use sc_network_types::PeerId;
use sp_api::{ApiExt, ProofRecorder, ProvideRuntimeApi, StorageProof};
use sp_application_crypto::AppPublic;
use sp_block_builder::BlockBuilder;
use sp_blockchain::HeaderBackend;
use sp_consensus::Environment;
use sp_consensus_aura::AuraApi;
use sp_core::crypto::Pair;
use sp_externalities::Extensions;
use sp_inherents::CreateInherentDataProviders;
use sp_keystore::KeystorePtr;
use sp_runtime::{
	traits::{Block as BlockT, HashingFor, Header as HeaderT, Member},
	Saturating,
};
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
		+ BlockBuilder<Block>
		+ cumulus_primitives_core::KeyToIncludeInRelayProof<Block>,
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

		let mut best_notifications = match relay_client.new_best_notification_stream().await {
			Ok(s) => s,
			Err(err) => {
				tracing::error!(
					target: LOG_TARGET,
					?err,
					"Failed to initialize consensus: no relay chain best block notification stream"
				);
				return;
			},
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

			// Wait for the best relay block to be from the current relay
			// chain slot. If propagation exceeded `slot_offset`, this
			// blocks until a new-best notification arrives.
			// See: https://github.com/paritytech/polkadot-sdk/pull/11453
			let Some(relay_best_header) = wait_for_current_relay_block(
				&relay_client,
				&mut relay_chain_data_cache,
				&mut best_notifications,
				slot_offset,
				relay_chain_slot_duration,
			)
			.await
			else {
				tracing::warn!(target: crate::LOG_TARGET, "Unable to fetch latest relay chain block hash.");
				continue;
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
				relay_best_header,
				relay_parent_offset,
			)
			.await
			else {
				continue;
			};

			// Use the slot calculated from relay parent
			let Some(para_slot) = adjust_para_to_relay_parent_slot(
				rp_data.relay_parent(),
				relay_chain_slot_duration,
				para_slot_duration,
			) else {
				continue;
			};

			let relay_parent = rp_data.relay_parent().hash();
			let relay_parent_header = rp_data.relay_parent().clone();

			let Some(parent_search_result) = crate::collators::find_parent(
				relay_parent,
				para_id,
				&*para_backend,
				&relay_client,
				|parent| {
					// We never want to build on any "middle block" that isn't the last block in a
					// core.
					// When the digest item doesn't exist, we are running in compatibility
					// mode and all parents are valid.
					CumulusDigestItem::is_last_block_in_core(parent.digest()).unwrap_or(true)
				},
			)
			.await
			else {
				continue;
			};

			let included_header = parent_search_result.included_header;
			let initial_parent_hash = parent_search_result.best_parent_header.hash();
			let initial_parent_header = parent_search_result.best_parent_header;
			let unincluded_segment_len =
				initial_parent_header.number().saturating_sub(*included_header.number());

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
				.check_block_status(initial_parent_hash, &initial_parent_header);

			let Ok(relay_slot) =
				sc_consensus_babe::find_pre_digest::<RelayBlock>(&relay_parent_header)
					.map(|babe_pre_digest| babe_pre_digest.slot())
			else {
				tracing::error!(target: crate::LOG_TARGET, "Relay chain does not contain babe slot. This should never happen.");
				continue;
			};

			let included_header_hash = included_header.hash();

			{
				let mut runtime_api = para_client.runtime_api();
				runtime_api
					.set_call_context(sp_core::traits::CallContext::Onchain { import: false });
				if let Ok(authorities) = runtime_api.authorities(initial_parent_hash) {
					connection_helper.update::<P>(para_slot.slot, &authorities).await;
				}
			}

			let Some(slot_claim) = crate::collators::claim_slot::<_, _, P>(
				para_slot.slot,
				para_slot.timestamp,
				initial_parent_hash,
				&*para_client,
				&keystore,
			)
			.await
			else {
				tracing::debug!(
					target: crate::LOG_TARGET,
					?unincluded_segment_len,
					relay_parent = ?relay_parent,
					relay_parent_num = %relay_parent_header.number(),
					included_hash = ?included_header_hash,
					included_num = %included_header.number(),
					initial_parent = ?initial_parent_hash,
					slot = ?para_slot.slot,
					"Not eligible to claim slot."
				);
				continue;
			};

			tracing::debug!(
				target: crate::LOG_TARGET,
				?unincluded_segment_len,
				relay_parent = ?relay_parent,
				relay_parent_num = %relay_parent_header.number(),
				relay_parent_offset,
				included_hash = ?included_header_hash,
				included_num = %included_header.number(),
				initial_parent = ?initial_parent_hash,
				slot = ?para_slot.slot,
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
				match para_client.runtime_api().target_block_rate(initial_parent_hash) {
					Ok(interval) => interval,
					Err(error) => {
						tracing::debug!(
							target: crate::LOG_TARGET,
							block = ?initial_parent_hash,
							?error,
							"Failed to fetch `slot_schedule`, assuming one block per core"
						);

						// Backwards compatible we use the number of cores as number of blocks.
						cores.total_cores()
					},
				};

			// In total we want to have at max `number_of_blocks` cores to use.
			cores.truncate_cores(number_of_blocks);
			let raw_blocks_per_core = (number_of_blocks / cores.total_cores()).max(1);
			let left_over_blocks = number_of_blocks % cores.total_cores();
			let blocks_per_cores = (0..cores.total_cores())
				.map(|i| {
					// We distribute the left over blocks across the cores.
					raw_blocks_per_core + u32::from(i < left_over_blocks)
				})
				.collect::<Vec<_>>();

			tracing::debug!(
				target: crate::LOG_TARGET,
				?blocks_per_cores,
				core_indices = ?cores.core_indices(),
				"Core configuration",
			);

			let mut pov_parent_header = initial_parent_header;
			let mut pov_parent_hash = initial_parent_hash;
			let block_time = relay_chain_slot_duration / number_of_blocks;

			for blocks_per_core in blocks_per_cores {
				let time_for_core = slot_time.time_left() / cores.cores_left();

				match build_collation_for_core(BuildCollationParams {
					pov_parent_header,
					pov_parent_hash,
					relay_parent_header: &relay_parent_header,
					relay_parent_hash: relay_parent,
					max_pov_size,
					para_id,
					relay_client: &relay_client,
					code_hash_provider: &code_hash_provider,
					slot_claim: &slot_claim,
					collator_sender: &collator_sender,
					collator: &mut collator,
					allowed_pov_size,
					core_info: cores.core_info(),
					core_index: cores.core_index(),
					block_time,
					blocks_per_core,
					time_for_core,
					is_last_core_in_parachain_slot: cores.is_last_core() &&
						slot_time.is_parachain_slot_ending(para_slot_duration.as_duration()),
					collator_peer_id,
					relay_parent_data: rp_data.clone(),
					total_number_of_blocks: number_of_blocks,
					included_header_hash,
					relay_slot,
					para_slot: para_slot.slot,
					para_client: &*para_client,
				})
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
					break;
				}
			}
		}
	}
}

/// Parameters for [`build_collation_for_core`].
struct BuildCollationParams<
	'a,
	Block: BlockT,
	P: Pair,
	RelayClient,
	BI,
	CIDP,
	Proposer,
	CS,
	CHP,
	Client,
> {
	pov_parent_header: Block::Header,
	pov_parent_hash: Block::Hash,
	relay_parent_header: &'a RelayHeader,
	relay_parent_hash: RelayHash,
	max_pov_size: u32,
	para_id: ParaId,
	relay_client: &'a RelayClient,
	code_hash_provider: &'a CHP,
	slot_claim: &'a SlotClaim<P::Public>,
	collator_sender: &'a sc_utils::mpsc::TracingUnboundedSender<CollatorMessage<Block>>,
	collator: &'a mut Collator<Block, P, BI, CIDP, RelayClient, Proposer, CS>,
	allowed_pov_size: usize,
	core_info: CoreInfo,
	core_index: CoreIndex,
	block_time: Duration,
	blocks_per_core: u32,
	/// Time allocated for the core.
	time_for_core: Duration,
	is_last_core_in_parachain_slot: bool,
	collator_peer_id: PeerId,
	relay_parent_data: RelayParentData,
	total_number_of_blocks: u32,
	included_header_hash: Block::Hash,
	relay_slot: cumulus_primitives_aura::Slot,
	para_slot: cumulus_primitives_aura::Slot,
	para_client: &'a Client,
}

/// Build a collation for one core.
///
/// One collation can be composed of multiple blocks.
async fn build_collation_for_core<
	Block: BlockT,
	P,
	RelayClient,
	BI,
	CIDP,
	Proposer,
	CS,
	CHP,
	Client,
>(
	BuildCollationParams {
		pov_parent_header,
		pov_parent_hash,
		relay_parent_header,
		relay_parent_hash,
		max_pov_size,
		para_id,
		relay_client,
		code_hash_provider,
		slot_claim,
		collator_sender,
		collator,
		allowed_pov_size,
		core_info,
		core_index,
		block_time,
		blocks_per_core,
		time_for_core: slot_time_for_core,
		is_last_core_in_parachain_slot,
		collator_peer_id,
		relay_parent_data,
		total_number_of_blocks,
		included_header_hash,
		relay_slot,
		para_slot,
		para_client,
	}: BuildCollationParams<'_, Block, P, RelayClient, BI, CIDP, Proposer, CS, CHP, Client>,
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
	CHP: consensus_common::ValidationCodeHashProvider<Block::Hash> + Send + Sync + 'static,
	Client: ProvideRuntimeApi<Block>,
	Client::Api: AuraUnincludedSegmentApi<Block>
		+ ApiExt<Block>
		+ cumulus_primitives_core::KeyToIncludeInRelayProof<Block>,
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

		return Err(());
	};

	check_validation_code_or_log(&validation_code_hash, para_id, relay_client, relay_parent_hash)
		.await;

	let mut blocks = Vec::new();
	let mut proofs = Vec::new();
	let mut ignored_nodes = IgnoredNodes::default();

	let mut parent_hash = pov_parent_hash;
	let mut parent_header = pov_parent_header.clone();

	for block_index in 0..blocks_per_core {
		// Check if we can build the next block
		if !crate::collators::can_build_upon::<Block, Client>(
			parent_hash,
			included_header_hash,
			relay_slot,
			para_slot,
			para_client,
		)
		.await
		{
			tracing::debug!(
				target: LOG_TARGET,
				?parent_hash,
				?included_header_hash,
				"Cannot build next block due to unincluded segment constraints, skipping entire bundle. Will continue at the next slot."
			);

			return Ok(None);
		}

		// Create schedule for this block to determine timing decisions
		let schedule = BlockProductionSchedule::new(
			block_index,
			blocks_per_core,
			total_number_of_blocks,
			is_last_core_in_parachain_slot,
		);

		if schedule.should_skip_production() {
			tracing::debug!(
				target: LOG_TARGET,
				"Skipping block production so that the next node is able to import all blocks before its slot."
			);
			break;
		}

		tracing::trace!(
			target: LOG_TARGET,
			%block_index,
			core_index = %core_index.0,
			"Preparing to build block"
		);

		let relay_proof_request =
			crate::collators::get_relay_proof_request::<Block, Client>(para_client, parent_hash);

		let (parachain_inherent_data, other_inherent_data) = match collator
			.create_inherent_data_with_rp_offset(
				relay_parent_hash,
				&validation_data,
				parent_hash,
				slot_claim.timestamp(),
				Some(relay_parent_data.clone()),
				relay_proof_request,
				collator_peer_id,
			)
			.await
		{
			Err(err) => {
				tracing::error!(target: crate::LOG_TARGET, ?err, "Failed to create inherent data.");
				return Ok(None);
			},
			Ok(x) => x,
		};

		let storage_proof_recorder =
			ProofRecorder::<Block>::with_ignored_nodes(ignored_nodes.clone());

		let proof_size_recorder = RecordingProofSizeProvider::new(storage_proof_recorder.clone());

		let mut extra_extensions = Extensions::default();
		extra_extensions.register(ProofSizeExt::new(proof_size_recorder.clone()));

		let block_production_start = Instant::now();
		// The time we have left to spent for the block.
		let time_left_for_block = slot_time_for_core.saturating_sub(core_start.elapsed()) /
			(blocks_per_core - block_index) as u32;

		// The first block on a multi-block core gets the full remaining core time so that the
		// runtime's `FullCore` weight mode can actually be utilized. Subsequent blocks are
		// capped at `block_time` because they only carry fractional weight.
		//
		// Single-block cores (blocks_per_core == 1) go through schedule.authoring_duration()
		// so that slot handover adjustments (e.g., Shorten) are applied on the last core.
		let authoring_duration = if block_index == 0 && blocks_per_core > 1 {
			slot_time_for_core.saturating_sub(core_start.elapsed())
		} else {
			schedule.authoring_duration(time_left_for_block, block_time)
		};

		tracing::trace!(
			target: LOG_TARGET,
			?authoring_duration,
			"Building block"
		);

		let Ok(Some((built_block, mut import_block))) = collator
			.build_block(BuildBlockAndImportParams {
				parent_header: &parent_header,
				slot_claim,
				additional_pre_digest: vec![
					CumulusDigestItem::CoreInfo(core_info.clone()).to_digest_item(),
					CumulusDigestItem::BlockBundleInfo(BlockBundleInfo {
						index: block_index as u8,
						is_last: schedule.block_ends_bundle(),
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
			prepare_proof_size_recording_aux_data(parent_hash, recorded_sizes).for_each(
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
				time_used_by_block_in_secs = %block_production_start.elapsed().as_secs_f32(),
				%full_core_digest,
				%runtime_upgrade_digest,
				"Stopping block production for core",
			);
			break;
		}

		ignored_nodes.extend(IgnoredNodes::from_storage_proof::<HashingFor<Block>>(
			proofs.last().expect("We just pushed the proof into the vector; qed"),
		));
		ignored_nodes.extend(IgnoredNodes::from_memory_db(built_block.backend_transaction));

		// If there is still time left for the block in the slot, we sleep the rest of the time.
		// This ensures that we have some steady block rate.
		if let Some(sleep) = time_left_for_block
			.checked_sub(block_production_start.elapsed())
			// Let's not sleep for the last block here, to send out the collation as early as
			// possible.
			.filter(|_| !schedule.is_effective_last_block())
		{
			tokio::time::sleep(sleep).await;
		}
	}

	if blocks.is_empty() {
		tracing::debug!(
			target: LOG_TARGET,
			?core_index,
			relay_parent = ?relay_parent_hash,
			"Did not build any blocks, returning"
		);

		return Ok(None);
	}

	let proof = StorageProof::merge(proofs);

	tracing::trace!(
		target: LOG_TARGET,
		?core_index,
		relay_parent = ?relay_parent_hash,
		blocks = ?blocks.iter().map(|b| b.hash()).collect::<Vec<_>>(),
		"Sending out PoV"
	);

	if let Err(err) = collator_sender.unbounded_send(CollatorMessage {
		relay_parent: relay_parent_hash,
		parent_header: pov_parent_header.clone(),
		blocks,
		proof,
		validation_code_hash,
		core_index,
		validation_data,
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

/// Returns `true` if the best relay chain block is from the current relay chain
/// slot. Uses the wall clock adjusted by `slot_offset`.
fn is_best_relay_block_current(
	best_relay_slot: u64,
	slot_offset: Duration,
	relay_chain_slot_duration: Duration,
) -> bool {
	let now = super::slot_timer::duration_now().saturating_sub(slot_offset);
	is_best_relay_block_current_at(best_relay_slot, now, relay_chain_slot_duration)
}

/// Pure logic for the relay block freshness check, taking the current time as
/// a parameter for testability.
fn is_best_relay_block_current_at(
	best_relay_slot: u64,
	now: Duration,
	relay_chain_slot_duration: Duration,
) -> bool {
	let current_relay_slot = now.as_millis() as u64 / relay_chain_slot_duration.as_millis() as u64;
	best_relay_slot >= current_relay_slot
}

/// Wait until the best relay chain block is from the current relay chain slot.
///
/// If the current best block is already current, returns its hash immediately.
/// Otherwise waits for a new-best notification and re-checks. This ensures
/// the collator doesn't build on a stale relay parent when relay block
/// propagation exceeds `slot_offset` at a slot boundary.
///
/// Returns the best relay block hash, or `None` on error.
pub(crate) async fn wait_for_current_relay_block<RelayClient>(
	relay_client: &RelayClient,
	relay_chain_data_cache: &mut RelayChainDataCache<RelayClient>,
	best_notifications: &mut (impl Stream<Item = RelayHeader> + Unpin),
	slot_offset: Duration,
	relay_chain_slot_duration: Duration,
) -> Option<RelayHeader>
where
	RelayClient: RelayChainInterface + Clone + 'static,
{
	let relay_best_hash = relay_client.best_block_hash().await.ok()?;
	let mut first_best_header = Some(
		relay_chain_data_cache
			.get_mut_relay_chain_data(relay_best_hash)
			.await
			.ok()
			.map(|d| d.relay_parent_header.clone())?,
	);

	loop {
		// Drain buffered notifications.
		while let Some(maybe_header) = best_notifications.next().now_or_never() {
			first_best_header = Some(maybe_header?);
		}

		let best_header = match first_best_header.take() {
			Some(h) => h,
			None => best_notifications.next().await?, // Block until one arrives.
		};

		let best_slot = sc_consensus_babe::find_pre_digest::<RelayBlock>(&best_header)
			.map(|d| d.slot())
			.ok()?;

		if is_best_relay_block_current(*best_slot, slot_offset, relay_chain_slot_duration) {
			return Some(best_header);
		}

		tracing::debug!(
			target: LOG_TARGET,
			?relay_best_hash,
			relay_best_num = %best_header.number(),
			?best_slot,
			"Best relay block is stale, waiting for fresh one."
		);
	}
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
	mut relay_header: RelayHeader,
	relay_parent_offset: u32,
) -> Result<Option<RelayParentData>, ()>
where
	RelayClient: RelayChainInterface + Clone + 'static,
{
	let relay_best_block = relay_header.hash();
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

	/// Truncate `cores` to `max_cores`.
	pub fn truncate_cores(&mut self, max_cores: u32) {
		self.core_indices.truncate(max_cores as usize);
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

/// Slot handover adjustment strategy based on total block rate.
///
/// These adjustments exist because without transaction streaming, the next author
/// must sequentially import all blocks before building their own. Each variant
/// uses a different strategy to provide import buffer time.
// TODO: Once transaction streaming is implemented, this can be removed.
#[derive(Debug, Clone, Copy)]
enum SlotHandoverAdjustment {
	/// 0-1 blocks per slot - no adjustment needed.
	/// The next author has plenty of time to import.
	None,

	/// 2-3 blocks per slot (~2-3s block time) - shorten authoring time.
	Shorten {
		/// Time adjustment factor of last block authoring time.
		time_factor: f32,
	},

	/// >3 blocks per slot (<2s block time) - skip last block.
	///
	/// Block time is too fast for time reduction alone, so we skip
	/// producing the last block in each parachain slot entirely.
	Skip,
}

impl SlotHandoverAdjustment {
	/// Determine the appropriate adjustment based on total blocks per relay slot and blocks per
	/// core.
	fn from_total_blocks(total_blocks: u32, blocks_per_core: u32) -> Self {
		match total_blocks {
			0..=1 => Self::None,
			2..=3 if blocks_per_core == 1 || blocks_per_core == total_blocks => {
				Self::Shorten { time_factor: 0.5 }
			},
			_ => Self::Skip,
		}
	}

	/// Whether this adjustment skips the last block (vs adjusting time).
	fn skips_last_block(&self) -> bool {
		matches!(self, Self::Skip)
	}
}

/// Policy object that determines block production timing decisions.
///
/// Encapsulates the complex timing logic for block production, making decisions
/// about when to skip blocks, how long to spend authoring, and when to sleep.
#[derive(Debug, Clone, Copy)]
struct BlockProductionSchedule {
	mode: SlotHandoverAdjustment,
	block_index: u32,
	blocks_per_core: u32,
	is_last_core_in_parachain_slot: bool,
}

impl BlockProductionSchedule {
	fn new(
		block_index: u32,
		blocks_per_core: u32,
		total_blocks: u32,
		is_last_core_in_parachain_slot: bool,
	) -> Self {
		Self {
			mode: SlotHandoverAdjustment::from_total_blocks(total_blocks, blocks_per_core),
			block_index,
			blocks_per_core,
			is_last_core_in_parachain_slot,
		}
	}

	/// Whether this is the actual last block index in the core.
	fn is_last_block_in_core(&self) -> bool {
		self.block_index + 1 == self.blocks_per_core
	}

	/// Whether this is the second-to-last block index.
	fn is_second_to_last(&self) -> bool {
		self.block_index + 2 == self.blocks_per_core
	}

	/// Whether to skip producing this block entirely.
	///
	/// In Bundling mode, we skip the last block in the parachain slot
	/// to give the next author time to import all previous blocks.
	fn should_skip_production(&self) -> bool {
		self.mode.skips_last_block() &&
			self.is_last_block_in_core() &&
			self.is_last_core_in_parachain_slot
	}

	/// Whether this is effectively the last block we'll produce for this core.
	///
	/// Used for `BundleInfo { is_last }` - validators need to know which
	/// block might be final. Also used for sleep decisions - we don't sleep
	/// after the last or second-to-last block to speed up the final stretch.
	///
	/// The second-to-last block is always included because:
	/// 1. In Bundling mode on the last core, we skip the actual last block
	/// 2. Even when not skipping, avoiding sleep on the last two blocks speeds things up
	fn is_effective_last_block(&self) -> bool {
		self.is_last_block_in_core() || self.is_second_to_last()
	}

	/// Whether the node stops block production after this block for this bundle.
	///
	/// Returns `true` when:
	/// - This is the last block in the core, OR
	/// - This is the second-to-last and the actual last will be skipped (Skip mode on the last core
	///   of the parachain slot).
	fn block_ends_bundle(&self) -> bool {
		self.is_last_block_in_core() ||
			(self.is_second_to_last() &&
				self.mode.skips_last_block() &&
				self.is_last_core_in_parachain_slot)
	}

	/// Compute the authoring duration given available time.
	fn authoring_duration(&self, time_left: Duration, block_time: Duration) -> Duration {
		let adjusted = match &self.mode {
			SlotHandoverAdjustment::Shorten { time_factor }
				if self.is_last_core_in_parachain_slot =>
			{
				time_left.mul_f32(*time_factor)
			},
			_ => time_left,
		};

		block_time.min(adjusted)
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

#[cfg(test)]
mod block_production_schedule_tests {
	use super::*;

	mod mode_tests {
		use super::*;

		#[test]
		fn mode_selection_from_total_blocks() {
			// 0-1 blocks = None
			assert!(matches!(
				SlotHandoverAdjustment::from_total_blocks(0, 1),
				SlotHandoverAdjustment::None
			));
			assert!(matches!(
				SlotHandoverAdjustment::from_total_blocks(1, 1),
				SlotHandoverAdjustment::None
			));

			// 2-3 blocks = Shorten with half time
			assert!(matches!(
				SlotHandoverAdjustment::from_total_blocks(2, 1),
				SlotHandoverAdjustment::Shorten { time_factor: 0.5 }
			));
			assert!(matches!(
				SlotHandoverAdjustment::from_total_blocks(3, 1),
				SlotHandoverAdjustment::Shorten { time_factor: 0.5 }
			));

			assert!(matches!(
				SlotHandoverAdjustment::from_total_blocks(3, 2),
				SlotHandoverAdjustment::Skip
			));

			// >3 blocks = Skip
			assert!(matches!(
				SlotHandoverAdjustment::from_total_blocks(4, 2),
				SlotHandoverAdjustment::Skip
			));
			assert!(matches!(
				SlotHandoverAdjustment::from_total_blocks(12, 4),
				SlotHandoverAdjustment::Skip
			));
		}

		#[test]
		fn mode_behavior_flags() {
			assert!(!SlotHandoverAdjustment::None.skips_last_block());

			let shorten = SlotHandoverAdjustment::Shorten { time_factor: 0.5 };
			assert!(!shorten.skips_last_block());

			assert!(SlotHandoverAdjustment::Skip.skips_last_block());
		}
	}

	mod schedule_tests {
		use super::*;

		#[test]
		fn skip_production_only_in_fast_mode_last_core_last_block() {
			// Should skip: Fast mode, last core, last block
			assert!(BlockProductionSchedule::new(0, 1, 4, true).should_skip_production());

			// Should NOT skip: not last core in parachain slot
			assert!(!BlockProductionSchedule::new(0, 1, 4, false).should_skip_production());

			// Should NOT skip: Medium mode (uses time adjustment instead)
			assert!(!BlockProductionSchedule::new(0, 1, 3, true).should_skip_production());

			// Should NOT skip: not last block in core
			assert!(!BlockProductionSchedule::new(0, 2, 4, true).should_skip_production());

			// Should skip: Fast mode, last core, last block
			assert!(BlockProductionSchedule::new(3, 4, 12, true).should_skip_production());
			// Should skip: Fast mode, last core, second to last block
			assert!(!BlockProductionSchedule::new(2, 4, 12, true).should_skip_production());

			// Should NOT skip: Fast mode, not last core, last block
			assert!(!BlockProductionSchedule::new(3, 4, 12, false).should_skip_production());
			assert!(!BlockProductionSchedule::new(2, 4, 12, false).should_skip_production());
		}

		#[test]
		fn effective_last_block_includes_second_to_last() {
			// block_index 2 is second-to-last (2+2 == 4), always effective last
			let schedule = BlockProductionSchedule::new(2, 4, 12, true);
			assert!(schedule.is_effective_last_block());
			assert!(!schedule.is_last_block_in_core());
			assert!(schedule.is_second_to_last());

			// Same config but not last core - second-to-last is STILL effective last
			// (original logic doesn't gate on is_last_core_in_parachain_slot)
			let schedule = BlockProductionSchedule::new(2, 4, 12, false);
			assert!(schedule.is_effective_last_block());

			let schedule = BlockProductionSchedule::new(3, 4, 12, false);
			assert!(schedule.is_effective_last_block());

			// First block is not effective last
			let schedule = BlockProductionSchedule::new(0, 4, 12, true);
			assert!(!schedule.is_effective_last_block());

			// With only 1 block per core, there's no second-to-last
			let schedule = BlockProductionSchedule::new(0, 1, 3, true);
			assert!(schedule.is_effective_last_block()); // actual last
			assert!(!schedule.is_second_to_last());
		}

		#[test]
		fn authoring_duration_halved_in_medium_mode() {
			let time_left = Duration::from_millis(2000);
			let block_time = Duration::from_millis(3000);

			// Medium mode, last block, 1 block per core -> halved
			let schedule = BlockProductionSchedule::new(0, 1, 2, true);
			assert_eq!(
				schedule.authoring_duration(time_left, block_time),
				Duration::from_millis(1000) // halved, capped by time_left/2
			);

			// Medium mode but NOT last block -> full time
			let schedule = BlockProductionSchedule::new(0, 2, 2, true);
			assert_eq!(
				schedule.authoring_duration(time_left, block_time),
				Duration::from_millis(1000) // halved
			);

			// Fast mode -> no time adjustment (uses skip instead)
			let schedule = BlockProductionSchedule::new(0, 1, 4, true);
			assert_eq!(
				schedule.authoring_duration(time_left, block_time),
				Duration::from_millis(2000)
			);
		}

		#[test]
		fn block_ends_bundle_only_on_true_last_block() {
			// 6 blocks per core, Skip mode, last core:
			// only the actual last (index 5) and second-to-last (index 4, because last
			// will be skipped) should return true.
			assert!(!BlockProductionSchedule::new(0, 6, 12, true).block_ends_bundle());
			assert!(!BlockProductionSchedule::new(3, 6, 12, true).block_ends_bundle());
			assert!(BlockProductionSchedule::new(4, 6, 12, true).block_ends_bundle());
			assert!(BlockProductionSchedule::new(5, 6, 12, true).block_ends_bundle());

			// Same config but NOT last core: second-to-last must NOT end the bundle
			// (skip only applies on last core).
			assert!(!BlockProductionSchedule::new(4, 6, 12, false).block_ends_bundle());
			assert!(BlockProductionSchedule::new(5, 6, 12, false).block_ends_bundle());

			// Shorten mode (2 blocks, 1 per core, last core): no skipping, so only the
			// actual last block ends the bundle.
			assert!(BlockProductionSchedule::new(0, 1, 2, true).block_ends_bundle());

			// None mode (1 block total): trivially the last.
			assert!(BlockProductionSchedule::new(0, 1, 1, true).block_ends_bundle());
			assert!(BlockProductionSchedule::new(0, 1, 1, false).block_ends_bundle());

			// 2 blocks on 1 core (Shorten mode): only index 1 ends the bundle.
			assert!(!BlockProductionSchedule::new(0, 2, 2, true).block_ends_bundle());
			assert!(BlockProductionSchedule::new(1, 2, 2, true).block_ends_bundle());
		}

		/// This test verifies that the new schedule logic matches the original inline logic
		/// for various block/core configurations.
		#[test]
		fn schedule_matches_original_logic() {
			// Test various configurations to ensure schedule matches original behavior
			let test_cases = [
				// (block_index, blocks_per_core, total_blocks, is_last_core)
				(0, 1, 1, false), // Normal: 1 block, not last core
				(0, 1, 1, true),  // Normal: 1 block, last core
				(0, 1, 2, true),  // Medium: 2 blocks, last core
				(0, 1, 3, true),  // Medium: 3 blocks, last core
				(0, 1, 4, true),  // Fast: 4 blocks, last core (should skip)
				(0, 1, 4, false), // Fast: 4 blocks, not last core
				(0, 2, 6, true),  // Fast: 6 blocks, 2 per core, block 0
				(1, 2, 6, true),  // Fast: 6 blocks, 2 per core, block 1 (last)
				(0, 4, 12, true), // Fast: 12 blocks, 4 per core, block 0
				(2, 4, 12, true), // Fast: 12 blocks, 4 per core, block 2 (second-to-last)
				(3, 4, 12, true), // Fast: 12 blocks, 4 per core, block 3 (last, should skip)
			];

			for (block_index, blocks_per_core, total_blocks, is_last_core) in test_cases {
				let schedule = BlockProductionSchedule::new(
					block_index,
					blocks_per_core,
					total_blocks,
					is_last_core,
				);

				// Original is_last_block_in_core logic
				let original_is_last = block_index + 1 == blocks_per_core ||
					(block_index + 2 == blocks_per_core && blocks_per_core > 1);

				// Original skip logic
				let original_skip =
					block_index + 1 == blocks_per_core && total_blocks > 3 && is_last_core;

				assert_eq!(
					schedule.is_effective_last_block(),
					original_is_last,
					"is_effective_last_block mismatch for ({}, {}, {}, {})",
					block_index,
					blocks_per_core,
					total_blocks,
					is_last_core
				);

				assert_eq!(
					schedule.should_skip_production(),
					original_skip,
					"should_skip_production mismatch for ({}, {}, {}, {})",
					block_index,
					blocks_per_core,
					total_blocks,
					is_last_core
				);
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const RELAY_SLOT_DURATION: Duration = Duration::from_secs(6);

	/// Simulate the wall clock at a specific point within a relay slot.
	///
	/// `relay_slot` is the current relay chain slot number, `ms_into_slot` is
	/// how far into that slot we are (0..6000).
	fn now_at(relay_slot: u64, ms_into_slot: u64) -> Duration {
		Duration::from_millis(relay_slot * 6000 + ms_into_slot)
	}

	// ---------------------------------------------------------------
	// Tests for `is_best_relay_block_current_at`
	// ---------------------------------------------------------------

	#[test]
	fn best_block_in_current_slot_is_current() {
		// Wall clock in slot 804, best block from slot 804 → current.
		assert!(is_best_relay_block_current_at(804, now_at(804, 500), RELAY_SLOT_DURATION));
	}

	#[test]
	fn best_block_in_previous_slot_is_stale() {
		// Wall clock in slot 805, best block from slot 804 → stale.
		assert!(!is_best_relay_block_current_at(804, now_at(805, 500), RELAY_SLOT_DURATION));
	}

	#[test]
	fn the_bug_scenario_best_block_stale_at_slot_boundary() {
		// THE BUG: wall clock just crossed into slot 805 (17ms in),
		// but best relay block is still from slot 804. Stale.
		assert!(!is_best_relay_block_current_at(804, now_at(805, 17), RELAY_SLOT_DURATION));
	}

	#[test]
	fn best_block_current_after_new_relay_block_arrives() {
		// New relay block (slot 805) arrives. Wall clock in slot 805.
		assert!(is_best_relay_block_current_at(805, now_at(805, 500), RELAY_SLOT_DURATION));
	}

	#[test]
	fn best_block_from_future_slot_is_current() {
		// Should not happen, but must not panic.
		assert!(is_best_relay_block_current_at(810, now_at(805, 0), RELAY_SLOT_DURATION));
	}

	#[test]
	fn stale_at_exact_slot_boundary() {
		// Exactly at the start of slot 805.
		// Best from 804 → stale (804 < 805).
		assert!(!is_best_relay_block_current_at(804, now_at(805, 0), RELAY_SLOT_DURATION));
		// Best from 805 → current.
		assert!(is_best_relay_block_current_at(805, now_at(805, 0), RELAY_SLOT_DURATION));
	}

	#[test]
	fn current_at_end_of_slot() {
		// 5999ms into slot 804 — still in slot 804.
		// Best from 804 → current.
		assert!(is_best_relay_block_current_at(804, now_at(804, 5999), RELAY_SLOT_DURATION));
	}

	#[test]
	fn no_wait_needed_during_normal_building() {
		// During elastic scaling in slot 804: best is from 804,
		// wall clock is mid-slot 804. No wait needed.
		for ms in (0..6000).step_by(500) {
			assert!(
				is_best_relay_block_current_at(804, now_at(804, ms), RELAY_SLOT_DURATION),
				"Should be current at {}ms into slot 804",
				ms
			);
		}
	}

	#[test]
	fn wait_needed_when_slot_advances() {
		// Wall clock moves to slot 805, best still from 804.
		// This is the race condition — must detect as stale.
		assert!(!is_best_relay_block_current_at(804, now_at(805, 0), RELAY_SLOT_DURATION));
		assert!(!is_best_relay_block_current_at(804, now_at(805, 17), RELAY_SLOT_DURATION));
		assert!(!is_best_relay_block_current_at(804, now_at(805, 500), RELAY_SLOT_DURATION));
	}
}
