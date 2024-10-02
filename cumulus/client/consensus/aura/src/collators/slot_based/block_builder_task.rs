// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use codec::{Codec, Encode};

use cumulus_client_collator::service::ServiceInterface as CollatorServiceInterface;
use cumulus_client_consensus_common::{self as consensus_common, ParachainBlockImportMarker};
use cumulus_client_consensus_proposer::ProposerInterface;
use cumulus_primitives_aura::AuraUnincludedSegmentApi;
use cumulus_primitives_core::{
	GetCoreSelectorApi, PersistedValidationData, DEFAULT_CLAIM_QUEUE_OFFSET,
};
use cumulus_relay_chain_interface::RelayChainInterface;

use polkadot_primitives::{
	vstaging::{ClaimQueueOffset, CoreSelector},
	BlockId, CoreIndex, Hash as RelayHash, Header as RelayHeader, Id as ParaId,
	OccupiedCoreAssumption,
};

use futures::prelude::*;
use sc_client_api::{backend::AuxStore, BlockBackend, BlockOf, UsageProvider};
use sc_consensus::BlockImport;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_application_crypto::AppPublic;
use sp_blockchain::HeaderBackend;
use sp_consensus_aura::{AuraApi, Slot};
use sp_core::{crypto::Pair, U256};
use sp_inherents::CreateInherentDataProviders;
use sp_keystore::KeystorePtr;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, Member, One};
use sp_timestamp::Timestamp;
use std::{collections::BTreeSet, sync::Arc, time::Duration};

use super::CollatorMessage;
use crate::{
	collator::{self as collator_util},
	collators::{check_validation_code_or_log, cores_scheduled_for_para},
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

		let mut relay_chain_fetcher = RelayChainCachingFetcher::new(relay_client.clone(), para_id);

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
				match core_selector(&*para_client, &parent).await {
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
			}) = relay_chain_fetcher
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

			let slot_claim = match crate::collators::can_build_upon::<_, _, P>(
				para_slot.slot,
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

/// Contains relay chain data necessary for parachain block building.
#[derive(Clone)]
struct RelayChainData {
	/// Current relay chain parent header.
	pub relay_parent_header: RelayHeader,
	/// The cores on which the para is scheduled at the configured claim queue offset.
	pub scheduled_cores: Vec<CoreIndex>,
	/// Maximum configured PoV size on the relay chain.
	pub max_pov_size: u32,
	/// The claimed cores at a relay parent.
	pub claimed_cores: BTreeSet<CoreIndex>,
}

/// Simple helper to fetch relay chain data and cache it based on the current relay chain best block
/// hash.
struct RelayChainCachingFetcher<RI> {
	relay_client: RI,
	para_id: ParaId,
	last_data: Option<(RelayHash, RelayChainData)>,
}

impl<RI> RelayChainCachingFetcher<RI>
where
	RI: RelayChainInterface + Clone + 'static,
{
	pub fn new(relay_client: RI, para_id: ParaId) -> Self {
		Self { relay_client, para_id, last_data: None }
	}

	/// Fetch required [`RelayChainData`] from the relay chain.
	/// If this data has been fetched in the past for the incoming hash, it will reuse
	/// cached data.
	pub async fn get_mut_relay_chain_data(
		&mut self,
		relay_parent: RelayHash,
		claim_queue_offset: ClaimQueueOffset,
	) -> Result<&mut RelayChainData, ()> {
		match &self.last_data {
			Some((last_seen_hash, _)) if *last_seen_hash == relay_parent => {
				tracing::trace!(target: crate::LOG_TARGET, %relay_parent, "Using cached data for relay parent.");
				Ok(&mut self.last_data.as_mut().expect("last_data is Some").1)
			},
			_ => {
				tracing::trace!(target: crate::LOG_TARGET, %relay_parent, "Relay chain best block changed, fetching new data from relay chain.");
				let data = self.update_for_relay_parent(relay_parent, claim_queue_offset).await?;
				self.last_data = Some((relay_parent, data));
				Ok(&mut self.last_data.as_mut().expect("last_data was just set above").1)
			},
		}
	}

	/// Fetch fresh data from the relay chain for the given relay parent hash.
	async fn update_for_relay_parent(
		&self,
		relay_parent: RelayHash,
		claim_queue_offset: ClaimQueueOffset,
	) -> Result<RelayChainData, ()> {
		let scheduled_cores = cores_scheduled_for_para(
			relay_parent,
			self.para_id,
			&self.relay_client,
			claim_queue_offset,
		)
		.await;

		let Ok(Some(relay_parent_header)) =
			self.relay_client.header(BlockId::Hash(relay_parent)).await
		else {
			tracing::warn!(target: crate::LOG_TARGET, "Unable to fetch latest relay chain block header.");
			return Err(())
		};

		let max_pov_size = match self
			.relay_client
			.persisted_validation_data(relay_parent, self.para_id, OccupiedCoreAssumption::Included)
			.await
		{
			Ok(None) => return Err(()),
			Ok(Some(pvd)) => pvd.max_pov_size,
			Err(err) => {
				tracing::error!(target: crate::LOG_TARGET, ?err, "Failed to gather information from relay-client");
				return Err(())
			},
		};

		Ok(RelayChainData {
			relay_parent_header,
			scheduled_cores,
			max_pov_size,
			claimed_cores: BTreeSet::new(),
		})
	}
}

async fn core_selector<Block: BlockT, Client>(
	para_client: &Client,
	parent: &consensus_common::PotentialParent<Block>,
) -> Result<(CoreSelector, ClaimQueueOffset), sp_api::ApiError>
where
	Client: ProvideRuntimeApi<Block> + Send + Sync,
	Client::Api: GetCoreSelectorApi<Block>,
{
	let block_hash = parent.hash;
	let runtime_api = para_client.runtime_api();

	if runtime_api.has_api::<dyn GetCoreSelectorApi<Block>>(block_hash)? {
		Ok(runtime_api.core_selector(block_hash)?)
	} else {
		let next_block_number: U256 = (*parent.header.number() + One::one()).into();

		// If the runtime API does not support the core selector API, fallback to some default
		// values.
		Ok((CoreSelector(next_block_number.byte(0)), ClaimQueueOffset(DEFAULT_CLAIM_QUEUE_OFFSET)))
	}
}
