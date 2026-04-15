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
	extract_relay_parent, rpsr_digest, ClaimQueueOffset, CoreInfo, CoreSelector, CumulusDigestItem,
	KeyToIncludeInRelayProof, PersistedValidationData, RelayParentOffsetApi,
};
use cumulus_relay_chain_interface::RelayChainInterface;
use futures::prelude::*;
use polkadot_primitives::{Block as RelayBlock, CoreIndex, Header as RelayHeader, Id as ParaId};
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
	Client::Api: AuraApi<Block, P::Public>
		+ RelayParentOffsetApi<Block>
		+ AuraUnincludedSegmentApi<Block>
		+ KeyToIncludeInRelayProof<Block>,
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
			if slot_timer.wait_until_next_slot().await.is_err() {
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

			// Retrieve the core.
			let core = match determine_core(
				&mut relay_chain_data_cache,
				&relay_parent_header,
				para_id,
				parent_header,
				relay_parent_offset,
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
				runtime_api
					.set_call_context(sp_core::traits::CallContext::Onchain { import: false });
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

			if let Err(err) = collator_sender.unbounded_send(CollatorMessage {
				relay_parent,
				parent_header: parent_header.clone(),
				parachain_candidate: candidate.into(),
				validation_code_hash,
				core_index: core.core_index(),
				validation_data,
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
pub(crate) async fn offset_relay_parent_find_descendants<RelayClient>(
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
pub(crate) async fn determine_core<H: HeaderT, RI: RelayChainInterface + 'static>(
	relay_chain_data_cache: &mut RelayChainDataCache<RI>,
	relay_parent: &RelayHeader,
	para_id: ParaId,
	para_parent: &H,
	relay_parent_offset: u32,
) -> Result<Option<Core>, ()> {
	let cores_at_offset = &relay_chain_data_cache
		.get_mut_relay_chain_data(relay_parent.hash())
		.await?
		.claim_queue
		.iter_claims_at_depth_for_para(relay_parent_offset as usize, para_id)
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
			.get_mut_relay_chain_data(relay_parent.hash())
			.await?
			.last_claimed_core_selector;

		let selector = last_claimed_core_selector.map_or(0, |cs| cs.0 as usize) + 1;
		let Some(core_index) = cores_at_offset.get(selector) else { return Ok(None) };

		(selector, *core_index)
	};

	Ok(Some(Core {
		selector: CoreSelector(selector as u8),
		core_index,
		claim_queue_offset: ClaimQueueOffset(relay_parent_offset as u8),
		number_of_cores: cores_at_offset.len() as u16,
	}))
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
