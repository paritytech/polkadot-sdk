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
//! Phase-1 simplifications: null authorizer (empty token, nothing to sign), fixed core, PoV is
//! NOT zstd-compressed (parasim rejects compressed PoVs; JIP-2 is silent on compression).

use super::{JamCollatorMessage, LOG_TARGET, para_head_stream, resubmission::*};
use crate::common::{ConstructNodeRuntimeApi, NodeBlock, types::ParachainClient};
use codec::{Decode, Encode};
use cumulus_primitives_core::ParachainBlockData;
use futures::{StreamExt, channel::mpsc};
use jam_cumulus_facade::{ParachainCandidate, authorizer::fixed_authorizer};
use jam_interface::{
	CoreIndex, JamChainSource, JamStateSource, JamWorkPackageSubmission, ServiceId,
	VersionedParameters, WorkPackage, WorkPackageHash,
};
use jam_types::{Authorization, RefineContext, WorkItem, WorkPayload};
use polkadot_primitives::Id as ParaId;
use sp_core::traits::SpawnNamed;
use sp_runtime::traits::{Block as BlockT, HashingFor, Header as HeaderT};
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
	service_id: ServiceId,
	core: CoreIndex,
	service_code_hash: jam_types::CodeHash,
	refine_gas_limit: jam_types::UnsignedGas,
	accumulate_gas_limit: jam_types::UnsignedGas,
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
	let JamCollatorMessage { parent_header, block, proof, context, triggered_by } = message;
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

	// The uncompressed PoV: parasim rejects zstd-compressed PoVs and JIP-2 is silent on
	// compression (phase-2 decision; see the design doc's B.0 item 8).
	let pov = ParachainBlockData::V1 { blocks: vec![block], proof: compact_proof }.encode();

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
	let validation_code_hash = sp_crypto_hashing::blake2_256(&validation_code);

	let payload = ParachainCandidate {
		validation_code_hash: jam_cumulus_facade::ValidationCodeHash(validation_code_hash.into()),
		pov,
	}
	.encode();

	let work_item = WorkItem {
		service: service_id,
		code_hash: service_code_hash,
		payload: WorkPayload(payload),
		refine_gas_limit,
		accumulate_gas_limit,
		import_segments: Default::default(),
		extrinsics: Default::default(),
		export_count: 0,
	};
	let package = WorkPackage {
		authorization: Authorization::default(),
		auth_code_host: 0,
		authorizer: fixed_authorizer(),
		context,
		items: vec![work_item].try_into().expect("a single work item always fits; qed"),
	};

	tracing::info!(
		target: LOG_TARGET,
		?block_hash,
		%block_number,
		pov_len = package.items[0].payload.0.len(),
		anchor = ?package.context.anchor,
		?triggered_by,
		core,
		"Assembled a work package for the block.",
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
			submit_and_follow::<Block, _, _>(&*jam, core, package, block_hash, policy).await;
		}),
	);
}

/// Submit the package and follow its status until it is reported, abandoning or re-contexting
/// (fresh anchor, same block) as the policy dictates.
async fn submit_and_follow<Block, Jam, Policy>(
	jam: &Jam,
	core: CoreIndex,
	mut package: WorkPackage,
	block_hash: Block::Hash,
	mut policy: Policy,
) where
	Block: BlockT,
	Jam: JamChainSource + JamWorkPackageSubmission + ?Sized,
	Policy: ResubmissionPolicy,
{
	loop {
		let package_hash = WorkPackageHash::from(sp_crypto_hashing::blake2_256(
			&jam_codec::Encode::encode(&package),
		));
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
			anchor = ?package.context.anchor,
			"Submitted the work package; following its status.",
		);

		let mut statuses = match jam
			.work_package_status_stream(package_hash, package.context.anchor, false)
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
						match recontext(jam, &mut package, block_hash).await {
							Ok(()) => continue,
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

		if recontext(jam, &mut package, block_hash).await.is_err() {
			return;
		}
	}
}

/// Phase 1–3 reaction to a failed/expired package: nothing ties the block to its anchor yet, so
/// build a fresh refine context around the SAME block and resubmit — no round trip through the
/// builder, no orphaned para block.
async fn recontext<Jam, BlockHash>(
	jam: &Jam,
	package: &mut WorkPackage,
	block_hash: BlockHash,
) -> Result<(), ()>
where
	Jam: JamChainSource + ?Sized,
	BlockHash: std::fmt::Debug,
{
	let fresh_context = async {
		let best = jam.best_block().await?;
		let anchor = jam.parent(best.header_hash).await?;
		let state_root = jam.state_root(anchor.header_hash).await?;
		let beefy_root = jam.beefy_root(anchor.header_hash).await?;
		let finalized = jam.finalized_block().await?;
		let lookup_anchor = jam.parent(finalized.header_hash).await?;
		Ok::<_, jam_interface::Error>(RefineContext {
			anchor: anchor.header_hash,
			state_root,
			beefy_root,
			lookup_anchor: lookup_anchor.header_hash,
			lookup_anchor_slot: lookup_anchor.slot,
			prerequisites: Default::default(),
		})
	}
	.await;

	match fresh_context {
		Ok(context) => {
			tracing::info!(
				target: LOG_TARGET,
				?block_hash,
				old_anchor = ?package.context.anchor,
				new_anchor = ?context.anchor,
				"Re-contexting the work package around a fresh anchor.",
			);
			package.context = context;
			Ok(())
		},
		Err(error) => {
			tracing::error!(
				target: LOG_TARGET,
				?block_hash,
				?error,
				"Unable to build a fresh refine context; abandoning the work package.",
			);
			Err(())
		},
	}
}
