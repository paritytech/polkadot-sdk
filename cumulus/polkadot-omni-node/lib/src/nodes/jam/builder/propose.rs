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

//! Building, sealing and importing one block, and the runtime's gate on whether another one fits.

use crate::{
	common::{
		aura::{AuraIdT, AuraRuntimeApi},
		types::JamClient,
		ConstructNodeRuntimeApi, NodeBlock,
	},
	nodes::jam::{
		authorizer::AuraPublic, builder::inherent::create_inherent_data,
		chain::anchor::register_jam_state_reader,
	},
};
use cumulus_client_consensus_aura::collator::SlotClaim;
use cumulus_primitives_aura::AuraUnincludedSegmentApi;
use cumulus_primitives_core::BlockBundleInfo;
use jam_types::RefineContext;
use parachain_service_core::StateProof;
use sc_consensus::{BlockImport, StateAction};
use sp_additional_data::AdditionalData;
use sp_api::{ProofRecorder, ProvideRuntimeApi, StorageProof};
use sp_consensus::{Environment, ProposeArgs, Proposer};
use sp_consensus_aura::Slot;
use sp_externalities::Extensions;
use sp_keystore::KeystorePtr;
use sp_timestamp::Timestamp;
use sp_trie::proof_size_extension::ProofSizeExt;
use std::{sync::Arc, time::Duration};

const PROPOSAL_DURATION: Duration = Duration::from_millis(2000);
/// JAM work-package PoV budget for a parachain collation.
///
/// `jam_types::max_input()` (the Gray Paper's Wb) is 13,791,360 bytes. 12 MiB (12,582,912 bytes)
/// leaves 1,208,448 bytes — ~1.15 MiB — for the work-package header, authorizer config,
/// extrinsics index and other per-package fields.
const MAX_POV_SIZE: usize = 12 * 1024 * 1024;

/// Build one block on `parent_header`, seal it with the key `slot_claim` names and import it.
///
/// Hands back the block, the storage proof it executed with and the additional-data map it
/// carries.
pub(crate) async fn build_seal_import<Block, AuraId, BI, PF>(
	block_import: &mut BI,
	proposer_factory: &mut PF,
	keystore: &KeystorePtr,
	parent_header: &Block::Header,
	block_timestamp: Timestamp,
	context: &RefineContext,
	anchor_state_proof: &StateProof,
	slot_claim: &SlotClaim<AuraPublic<AuraId>>,
) -> Result<(Block, StorageProof, AdditionalData), String>
where
	Block: NodeBlock,
	AuraId: AuraIdT + Sync,
	BI: BlockImport<Block> + Send + Sync,
	PF: Environment<Block>,
{
	let inherent_data = create_inherent_data(block_timestamp).await?;

	let proposer = proposer_factory
		.init(parent_header)
		.await
		.map_err(|e| format!("proposer init: {e}"))?;
	let storage_proof_recorder = ProofRecorder::<Block>::default();
	let mut extra_extensions = Extensions::new();
	extra_extensions.register(ProofSizeExt::new(storage_proof_recorder.clone()));
	let additional_data = register_jam_state_reader(
		&mut extra_extensions,
		*context.state_root,
		anchor_state_proof.clone(),
	);

	let proposal = proposer
		.propose(ProposeArgs {
			inherent_data,
			inherent_digests: sp_runtime::generic::Digest {
				logs: vec![
					slot_claim.pre_digest().clone(),
					cumulus_primitives_core::CumulusDigestItem::JamParent {
						anchor: context.anchor.0.into(),
						anchor_slot: context.anchor_slot,
						lookup_anchor: context.lookup_anchor.0.into(),
						lookup_anchor_slot: context.lookup_anchor_slot,
					}
					.to_digest_item(),
					// Every block is alone in its work package, so it is the first and
					// the last block of its core: the runtime grants it the full core.
					BlockBundleInfo { index: 0, is_last: true }.to_digest_item(),
				],
			},
			max_duration: PROPOSAL_DURATION,
			block_size_limit: Some(MAX_POV_SIZE),
			extra_extensions,
			storage_proof_recorder: Some(storage_proof_recorder.clone()),
		})
		.await
		.map_err(|e| format!("propose: {e}"))?;

	let mut sealed_importable =
		cumulus_client_consensus_aura::collator::seal::<_, <AuraId as AuraIdT>::BoundedPair>(
			proposal.block,
			proposal.storage_changes,
			slot_claim.author_pub(),
			keystore,
		)
		.map_err(|e| format!("seal: {e}"))?;

	// Mirror the relay collator's `build_block_and_import` (`collator.rs`): carry the
	// built additional-data blob (the `JAM_PROOF_KEY` entry for this block's reads) on
	// the sealed import, so the importing path and peer sync serve `jam_state_read`
	// instead of trapping.
	sealed_importable.additional_data = Some(additional_data.clone());

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

	Ok::<_, String>((block, proof, additional_data))
}

/// Ask the runtime whether one more block may be chained onto `parent`.
///
/// Capacity and velocity are runtime-owned (`FixedVelocityConsensusHook`), and on JAM there is no
/// second, relay-side depth cap behind them — this answer is the only hard limit there is.
/// Mirrors `cumulus_client_consensus_aura::collators::can_build_upon`: with an empty segment the
/// runtime cannot recognise the parent as the included block (it never sees its own hash), so
/// that case is decided here. The para slot is passed as the API's slot; the JAM hook compares it
/// with the Aura slot of the last anchor.
pub(crate) fn can_build_upon<Block, RuntimeApi, AuraId>(
	para_client: &Arc<JamClient<Block, RuntimeApi>>,
	parent_hash: Block::Hash,
	included_hash: Block::Hash,
	para_slot: Slot,
) -> Result<bool, String>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, JamClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
{
	if parent_hash == included_hash {
		return Ok(true);
	}
	para_client
		.runtime_api()
		.can_build_upon(parent_hash, included_hash, para_slot)
		.map_err(|e| format!("can_build_upon: {e}"))
}
