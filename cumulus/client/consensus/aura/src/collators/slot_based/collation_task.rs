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

use std::path::PathBuf;

use cumulus_client_collator::service::ServiceInterface as CollatorServiceInterface;
use cumulus_relay_chain_interface::RelayChainInterface;

use polkadot_node_primitives::{
	MaybeCompressedPoV, SegmentCollation, SubmitCollationParams, SubmitSegmentParams,
};
use polkadot_node_subsystem::messages::CollationGenerationMessage;
use polkadot_overseer::Handle as OverseerHandle;
use polkadot_primitives::{CollatorPair, Id as ParaId};

use cumulus_primitives_core::relay_chain::BlockId;
use futures::prelude::*;

use crate::export_pov_to_path;
use sc_utils::mpsc::TracingUnboundedReceiver;
use sp_runtime::traits::{Block as BlockT, Header};

use super::{CollatorMessage, CollatorResubmitSegment, SegmentKind};

const LOG_TARGET: &str = "aura::cumulus::collation_task";

/// Parameters for the collation task.
pub struct Params<Block: BlockT, RClient, CS> {
	/// A handle to the relay-chain client.
	pub relay_client: RClient,
	/// The collator key used to sign collations before submitting to validators.
	pub collator_key: CollatorPair,
	/// The para's ID.
	pub para_id: ParaId,
	/// Whether we should reinitialize the collator config (i.e. we are transitioning to aura).
	pub reinitialize: bool,
	/// Collator service interface
	pub collator_service: CS,
	/// Receiver channel for V2 single-bundle collations from the block builder task.
	pub collator_receiver: TracingUnboundedReceiver<CollatorMessage<Block>>,
	/// Receiver channel for V3 segment collations from the block builder task.
	pub resubmit_receiver: TracingUnboundedReceiver<CollatorResubmitSegment<Block>>,
	/// The handle from the special slot based block import.
	pub block_import_handle: super::SlotBasedBlockImportHandle<Block>,
	/// When set, the collator will export every produced `POV` to this folder.
	pub export_pov: Option<PathBuf>,
}

/// Asynchronously executes the collation task for a parachain.
///
/// This function initializes the collator subsystems necessary for producing and submitting
/// collations to the relay chain. It listens for new best relay chain block notifications and
/// handles collator messages. If our parachain is scheduled on a core and we have a candidate,
/// the task will build a collation and send it to the relay chain.
pub async fn run_collation_task<Block, RClient, CS>(
	Params {
		relay_client,
		collator_key,
		para_id,
		reinitialize,
		collator_service,
		mut collator_receiver,
		mut resubmit_receiver,
		mut block_import_handle,
		export_pov,
	}: Params<Block, RClient, CS>,
) where
	Block: BlockT,
	CS: CollatorServiceInterface<Block> + Send + Sync + 'static,
	RClient: RelayChainInterface + Clone + 'static,
{
	let Ok(mut overseer_handle) = relay_client.overseer_handle() else {
		tracing::error!(target: LOG_TARGET, "Failed to get overseer handle.");
		return;
	};

	cumulus_client_collator::initialize_collator_subsystems(
		&mut overseer_handle,
		collator_key,
		para_id,
		reinitialize,
	)
	.await;

	loop {
		futures::select! {
			collator_message = collator_receiver.next() => {
				let Some(message) = collator_message else {
					return;
				};

				handle_collation_message(message, &collator_service, &mut overseer_handle, relay_client.clone(), export_pov.clone()).await;
			},
			resubmit_message = resubmit_receiver.next() => {
				let Some(message) = resubmit_message else {
					return;
				};

				// Seed the segment with the current unincluded-segment collations. Empty for now;
				// future resubmission machinery — the layer that computes and keeps the unincluded
				// segment up to date across parachain slots — will populate this before dispatch.
				let collations = Vec::new();
				handle_resubmit_segment(message, collations, &collator_service, &mut overseer_handle, relay_client.clone(), export_pov.clone()).await;
			},
			block_import_msg = block_import_handle.next().fuse() => {
				// TODO: Implement me.
				// Issue: https://github.com/paritytech/polkadot-sdk/issues/6495
				let _ = block_import_msg;
			}
		}
	}
}

/// Handle an incoming single-bundle V2 [`CollatorMessage`] and forward it to the collation
/// generation subsystem as a [`CollationGenerationMessage::SubmitCollation`].
async fn handle_collation_message<Block: BlockT, RClient: RelayChainInterface + Clone + 'static>(
	message: CollatorMessage<Block>,
	collator_service: &impl CollatorServiceInterface<Block>,
	overseer_handle: &mut OverseerHandle,
	relay_client: RClient,
	export_pov: Option<PathBuf>,
) {
	let CollatorMessage {
		parent_header,
		blocks,
		proof,
		validation_code_hash,
		relay_parent,
		core_index,
		validation_data,
	} = message;

	let (collation, block_data) =
		match collator_service.build_multi_block_collation(&parent_header, blocks, proof, None) {
			Some(collation) => collation,
			None => {
				tracing::warn!(target: LOG_TARGET, ?core_index, "Unable to build collation.");
				return;
			},
		};

	block_data.log_size_info();

	if let MaybeCompressedPoV::Compressed(ref pov) = collation.proof_of_validity {
		if let Some(pov_path) = export_pov {
			if let Ok(Some(relay_parent_header)) =
				relay_client.header(BlockId::Hash(relay_parent)).await
			{
				if let Some(header) = block_data.blocks().first().map(|b| b.header()) {
					export_pov_to_path::<Block>(
						pov_path.clone(),
						pov.clone(),
						header.hash(),
						*header.number(),
						parent_header.clone(),
						relay_parent_header.state_root,
						relay_parent_header.number,
						validation_data.max_pov_size,
					);
				}
			} else {
				tracing::error!(target: LOG_TARGET, "Failed to get relay parent header from hash: {relay_parent:?}");
			}
		}

		tracing::info!(
			target: LOG_TARGET,
			block_numbers = ?block_data.blocks().iter().map(|b| *b.header().number()).collect::<Vec<_>>(),
			"Compressed PoV size: {}kb",
			pov.block_data.0.len() as f64 / 1024f64,
		);
	}

	let session_index = match relay_client.session_index_for_child(relay_parent).await {
		Ok(session_index) => session_index,
		Err(err) => {
			tracing::error!(
				target: LOG_TARGET,
				?err,
				?relay_parent,
				"Failed to fetch session index."
			);
			return;
		},
	};

	tracing::debug!(
		target: LOG_TARGET,
		?core_index,
		block_numbers = ?block_data.blocks().iter().map(|b| *b.header().number()).collect::<Vec<_>>(),
		"Submitting collation for core.",
	);

	overseer_handle
		.send_msg(
			CollationGenerationMessage::SubmitCollation(SubmitCollationParams {
				relay_parent,
				collation,
				validation_code_hash,
				core_index,
				result_sender: None,
				scheduling_parent: None,
				session_index,
				validation_data,
			}),
			"SubmitCollation",
		)
		.await;
}

/// Handle an incoming segment message from the block builder task. Each entry is built into a
/// [`SegmentCollation`] (one PoV / one candidate on the relay chain); the whole segment is
/// forwarded to the collation-generation subsystem as a single
/// [`CollationGenerationMessage::SubmitSegment`]. Entries that fail to build or whose session
/// lookup fails are skipped — they do not abort the whole segment.
async fn handle_resubmit_segment<Block: BlockT, RClient: RelayChainInterface + Clone + 'static>(
	message: CollatorResubmitSegment<Block>,
	mut collations: Vec<SegmentCollation>,
	collator_service: &impl CollatorServiceInterface<Block>,
	overseer_handle: &mut OverseerHandle,
	relay_client: RClient,
	export_pov: Option<PathBuf>,
) {
	let CollatorResubmitSegment { scheduling_proof, kind } = message;
	let scheduling_parent = scheduling_proof.scheduling_parent();
	let core_index = match kind {
		SegmentKind::WithBundle { bundle } => {
			let CollatorMessage {
				relay_parent,
				parent_header,
				blocks,
				proof,
				validation_code_hash,
				core_index,
				validation_data,
			} = bundle;

			let (collation, block_data) = match collator_service.build_multi_block_collation(
				&parent_header,
				blocks,
				proof,
				Some(scheduling_proof.clone()),
			) {
				Some(collation) => collation,
				None => {
					tracing::warn!(target: LOG_TARGET, ?core_index, ?relay_parent, "Unable to build collation for segment entry.");
					return;
				},
			};

			block_data.log_size_info();

			if let MaybeCompressedPoV::Compressed(ref pov) = collation.proof_of_validity {
				if let Some(pov_path) = export_pov.clone() {
					if let Ok(Some(relay_parent_header)) =
						relay_client.header(BlockId::Hash(relay_parent)).await
					{
						if let Some(header) = block_data.blocks().first().map(|b| b.header()) {
							export_pov_to_path::<Block>(
								pov_path,
								pov.clone(),
								header.hash(),
								*header.number(),
								parent_header.clone(),
								relay_parent_header.state_root,
								relay_parent_header.number,
								validation_data.max_pov_size,
							);
						}
					} else {
						tracing::error!(target: LOG_TARGET, "Failed to get relay parent header from hash: {relay_parent:?}");
					}
				}

				tracing::info!(
					target: LOG_TARGET,
					block_numbers = ?block_data.blocks().iter().map(|b| *b.header().number()).collect::<Vec<_>>(),
					"Compressed PoV size: {}kb",
					pov.block_data.0.len() as f64 / 1024f64,
				);
			}

			let session_index = match relay_client.session_index_for_child(relay_parent).await {
				Ok(session_index) => session_index,
				Err(err) => {
					tracing::error!(
						target: LOG_TARGET,
						?err,
						?relay_parent,
						"Failed to fetch session index for segment entry.",
					);
					return;
				},
			};

			tracing::debug!(
				target: LOG_TARGET,
				?core_index,
				?relay_parent,
				block_numbers = ?block_data.blocks().iter().map(|b| *b.header().number()).collect::<Vec<_>>(),
				"Adding entry to segment for core.",
			);

			collations.push(SegmentCollation {
				relay_parent,
				collation,
				validation_code_hash,
				result_sender: None,
				session_index,
				validation_data,
			});

			core_index
		},
		SegmentKind::ResubmitOnly { core_index } => {
			if collations.is_empty() {
				return;
			}
			core_index
		},
	};

	overseer_handle
		.send_msg(
			CollationGenerationMessage::SubmitSegment(SubmitSegmentParams {
				scheduling_parent,
				core_index,
				collations,
			}),
			"SubmitSegment",
		)
		.await;
}
