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

use super::{
	JamCollatorMessage, LOG_TARGET, fetch_anchor_state_proof, jam_slot_as_relay_slot,
	jam_slot_timestamp,
};
use crate::common::{
	ConstructNodeRuntimeApi, NodeBlock,
	aura::{AuraIdT, AuraRuntimeApi},
	types::ParachainClient,
};
use codec::{Decode, Encode};
use cumulus_client_consensus_aura::collator::SlotClaim;
use cumulus_client_parachain_inherent::MockValidationDataInherentDataProvider;
use cumulus_primitives_core::{CollectCollationInfo, RelayParentOffsetApi};
use futures::{StreamExt, channel::mpsc};
use jam_cumulus_facade::service_state::{ParaInfo, para_info_key};
use jam_interface::{BlockDesc, JamChainSource, JamStateSource, ServiceId, Slot as JamSlot};
use jam_types::RefineContext;
use polkadot_primitives::{HeadData, Id as ParaId, UpgradeGoAhead};
use sc_consensus::{BlockImport, StateAction};
use sc_consensus_aura::standalone as aura_internal;
use sp_api::{ProofRecorder, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
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
/// How long to leave an authored block alone before authoring on its parent again.
///
/// One block per included head is the rule; a head that has not moved after this many JAM slots
/// means the package presumably never landed, and the only way out is to author again.
const RETRY_AFTER_SLOTS: JamSlot = 5;

pub(crate) struct BuilderTaskParams<Block: NodeBlock, RuntimeApi, BI, PF, Jam> {
	pub para_client: Arc<ParachainClient<Block, RuntimeApi>>,
	pub block_import: BI,
	pub proposer_factory: PF,
	pub keystore: KeystorePtr,
	pub para_id: ParaId,
	pub service_id: ServiceId,
	pub jam: Arc<Jam>,
	pub message_sender: mpsc::Sender<JamCollatorMessage<Block>>,
	pub rebuild_receiver: mpsc::Receiver<()>,
}

/// What the builder remembers between iterations.
struct BuilderState<Hash> {
	/// Aura guard: the parachain slot last claimed.
	last_claimed_slot: Option<Slot>,
	/// The parent the last block was built on, and the JAM slot it was built at.
	last_built: Option<(Hash, JamSlot)>,
	/// The parent chosen on the previous iteration, so an unchanged choice logs quietly.
	last_selected_parent: Option<Hash>,
}

/// Whether to author on the selected parent now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuildDecision {
	/// The parent moved (or this is the first block): author.
	Build,
	/// The same parent as last time, but long enough ago that the package is presumed lost.
	Retry,
	/// Already authored on this parent; wait for it to be included.
	Skip,
}

/// One block per included head, with a retry once the included head has visibly stalled.
///
/// Without the retry a single dropped work package would stall the para forever: the head never
/// moves, so the parent never changes, so nothing is ever authored again.
fn pacing_decision<Hash: PartialEq>(
	last_built: Option<&(Hash, JamSlot)>,
	parent: &Hash,
	slot: JamSlot,
) -> BuildDecision {
	match last_built {
		Some((last_parent, last_slot)) if last_parent == parent => {
			if slot.saturating_sub(*last_slot) >= RETRY_AFTER_SLOTS {
				BuildDecision::Retry
			} else {
				BuildDecision::Skip
			}
		},
		_ => BuildDecision::Build,
	}
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
	Jam: JamChainSource + JamStateSource,
{
	let BuilderTaskParams {
		para_client,
		mut block_import,
		mut proposer_factory,
		keystore,
		para_id,
		service_id,
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

	let mut state =
		BuilderState { last_claimed_slot: None, last_built: None, last_selected_parent: None };
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
			service_id,
			&*jam,
			slot_duration,
			jam_best,
			&mut state,
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
	service_id: ServiceId,
	jam: &Jam,
	slot_duration: sp_consensus_aura::SlotDuration,
	jam_best: BlockDesc,
	state: &mut BuilderState<Block::Hash>,
) -> Result<Option<JamCollatorMessage<Block>>, String>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
	BI: BlockImport<Block> + Send + Sync,
	PF: Environment<Block>,
	Jam: JamChainSource + JamStateSource,
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
	if state.last_claimed_slot.is_some_and(|last| para_slot <= last) {
		tracing::debug!(
			target: LOG_TARGET,
			?para_slot,
			last_claimed_slot = ?state.last_claimed_slot,
			anchor_slot = anchor.slot,
			"Parachain slot not advanced yet; skipping this JAM block.",
		);
		return Ok(None);
	}

	// Build on the head JAM has *included*, not on local best: a block chained onto anything else
	// is one the parachain service will refuse, and a package that stalls heals only if the
	// builder keeps returning to the stalled head.
	let para_id_u32: u32 = para_id.into();
	let included = jam
		.service_value(anchor.header_hash, service_id, &para_info_key(para_id_u32.into()))
		.await
		.map_err(|e| format!("included head: {e}"))?;
	let included_head = match &included {
		Some(bytes) => {
			let info =
				ParaInfo::decode(&mut &bytes[..]).map_err(|e| format!("included ParaInfo: {e}"))?;
			let head = info.head_data.into_inner();
			Some(
				<Block::Header as Decode>::decode(&mut &head[..])
					.map_err(|e| format!("included head data: {e}"))?,
			)
		},
		None => None,
	};

	let parent_header = match &included_head {
		Some(header) => {
			let hash = header.hash();
			if para_client.header(hash).map_err(|e| format!("included head: {e}"))?.is_none() {
				tracing::warn!(
					target: LOG_TARGET,
					included_head = ?hash,
					included_number = %header.number(),
					"Included head is not known locally; waiting for import/sync.",
				);
				return Ok(None);
			}
			header.clone()
		},
		None => {
			// Nothing included for this para yet, so the next block is its first one.
			let genesis_hash = para_client.info().genesis_hash;
			para_client
				.header(genesis_hash)
				.map_err(|e| format!("genesis header: {e}"))?
				.ok_or_else(|| format!("genesis header {genesis_hash:?} not found"))?
		},
	};
	let parent_hash = parent_header.hash();

	if state.last_selected_parent.replace(parent_hash) == Some(parent_hash) {
		tracing::debug!(target: LOG_TARGET, parent = ?parent_hash, "Parent unchanged.");
	} else {
		tracing::info!(
			target: LOG_TARGET,
			included = included_head.is_some(),
			parent = ?parent_hash,
			parent_number = %parent_header.number(),
			anchor_slot = anchor.slot,
			"Parent selected from the para head included in JAM state.",
		);
	}

	match pacing_decision(state.last_built.as_ref(), &parent_hash, anchor.slot) {
		BuildDecision::Build => {},
		BuildDecision::Retry => tracing::warn!(
			target: LOG_TARGET,
			parent = ?parent_hash,
			anchor_slot = anchor.slot,
			retry_after_slots = RETRY_AFTER_SLOTS,
			"The included head has not moved since the last build; the work package presumably \
			 never landed. Authoring on the same parent again.",
		),
		BuildDecision::Skip => {
			tracing::debug!(
				target: LOG_TARGET,
				parent = ?parent_hash,
				anchor_slot = anchor.slot,
				"Already authored on this parent; waiting for it to be included.",
			);
			return Ok(None);
		},
	}

	let (anchor_state_proof, proved_head) =
		fetch_anchor_state_proof(jam, anchor.header_hash, &state_root, service_id, para_id_u32)
			.await?;
	// The proof and the head read above describe the same key at the same anchor, so anything
	// but equality means one of the two reads is stale — shipping it would only earn a refine
	// rejection.
	if proved_head != included {
		tracing::error!(
			target: LOG_TARGET,
			anchor = ?anchor.header_hash,
			proved_head = proved_head.is_some(),
			read_head = included.is_some(),
			"The anchor state proof disagrees with the para head read at the same anchor; \
			 skipping this JAM block.",
		);
		return Ok(None);
	}

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

	state.last_claimed_slot = Some(para_slot);
	state.last_built = Some((parent_hash, anchor.slot));
	tracing::info!(
		target: LOG_TARGET,
		block_hash = ?block.hash(),
		block_number = %block.header().number(),
		extrinsics = block.extrinsics().len(),
		proof_nodes = proof.iter_nodes().count(),
		anchor_proof_nodes = anchor_state_proof.nodes.len(),
		"Built and imported a parachain block.",
	);

	Ok(Some(JamCollatorMessage {
		parent_header,
		block,
		proof,
		context,
		anchor_state_root: *state_root,
		anchor_state_proof,
		triggered_by: jam_best,
	}))
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

#[cfg(test)]
mod tests {
	use super::*;

	/// A head that has just moved is authored on immediately — that is the normal case, one
	/// parachain block per included head.
	#[test]
	fn a_new_parent_is_built_on_at_once() {
		assert_eq!(pacing_decision::<u8>(None, &1, 100), BuildDecision::Build);
		assert_eq!(pacing_decision(Some(&(1u8, 100)), &2, 101), BuildDecision::Build);
	}

	/// Authoring twice on the same parent would orphan the first block, so the builder waits
	/// for the head to move instead.
	#[test]
	fn the_same_parent_is_not_built_on_twice() {
		assert_eq!(pacing_decision(Some(&(1u8, 100)), &1, 100), BuildDecision::Skip);
		assert_eq!(
			pacing_decision(Some(&(1u8, 100)), &1, 100 + RETRY_AFTER_SLOTS - 1),
			BuildDecision::Skip
		);
	}

	/// ...but not forever: a dropped work package leaves the head where it is, and only a
	/// rebuild on that same parent can heal the chain.
	#[test]
	fn a_stalled_parent_is_retried() {
		assert_eq!(
			pacing_decision(Some(&(1u8, 100)), &1, 100 + RETRY_AFTER_SLOTS),
			BuildDecision::Retry
		);
	}
}
