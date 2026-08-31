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

//! The JAM collation manager (phase 5a): the work-package lifecycle for the blocks this
//! collator authors.
//!
//! One task owns everything, instead of the phase-1 follower spawned per package. It keeps the
//! packages *it* submitted — [`InFlightPackages`] — and selects over the builder's channel, the
//! para-head stream, the status subscriptions of every submitted package, and a
//! once-per-JAM-slot timer.
//!
//! Each block the builder hands over becomes **one independent work package**: no prerequisite,
//! no imported segment, `export_count = 0`, submitted with a plain `submitWorkPackage`. Phase 5a
//! removed the in-core link between a block's package and its parent's, because an import
//! authenticates bytes to a *package*, not to a service, and so never carried the security it
//! appeared to; lineage is declared in the work output (the parent head hash refine derives from
//! the block) and settled by the parachain service at accumulate, which applies a head only if
//! it chains onto the stored one and buffers the rest.
//!
//! What a package still carries is the anchor state proof of the para head, inside the PoV. The
//! service verifies it in-core, which is why anchor and PoV are inseparable: re-anchoring a
//! package means re-proving the head and rebuilding the payload, not just swapping the context
//! out. Re-anchoring is legal for *any* package now — nothing names a package's hash any more,
//! so changing it cannot orphan anything.
//!
//! Failure handling is per package and has no tail: a package that can no longer be reported is
//! forgotten. Nothing else has to be undone, because no other package depended on it; the block
//! itself stays in the local database, and the next parachain slot authors on whatever is
//! deepest there.
//!
//! Phase-1 simplifications that still stand: null authorizer (empty token, nothing to sign),
//! fixed core, PoV is NOT zstd-compressed (parasim rejects compressed PoVs; JIP-2 is silent on
//! compression).

use super::{
	ANCHOR_STATE_PROOF_KEY, JAM_SLOT_DURATION_MS, JamCollatorMessage, LOG_TARGET,
	fetch_anchor_state_proof, jam_slot_at, para_head_stream, resubmission::*,
};
use crate::common::{ConstructNodeRuntimeApi, NodeBlock, types::ParachainClient};
use codec::{Decode, Encode};
use cumulus_primitives_core::{AdditionalData, ParachainBlockData, SchedulingProof};
use futures::{
	FutureExt, StreamExt,
	channel::mpsc,
	future::AbortHandle,
	stream::{SelectAll, abortable},
};
use jam_cumulus_facade::{ParachainCandidate, authorizer::fixed_authorizer};
use jam_interface::{
	BoxStream, CoreIndex, HeaderHash, JamChainSource, JamStateSource,
	JamWorkPackageSubmission, ServiceId, Slot as JamSlot, VersionedParameters, WorkPackage,
	WorkPackageHash, WorkPackageStatus,
};
use jam_state_helpers::StateProof;
use jam_types::{Authorization, CodeHash, RefineContext, UnsignedGas, WorkItem, WorkPayload};
use polkadot_primitives::Id as ParaId;
use sp_runtime::traits::{Block as BlockT, HashingFor, Header as HeaderT};
use sp_timestamp::Timestamp;
use sp_trie::CompactProof;
use std::{
	collections::{HashMap, VecDeque},
	sync::Arc,
	time::{Duration, Instant},
};

const RETRY_DELAY: Duration = Duration::from_secs(6);

/// How long a package has to be reported, counted from its anchor: the anchor must still be in
/// JAM's recent history when the package is reported. With no links between packages this is the
/// only such clock left.
const REPORT_DEADLINE_SLOTS: JamSlot = 8;

pub(crate) struct CollationTaskParams<Block: NodeBlock, RuntimeApi, Jam> {
	pub para_client: Arc<ParachainClient<Block, RuntimeApi>>,
	pub jam: Arc<Jam>,
	pub para_id: ParaId,
	pub service_id: ServiceId,
	pub core: CoreIndex,
	pub message_receiver: mpsc::Receiver<JamCollatorMessage<Block>>,
	pub announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
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
		announce_block,
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
		resubmit_after_slots = RESUBMIT_AFTER_SLOTS,
		"JAM collation task started.",
	);

	// Nothing is tracked after a restart and nothing needs to be: the packages this task lost
	// track of are either already accumulated or lost, and the builder authors from the local
	// database and the accumulated head either way.
	let mut manager = Manager {
		para_client,
		jam,
		para_id: para_id.into(),
		service_id,
		core,
		service_code_hash,
		refine_gas_limit,
		accumulate_gas_limit,
		policy: ReanchorThenForget::new(max_resubmits),
		announce_block,
		packages: InFlightPackages::new(),
		included_head: None,
		statuses: SelectAll::new(),
		subscriptions: StatusSubscriptions::new(),
	};

	// One tick per JAM slot, the granularity every deadline in this task is counted in. It is
	// rearmed only when it fires: rebuilding it per iteration would reset it on every block and
	// every status update, and it would never elapse at all.
	let slot = Duration::from_millis(JAM_SLOT_DURATION_MS);
	let mut tick = futures_timer::Delay::new(slot).fuse();
	loop {
		futures::select! {
			message = message_receiver.next() => {
				let Some(message) = message else {
					tracing::error!(target: LOG_TARGET, "Builder task is gone; stopping.");
					return;
				};
				manager.on_new_block(message).await;
			},
			head = para_heads.next() => {
				let Some(head) = head else {
					tracing::error!(target: LOG_TARGET, "Para-head stream ended; stopping.");
					return;
				};
				manager.on_para_head(&head);
			},
			update = manager.statuses.next() => {
				// `None` only means the last subscription ended while nothing is in flight.
				if let Some((wp_hash, status)) = update {
					manager.on_status(wp_hash, status).await;
				}
			},
			_ = tick => {
				tick = futures_timer::Delay::new(slot).fuse();
				manager.on_slot_tick().await;
			},
		}
	}
}

/// One work package this collator submitted, still in flight.
struct InFlight<Block: BlockT> {
	block_hash: Block::Hash,
	block_number: <Block::Header as HeaderT>::Number,
	parent_hash: Block::Hash,
	wp_hash: WorkPackageHash,
	/// The package exactly as submitted. A soft resubmission replays it verbatim, which is what
	/// keeps the hash JAM knows it by — and therefore the status subscription — unchanged.
	package: WorkPackage,
	/// What the package would be rebuilt from around a fresh anchor.
	source: PackageSource<Block>,
	anchored: Anchored,
	/// The JAM slot this package was last submitted in: the zero of the soft-resubmit timer.
	submitted_at: JamSlot,
	reported: bool,
	resubmits: u32,
}

/// The work packages this collator has in flight, in the order they were submitted.
///
/// A plain list, not a chain: phase 5a packages depend on nothing, so one leaving — accumulated,
/// forgotten, superseded — says nothing about any other.
struct InFlightPackages<Block: BlockT> {
	entries: VecDeque<InFlight<Block>>,
}

impl<Block: BlockT> InFlightPackages<Block> {
	fn new() -> Self {
		Self { entries: VecDeque::new() }
	}

	fn len(&self) -> usize {
		self.entries.len()
	}

	fn position_of_package(&self, wp_hash: WorkPackageHash) -> Option<usize> {
		self.entries.iter().position(|entry| entry.wp_hash == wp_hash)
	}

	fn remove(&mut self, index: usize) -> InFlight<Block> {
		self.entries.remove(index).expect("callers only ever pass an index they just found; qed")
	}

	/// Take out every package whose block is at or below `number`, newest first, and say for each
	/// whether it is the block that became the head.
	///
	/// The parachain service applies a head only if it chains onto the stored one and evicts
	/// everything at or below the stored head's height, so a package for a block at that height
	/// or lower has either just accumulated or lost its fork. Either way it is done.
	fn remove_up_to(
		&mut self,
		number: <Block::Header as HeaderT>::Number,
	) -> Vec<InFlight<Block>> {
		let (settled, remaining): (Vec<_>, Vec<_>) =
			self.entries.drain(..).partition(|entry| entry.block_number <= number);
		self.entries = remaining.into();
		settled
	}

	fn block_hashes(&self) -> Vec<Block::Hash> {
		self.entries.iter().map(|entry| entry.block_hash).collect()
	}
}

/// The status subscription following each package in flight, keyed by package hash.
///
/// A subscription is closed on the node only when the client drops its stream, and the stream is
/// dropped out of the select only when it *ends* — which a server-side status stream never does
/// on its own. So every package leaving the chain has to have its stream ended here. Without
/// that the node keeps every subscription this collator ever opened and refuses new ones past
/// its per-connection cap, at which point every further package fails the moment it is
/// submitted (observed live after ~1050 packages: `Too many subscriptions on the connection`).
struct StatusSubscriptions {
	handles: HashMap<WorkPackageHash, AbortHandle>,
}

impl StatusSubscriptions {
	fn new() -> Self {
		Self { handles: HashMap::new() }
	}

	/// Wrap a package's status stream so it can be ended on demand.
	///
	/// A resubmission subscribes again for the same package hash; the earlier subscription is
	/// closed here, so a package holds exactly one however often it is resubmitted.
	fn follow(
		&mut self,
		wp_hash: WorkPackageHash,
		stream: BoxStream<'static, (WorkPackageHash, WorkPackageStatus)>,
	) -> BoxStream<'static, (WorkPackageHash, WorkPackageStatus)> {
		let (stream, handle) = abortable(stream);
		if let Some(previous) = self.handles.insert(wp_hash, handle) {
			previous.abort();
		}
		stream.boxed()
	}

	/// End a package's stream: the select drops it, and dropping it unsubscribes on the node.
	/// `false` means there was nothing to close — the subscription never opened.
	fn close(&mut self, wp_hash: WorkPackageHash) -> bool {
		match self.handles.remove(&wp_hash) {
			Some(handle) => {
				handle.abort();
				true
			},
			None => false,
		}
	}

	fn len(&self) -> usize {
		self.handles.len()
	}
}

/// The hash JAM keys a work package by: blake2b-256 over its encoding.
///
/// polkajam derives it inside its bundle builder, which phase 5a no longer uses; a test pins the
/// two against each other so the status subscriptions keep naming the package the node sees.
fn work_package_hash(package: &WorkPackage) -> WorkPackageHash {
	WorkPackageHash::from(sp_crypto_hashing::blake2_256(&jam_codec::Encode::encode(package)))
}

struct Manager<Block: NodeBlock, RuntimeApi, Jam> {
	para_client: Arc<ParachainClient<Block, RuntimeApi>>,
	jam: Arc<Jam>,
	para_id: u32,
	service_id: ServiceId,
	core: CoreIndex,
	service_code_hash: CodeHash,
	refine_gas_limit: UnsignedGas,
	accumulate_gas_limit: UnsignedGas,
	policy: ReanchorThenForget,
	announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	packages: InFlightPackages<Block>,
	/// The para head last seen in JAM state, for the log alone; `None` until the stream reports
	/// one. No decision reads it: it is a strictly later observation than the anchor a package
	/// carries, and letting it override the anchor is exactly the class of race phase 5 spent
	/// three fixes on.
	included_head: Option<Block::Hash>,
	statuses: SelectAll<BoxStream<'static, (WorkPackageHash, WorkPackageStatus)>>,
	/// One handle per live status subscription, fired when its package leaves the chain.
	subscriptions: StatusSubscriptions,
}

impl<Block, RuntimeApi, Jam> Manager<Block, RuntimeApi, Jam>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	Jam: JamChainSource + JamStateSource + JamWorkPackageSubmission + 'static,
{
	/// A block from the builder: assemble its package, submit it, track it.
	///
	/// There is no linking step left. The package stands on its own: the parent it declares
	/// travels inside the PoV, and the parachain service decides at accumulate whether that
	/// parent is the stored head, a head still to come (buffered) or a fork loser (dropped).
	async fn on_new_block(&mut self, message: JamCollatorMessage<Block>) {
		let JamCollatorMessage {
			parent_header,
			block,
			proof,
			context,
			anchor_state_root,
			anchor_state_proof,
			anchor_slot,
			triggered_by,
		} = message;
		let block_hash = block.hash();
		let block_number = *block.header().number();
		let parent_hash = parent_header.hash();

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

		let validation_code = match self.para_client.code_at(parent_hash) {
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
			service_id: self.service_id,
			service_code_hash: self.service_code_hash,
			refine_gas_limit: self.refine_gas_limit,
			accumulate_gas_limit: self.accumulate_gas_limit,
		};
		let anchored = Anchored {
			context,
			state_root: anchor_state_root,
			head_proof: anchor_state_proof,
			anchor_slot,
		};
		let package = source.package(&anchored);
		let wp_hash = work_package_hash(&package);

		tracing::info!(
			target: LOG_TARGET,
			?block_hash,
			%block_number,
			?parent_hash,
			?wp_hash,
			core = self.core,
			anchor = ?anchored.context.anchor,
			anchor_slot,
			pov_len = package.items[0].payload.0.len(),
			anchor_proof_nodes = anchored.head_proof.nodes.len(),
			in_flight = self.packages.len(),
			?triggered_by,
			"Assembled the work package for the block.",
		);

		(self.announce_block)(block_hash, None);

		let submitted_at = jam_slot_at(Timestamp::current());
		// The entry is recorded even if the submission itself failed: the soft-resubmit timer is
		// what retries it, and it can only retry a package this task still holds.
		self.submit(wp_hash, &package, anchored.context.anchor, block_hash).await;
		self.packages.entries.push_back(InFlight {
			block_hash,
			block_number,
			parent_hash,
			wp_hash,
			package,
			source,
			anchored,
			submitted_at,
			reported: false,
			resubmits: 0,
		});
		self.log_state("a package was submitted");
	}

	/// Submit a package and subscribe to its status; `false` means the submission itself failed.
	///
	/// A plain `submitWorkPackage` with no extrinsics: nothing has to be assembled into a bundle
	/// by hand any more, because the package imports nothing that would have to travel inline.
	async fn submit(
		&mut self,
		wp_hash: WorkPackageHash,
		package: &WorkPackage,
		anchor: HeaderHash,
		block_hash: Block::Hash,
	) -> bool {
		let started = Instant::now();
		let result = self.jam.submit_work_package(self.core, package, Vec::new()).await;
		let elapsed_ms = started.elapsed().as_millis();
		if let Err(error) = result {
			tracing::warn!(
				target: LOG_TARGET,
				?block_hash,
				?wp_hash,
				elapsed_ms,
				?error,
				"Work-package submission failed.",
			);
			return false;
		}
		tracing::info!(
			target: LOG_TARGET,
			?block_hash,
			?wp_hash,
			core = self.core,
			?anchor,
			elapsed_ms,
			"Submitted the work package; following its status.",
		);

		match self.jam.work_package_status_stream(wp_hash, anchor, false).await {
			Ok(stream) => {
				let stream = stream.map(move |status| (wp_hash, status)).boxed();
				let followed = self.subscriptions.follow(wp_hash, stream);
				self.statuses.push(followed);
			},
			Err(error) => tracing::warn!(
				target: LOG_TARGET,
				?wp_hash,
				?block_hash,
				?error,
				"Unable to follow the work-package status; the soft-resubmit timer is the only \
				 thing left watching this package.",
			),
		}
		true
	}

	/// A status update for one of the packages in flight.
	async fn on_status(&mut self, wp_hash: WorkPackageHash, status: WorkPackageStatus) {
		let Some(index) = self.packages.position_of_package(wp_hash) else {
			tracing::debug!(
				target: LOG_TARGET,
				?wp_hash,
				?status,
				"Status update for a package that is no longer in flight; ignoring.",
			);
			return;
		};
		let action = self.policy.on_status(&status);
		tracing::info!(
			target: LOG_TARGET,
			?wp_hash,
			block_hash = ?self.packages.entries[index].block_hash,
			block_number = %self.packages.entries[index].block_number,
			index,
			in_flight = self.packages.len(),
			?status,
			?action,
			"Work-package status update.",
		);
		match action {
			PolicyAction::Wait => {},
			PolicyAction::Done => self.packages.entries[index].reported = true,
			PolicyAction::Resubmit => self.resubmit(index).await,
			PolicyAction::Reanchor => self.reanchor(index, &format!("{status:?}")).await,
			PolicyAction::Forget => self.forget(index, &format!("{status:?}")),
		}
	}

	/// Once per JAM slot: give the policy a look at every package that has not been reported yet.
	async fn on_slot_tick(&mut self) {
		let now = jam_slot_at(Timestamp::current());
		let overdue: Vec<(WorkPackageHash, PolicyAction)> = self
			.packages
			.entries
			.iter()
			.filter(|entry| !entry.reported)
			.map(|entry| {
				let waiting = now.saturating_sub(entry.submitted_at);
				(entry.wp_hash, self.policy.on_silence(waiting, entry.resubmits))
			})
			.filter(|(_, action)| !matches!(action, PolicyAction::Wait))
			.collect();

		// Keyed by package hash rather than by index: forgetting one shifts every index after it,
		// and packages are independent now, so several may come due in the same tick.
		for (wp_hash, action) in overdue {
			let Some(index) = self.packages.position_of_package(wp_hash) else { continue };
			match action {
				PolicyAction::Resubmit => self.resubmit(index).await,
				PolicyAction::Reanchor =>
					self.reanchor(index, "no report within the resubmit budget").await,
				PolicyAction::Forget =>
					self.forget(index, "no report within the resubmit budget"),
				PolicyAction::Wait | PolicyAction::Done => {},
			}
		}
	}

	/// Send the very same package again.
	///
	/// Same bytes means the same work-package hash, so JAM sees one package however often it is
	/// repeated and the status subscription this task already holds keeps naming it.
	async fn resubmit(&mut self, index: usize) {
		let now = jam_slot_at(Timestamp::current());
		let entry = &mut self.packages.entries[index];
		entry.resubmits += 1;
		entry.submitted_at = now;
		let (wp_hash, package, anchor, block_hash, resubmits) = (
			entry.wp_hash,
			entry.package.clone(),
			entry.anchored.context.anchor,
			entry.block_hash,
			entry.resubmits,
		);
		tracing::info!(
			target: LOG_TARGET,
			?block_hash,
			?wp_hash,
			index,
			resubmits,
			in_flight = self.packages.len(),
			"No report yet; resubmitting the identical package.",
		);
		self.submit(wp_hash, &package, anchor, block_hash).await;
	}

	/// Rebuild a package around a fresh anchor and a fresh para-head proof, and submit it as the
	/// new package it now is.
	///
	/// Phase 5a made this legal for *any* package: a re-anchored package has different bytes and
	/// therefore a different hash, which used to orphan every child that had named the old one.
	/// Nothing names a package's hash any more. If the re-anchoring cannot be completed the
	/// package is forgotten, which is what would have happened anyway.
	async fn reanchor(&mut self, index: usize, reason: &str) {
		let entry = &self.packages.entries[index];
		let block_hash = entry.block_hash;
		self.log_deadline(index, jam_slot_at(Timestamp::current()), reason);

		let Ok(anchored) =
			recontext(&*self.jam, self.service_id, self.para_id, &entry.anchored, block_hash).await
		else {
			self.forget(index, "the package failed and could not be re-anchored");
			return;
		};

		let entry = &self.packages.entries[index];
		let old_wp_hash = entry.wp_hash;
		let package = entry.source.package(&anchored);
		let wp_hash = work_package_hash(&package);
		let anchor = anchored.context.anchor;
		tracing::info!(
			target: LOG_TARGET,
			?block_hash,
			?old_wp_hash,
			new_wp_hash = ?wp_hash,
			?anchor,
			anchor_slot = anchored.anchor_slot,
			reason,
			"Re-anchored the package; nothing names a package's hash, so this breaks no links.",
		);

		if !self.submit(wp_hash, &package, anchor, block_hash).await {
			self.forget(index, "the re-anchored package could not be submitted");
			return;
		}
		// The old package hash is gone, so its subscription has to go with it.
		self.stop_following(old_wp_hash, "re-anchored");
		let submitted_at = jam_slot_at(Timestamp::current());
		let entry = &mut self.packages.entries[index];
		entry.wp_hash = wp_hash;
		entry.package = package;
		entry.anchored = anchored;
		entry.submitted_at = submitted_at;
		entry.resubmits += 1;
		entry.reported = false;
	}

	/// Give up on a package.
	///
	/// Nothing else has to be undone — no other package named this one — and the block itself
	/// stays in the local database, so the next parachain slot simply authors on whatever is
	/// deepest there. What this does cost is the parachain's progress until somebody resubmits
	/// the missing package: descendants of the lost block sit in the service's reorder buffer
	/// until the buffer evicts them. Resubmission by another collator is phase-7 work, so this
	/// logs loudly enough to be the thing a stalled parachain is diagnosed from.
	fn forget(&mut self, index: usize, reason: &str) {
		let entry = self.packages.remove(index);
		self.stop_following(entry.wp_hash, "forgotten");
		tracing::warn!(
			target: LOG_TARGET,
			block_hash = ?entry.block_hash,
			block_number = %entry.block_number,
			parent_hash = ?entry.parent_hash,
			wp_hash = ?entry.wp_hash,
			reason,
			resubmits = entry.resubmits,
			in_flight = self.packages.len(),
			"Giving up on a work package. Its block stays in the local database, so authoring \
			 continues, but nothing this collator does will make that block accumulate.",
		);
		self.log_state("a package was forgotten");
	}

	/// How much of the package's one deadline was left when it failed.
	fn log_deadline(&self, index: usize, now: JamSlot, reason: &str) {
		let entry = &self.packages.entries[index];
		let anchor_age = now.saturating_sub(entry.anchored.anchor_slot);
		tracing::warn!(
			target: LOG_TARGET,
			block_hash = ?entry.block_hash,
			parent_hash = ?entry.parent_hash,
			wp_hash = ?entry.wp_hash,
			index,
			reason,
			now_jam_slot = now,
			anchor = ?entry.anchored.context.anchor,
			anchor_slot = entry.anchored.anchor_slot,
			anchor_age,
			anchor_expired = anchor_age > REPORT_DEADLINE_SLOTS,
			deadline_slots = REPORT_DEADLINE_SLOTS,
			submitted_at = entry.submitted_at,
			resubmits = entry.resubmits,
			"A work package failed; here is what its anchor deadline had left.",
		);
	}

	/// The para head advanced in JAM state.
	///
	/// Everything at or below the new head's height is settled: the service applies a head only
	/// if it chains onto the stored one, and sweeps the rest of that height out of its reorder
	/// buffer. So those packages either just accumulated or lost their fork, and either way this
	/// task is done with them.
	fn on_para_head(&mut self, head: &[u8]) {
		let header = match Block::Header::decode(&mut &head[..]) {
			Ok(header) => header,
			Err(error) => {
				tracing::warn!(
					target: LOG_TARGET,
					?error,
					head = ?format!("0x{}", hex_prefix(head)),
					"Para head in JAM state does not decode as a header.",
				);
				return;
			},
		};
		let hash = header.hash();
		let number = *header.number();
		self.included_head = Some(hash);

		let settled = self.packages.remove_up_to(number);
		let accumulated = settled.iter().any(|entry| entry.block_hash == hash);
		let superseded: Vec<Block::Hash> = settled
			.iter()
			.filter(|entry| entry.block_hash != hash)
			.map(|entry| entry.block_hash)
			.collect();
		for entry in settled {
			let why = if entry.block_hash == hash { "accumulated" } else { "superseded" };
			self.stop_following(entry.wp_hash, why);
		}

		tracing::info!(
			target: LOG_TARGET,
			block_hash = ?hash,
			block_number = %number,
			accumulated,
			?superseded,
			remaining = self.packages.len(),
			"Para head advanced in JAM state; a package of ours accumulated if `accumulated`, and \
			 anything of ours at or below that height is settled either way.",
		);
		self.log_state("the para head advanced");
	}

	/// Close one package's status subscription. Every path that stops tracking a package goes
	/// through here: a handle outliving its entry is a subscription the node holds open for the
	/// rest of the connection's life, and enough of those stop the collator dead.
	fn stop_following(&mut self, wp_hash: WorkPackageHash, why: &str) {
		if self.subscriptions.close(wp_hash) {
			tracing::debug!(
				target: LOG_TARGET,
				?wp_hash,
				why,
				live_subscriptions = self.subscriptions.len(),
				"Closed a work-package status subscription.",
			);
		}
	}

	fn log_state(&self, after: &str) {
		tracing::debug!(
			target: LOG_TARGET,
			after,
			in_flight = self.packages.len(),
			blocks = ?self.packages.block_hashes(),
			included_head = ?self.included_head,
			live_subscriptions = self.subscriptions.len(),
			"In-flight work packages.",
		);
	}
}

fn hex_prefix(bytes: &[u8]) -> String {
	bytes.iter().take(32).map(|byte| format!("{byte:02x}")).collect()
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
	/// The anchor's timeslot — the start of the window the package has to be reported in.
	anchor_slot: JamSlot,
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

		// Nothing links this package to another: no prerequisite ordering it behind one, no
		// imported segment carrying a parent's header, nothing exported for a child to import.
		// The block's parent travels inside the PoV and the parachain service settles the
		// lineage at accumulate.
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

/// Re-anchor a package: fresh anchor, fresh para-head proof, same block.
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
	let (context, anchor_slot) = match fresh_context(jam).await {
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
		anchor_slot,
		anchor_proof_nodes = head_proof.nodes.len(),
		head_present = proved_head.is_some(),
		"Re-anchored the work package around a fresh anchor and re-proved the para head.",
	);
	Ok(Anchored { state_root: *context.state_root, head_proof, context, anchor_slot })
}

/// The refine context around the current best JAM block (anchor = parent of best, lookup anchor
/// = parent of finalized), as in polkajam's `create_refine_context`, plus the anchor's slot.
async fn fresh_context<Jam>(jam: &Jam) -> jam_interface::Result<(RefineContext, JamSlot)>
where
	Jam: JamChainSource + ?Sized,
{
	let best = jam.best_block().await?;
	let anchor = jam.parent(best.header_hash).await?;
	let state_root = jam.state_root(anchor.header_hash).await?;
	let beefy_root = jam.beefy_root(anchor.header_hash).await?;
	let finalized = jam.finalized_block().await?;
	let lookup_anchor = jam.parent(finalized.header_hash).await?;
	Ok((
		RefineContext {
			anchor: anchor.header_hash,
			state_root,
			beefy_root,
			lookup_anchor: lookup_anchor.header_hash,
			lookup_anchor_slot: lookup_anchor.slot,
			prerequisites: Default::default(),
		},
		anchor.slot,
	))
}

#[cfg(test)]
mod tests {
	use super::*;
	use cumulus_test_runtime::{Block as TestBlock, Header as TestHeader};
	use jam_std_common::build_encoded_bundle;
	use sp_core::H256;

	fn test_proof() -> StateProof {
		StateProof { nodes: vec![[7u8; 64], [8u8; 64]], values: vec![([3u8; 31], vec![9, 9, 9])] }
	}

	fn wp_hash(byte: u8) -> WorkPackageHash {
		WorkPackageHash::from([byte; 32])
	}

	fn header(number: u32, parent: H256) -> TestHeader {
		TestHeader::new(number, H256::repeat_byte(1), H256::repeat_byte(2), parent, <_>::default())
	}

	fn anchored(anchor_slot: JamSlot) -> Anchored {
		Anchored {
			context: RefineContext {
				anchor: HeaderHash::from([9u8; 32]),
				state_root: [4u8; 32].into(),
				beefy_root: [5u8; 32].into(),
				lookup_anchor: HeaderHash::from([6u8; 32]),
				lookup_anchor_slot: anchor_slot,
				prerequisites: Default::default(),
			},
			state_root: [4u8; 32],
			head_proof: test_proof(),
			anchor_slot,
		}
	}

	fn package_source() -> PackageSource<TestBlock> {
		PackageSource {
			blocks: vec![TestBlock::new(header(1, H256::repeat_byte(7)), vec![])],
			proof: CompactProof { encoded_nodes: vec![vec![1u8, 2, 3]] },
			validation_code_hash: [8u8; 32],
			service_id: 42,
			service_code_hash: CodeHash::from([9u8; 32]),
			refine_gas_limit: 1_000,
			accumulate_gas_limit: 1_000,
		}
	}

	/// `count` packages for a parent/child line of blocks, package `k` hashed as `[k; 32]`, all
	/// submitted in slot `k`. The blocks form a chain because that is what a collator authors;
	/// the packages themselves are independent, which is the point.
	fn in_flight(count: u8) -> InFlightPackages<TestBlock> {
		let mut packages = InFlightPackages::new();
		let mut parent_hash = H256::repeat_byte(200);
		for index in 0..count {
			let block_header = header(u32::from(index) + 1, parent_hash);
			let block_hash = block_header.hash();
			packages.entries.push_back(InFlight {
				block_hash,
				block_number: *block_header.number(),
				parent_hash,
				wp_hash: wp_hash(index),
				package: package_source().package(&anchored(JamSlot::from(index))),
				source: package_source(),
				anchored: anchored(JamSlot::from(index)),
				submitted_at: JamSlot::from(index),
				reported: false,
				resubmits: 0,
			});
			parent_hash = block_hash;
		}
		packages
	}

	/// A package stands alone: nothing orders it behind another package, nothing is imported
	/// into it, and nothing is exported out of it for a child to import. This is the whole of
	/// what phase 5a changed on the wire.
	#[test]
	fn a_package_names_no_other_package() {
		let package = package_source().package(&anchored(11));

		assert!(package.context.prerequisites.as_ref().is_empty());
		assert!(package.items[0].import_segments.is_empty());
		assert_eq!(package.items[0].export_count, 0);
		assert!(package.items[0].extrinsics.is_empty());
	}

	/// The hash is the key everything else uses — the status subscription, the manager's own
	/// lookup — so it has to be the hash the node derives. polkajam's bundle builder is the
	/// reference; with no imports and no extrinsics a bundle is just the encoded package, so the
	/// two must agree exactly.
	#[test]
	fn the_package_hash_is_the_one_polkajam_derives() {
		let package = package_source().package(&anchored(11));

		let (reference, bundle) =
			build_encoded_bundle(&package, Vec::<Vec<u8>>::new(), &[Vec::new()]);

		assert_eq!(work_package_hash(&package), reference);
		assert_eq!(bundle, jam_codec::Encode::encode(&package), "nothing travels beside it");
	}

	/// Re-anchoring keeps the block and its witness and changes only the anchor, which is the
	/// whole reason `PackageSource` is kept alongside the submitted package.
	#[test]
	fn re_anchoring_changes_the_package_hash_and_nothing_else() {
		let source = package_source();
		let first = source.package(&anchored(11));
		let second = source.package(&anchored(12));

		assert_ne!(work_package_hash(&first), work_package_hash(&second));
		assert_eq!(first.items[0].payload.0, second.items[0].payload.0, "the PoV is untouched");
	}

	/// A soft resubmission has to be the *same bytes*: a package rebuilt instead of replayed
	/// would hash differently, and JAM would see a second package where the collator meant to
	/// repeat one — a second refine, a second report, and a status subscription following a hash
	/// nothing else knows about.
	#[test]
	fn a_resubmission_replays_the_stored_package() {
		let packages = in_flight(1);
		let entry = &packages.entries[0];

		assert_eq!(work_package_hash(&entry.package), work_package_hash(&entry.package.clone()));
		assert_eq!(
			jam_codec::Encode::encode(&entry.package),
			jam_codec::Encode::encode(&entry.source.package(&entry.anchored)),
			"the stored package is exactly what the source would build again",
		);
	}

	/// The para head advancing settles everything at or below its height: the block that became
	/// the head accumulated, and a package for another block of that height lost the fork — the
	/// service sweeps it out of its reorder buffer by exactly this rule.
	#[test]
	fn the_new_head_settles_every_package_at_or_below_its_height() {
		let mut packages = in_flight(4);
		let third = packages.entries[2].block_number;

		let settled = packages.remove_up_to(third);

		assert_eq!(settled.len(), 3);
		assert_eq!(packages.len(), 1);
		assert_eq!(packages.entries[0].wp_hash, wp_hash(3));
	}

	/// A head deeper than anything this collator has in flight settles the lot; a head below
	/// them all settles nothing.
	#[test]
	fn a_head_past_or_behind_everything_settles_accordingly() {
		let mut packages = in_flight(3);
		assert_eq!(packages.remove_up_to(0).len(), 0, "nothing is at or below height zero");
		assert_eq!(packages.remove_up_to(99).len(), 3);
		assert_eq!(packages.len(), 0);
	}

	/// A package that is neither the head nor below it stays in flight even though its block is
	/// nothing this collator can prove yet: under 5a the service buffers a block whose parent has
	/// not arrived, so a package one height ahead of the head is not lost, it is early.
	#[test]
	fn a_package_above_the_head_is_early_rather_than_lost() {
		let mut packages = in_flight(2);
		let first = packages.entries[0].block_number;

		packages.remove_up_to(first);

		assert_eq!(packages.len(), 1);
		assert_eq!(packages.entries[0].wp_hash, wp_hash(1));
	}

	/// Forgetting one package must leave every other one exactly where it was: packages are
	/// independent now, and dropping a "tail" would throw away blocks that can still accumulate.
	#[test]
	fn forgetting_one_package_keeps_the_others() {
		let mut packages = in_flight(4);
		let index = packages.position_of_package(wp_hash(1)).expect("the package is in flight");

		let forgotten = packages.remove(index);

		assert_eq!(forgotten.wp_hash, wp_hash(1));
		assert_eq!(packages.len(), 3);
		assert_eq!(
			packages.entries.iter().map(|entry| entry.wp_hash).collect::<Vec<_>>(),
			vec![wp_hash(0), wp_hash(2), wp_hash(3)],
		);
	}

	/// A status stream that never ends on its own — which is what a real subscription is, and
	/// the reason nothing leaves the select unless it is ended deliberately.
	fn endless_statuses() -> BoxStream<'static, (WorkPackageHash, WorkPackageStatus)> {
		futures::stream::pending().boxed()
	}

	/// The node closes a status subscription only when the client drops its stream, and it caps a
	/// connection at 1024 of them. A handle that outlives its entry therefore leaks a
	/// subscription the node holds open for good: live, after ~1050 packages, every new
	/// submission failed with "Too many subscriptions on the connection". Whichever way a package
	/// stops being tracked — accumulated, superseded, forgotten, re-anchored — its subscription
	/// has to go with it, and a resubmission must not add a second one.
	#[test]
	fn a_status_subscription_never_outlives_the_package_it_follows() {
		let mut packages = in_flight(4);
		let mut subscriptions = StatusSubscriptions::new();
		let mut statuses = SelectAll::new();
		for entry in &packages.entries {
			statuses.push(subscriptions.follow(entry.wp_hash, endless_statuses()));
		}
		assert_eq!(subscriptions.len(), 4);

		let resubmitted = packages.entries[3].wp_hash;
		statuses.push(subscriptions.follow(resubmitted, endless_statuses()));
		assert_eq!(subscriptions.len(), 4, "resubmitting replaces a package's subscription");

		let settled = packages.entries[1].block_number;
		for entry in packages.remove_up_to(settled) {
			assert!(subscriptions.close(entry.wp_hash), "the settled packages are closed");
		}
		assert_eq!(subscriptions.len(), 2);

		for entry in packages.remove_up_to(99) {
			assert!(subscriptions.close(entry.wp_hash), "and so is a forgotten one");
		}
		assert_eq!(subscriptions.len(), 0, "no handle outlives the packages");
		assert!(!subscriptions.close(resubmitted), "closing twice is not a leak either");
	}

	/// ...and closing has to actually end the stream, because that — and only that — is what
	/// drops it out of the select and unsubscribes on the node side.
	#[test]
	fn closing_a_subscription_ends_its_stream() {
		let mut subscriptions = StatusSubscriptions::new();
		let mut statuses = SelectAll::new();
		statuses.push(subscriptions.follow(wp_hash(1), endless_statuses()));
		assert!(subscriptions.close(wp_hash(1)));

		assert!(
			futures::executor::block_on(statuses.next()).is_none(),
			"the select is empty again, so the subscription was dropped",
		);
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
		let block_header = header(5, parent_hash);
		let anchor_state_root = [4u8; 32];
		let head_proof = test_proof();

		let pov = build_pov(
			&[TestBlock::new(block_header.clone(), vec![])],
			&CompactProof { encoded_nodes: vec![vec![1u8, 2, 3]] },
			anchor_state_root,
			&head_proof,
		);

		let decoded = parasim_service::pov::decode_pov(&pov).expect("the service parses our PoV");
		assert_eq!(decoded.head, block_header.encode(), "the new para head is the encoded header");
		assert_eq!(decoded.parent_hash, <[u8; 32]>::from(parent_hash));

		let (root, proof) =
			<([u8; 32], StateProof)>::decode(&mut &decoded.anchor_state_proof[..])
				.expect("the anchor state proof decodes");
		assert_eq!(root, anchor_state_root);
		assert_eq!(proof, head_proof);
	}
}
