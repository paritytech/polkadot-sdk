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

//! The JAM block-builder task (phase 5).
//!
//! A wall-clock **para-slot timer** drives authoring — one tick per parachain slot (the Aura slot
//! duration, 6 s today) — instead of one iteration per JAM best-block notification, which could
//! never pace a parachain producing several blocks per JAM slot. The JAM best-block subscription
//! is demoted to a tip cache that the tick reads synchronously; the anchor is that cached tip.
//!
//! Per tick: prune the [`UnincludedSegment`] against the para head JAM has accumulated, ask the
//! runtime whether one more block fits on top (`can_build_upon` — capacity and velocity are
//! runtime-owned), author on the deepest in-flight block, import it, and hand
//! `(block, proof, context)` to the collation task. Blocks stay in the segment until JAM
//! accumulates them, so authoring no longer waits for inclusion.
//!
//! Phases 1–5 inject a **mocked** parachain inherent (the runtime still requires
//! `set_validation_data`); its fake relay slot follows the wall-clock JAM slot. Both the mocked
//! validation data and the timestamp travel *inside* the block, so importers validate them
//! instead of recomputing them from their own clock — exactly as in relay mode.

use super::{
	JamCollatorMessage, LOG_TARGET, fetch_anchor_state_proof, jam_slot_as_relay_slot, jam_slot_at,
};
use crate::common::{
	ConstructNodeRuntimeApi, NodeBlock,
	aura::{AuraIdT, AuraRuntimeApi},
	types::ParachainClient,
};
use codec::{Decode, Encode};
use cumulus_client_consensus_aura::collator::SlotClaim;
use cumulus_client_parachain_inherent::MockValidationDataInherentDataProvider;
use cumulus_primitives_aura::AuraUnincludedSegmentApi;
use cumulus_primitives_core::{CollectCollationInfo, RelayParentOffsetApi};
use futures::{FutureExt, StreamExt, channel::mpsc};
use jam_cumulus_facade::service_state::{ParaInfo, para_info_key};
use jam_interface::{
	BlockDesc, HeaderHash, JamChainSource, JamStateSource, ServiceId, Slot as JamSlot,
	StateRootHash,
};
use jam_state_helpers::StateProof;
use jam_types::RefineContext;
use polkadot_primitives::{HeadData, Id as ParaId, UpgradeGoAhead};
use sc_consensus::{BlockImport, StateAction};
use sc_consensus_aura::standalone as aura_internal;
use sp_api::{ProofRecorder, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_consensus::{Environment, ProposeArgs, Proposer};
use sp_consensus_aura::{AuraApi, Slot, SlotDuration};
use sp_externalities::Extensions;
use sp_inherents::{InherentData, InherentDataProvider};
use sp_keystore::KeystorePtr;
use sp_runtime::traits::{Header as HeaderT, UniqueSaturatedInto};
use sp_timestamp::Timestamp;
use sp_trie::proof_size_extension::ProofSizeExt;
use std::{
	collections::VecDeque,
	future::Future,
	sync::Arc,
	time::{Duration, Instant},
};

const PROPOSAL_DURATION: Duration = Duration::from_millis(2000);
/// Phase-1 PoV budget; generous for a mostly-empty test chain, small enough for any JAM
/// work-package size limit.
const MAX_POV_SIZE: usize = 3 * 1024 * 1024;
/// How many JAM blocks back from the cached tip the anchor is taken.
///
/// Zero anchors at the tip itself, which buys a slot of proof freshness; the knob mirrors the
/// relay collator's `relay_parent_offset` for the day a fork-prone JAM chain wants distance.
const ANCHOR_OFFSET: u32 = 0;
/// How far the cached JAM tip may lag the wall clock before a tick is skipped.
///
/// Notification driving could not build on a stalled JAM chain — no notification, no block. A
/// wall-clock timer keeps ticking regardless, so it needs to be told when the tip it caches went
/// stale (JAM stalled, or the subscription died silently) rather than anchoring against it.
const MAX_TIP_LAG_SLOTS: JamSlot = 2;
/// Sanity bound on the number of in-flight blocks.
///
/// The runtime's consensus hook owns capacity for real (`can_build_upon`); this only stops a
/// runaway should that ever answer wrongly, and tripping it is a bug worth a loud log.
const MAX_UNINCLUDED: usize = 8;

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

/// The parachain blocks that have been authored but not accumulated by JAM yet.
///
/// The JAM twin of Cumulus' unincluded segment: `included` is the para head proven in JAM state
/// at the anchor, `entries` are the blocks chained on top of it, oldest first, each one the child
/// of its predecessor.
struct UnincludedSegment<Header> {
	/// The para head accumulated in JAM state; `None` until the para's first block lands.
	included: Option<Header>,
	entries: VecDeque<Header>,
}

/// What reading the accumulated para head did to the segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PruneOutcome {
	/// The included head moved and `popped` in-flight blocks became accumulated. Zero means it
	/// moved while we had nothing in flight — another collator authored that block.
	Advanced { popped: usize },
	/// The included head is still where it was; the segment is untouched.
	Unchanged,
	/// The included head is a block we do not have in flight.
	Diverged,
}

impl<Header: HeaderT> UnincludedSegment<Header> {
	fn new() -> Self {
		Self { included: None, entries: VecDeque::new() }
	}

	/// Fold a freshly read accumulated head into the segment.
	///
	/// The head JAM accepted either is one of our in-flight blocks (everything up to and
	/// including it is now accumulated and leaves the segment) or it is not — in which case
	/// another collator's chain won and everything we still hold is orphaned. Dropping it all is
	/// the correct move: the next tick rebuilds from the head JAM accepted (drop-tail self-heal),
	/// whereas keeping the orphans would only produce blocks the service refuses to chain on.
	fn prune(&mut self, included: Option<Header>) -> PruneOutcome {
		let new_head = included.as_ref().map(|header| header.hash());
		if new_head == self.included.as_ref().map(|header| header.hash()) {
			return PruneOutcome::Unchanged;
		}

		let position =
			new_head.and_then(|hash| self.entries.iter().position(|entry| entry.hash() == hash));
		self.included = included;
		match position {
			Some(index) => {
				self.entries.drain(..=index);
				PruneOutcome::Advanced { popped: index + 1 }
			},
			None if self.entries.is_empty() => PruneOutcome::Advanced { popped: 0 },
			None => {
				self.entries.clear();
				PruneOutcome::Diverged
			},
		}
	}

	/// The block to author on: the deepest in-flight block, else the accumulated head. `None`
	/// means the para has no head at all yet and the next block is its first.
	fn tip(&self) -> Option<&Header> {
		self.entries.back().or(self.included.as_ref())
	}

	fn push(&mut self, header: Header) {
		self.entries.push_back(header);
	}

	/// Forget everything in flight, keeping the accumulated head. Sent by the collation task when
	/// it gives up on the packages carrying those blocks.
	fn reset(&mut self) {
		self.entries.clear();
	}

	fn depth(&self) -> usize {
		self.entries.len()
	}

	fn is_full(&self) -> bool {
		self.entries.len() >= MAX_UNINCLUDED
	}

	fn entry_hashes(&self) -> Vec<Header::Hash> {
		self.entries.iter().map(|entry| entry.hash()).collect()
	}
}

/// What the builder remembers between ticks.
struct BuilderState<Header> {
	/// Aura guard: the parachain slot last claimed.
	last_claimed_slot: Option<Slot>,
	segment: UnincludedSegment<Header>,
}

/// Everything one tick reads from JAM at its anchor.
struct AnchorReads {
	anchor: BlockDesc,
	context: RefineContext,
	state_root: StateRootHash,
	/// The para's entry in the service's state, exactly as stored; `None` = no head yet.
	included: Option<Vec<u8>>,
	proof: StateProof,
}

/// How long until the next para slot starts.
///
/// Mirrors the relay slot-based collator's timer (`slot_based/slot_timer.rs`): slots are aligned
/// to the unix epoch, so all collators tick on the same boundaries.
fn time_until_next_para_slot(now: Duration, slot_duration: Duration) -> Duration {
	let now = now.as_millis();
	let slot_millis = slot_duration.as_millis().max(1);
	let next_slot_start = ((now + slot_millis) / slot_millis) * slot_millis;
	Duration::from_millis((next_slot_start - now) as u64)
}

/// One block per para slot, the Aura guard carried over from phase 1 — now on the wall-clock
/// para slot rather than the anchor's timeslot.
fn para_slot_claimed(last_claimed: Option<Slot>, para_slot: Slot) -> bool {
	last_claimed.is_some_and(|last| para_slot <= last)
}

/// Whether the cached JAM tip is recent enough to anchor against.
fn tip_is_fresh(wall_jam_slot: JamSlot, tip_slot: JamSlot) -> bool {
	wall_jam_slot.saturating_sub(tip_slot) <= MAX_TIP_LAG_SLOTS
}

/// Run one JAM read, logging the method, the block it was read at, what came back and the
/// round-trip: latency is a first-class debugging signal on this path.
async fn jam_read<T, F>(method: &'static str, at: HeaderHash, read: F) -> Result<T, String>
where
	F: Future<Output = jam_interface::Result<T>>,
	T: std::fmt::Debug,
{
	let started = Instant::now();
	let result = read.await;
	let elapsed_ms = started.elapsed().as_millis();
	match &result {
		Ok(value) => tracing::debug!(
			target: LOG_TARGET,
			method,
			?at,
			?value,
			elapsed_ms,
			"JAM read.",
		),
		Err(error) => tracing::warn!(
			target: LOG_TARGET,
			method,
			?at,
			?error,
			elapsed_ms,
			"JAM read failed.",
		),
	}
	result.map_err(|error| format!("{method}: {error}"))
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
		anchor_offset = ANCHOR_OFFSET,
		max_tip_lag_slots = MAX_TIP_LAG_SLOTS,
		"JAM builder task started; building one block per parachain slot.",
	);

	// The segment starts empty because 5.2 only ever feeds it blocks we author ourselves;
	// reconstructing what is already in flight (after a restart, or authored by another
	// collator) is filled from JAM state in 5.4.
	let mut state = BuilderState { last_claimed_slot: None, segment: UnincludedSegment::new() };
	let mut cached_tip: Option<BlockDesc> = None;
	loop {
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
				rebuild = rebuild_receiver.next() => match rebuild {
					Some(()) => {
						tracing::info!(
							target: LOG_TARGET,
							dropped = state.segment.depth(),
							entries = ?state.segment.entry_hashes(),
							"Segment reset requested by the collation task; dropping the \
							 in-flight blocks and rebuilding from the accumulated head.",
						);
						state.segment.reset();
					},
					None => {
						tracing::error!(
							target: LOG_TARGET,
							"Collation task is gone; stopping the builder task."
						);
						return;
					},
				},
			}
		}

		let Some(tip) = cached_tip else {
			tracing::debug!(
				target: LOG_TARGET,
				"No JAM best block seen yet; skipping this parachain slot.",
			);
			continue;
		};

		match run_tick::<Block, RuntimeApi, AuraId, _, _, _>(
			&para_client,
			&mut block_import,
			&mut proposer_factory,
			&keystore,
			para_id,
			service_id,
			&*jam,
			slot_duration,
			tip,
			Timestamp::current(),
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
					?tip,
					"Failed to build a block in this parachain slot.",
				);
			},
		}
	}
}

/// One para-slot tick: guards, JAM reads, segment maintenance, capacity gate, authoring.
///
/// `Ok(None)` means the tick was skipped; the reason is always logged where it was decided.
async fn run_tick<Block, RuntimeApi, AuraId, BI, PF, Jam>(
	para_client: &Arc<ParachainClient<Block, RuntimeApi>>,
	block_import: &mut BI,
	proposer_factory: &mut PF,
	keystore: &KeystorePtr,
	para_id: ParaId,
	service_id: ServiceId,
	jam: &Jam,
	slot_duration: SlotDuration,
	tip: BlockDesc,
	now: Timestamp,
	state: &mut BuilderState<Block::Header>,
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
	// Everything time-derived comes from this one instant: the block's Aura slot and the mocked
	// inherent's fake relay slot have to agree, and the consensus hook panics if they do not.
	let para_slot = Slot::from_timestamp(now, slot_duration);
	let wall_jam_slot = jam_slot_at(now);
	tracing::debug!(
		target: LOG_TARGET,
		?para_slot,
		wall_jam_slot,
		timestamp = now.as_millis(),
		?tip,
		tip_lag_slots = wall_jam_slot.saturating_sub(tip.slot),
		tip_fresh = tip_is_fresh(wall_jam_slot, tip.slot),
		segment_depth = state.segment.depth(),
		"Parachain slot tick.",
	);

	if para_slot_claimed(state.last_claimed_slot, para_slot) {
		tracing::debug!(
			target: LOG_TARGET,
			?para_slot,
			last_claimed_slot = ?state.last_claimed_slot,
			"Parachain slot already claimed; skipping this tick.",
		);
		return Ok(None);
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
		return Ok(None);
	}

	let para_id_u32: u32 = para_id.into();
	// The tick is the reads' budget: a JAM node that cannot answer within one parachain slot has
	// nothing fresh to say, and the next tick supersedes this one anyway.
	let reads = match tokio::time::timeout(
		slot_duration.as_duration(),
		read_anchor(jam, tip, service_id, para_id_u32),
	)
	.await
	{
		Ok(reads) => reads?,
		Err(_) => {
			tracing::warn!(
				target: LOG_TARGET,
				?tip,
				budget_ms = slot_duration.as_millis(),
				"JAM reads did not finish within the parachain slot; skipping this tick.",
			);
			return Ok(None);
		},
	};
	let Some(AnchorReads { anchor, context, state_root, included, proof: anchor_state_proof }) =
		reads
	else {
		return Ok(None);
	};

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
	tracing::debug!(
		target: LOG_TARGET,
		anchor = ?anchor.header_hash,
		included_head = ?included_head.as_ref().map(|header| header.hash()),
		included_number = ?included_head.as_ref().map(|header| *header.number()),
		"Para head accumulated in JAM state at the anchor.",
	);

	let head_before = state.segment.included.as_ref().map(|header| header.hash());
	let entries_before = state.segment.entry_hashes();
	let outcome = state.segment.prune(included_head);
	// The level is the only thing that varies, and `tracing` needs it at the macro's call site.
	macro_rules! log_prune {
		($level:ident) => {
			tracing::$level!(
				target: LOG_TARGET,
				?outcome,
				?head_before,
				head_after = ?state.segment.included.as_ref().map(|header| header.hash()),
				entries_before = ?entries_before,
				entries_after = ?state.segment.entry_hashes(),
				depth = state.segment.depth(),
				"Unincluded segment pruned against the accumulated head.",
			)
		};
	}
	match outcome {
		PruneOutcome::Diverged => log_prune!(warn),
		PruneOutcome::Advanced { popped } if popped > 0 => log_prune!(info),
		_ => log_prune!(debug),
	}

	let genesis_hash = para_client.info().genesis_hash;
	let parent_source = match (state.segment.depth(), state.segment.included.is_some()) {
		(0, false) => "genesis",
		(0, true) => "accumulated head",
		_ => "deepest in-flight block",
	};
	let parent_header = match state.segment.tip() {
		Some(header) => header.clone(),
		// Nothing accumulated and nothing in flight, so the next block is the para's first one.
		None => para_client
			.header(genesis_hash)
			.map_err(|e| format!("genesis header: {e}"))?
			.ok_or_else(|| format!("genesis header {genesis_hash:?} not found"))?,
	};
	let parent_hash = parent_header.hash();
	tracing::debug!(
		target: LOG_TARGET,
		parent_source,
		parent = ?parent_hash,
		parent_number = %parent_header.number(),
		depth = state.segment.depth(),
		"Parent selected.",
	);
	// In-flight entries are blocks we authored and imported, so this only ever fires for a head
	// accumulated from a block we have not seen (another collator's, before it reached us).
	if para_client.header(parent_hash).map_err(|e| format!("parent header: {e}"))?.is_none() {
		tracing::warn!(
			target: LOG_TARGET,
			parent = ?parent_hash,
			parent_number = %parent_header.number(),
			"The selected parent is not known locally; waiting for import/sync.",
		);
		return Ok(None);
	}

	if state.segment.is_full() {
		tracing::error!(
			target: LOG_TARGET,
			depth = state.segment.depth(),
			max_unincluded = MAX_UNINCLUDED,
			"The unincluded segment hit the local sanity bound; the runtime's capacity gate \
			 should have stopped us long before. Skipping this tick.",
		);
		return Ok(None);
	}

	let included_hash =
		state.segment.included.as_ref().map(|header| header.hash()).unwrap_or(genesis_hash);
	let started = Instant::now();
	let can_build = can_build_upon::<Block, RuntimeApi, AuraId>(
		para_client,
		parent_hash,
		included_hash,
		para_slot,
	)?;
	tracing::debug!(
		target: LOG_TARGET,
		parent = ?parent_hash,
		?included_hash,
		?para_slot,
		depth = state.segment.depth(),
		can_build,
		elapsed_ms = started.elapsed().as_millis(),
		"Asked the runtime whether the unincluded segment has room.",
	);
	if !can_build {
		tracing::info!(
			target: LOG_TARGET,
			parent = ?parent_hash,
			?included_hash,
			?para_slot,
			depth = state.segment.depth(),
			"The runtime refuses another block on this parent (capacity or velocity); \
			 skipping this tick.",
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
		SlotClaim::unchecked::<<AuraId as AuraIdT>::BoundedPair>(author_pub, para_slot, now);

	tracing::info!(
		target: LOG_TARGET,
		?tip,
		?anchor,
		?para_slot,
		wall_jam_slot,
		timestamp = now.as_millis(),
		parent = ?parent_hash,
		parent_number = %parent_header.number(),
		depth = state.segment.depth(),
		"Building a parachain block against the JAM anchor.",
	);

	let inherent_data = create_inherent_data::<Block, RuntimeApi>(
		para_client,
		para_id,
		&parent_header,
		wall_jam_slot,
		now,
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
	state.segment.push(block.header().clone());
	tracing::info!(
		target: LOG_TARGET,
		block_hash = ?block.hash(),
		block_number = %block.header().number(),
		extrinsics = block.extrinsics().len(),
		proof_nodes = proof.iter_nodes().count(),
		anchor_proof_nodes = anchor_state_proof.nodes.len(),
		depth = state.segment.depth(),
		"Built and imported a parachain block.",
	);

	Ok(Some(JamCollatorMessage {
		parent_header,
		block,
		proof,
		context,
		anchor_state_root: *state_root,
		anchor_state_proof,
		anchor_slot: anchor.slot,
		triggered_by: tip,
	}))
}

/// The JAM reads one tick makes at its anchor, plus the refine context built around it.
///
/// `Ok(None)` means the reads disagree with each other and the tick has to be skipped.
async fn read_anchor<Jam: JamChainSource + JamStateSource + ?Sized>(
	jam: &Jam,
	tip: BlockDesc,
	service_id: ServiceId,
	para_id: u32,
) -> Result<Option<AnchorReads>, String> {
	// Anchoring at the cached tip itself is what buys back the freshness the phase-4 "parent of
	// best" convention gave away; `ANCHOR_OFFSET` walks back from it when distance is wanted.
	let mut anchor = tip;
	for _ in 0..ANCHOR_OFFSET {
		anchor = jam_read("parent", anchor.header_hash, jam.parent(anchor.header_hash)).await?;
	}
	let state_root =
		jam_read("stateRoot", anchor.header_hash, jam.state_root(anchor.header_hash)).await?;
	let beefy_root =
		jam_read("beefyRoot", anchor.header_hash, jam.beefy_root(anchor.header_hash)).await?;
	let finalized = jam_read("finalizedBlock", anchor.header_hash, jam.finalized_block()).await?;
	let lookup_anchor =
		jam_read("parent", finalized.header_hash, jam.parent(finalized.header_hash)).await?;
	let included = jam_read(
		"serviceValue",
		anchor.header_hash,
		jam.service_value(anchor.header_hash, service_id, &para_info_key(para_id.into())),
	)
	.await?;

	let started = Instant::now();
	let (proof, proved_head) =
		fetch_anchor_state_proof(jam, anchor.header_hash, &state_root, service_id, para_id).await?;
	tracing::debug!(
		target: LOG_TARGET,
		method = "stateProof",
		at = ?anchor.header_hash,
		nodes = proof.nodes.len(),
		values = proof.values.len(),
		proved = proved_head.is_some(),
		elapsed_ms = started.elapsed().as_millis(),
		"JAM read.",
	);
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
			 skipping this tick.",
		);
		return Ok(None);
	}

	Ok(Some(AnchorReads {
		anchor,
		context: RefineContext {
			anchor: anchor.header_hash,
			state_root,
			beefy_root,
			lookup_anchor: lookup_anchor.header_hash,
			lookup_anchor_slot: lookup_anchor.slot,
			prerequisites: Default::default(),
		},
		state_root,
		included,
		proof,
	}))
}

/// Ask the runtime whether one more block may be chained onto `parent`.
///
/// Capacity and velocity are runtime-owned (`FixedVelocityConsensusHook`), and on JAM there is no
/// second, relay-side depth cap behind them — this answer is the only hard limit there is.
/// Mirrors `cumulus_client_consensus_aura::collators::can_build_upon`: with an empty segment the
/// runtime cannot recognise the parent as the included block (it never sees its own hash), so
/// that case is decided here. The para slot doubles as the API's relay slot because under the
/// mocked inherent both are the same wall-clock 6 s slot number.
fn can_build_upon<Block, RuntimeApi, AuraId>(
	para_client: &Arc<ParachainClient<Block, RuntimeApi>>,
	parent_hash: Block::Hash,
	included_hash: Block::Hash,
	para_slot: Slot,
) -> Result<bool, String>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
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

/// Timestamp + mocked parachain inherent, both derived from the wall clock.
///
/// Mirrors omni-node's relay-less dev mode, except the fake relay slot is the wall-clock JAM
/// timeslot instead of a plain wall-clock relay slot. Both values end up *inside* the block (as
/// the timestamp and `set_validation_data` extrinsics), so importers validate them rather than
/// recomputing them from their own clock — the same contract as in relay mode.
async fn create_inherent_data<Block, RuntimeApi>(
	para_client: &Arc<ParachainClient<Block, RuntimeApi>>,
	para_id: ParaId,
	parent_header: &Block::Header,
	wall_jam_slot: JamSlot,
	timestamp: Timestamp,
	slot_duration: SlotDuration,
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
	let target_relay_slot = jam_slot_as_relay_slot(wall_jam_slot);
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

	let timestamp_provider = sp_timestamp::InherentDataProvider::new(timestamp);

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
	use sp_core::H256;
	use sp_runtime::traits::BlakeTwo256;

	type TestHeader = sp_runtime::generic::Header<u32, BlakeTwo256>;

	/// A chain of headers, each one the child of its predecessor — the shape the segment holds.
	fn chain(length: u32) -> Vec<TestHeader> {
		let mut parent = H256::zero();
		(1..=length)
			.map(|number| {
				let header = TestHeader::new(
					number,
					Default::default(),
					Default::default(),
					parent,
					Default::default(),
				);
				parent = header.hash();
				header
			})
			.collect()
	}

	fn segment_of(
		included: Option<TestHeader>,
		entries: &[TestHeader],
	) -> UnincludedSegment<TestHeader> {
		UnincludedSegment { included, entries: entries.iter().cloned().collect() }
	}

	/// The normal pipelined case: JAM accumulated the oldest in-flight block, so exactly that one
	/// block leaves the segment and the rest stay in flight.
	#[test]
	fn the_accumulated_block_leaves_the_segment() {
		let chain = chain(3);
		let mut segment = segment_of(None, &chain);

		assert_eq!(segment.prune(Some(chain[0].clone())), PruneOutcome::Advanced { popped: 1 });
		assert_eq!(segment.entry_hashes(), vec![chain[1].hash(), chain[2].hash()]);
	}

	/// Accumulation can catch up by several blocks at once (one JAM block accumulating a chain of
	/// work reports); everything up to and including the new head has to go.
	#[test]
	fn catching_up_pops_every_block_up_to_the_accumulated_head() {
		let chain = chain(4);
		let mut segment = segment_of(None, &chain);

		assert_eq!(segment.prune(Some(chain[2].clone())), PruneOutcome::Advanced { popped: 3 });
		assert_eq!(segment.entry_hashes(), vec![chain[3].hash()]);
	}

	/// The common tick: nothing was accumulated since the last one, so the segment must be left
	/// exactly as it was or the builder would re-author blocks that are still in flight.
	#[test]
	fn an_unmoved_head_leaves_the_segment_alone() {
		let chain = chain(3);
		let mut segment = segment_of(Some(chain[0].clone()), &chain[1..]);

		assert_eq!(segment.prune(Some(chain[0].clone())), PruneOutcome::Unchanged);
		assert_eq!(segment.entry_hashes(), vec![chain[1].hash(), chain[2].hash()]);
	}

	/// A head we have nothing in flight against is another collator's block, not a divergence:
	/// with slot rotation this is what every tick after someone else's slot looks like.
	#[test]
	fn an_empty_segment_just_adopts_the_new_head() {
		let chain = chain(2);
		let mut segment = segment_of(Some(chain[0].clone()), &[]);

		assert_eq!(segment.prune(Some(chain[1].clone())), PruneOutcome::Advanced { popped: 0 });
		assert_eq!(segment.included.map(|header| header.hash()), Some(chain[1].hash()));
	}

	/// A head that is none of our in-flight blocks means another chain won; our blocks are
	/// orphaned and keeping them would only produce blocks the service refuses to chain on.
	#[test]
	fn a_foreign_head_diverges_and_clears_the_segment() {
		let ours = chain(3);
		let theirs = chain(2)
			.into_iter()
			.map(|mut header| {
				header.state_root = H256::repeat_byte(0xaa);
				header
			})
			.collect::<Vec<_>>();
		let mut segment = segment_of(Some(ours[0].clone()), &ours[1..]);

		assert_eq!(segment.prune(Some(theirs[1].clone())), PruneOutcome::Diverged);
		assert!(segment.entry_hashes().is_empty());
		assert_eq!(segment.included.map(|header| header.hash()), Some(theirs[1].hash()));
	}

	/// Authoring always extends the deepest block in flight — that is what pipelining is — and
	/// falls back to the accumulated head once the segment drains.
	#[test]
	fn the_tip_is_the_deepest_in_flight_block_else_the_accumulated_head() {
		let chain = chain(3);
		let mut segment = segment_of(Some(chain[0].clone()), &chain[1..]);
		assert_eq!(segment.tip().map(|header| header.hash()), Some(chain[2].hash()));

		segment.reset();
		assert_eq!(segment.tip().map(|header| header.hash()), Some(chain[0].hash()));

		assert!(segment_of(None, &[]).tip().is_none());
	}

	/// The local bound is a backstop for a runtime that answers `can_build_upon` wrongly, so it
	/// must trip exactly at the depth it names.
	#[test]
	fn the_sanity_bound_trips_at_max_unincluded() {
		let chain = chain(MAX_UNINCLUDED as u32);
		assert!(!segment_of(None, &chain[..MAX_UNINCLUDED - 1]).is_full());
		assert!(segment_of(None, &chain).is_full());
	}

	/// Authoring twice in one parachain slot is equivocation, so the guard must reject a slot it
	/// has already claimed while letting the next one through.
	#[test]
	fn one_block_per_parachain_slot() {
		assert!(!para_slot_claimed(None, Slot::from(7)));
		assert!(para_slot_claimed(Some(Slot::from(7)), Slot::from(7)));
		assert!(para_slot_claimed(Some(Slot::from(7)), Slot::from(6)));
		assert!(!para_slot_claimed(Some(Slot::from(7)), Slot::from(8)));
	}

	/// The guard exists to catch a JAM chain that stalled, so the boundary matters: a tip that is
	/// exactly `MAX_TIP_LAG_SLOTS` behind is still usable, one slot older is not.
	#[test]
	fn a_tip_lagging_more_than_the_bound_is_stale() {
		assert!(tip_is_fresh(100, 100));
		assert!(tip_is_fresh(100, 100 - MAX_TIP_LAG_SLOTS));
		assert!(!tip_is_fresh(100, 100 - MAX_TIP_LAG_SLOTS - 1));
	}

	/// The timer has to land on the next slot boundary; being early would make the tick derive
	/// the previous slot's number and skip itself on the one-block-per-slot guard.
	#[test]
	fn the_timer_waits_for_the_next_slot_boundary() {
		let slot = Duration::from_millis(6000);
		assert_eq!(time_until_next_para_slot(Duration::from_millis(0), slot).as_millis(), 6000);
		assert_eq!(time_until_next_para_slot(Duration::from_millis(1000), slot).as_millis(), 5000);
		assert_eq!(time_until_next_para_slot(Duration::from_millis(5999), slot).as_millis(), 1);
		assert_eq!(time_until_next_para_slot(Duration::from_millis(6000), slot).as_millis(), 6000);
	}

	/// The consensus hook panics unless the block's Aura slot equals the slot it derives from the
	/// mocked relay slot, so both wall-clock derivations must agree at every instant in a slot.
	#[test]
	fn the_aura_slot_matches_the_mocked_relay_slot() {
		let slot_duration = SlotDuration::from_millis(6000);
		for offset_ms in [0, 1, 2999, 5999, 6000, 6001, 123_456] {
			let now = Timestamp::new(jam_types::JAM_COMMON_ERA * 1000 + offset_ms);
			assert_eq!(
				*Slot::from_timestamp(now, slot_duration),
				jam_slot_as_relay_slot(jam_slot_at(now)),
				"disagreement {offset_ms} ms into the JAM common era",
			);
		}
	}
}
