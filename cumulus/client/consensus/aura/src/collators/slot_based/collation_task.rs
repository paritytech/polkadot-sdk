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

use std::collections::VecDeque;

use codec::Encode;

use cumulus_client_collator::service::ServiceInterface as CollatorServiceInterface;
use cumulus_relay_chain_interface::RelayChainInterface;

use polkadot_node_primitives::{MaybeCompressedPoV, SubmitCollationParams};
use polkadot_node_subsystem::messages::CollationGenerationMessage;
use polkadot_overseer::Handle as OverseerHandle;
use polkadot_primitives::{CollatorPair, CoreIndex, Id as ParaId};

use futures::prelude::*;

use sp_runtime::traits::{Block as BlockT, Header};

use super::{scheduled_cores, CollatorMessage};

const LOG_TARGET: &str = "aura::cumulus::collation_task";

/// Parameters for the collation task.
pub struct Params<Block: BlockT, RClient, CS> {
	/// A handle to the relay-chain client.
	pub relay_client: RClient,
	/// The collator key used to sign collations before submitting to validators.
	pub collator_key: CollatorPair,
	/// The para's ID.
	pub para_id: ParaId,
	/// A handle to the relay-chain client's "Overseer" or task orchestrator.
	pub overseer_handle: OverseerHandle,
	/// Whether we should reinitialize the collator config (i.e. we are transitioning to aura).
	pub reinitialize: bool,
	/// Collator service interface
	pub collator_service: CS,
	/// Receiver channel for communication with the block builder task.
	pub collator_receiver: tokio::sync::mpsc::Receiver<CollatorMessage<Block>>,
}

/// Asynchronously executes the collation task for a parachain.
///
/// This function initializes the collator subsystems necessary for producing and submitting
/// collations to the relay chain. It listens for new best relay chain block notifications and
/// handles collator messages. If our parachain is scheduled on a core and we have a candidate,
/// the task will build a collation and send it to the relay chain.
pub async fn run_collation_task<Block, RClient, CS>(mut params: Params<Block, RClient, CS>)
where
	Block: BlockT,
	CS: CollatorServiceInterface<Block> + Send + Sync + 'static,
	RClient: RelayChainInterface + Clone + 'static,
{
	cumulus_client_collator::initialize_collator_subsystems(
		&mut params.overseer_handle,
		params.collator_key,
		params.para_id,
		params.reinitialize,
	)
	.await;

	let collator_service = params.collator_service;
	let mut best_notifications = match params.relay_client.new_best_notification_stream().await {
		Ok(s) => s,
		Err(err) => {
			tracing::error!(
				target: LOG_TARGET,
				?err,
				"Failed to initialize consensus: no relay chain import notification stream"
			);

			return
		},
	};

	let mut overseer_handle = params.overseer_handle;
	let mut core_queue = Default::default();
	let mut messages = VecDeque::new();
	loop {
		tokio::select! {
			// Check for scheduled cores.
			Some(notification) = best_notifications.next() => {
				core_queue =
					scheduled_cores(notification.hash(), params.para_id, &params.relay_client).await;
				tracing::debug!(
					target: LOG_TARGET,
					relay_parent = ?notification.hash(),
					?params.para_id,
					cores = ?core_queue,
					"New best relay block.",
				);
			},
			// Add new message from the block builder to the queue.
			collator_message = params.collator_receiver.recv() => {
				if let Some(message) = collator_message {
				tracing::debug!(
					target: LOG_TARGET,
					hash = ?message.hash,
					"Pushing new message.",
				);
					messages.push_back(message);
				}
			}
		}

		while !core_queue.is_empty() {
			// If there are no more messages to process, we wait for new messages.
			let Some(message) = messages.pop_front() else {
				break;
			};

			handle_collation_message(
				message,
				&collator_service,
				&mut overseer_handle,
				&mut core_queue,
			)
			.await;
		}
	}
}

async fn handle_collation_message<Block: BlockT>(
	message: CollatorMessage<Block>,
	collator_service: &impl CollatorServiceInterface<Block>,
	overseer_handle: &mut OverseerHandle,
	core_queue: &mut VecDeque<CoreIndex>,
) {
	let CollatorMessage {
		parent_header,
		hash,
		parachain_candidate,
		validation_code_hash,
		relay_parent,
	} = message;

	if core_queue.is_empty() {
		tracing::warn!(target: crate::LOG_TARGET, cores_for_para = core_queue.len(), "Not submitting since we have no cores left!.");
		return;
	}

	let number = parachain_candidate.block.header().number().clone();
	let (collation, block_data) =
		match collator_service.build_collation(&parent_header, hash, parachain_candidate) {
			Some(collation) => collation,
			None => {
				tracing::warn!(target: LOG_TARGET, ?hash, ?number, "Unable to build collation.");
				return;
			},
		};

	tracing::info!(
		target: LOG_TARGET,
		"PoV size {{ header: {}kb, extrinsics: {}kb, storage_proof: {}kb }}",
		block_data.header().encode().len() as f64 / 1024f64,
		block_data.extrinsics().encode().len() as f64 / 1024f64,
		block_data.storage_proof().encode().len() as f64 / 1024f64,
	);

	if let MaybeCompressedPoV::Compressed(ref pov) = collation.proof_of_validity {
		tracing::info!(
			target: LOG_TARGET,
			"Compressed PoV size: {}kb",
			pov.block_data.0.len() as f64 / 1024f64,
		);
	}

	if let Some(core) = core_queue.pop_front() {
		tracing::debug!(target: LOG_TARGET, ?core, ?hash, ?number, "Submitting collation for core.");
		overseer_handle
			.send_msg(
				CollationGenerationMessage::SubmitCollation(SubmitCollationParams {
					relay_parent,
					collation,
					parent_head: parent_header.encode().into(),
					validation_code_hash,
					core_index: core,
					result_sender: None,
				}),
				"SubmitCollation",
			)
			.await;
	}
}
