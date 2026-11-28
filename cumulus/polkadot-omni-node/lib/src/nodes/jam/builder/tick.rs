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

//! One para-slot tick of the builder: guards, JAM reads, parent choice, and the turn's packages.

use crate::{
	common::{
		aura::{AuraIdT, AuraRuntimeApi},
		types::{JamClient, ParachainBackend},
		ConstructNodeRuntimeApi, NodeBlock,
	},
	nodes::jam::{
		authorizer::{AuraPublic, AuthoritySource},
		builder::{
			parent::{
				choose_parent, local_descendants, select_parent, Descendant, ParentSource,
				STALL_REROOT_SLOTS,
			},
			propose::{build_seal_import, can_build_upon},
			state::{head_move, BuilderState, HeadMove},
			turn::{
				anchor_slot_claimed, next_parent_source, package_fits_turn, tip_is_fresh,
				turn_package_budget, MAX_TIP_LAG_SLOTS,
			},
		},
		chain::{
			anchor::{fetch_anchor_state_proof, read_anchor, AnchorReads},
			in_flight_reports::{monitor_in_flight, InFlightReport},
			para_head::head_hash_of,
			slots::jam_slot_at,
		},
		code::ensure_code_blob_at,
		message::{AuthoringHold, JamCollatorMessage},
		LOG_TARGET,
	},
};
use cumulus_client_consensus_aura::collator::SlotClaim;
use cumulus_jam_aura_primitives::timestamp_for_anchor;
use jam_interface::{
	BlockDesc, CoreIndex, JamChainSource, JamStateSource, ServiceId, Slot as JamSlot,
};
use parachain_service_core::StateProof;
use polkadot_primitives::Id as ParaId;
use sc_consensus::BlockImport;
use sc_consensus_aura::standalone as aura_internal;
use sp_blockchain::HeaderBackend;
use sp_consensus::Environment;
use sp_consensus_aura::{Slot, SlotDuration};
use sp_keystore::KeystorePtr;
use sp_runtime::traits::Header as HeaderT;
use sp_timestamp::Timestamp;
use std::{sync::Arc, time::Instant};

/// The parent a turn starts on: its header, its depth below the accumulated head, where it hangs,
/// and the accumulated head's hash.
type ChosenParent<Block> = (
	<Block as sp_runtime::traits::Block>::Header,
	usize,
	ParentSource,
	<Block as sp_runtime::traits::Block>::Hash,
);

/// Everything the builder holds across ticks: the node handles a tick works with, and the
/// [`BuilderState`] it carries from one tick to the next.
pub(crate) struct BuilderContext<Block: NodeBlock, RuntimeApi, BI, PF, Jam> {
	pub(crate) para_client: Arc<JamClient<Block, RuntimeApi>>,
	pub(crate) para_backend: Arc<ParachainBackend<Block>>,
	pub(crate) block_import: BI,
	pub(crate) proposer_factory: PF,
	pub(crate) keystore: KeystorePtr,
	pub(crate) para_id: ParaId,
	pub(crate) service_id: ServiceId,
	pub(crate) authorizers: Arc<dyn AuthoritySource<Block>>,
	pub(crate) jam: Arc<Jam>,
	pub(crate) hold: AuthoringHold,
	pub(crate) state: BuilderState<Block::Header>,
}

impl<Block, RuntimeApi, BI, PF, Jam> BuilderContext<Block, RuntimeApi, BI, PF, Jam>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, JamClient<Block, RuntimeApi>>,
	BI: BlockImport<Block> + Send + Sync,
	PF: Environment<Block>,
	Jam: JamChainSource + JamStateSource + 'static,
{
	/// One para-slot tick: guards, JAM reads, segment maintenance, capacity gate, authoring.
	///
	/// The tick's turn authors one package per core holding the para's authorizer, each package
	/// on its own core and chained on the previous package's block. An empty vector means the
	/// tick was skipped; the reason is always logged where it was decided.
	pub(crate) async fn run_tick<AuraId>(
		&mut self,
		tick_slot_duration: SlotDuration,
		tip: BlockDesc,
		now: Timestamp,
	) -> Result<Vec<JamCollatorMessage<Block>>, String>
	where
		RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
		AuraId: AuraIdT + Sync,
	{
		let Some(wall_jam_slot) = self.gate(tick_slot_duration, tip, now) else {
			return Ok(Vec::new());
		};
		let Some((reads, reports)) = self.read_under_timeout(tick_slot_duration, tip).await? else {
			return Ok(Vec::new());
		};
		self.state.note_pool_scan(&reads.pool_scan, &reads.anchor, &reads.authorizer);

		// One turn per anchor slot. The early check in `gate` is on the tip so a repeat tick costs
		// no JAM read; this one is on the anchor the blocks actually derive from, which is what the
		// guard is about when `ANCHOR_OFFSET` walks back from the tip.
		if anchor_slot_claimed(self.state.last_claimed_anchor, reads.anchor.slot) {
			tracing::debug!(
				target: LOG_TARGET,
				anchor = ?reads.anchor.header_hash,
				anchor_slot = reads.anchor.slot,
				last_claimed_anchor = ?self.state.last_claimed_anchor,
				"Anchor slot already claimed; skipping this tick.",
			);
			return Ok(Vec::new());
		}

		// The block's timestamp is the anchor's, and its Aura slot follows from that timestamp:
		// the parachain, the collator and the JAM authorizer all read the same clock. A turn per
		// anchor slot, so a tip that lags the wall clock does not produce a second turn for the
		// same anchor.
		let block_timestamp = Timestamp::new(timestamp_for_anchor(reads.anchor.slot));
		let anchor_para_slot = Slot::from_timestamp(block_timestamp, tick_slot_duration);
		let stalled_for = self.observe_head(&reads, anchor_para_slot)?;

		// Authoring past a package that is still being resent would only deepen a branch whose
		// package has not landed. `note_pool_scan`, `state.included` and `note_head` have already
		// run and `last_claimed_slot` is untouched, so the stall clock keeps counting through held
		// ticks and the re-root fires once the hold lifts if the head is still stuck.
		let pending = self.hold.pending();
		if pending > 0 {
			tracing::info!(
				target: LOG_TARGET,
				pending,
				?anchor_para_slot,
				stalled_for,
				"Holding this parachain slot: a lost block is being resubmitted in a new work \
				 package instead of a new block being built.",
			);
			return Ok(Vec::new());
		}

		let Some(parent) = self.select_parent(stalled_for)? else { return Ok(Vec::new()) };
		let (parent_hash, included_hash) = (parent.0.hash(), parent.3);
		let Some(slot_claim) = self
			.claim_slot::<AuraId>(
				&reads,
				tip,
				block_timestamp,
				parent_hash,
				&reports,
				included_hash,
			)
			.await?
		else {
			return Ok(Vec::new());
		};
		let Some(anchor_state_proof) = self.anchor_proof(&reads).await? else {
			return Ok(Vec::new());
		};
		self.author_turn::<AuraId>(
			&reads,
			tip,
			wall_jam_slot,
			&slot_claim,
			&anchor_state_proof,
			parent,
		)
		.await
	}

	/// The checks that need no JAM read: the tip's anchor slot is not claimed yet and the tip is
	/// fresh. Returns the wall clock's JAM slot when the tick goes on.
	fn gate(
		&self,
		tick_slot_duration: SlotDuration,
		tip: BlockDesc,
		now: Timestamp,
	) -> Option<JamSlot> {
		// The tick's budget and early stall clock come from the runtime at the best block. The
		// block's own Aura slot and timestamp are re-derived from the anchor, so an upgrade that
		// changes the slot keeps them in step.
		let wall_para_slot = Slot::from_timestamp(now, tick_slot_duration);
		let wall_jam_slot = jam_slot_at(now);
		tracing::debug!(
			target: LOG_TARGET,
			para_slot = ?wall_para_slot,
			wall_jam_slot,
			timestamp = now.as_millis(),
			?tip,
			tip_lag_slots = wall_jam_slot.saturating_sub(tip.slot),
			tip_fresh = tip_is_fresh(wall_jam_slot, tip.slot),
			included_head = ?self.state.included,
			"Parachain slot tick.",
		);

		if anchor_slot_claimed(self.state.last_claimed_anchor, tip.slot) {
			tracing::debug!(
				target: LOG_TARGET,
				?tip,
				last_claimed_anchor = ?self.state.last_claimed_anchor,
				"Anchor slot already claimed; skipping this tick.",
			);
			return None;
		}

		if !tip_is_fresh(wall_jam_slot, tip.slot) {
			tracing::warn!(
				target: LOG_TARGET,
				?tip,
				wall_jam_slot,
				lag_slots = wall_jam_slot.saturating_sub(tip.slot),
				max_tip_lag_slots = MAX_TIP_LAG_SLOTS,
				"The cached JAM tip lags the wall clock; skipping this tick.",
			);
			return None;
		}
		Some(wall_jam_slot)
	}

	/// The anchor reads and the in-flight monitor, bounded by one parachain slot. `None` skips the
	/// tick: the reads ran over, or they disagree with each other.
	async fn read_under_timeout(
		&self,
		tick_slot_duration: SlotDuration,
		tip: BlockDesc,
	) -> Result<Option<(AnchorReads, Vec<InFlightReport<Block::Header>>)>, String> {
		// The tick is the reads' budget: a JAM node that cannot answer within one parachain slot
		// has nothing fresh to say, and the next tick supersedes this one anyway. The monitor read
		// runs alongside the anchor reads rather than after them, so watching the pipeline costs
		// the tick no latency of its own, and it carries its own bound: nothing here may skip a
		// tick.
		let (reads, reports) = match tokio::time::timeout(
			tick_slot_duration.as_duration(),
			futures::future::join(
				read_anchor::<Block, _>(
					&*self.jam,
					tip,
					self.service_id,
					self.para_id.into(),
					&*self.authorizers,
					self.para_client.info().best_hash,
					true,
				),
				monitor_in_flight::<Block::Header, _>(&*self.jam, tip.header_hash, self.service_id),
			),
		)
		.await
		{
			Ok((reads, reports)) => (reads?, reports),
			Err(_) => {
				tracing::warn!(
					target: LOG_TARGET,
					?tip,
					budget_ms = tick_slot_duration.as_millis(),
					"JAM reads did not finish within the parachain slot; skipping this tick.",
				);
				return Ok(None);
			},
		};
		Ok(reads.map(|reads| (reads, reports)))
	}

	/// Fold the para head JAM has accumulated at the anchor into the builder's view of it, and
	/// say how many para slots it has stood still.
	fn observe_head(&mut self, reads: &AnchorReads, anchor_para_slot: Slot) -> Result<u64, String> {
		let included_head = reads
			.included
			.as_deref()
			.map(head_hash_of::<Block::Hash>)
			.transpose()
			.map_err(|e| format!("included head: {e}"))?;
		// Best effort, for the log: JAM holds only the hash.
		let para_client = &self.para_client;
		let local_number = |hash: Option<Block::Hash>| {
			hash.and_then(|hash| para_client.header(hash).ok().flatten())
				.map(|h| *h.number())
		};
		tracing::debug!(
			target: LOG_TARGET,
			anchor = ?reads.anchor.header_hash,
			?included_head,
			included_number = ?local_number(included_head),
			"Para head accumulated in JAM state at the anchor.",
		);

		let state = &mut self.state;
		let head_before = state.included;
		let moved = head_move(state.included.as_ref(), included_head.as_ref(), &state.own_recent);
		state.included = included_head;
		let stalled_for = state.note_head(moved, anchor_para_slot);
		// The level is the only thing that varies, and `tracing` needs it at the macro's call site.
		macro_rules! log_head_move {
			($level:ident) => {
				tracing::$level!(
					target: LOG_TARGET,
					?moved,
					?head_before,
					head_after = ?state.included,
					head_number = ?local_number(state.included),
					own_recent = ?state.own_recent,
					stalled_for,
					stall_reroot_slots = STALL_REROOT_SLOTS,
					"The para head JAM has accumulated.",
				)
			};
		}
		match moved {
			HeadMove::Unchanged => log_head_move!(debug),
			_ => log_head_move!(info),
		}
		Ok(stalled_for)
	}

	/// The block the turn starts on, out of what the local database holds below the accumulated
	/// head. `None` skips the tick: the head or the parent is not known locally yet.
	fn select_parent(&mut self, stalled_for: u64) -> Result<Option<ChosenParent<Block>>, String> {
		let para_client = &self.para_client;
		let genesis_hash = para_client.info().genesis_hash;
		// The whole header, not just its hash, comes from the local database, as JAM holds only
		// its hash.
		let included_header = match self.state.included {
			Some(hash) => {
				match para_client.header(hash).map_err(|e| format!("included header: {e}"))? {
					Some(header) => header,
					None => {
						tracing::warn!(
							target: LOG_TARGET,
							accumulated_head = ?hash,
							"The accumulated head is not known locally; waiting for import/sync.",
						);
						return Ok(None);
					},
				}
			},
			// Nothing accumulated yet, so the para's included head is still its genesis block.
			None => para_client
				.header(genesis_hash)
				.map_err(|e| format!("genesis header: {e}"))?
				.ok_or_else(|| format!("genesis header {genesis_hash:?} not found"))?,
		};
		let included_hash = included_header.hash();

		let descendants = local_descendants::<Block>(&self.para_backend, included_hash);
		let state = &mut self.state;
		state.forget_unreachable(&descendants);
		// The branch a previous tick re-rooted onto, if its tip is still a block we hold.
		let committed = state.rerooted.and_then(|reroot| {
			let header = para_client.header(reroot.tip).ok().flatten()?;
			Some(Descendant { header, depth: reroot.depth })
		});
		let (chosen, parent_source) =
			choose_parent(&descendants, &state.own_recent, committed.as_ref(), stalled_for);
		let (parent_header, depth) = match chosen {
			Some(parent) => (parent.header.clone(), parent.depth),
			None => (included_header.clone(), 0),
		};
		let parent_hash = parent_header.hash();
		if parent_source == ParentSource::Reroot {
			let abandoned = select_parent(&descendants, &state.own_recent);
			tracing::warn!(
				target: LOG_TARGET,
				stuck_head = ?included_hash,
				stuck_head_number = %included_header.number(),
				stalled_for,
				stall_reroot_slots = STALL_REROOT_SLOTS,
				abandoned_tip = ?abandoned.map(|parent| parent.header.hash()),
				abandoned_depth = abandoned.map_or(0, |parent| parent.depth),
				"The para head JAM has accumulated has not moved for too long; the branch above it \
				 carries a work package nobody is going to report. Abandoning that branch and \
				 authoring a sibling of the stuck block instead.",
			);
		}
		tracing::debug!(
			target: LOG_TARGET,
			?parent_source,
			parent = ?parent_hash,
			parent_number = %parent_header.number(),
			depth,
			?included_hash,
			stalled_for,
			local_descendants = descendants.len(),
			siblings_at_depth = descendants.iter().filter(|other| other.depth == depth).count(),
			"Parent selected out of the blocks the local database holds below the accumulated head.",
		);
		// Only the accumulated-head fallback can name a block we do not have: everything the walk
		// returns came out of the local database in the first place.
		if para_client
			.header(parent_hash)
			.map_err(|e| format!("parent header: {e}"))?
			.is_none()
		{
			tracing::warn!(
				target: LOG_TARGET,
				parent = ?parent_hash,
				parent_number = %parent_header.number(),
				"The accumulated head is not known locally; waiting for import/sync.",
			);
			return Ok(None);
		}
		Ok(Some((parent_header, depth, parent_source, included_hash)))
	}

	/// The Aura slot the turn's blocks claim, and the claim itself. `None` skips the tick: the
	/// slot duration could not be read, or the anchor's Aura slot names another author.
	async fn claim_slot<AuraId>(
		&self,
		reads: &AnchorReads,
		tip: BlockDesc,
		block_timestamp: Timestamp,
		parent_hash: Block::Hash,
		reports: &[InFlightReport<Block::Header>],
		included_hash: Block::Hash,
	) -> Result<Option<SlotClaim<AuraPublic<AuraId>>>, String>
	where
		RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
		AuraId: AuraIdT + Sync,
	{
		// The parent's runtime owns the slot duration: the block's Aura slot and the runtime's own
		// `JamFixedVelocityConsensusHook` both have to agree on it, and an upgrade can change it
		// between rounds (the 18s-slot test). Reading it at the block's actual parent keeps both
		// in step; `--jam-slot-duration` — the authorizer round-robin divisor — is a separate,
		// immutable config and is never read from the runtime.
		ensure_code_blob_at(
			&self.para_client,
			&self.para_backend,
			&*self.jam,
			self.service_id,
			parent_hash,
			tip.header_hash,
		)
		.await;
		let slot_duration = match aura_internal::slot_duration_at(&*self.para_client, parent_hash) {
			Ok(slot_duration) => slot_duration,
			Err(error) => {
				tracing::error!(
					target: LOG_TARGET,
					?error,
					?parent_hash,
					"Failed to read the Aura slot duration."
				);
				return Ok(None);
			},
		};
		let para_slot = Slot::from_timestamp(block_timestamp, slot_duration);
		self.log_in_flight_reports(reports, included_hash);

		// The block is sealed against the set JAM authorizes at its anchor slot, and its Aura slot
		// is the anchor's, so the Aura round-robin index is the authorizer's: the turn check in
		// `read_anchor` already named this node, and the author is its key in that set.
		let authorizer = &reads.authorizer;
		if (*para_slot % u64::from(authorizer.collator_set_size())) as u32 != authorizer.own_index()
		{
			tracing::warn!(
				target: LOG_TARGET,
				?para_slot,
				anchor_slot = reads.anchor.slot,
				own_index = authorizer.own_index(),
				"The Aura slot of the anchor does not name us although the authorizer does; \
				 `--jam-slot-duration` disagrees with the runtime's Aura slot duration. Skipping.",
			);
			return Ok(None);
		}
		let author_pub =
			<AuraPublic<AuraId> as sp_core::crypto::ByteArray>::from_slice(&authorizer.own_key())
				.map_err(|_| "the authorizer key is no aura key".to_string())?;
		Ok(Some(SlotClaim::unchecked::<<AuraId as AuraIdT>::BoundedPair>(
			author_pub,
			para_slot,
			block_timestamp,
		)))
	}

	/// The monitor's derived events. Nothing branches on them; they are the pre-accumulation view
	/// of the pipeline, which with no links left is the only one there is.
	fn log_in_flight_reports(
		&self,
		reports: &[InFlightReport<Block::Header>],
		included_hash: Block::Hash,
	) {
		for report in reports {
			let known = report.head.and_then(|head| self.para_client.header(head).ok().flatten());
			match (&report.head, known) {
				(Some(head), None) => tracing::warn!(
					target: LOG_TARGET,
					wp_hash = ?report.wp_hash,
					source = ?report.source,
					?head,
					?included_hash,
					"A work package is in flight for a parachain block we do not hold. Somebody's \
					 chain is ahead of ours and we have not been told about the block — normally \
					 the announcement is still on its way, but a block withheld on purpose looks \
					 exactly like this.",
				),
				(Some(head), Some(header)) => tracing::debug!(
					target: LOG_TARGET,
					wp_hash = ?report.wp_hash,
					source = ?report.source,
					?head,
					head_number = %header.number(),
					"A work package is in flight for a block we hold.",
				),
				(None, _) => tracing::debug!(
					target: LOG_TARGET,
					wp_hash = ?report.wp_hash,
					source = ?report.source,
					"A work package is in flight; its work digest is in a shape we cannot read, so \
					 which block it carries is unknown here.",
				),
			}
		}
	}

	/// The para-head state proof every package of the turn carries. `None` skips the tick: the
	/// proof disagrees with the head read at the same anchor.
	async fn anchor_proof(&self, reads: &AnchorReads) -> Result<Option<StateProof>, String> {
		// The proof is prefetched here — the JAM RPC is async, host calls are sync — and verified
		// against the anchor's own state root with the very code the service runs, so a proof
		// refine would reject never leaves the node: an unverifiable proof fails this tick loudly
		// (`?`) instead of shipping a block JAM would refuse. It describes the anchor, not the
		// parent, so every package of the turn carries a copy of the same proof.
		let (anchor_state_proof, proved_head) = fetch_anchor_state_proof(
			&*self.jam,
			reads.anchor.header_hash,
			&reads.context.state_root,
			self.service_id,
			self.para_id.into(),
		)
		.await?;

		// The proof and the head read above describe the same key at the same anchor, so anything
		// but equality means one of the two reads is stale — shipping it would only earn a refine
		// rejection.
		if proved_head != reads.included {
			tracing::error!(
				target: LOG_TARGET,
				anchor = ?reads.anchor.header_hash,
				proved_head = proved_head.is_some(),
				read_head = reads.included.is_some(),
				"The anchor state proof disagrees with the para head read at the same anchor; \
				 skipping this JAM block.",
			);
			return Ok(None);
		}
		Ok(Some(anchor_state_proof))
	}

	/// The turn: one package per core holding the para's authorizer, each built on the previous
	/// one's block and submitted to its own core. The runtime's gate is asked per package — the
	/// second package is decided against the first package's block — so velocity and capacity end
	/// the burst exactly where the runtime says they are spent.
	async fn author_turn<AuraId>(
		&mut self,
		reads: &AnchorReads,
		tip: BlockDesc,
		wall_jam_slot: JamSlot,
		slot_claim: &SlotClaim<AuraPublic<AuraId>>,
		anchor_state_proof: &StateProof,
		parent: ChosenParent<Block>,
	) -> Result<Vec<JamCollatorMessage<Block>>, String>
	where
		RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
		AuraId: AuraIdT + Sync,
	{
		let (para_slot, block_timestamp) = (slot_claim.slot(), slot_claim.timestamp());
		let (anchor, context, pool_scan) = (reads.anchor, &reads.context, &reads.pool_scan);
		let (mut parent_header, mut depth, mut parent_source, included_hash) = parent;
		let mut parent_hash = parent_header.hash();
		let mut messages = Vec::new();
		for package in 0..turn_package_budget(pool_scan.core_count()) {
			let core = pool_scan.core_for_package(package);
			if !self
				.admits_package::<AuraId>(
					tip,
					package,
					core,
					parent_hash,
					included_hash,
					para_slot,
					depth,
				)
				.await?
			{
				break;
			}

			tracing::info!(
				target: LOG_TARGET,
				?tip,
				?anchor,
				?para_slot,
				wall_jam_slot,
				timestamp = block_timestamp.as_millis(),
				parent = ?parent_hash,
				parent_number = %parent_header.number(),
				?parent_source,
				depth,
				package,
				submit_target = ?core,
				"Building a parachain block against the JAM anchor.",
			);

			// A later package's failure must not drop the packages already authored: their
			// blocks are imported and their messages are ready, and nothing else would ever
			// submit them.
			let built = build_seal_import::<Block, AuraId, _, _>(
				&mut self.block_import,
				&mut self.proposer_factory,
				&self.keystore,
				&parent_header,
				block_timestamp,
				context,
				anchor_state_proof,
				slot_claim,
			)
			.await;
			let (block, proof, additional_data) = match built {
				Ok(built) => built,
				Err(error) if messages.is_empty() => return Err(error),
				Err(error) => {
					tracing::warn!(
						target: LOG_TARGET,
						error,
						package,
						parent = ?parent_hash,
						"The turn's next package failed to build; keeping the packages already \
						 authored and ending the turn here.",
					);
					break;
				},
			};

			let block_hash = block.hash();
			let block_header = block.header().clone();
			self.state.note_authored(parent_source, block_hash, para_slot);
			tracing::info!(
				target: LOG_TARGET,
				?block_hash,
				block_number = %block_header.number(),
				extrinsics = block.extrinsics().len(),
				proof_nodes = proof.iter_nodes().count(),
				depth = depth + 1,
				package,
				submit_target = ?core,
				?parent_source,
				"Built and imported a parachain block.",
			);
			messages.push(JamCollatorMessage {
				parent_header,
				block,
				proof,
				context: context.clone(),
				anchor_slot: anchor.slot,
				submit_target: core,
				additional_data,
				triggered_by: tip,
			});
			parent_header = block_header;
			parent_hash = block_hash;
			depth += 1;
			parent_source = next_parent_source(parent_source);
		}

		if !messages.is_empty() {
			self.state.last_claimed_anchor = Some(anchor.slot);
		}
		Ok(messages)
	}

	/// Whether the turn may author its `package`-th package on `parent_hash`: a core is left for
	/// it and the runtime admits another block there.
	async fn admits_package<AuraId>(
		&self,
		tip: BlockDesc,
		package: usize,
		core: Option<CoreIndex>,
		parent_hash: Block::Hash,
		included_hash: Block::Hash,
		para_slot: Slot,
		depth: usize,
	) -> Result<bool, String>
	where
		RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
		AuraId: AuraIdT + Sync,
	{
		// Every package after the first builds on the previous package's block, which can carry
		// the offchain-code marker the runtime just wrote; resolve it before this package reads
		// that block's runtime.
		ensure_code_blob_at(
			&self.para_client,
			&self.para_backend,
			&*self.jam,
			self.service_id,
			parent_hash,
			tip.header_hash,
		)
		.await;
		let started = Instant::now();
		let can_build = can_build_upon::<Block, RuntimeApi, AuraId>(
			&self.para_client,
			parent_hash,
			included_hash,
			para_slot,
		)?;
		tracing::debug!(
			target: LOG_TARGET,
			parent = ?parent_hash,
			?included_hash,
			?para_slot,
			depth,
			package,
			can_build,
			elapsed_ms = started.elapsed().as_millis(),
			"Asked the runtime whether the unincluded segment has room.",
		);
		if package_fits_turn(package, core, can_build) {
			return Ok(true);
		}
		// `package_fits_turn` is false for one of two reasons, and they are worth telling apart:
		// the runtime said no, or the turn has no core left for another package.
		let stopped = if can_build {
			"the turn has no core left for another package"
		} else {
			"the runtime refuses another block on this parent (capacity or velocity)"
		};
		tracing::info!(
			target: LOG_TARGET,
			parent = ?parent_hash,
			?included_hash,
			?para_slot,
			depth,
			package,
			submit_target = ?core,
			can_build,
			"The turn stops here: {}.",
			stopped,
		);
		Ok(false)
	}
}
