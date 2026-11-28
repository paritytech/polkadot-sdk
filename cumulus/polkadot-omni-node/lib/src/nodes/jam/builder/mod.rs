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

//! The JAM block-builder task.
//!
//! A wall-clock **para-slot timer** drives authoring — one tick per parachain slot (the Aura slot
//! duration, 6 s today) — instead of one iteration per JAM best-block notification, which could
//! never pace a parachain producing several blocks per JAM slot. The JAM best-block subscription
//! is demoted to a tip cache that the tick reads synchronously; the anchor is that cached tip.
//!
//! Per tick: read the para head JAM has accumulated, pick the parent to author on out of the
//! blocks the local database holds below that head ([`choose_parent`](parent::choose_parent) over
//! [`select_parent`](parent::select_parent)),
//! ask the runtime whether another block fits on top (`can_build_upon` — capacity and velocity
//! are runtime-owned), author, import, and hand `(block, proof, context)` to the collation task.
//! A tick does that once per core holding the para's authorizer: the turn's packages go to
//! different cores, and every package after the first is built on the previous one's block —
//! which is what the collation task turns into a `prerequisites` link between their work
//! packages. Authoring never waits for inclusion and never waits on JAM state for its parent
//! either: the only things the builder needs from JAM are the accumulated head it prunes and
//! anchors against, and the cores its packages may go to.
//!
//! The one thing that head is still needed for is liveness. A work package the collation manager
//! gives up on leaves a block that will never accumulate, and the deepest-block rule would keep
//! authoring on top of it forever. So when the accumulated head has stood still for
//! [`STALL_REROOT_SLOTS`](parent::STALL_REROOT_SLOTS) para slots, the builder abandons the branch
//! above it and authors a *sibling* of the stuck block instead, then stays on that new branch until
//! the head moves.
//!
//! JAM's in-flight work reports are still read every tick, but purely as a **monitor**: with no
//! links left, that log line is the only pre-accumulate view of the pipeline. Nothing in the tick
//! branches on it.
//!
//! A block carries no parachain inherent: its JAM context is the `JamParent` digest, and its
//! timestamp is the anchor's, so its Aura slot is the anchor's Aura slot and the parachain, the
//! collator and the JAM authorizer agree on who may author. The timestamp travels *inside* the
//! block, so importers validate it instead of recomputing it from their own clock.

pub(crate) mod inherent;
pub(crate) mod parent;
pub(crate) mod propose;
pub(crate) mod state;
#[cfg(test)]
mod tests;
pub(crate) mod tick;
pub(crate) mod turn;

use crate::{
	common::{
		aura::{AuraIdT, AuraRuntimeApi},
		types::{JamClient, ParachainBackend},
		ConstructNodeRuntimeApi, NodeBlock,
	},
	nodes::jam::{
		authorizer::AuthoritySource,
		builder::{
			state::BuilderState,
			tick::BuilderContext,
			turn::{time_until_next_para_slot, MAX_TIP_LAG_SLOTS},
		},
		chain::anchor::ANCHOR_OFFSET,
		code::ensure_code_blob_at,
		message::{AuthoringHold, JamCollatorMessage},
		LOG_TARGET,
	},
};
use futures::{channel::mpsc, FutureExt, StreamExt};
use jam_interface::{BlockDesc, JamChainSource, JamStateSource, ServiceId};
use polkadot_primitives::Id as ParaId;
use sc_consensus::BlockImport;
use sc_consensus_aura::standalone as aura_internal;
use sp_blockchain::HeaderBackend;
use sp_consensus::Environment;
use sp_keystore::KeystorePtr;
use sp_timestamp::Timestamp;
use std::{collections::VecDeque, sync::Arc};

pub(crate) struct BuilderTaskParams<Block: NodeBlock, RuntimeApi, BI, PF, Jam> {
	pub para_client: Arc<JamClient<Block, RuntimeApi>>,
	/// Needed for the block tree alone: picking a parent means walking the children of the
	/// accumulated head, and `children` lives on the backend's blockchain, not on the client.
	pub para_backend: Arc<ParachainBackend<Block>>,
	pub block_import: BI,
	pub proposer_factory: PF,
	pub keystore: KeystorePtr,
	pub para_id: ParaId,
	pub service_id: ServiceId,
	/// The para's AURA authorizers by anchor slot: which set JAM authorizes there, and whether it
	/// names this node. Re-read per anchor because the set changes at a JAM slot.
	pub authorizers: Arc<dyn AuthoritySource<Block>>,
	pub jam: Arc<Jam>,
	pub message_sender: mpsc::Sender<JamCollatorMessage<Block>>,
	/// How many in-flight resubmission packages the collation task is holding; while non-zero
	/// the tick authors nothing.
	pub hold: AuthoringHold,
}

/// Run the builder task. Ends (taking the node down, it is an essential task) only if the JAM
/// best-block stream cannot be (re-)established or the collation task is gone.
pub(crate) async fn run_builder_task<Block, RuntimeApi, AuraId, BI, PF, Jam>(
	params: BuilderTaskParams<Block, RuntimeApi, BI, PF, Jam>,
) where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, JamClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
	BI: BlockImport<Block> + Send + Sync,
	PF: Environment<Block>,
	Jam: JamChainSource + JamStateSource + 'static,
{
	let BuilderTaskParams {
		para_client,
		para_backend,
		block_import,
		proposer_factory,
		keystore,
		para_id,
		service_id,
		authorizers,
		jam,
		mut message_sender,
		hold,
	} = params;

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
		anchor_offset = ANCHOR_OFFSET,
		max_tip_lag_slots = MAX_TIP_LAG_SLOTS,
		"JAM builder task started; building up to one block per core per parachain slot.",
	);

	// Nothing here survives a restart, and nothing needs to: every tick reads the accumulated
	// head from JAM and finds the blocks above it in the local database, which is also how this
	// collator extends a block another one authored.
	let mut builder = BuilderContext {
		para_client,
		para_backend,
		block_import,
		proposer_factory,
		keystore,
		para_id,
		service_id,
		authorizers,
		jam,
		hold,
		state: BuilderState {
			last_claimed_anchor: None,
			included: None,
			own_recent: VecDeque::new(),
			head_advanced_at: None,
			rerooted: None,
			last_pool_scan: None,
		},
	};
	let mut cached_tip: Option<BlockDesc> = None;
	loop {
		// The Aura slot duration lives in the runtime and can change under an upgrade (the
		// 18s-slot test), so the tick cadence is re-read every round rather than captured once
		// at task start. The authoritative per-parent value for the block itself is read in
		// `run_tick`, once the parent is chosen. A test that changes the slot must run a single
		// collator: the authorizer round-robin divisor (`--jam-slot-duration`) is separate,
		// immutable config, and with one collator the Aura author index is always 0.
		// A block this collator just produced can carry the offchain-code marker the runtime
		// writes once the preimage is available; resolve it before reading that block's runtime.
		if let Some(tip) = &cached_tip {
			ensure_code_blob_at(
				&builder.para_client,
				&builder.para_backend,
				&*builder.jam,
				service_id,
				builder.para_client.info().best_hash,
				tip.header_hash,
			)
			.await;
		}
		let best_hash = builder.para_client.info().best_hash;
		let slot_duration = match aura_internal::slot_duration_at(&*builder.para_client, best_hash)
		{
			Ok(slot_duration) => slot_duration,
			Err(error) => {
				tracing::error!(
					target: LOG_TARGET,
					?error,
					"Failed to read the Aura slot duration."
				);
				return;
			},
		};
		let mut next_tick = futures_timer::Delay::new(time_until_next_para_slot(
			Timestamp::current().as_duration(),
			slot_duration.as_duration(),
		))
		.fuse();
		loop {
			futures::select! {
				_ = next_tick => break,
				jam_best = best_blocks.next() => match jam_best {
					Some(jam_best) => {
						tracing::debug!(target: LOG_TARGET, ?jam_best, "JAM tip cached.");
						cached_tip = Some(jam_best);
					},
					None => {
						tracing::error!(target: LOG_TARGET, "JAM best-block stream ended.");
						return;
					},
				},
			}
		}

		// Captured once, before anything else: the block's Aura slot, and its timestamp
		// derive from this instant and have to agree, so
		// re-reading the clock after an await would let a tick claim the following slot.
		let now = Timestamp::current();

		let Some(tip) = cached_tip else {
			tracing::debug!(
				target: LOG_TARGET,
				"No JAM best block seen yet; skipping this parachain slot.",
			);
			continue;
		};

		match builder.run_tick::<AuraId>(slot_duration, tip, now).await {
			Ok(messages) => {
				// A turn's packages leave in order, so the collation task sees the first one's
				// hash recorded in the ledger before it assembles the second one's package and
				// names it as the second one's prerequisite.
				for message in messages {
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
				}
			},
			Err(error) => {
				tracing::warn!(
					target: LOG_TARGET,
					error,
					?tip,
					"Failed to build a block in this parachain slot.",
				);
			},
		}
	}
}
