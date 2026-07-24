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
use cumulus_relay_chain_interface::RelayChainInterface;

use polkadot_node_primitives::{
	MaybeCompressedPoV, SegmentCollation, SubmitCollationParams, SubmitSegmentParams,
	UpwardMessages,
};
use polkadot_node_subsystem::messages::CollationGenerationMessage;
use polkadot_overseer::Handle as OverseerHandle;
use polkadot_primitives::{CandidateDescriptorVersion, CollatorPair, Id as ParaId};

use codec::{Decode, Encode};
use cumulus_primitives_core::{
	relay_chain::{BlockId, UMPSignal, UMP_SEPARATOR},
	ClaimQueueOffset, SchedulingProof, SignedSchedulingInfo,
};
use futures::prelude::*;

use crate::export_pov_to_path;
use sc_utils::mpsc::TracingUnboundedReceiver;
use sp_runtime::{
	traits::{Block as BlockT, Header},
	BoundedVec,
};

use super::{CollatorMessage, CollatorSegmentEntry, CollatorSegmentMessage};

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

	// Read-side handle over the resubmission aux store, used to hydrate the V3 unincluded segment
	// off the block-production hot path. Cheap to hold (wraps the backend `Arc`).
	let resubmission_store = ResubmissionStore::new(para_backend.clone());

	while let Some(message) = collator_receiver.next().await {
		message
			.handle(
				&collator_service,
				&mut overseer_handle,
				relay_client.clone(),
				export_pov.clone(),
				&*para_backend,
				&code_hash_provider,
				&resubmission_store,
			)
			.await;
	}
}

impl<Block: BlockT> CollatorMessage<Block> {
	/// Build the collation(s) carried by this message and forward them to the collation-generation
	/// subsystem: a single collation via [`CollationGenerationMessage::SubmitCollation`], a segment
	/// via [`CollationGenerationMessage::SubmitSegment`].
	async fn handle<RClient, Backend, CHP>(
		self,
		collator_service: &impl CollatorServiceInterface<Block>,
		overseer_handle: &mut OverseerHandle,
		relay_client: RClient,
		export_pov: Option<PathBuf>,
		para_backend: &Backend,
		code_hash_provider: &CHP,
		resubmission_store: &ResubmissionStore<Block, Backend>,
	) where
		RClient: RelayChainInterface + Clone + 'static,
		Backend: sc_client_api::Backend<Block>,
		CHP: ValidationCodeHashProvider<Block::Hash>,
	{
		match self {
			CollatorMessage::Collation { core_index, entry } => {
				// Single collations are submitted as V2: no scheduling proof, no scheduling parent.
				let Some(SegmentCollation {
					relay_parent,
					collation,
					validation_code_hash,
					result_sender,
					session_index,
					validation_data,
				}) = build_collation(entry, None, collator_service, &relay_client, export_pov).await
				else {
					return;
				};

				tracing::debug!(target: LOG_TARGET, ?core_index, ?relay_parent, "Submitting collation for core.");

				overseer_handle
					.send_msg(
						CollationGenerationMessage::SubmitCollation(SubmitCollationParams {
							relay_parent,
							collation,
							validation_code_hash,
							core_index,
							result_sender,
							scheduling_parent: None,
							session_index,
							validation_data,
						}),
						"SubmitCollation",
					)
					.await;
			},
			CollatorMessage::Segment(CollatorSegmentMessage {
				scheduling_proof,
				core_index,
				unincluded_headers,
				bundle,
			}) => {
				// Segments are V3-only, so the scheduling parent is always derived from the proof.
				let scheduling_parent = scheduling_proof.scheduling_parent();

				// Hydrate the resubmitted unincluded segment here (proof/body reads), off the
				// block-production hot path, then prepend it (oldest first) to the freshly-built
				// entries.
				let mut all_entries = super::unincluded_segment::hydrate_segment(
					unincluded_headers,
					para_backend,
					code_hash_provider,
					resubmission_store,
				);
				all_entries.extend(bundle);

				// Entries that fail to build or whose session lookup fails are skipped — they do
				// not abort the whole segment.
				let mut collations = Vec::with_capacity(all_entries.len());
				for entry in all_entries {
					if let Some(collation) = build_collation(
						entry,
						Some(scheduling_proof.clone()),
						collator_service,
						&relay_client,
						export_pov.clone(),
					)
					.await
					{
						collations.push(collation);
					}
				}

				if collations.is_empty() {
					return;
				}

				tracing::debug!(
					target: LOG_TARGET,
					?core_index,
					segment_len = collations.len(),
					"Submitting segment for core.",
				);

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
			},
		}
	}
}

/// Mirror the PVF's `signed_scheduling_info` override on the collator side: strip the existing
/// scheduling tail from `upward_messages` (everything from the first `UMP_SEPARATOR` onwards)
/// and re-emit it from `signed_info.payload`. After this, collation-generation's
/// `parse_ump_signals` reads the same `SelectCore`/`ApprovedPeer` the PVF would compute.
fn override_ump_scheduling_tail(
	upward_messages: &mut UpwardMessages,
	signed_info: &SignedSchedulingInfo,
) {
	// Strip everything from the first `UMP_SEPARATOR` onwards (the existing scheduling tail).
	if let Some(pos) = upward_messages.iter().position(|m| m == &UMP_SEPARATOR) {
		for bytes in upward_messages.iter().skip(pos + 1) {
			// NOTE: intentionally exhaustive (no `_` arm), mirroring
			// `SchedulingSignals::from_block_signals`: a new `UMPSignal` variant must fail to
			// compile here, because the truncate below would silently drop it.
			match UMPSignal::decode(&mut &bytes[..]).expect("Failed to decode `UMPSignal`") {
				UMPSignal::SelectCore(..) | UMPSignal::ApprovedPeer(..) => {},
			}
		}
		upward_messages.truncate(pos);
	}

	// Re-emit the tail using the signed info's selector/offset/peer.
	let _ = upward_messages.try_push(UMP_SEPARATOR);
	let _ = upward_messages.try_push(
		UMPSignal::SelectCore(
			signed_info.payload.core_selector,
			ClaimQueueOffset(signed_info.payload.claim_queue_offset),
		)
		.encode(),
	);
	let _ = upward_messages
		.try_push(UMPSignal::ApprovedPeer(signed_info.payload.peer_id.clone()).encode());
}

/// Build one collation from an entry: build the PoV, export it if configured, and look up the
/// session index. Returns `None` if the collation could not be built or the session lookup failed.
async fn build_collation<Block: BlockT, RClient: RelayChainInterface + Clone + 'static>(
	entry: CollatorSegmentEntry<Block>,
	scheduling_proof: Option<SchedulingProof>,
	collator_service: &impl CollatorServiceInterface<Block>,
	relay_client: &RClient,
	export_pov: Option<PathBuf>,
) -> Option<SegmentCollation> {
	let CollatorSegmentEntry {
		relay_parent,
		parent_header,
		blocks,
		proof,
		validation_code_hash,
		validation_data,
	} = entry;

	// Capture the signed scheduling info before `build_multi_block_collation` consumes the proof;
	// used to pre-apply the PVF's UMP-signal override below.
	let scheduling_signals_override =
		scheduling_proof.as_ref().and_then(|p| p.signed_scheduling_info.clone());

	let (mut collation, block_data) = match collator_service.build_multi_block_collation(
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

	// Pre-apply the PVF's UMP-signal override locally. The PVF replaces the block's emitted
	// `SelectCore`/`ApprovedPeer` signals wholesale with the ones in `signed_scheduling_info`
	// (see `cumulus_pallet_parachain_system::validate_block::implementation`). Doing the same
	// rewrite on `collation.upward_messages` here lets collation-generation's `parse_ump_signals`
	// see the post-override signals, so a historical entry whose body committed to a different
	// selector than the segment's `core_index` won't trip `CoreIndexMismatch` at the collator-side
	// pre-check, and the committed commitments match what the PVF produces.
	if let Some(signed_info) = scheduling_signals_override.as_ref() {
		override_ump_scheduling_tail(&mut collation.upward_messages, signed_info);
	}

	block_data.log_size_info();

	if let MaybeCompressedPoV::Compressed(ref pov) = collation.proof_of_validity {
		if let Some(pov_path) = export_pov {
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
				"Failed to fetch session index."
			);
			return None;
		},
	};

	Some(SegmentCollation {
		relay_parent,
		collation,
		validation_code_hash,
		result_sender: None,
		session_index,
		validation_data,
	})
}
