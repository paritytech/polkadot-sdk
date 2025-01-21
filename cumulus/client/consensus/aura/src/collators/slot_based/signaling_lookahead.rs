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

use codec::Codec;

use cumulus_primitives_aura::AuraUnincludedSegmentApi;
use cumulus_primitives_core::GetCoreSelectorApi;
use cumulus_relay_chain_interface::RelayChainInterface;

use polkadot_primitives::{Id as ParaId, OccupiedCoreAssumption};

use crate::{collators::slot_based::SignalingTaskMessage, LOG_TARGET};
use cumulus_client_collator::service::ServiceInterface as CollatorServiceInterface;
use futures::prelude::*;
use polkadot_primitives::vstaging::{ClaimQueueOffset, DEFAULT_CLAIM_QUEUE_OFFSET};
use sc_client_api::{BlockBackend, UsageProvider};
use sp_api::ProvideRuntimeApi;
use sp_application_crypto::AppPublic;
use sp_blockchain::HeaderBackend;
use sp_consensus_aura::{AuraApi, Slot};
use sp_core::crypto::Pair;
use sp_keystore::KeystorePtr;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, Member};
use sp_timestamp::Timestamp;
use std::{sync::Arc, time::Duration};

/// Parameters for [`run_block_builder`].
pub struct SignalingTaskParams<Block: BlockT, Client, Backend, RelayClient, Pub, CS> {
	/// The underlying para client.
	pub para_client: Arc<Client>,
	/// The para client's backend, used to access the database.
	pub para_backend: Arc<Backend>,
	/// A handle to the relay-chain client.
	pub relay_client: RelayClient,
	/// The underlying keystore, which should contain Aura consensus keys.
	pub keystore: KeystorePtr,
	/// The para's ID.
	pub para_id: ParaId,
	/// The amount of time to spend authoring each block.
	pub authoring_duration: Duration,
	/// Drift every slot by this duration.
	/// This is a time quantity that is subtracted from the actual timestamp when computing
	/// the time left to enter a new slot. In practice, this *left-shifts* the clock time with the
	/// intent to keep our "clock" slightly behind the relay chain one and thus reducing the
	/// likelihood of encountering unfavorable notification arrival timings (i.e. we don't want to
	/// wait for relay chain notifications because we woke up too early).
	pub slot_drift: Duration,
	pub building_task_sender:
		sc_utils::mpsc::TracingUnboundedSender<SignalingTaskMessage<Pub, Block>>,
	pub collator_service: CS,
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
pub fn run_signaling_task<Block, P, Client, CS, Backend, RelayClient>(
	params: SignalingTaskParams<Block, Client, Backend, RelayClient, P::Public, CS>,
) -> impl Future<Output = ()> + Send + 'static
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block>
		+ UsageProvider<Block>
		+ HeaderBackend<Block>
		+ BlockBackend<Block>
		+ Send
		+ Sync
		+ 'static,
	Client::Api:
		AuraApi<Block, P::Public> + GetCoreSelectorApi<Block> + AuraUnincludedSegmentApi<Block>,
	Backend: sc_client_api::Backend<Block> + 'static,
	RelayClient: RelayChainInterface + Clone + 'static,
	CS: CollatorServiceInterface<Block> + Send + Sync + 'static,
	P: Pair,
	P::Public: AppPublic + Member + Codec,
	P::Signature: TryFrom<Vec<u8>> + Member + Codec,
{
	async move {
		tracing::info!(target: LOG_TARGET, "Starting lookahead slot-based block-builder task.");
		let SignalingTaskParams {
			relay_client,
			para_client,
			keystore,
			para_id,
			authoring_duration,
			para_backend,
			slot_drift,
			building_task_sender,
			collator_service,
		} = params;

		let mut import_notifications = match relay_client.import_notification_stream().await {
			Ok(s) => s,
			Err(err) => {
				tracing::error!(
					target: crate::LOG_TARGET,
					?err,
					"Failed to initialize consensus: no relay chain import notification stream"
				);

				return
			},
		};

		while let Some(relay_parent_header) = import_notifications.next().await {
			tracing::warn!(target: crate::LOG_TARGET, "New round in signaling task.");
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

			let core_index = if let Some(core_index) = crate::collators::cores_scheduled_for_para(
				relay_parent,
				params.para_id,
				&relay_client,
				ClaimQueueOffset(DEFAULT_CLAIM_QUEUE_OFFSET),
			)
			.await
			.get(0)
			{
				*core_index
			} else {
				tracing::trace!(
					target: crate::LOG_TARGET,
					?relay_parent,
					?para_id,
					"Para is not scheduled on any core, skipping import notification",
				);

				continue
			};

			let max_pov_size = match relay_client
				.persisted_validation_data(relay_parent, para_id, OccupiedCoreAssumption::Included)
				.await
			{
				Ok(None) => continue,
				Ok(Some(pvd)) => pvd.max_pov_size,
				Err(err) => {
					tracing::error!(target: crate::LOG_TARGET, ?err, "Failed to gather information from relay-client");
					continue
				},
			};

			let (included_block, initial_parent) = match crate::collators::find_parent(
				relay_parent,
				para_id,
				&*para_backend,
				&relay_client,
			)
			.await
			{
				Some(value) => value,
				None => continue,
			};

			let para_client = &*para_client;
			let keystore = &keystore;
			let Some((relay_slot, timestamp)) =
				cumulus_client_consensus_common::relay_slot_and_timestamp(
					&relay_parent_header,
					// TODO skunert fix this
					Duration::from_secs(6),
				)
			else {
				continue;
			};

			// Build in a loop until not allowed. Note that the authorities can change
			// at any block, so we need to re-claim our slot every time.
			let parent_hash = initial_parent.hash;
			let parent_header = initial_parent.header;

			let slot_duration =
				match sc_consensus_aura::standalone::slot_duration_at(&*para_client, parent_hash) {
					Ok(sd) => sd,
					Err(err) => {
						tracing::error!(target: crate::LOG_TARGET, ?err, "Failed to acquire parachain slot duration");
						continue
					},
				};

			let slot_now = Slot::from_timestamp(timestamp, slot_duration);
			tracing::debug!(
				target: crate::LOG_TARGET,
				?relay_slot,
				para_slot = ?slot_now,
				?timestamp,
				?slot_duration,
				// TODO fix this
				// relay_chain_slot_duration = ?relay_chain_slot_duration,
				"Adjusted relay-chain slot to parachain slot"
			);
			let Some(slot_claim) = crate::collators::can_build_upon::<_, _, P>(
				slot_now,
				relay_slot,
				timestamp,
				parent_hash,
				included_block,
				para_client,
				&keystore,
			)
			.await
			else {
				tracing::debug!(
					target: crate::LOG_TARGET,
					?core_index,
					slot_info = ?slot_now,
					unincluded_segment_len = parent.depth,
					relay_parent = %relay_parent,
					included = %included_block,
					parent = %parent_hash,
					"Not building block."
				);
				continue
			};

			// Do not try to build upon an unknown, pruned or bad block
			if !collator_service.check_block_status(parent_hash, &parent_header) {
				continue
			}

			tracing::debug!(
				target: crate::LOG_TARGET,
				?core_index,
				slot_info = ?slot_now,
				unincluded_segment_len = parent.depth,
				relay_parent = %relay_parent,
				included = %included_block,
				parent = %parent_hash,
				"Building block."
			);
			let build_signal = SignalingTaskMessage {
				slot_claim,
				parent_header,
				authoring_duration,
				core_index: core_index.clone(),
				relay_parent_header: relay_parent_header.clone(),
				max_pov_size,
			};
			building_task_sender
				.unbounded_send(build_signal)
				.expect("Should be able to send.")
		}
	}
}
