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

use codec::Encode;
use cumulus_client_collator::service::ServiceInterface as CollatorServiceInterface;
use cumulus_client_consensus_common::ValidationCodeHashProvider;
use cumulus_client_resubmission_store::ResubmissionStore;
use cumulus_primitives_core::SignedSchedulingInfo;
use cumulus_relay_chain_interface::RelayChainInterface;

use polkadot_node_primitives::{
	MaybeCompressedPoV, SegmentCollation, SubmitCollationParams, SubmitSegmentParams,
	UpwardMessages,
};
use polkadot_node_subsystem::messages::CollationGenerationMessage;
use polkadot_overseer::Handle as OverseerHandle;
use polkadot_primitives::{ClaimQueueOffset, CollatorPair, Id as ParaId, UMPSignal, UMP_SEPARATOR};

use cumulus_primitives_core::relay_chain::BlockId;
use futures::prelude::*;

use crate::export_pov_to_path;
use sc_client_api::{backend::AuxStore, Backend};
use sc_utils::mpsc::TracingUnboundedReceiver;
use sp_runtime::traits::{Block as BlockT, Header};

use super::{
	relay_chain_data_cache::RelayChainDataCache,
	unincluded_segment::{hydrate_segment, SegmentEntry},
	CollatorMessage, CollatorResubmitSegment, SegmentKind,
};

const LOG_TARGET: &str = "aura::cumulus::collation_task";

/// Parameters for the collation task.
pub struct Params<Block: BlockT, B, Client, RClient, CHP, CS> {
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
	/// Parachain backend — used to read unincluded block bodies + parent headers during V3
	/// segment hydration.
	pub para_backend: Arc<B>,
	/// Per-block storage-proof store — used to look up stored proofs during V3 segment hydration.
	pub store: ResubmissionStore<Block, Client>,
	/// Validation code hash provider — used during V3 segment hydration.
	pub code_hash_provider: CHP,
}

/// Asynchronously executes the collation task for a parachain.
///
/// This function initializes the collator subsystems necessary for producing and submitting
/// collations to the relay chain. It listens for new best relay chain block notifications and
/// handles collator messages. If our parachain is scheduled on a core and we have a candidate,
/// the task will build a collation and send it to the relay chain.
pub async fn run_collation_task<Block, B, Client, RClient, CHP, CS>(
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
		para_backend,
		store,
		code_hash_provider,
	}: Params<Block, B, Client, RClient, CHP, CS>,
) where
	Block: BlockT,
	B: Backend<Block> + 'static,
	Client: AuxStore + Send + Sync + 'static,
	CHP: ValidationCodeHashProvider<Block::Hash> + Send + Sync + 'static,
	CS: CollatorServiceInterface<Block> + Send + Sync + 'static,
	RClient: RelayChainInterface + Clone + 'static,
{
	let mut relay_chain_data_cache = RelayChainDataCache::new(relay_client.clone(), para_id);
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

				// Hydrate the unincluded segment locally and ship as a SubmitSegment. The block
				// builder only sends headers across; hydration (body + storage proof + relay
				// data + code hash) happens here via `hydrate_segment`.
				handle_resubmit_segment::<Block, _, _, _, _>(
					message,
					&collator_service,
					&mut overseer_handle,
					relay_client.clone(),
					export_pov.clone(),
					&*para_backend,
					&store,
					&code_hash_provider,
					&mut relay_chain_data_cache,
				).await;
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

/// Handle an incoming segment message from the block builder task. The message's
/// `unincluded_segment` headers are hydrated locally via [`hydrate_segment`] (body + storage
/// proof + relay data + validation-code hash), then each is built into a [`SegmentCollation`]
/// and forwarded with the optional bundle's collation to the collation-generation subsystem as
/// a single [`CollationGenerationMessage::SubmitSegment`]. Entries that fail to hydrate or build
/// are skipped — they do not abort the whole segment.
#[allow(clippy::too_many_arguments)]
async fn handle_resubmit_segment<Block, B, Client, RClient, CHP>(
	message: CollatorResubmitSegment<Block>,
	collator_service: &impl CollatorServiceInterface<Block>,
	overseer_handle: &mut OverseerHandle,
	relay_client: RClient,
	export_pov: Option<PathBuf>,
	para_backend: &B,
	store: &ResubmissionStore<Block, Client>,
	code_hash_provider: &CHP,
	relay_chain_data_cache: &mut RelayChainDataCache<RClient>,
) where
	Block: BlockT,
	B: Backend<Block>,
	Client: AuxStore,
	RClient: RelayChainInterface + Clone + 'static,
	CHP: ValidationCodeHashProvider<Block::Hash>,
{
	let CollatorResubmitSegment { scheduling_proof, kind, unincluded_segment } = message;
	let scheduling_parent = scheduling_proof.scheduling_parent();
	let (core_index, bundle) = match kind {
		SegmentKind::WithBundle { bundle } => (bundle.core_index, Some(bundle)),
		SegmentKind::ResubmitOnly { core_index } => (core_index, None),
	};

	let entries = hydrate_segment(
		unincluded_segment,
		para_backend,
		code_hash_provider,
		store,
		relay_chain_data_cache,
	)
	.await;

	let mut collations = Vec::with_capacity(entries.len() + bundle.is_some() as usize);

	for entry in entries {
		let SegmentEntry {
			relay_parent,
			parent_header,
			blocks,
			proof,
			validation_code_hash,
			validation_data,
		} = entry;
		if let Some(collation) = build_segment_collation::<Block, _>(
			collator_service,
			&relay_client,
			&scheduling_proof,
			export_pov.clone(),
			core_index,
			relay_parent,
			parent_header,
			blocks,
			proof,
			validation_code_hash,
			validation_data,
		)
		.await
		{
			collations.push(collation);
		}
	}

	if let Some(bundle) = bundle {
		let CollatorMessage {
			relay_parent,
			parent_header,
			blocks,
			proof,
			validation_code_hash,
			core_index: _,
			validation_data,
		} = bundle;
		if let Some(collation) = build_segment_collation::<Block, _>(
			collator_service,
			&relay_client,
			&scheduling_proof,
			export_pov,
			core_index,
			relay_parent,
			parent_header,
			blocks,
			proof,
			validation_code_hash,
			validation_data,
		)
		.await
		{
			collations.push(collation);
		}
	}

	if collations.is_empty() {
		return;
	}

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

/// Build a single [`SegmentCollation`] from one bundle's worth of inputs. Returns `None` if the
/// collator service fails to assemble the collation or if the session-index lookup fails.
#[allow(clippy::too_many_arguments)]
async fn build_segment_collation<Block: BlockT, RClient: RelayChainInterface + Clone + 'static>(
	collator_service: &impl CollatorServiceInterface<Block>,
	relay_client: &RClient,
	scheduling_proof: &cumulus_primitives_core::SchedulingProof,
	export_pov: Option<PathBuf>,
	core_index: polkadot_primitives::CoreIndex,
	relay_parent: cumulus_primitives_core::relay_chain::Hash,
	parent_header: Block::Header,
	blocks: Vec<Block>,
	proof: sp_api::StorageProof,
	validation_code_hash: polkadot_primitives::ValidationCodeHash,
	validation_data: cumulus_primitives_core::PersistedValidationData,
) -> Option<SegmentCollation> {
	let (mut collation, block_data) = collator_service.build_multi_block_collation(
		&parent_header,
		blocks,
		proof,
		Some(scheduling_proof.clone()),
	)?;

	// Pre-apply the PVF's UMP-signal override locally. The PVF replaces the block's emitted
	// `SelectCore`/`ApprovedPeer` signals wholesale with the ones in `signed_scheduling_info`
	// (see `cumulus_pallet_parachain_system::validate_block::implementation`). Doing the same
	// rewrite on `collation.upward_messages` here lets collation-generation's
	// `parse_ump_signals` see the post-override signals, so a historical entry whose body
	// committed to a different selector than the segment's `core_index` won't trip
	// `CoreIndexMismatch` at the collator-side pre-check.
	if let Some(signed_info) = scheduling_proof.signed_scheduling_info.as_ref() {
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
		Ok(s) => s,
		Err(err) => {
			tracing::error!(
				target: LOG_TARGET,
				?err,
				?relay_parent,
				"Failed to fetch session index for segment entry.",
			);
			return None;
		},
	};

	tracing::debug!(
		target: LOG_TARGET,
		?core_index,
		?relay_parent,
		block_numbers = ?block_data.blocks().iter().map(|b| *b.header().number()).collect::<Vec<_>>(),
		"Adding entry to segment for core.",
	);

	Some(SegmentCollation {
		relay_parent,
		collation,
		validation_code_hash,
		result_sender: None,
		session_index,
		validation_data,
	})
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
