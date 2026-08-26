// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The JAM block-builder task (phase 1).
//!
//! One parachain block per new JAM best block: for every update of the JAM best-block stream
//! the task selects the anchor (parent of best, as in polkajam's `create_refine_context`),
//! derives the parachain timestamp and Aura slot from the anchor's timeslot, builds the block
//! with the shared authoring primitives (proposer + Aura pre-digest and seal), imports it, and
//! hands `(block, proof, context)` to the collation task.
//!
//! Phases 1–3 inject a **mocked** parachain inherent (the runtime still requires
//! `set_validation_data`); the fake relay slot is a pure function of the JAM anchor's timeslot,
//! so importers re-execute it deterministically.

use super::{JamCollatorMessage, LOG_TARGET, jam_slot_as_relay_slot, jam_slot_timestamp};
use crate::common::{
	ConstructNodeRuntimeApi, NodeBlock,
	aura::{AuraIdT, AuraRuntimeApi},
	types::ParachainClient,
};
use codec::Encode;
use cumulus_client_consensus_aura::collator::SlotClaim;
use cumulus_client_parachain_inherent::MockValidationDataInherentDataProvider;
use cumulus_primitives_core::{CollectCollationInfo, RelayParentOffsetApi};
use futures::{StreamExt, channel::mpsc};
use jam_interface::{BlockDesc, JamChainSource};
use jam_types::RefineContext;
use polkadot_primitives::{HeadData, Id as ParaId, UpgradeGoAhead};
use sc_client_api::UsageProvider;
use sc_consensus::{BlockImport, StateAction};
use sc_consensus_aura::standalone as aura_internal;
use sp_api::{ProofRecorder, ProvideRuntimeApi};
use sp_consensus::{Environment, ProposeArgs, Proposer};
use sp_consensus_aura::{AuraApi, Slot};
use sp_externalities::Extensions;
use sp_inherents::{InherentData, InherentDataProvider};
use sp_keystore::KeystorePtr;
use sp_runtime::traits::{Header as HeaderT, UniqueSaturatedInto};
use sp_trie::proof_size_extension::ProofSizeExt;
use std::{sync::Arc, time::Duration};

const PROPOSAL_DURATION: Duration = Duration::from_millis(2000);
/// Phase-1 PoV budget; generous for a mostly-empty test chain, small enough for any JAM
/// work-package size limit.
const MAX_POV_SIZE: usize = 3 * 1024 * 1024;

pub(crate) struct BuilderTaskParams<Block: NodeBlock, RuntimeApi, BI, PF, Jam> {
	pub para_client: Arc<ParachainClient<Block, RuntimeApi>>,
	pub block_import: BI,
	pub proposer_factory: PF,
	pub keystore: KeystorePtr,
	pub para_id: ParaId,
	pub jam: Arc<Jam>,
	pub message_sender: mpsc::Sender<JamCollatorMessage<Block>>,
	pub rebuild_receiver: mpsc::Receiver<()>,
}

/// Run the builder task. Ends (taking the node down, it is an essential task) only if the JAM
/// best-block stream cannot be (re-)established or the collation task is gone.
pub(crate) async fn run_builder_task<Block, RuntimeApi, AuraId, BI, PF, Jam>(
	params: BuilderTaskParams<Block, RuntimeApi, BI, PF, Jam>,
) where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
	BI: BlockImport<Block> + Send + Sync,
	PF: Environment<Block>,
	Jam: JamChainSource,
{
	let BuilderTaskParams {
		para_client,
		mut block_import,
		mut proposer_factory,
		keystore,
		para_id,
		jam,
		mut message_sender,
		mut rebuild_receiver,
	} = params;

	let slot_duration = match sc_consensus_aura::slot_duration(&*para_client) {
		Ok(slot_duration) => slot_duration,
		Err(error) => {
			tracing::error!(target: LOG_TARGET, ?error, "Failed to read the Aura slot duration.");
			return;
		},
	};

	let mut best_blocks = match jam.best_block_stream().await {
		Ok(stream) => stream.fuse(),
		Err(error) => {
			tracing::error!(
				target: LOG_TARGET,
				?error,
				"Unable to subscribe to JAM best blocks."
			);
			return;
		},
	};

	tracing::info!(
		target: LOG_TARGET,
		?para_id,
		slot_duration = slot_duration.as_millis(),
		"JAM builder task started; building one block per JAM best block.",
	);

	let mut last_claimed_slot: Option<Slot> = None;
	loop {
		let jam_best = futures::select! {
			jam_best = best_blocks.next() => match jam_best {
				Some(jam_best) => jam_best,
				None => {
					tracing::error!(target: LOG_TARGET, "JAM best-block stream ended.");
					return;
				},
			},
			rebuild = rebuild_receiver.next() => {
				// Phase-4 seam: nothing sends on this channel in phases 1–3.
				tracing::warn!(
					target: LOG_TARGET,
					?rebuild,
					"Unexpected rebuild request in phase 1; ignoring.",
				);
				continue;
			},
		};

		match build_one_block::<Block, RuntimeApi, AuraId, _, _, _>(
			&para_client,
			&mut block_import,
			&mut proposer_factory,
			&keystore,
			para_id,
			&*jam,
			slot_duration,
			jam_best,
			&mut last_claimed_slot,
		)
		.await
		{
			Ok(Some(message)) => {
				let block_hash = message.block.hash();
				if let Err(error) = message_sender.try_send(message) {
					if error.is_disconnected() {
						tracing::error!(
							target: LOG_TARGET,
							"Collation task is gone; stopping the builder task."
						);
						return;
					}
					tracing::warn!(
						target: LOG_TARGET,
						?block_hash,
						"Collation task is backlogged; dropping this block's collation.",
					);
				}
			},
			Ok(None) => {},
			Err(error) => {
				tracing::warn!(
					target: LOG_TARGET,
					error,
					?jam_best,
					"Failed to build a block for this JAM best block.",
				);
			},
		}
	}
}

async fn build_one_block<Block, RuntimeApi, AuraId, BI, PF, Jam>(
	para_client: &Arc<ParachainClient<Block, RuntimeApi>>,
	block_import: &mut BI,
	proposer_factory: &mut PF,
	keystore: &KeystorePtr,
	para_id: ParaId,
	jam: &Jam,
	slot_duration: sp_consensus_aura::SlotDuration,
	jam_best: BlockDesc,
	last_claimed_slot: &mut Option<Slot>,
) -> Result<Option<JamCollatorMessage<Block>>, String>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
	BI: BlockImport<Block> + Send + Sync,
	PF: Environment<Block>,
	Jam: JamChainSource,
{
	// Anchor selection (polkajam's `create_refine_context`): anchor = parent of best (other
	// nodes may not have seen best yet), lookup anchor = parent of finalized.
	let anchor = jam.parent(jam_best.header_hash).await.map_err(|e| format!("anchor: {e}"))?;
	let state_root = jam
		.state_root(anchor.header_hash)
		.await
		.map_err(|e| format!("state root: {e}"))?;
	let beefy_root = jam
		.beefy_root(anchor.header_hash)
		.await
		.map_err(|e| format!("beefy root: {e}"))?;
	let finalized = jam.finalized_block().await.map_err(|e| format!("finalized: {e}"))?;
	let lookup_anchor = jam
		.parent(finalized.header_hash)
		.await
		.map_err(|e| format!("lookup anchor: {e}"))?;
	let context = RefineContext {
		anchor: anchor.header_hash,
		state_root,
		beefy_root,
		lookup_anchor: lookup_anchor.header_hash,
		lookup_anchor_slot: lookup_anchor.slot,
		prerequisites: Default::default(),
	};

	// The anchor's timeslot decides the slot claim and the (deterministic) timestamp.
	let timestamp = jam_slot_timestamp(anchor.slot);
	let para_slot = Slot::from_timestamp(timestamp, slot_duration);
	if last_claimed_slot.is_some_and(|last| para_slot <= last) {
		tracing::debug!(
			target: LOG_TARGET,
			?para_slot,
			last_claimed_slot = ?*last_claimed_slot,
			anchor_slot = anchor.slot,
			"Parachain slot not advanced yet; skipping this JAM block.",
		);
		return Ok(None);
	}

	let parent_hash = para_client.usage_info().chain.best_hash;
	let parent_header = para_client
		.header(parent_hash)
		.map_err(|e| format!("parent header: {e}"))?
		.ok_or_else(|| format!("parent header {parent_hash:?} not found"))?;

	let authorities = para_client
		.runtime_api()
		.authorities(parent_hash)
		.map_err(|e| format!("authorities: {e}"))?;
	let Some(author_pub) = aura_internal::claim_slot::<<AuraId as AuraIdT>::BoundedPair>(
		para_slot,
		&authorities,
		keystore,
	)
	.await
	else {
		tracing::debug!(
			target: LOG_TARGET,
			?para_slot,
			authorities = authorities.len(),
			"Slot not ours; skipping.",
		);
		return Ok(None);
	};
	let slot_claim =
		SlotClaim::unchecked::<<AuraId as AuraIdT>::BoundedPair>(author_pub, para_slot, timestamp);

	tracing::info!(
		target: LOG_TARGET,
		?jam_best,
		?anchor,
		?finalized,
		?para_slot,
		timestamp = timestamp.as_millis(),
		parent = ?parent_hash,
		parent_number = %parent_header.number(),
		"Building a parachain block against the JAM anchor.",
	);

	let inherent_data = create_inherent_data::<Block, RuntimeApi>(
		para_client,
		para_id,
		&parent_header,
		anchor,
		slot_duration,
	)
	.await?;

	let proposer = proposer_factory
		.init(&parent_header)
		.await
		.map_err(|e| format!("proposer init: {e}"))?;
	let storage_proof_recorder = ProofRecorder::<Block>::default();
	let mut extra_extensions = Extensions::new();
	extra_extensions.register(ProofSizeExt::new(storage_proof_recorder.clone()));

	let proposal = proposer
		.propose(ProposeArgs {
			inherent_data,
			inherent_digests: sp_runtime::generic::Digest {
				logs: vec![slot_claim.pre_digest().clone()],
			},
			max_duration: PROPOSAL_DURATION,
			block_size_limit: Some(MAX_POV_SIZE),
			extra_extensions,
			storage_proof_recorder: Some(storage_proof_recorder.clone()),
		})
		.await
		.map_err(|e| format!("propose: {e}"))?;

	let sealed_importable =
		cumulus_client_consensus_aura::collator::seal::<_, <AuraId as AuraIdT>::BoundedPair>(
			proposal.block,
			proposal.storage_changes,
			slot_claim.author_pub(),
			keystore,
		)
		.map_err(|e| format!("seal: {e}"))?;

	let block = Block::new(
		sealed_importable.post_header(),
		sealed_importable
			.body
			.clone()
			.ok_or_else(|| "sealed block has no body".to_string())?,
	);
	if !matches!(sealed_importable.state_action, StateAction::ApplyChanges(_)) {
		return Err("Building a block should return storage changes".into());
	}
	let proof = storage_proof_recorder.drain_storage_proof();

	block_import
		.import_block(sealed_importable)
		.await
		.map_err(|e| format!("import: {e}"))?;

	*last_claimed_slot = Some(para_slot);
	tracing::info!(
		target: LOG_TARGET,
		block_hash = ?block.hash(),
		block_number = %block.header().number(),
		extrinsics = block.extrinsics().len(),
		proof_nodes = proof.iter_nodes().count(),
		"Built and imported a parachain block.",
	);

	Ok(Some(JamCollatorMessage { parent_header, block, proof, context, triggered_by: jam_best }))
}

/// Timestamp + mocked parachain inherent, both derived from the JAM anchor's timeslot.
///
/// Mirrors omni-node's relay-less dev mode, except the fake relay slot is a pure function of
/// the anchor timeslot instead of the wall clock.
async fn create_inherent_data<Block, RuntimeApi>(
	para_client: &Arc<ParachainClient<Block, RuntimeApi>>,
	para_id: ParaId,
	parent_header: &Block::Header,
	anchor: BlockDesc,
	slot_duration: sp_consensus_aura::SlotDuration,
) -> Result<InherentData, String>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: CollectCollationInfo<Block> + RelayParentOffsetApi<Block>,
{
	const RELAY_CHAIN_SLOT_DURATION_MILLIS: u64 = 6000;

	let parent_hash = parent_header.hash();
	let should_send_go_ahead = para_client
		.runtime_api()
		.collect_collation_info(parent_hash, parent_header)
		.map(|info| info.new_validation_code.is_some())
		.unwrap_or_default();

	let current_para_block =
		UniqueSaturatedInto::<u32>::unique_saturated_into(*parent_header.number()) + 1;
	let relay_parent_offset =
		para_client.runtime_api().relay_parent_offset(parent_hash).unwrap_or_default();
	let relay_blocks_per_para_block =
		(slot_duration.as_millis() / RELAY_CHAIN_SLOT_DURATION_MILLIS).max(1) as u32;
	let target_relay_slot = jam_slot_as_relay_slot(anchor.slot);
	let relay_offset =
		(target_relay_slot as u32).saturating_sub(relay_blocks_per_para_block * current_para_block);

	let mocked_parachain = MockValidationDataInherentDataProvider::<()> {
		current_para_block,
		para_id,
		current_para_block_head: Some(HeadData(parent_header.encode())),
		relay_blocks_per_para_block,
		relay_offset,
		relay_parent_offset,
		para_blocks_per_relay_epoch: 10,
		upgrade_go_ahead: should_send_go_ahead.then(|| {
			tracing::info!(
				target: LOG_TARGET,
				"Detected pending validation code, sending go-ahead signal."
			);
			UpgradeGoAhead::GoAhead
		}),
		..Default::default()
	};

	tracing::debug!(
		target: LOG_TARGET,
		current_para_block,
		target_relay_slot,
		relay_offset,
		relay_blocks_per_para_block,
		relay_parent_offset,
		"Mocked parachain inherent parameters.",
	);

	let timestamp_provider =
		sp_timestamp::InherentDataProvider::new(jam_slot_timestamp(anchor.slot));

	let mut inherent_data = InherentData::new();
	timestamp_provider
		.provide_inherent_data(&mut inherent_data)
		.await
		.map_err(|e| format!("timestamp inherent: {e}"))?;
	mocked_parachain
		.provide_inherent_data(&mut inherent_data)
		.await
		.map_err(|e| format!("mocked parachain inherent: {e}"))?;

	Ok(inherent_data)
}
