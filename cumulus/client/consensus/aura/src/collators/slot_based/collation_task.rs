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

use std::{path::PathBuf, sync::Arc};

use cumulus_client_collator::service::ServiceInterface as CollatorServiceInterface;
use cumulus_client_consensus_common::ValidationCodeHashProvider;
use cumulus_client_resubmission_store::ResubmissionStore;
use cumulus_primitives_core::SchedulingProof;
use cumulus_relay_chain_interface::RelayChainInterface;
use futures::prelude::*;
use polkadot_node_primitives::{
	MaybeCompressedPoV, SegmentCollation, SubmitSegmentParams, MAX_SEGMENT_LEN,
};
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
pub struct Params<Block: BlockT, RClient, CS, Backend, CHP> {
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
	/// The para client's backend, used to hydrate resubmitted segment entries from the
	/// resubmission store — done here, off the block-production hot path.
	pub para_backend: Arc<Backend>,
	/// Validation code hash provider, used while hydrating resubmitted segment entries.
	pub code_hash_provider: CHP,
}

/// Asynchronously executes the collation task for a parachain.
///
/// This function initializes the collator subsystems necessary for producing and submitting
/// collations to the relay chain. It listens for new best relay chain block notifications and
/// handles collator messages. If our parachain is scheduled on a core and we have a candidate,
/// the task will build a collation and send it to the relay chain.
pub async fn run_collation_task<Block, RClient, CS, Backend, CHP>(
	Params {
		relay_client,
		collator_key,
		para_id,
		reinitialize,
		collator_service,
		mut collator_receiver,
		export_pov,
		para_backend,
		code_hash_provider,
	}: Params<Block, RClient, CS, Backend, CHP>,
) where
	Block: BlockT,
	CS: CollatorServiceInterface<Block> + Send + Sync + 'static,
	RClient: RelayChainInterface + Clone + 'static,
	Backend: sc_client_api::Backend<Block> + 'static,
	CHP: ValidationCodeHashProvider<Block::Hash> + Send + Sync + 'static,
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

	// Read-side handle over the resubmission aux store, used to hydrate the V3 resubmittable
	// headers off the block-production hot path. Cheap to hold (wraps the backend `Arc`).
	let resubmission_store = ResubmissionStore::new(para_backend.clone());

	let mut submitter = CollationSubmitter {
		collator_service,
		overseer_handle,
		export_pov,
		para_backend,
		code_hash_provider,
		resubmission_store,
	};

	while let Some(message) = collator_receiver.next().await {
		submitter.submit(message).await;
	}
}

/// Turns [`CollatorMessage`]s into collations and forwards them to the collation-generation
/// subsystem.
struct CollationSubmitter<Block: BlockT, CS, Backend, CHP> {
	collator_service: CS,
	overseer_handle: OverseerHandle,
	export_pov: Option<PathBuf>,
	/// Backend, store, and code-hash provider hydrate the resubmitted headers into
	/// [`CollationParts`] off the block-production hot path.
	para_backend: Arc<Backend>,
	code_hash_provider: CHP,
	resubmission_store: ResubmissionStore<Block, Backend>,
}

impl<Block, CS, Backend, CHP> CollationSubmitter<Block, CS, Backend, CHP>
where
	Block: BlockT,
	CS: CollatorServiceInterface<Block>,
	Backend: sc_client_api::Backend<Block>,
	CHP: ValidationCodeHashProvider<Block::Hash>,
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

				// Hydrate the resubmitted headers here (proof/body reads), off the block-production
				// hot path, then prepend them (oldest first) to the freshly-built bundle.
				let mut all_parts = super::resubmittable_segment::hydrate_segment(
					resubmittable_headers,
					&*self.para_backend,
					&self.code_hash_provider,
					&self.resubmission_store,
				);
				let resubmitted = all_parts.len();
				all_parts.extend(bundle);
				let total = all_parts.len();
				let fresh = total.saturating_sub(resubmitted);

				// Parts that fail to build or whose session lookup fails are skipped — they do not
				// abort the whole segment.
				let mut collations = Vec::with_capacity(all_parts.len());
				for parts in all_parts {
					if let Some(collation) =
						self.build_collation(parts, Some(scheduling_proof.clone())).await
					{
						collations.push(collation);
					}
				}

				if collations.is_empty() {
					tracing::debug!(
						target: LOG_TARGET,
						?core_index,
						resubmitted,
						fresh,
						"No collations built for segment; nothing submitted for core.",
					);
					return;
				}

				if collations.len() > MAX_SEGMENT_LEN as usize {
					tracing::warn!(
						target: LOG_TARGET,
						?core_index,
						segment_len = collations.len(),
						max = MAX_SEGMENT_LEN,
						"Segment exceeds MAX_SEGMENT_LEN; truncating.",
					);
				}

				tracing::debug!(
					target: LOG_TARGET,
					?core_index,
					segment_len = collations.len(),
					resubmitted,
					fresh,
					dropped = total.saturating_sub(collations.len()),
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

impl<Block, CS, Backend, CHP> CollationSubmitter<Block, CS, Backend, CHP>
where
	Block: BlockT,
	CS: CollatorServiceInterface<Block>,
	Backend: sc_client_api::Backend<Block>,
	CHP: ValidationCodeHashProvider<Block::Hash>,
{
	/// Build one `SegmentCollation` from `parts`: build the PoV and export it if configured. The
	/// session and relay-parent header data are carried in `parts`, resolved at build/hydration
	/// time. Returns `None` if the collation could not be built.
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
			relay_parent_session,
			relay_parent_storage_root,
			relay_parent_number,
		} = parts;

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
			if let Some(pov_path) = self.export_pov.as_ref() {
				if let Some(header) = block_data.blocks().first().map(|b| b.header()) {
					export_pov_to_path::<Block>(
						pov_path,
						pov.clone(),
						header.hash(),
						*header.number(),
						parent_header.clone(),
						relay_parent_storage_root,
						relay_parent_number,
						validation_data.max_pov_size,
					);
				}
			}

			tracing::info!(
				target: LOG_TARGET,
				block_numbers = ?block_data.blocks().iter().map(|b| *b.header().number()).collect::<Vec<_>>(),
				"Compressed PoV size: {}kb",
				pov.block_data.0.len() as f64 / 1024f64,
			);
		}

		Some(SegmentCollation {
			relay_parent,
			collation,
			validation_code_hash,
			session_index: relay_parent_session,
			validation_data,
		})
	}
}
