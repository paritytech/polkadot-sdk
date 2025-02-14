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

use cumulus_client_collator::service::ServiceInterface as CollatorServiceInterface;
use cumulus_client_consensus_common::{self as consensus_common, ParachainBlockImportMarker};
use cumulus_client_consensus_proposer::ProposerInterface;
use cumulus_primitives_aura::AuraUnincludedSegmentApi;
use cumulus_primitives_core::{GetCoreSelectorApi, PersistedValidationData};
use cumulus_relay_chain_interface::RelayChainInterface;

use polkadot_primitives::{Block as RelayBlock, Id as ParaId};

use futures::prelude::*;
use sc_client_api::{backend::AuxStore, BlockBackend, BlockOf, UsageProvider};
use sc_consensus::BlockImport;
use sp_api::ProvideRuntimeApi;
use sp_application_crypto::AppPublic;
use sp_blockchain::HeaderBackend;
use sp_consensus_aura::{AuraApi, Slot};
use sp_core::crypto::Pair;
use sp_inherents::CreateInherentDataProviders;
use sp_keystore::KeystorePtr;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, Member};
use sp_timestamp::Timestamp;
use std::{sync::Arc, time::Duration};

use super::CollatorMessage;
use crate::{
	collator::{self as collator_util},
	collators::{
		check_validation_code_or_log,
		slot_based::{
			core_selector,
			relay_chain_data_cache::{RelayChainData, RelayChainDataCache},
		},
	},
	LOG_TARGET,
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
	/// The para's ID.
	pub para_id: ParaId,
	/// The underlying block proposer this should call into.
	pub proposer: Proposer,
	/// The generic collator service used to plug into this consensus engine.
	pub collator_service: CS,
	/// The amount of time to spend authoring each block.
	pub authoring_duration: Duration,
	/// Channel to send built blocks to the collation task.
	pub collator_sender: sc_utils::mpsc::TracingUnboundedSender<CollatorMessage<Block>>,
	/// Drift every slot by this duration.
	/// This is a time quantity that is subtracted from the actual timestamp when computing
	/// the time left to enter a new slot. In practice, this *left-shifts* the clock time with the
	/// intent to keep our "clock" slightly behind the relay chain one and thus reducing the
	/// likelihood of encountering unfavorable notification arrival timings (i.e. we don't want to
	/// wait for relay chain notifications because we woke up too early).
	pub slot_drift: Duration,
}

#[derive(Debug)]
struct SlotInfo {
	pub timestamp: Timestamp,
	pub slot: Slot,
}

#[derive(Debug)]
struct SlotTimer<Block, Client, P> {
	client: Arc<Client>,
	drift: Duration,
	_marker: std::marker::PhantomData<(Block, Box<dyn Fn(P) + Send + Sync + 'static>)>,
}

/// Returns current duration since Unix epoch.
fn duration_now() -> Duration {
	use std::time::SystemTime;
	let now = SystemTime::now();
	now.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_else(|e| {
		panic!("Current time {:?} is before Unix epoch. Something is wrong: {:?}", now, e)
	})
}

/// Returns the duration until the next slot from now.
fn time_until_next_slot(slot_duration: Duration, drift: Duration) -> Duration {
	let now = duration_now().as_millis() - drift.as_millis();

	let next_slot = (now + slot_duration.as_millis()) / slot_duration.as_millis();
	let remaining_millis = next_slot * slot_duration.as_millis() - now;
	Duration::from_millis(remaining_millis as u64)
}

impl<Block, Client, P> SlotTimer<Block, Client, P>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + Send + Sync + 'static + UsageProvider<Block>,
	Client::Api: AuraApi<Block, P::Public>,
	P: Pair,
	P::Public: AppPublic + Member + Codec,
	P::Signature: TryFrom<Vec<u8>> + Member + Codec,
{
	pub fn new_with_drift(client: Arc<Client>, drift: Duration) -> Self {
		Self { client, drift, _marker: Default::default() }
	}

	/// Returns a future that resolves when the next slot arrives.
	pub async fn wait_until_next_slot(&self) -> Result<SlotInfo, ()> {
		let Ok(slot_duration) = crate::slot_duration(&*self.client) else {
			tracing::error!(target: crate::LOG_TARGET, "Failed to fetch slot duration from runtime.");
			return Err(())
		};

		let time_until_next_slot = time_until_next_slot(slot_duration.as_duration(), self.drift);
		tokio::time::sleep(time_until_next_slot).await;
		let timestamp = sp_timestamp::Timestamp::current();
		Ok(SlotInfo { slot: Slot::from_timestamp(timestamp, slot_duration), timestamp })
	}
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
		AuraApi<Block, P::Public> + GetCoreSelectorApi<Block> + AuraUnincludedSegmentApi<Block>,
	Backend: sc_client_api::Backend<Block> + 'static,
	RelayClient: RelayChainInterface + Clone + 'static,
	CIDP: CreateInherentDataProviders<Block, ()> + 'static,
	CIDP::InherentDataProviders: Send,
	BI: BlockImport<Block> + ParachainBlockImportMarker + Send + Sync + 'static,
	Proposer: ProposerInterface<Block> + Send + Sync + 'static,
	CS: CollatorServiceInterface<Block> + Send + Sync + 'static,
	CHP: consensus_common::ValidationCodeHashProvider<Block::Hash> + Send + 'static,
	P: Pair,
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
			para_id,
			proposer,
			collator_service,
			collator_sender,
			code_hash_provider,
			authoring_duration,
			para_backend,
			slot_drift,
		} = params;

		let slot_timer = SlotTimer::<_, _, P>::new_with_drift(para_client.clone(), slot_drift);

		let mut collator = {
			let params = collator_util::Params {
				create_inherent_data_providers,
				block_import,
				relay_client: relay_client.clone(),
				keystore: keystore.clone(),
				para_id,
				proposer,
				collator_service,
			};

			collator_util::Collator::<Block, P, _, _, _, _, _>::new(params)
		};

		let mut relay_chain_data_cache = RelayChainDataCache::new(relay_client.clone(), para_id);

		loop {
			// We wait here until the next slot arrives.
			let Ok(para_slot) = slot_timer.wait_until_next_slot().await else {
				return;
			};

			let Ok(relay_parent) = relay_client.best_block_hash().await else {
				tracing::warn!(target: crate::LOG_TARGET, "Unable to fetch latest relay chain block hash.");
				continue
			};

			let Some((included_block, parent)) =
				crate::collators::find_parent(relay_parent, para_id, &*para_backend, &relay_client)
					.await
			else {
				continue
			};

			let parent_hash = parent.hash;

			// Retrieve the core selector.
			let (core_selector, claim_queue_offset) =
				match core_selector(&*para_client, parent.hash, *parent.header.number()) {
					Ok(core_selector) => core_selector,
					Err(err) => {
						tracing::trace!(
							target: crate::LOG_TARGET,
							"Unable to retrieve the core selector from the runtime API: {}",
							err
						);
						continue
					},
				};

			let Ok(RelayChainData {
				relay_parent_header,
				max_pov_size,
				scheduled_cores,
				claimed_cores,
			}) = relay_chain_data_cache
				.get_mut_relay_chain_data(relay_parent, claim_queue_offset)
				.await
			else {
				continue;
			};

			if scheduled_cores.is_empty() {
				tracing::debug!(target: LOG_TARGET, "Parachain not scheduled, skipping slot.");
				continue;
			} else {
				tracing::debug!(
					target: LOG_TARGET,
					?relay_parent,
					"Parachain is scheduled on cores: {:?}",
					scheduled_cores
				);
			}

			let core_selector = core_selector.0 as usize % scheduled_cores.len();
			let Some(core_index) = scheduled_cores.get(core_selector) else {
				// This cannot really happen, as we modulo the core selector with the
				// scheduled_cores length and we check that the scheduled_cores is not empty.
				continue;
			};

			if !claimed_cores.insert(*core_index) {
				tracing::debug!(
					target: LOG_TARGET,
					"Core {:?} was already claimed at this relay chain slot",
					core_index
				);
				continue
			}

			let parent_header = parent.header;

			// We mainly call this to inform users at genesis if there is a mismatch with the
			// on-chain data.
			collator.collator_service().check_block_status(parent_hash, &parent_header);

			let Ok(relay_slot) =
				sc_consensus_babe::find_pre_digest::<RelayBlock>(relay_parent_header)
					.map(|babe_pre_digest| babe_pre_digest.slot())
			else {
				tracing::error!(target: crate::LOG_TARGET, "Relay chain does not contain babe slot. This should never happen.");
				continue;
			};

			let slot_claim = match crate::collators::can_build_upon::<_, _, P>(
				para_slot.slot,
				relay_slot,
				para_slot.timestamp,
				parent_hash,
				included_block,
				&*para_client,
				&keystore,
			)
			.await
			{
				Some(slot) => slot,
				None => {
					tracing::debug!(
						target: crate::LOG_TARGET,
						?core_index,
						slot_info = ?para_slot,
						unincluded_segment_len = parent.depth,
						relay_parent = %relay_parent,
						included = %included_block,
						parent = %parent_hash,
						"Not building block."
					);
					continue
				},
			};

			tracing::debug!(
				target: crate::LOG_TARGET,
				?core_index,
				slot_info = ?para_slot,
				unincluded_segment_len = parent.depth,
				relay_parent = %relay_parent,
				included = %included_block,
				parent = %parent_hash,
				"Building block."
			);

			let validation_data = PersistedValidationData {
				parent_head: parent_header.encode().into(),
				relay_parent_number: *relay_parent_header.number(),
				relay_parent_storage_root: *relay_parent_header.state_root(),
				max_pov_size: *max_pov_size,
			};

			let (parachain_inherent_data, other_inherent_data) = match collator
				.create_inherent_data(
					relay_parent,
					&validation_data,
					parent_hash,
					slot_claim.timestamp(),
				)
				.await
			{
				Err(err) => {
					tracing::error!(target: crate::LOG_TARGET, ?err);
					break
				},
				Ok(x) => x,
			};

			let validation_code_hash = match code_hash_provider.code_hash_at(parent_hash) {
				None => {
					tracing::error!(target: crate::LOG_TARGET, ?parent_hash, "Could not fetch validation code hash");
					break
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

			let allowed_pov_size = if cfg!(feature = "full-pov-size") {
				validation_data.max_pov_size
			} else {
				// Set the block limit to 50% of the maximum PoV size.
				//
				// TODO: If we got benchmarking that includes the proof size,
				// we should be able to use the maximum pov size.
				validation_data.max_pov_size / 2
			} as usize;

			let Ok(Some(candidate)) = collator
				.build_block_and_import(
					&parent_header,
					&slot_claim,
					None,
					(parachain_inherent_data, other_inherent_data),
					authoring_duration,
					allowed_pov_size,
				)
				.await
			else {
				tracing::error!(target: crate::LOG_TARGET, "Unable to build block at slot.");
				continue;
			};

			let new_block_hash = candidate.block.header().hash();

			// Announce the newly built block to our peers.
			collator.collator_service().announce_block(new_block_hash, None);

			if let Err(err) = collator_sender.unbounded_send(CollatorMessage {
				relay_parent,
				parent_header,
				parachain_candidate: candidate,
				validation_code_hash,
				core_index: *core_index,
			}) {
				tracing::error!(target: crate::LOG_TARGET, ?err, "Unable to send block to collation task.");
				return
			}
		}
	}
}
