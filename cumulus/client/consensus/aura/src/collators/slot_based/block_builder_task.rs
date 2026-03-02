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

use codec::{Codec, Encode};

use super::CollatorMessage;
use crate::{
	collator::{self as collator_util, BuildBlockAndImportParams},
	collators::{
		check_validation_code_or_log,
		slot_based::{
			relay_chain_data_cache::{RelayChainData, RelayChainDataCache},
			slot_timer::{SlotInfo, SlotTimer},
		},
		BackingGroupConnectionHelper, RelayParentData,
	},
	LOG_TARGET,
};
use cumulus_client_collator::service::ServiceInterface as CollatorServiceInterface;
use cumulus_client_consensus_common::{self as consensus_common, ParachainBlockImportMarker};
use cumulus_primitives_aura::{AuraUnincludedSegmentApi, Slot};
use cumulus_primitives_core::{
	extract_relay_parent, relay_chain::BlockId, rpsr_digest, ClaimQueueOffset, CoreInfo, CoreSelector, CumulusDigestItem,
	KeyToIncludeInRelayProof, PersistedValidationData, RelayParentOffsetApi, SchedulingProof, SchedulingV3EnabledApi,
};
use cumulus_relay_chain_interface::RelayChainInterface;
use futures::prelude::*;
use polkadot_primitives::{
	Block as RelayBlock, CoreIndex, Hash as RelayHash, Header as RelayHeader, Id as ParaId,
};
use sc_client_api::{backend::AuxStore, BlockBackend, BlockOf, UsageProvider};
use sc_consensus::BlockImport;
use sc_consensus_aura::SlotDuration;
use sc_network_types::PeerId;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_application_crypto::AppPublic;
use sp_blockchain::HeaderBackend;
use sp_consensus::Environment;
use sp_consensus_aura::AuraApi;
use sp_core::crypto::Pair;
use sp_inherents::CreateInherentDataProviders;
use sp_keystore::KeystorePtr;
use sp_runtime::{
	traits::{Block as BlockT, Header as HeaderT, Member, Zero},
	Saturating,
};
use sp_timestamp::Timestamp;
use std::{collections::VecDeque, sync::Arc, time::Duration};

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
	/// The amount of time to spend authoring each block.
	pub authoring_duration: Duration,
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
	Client::Api:
		AuraApi<Block, P::Public> + RelayParentOffsetApi<Block> + AuraUnincludedSegmentApi<Block> + KeyToIncludeInRelayProof<Block> + SchedulingV3EnabledApi<Block>,
	Backend: sc_client_api::Backend<Block> + 'static,
	RelayClient: RelayChainInterface + Clone + 'static,
	CIDP: CreateInherentDataProviders<Block, ()> + 'static,
	CIDP::InherentDataProviders: Send,
	BI: BlockImport<Block> + ParachainBlockImportMarker + Send + Sync + 'static,
	Proposer: Environment<Block> + Send + Sync + 'static,
	CS: CollatorServiceInterface<Block> + Send + Sync + 'static,
	CHP: consensus_common::ValidationCodeHashProvider<Block::Hash> + Send + 'static,
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
			authoring_duration,
			relay_chain_slot_duration,
			para_backend,
			slot_offset,
			max_pov_percentage,
		} = params;

		let mut slot_timer = SlotTimer::<_, _, P>::new_with_offset(
			para_client.clone(),
			slot_offset,
			relay_chain_slot_duration,
		);

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

		// Cache the scheduling parent to avoid re-determining it within the same relay
		// chain slot. With elastic scaling (multiple cores), the para slot timer fires
		// multiple times per relay chain slot — the scheduling parent cannot change
		// within the same slot. We store (scheduling_parent_hash, relay_slot_timestamp)
		// and reuse as long as the current time is still within that relay chain slot.
		let mut cached_scheduling: Option<(RelayHash, Timestamp)> = None;

		loop {
			// We wait here until the next slot arrives.
			if slot_timer.wait_until_next_slot().await.is_err() {
				tracing::error!(target: LOG_TARGET, "Unable to wait for next slot.");
				return;
			};

			let relay_slot_duration_ms = relay_chain_slot_duration.as_millis() as u64;

			// Check if the cached scheduling parent is still valid (within the same
			// relay chain slot). If not, or if there's no cache, re-determine it.
			let needs_refresh = match cached_scheduling {
				Some((cached_parent, cached_slot_ts)) => {
					let now = Timestamp::current();
					let slot_age_ms = (*now).saturating_sub(*cached_slot_ts);
					if slot_age_ms < relay_slot_duration_ms {
						tracing::debug!(
							target: LOG_TARGET,
							?cached_parent,
							slot_age_ms,
							"Reusing cached scheduling parent (still within relay chain slot).",
						);
						false
					} else {
						true
					}
				},
				None => true,
			};

			if needs_refresh {
				match determine_scheduling_parent(&relay_client, relay_chain_slot_duration).await {
					Some((parent, slot_ts)) => cached_scheduling = Some((parent, slot_ts)),
					None => {
						cached_scheduling = None;
						continue;
					},
				}
			}

			let Some((scheduling_parent_hash, _)) = cached_scheduling else {
				continue;
			};

			let best_hash = para_client.info().best_hash;
			let relay_parent_offset =
				para_client.runtime_api().relay_parent_offset(best_hash).unwrap_or_default();

			// Fetch max_claim_queue_offset from runtime API, defaulting to 1 for backwards
			// compatibility with runtimes that don't implement this method yet.
			// See: https://github.com/paritytech/polkadot-sdk/issues/8893
			let max_claim_queue_offset =
				para_client.runtime_api().max_claim_queue_offset(best_hash).unwrap_or(1);

			// Check if V3 scheduling is enabled
			let v3_enabled =
				para_client.runtime_api().scheduling_v3_enabled(best_hash).unwrap_or(false);

			// With V3 scheduling, the scheduling parent determination already accounts
			// for relay chain slot timing, so the slot offset is not needed.
			slot_timer.set_time_offset(if v3_enabled { Duration::ZERO } else { slot_offset });

			let Ok(para_slot_duration) = crate::slot_duration(&*para_client) else {
				tracing::error!(target: LOG_TARGET, "Failed to fetch slot duration from runtime.");
				continue;
			};

			let Ok(Some(rp_data)) = offset_relay_parent_find_descendants(
				&mut relay_chain_data_cache,
				scheduling_parent_hash,
				relay_parent_offset,
			)
			.await
			else {
				continue;
			};

			let Some(para_slot) = adjust_para_to_relay_parent_slot(
				rp_data.relay_parent(),
				relay_chain_slot_duration,
				para_slot_duration,
			) else {
				continue;
			};

			let relay_parent = rp_data.relay_parent().hash();
			let relay_parent_header = rp_data.relay_parent().clone();

			let Some(parent_search_result) =
				crate::collators::find_parent(relay_parent, para_id, &*para_backend, &relay_client)
					.await
			else {
				continue;
			};

			let parent_hash = parent_search_result.best_parent_header.hash();
			let included_header = parent_search_result.included_header;
			let parent_header = &parent_search_result.best_parent_header;
			// Distance from included block to best parent (unincluded segment length).
			let unincluded_segment_len =
				parent_header.number().saturating_sub(*included_header.number());

			// Determine claim queue lookup parameters based on V3 scheduling mode.
			//
			// For V3 (with scheduling_parent):
			//   - Look up claim queue at scheduling_parent
			//   - Use depth = max_claim_queue_offset (typically 1)
			//   - claim_queue_offset = max_claim_queue_offset
			//
			// For V1/V2 (without scheduling_parent):
			//   - Look up claim queue at relay_parent
			//   - Use depth = relay_parent_offset + max_claim_queue_offset
			//   - claim_queue_offset = relay_parent_offset + max_claim_queue_offset
			//
			// Collators may use lower offsets for optimistic scenarios. The runtime
			// enforces: claim_queue_offset <= relay_parent_offset + max_claim_queue_offset
			//
			// See: https://github.com/paritytech/polkadot-sdk/issues/8893
			let (claim_queue_relay_block, claim_queue_depth, claim_queue_offset) = if v3_enabled {
				// V3: look up at scheduling_parent (fresh tip), use max_claim_queue_offset
				(scheduling_parent_hash, max_claim_queue_offset as u32, max_claim_queue_offset)
			} else {
				// V1/V2: look up at relay_parent, use relay_parent_offset + max_claim_queue_offset
				let total_offset = relay_parent_offset as u8 + max_claim_queue_offset;
				(relay_parent, total_offset as u32, total_offset)
			};

			// Retrieve the core.
			let core = match determine_core(
				&mut relay_chain_data_cache,
				claim_queue_relay_block,
				&relay_parent_header,
				para_id,
				parent_header,
				claim_queue_depth,
				claim_queue_offset,
			)
			.await
			{
				Err(()) => {
					tracing::debug!(
						target: LOG_TARGET,
						?relay_parent,
						"Failed to determine core"
					);

					continue;
				},
				Ok(Some(cores)) => {
					tracing::debug!(
						target: LOG_TARGET,
						?relay_parent,
						core_selector = ?cores.selector,
						claim_queue_offset = ?cores.claim_queue_offset,
						"Going to claim core",
					);

					cores
				},
				Ok(None) => {
					tracing::debug!(
						target: LOG_TARGET,
						?relay_parent,
						"No core scheduled"
					);

					continue;
				},
			};

			let Ok(RelayChainData { max_pov_size, last_claimed_core_selector, .. }) =
				relay_chain_data_cache.get_mut_relay_chain_data(relay_parent).await
			else {
				continue;
			};

			slot_timer.update_scheduling(core.total_cores().into());

			// We mainly call this to inform users at genesis if there is a mismatch with the
			// on-chain data.
			collator.collator_service().check_block_status(parent_hash, parent_header);

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
				runtime_api.set_call_context(sp_core::traits::CallContext::Onchain);
				if let Ok(authorities) = runtime_api.authorities(parent_hash) {
					connection_helper.update::<P>(para_slot.slot, &authorities).await;
				}
			}

			let slot_claim = match crate::collators::can_build_upon::<_, _, P>(
				para_slot.slot,
				relay_slot,
				para_slot.timestamp,
				parent_hash,
				included_header_hash,
				&*para_client,
				&keystore,
			)
			.await
			{
				Some(slot) => slot,
				None => {
					tracing::debug!(
						target: crate::LOG_TARGET,
						?unincluded_segment_len,
						relay_parent = ?relay_parent,
						relay_parent_num = %relay_parent_header.number(),
						included_hash = ?included_header_hash,
						included_num = %included_header.number(),
						parent = ?parent_hash,
						slot = ?para_slot.slot,
						"Not building block."
					);
					continue;
				},
			};

			tracing::debug!(
				target: crate::LOG_TARGET,
				?unincluded_segment_len,
				relay_parent = %relay_parent,
				relay_parent_num = %relay_parent_header.number(),
				relay_parent_offset,
				claim_queue_offset,
				v3_enabled,
				included_hash = %included_header_hash,
				included_num = %included_header.number(),
				parent = %parent_hash,
				slot = ?para_slot.slot,
				"Building block."
			);

			let validation_data = PersistedValidationData {
				parent_head: parent_header.encode().into(),
				relay_parent_number: *relay_parent_header.number(),
				relay_parent_storage_root: *relay_parent_header.state_root(),
				max_pov_size: *max_pov_size,
			};

			let relay_proof_request =
				super::super::get_relay_proof_request(&*para_client, parent_hash);

			let (parachain_inherent_data, other_inherent_data) = match collator
				.create_inherent_data_with_rp_offset(
					relay_parent,
					&validation_data,
					parent_hash,
					slot_claim.timestamp(),
					Some(rp_data),
					relay_proof_request,
					collator_peer_id,
				)
				.await
			{
				Err(err) => {
					tracing::error!(target: crate::LOG_TARGET, ?err);
					break;
				},
				Ok(x) => x,
			};

			let validation_code_hash = match code_hash_provider.code_hash_at(parent_hash) {
				None => {
					tracing::error!(target: crate::LOG_TARGET, ?parent_hash, "Could not fetch validation code hash");
					break;
				},
				Some(v) => v,
			};

			check_validation_code_or_log(
				&validation_code_hash,
				para_id,
				&relay_client,
				relay_parent,
			)
			.await;

			let allowed_pov_size = if let Some(max_pov_percentage) = max_pov_percentage {
				validation_data.max_pov_size * max_pov_percentage / 100
			} else {
				// Set the block limit to 85% of the maximum PoV size.
				//
				// Once https://github.com/paritytech/polkadot-sdk/issues/6020 issue is
				// fixed, this should be removed.
				validation_data.max_pov_size * 85 / 100
			} as usize;

			let adjusted_authoring_duration =
				slot_timer.adjust_authoring_duration(authoring_duration);
			tracing::debug!(target: crate::LOG_TARGET, duration = ?adjusted_authoring_duration, "Adjusted proposal duration.");

			let Some(adjusted_authoring_duration) = adjusted_authoring_duration else {
				tracing::debug!(
					target: crate::LOG_TARGET,
					?unincluded_segment_len,
					relay_parent = ?relay_parent,
					relay_parent_num = %relay_parent_header.number(),
					included_hash = ?included_header_hash,
					included_num = %included_header.number(),
					parent = ?parent_hash,
					slot = ?para_slot.slot,
					"Not building block due to insufficient authoring duration."
				);

				continue;
			};

			let Ok(Some(candidate)) = collator
				.build_block_and_import(BuildBlockAndImportParams {
					parent_header: &parent_header,
					slot_claim: &slot_claim,
					additional_pre_digest: vec![
						CumulusDigestItem::CoreInfo(core.core_info()).to_digest_item()
					],
					parachain_inherent_data,
					extra_inherent_data: other_inherent_data,
					proposal_duration: adjusted_authoring_duration,
					max_pov_size: allowed_pov_size,
					storage_proof_recorder: None,
					extra_extensions: Default::default(),
				})
				.await
			else {
				tracing::error!(target: crate::LOG_TARGET, "Unable to build block at slot.");
				continue;
			};

			let new_block_hash = candidate.block.header().hash();

			// Announce the newly built block to our peers.
			collator.collator_service().announce_block(new_block_hash, None);

			*last_claimed_core_selector = Some(core.core_selector());

			// Check if V3 scheduling is enabled and build scheduling proof if so
			let scheduling_proof =
				if para_client.runtime_api().scheduling_v3_enabled(parent_hash).unwrap_or(false) {
					// For V3, build the scheduling proof (header chain from scheduling_parent back
					// to relay_parent)
					// - scheduling_parent = used for scheduling/backing group
					// - relay_parent = older block (used for execution context)
					// - header_chain contains headers from newest to oldest (scheduling_parent
					//   backward)
					// - header_chain length = relay_parent_offset (number of blocks between them)
					// - last header's parent_hash = relay_parent (internal scheduling parent)

					// The descendants are ordered from oldest to newest, so reverse them
					let header_chain: Vec<_> = rp_descendants.iter().rev().cloned().collect();

					tracing::debug!(
						target: crate::LOG_TARGET,
						?relay_parent,
						?scheduling_parent_hash,
						header_chain_len = header_chain.len(),
						"Building V3 collation with scheduling proof",
					);

					Some(SchedulingProof {
						header_chain,
						// Initial submission: no signature needed, core selection from UMP signals
						signed_scheduling_info: None,
					})
				} else {
					None
				};

			if let Err(err) = collator_sender.unbounded_send(CollatorMessage {
				relay_parent,
				scheduling_parent: scheduling_proof.is_some().then_some(scheduling_parent_hash),
				parent_header: parent_header.clone(),
				parachain_candidate: candidate.into(),
				validation_code_hash,
				core_index: core.core_index(),
				max_pov_size: validation_data.max_pov_size,
				scheduling_proof,
			}) {
				tracing::error!(target: crate::LOG_TARGET, ?err, "Unable to send block to collation task.");
				return;
			}
		}
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
pub(crate) async fn offset_relay_parent_find_descendants<RelayClient>(
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
		return Err(());
	};

	if relay_parent_offset == 0 {
		return Ok(Some(RelayParentData::new(relay_header)));
	}

	if sc_consensus_babe::contains_epoch_change::<RelayBlock>(&relay_header) {
		tracing::debug!(target: LOG_TARGET, ?relay_best_block, relay_best_block_number = relay_header.number(), "Relay parent is in previous session.");
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
			tracing::debug!(target: LOG_TARGET, ?relay_best_block, ancestor = %next_header.hash(), ancestor_block_number = next_header.number(), "Ancestor of best block is in previous session.");
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

/// Determines the scheduling parent hash for V3 scheduling.
///
/// If the relay best block's slot is at least one full slot duration old, returns
/// its hash. Otherwise falls back to its parent (the previous, finished slot).
///
/// Also returns the slot timestamp of the relay best block, so the caller can
/// cache and reuse the result within the same relay chain slot.
async fn determine_scheduling_parent<RelayClient>(
	relay_client: &RelayClient,
	relay_chain_slot_duration: Duration,
) -> Option<(RelayHash, Timestamp)>
where
	RelayClient: RelayChainInterface + Clone + 'static,
{
	let relay_best_hash = match relay_client.best_block_hash().await {
		Ok(hash) => hash,
		Err(err) => {
			tracing::warn!(
				target: LOG_TARGET,
				?err,
				"Unable to fetch latest relay chain block hash.",
			);
			return None;
		},
	};

	let relay_best_header = match relay_client.header(BlockId::Hash(relay_best_hash)).await {
		Ok(Some(header)) => header,
		Ok(None) => {
			tracing::warn!(
				target: LOG_TARGET,
				?relay_best_hash,
				"Relay best block header not found when determining scheduling parent.",
			);
			return None;
		},
		Err(err) => {
			tracing::warn!(
				target: LOG_TARGET,
				?relay_best_hash,
				?err,
				"Failed to fetch relay best block header for scheduling parent.",
			);
			return None;
		},
	};

	let babe_slot = match sc_consensus_babe::find_pre_digest::<RelayBlock>(&relay_best_header) {
		Ok(pre_digest) => pre_digest.slot(),
		Err(err) => {
			tracing::error!(
				target: LOG_TARGET,
				?relay_best_hash,
				?err,
				"Relay chain block does not contain a BABE pre-digest.",
			);
			return None;
		},
	};

	let slot_duration_millis = relay_chain_slot_duration.as_millis() as u64;
	let slot_timestamp = match babe_slot
		.timestamp(sc_consensus_aura::SlotDuration::from_millis(slot_duration_millis))
	{
		Some(ts) => ts,
		None => {
			tracing::error!(
				target: LOG_TARGET,
				?relay_best_hash,
				?babe_slot,
				"Failed to compute timestamp for relay best block BABE slot.",
			);
			return None;
		},
	};

	let now = Timestamp::current();
	let slot_age_ms = (*now).saturating_sub(*slot_timestamp);

	// If the relay best block's slot is at least one full relay chain slot duration old,
	// it belongs to a finished relay chain slot and can be used directly.
	if slot_age_ms >= slot_duration_millis {
		tracing::debug!(
			target: LOG_TARGET,
			?relay_best_hash,
			slot_age_ms,
			"Scheduling parent is relay best hash (slot finished).",
		);
		Some((relay_best_hash, slot_timestamp))
	} else {
		let parent_hash = *relay_best_header.parent_hash();
		tracing::debug!(
			target: LOG_TARGET,
			?relay_best_hash,
			?parent_hash,
			slot_age_ms,
			"Current relay slot still in progress, using parent as scheduling parent.",
		);
		Some((parent_hash, slot_timestamp))
	}
}

/// Return value of [`determine_core`].
pub(crate) struct Core {
	selector: CoreSelector,
	claim_queue_offset: ClaimQueueOffset,
	core_index: CoreIndex,
	number_of_cores: u16,
}

impl Core {
	/// Returns the current [`CoreInfo`].
	fn core_info(&self) -> CoreInfo {
		CoreInfo {
			selector: self.selector,
			claim_queue_offset: self.claim_queue_offset,
			number_of_cores: self.number_of_cores.into(),
		}
	}

	/// Returns the current [`CoreSelector`].
	pub(crate) fn core_selector(&self) -> CoreSelector {
		self.selector
	}

	/// Returns the current [`CoreIndex`].
	pub(crate) fn core_index(&self) -> CoreIndex {
		self.core_index
	}

	/// Returns the total number of cores.
	pub(crate) fn total_cores(&self) -> u16 {
		self.number_of_cores
	}
}

/// Determine the core for the given `para_id`.
///
/// # Parameters
///
/// - `relay_chain_data_cache`: Cache for relay chain data.
/// - `claim_queue_relay_block`: The relay block hash to look up the claim queue at. For V3: this is
///   the scheduling_parent (fresh tip). For V1/V2: this is the relay_parent.
/// - `relay_parent`: The relay parent header (used for checking if relay parent changed).
/// - `para_id`: The parachain ID.
/// - `para_parent`: The parachain parent header.
/// - `claim_queue_depth`: The depth in the claim queue to look up cores. For V3: this is
///   max_claim_queue_offset. For V1/V2: this is relay_parent_offset + max_claim_queue_offset.
/// - `claim_queue_offset`: The claim_queue_offset value to use in the result CoreInfo. This is what
///   gets sent to the relay chain via UMP signals.
///
/// # Claim Queue Offset Design
///
/// The claim_queue_offset determines how far "into the future" the collator targets in the
/// claim queue. The runtime enforces: `claim_queue_offset <= relay_parent_offset +
/// max_claim_queue_offset`
///
/// Collators may use lower offsets for optimistic scenarios (fast execution, catching up after
/// missed slots). Higher offsets are not allowed to prevent slot skipping.
///
/// See: <https://github.com/paritytech/polkadot-sdk/issues/8893>
pub(crate) async fn determine_core<H: HeaderT, RI: RelayChainInterface + 'static>(
	relay_chain_data_cache: &mut RelayChainDataCache<RI>,
	claim_queue_relay_block: RelayHash,
	relay_parent: &RelayHeader,
	para_id: ParaId,
	para_parent: &H,
	claim_queue_depth: u32,
	claim_queue_offset: u8,
) -> Result<Option<Core>, ()> {
	let cores_at_offset = &relay_chain_data_cache
		.get_mut_relay_chain_data(claim_queue_relay_block)
		.await?
		.claim_queue
		.iter_claims_at_depth_for_para(claim_queue_depth as usize, para_id)
		.collect::<Vec<_>>();

	let is_new_relay_parent = if para_parent.number().is_zero() {
		true
	} else {
		match extract_relay_parent(para_parent.digest()) {
			Some(last_relay_parent) => last_relay_parent != relay_parent.hash(),
			None => {
				rpsr_digest::extract_relay_parent_storage_root(para_parent.digest())
					.ok_or(())?
					.0 != *relay_parent.state_root()
			},
		}
	};

	let core_info = CumulusDigestItem::find_core_info(para_parent.digest());

	// If we are using a new relay parent, we can start over from the start.
	let (selector, core_index) = if is_new_relay_parent {
		let Some(core_index) = cores_at_offset.get(0) else { return Ok(None) };

		(0, *core_index)
	} else if let Some(core_info) = core_info {
		let selector = core_info.selector.0 as usize + 1;
		let Some(core_index) = cores_at_offset.get(selector) else { return Ok(None) };

		(selector, *core_index)
	} else {
		let last_claimed_core_selector = relay_chain_data_cache
			.get_mut_relay_chain_data(claim_queue_relay_block)
			.await?
			.last_claimed_core_selector;

		let selector = last_claimed_core_selector.map_or(0, |cs| cs.0 as usize) + 1;
		let Some(core_index) = cores_at_offset.get(selector) else { return Ok(None) };

		(selector, *core_index)
	};

	Ok(Some(Core {
		selector: CoreSelector(selector as u8),
		core_index,
		claim_queue_offset: ClaimQueueOffset(claim_queue_offset),
		number_of_cores: cores_at_offset.len() as u16,
	}))
}
