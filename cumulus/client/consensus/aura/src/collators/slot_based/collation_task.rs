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

use std::{marker::PhantomData, path::PathBuf};

use cumulus_client_collator::service::ServiceInterface as CollatorServiceInterface;
use cumulus_primitives_core::{relay_chain::BlockId, SchedulingProof};
use cumulus_relay_chain_interface::RelayChainInterface;
use futures::prelude::*;
use polkadot_node_primitives::{MaybeCompressedPoV, SegmentCollation, SubmitSegmentParams};
use polkadot_node_subsystem::messages::CollationGenerationMessage;
use polkadot_overseer::Handle as OverseerHandle;
use polkadot_primitives::{CandidateDescriptorVersion, CollatorPair, Id as ParaId};
use sc_utils::mpsc::TracingUnboundedReceiver;
use sp_runtime::{
	traits::{Block as BlockT, Header},
	BoundedVec,
};

use crate::export_pov_to_path;

use super::message::{CollationParts, CollatorMessage};

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
	/// Receiver channel for collation/segment messages from the block builder task.
	pub collator_receiver: TracingUnboundedReceiver<CollatorMessage<Block>>,
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

	let mut submitter = CollationSubmitter {
		collator_service,
		overseer_handle,
		relay_client,
		export_pov,
		_phantom: PhantomData,
	};

	while let Some(message) = collator_receiver.next().await {
		submitter.submit(message).await;
	}
}

/// Turns [`CollatorMessage`]s into collations and forwards them to the collation-generation
/// subsystem.
struct CollationSubmitter<Block: BlockT, RClient, CS> {
	collator_service: CS,
	overseer_handle: OverseerHandle,
	relay_client: RClient,
	export_pov: Option<PathBuf>,
	_phantom: PhantomData<Block>,
}

impl<Block, RClient, CS> CollationSubmitter<Block, RClient, CS>
where
	Block: BlockT,
	CS: CollatorServiceInterface<Block>,
	RClient: RelayChainInterface + Clone + 'static,
{
	/// Build the collation(s) in `message` and submit them via
	/// [`CollationGenerationMessage::SubmitSegment`]: a `Collation` as a one-element V2 segment, a
	/// `Segment` as V3.
	async fn submit(&mut self, message: CollatorMessage<Block>) {
		match message {
			CollatorMessage::Collation { core_index, parts } => {
				// V2: no scheduling proof, scheduling parent is the relay parent.
				let Some(segment_collation) = self.build_collation(parts, None).await else {
					return;
				};
				let scheduling_parent = segment_collation.relay_parent;

				tracing::debug!(target: LOG_TARGET, ?core_index, ?scheduling_parent, "Submitting collation for core.");

				self.overseer_handle
					.send_msg(
						CollationGenerationMessage::SubmitSegment(SubmitSegmentParams {
							scheduling_parent,
							core_index,
							candidates_descriptor_version: CandidateDescriptorVersion::V2,
							collations: BoundedVec::truncate_from(vec![segment_collation]),
						}),
						"SubmitSegment",
					)
					.await;
			},
			CollatorMessage::Segment {
				scheduling_proof,
				core_index,
				resubmittable_headers,
				bundle,
			} => {
				// V3: scheduling parent comes from the proof.
				let scheduling_parent = scheduling_proof.scheduling_parent();

				// For now the bundle is the segment's only collation.
				// TODO: hydrate `resubmittable_headers` into parts ahead of the bundle.
				let mut collations = Vec::new();
				if let Some(parts) = bundle {
					if let Some(collation) =
						self.build_collation(parts, Some(scheduling_proof)).await
					{
						collations.push(collation);
					}
				}

				if collations.is_empty() {
					tracing::debug!(
						target: LOG_TARGET,
						?core_index,
						"No collations built for segment; nothing submitted for core.",
					);
					return;
				}

				tracing::debug!(
					target: LOG_TARGET,
					?core_index,
					segment_len = collations.len(),
					resubmitted = resubmittable_headers.len(),
					"Submitting segment for core.",
				);

				self.overseer_handle
					.send_msg(
						CollationGenerationMessage::SubmitSegment(SubmitSegmentParams {
							scheduling_parent,
							core_index,
							candidates_descriptor_version: CandidateDescriptorVersion::V3,
							collations: BoundedVec::truncate_from(collations),
						}),
						"SubmitSegment",
					)
					.await;
			},
		}
	}
}

impl<Block, RClient, CS> CollationSubmitter<Block, RClient, CS>
where
	Block: BlockT,
	CS: CollatorServiceInterface<Block>,
	RClient: RelayChainInterface + Clone + 'static,
{
	/// Build one `SegmentCollation` from `parts`: build the PoV, export it if configured, and look
	/// up the session index. Returns `None` if the collation could not be built or the session
	/// lookup failed.
	async fn build_collation(
		&self,
		parts: CollationParts<Block>,
		scheduling_proof: Option<SchedulingProof>,
	) -> Option<SegmentCollation> {
		let CollationParts {
			relay_parent,
			parent_header,
			blocks,
			proof,
			validation_code_hash,
			validation_data,
		} = parts;
		let export_pov = self.export_pov.clone();

		// The scheduling proof, including its UMP scheduling-tail override, is applied inside
		// `build_multi_block_collation`.
		let (collation, block_data) = match self.collator_service.build_multi_block_collation(
			&parent_header,
			blocks,
			proof,
			scheduling_proof,
		) {
			Some(collation) => collation,
			None => {
				tracing::warn!(target: LOG_TARGET, ?relay_parent, "Unable to build collation.");
				return None;
			},
		};

		block_data.log_size_info();

		if let MaybeCompressedPoV::Compressed(ref pov) = collation.proof_of_validity {
			if let Some(pov_path) = export_pov {
				if let Ok(Some(relay_parent_header)) =
					self.relay_client.header(BlockId::Hash(relay_parent)).await
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

		let session_index = match self.relay_client.session_index_for_child(relay_parent).await {
			Ok(session_index) => session_index,
			Err(err) => {
				tracing::error!(
					target: LOG_TARGET,
					?err,
					?relay_parent,
					"Failed to fetch session index."
				);
				return None;
			},
		};

		Some(SegmentCollation {
			relay_parent,
			collation,
			validation_code_hash,
			session_index,
			validation_data,
		})
	}
}
