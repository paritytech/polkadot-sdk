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

//! The JAM collation task (phase 1): the work-package lifecycle manager.
//!
//! For every block from the builder it assembles the work package (payload = SCALE-encoded
//! `ParachainCandidate` carrying the uncompressed PoV), submits it to the fixed core, and spawns
//! a follower that tracks `workPackageStatus` and drives resubmission through the pluggable
//! [`ResubmissionPolicy`]. "Done" has no direct signal — the task watches the para-head entry in
//! the parachain service's state and reports when a submitted block lands.
//!
//! The PoV is a V3 `ParachainBlockData` whose single additional-data slot carries the anchor
//! state proof. Because that proof is verified in-core against the refine context's state root,
//! anchor and PoV are inseparable: re-contexting a package means re-proving the para head and
//! rebuilding the payload, not just swapping the context out.
//!
//! Phase-1 simplifications: null authorizer (empty token, nothing to sign), fixed core, PoV is
//! NOT zstd-compressed (parasim rejects compressed PoVs; JIP-2 is silent on compression).

use super::{
	ANCHOR_STATE_PROOF_KEY, JamCollatorMessage, LOG_TARGET, fetch_anchor_state_proof,
	para_head_stream, resubmission::*,
};
use crate::common::{ConstructNodeRuntimeApi, NodeBlock, types::ParachainClient};
use codec::{Decode, Encode};
use cumulus_primitives_core::{AdditionalData, ParachainBlockData, SchedulingProof};
use futures::{StreamExt, channel::mpsc};
use jam_cumulus_facade::{ParachainCandidate, authorizer::fixed_authorizer};
use jam_interface::{
	CoreIndex, JamChainSource, JamStateSource, JamWorkPackageSubmission, ServiceId,
	VersionedParameters, WorkPackage, WorkPackageHash,
};
use jam_state_helpers::StateProof;
use jam_types::{Authorization, CodeHash, RefineContext, UnsignedGas, WorkItem, WorkPayload};
use polkadot_primitives::Id as ParaId;
use sp_core::traits::SpawnNamed;
use sp_runtime::traits::{Block as BlockT, HashingFor, Header as HeaderT};
use sp_trie::CompactProof;
use std::{
	collections::HashSet,
	sync::{Arc, Mutex},
	time::Duration,
};

const RETRY_DELAY: Duration = Duration::from_secs(6);

pub(crate) struct CollationTaskParams<Block: NodeBlock, RuntimeApi, Jam> {
	pub para_client: Arc<ParachainClient<Block, RuntimeApi>>,
	pub jam: Arc<Jam>,
	pub para_id: ParaId,
	pub service_id: ServiceId,
	pub core: CoreIndex,
	pub message_receiver: mpsc::Receiver<JamCollatorMessage<Block>>,
	/// Phase-4 seam: ask the builder for a rebuild on anchor loss. Never used in phases 1–3
	/// (nothing ties a block to its anchor yet — re-contexting is always enough).
	pub rebuild_sender: mpsc::Sender<()>,
	pub announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	pub spawner: Box<dyn SpawnNamed>,
	pub max_resubmits: u32,
}

pub(crate) async fn run_collation_task<Block, RuntimeApi, Jam>(
	params: CollationTaskParams<Block, RuntimeApi, Jam>,
) where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	Jam: JamChainSource + JamStateSource + JamWorkPackageSubmission + 'static,
{
	let CollationTaskParams {
		para_client,
		jam,
		para_id,
		service_id,
		core,
		mut message_receiver,
		rebuild_sender: _rebuild_sender,
		announce_block,
		spawner,
		max_resubmits,
	} = params;

	let (refine_gas_limit, accumulate_gas_limit) = loop {
		match jam.parameters().await {
			Ok(VersionedParameters::V1(parameters)) => {
				break (parameters.max_refine_gas, parameters.max_accumulate_gas);
			},
			Err(error) => {
				tracing::warn!(
					target: LOG_TARGET,
					?error,
					"Unable to fetch JAM chain parameters; retrying.",
				);
				tokio::time::sleep(RETRY_DELAY).await;
			},
		}
	};

	let service_code_hash = loop {
		let result = match jam.best_block().await {
			Ok(best) => jam.service_info(best.header_hash, service_id).await,
			Err(error) => Err(error),
		};
		match result {
			Ok(Some(service)) => {
				tracing::info!(
					target: LOG_TARGET,
					service_id,
					code_hash = ?service.code_hash,
					balance = service.balance,
					"Found the parachain service on JAM.",
				);
				break service.code_hash;
			},
			Ok(None) => {
				tracing::info!(
					target: LOG_TARGET,
					service_id,
					"Parachain service not registered on JAM yet; waiting.",
				);
				tokio::time::sleep(RETRY_DELAY).await;
			},
			Err(error) => {
				tracing::warn!(
					target: LOG_TARGET,
					service_id,
					?error,
					"Unable to read the parachain service info; retrying.",
				);
				tokio::time::sleep(RETRY_DELAY).await;
			},
		}
	};

	// Every para block whose work package was submitted; the head watcher reports when one of
	// them shows up as the para head in JAM state.
	let submitted_blocks: Arc<Mutex<HashSet<Block::Hash>>> = Default::default();

	let mut para_heads = match para_head_stream(&*jam, service_id, para_id.into(), false).await {
		Ok(stream) => stream.boxed().fuse(),
		Err(error) => {
			tracing::error!(target: LOG_TARGET, ?error, "Unable to watch the para head.");
			return;
		},
	};

	tracing::info!(
		target: LOG_TARGET,
		?para_id,
		service_id,
		core,
		refine_gas_limit,
		accumulate_gas_limit,
		max_resubmits,
		"JAM collation task started.",
	);

	loop {
		futures::select! {
			message = message_receiver.next() => {
				let Some(message) = message else {
					tracing::error!(target: LOG_TARGET, "Builder task is gone; stopping.");
					return;
				};
				handle_new_block(
					&para_client,
					&jam,
					para_id,
					service_id,
					core,
					service_code_hash,
					refine_gas_limit,
					accumulate_gas_limit,
					max_resubmits,
					message,
					&announce_block,
					&spawner,
					&submitted_blocks,
				);
			},
			head = para_heads.next() => {
				let Some(head) = head else {
					tracing::error!(target: LOG_TARGET, "Para-head stream ended; stopping.");
					return;
				};
				report_new_para_head::<Block>(&head, &submitted_blocks);
			},
		}
	}
}

fn report_new_para_head<Block: BlockT>(
	head: &[u8],
	submitted_blocks: &Arc<Mutex<HashSet<Block::Hash>>>,
) {
	match Block::Header::decode(&mut &head[..]) {
		Ok(header) => {
			let hash = header.hash();
			let ours =
				submitted_blocks.lock().expect("submitted-blocks lock poisoned").contains(&hash);
			tracing::info!(
				target: LOG_TARGET,
				block_hash = ?hash,
				block_number = %header.number(),
				submitted_by_us = ours,
				"Para head advanced in JAM state.",
			);
		},
		Err(error) => tracing::warn!(
			target: LOG_TARGET,
			?error,
			head = ?format!("0x{}", hex_prefix(head)),
			"Para head in JAM state does not decode as a header.",
		),
	}
}

fn hex_prefix(bytes: &[u8]) -> String {
	bytes.iter().take(32).map(|byte| format!("{byte:02x}")).collect()
}

fn handle_new_block<Block, RuntimeApi, Jam>(
	para_client: &Arc<ParachainClient<Block, RuntimeApi>>,
	jam: &Arc<Jam>,
	para_id: ParaId,
	service_id: ServiceId,
	core: CoreIndex,
	service_code_hash: CodeHash,
	refine_gas_limit: UnsignedGas,
	accumulate_gas_limit: UnsignedGas,
	max_resubmits: u32,
	message: JamCollatorMessage<Block>,
	announce_block: &Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	spawner: &Box<dyn SpawnNamed>,
	submitted_blocks: &Arc<Mutex<HashSet<Block::Hash>>>,
) where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	Jam: JamChainSource + JamStateSource + JamWorkPackageSubmission + 'static,
{
	let JamCollatorMessage {
		parent_header,
		block,
		proof,
		context,
		anchor_state_root,
		anchor_state_proof,
		triggered_by,
	} = message;
	let block_hash = block.hash();
	let block_number = *block.header().number();

	let compact_proof =
		match proof.into_compact_proof::<HashingFor<Block>>(*parent_header.state_root()) {
			Ok(compact_proof) => compact_proof,
			Err(error) => {
				tracing::error!(
					target: LOG_TARGET,
					?block_hash,
					?error,
					"Failed to compact the storage proof; dropping the block.",
				);
				return;
			},
		};

	let validation_code = match para_client.code_at(parent_header.hash()) {
		Ok(code) => code,
		Err(error) => {
			tracing::error!(
				target: LOG_TARGET,
				?block_hash,
				?error,
				"Failed to read the validation code; dropping the block.",
			);
			return;
		},
	};

	let source = PackageSource {
		blocks: vec![block],
		proof: compact_proof,
		validation_code_hash: sp_crypto_hashing::blake2_256(&validation_code),
		service_id,
		service_code_hash,
		refine_gas_limit,
		accumulate_gas_limit,
	};
	let anchored =
		Anchored { context, state_root: anchor_state_root, head_proof: anchor_state_proof };

	tracing::info!(
		target: LOG_TARGET,
		?block_hash,
		%block_number,
		anchor = ?anchored.context.anchor,
		?triggered_by,
		core,
		"Prepared a candidate for submission.",
	);

	submitted_blocks
		.lock()
		.expect("submitted-blocks lock poisoned")
		.insert(block_hash);
	announce_block(block_hash, None);

	let jam = jam.clone();
	let policy = RecontextOnFailure::new(max_resubmits);
	spawner.spawn(
		"jam-work-package-follow",
		Some("jam-collator"),
		Box::pin(async move {
			submit_and_follow::<Block, _, _>(
				&*jam,
				core,
				para_id.into(),
				source,
				anchored,
				block_hash,
				policy,
			)
			.await;
		}),
	);
}

/// The parts of a work package that survive a change of anchor: the built block(s), the
/// parachain storage proof witnessing them, and the work-item settings.
struct PackageSource<Block> {
	blocks: Vec<Block>,
	proof: CompactProof,
	validation_code_hash: [u8; 32],
	service_id: ServiceId,
	service_code_hash: CodeHash,
	refine_gas_limit: UnsignedGas,
	accumulate_gas_limit: UnsignedGas,
}

/// A refine context together with the anchor state proof that has to travel with it.
///
/// The two are inseparable: the service verifies the proof in-core against the context's state
/// root, so a package cannot keep its payload when it changes anchor.
struct Anchored {
	context: RefineContext,
	state_root: [u8; 32],
	head_proof: StateProof,
}

impl<Block: BlockT> PackageSource<Block> {
	/// Assemble the work package for `anchored`.
	fn package(&self, anchored: &Anchored) -> WorkPackage {
		let payload = ParachainCandidate {
			validation_code_hash: jam_cumulus_facade::ValidationCodeHash(
				self.validation_code_hash.into(),
			),
			pov: build_pov(
				&self.blocks,
				&self.proof,
				anchored.state_root,
				&anchored.head_proof,
			),
		}
		.encode();

		let work_item = WorkItem {
			service: self.service_id,
			code_hash: self.service_code_hash,
			payload: WorkPayload(payload),
			refine_gas_limit: self.refine_gas_limit,
			accumulate_gas_limit: self.accumulate_gas_limit,
			import_segments: Default::default(),
			extrinsics: Default::default(),
			export_count: 0,
		};
		WorkPackage {
			authorization: Authorization::default(),
			auth_code_host: 0,
			authorizer: fixed_authorizer(),
			context: anchored.context.clone(),
			items: vec![work_item].try_into().expect("a single work item always fits; qed"),
		}
	}
}

/// The PoV: a V3 [`ParachainBlockData`] whose single additional-data slot carries the
/// SCALE-encoded `(anchor_state_root, StateProof)` pair the service needs to establish what the
/// para's previous head was.
///
/// The scheduling proof is empty — JAM has no relay-chain scheduling, and the field only exists
/// because V3 extends V2. The PoV is not zstd-compressed; JIP-2 is silent on compression and the
/// service refuses compressed PoVs.
fn build_pov<Block: BlockT>(
	blocks: &[Block],
	proof: &CompactProof,
	anchor_state_root: [u8; 32],
	head_proof: &StateProof,
) -> Vec<u8> {
	let mut additional_data = AdditionalData::new();
	additional_data
		.insert(ANCHOR_STATE_PROOF_KEY.into(), (anchor_state_root, head_proof).encode());

	ParachainBlockData::V3 {
		blocks: blocks.to_vec(),
		proof: proof.clone(),
		scheduling_proof: SchedulingProof::empty(),
		additional_data: vec![Some(additional_data)],
	}
	.encode()
}

/// Submit the package and follow its status until it is reported, abandoning or re-anchoring
/// (fresh anchor, fresh proof, same block) as the policy dictates.
async fn submit_and_follow<Block, Jam, Policy>(
	jam: &Jam,
	core: CoreIndex,
	para_id: u32,
	source: PackageSource<Block>,
	mut anchored: Anchored,
	block_hash: Block::Hash,
	mut policy: Policy,
) where
	Block: BlockT,
	Jam: JamChainSource + JamStateSource + JamWorkPackageSubmission + ?Sized,
	Policy: ResubmissionPolicy,
{
	loop {
		let package = source.package(&anchored);
		let package_hash = WorkPackageHash::from(sp_crypto_hashing::blake2_256(
			&jam_codec::Encode::encode(&package),
		));
		tracing::info!(
			target: LOG_TARGET,
			?block_hash,
			?package_hash,
			core,
			anchor = ?anchored.context.anchor,
			pov_len = package.items[0].payload.0.len(),
			anchor_proof_nodes = anchored.head_proof.nodes.len(),
			"Assembled a work package for the block.",
		);

		if let Err(error) = jam.submit_work_package(core, &package, Vec::new()).await {
			tracing::warn!(
				target: LOG_TARGET,
				?block_hash,
				?package_hash,
				?error,
				"Work-package submission failed.",
			);
			match policy.on_stream_closed() {
				PolicyAction::Resubmit | PolicyAction::Wait => {
					tokio::time::sleep(RETRY_DELAY).await;
					continue;
				},
				_ => {
					tracing::error!(
						target: LOG_TARGET,
						?block_hash,
						"Abandoning the work package after failed submissions.",
					);
					return;
				},
			}
		}
		tracing::info!(
			target: LOG_TARGET,
			?block_hash,
			?package_hash,
			core,
			anchor = ?anchored.context.anchor,
			"Submitted the work package; following its status.",
		);

		let mut statuses = match jam
			.work_package_status_stream(package_hash, anchored.context.anchor, false)
			.await
		{
			Ok(statuses) => statuses,
			Err(error) => {
				tracing::warn!(
					target: LOG_TARGET,
					?package_hash,
					?error,
					"Unable to follow the work-package status.",
				);
				match policy.on_stream_closed() {
					PolicyAction::Resubmit => {
						match recontext(jam, source.service_id, para_id, &anchored, block_hash)
							.await
						{
							Ok(fresh) => {
								anchored = fresh;
								continue;
							},
							Err(()) => return,
						}
					},
					_ => return,
				}
			},
		};

		loop {
			let Some(status) = statuses.next().await else {
				tracing::warn!(
					target: LOG_TARGET,
					?package_hash,
					?block_hash,
					"Work-package status stream closed.",
				);
				match policy.on_stream_closed() {
					PolicyAction::Resubmit => break,
					_ => {
						tracing::error!(
							target: LOG_TARGET,
							?block_hash,
							"Abandoning the work package.",
						);
						return;
					},
				}
			};

			let action = policy.on_status(&status);
			tracing::info!(
				target: LOG_TARGET,
				?package_hash,
				?block_hash,
				?status,
				?action,
				"Work-package status update.",
			);
			match action {
				PolicyAction::Wait => {},
				PolicyAction::Done => {
					tracing::info!(
						target: LOG_TARGET,
						?package_hash,
						?block_hash,
						"Work package reported on JAM. \
						 Completion (\"done\") shows as the para head advancing.",
					);
					return;
				},
				PolicyAction::Resubmit => break,
				PolicyAction::Abandon => {
					tracing::error!(
						target: LOG_TARGET,
						?package_hash,
						?block_hash,
						"Abandoning the work package.",
					);
					return;
				},
			}
		}

		match recontext(jam, source.service_id, para_id, &anchored, block_hash).await {
			Ok(fresh) => anchored = fresh,
			Err(()) => return,
		}
	}
}

/// Phase 1–3 reaction to a failed/expired package: nothing ties the block to its anchor yet, so
/// re-anchor the SAME block — no round trip through the builder, no orphaned para block.
///
/// The proof of the para head lives inside the PoV and is checked against the anchor's state
/// root, so a new anchor needs a new proof; it is verified here for the same reason the builder
/// verifies its own, namely that a proof the service would reject must never be submitted.
async fn recontext<Jam, BlockHash>(
	jam: &Jam,
	service_id: ServiceId,
	para_id: u32,
	previous: &Anchored,
	block_hash: BlockHash,
) -> Result<Anchored, ()>
where
	Jam: JamChainSource + JamStateSource + ?Sized,
	BlockHash: std::fmt::Debug,
{
	let context = match fresh_context(jam).await {
		Ok(context) => context,
		Err(error) => {
			tracing::error!(
				target: LOG_TARGET,
				?block_hash,
				?error,
				"Unable to build a fresh refine context; abandoning the work package.",
			);
			return Err(());
		},
	};

	let (head_proof, proved_head) = match fetch_anchor_state_proof(
		jam,
		context.anchor,
		&context.state_root,
		service_id,
		para_id,
	)
	.await
	{
		Ok(proof) => proof,
		Err(error) => {
			tracing::error!(
				target: LOG_TARGET,
				?block_hash,
				new_anchor = ?context.anchor,
				error,
				"Unable to prove the para head at the fresh anchor; abandoning the work package.",
			);
			return Err(());
		},
	};

	tracing::info!(
		target: LOG_TARGET,
		?block_hash,
		old_anchor = ?previous.context.anchor,
		new_anchor = ?context.anchor,
		anchor_proof_nodes = head_proof.nodes.len(),
		head_present = proved_head.is_some(),
		"Re-anchored the work package around a fresh anchor and re-proved the para head.",
	);
	Ok(Anchored { state_root: *context.state_root, head_proof, context })
}

/// The refine context around the current best JAM block (anchor = parent of best, lookup anchor
/// = parent of finalized), as in polkajam's `create_refine_context`.
async fn fresh_context<Jam>(jam: &Jam) -> jam_interface::Result<RefineContext>
where
	Jam: JamChainSource + ?Sized,
{
	let best = jam.best_block().await?;
	let anchor = jam.parent(best.header_hash).await?;
	let state_root = jam.state_root(anchor.header_hash).await?;
	let beefy_root = jam.beefy_root(anchor.header_hash).await?;
	let finalized = jam.finalized_block().await?;
	let lookup_anchor = jam.parent(finalized.header_hash).await?;
	Ok(RefineContext {
		anchor: anchor.header_hash,
		state_root,
		beefy_root,
		lookup_anchor: lookup_anchor.header_hash,
		lookup_anchor_slot: lookup_anchor.slot,
		prerequisites: Default::default(),
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use cumulus_test_runtime::{Block as TestBlock, Header as TestHeader};
	use sp_core::H256;

	fn test_proof() -> StateProof {
		StateProof { nodes: vec![[7u8; 64], [8u8; 64]], values: vec![([3u8; 31], vec![9, 9, 9])] }
	}

	/// The key is a cross-repo contract: the collator writes it, the parachain service reads it,
	/// and a typo either way would silently strip the ancestry check.
	#[test]
	fn the_proof_key_is_the_one_the_service_reads() {
		assert_eq!(ANCHOR_STATE_PROOF_KEY, parasim_service::pov::ANCHOR_STATE_PROOF_KEY);
	}

	/// The PoV is checked by parsing it with the reader that will actually consume it in-core,
	/// so a layout change on either side fails here rather than on a live network.
	#[test]
	fn the_pov_is_readable_by_the_parachain_service() {
		let parent_hash = H256::repeat_byte(7);
		let header = TestHeader::new(
			5,
			H256::repeat_byte(1),
			H256::repeat_byte(2),
			parent_hash,
			Default::default(),
		);
		let anchor_state_root = [4u8; 32];
		let head_proof = test_proof();

		let pov = build_pov(
			&[TestBlock::new(header.clone(), vec![])],
			&CompactProof { encoded_nodes: vec![vec![1u8, 2, 3]] },
			anchor_state_root,
			&head_proof,
		);

		let decoded = parasim_service::pov::decode_pov(&pov).expect("the service parses our PoV");
		assert_eq!(decoded.head, header.encode(), "the new para head is the encoded header");
		assert_eq!(decoded.parent_hash, <[u8; 32]>::from(parent_hash));

		let (root, proof) =
			<([u8; 32], StateProof)>::decode(&mut &decoded.anchor_state_proof[..])
				.expect("the anchor state proof decodes");
		assert_eq!(root, anchor_state_root);
		assert_eq!(proof, head_proof);
	}
}
