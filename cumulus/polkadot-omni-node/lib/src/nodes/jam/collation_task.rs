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

//! The JAM collation manager: the work-package lifecycle for the blocks this collator authors,
//! including resubmission of lost blocks.
//!
//! One task owns everything. It keeps the packages *it* submitted — [`InFlightPackages`] — and
//! selects over the builder's channel, the para-head stream, the status subscriptions of every
//! submitted package, and the JAM best-block stream. Every new JAM block is one look at β (the
//! recent-blocks history): a package named there is reported, and one whose anchor has expired
//! is forgotten.
//!
//! # New-block path
//!
//! Each block the builder hands over becomes **one work package** with a single extrinsic (the
//! PoV): no imported segment, `export_count = 0`, submitted with `submitWorkPackage`. A package
//! names the work package of the block it builds on as its single prerequisite, when the ledger
//! still remembers it; a miss starts a new chain.
//!
//! # Resubmission
//!
//! When the accumulated head stalls for [`RESUBMIT_STALL_SLOTS`] JAM slots and it is this
//! collator's round-robin turn, a **resubmission round** rebuilds up to
//! [`MAX_RESUBMIT_BLOCKS`] blocks above the head into new work packages: the leading run of the
//! local best chain above the head whose PoVs the store holds and whose original packages can
//! no longer be reported ([`original_package_expired`]) — a block whose first package may still
//! land is not rebuilt beside it. Each resubmission package carries the rebuilt PoV as its only
//! extrinsic, and the PoV's `ParachainBlockData` carries the anchor chain (encoded JAM headers,
//! one walk per round cut per package) that lets the PVF verify the block's `JamParent` digest
//! against the current chain. Package *k* > 0 names package *k* − 1's hash as its prerequisite,
//! so the guarantor processes them in order, and any failure ends the round instead of skipping
//! a block, so no package ever names a prerequisite that was never sent.
//!
//! The resubmission predicate ([`resubmission_due`]) is pure and evaluated after every JAM
//! block. It guards against double rounds via `last_round_at` and against rebuilding while a
//! round is already in flight.
//!
//! # Package policy
//!
//! At every new JAM block β says whether a package appeared on chain. If it did, the package
//! is reported; if the anchor has expired it is forgotten; otherwise it waits. A package is
//! sent once — there is no byte-identical resend — and a package is never re-anchored: its
//! block's `JamParent` digest pins it to the anchor it was built against. A lost block travels
//! again only inside a *new* package whose anchor chain proves that old anchor to the PVF (see
//! Resubmission above).
//!
//! Failure handling is per package and cascades: a forgotten package takes every package that
//! named it, so no child outlives the prerequisite it named. The block stays in the local
//! database and the PoV store, the next parachain slot authors on whatever is deepest there,
//! and a resubmission round rebuilds it once its package has expired.
//!
//! Every package runs under the para's own [AURA authorizer](super::authorizer) and carries a
//! token this collator signs with its aura key.
//!
//! Phase-1 simplification that still stands: the PoV is NOT zstd-compressed (parasim rejects
//! compressed PoVs; JIP-2 is silent on compression).

use super::{
	authorizer::AuraAuthorizer,
	choose_lookup_anchor,
	hash_ledger::WpHashLedger,
	jam_read,
	package::{build_pov, extrinsic_spec, work_package, work_package_hash, PackageParams},
	para_header_stream,
	pov_store::{BlockPov, BlockPovs},
	resubmission::*,
	AuthoringHold, JamCollatorMessage, LOG_TARGET,
};
use crate::common::{
	types::{ParachainBackend, ParachainClient},
	ConstructNodeRuntimeApi, NodeBlock,
};
use cumulus_primitives_core::{jam_anchor_proof::MAX_ANCHOR_CHAIN_LEN, CumulusDigestItem};
use futures::{
	channel::mpsc,
	future::AbortHandle,
	stream::{abortable, SelectAll},
	StreamExt,
};
use jam_interface::{
	BlockDesc, BlockInfo, BoxStream, CoreIndex, HeaderHash, JamChainSource, JamStateSource,
	JamWorkPackageSubmission, RecentBlocks, ServiceId, Slot as JamSlot, WorkPackage,
	WorkPackageHash, WorkPackageStatus,
};
use jam_types::{ExtrinsicSpec, MapLike, RefineContext, VecSet};
use parachain_service_core::authorizer::Authorizer;
use polkadot_primitives::Id as ParaId;
use sc_client_api::backend::AuxStore;
use sc_client_db::DbHash;
use sp_additional_data::AdditionalData;
use sp_runtime::traits::{Block as BlockT, HashingFor, Header as HeaderT, NumberFor};
use sp_trie::CompactProof;
use std::{
	collections::{HashMap, HashSet, VecDeque},
	sync::Arc,
	time::Instant,
};

pub(crate) struct CollationTaskParams<Block: NodeBlock, RuntimeApi, Jam> {
	pub para_client: Arc<ParachainClient<Block, RuntimeApi>>,
	pub para_backend: Arc<ParachainBackend<Block>>,
	pub jam: Arc<Jam>,
	pub para_id: ParaId,
	pub service_id: ServiceId,
	pub authorizer: Arc<AuraAuthorizer>,
	pub message_receiver: mpsc::Receiver<JamCollatorMessage<Block>>,
	pub announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	/// The service settings every package is built with.
	pub params: PackageParams,
	/// The PoV store every block this collator authors is recorded in.
	pub block_povs: BlockPovs<Block>,
	/// Shared with the builder: how many in-flight resubmission packages are pending.
	pub hold: AuthoringHold,
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
		para_backend,
		jam,
		para_id,
		service_id,
		authorizer,
		mut message_receiver,
		announce_block,
		params,
		block_povs,
		hold,
	} = params;

	let mut para_heads = match para_header_stream::<Block, _>(
		&*jam,
		Arc::clone(&para_client),
		service_id,
		para_id.into(),
		false,
	)
	.await
	{
		Ok(stream) => stream.boxed().fuse(),
		Err(error) => {
			tracing::error!(target: LOG_TARGET, ?error, "Unable to watch the para head.");
			return;
		},
	};

	let mut jam_blocks = match jam.best_block_stream().await {
		Ok(stream) => stream.fuse(),
		Err(error) => {
			tracing::error!(target: LOG_TARGET, ?error, "Unable to watch JAM best blocks.");
			return;
		},
	};

	tracing::info!(
		target: LOG_TARGET,
		?para_id,
		service_id,
		refine_gas_limit = params.refine_gas_limit,
		accumulate_gas_limit = params.accumulate_gas_limit,
		report_deadline_slots = REPORT_DEADLINE_SLOTS,
		authorizer_hash = ?authorizer.hash(),
		collator_set_size = authorizer.collator_set_size(),
		own_index = authorizer.own_index(),
		"JAM collation task started.",
	);

	// Nothing is tracked after a restart and nothing needs to be: the packages this task lost
	// track of are either already accumulated or lost, and the builder authors from the local
	// database and the accumulated head either way.
	let hash_ledger = WpHashLedger::new(Arc::clone(&para_client));
	let mut manager = Manager {
		para_client,
		para_backend,
		jam,
		authorizer,
		params,
		policy: ForgetWhenAnchorExpires,
		announce_block,
		hash_ledger,
		hold,
		packages: InFlightPackages::new(),
		block_povs,
		included_head: None,
		accumulated_head: None,
		head_changed_at: 0,
		last_round_at: None,
		tip_slot: 0,
		statuses: SelectAll::new(),
		subscriptions: StatusSubscriptions::new(),
	};

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
				let Some((head, slot)) = head else {
					tracing::error!(target: LOG_TARGET, "Para-head stream ended; stopping.");
					return;
				};
				manager.on_para_head(&head, slot);
			},
			tip = jam_blocks.next() => {
				let Some(tip) = tip else {
					tracing::error!(target: LOG_TARGET, "JAM best-block stream ended; stopping.");
					return;
				};
				manager.on_jam_block(tip).await;
			},
			update = manager.statuses.next() => {
				// `None` only means the last subscription ended while nothing is in flight.
				if let Some((wp_hash, status)) = update {
					manager.on_status(wp_hash, status).await;
				}
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
	/// The package exactly as submitted. Its prerequisite is what a forget cascades along.
	package: WorkPackage,
	anchored: Anchored,
	reported: bool,
	/// Whether this package was created by a resubmission round rather than by the builder.
	resubmission: bool,
	/// `submit()` calls that returned `true`: real hand-offs to a guarantor.
	sends: u32,
}

/// The work packages this collator has in flight, in the order they were submitted.
///
/// A plain list, not a chain: a package's prerequisite link lives in its own context, and
/// nothing here walks it.
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
		self.entries
			.remove(index)
			.expect("callers only ever pass an index they just found; qed")
	}

	/// Take out every package whose block is at or below `number`, newest first, and say for each
	/// whether it is the block that became the head.
	///
	/// The parachain service applies a head only if it chains onto the stored one and evicts
	/// everything at or below the stored head's height, so a package for a block at that height
	/// or lower has either just accumulated or lost its fork. Either way it is done.
	fn remove_up_to(&mut self, number: <Block::Header as HeaderT>::Number) -> Vec<InFlight<Block>> {
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
/// on its own. So every package this task stops tracking has to have its stream ended here. Without
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
	/// Following a package that is already followed closes the earlier subscription, so a package
	/// never holds two.
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

/// The JAM block in β whose guarantees name `wp_hash`, if any.
fn appeared_in(history: &RecentBlocks, wp_hash: WorkPackageHash) -> Option<&BlockInfo> {
	history
		.history
		.iter()
		.find(|block| MapLike::contains_key(&block.reported, &wp_hash))
}

/// What to do with each unreported package when a JAM block arrives: the pure heart of
/// [`Manager::on_jam_block`], so the sorting between reported, expired and waiting packages is
/// unit-testable without a chain.
fn plan_on_jam_block<Block: BlockT>(
	packages: &InFlightPackages<Block>,
	tip_slot: JamSlot,
	history: &RecentBlocks,
	policy: &ForgetWhenAnchorExpires,
) -> Vec<(WorkPackageHash, PolicyAction, Option<BlockDesc>)> {
	packages
		.entries
		.iter()
		.filter(|entry| !entry.reported)
		.map(|entry| {
			let appeared = appeared_in(history, entry.wp_hash);
			let action = policy.on_jam_block(Observed {
				tip_slot,
				anchor_slot: entry.anchored.anchor_slot,
				on_chain: appeared.is_some(),
			});
			let block = appeared.map(|info| BlockDesc { header_hash: info.hash, slot: info.slot });
			(entry.wp_hash, action, block)
		})
		.collect()
}

/// The prerequisite a package for a block must name: the work package this node submitted for
/// the block's parent, when the ledger still remembers it.
///
/// A miss starts a new chain, and so does a ledger read failure — a broken link only costs the
/// link, while dropping the block over it would lose the block for good. One prerequisite is all
/// a linear chain ever needs, well under the protocol's cap of eight.
fn prerequisites_for_parent<C: AuxStore>(
	ledger: &WpHashLedger<C>,
	parent_hash: [u8; 32],
) -> VecSet<WorkPackageHash> {
	match ledger.get(&parent_hash) {
		Ok(Some(parent_wp_hash)) => vec![parent_wp_hash].into(),
		Ok(None) => Default::default(),
		Err(error) => {
			tracing::warn!(
				target: LOG_TARGET,
				?error,
				?parent_hash,
				"Unable to read the parent's work-package hash; starting a new chain.",
			);
			Default::default()
		},
	}
}

/// The local best chain above `head`, oldest first: every block from the head's child up to
/// `best`. `None` when `head` is not an ancestor of `best` — nothing above the head is on the
/// best chain then, so a resubmission round has nothing to rebuild — or when `lookup` cannot
/// resolve a hash on the way.
fn chain_above<Header: HeaderT>(
	best: Header::Hash,
	(head_hash, head_number): (Header::Hash, Header::Number),
	lookup: impl Fn(Header::Hash) -> Option<Header>,
) -> Option<Vec<Header::Hash>> {
	let mut above = Vec::new();
	let mut current = best;
	loop {
		let header = lookup(current)?;
		if *header.number() <= head_number {
			break;
		}
		above.push(current);
		current = *header.parent_hash();
	}
	if current != head_hash {
		return None;
	}
	above.reverse();
	Some(above)
}

/// The blocks a round at `tip_slot` rebuilds, oldest first: the leading run of `above` — the
/// chain above the accumulated head — whose PoVs the store holds and whose original packages can
/// no longer be reported, at most `limit` long.
///
/// The run stops at the first block failing either test rather than skipping it: the packages
/// of a round chain by prerequisite, so a block after a gap would name a package for a block
/// the service cannot apply yet — and a block whose first package may still land must not be
/// rebuilt beside it.
fn rebuildable<Block: BlockT>(
	above: &[Block::Hash],
	tip_slot: JamSlot,
	pov_of: impl Fn(&Block::Hash) -> Option<BlockPov<Block>>,
	limit: usize,
) -> Vec<(Block::Hash, BlockPov<Block>)> {
	above
		.iter()
		.map_while(|hash| {
			let block_pov = pov_of(hash)?;
			original_package_expired(tip_slot, block_pov.jam_parent.anchor_slot)
				.then_some((*hash, block_pov))
		})
		.take(limit)
		.collect()
}

/// Assemble the package that rebuilds `block_pov` against `context`, unsigned, together with the
/// rebuilt PoV it is submitted with. The PoV carries the anchor chain `headers` the PVF verifies
/// the block's `JamParent` digest against.
///
/// `prerequisite` is the package the round submitted just before this one — package *k* > 0
/// names package *k* − 1 as its only prerequisite, so the guarantor processes the round in
/// order; package 0 names nothing. The token cannot be built here: it signs the finished
/// package, prerequisite included ([`AuraAuthorizer::authorize`] is the step after this one).
fn resubmission_package<Block: BlockT>(
	block_pov: &BlockPov<Block>,
	headers: &[Vec<u8>],
	prerequisite: Option<WorkPackageHash>,
	validation_code_hash: [u8; 32],
	params: &PackageParams,
	authorizer: Authorizer,
	context: &RefineContext,
) -> (WorkPackage, Vec<u8>) {
	let pov = build_pov(
		std::slice::from_ref(&block_pov.block),
		&block_pov.compact_proof,
		&block_pov.parent_header,
		&block_pov.additional_data,
		headers.to_vec(),
	);
	let context = RefineContext {
		prerequisites: prerequisite.into_iter().collect::<Vec<_>>().into(),
		..context.clone()
	};
	let package =
		work_package(vec![extrinsic_spec(&pov)], validation_code_hash, params, authorizer, context);
	(package, pov)
}

/// Record the hash of the package submitted for `block_hash` in the ledger and return it, so the
/// block's child can name it as its prerequisite.
///
/// A write failure must not drop the block: the only consequence is that the child starts a new
/// chain, so it is logged and swallowed.
fn record_submitted_hash<C: AuxStore>(
	ledger: &WpHashLedger<C>,
	block_hash: [u8; 32],
	package: &WorkPackage,
) -> WorkPackageHash {
	let wp_hash = work_package_hash(package);
	if let Err(error) = ledger.insert(&block_hash, wp_hash) {
		tracing::warn!(
			target: LOG_TARGET,
			?error,
			?block_hash,
			?wp_hash,
			"Unable to record the work-package hash; this block's child starts a new chain.",
		);
	}
	wp_hash
}

/// Forget the package at `index` and, with it, every package whose prerequisite chain leads back
/// to it, handing back every entry that was forgotten so the caller can close one subscription
/// per package and log it.
///
/// Forgetting a package has to take its ledger entry with it: that entry is what the block's
/// child names as its prerequisite, and a child chained to a package nothing will ever report
/// can never accumulate. The child itself has to go too, for the same reason — transitively,
/// because a grandchild names the child. `Block::Hash` is the node's `DbHash` because that is
/// what the ledger is keyed by.
fn forget_package<Block: BlockT<Hash = DbHash>, C: AuxStore>(
	packages: &mut InFlightPackages<Block>,
	ledger: &WpHashLedger<C>,
	index: usize,
) -> Vec<InFlight<Block>> {
	let primary = packages.remove(index);
	let mut dead = vec![primary.wp_hash];
	let mut forgotten = vec![primary];

	// An explicit worklist, not recursion: the chain is linear, so the depth is bounded by the
	// number of in-flight packages, but a malformed store could in principle describe a cycle
	// and this must not hang on one. Removing each entry as it is found is what terminates a
	// cycle: a package that is already gone cannot name a dead one again.
	while let Some(dead_hash) = dead.pop() {
		// Collect first, then remove from the highest index down: every removal shifts the
		// indices after it.
		let children: Vec<usize> = packages
			.entries
			.iter()
			.enumerate()
			.filter(|(_, entry)| entry.package.context.prerequisites.contains(&dead_hash))
			.map(|(index, _)| index)
			.collect();
		for index in children.into_iter().rev() {
			let entry = packages.remove(index);
			dead.push(entry.wp_hash);
			forgotten.push(entry);
		}
	}

	// Every forgotten package's ledger entry goes with it, cascaded ones included: the entry is
	// what a later child would name, and naming a forgotten package is exactly the dangling
	// prerequisite this cascade exists to prevent. A failed removal must not be fatal — the
	// package is already out of flight either way — so it is logged and swallowed, like the
	// write in `record_submitted_hash`.
	for entry in &forgotten {
		if let Err(error) = ledger.remove(&entry.block_hash.into()) {
			tracing::warn!(
				target: LOG_TARGET,
				block_hash = ?entry.block_hash,
				wp_hash = ?entry.wp_hash,
				?error,
				"Unable to remove the work-package hash ledger entry; a later child of this block \
				 may name a package nothing will report.",
			);
		}
	}

	forgotten
}

struct Manager<Block: NodeBlock, RuntimeApi, Jam> {
	para_client: Arc<ParachainClient<Block, RuntimeApi>>,
	para_backend: Arc<ParachainBackend<Block>>,
	jam: Arc<Jam>,
	/// The para's AURA authorizer: what every package here runs under, and what signs it.
	authorizer: Arc<AuraAuthorizer>,
	/// The service settings every package is assembled from.
	params: PackageParams,
	policy: ForgetWhenAnchorExpires,
	/// Shared with the builder; refreshed by [`Manager::log_state`] on every mutation.
	hold: AuthoringHold,
	announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	/// Which work package this node submitted for which block. Signing is non-deterministic, so
	/// a package's hash can only be remembered, never recomputed — not even by its author.
	hash_ledger: WpHashLedger<ParachainClient<Block, RuntimeApi>>,
	packages: InFlightPackages<Block>,
	/// Where every block this collator authored is recorded, so its PoV can be rebuilt later.
	block_povs: BlockPovs<Block>,
	/// The para head last seen in JAM state, for the log alone; `None` until the stream reports
	/// one. No decision reads it: it is a strictly later observation than the anchor a package
	/// carries, and letting it override the anchor is exactly the class of race phase 5 spent
	/// three fixes on.
	included_head: Option<Block::Hash>,
	/// The accumulated head's hash and number, updated by `on_para_head`.
	accumulated_head: Option<(Block::Hash, <Block::Header as HeaderT>::Number)>,
	/// The slot of the JAM block whose state last changed `accumulated_head`. The head stream
	/// carries it, because the best-block stream may deliver that block before or after the head
	/// it moved, and `tip_slot` at the time would be one slot off half the time.
	head_changed_at: JamSlot,
	/// The JAM slot at which the last resubmission round started, if any.
	last_round_at: Option<JamSlot>,
	/// The latest JAM block's slot, updated on every `on_jam_block`.
	tip_slot: JamSlot,
	statuses: SelectAll<BoxStream<'static, (WorkPackageHash, WorkPackageStatus)>>,
	/// One handle per live status subscription, fired when its package stops being tracked.
	subscriptions: StatusSubscriptions,
}

impl<Block, RuntimeApi, Jam> Manager<Block, RuntimeApi, Jam>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	Jam: JamChainSource + JamStateSource + JamWorkPackageSubmission + 'static,
{
	/// Resolve an offchain-code marker at `block`; see [`super::ensure_code_blob_at`].
	async fn ensure_code_blob_at(&mut self, block: Block::Hash, anchor: HeaderHash) {
		super::ensure_code_blob_at(
			&self.para_client,
			&self.para_backend,
			&*self.jam,
			self.params.service_id,
			block,
			anchor,
		)
		.await;
	}

	/// A block from the builder: assemble its package, submit it, track it.
	///
	/// The package names the work package this node submitted for the parent block as its one
	/// prerequisite, when the ledger still remembers it. The parent it declares also travels
	/// inside the PoV, and the parachain service decides at accumulate whether that parent is
	/// the stored head, a head still to come (buffered) or a fork loser (dropped).
	async fn on_new_block(&mut self, message: JamCollatorMessage<Block>) {
		let JamCollatorMessage {
			parent_header,
			block,
			proof,
			context,
			anchor_slot,
			submit_target,
			additional_data,
			triggered_by,
		} = message;
		let block_hash = block.hash();
		let block_number = *block.header().number();
		let parent_hash = parent_header.hash();
		let state_root = *parent_header.state_root();

		let compact_proof = match proof.into_compact_proof::<HashingFor<Block>>(state_root) {
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

		self.ensure_code_blob_at(parent_hash, context.anchor).await;

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

		// The parent's `:code`, not this block's own. A `:code` write in block N+1 only takes
		// effect for validation at N+2, so N+1 is still validated with the old code; the
		// service's dual-code window accepts both while the upgrade is in flight (spec §5.2
		// phase 4/5). Hashing the parent's code is therefore the code this package must commit
		// to. Caveat: `system_version: 3` runtimes defer the swap by two blocks (N+2 still runs
		// the old code, N+3 the new one), which the same parent-code hash covers.
		let validation_code_hash = sp_crypto_hashing::blake2_256(&validation_code);
		tracing::debug!(
			target: LOG_TARGET,
			?block_hash,
			?parent_hash,
			?validation_code_hash,
			"Hashed the parent's validation code for the work package.",
		);
		let source = PackageSource {
			blocks: vec![block],
			proof: compact_proof,
			// The additional-data map assembled at build time — the `JAM_PROOF_KEY` entry — is
			// part of what makes this block's package what it is, like the block it proves.
			additional_data,
			validation_code_hash,
			params: self.params.clone(),
			authorizer: self.authorizer.authorizer(),
			parent_header,
		};
		// The prerequisite goes in before the package is signed: the token covers the context,
		// so a prerequisite set afterwards would leave the signature over the wrong bytes and
		// every guarantor would reject the package.
		let anchored = Anchored {
			context: RefineContext {
				prerequisites: prerequisites_for_parent(&self.hash_ledger, parent_hash.into()),
				..context
			},
			anchor_slot,
			submit_target,
		};
		let (package, pov) = match self.authorized_package(&source, &anchored) {
			Ok(package) => package,
			Err(error) => {
				tracing::error!(
					target: LOG_TARGET,
					?block_hash,
					%block_number,
					lookup_anchor_slot = anchored.context.lookup_anchor_slot,
					error,
					"Failed to authorize the work package; dropping the block. The block stays in \
					 the local database, but nothing will make it accumulate.",
				);
				return;
			},
		};
		// Kept the way the import path keeps a block it re-executes, so a later resubmission can
		// rebuild this block's PoV too.
		match source.into_block_pov(package.items[0].extrinsics[0].clone()) {
			Some(block_pov) => self.block_povs.insert(block_hash, block_pov),
			None => tracing::debug!(
				target: LOG_TARGET,
				?block_hash,
				"The block carries no `JamParent` digest; not keeping its PoV for a rebuild.",
			),
		}
		let wp_hash = record_submitted_hash(&self.hash_ledger, block_hash.into(), &package);

		tracing::info!(
			target: LOG_TARGET,
			?block_hash,
			%block_number,
			?parent_hash,
			?wp_hash,
			prerequisite = ?anchored.context.prerequisites.as_ref().first(),
			core = ?anchored.submit_target,
			anchor = ?anchored.context.anchor,
			anchor_slot,
			lookup_anchor = ?anchored.context.lookup_anchor,
			lookup_anchor_slot = anchored.context.lookup_anchor_slot,
			expected_collator = self.authorizer.collator_for(anchored.context.lookup_anchor_slot),
			own_index = self.authorizer.own_index(),
			authorizer_hash = ?self.authorizer.hash(),
			token_len = package.authorization.len(),
			pov_len = package.items[0].extrinsics[0].len,
			in_flight = self.packages.len(),
			?triggered_by,
			"Assembled and signed the work package for the block.",
		);

		(self.announce_block)(block_hash, None);

		// The entry is recorded even if the submission itself failed: the package may still have
		// reached a guarantor, and the next JAM block re-decides it either way.
		let sent = self.submit(wp_hash, &package, pov, &anchored, block_hash).await;
		self.packages.entries.push_back(InFlight {
			block_hash,
			block_number,
			parent_hash,
			wp_hash,
			package,
			anchored,
			reported: false,
			resubmission: false,
			sends: u32::from(sent),
		});
		self.log_state("a package was submitted");
	}

	/// Assemble the package for `anchored` and sign it as this collator.
	///
	/// One step, always taken together: an unsigned package is one no guarantor will look at, and
	/// the signature is over the finished package, so the two cannot be reordered.
	fn authorized_package(
		&self,
		source: &PackageSource<Block>,
		anchored: &Anchored,
	) -> Result<(WorkPackage, Vec<u8>), String> {
		let (mut package, pov) = source.package(anchored);
		self.authorizer.authorize(&mut package)?;
		Ok((package, pov))
	}

	/// Submit a package to the core its anchor's pool scan named, and subscribe to its status;
	/// `false` means it was not submitted.
	///
	/// `pov` is the work item's only extrinsic and travels beside the package, not in its
	/// payload: CE 133 caps the first message (core index plus package, payloads included) at
	/// 200 KiB, while extrinsics ride the bulk channel, bounded only by `max_input`. The node
	/// rejects a submission whose extrinsic count differs from the spec's. With no core holding
	/// the para's authorizer there is nowhere to send it — a guarantor on any other core would
	/// refuse it — so it is kept in flight until its anchor expires, but never sent.
	async fn submit(
		&mut self,
		wp_hash: WorkPackageHash,
		package: &WorkPackage,
		pov: Vec<u8>,
		anchored: &Anchored,
		block_hash: Block::Hash,
	) -> bool {
		let anchor = anchored.context.anchor;
		let Some(core) = anchored.submit_target else {
			tracing::warn!(
				target: LOG_TARGET,
				?block_hash,
				?wp_hash,
				?anchor,
				anchor_slot = anchored.anchor_slot,
				authorizer_hash = ?self.authorizer.hash(),
				in_flight = self.packages.len(),
				"No core held this para's authorizer at the package's anchor, so it was not \
				 submitted. It stays in flight until its anchor expires.",
			);
			return false;
		};

		let started = Instant::now();
		let result = self.jam.submit_work_package(core, package, vec![pov]).await;
		let elapsed_ms = started.elapsed().as_millis();
		if let Err(error) = result {
			tracing::warn!(
				target: LOG_TARGET,
				?block_hash,
				?wp_hash,
				core,
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
			core,
			?anchor,
			anchor_slot = anchored.anchor_slot,
			lookup_anchor_slot = anchored.context.lookup_anchor_slot,
			package_len = jam_codec::Encode::encode(package).len(),
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
				"Unable to follow the work-package status; the recent-blocks history is the only \
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
			PolicyAction::Reported => {
				self.packages.entries[index].reported = true;
				let reported_in = match &status {
					WorkPackageStatus::Reported { reported_in, .. } => Some(*reported_in),
					_ => None,
				};
				let entry = &self.packages.entries[index];
				tracing::info!(
					target: LOG_TARGET,
					wp_hash = ?entry.wp_hash,
					block_hash = ?entry.block_hash,
					jam_block = ?reported_in.map(|block| block.header_hash),
					jam_slot = reported_in.map(|block| block.slot),
					via = "status",
					"The work package appeared on chain in a JAM block.",
				);
			},
			PolicyAction::Forget => self.forget(index, &format!("{status:?}")),
		}
		self.log_state("a work-package status update arrived");
	}

	/// A new JAM block arrived: read β and re-decide every package that has not been reported.
	async fn on_jam_block(&mut self, tip: BlockDesc) {
		self.tip_slot = tip.slot;
		if self.packages.entries.is_empty() {
			self.resubmit_if_due(tip).await;
			return;
		}
		let history = match self.jam.recent_blocks(tip.header_hash).await {
			Ok(history) => history,
			Err(error) => {
				tracing::warn!(
					target: LOG_TARGET,
					?tip,
					?error,
					"Unable to read the recent-blocks history; leaving the packages for the next \
					 JAM block.",
				);
				return;
			},
		};
		let plan = plan_on_jam_block(&self.packages, tip.slot, &history, &self.policy);
		// Keyed by package hash rather than by index: forgetting one shifts every index after it,
		// and packages are independent, so several may come due with the same block.
		for (wp_hash, action, appeared) in plan {
			let Some(index) = self.packages.position_of_package(wp_hash) else { continue };
			match action {
				PolicyAction::Reported => {
					self.packages.entries[index].reported = true;
					let entry = &self.packages.entries[index];
					tracing::info!(
						target: LOG_TARGET,
						wp_hash = ?entry.wp_hash,
						block_hash = ?entry.block_hash,
						jam_block = ?appeared.map(|block| block.header_hash),
						jam_slot = appeared.map(|block| block.slot),
						via = "recent_history",
						"The work package appeared on chain in a JAM block.",
					);
				},
				PolicyAction::Forget => {
					self.forget(index, "anchor expired before the package appeared on chain")
				},
				PolicyAction::Wait => {},
			}
		}
		self.log_state("a JAM block arrived");
		self.resubmit_if_due(tip).await;
	}

	/// Give up on a package and, transitively, on every package that named it.
	///
	/// The block itself stays in the local database and the PoV store, so the next parachain slot
	/// simply authors on whatever is deepest there, and a resubmission round — this collator's or
	/// another's, whichever owns the turn once the head has stalled long enough — rebuilds the
	/// block into a new package. Until then the parachain's progress is what this costs:
	/// descendants of the lost block sit in the service's reorder buffer. This logs loudly enough
	/// to be the thing a stalled parachain is diagnosed from.
	fn forget(&mut self, index: usize, reason: &str) {
		let forgotten = forget_package(&mut self.packages, &self.hash_ledger, index);
		let cascade_len = forgotten.len();
		for entry in &forgotten {
			self.stop_following(entry.wp_hash, "forgotten");
			tracing::warn!(
				target: LOG_TARGET,
				block_hash = ?entry.block_hash,
				block_number = %entry.block_number,
				parent_hash = ?entry.parent_hash,
				wp_hash = ?entry.wp_hash,
				reason,
				sends = entry.sends,
				anchor_slot = entry.anchored.anchor_slot,
				cascade_len,
				in_flight = self.packages.len(),
				"Giving up on a work package. Its block stays in the local database, so authoring \
				 continues, but nothing this collator does will make that block accumulate.",
			);
		}
		self.log_state("a package was forgotten");
	}

	/// The para head advanced in JAM state.
	///
	/// Everything at or below the new head's height is settled: the service applies a head only
	/// if it chains onto the stored one, and sweeps the rest of that height out of its reorder
	/// buffer. So those packages either just accumulated or lost their fork, and either way this
	/// task is done with them.
	fn on_para_head(&mut self, header: &Block::Header, changed_at: JamSlot) {
		let hash = header.hash();
		let number = *header.number();
		let head_changed = self.accumulated_head.map_or(true, |(h, _)| h != hash);
		self.included_head = Some(hash);
		self.accumulated_head = Some((hash, number));
		if head_changed {
			self.head_changed_at = changed_at;
		}

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
		let pending = self.resubmissions_pending();
		self.hold.set_pending(pending);
		tracing::debug!(
			target: LOG_TARGET,
			after,
			in_flight = self.packages.len(),
			blocks = ?self.packages.block_hashes(),
			included_head = ?self.included_head,
			accumulated_head = ?self.accumulated_head,
			head_changed_at = self.head_changed_at,
			last_round_at = ?self.last_round_at,
			tip_slot = self.tip_slot,
			pending,
			live_subscriptions = self.subscriptions.len(),
			"In-flight work packages.",
		);
	}

	/// Count in-flight resubmission packages that have not been reported yet.
	fn resubmissions_pending(&self) -> usize {
		self.packages.entries.iter().filter(|e| e.resubmission && !e.reported).count()
	}

	/// Check the resubmission predicate and, when it fires, run a resubmission round.
	///
	/// `last_round_at` moves only when a round actually starts — when the chain above the head
	/// begins with a block that can be rebuilt. While it does not (the first block's original
	/// package may still be reported, or the store has no PoV for it), the check is cheap enough
	/// to re-ask on every JAM block, and a block must not wait out a second stall window after
	/// the slot it becomes rebuildable in.
	async fn resubmit_if_due(&mut self, tip: BlockDesc) {
		let Some(head) = self.accumulated_head else { return };
		let has_unaccumulated_child = {
			use sp_blockchain::HeaderBackend;
			self.para_client.info().best_number > head.1
		};
		let our_turn = self.authorizer.names_us(tip.slot);
		let round_in_flight = self.resubmissions_pending() > 0;

		if !resubmission_due(
			tip.slot,
			self.head_changed_at,
			self.last_round_at,
			has_unaccumulated_child,
			our_turn,
			round_in_flight,
		) {
			return;
		}

		let above = self.chain_above_head(head);
		let blocks =
			rebuildable(&above, tip.slot, |hash| self.block_povs.get(hash), MAX_RESUBMIT_BLOCKS);
		if blocks.is_empty() {
			tracing::debug!(
				target: LOG_TARGET,
				?tip,
				head = ?head.0,
				head_number = %head.1,
				above_head = above.len(),
				"Resubmission is due, but nothing above the accumulated head is rebuildable yet: \
				 the first block there has no stored PoV, or its original package can still be \
				 reported.",
			);
			return;
		}
		self.last_round_at = Some(tip.slot);
		self.resubmit_round(tip, blocks).await;
	}

	/// One resubmission round: rebuild `blocks` — oldest first, each with a stored PoV and an
	/// expired original package — into new work packages anchored at `tip`, chained by
	/// prerequisites in that order.
	///
	/// Any failure ends the round instead of skipping the block it hit: the packages chain, so
	/// a block skipped in the middle would leave every later one naming a prerequisite for a
	/// block the service cannot apply yet. What was submitted before the failure stands, and the
	/// next round picks up where this one stopped.
	async fn resubmit_round(
		&mut self,
		tip: BlockDesc,
		blocks: Vec<(Block::Hash, BlockPov<Block>)>,
	) {
		// Build a fresh anchor context from the current tip.
		let (anchor_context, anchor_slot, pool_scan) = match self.read_resubmit_anchor(tip).await {
			Ok(Some(reads)) => reads,
			Ok(None) => {
				tracing::warn!(
					target: LOG_TARGET,
					?tip,
					"Resubmission round skipped: no suitable lookup anchor.",
				);
				return;
			},
			Err(error) => {
				tracing::warn!(
					target: LOG_TARGET,
					?tip,
					error,
					"Resubmission round skipped: anchor reads failed.",
				);
				return;
			},
		};
		// A package nothing can be sent to would only hold the builder: it would count as a round
		// in flight until its anchor expired.
		let Some(core) = pool_scan.target else {
			tracing::warn!(
				target: LOG_TARGET,
				?tip,
				authorizer_hash = ?self.authorizer.hash(),
				blocks = blocks.len(),
				"Resubmission round skipped: no core holds this para's authorizer at the tip, so \
				 nothing could be submitted.",
			);
			return;
		};

		// One header walk for the whole round: the chain has to reach every block's anchor and
		// lookup anchor, and each package gets the prefix down to the oldest hash its block needs.
		let anchors_of = |block_pov: &BlockPov<Block>| {
			[
				HeaderHash(block_pov.jam_parent.anchor.0),
				HeaderHash(block_pov.jam_parent.lookup_anchor.0),
			]
		};
		let needed: HashSet<HeaderHash> =
			blocks.iter().flat_map(|(_, block_pov)| anchors_of(block_pov)).collect();
		let jam = Arc::clone(&self.jam);
		let walk = header_chain_until(tip.header_hash, needed, |hash| {
			let jam = Arc::clone(&jam);
			async move { jam.header(hash).await.map_err(|error| format!("header({hash:?}): {error}")) }
		})
		.await;
		let chain = match walk {
			Ok(chain) if chain.missing.is_empty() => chain,
			Ok(chain) => {
				tracing::warn!(
					target: LOG_TARGET,
					?tip,
					missing = ?chain.missing,
					headers_walked = chain.headers.len(),
					max_headers = MAX_ANCHOR_CHAIN_LEN,
					"Resubmission round skipped: the header chain from the tip did not reach every \
					 anchor the blocks were built against.",
				);
				return;
			},
			Err(error) => {
				tracing::warn!(
					target: LOG_TARGET,
					?tip,
					error,
					"Resubmission round skipped: the header chain walk failed.",
				);
				return;
			},
		};

		let mut prerequisite: Option<WorkPackageHash> = None;
		let mut submitted = 0usize;

		for (block_hash, block_pov) in &blocks {
			let Some(headers) = chain.prefix_reaching(&anchors_of(block_pov)) else {
				// The walk above saw every hash in `needed`, so this cannot happen.
				tracing::warn!(
					target: LOG_TARGET,
					?block_hash,
					"Resubmission round stopped: the header chain does not reach the block's \
					 anchors.",
				);
				break;
			};

			// The parent's `:code`, hashed exactly as `on_new_block` hashes it for a fresh block,
			// a marker there resolved first.
			self.ensure_code_blob_at(block_pov.parent_hash, tip.header_hash).await;
			let validation_code_hash = match self.para_client.code_at(block_pov.parent_hash) {
				Ok(code) => sp_crypto_hashing::blake2_256(&code),
				Err(error) => {
					tracing::warn!(
						target: LOG_TARGET,
						?block_hash,
						parent_hash = ?block_pov.parent_hash,
						?error,
						"Resubmission round stopped: failed to read the validation code.",
					);
					break;
				},
			};

			let (mut package, pov) = resubmission_package(
				block_pov,
				headers,
				prerequisite,
				validation_code_hash,
				&self.params,
				self.authorizer.authorizer(),
				&anchor_context,
			);
			if let Err(error) = self.authorizer.authorize(&mut package) {
				tracing::warn!(
					target: LOG_TARGET,
					?block_hash,
					error,
					"Resubmission round stopped: failed to authorize the package.",
				);
				break;
			}

			let wp_hash = record_submitted_hash(&self.hash_ledger, (*block_hash).into(), &package);
			let anchored = Anchored {
				context: package.context.clone(),
				anchor_slot,
				submit_target: Some(core),
			};

			tracing::info!(
				target: LOG_TARGET,
				?block_hash,
				block_number = %block_pov.number,
				?wp_hash,
				?prerequisite,
				core,
				anchor = ?anchored.context.anchor,
				anchor_slot,
				lookup_anchor = ?anchored.context.lookup_anchor,
				lookup_anchor_slot = anchored.context.lookup_anchor_slot,
				original_anchor_slot = block_pov.jam_parent.anchor_slot,
				headers = headers.len(),
				"Resubmitting a parachain block in a new work package.",
			);

			let sent = self.submit(wp_hash, &package, pov, &anchored, *block_hash).await;
			self.packages.entries.push_back(InFlight {
				block_hash: *block_hash,
				block_number: block_pov.number,
				parent_hash: block_pov.parent_hash,
				wp_hash,
				package,
				anchored,
				reported: false,
				resubmission: true,
				sends: u32::from(sent),
			});
			prerequisite = Some(wp_hash);
			submitted += 1;
		}

		tracing::info!(
			target: LOG_TARGET,
			submitted,
			total_blocks = blocks.len(),
			"Resubmission round complete.",
		);
		if submitted > 0 {
			self.log_state("a resubmission round completed");
		}
	}

	/// Read the anchor context for a resubmission round, mirroring the builder's `read_anchor`
	/// but without the parachain-state read.
	async fn read_resubmit_anchor(
		&mut self,
		tip: BlockDesc,
	) -> Result<Option<(RefineContext, JamSlot, super::PoolScan)>, String> {
		let anchor = tip;
		let state_root =
			jam_read("stateRoot", anchor.header_hash, self.jam.state_root(anchor.header_hash))
				.await?;
		let beefy_root =
			jam_read("beefyRoot", anchor.header_hash, self.jam.beefy_root(anchor.header_hash))
				.await?;
		let finalized =
			jam_read("finalizedBlock", anchor.header_hash, self.jam.finalized_block()).await?;
		let newest_lookup_anchor =
			jam_read("parent", finalized.header_hash, self.jam.parent(finalized.header_hash))
				.await?;
		let Some(lookup_anchor) =
			choose_lookup_anchor(&*self.jam, &anchor, newest_lookup_anchor, &self.authorizer).await
		else {
			return Ok(None);
		};
		let lookup_anchor_state_root = jam_read(
			"stateRoot",
			lookup_anchor.header_hash,
			self.jam.state_root(lookup_anchor.header_hash),
		)
		.await?;
		let pool_scan =
			super::scan_pools_at(&*self.jam, anchor.header_hash, &self.authorizer).await?;

		Ok(Some((
			RefineContext {
				anchor: anchor.header_hash,
				anchor_slot: anchor.slot,
				state_root,
				beefy_root,
				lookup_anchor: lookup_anchor.header_hash,
				lookup_anchor_slot: lookup_anchor.slot,
				lookup_anchor_state_root,
				prerequisites: Default::default(),
			},
			anchor.slot,
			pool_scan,
		)))
	}

	/// The local best chain above the accumulated head, oldest first; see [`chain_above`].
	fn chain_above_head(&self, head: (Block::Hash, NumberFor<Block>)) -> Vec<Block::Hash> {
		use sp_blockchain::HeaderBackend;
		let best = self.para_client.info().best_hash;
		chain_above(best, head, |hash| match self.para_client.header(hash) {
			Ok(header) => header,
			Err(error) => {
				tracing::warn!(
					target: LOG_TARGET,
					?hash,
					?error,
					"Unable to read a header while walking the chain above the accumulated head.",
				);
				None
			},
		})
		.unwrap_or_default()
	}
}

/// The parts a package is assembled from: the built block(s), the parachain storage proof
/// witnessing them, the parent header (travels in the V4 PoV), and the work-item settings.
struct PackageSource<Block: BlockT> {
	blocks: Vec<Block>,
	proof: CompactProof,
	/// SCALE-encoded header of the block `blocks[0]` extends. Travels untrusted in the V4 PoV;
	/// the parachain service's accumulate verifies it against its stored head.
	parent_header: Block::Header,
	/// The additional-data map assembled at build time, carried in the V4 PoV's
	/// `additional_data` slot for `blocks[0]`.
	additional_data: AdditionalData,
	validation_code_hash: [u8; 32],
	/// The service settings the work item is assembled from.
	params: PackageParams,
	/// The para's AURA authorizer. Every core this para runs on holds
	/// `blake2b(code_hash ‖ config)` of exactly this in its pool, so it is as much a part of the
	/// package's identity as the block inside it.
	authorizer: Authorizer,
}

/// A refine context plus the anchor-derived submission target.
#[derive(Clone)]
struct Anchored {
	context: RefineContext,
	/// The anchor's timeslot — the start of the window the package has to be reported in.
	anchor_slot: JamSlot,
	/// The core the pool scan at this anchor named, if any. Anchor-derived like everything else
	/// here, and fixed for the package's life.
	submit_target: Option<CoreIndex>,
}

impl<Block: BlockT> PackageSource<Block> {
	/// Assemble the work package for `anchored`, still unauthorized, and hand back the PoV it
	/// carries.
	///
	/// The PoV travels as work-item extrinsic 0, not in the payload: CE 133 caps the first message
	/// (core index plus package, payloads included) at 200 KiB, while extrinsics ride the bulk
	/// channel, bounded only by `max_input`. The payload's `ParachainCandidate` keeps its
	/// `validation_code` — the parachain service still reads that — and carries no PoV.
	///
	/// The token cannot be built here: it signs a hash of the finished package, so authorizing is
	/// the step after this one ([`AuraAuthorizer::authorize`]).
	fn package(&self, anchored: &Anchored) -> (WorkPackage, Vec<u8>) {
		let pov = build_pov(
			&self.blocks,
			&self.proof,
			&self.parent_header,
			&self.additional_data,
			Vec::new(),
		);
		let spec = extrinsic_spec(&pov);
		let package = work_package(
			vec![spec],
			self.validation_code_hash,
			&self.params,
			self.authorizer.clone(),
			anchored.context.clone(),
		);
		(package, pov)
	}

	/// What the PoV store keeps for the block once its package carries `spec` as the PoV: the
	/// entry the import path records for a block it re-executes. `None` for a block without a
	/// `JamParent` digest, which the import path does not record either.
	fn into_block_pov(self, spec: ExtrinsicSpec) -> Option<BlockPov<Block>> {
		let block = self.blocks.into_iter().next()?;
		let header = block.header();
		let parent_hash = *header.parent_hash();
		let number = *header.number();
		let jam_parent = CumulusDigestItem::find_jam_parent_info(header.digest())?;
		Some(BlockPov {
			block,
			parent_header: self.parent_header,
			parent_hash,
			number,
			spec,
			jam_parent,
			compact_proof: self.proof,
			additional_data: self.additional_data,
		})
	}
}

#[cfg(test)]
mod tests {
	use super::{
		super::{authorizer::tests::authorizer_of, hash_ledger::test_support::ledger},
		*,
	};
	use codec::{Decode, DecodeAll, Encode};
	use cumulus_jam_state_reader::JAM_PROOF_KEY;
	use cumulus_primitives_core::{JamParent, ParachainBlockData};
	use cumulus_test_runtime::{Block as TestBlock, Header as TestHeader};
	use jam_std_common::{build_encoded_bundle, BlockInfo, Mmr, RecentBlocks};
	use jam_types::{BoundedVec, CodeHash, RecentBlockCount, SegmentTreeRoot, VecMap};
	use parachain_authorizer::aura::{signable_work_package_hash, AuthToken as GuestToken};
	use parachain_authorizer_sr25519::Sr25519;
	use parachain_service_core::{candidate::ParachainCandidate, StateProof};
	use sp_core::H256;

	/// The one-collator set this node is in, so every package a test builds can be signed.
	fn aura() -> AuraAuthorizer {
		authorizer_of("alice", "Alice", 1)
	}

	/// A package as the manager builds one: assembled, then signed.
	fn signed(source: &PackageSource<TestBlock>, anchored: &Anchored) -> WorkPackage {
		signed_with_pov(source, anchored).0
	}

	/// A signed package together with the PoV it sends as work-item extrinsic 0.
	fn signed_with_pov(
		source: &PackageSource<TestBlock>,
		anchored: &Anchored,
	) -> (WorkPackage, Vec<u8>) {
		let (mut package, pov) = source.package(anchored);
		aura()
			.authorize(&mut package)
			.expect("the keystore holds Alice's aura key; qed");
		(package, pov)
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
				anchor_slot,
				state_root: [4u8; 32].into(),
				beefy_root: [5u8; 32].into(),
				lookup_anchor: HeaderHash::from([6u8; 32]),
				lookup_anchor_slot: anchor_slot,
				lookup_anchor_state_root: Default::default(),
				prerequisites: Default::default(),
			},
			anchor_slot,
			submit_target: Some(0),
		}
	}

	/// `anchored(11)` carrying `prerequisites`, the way `on_new_block` hands them to the
	/// assembler.
	fn anchored_naming(prerequisites: VecSet<WorkPackageHash>) -> Anchored {
		Anchored {
			context: RefineContext { prerequisites, ..anchored(11).context },
			..anchored(11)
		}
	}

	fn package_source() -> PackageSource<TestBlock> {
		PackageSource {
			blocks: vec![TestBlock::new(header(1, H256::repeat_byte(7)), vec![])],
			proof: CompactProof { encoded_nodes: vec![vec![1u8, 2, 3]] },
			parent_header: header(0, H256::repeat_byte(6)),
			additional_data: [(JAM_PROOF_KEY.to_string(), vec![1u8, 2, 3])].into(),
			validation_code_hash: [8u8; 32],
			params: PackageParams {
				service_id: 42,
				service_code_hash: CodeHash::from([9u8; 32]),
				refine_gas_limit: 1_000,
				accumulate_gas_limit: 1_000,
			},
			authorizer: aura().authorizer(),
		}
	}

	/// `count` packages for a parent/child line of blocks, package `k` hashed as `[k; 32]`, all
	/// anchored at slot `k`. The blocks form a chain because that is what a collator authors;
	/// the packages themselves are independent, which is the point.
	fn in_flight(count: u8) -> InFlightPackages<TestBlock> {
		let mut packages = InFlightPackages::new();
		let mut parent_hash = H256::repeat_byte(200);
		for index in 0..count {
			let block_header = header(u32::from(index) + 1, parent_hash);
			let block_hash = block_header.hash();
			let package = signed(&package_source(), &anchored(JamSlot::from(index)));
			packages.entries.push_back(InFlight {
				block_hash,
				block_number: *block_header.number(),
				parent_hash,
				wp_hash: wp_hash(index),
				package,
				anchored: anchored(JamSlot::from(index)),
				reported: false,
				resubmission: false,
				sends: 1,
			});
			parent_hash = block_hash;
		}
		packages
	}

	/// β naming each hash in `reported`, one block per hash, newest last.
	fn recent_blocks(reported: &[WorkPackageHash]) -> RecentBlocks {
		let history: BoundedVec<BlockInfo, RecentBlockCount> = reported
			.iter()
			.enumerate()
			.map(|(index, wp_hash)| {
				let mut named = VecMap::new();
				named.insert(*wp_hash, SegmentTreeRoot([0u8; 32]));
				BlockInfo {
					hash: HeaderHash::from([index as u8; 32]),
					beefy_root: [0u8; 32].into(),
					state_root: [0u8; 32].into(),
					slot: index as JamSlot,
					reported: named,
				}
			})
			.collect::<Vec<_>>()
			.try_into()
			.expect("within recent block count");
		RecentBlocks { history, mmr: Mmr::default() }
	}

	/// The β read is what tells "reported" apart from "still missing": a hash named by some block
	/// in the history comes back with that block, and an unknown hash comes back empty.
	#[test]
	fn appeared_in_finds_the_block_that_named_the_package() {
		let named = wp_hash(0x11);
		let history = recent_blocks(&[wp_hash(0x22), named]);

		let found = appeared_in(&history, named).expect("the second block named it");
		assert_eq!(found.slot, 1);
		assert!(appeared_in(&history, wp_hash(0x33)).is_none());
	}

	/// One JAM block sorts every unreported package: a package in β is reported, one whose anchor
	/// is past its deadline is forgotten, and the rest wait — however long they have been missing,
	/// since nothing is ever sent twice.
	#[test]
	fn a_jam_block_sorts_reported_expired_and_waiting() {
		let tip = 100;
		let mut packages = in_flight(4);
		// 0: named in β.
		packages.entries[0].anchored.anchor_slot = tip;
		// 1: missing for most of its window, but its anchor still lives.
		packages.entries[1].anchored.anchor_slot = tip - (REPORT_DEADLINE_SLOTS - 1);
		// 2: missing and its anchor has expired.
		packages.entries[2].anchored.anchor_slot = tip - REPORT_DEADLINE_SLOTS;
		// 3: missing, but only just anchored.
		packages.entries[3].anchored.anchor_slot = tip;

		let history = recent_blocks(&[wp_hash(0)]);
		let plan = plan_on_jam_block(&packages, tip, &history, &ForgetWhenAnchorExpires);
		let actions: Vec<_> = plan.iter().map(|(hash, action, _)| (*hash, *action)).collect();

		assert_eq!(
			actions,
			vec![
				(wp_hash(0), PolicyAction::Reported),
				(wp_hash(1), PolicyAction::Wait),
				(wp_hash(2), PolicyAction::Forget),
				(wp_hash(3), PolicyAction::Wait),
			],
		);
		assert_eq!(plan[0].2.map(|block| block.slot), Some(0), "the β block travels with it");
	}

	/// A package already known to be reported is filtered out before the clock is even
	/// consulted: nothing forgets a package JAM has already accepted.
	#[test]
	fn a_reported_package_is_left_alone_by_the_clock() {
		let tip = 100;
		let mut packages = in_flight(2);
		packages.entries[0].reported = true;
		packages.entries[0].anchored.anchor_slot = tip - REPORT_DEADLINE_SLOTS;
		packages.entries[1].anchored.anchor_slot = tip;

		let plan = plan_on_jam_block(&packages, tip, &recent_blocks(&[]), &ForgetWhenAnchorExpires);

		assert_eq!(plan.len(), 1, "the reported package is not re-decided");
		assert_eq!(plan[0].0, wp_hash(1));
		assert_eq!(plan[0].1, PolicyAction::Wait);
	}

	/// With no entry for the parent block in the ledger the chain restarts here: the assembled
	/// package names nothing. Nothing is imported into it and nothing is exported out of it for
	/// a child to import — the whole of what phase 5a changed on the wire.
	#[test]
	fn a_package_with_no_known_parent_has_empty_prerequisites() {
		let ledger = ledger();
		let prerequisites = prerequisites_for_parent(&ledger, H256::repeat_byte(0x22).into());
		let (package, _pov) = package_source().package(&anchored_naming(prerequisites));

		assert!(package.context.prerequisites.as_ref().is_empty());
		assert!(package.items[0].import_segments.is_empty());
		assert_eq!(package.items[0].export_count, 0);
		assert_eq!(package.items[0].extrinsics.len(), 1, "the PoV, and only the PoV, travels");
	}

	/// When the ledger remembers the parent block's package, the assembled package names exactly
	/// that one package as its prerequisite — the linear chain.
	#[test]
	fn a_package_with_known_parent_names_it_as_prerequisite() {
		let parent_hash = H256::repeat_byte(0x11);
		let parent_wp_hash = wp_hash(0xAB);
		let ledger = ledger();
		ledger.insert(&parent_hash.into(), parent_wp_hash).expect("insert ok; qed");

		let prerequisites = prerequisites_for_parent(&ledger, parent_hash.into());
		let (package, _pov) = package_source().package(&anchored_naming(prerequisites));

		assert_eq!(
			package.context.prerequisites.as_ref(),
			&[parent_wp_hash],
			"exactly the parent's package",
		);
	}

	/// A package chains behind its parent's package when the ledger remembers that package's
	/// hash: exactly one prerequisite, the parent — the linear chain.
	#[test]
	fn package_names_parent_as_prerequisite() {
		let parent_hash = H256::repeat_byte(0x11);
		let parent_wp_hash = wp_hash(0xAB);
		let ledger = ledger();
		ledger.insert(&parent_hash.into(), parent_wp_hash).expect("insert ok; qed");

		let prerequisites = prerequisites_for_parent(&ledger, parent_hash.into());
		assert_eq!(prerequisites.as_ref(), &[parent_wp_hash], "exactly the parent's package");

		let package = signed(&package_source(), &anchored_naming(prerequisites));
		assert_eq!(
			package.context.prerequisites.as_ref(),
			&[parent_wp_hash],
			"the assembled package carries the parent's hash",
		);
	}

	/// With no entry for the parent block the chain restarts here: the package names nothing.
	#[test]
	fn package_without_known_parent_has_no_prerequisite() {
		let ledger = ledger();

		let prerequisites = prerequisites_for_parent(&ledger, H256::repeat_byte(0x22).into());
		assert!(prerequisites.is_empty(), "a parent hash the ledger does not hold is no link");

		let package = signed(&package_source(), &anchored_naming(prerequisites));
		assert!(package.context.prerequisites.as_ref().is_empty());
	}

	/// The recording step of `on_new_block`: the hash JAM keys the submitted package by is the
	/// hash the ledger remembers for its block, ready for the block's child to name.
	#[test]
	fn submitted_hash_is_recorded_in_the_ledger() {
		let ledger = ledger();
		let block_hash = H256::repeat_byte(0x33);
		let package = signed(&package_source(), &anchored(11));

		let recorded = record_submitted_hash(&ledger, block_hash.into(), &package);

		assert_eq!(recorded, work_package_hash(&package));
		assert_eq!(
			ledger.get(&block_hash.into()).expect("read ok; qed"),
			Some(work_package_hash(&package)),
		);
	}

	/// The prerequisite is part of the bytes the token signs: the token verifies against the
	/// package that names the parent and fails against the same package without the link. A
	/// prerequisite set after signing would invert both.
	#[test]
	fn prerequisite_is_covered_by_the_signature() {
		let parent_wp_hash = wp_hash(0xAB);
		let package = signed(&package_source(), &anchored_naming(vec![parent_wp_hash].into()));
		assert_eq!(package.context.prerequisites.as_ref(), &[parent_wp_hash]);

		let token = GuestToken::decode_all(&mut &package.authorization[..])
			.expect("the guest decodes the token this node encoded");
		token
			.check_signature::<Sr25519>(signable_work_package_hash(&package))
			.expect("the signature covers the prerequisite");

		let without = signed(&package_source(), &anchored(11));
		assert!(
			token.check_signature::<Sr25519>(signable_work_package_hash(&without)).is_err(),
			"the same token does not authorize the package without the prerequisite",
		);
	}

	/// The hash is the key everything else uses — the status subscription, the manager's own
	/// lookup — so it has to be the hash the node derives. polkajam's bundle builder is the
	/// reference; with no imports the bundle opens with the encoded package, so the two must
	/// agree exactly.
	/// A package is submitted under the para's own authorizer and carries a token, so a guarantor
	/// looks the AURA blob up by the hash the core's pool holds instead of waving the package
	/// through. `auth_code_host` is the parachain service, which is where that blob's preimage is
	/// hosted — a package naming any other host is one no guarantor can resolve the code for.
	#[test]
	fn a_package_runs_under_the_paras_own_authorizer() {
		let package = signed(&package_source(), &anchored(11));

		// The literal is `package_source`'s parachain service, so this pins the host to that
		// service rather than to the bootstrap service 0 the blob used to be hosted by.
		assert_eq!(package.auth_code_host, 42);
		assert_eq!(
			parachain_service_core::authorizer::authorizer_hash(&package.authorizer),
			aura().hash(),
			"the package names the authorizer whose hash a core's pool must hold",
		);
		assert!(!package.authorization.is_empty(), "and it carries a token, not an empty one");
	}

	#[test]
	fn the_package_hash_is_the_one_polkajam_derives() {
		let (package, pov) = signed_with_pov(&package_source(), &anchored(11));
		let package_bytes = jam_codec::Encode::encode(&package);

		let (reference, bundle) = build_encoded_bundle(&package, [&pov], &[Vec::new()]);

		assert_eq!(work_package_hash(&package), reference, "the hash covers only the package");
		assert_eq!(
			bundle,
			[&package_bytes[..], &pov[..]].concat(),
			"the bundle is the package followed by the PoV extrinsic",
		);
	}

	/// The PoV does not ride in the payload: `ParachainCandidate` carries only the
	/// validation-code hash, and the package's one extrinsic spec names the PoV's hash and length.
	#[test]
	fn the_pov_travels_as_the_only_extrinsic() {
		let source = package_source();
		let (package, pov) = signed_with_pov(&source, &anchored(11));

		let candidate =
			<ParachainCandidate as Decode>::decode(&mut &package.items[0].payload.0[..])
				.expect("the payload is a ParachainCandidate");
		let expected = ParachainCandidate { validation_code: source.validation_code_hash.into() };
		assert_eq!(
			candidate.encode(),
			expected.encode(),
			"the payload is the candidate and carries only the validation-code hash",
		);

		assert_eq!(package.items[0].extrinsics.len(), 1, "the PoV is the one extrinsic");
		let spec = &package.items[0].extrinsics[0];
		assert_eq!(spec.len, pov.len() as u32);
		assert_eq!(spec.hash, jam_types::ExtrinsicHash::from(jam_std_common::hash_raw(&pov)));
	}

	/// The extrinsic bytes a bundle carries decode as the V4 `ParachainBlockData` the runtime and
	/// the recovery decoder expect.
	#[test]
	fn the_pov_extrinsic_decodes_as_parachain_block_data() {
		let (package, pov) = signed_with_pov(&package_source(), &anchored(11));
		let package_bytes = jam_codec::Encode::encode(&package);
		let (_, bundle) = build_encoded_bundle(&package, [&pov], &[Vec::new()]);

		let decoded = ParachainBlockData::<TestBlock>::decode(&mut &bundle[package_bytes.len()..])
			.expect("the trailing extrinsic bytes decode as the PoV");

		assert_eq!(decoded.blocks().len(), 1, "the one block the package proves");
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

	/// A forgotten package's child goes with it: the child names the parent's package as its
	/// prerequisite, and a prerequisite nothing will ever report blocks the child for good. Both
	/// entries come back so `forget` closes both subscriptions.
	#[test]
	fn cascade_forget_drops_child_package() {
		let mut packages = in_flight(2);
		let parent_block_hash = packages.entries[0].block_hash;
		let parent = packages.entries[0].wp_hash;
		let child = packages.entries[1].wp_hash;
		packages.entries[1].anchored.context.prerequisites = vec![parent].into();
		packages.entries[1].package.context.prerequisites = vec![parent].into();
		let ledger = ledger();
		ledger.insert(&parent_block_hash.into(), parent).expect("insert ok; qed");

		let forgotten = forget_package(&mut packages, &ledger, 0);

		assert_eq!(forgotten.len(), 2, "the child is forgotten with the parent it named");
		assert!(packages.position_of_package(parent).is_none());
		assert!(packages.position_of_package(child).is_none(), "no child outlives its parent");
	}

	/// A forgotten package leaves no ledger entry behind: that entry is what a later child would
	/// name as its prerequisite, and keeping it would chain the child to a package nothing will
	/// ever report.
	#[test]
	fn forget_removes_the_ledger_entry() {
		let mut packages = in_flight(1);
		let block_hash = packages.entries[0].block_hash;
		let submitted = packages.entries[0].wp_hash;
		let ledger = ledger();
		ledger.insert(&block_hash.into(), submitted).expect("insert ok; qed");

		forget_package(&mut packages, &ledger, 0);

		assert_eq!(
			ledger.get(&block_hash.into()).expect("read ok; qed"),
			None,
			"a forgotten package leaves no entry for a child to name",
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
	/// stops being tracked — accumulated, superseded, forgotten — its subscription has to go
	/// with it, and following it again must not add a second one.
	#[test]
	fn a_status_subscription_never_outlives_the_package_it_follows() {
		let mut packages = in_flight(4);
		let mut subscriptions = StatusSubscriptions::new();
		let mut statuses = SelectAll::new();
		for entry in &packages.entries {
			statuses.push(subscriptions.follow(entry.wp_hash, endless_statuses()));
		}
		assert_eq!(subscriptions.len(), 4);

		let followed_twice = packages.entries[3].wp_hash;
		statuses.push(subscriptions.follow(followed_twice, endless_statuses()));
		assert_eq!(subscriptions.len(), 4, "following again replaces a package's subscription");

		let settled = packages.entries[1].block_number;
		for entry in packages.remove_up_to(settled) {
			assert!(subscriptions.close(entry.wp_hash), "the settled packages are closed");
		}
		assert_eq!(subscriptions.len(), 2);

		for entry in packages.remove_up_to(99) {
			assert!(subscriptions.close(entry.wp_hash), "and so is a forgotten one");
		}
		assert_eq!(subscriptions.len(), 0, "no handle outlives the packages");
		assert!(!subscriptions.close(followed_twice), "closing twice is not a leak either");
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

	/// `build_pov` must produce a V4 PoV whose `parent_header()` accessor returns the
	/// SCALE-encoded header that was passed in. The PVF reads it to establish `state_root`; the
	/// parachain service's `accumulate` verifies it against its stored head. The additional-data
	/// map assembled at build time travels with it, under `JAM_PROOF_KEY`.
	#[test]
	fn the_pov_carries_the_parent_header() {
		let parent_header = header(4, H256::repeat_byte(6));
		let block_header = header(5, parent_header.hash());
		// Shaped like the production entry `register_jam_state_reader` stores.
		let entry = ([7u8; 32], [8u8; 32], StateProof { nodes: vec![], values: vec![] }).encode();
		let additional_data = [(JAM_PROOF_KEY.to_string(), entry.clone())].into();

		let pov = build_pov(
			&[TestBlock::new(block_header, vec![])],
			&CompactProof { encoded_nodes: vec![vec![1u8, 2, 3]] },
			&parent_header,
			&additional_data,
			Vec::new(),
		);

		let decoded = ParachainBlockData::<TestBlock>::decode(&mut &pov[..])
			.expect("PoV decodes as ParachainBlockData");

		let expected = parent_header.encode();
		assert_eq!(
			decoded.parent_header(),
			Some(expected.as_slice()),
			"the V4 PoV carries the SCALE-encoded parent header",
		);
		assert_eq!(
			decoded.additional_data(),
			vec![Some(additional_data.clone())],
			"and the additional-data map assembled at build time",
		);
		let carried = decoded.additional_data()[0].as_ref().expect("the V4 PoV carries the map");
		assert_eq!(
			carried.get(JAM_PROOF_KEY).map(|value| value.as_slice()),
			Some(entry.as_slice()),
			"the `JAM_PROOF_KEY` entry round-trips byte-exact",
		);
		assert!(decoded.anchor_chain().is_empty(), "an authoring-time PoV carries no anchor chain");
	}

	/// The extracted `work_package` has to assemble exactly what `PackageSource::package` did, so
	/// every path that builds a package around a block produces the same work item.
	#[test]
	fn work_package_matches_package_source() {
		let source = package_source();
		let anchored = anchored(11);
		let (package, pov) = source.package(&anchored);

		let expected = work_package(
			vec![extrinsic_spec(&pov)],
			source.validation_code_hash,
			&source.params,
			source.authorizer.clone(),
			anchored.context.clone(),
		);

		assert_eq!(
			jam_codec::Encode::encode(&package),
			jam_codec::Encode::encode(&expected),
			"the extracted assembler and the author path agree byte for byte",
		);
	}

	/// The PoV store keeps an authored block the way the import path keeps an imported one, and
	/// what it keeps rebuilds the very PoV the block's package carries.
	#[test]
	fn an_authored_block_is_kept_with_what_rebuilds_its_pov() {
		let jam_parent = JamParent {
			anchor: H256::repeat_byte(9),
			anchor_slot: 11,
			lookup_anchor: H256::repeat_byte(6),
			lookup_anchor_slot: 11,
		};
		let mut source = package_source();
		source.blocks[0].header.digest.push(
			CumulusDigestItem::JamParent {
				anchor: jam_parent.anchor,
				anchor_slot: jam_parent.anchor_slot,
				lookup_anchor: jam_parent.lookup_anchor,
				lookup_anchor_slot: jam_parent.lookup_anchor_slot,
			}
			.to_digest_item(),
		);
		let block = source.blocks[0].clone();
		let parent_header = source.parent_header.clone();
		let (package, pov) = source.package(&anchored(11));
		let spec = package.items[0].extrinsics[0].clone();

		let kept = source.into_block_pov(spec.clone()).expect("the block carries `JamParent`");

		assert_eq!(kept.block, block);
		assert_eq!(kept.parent_header, parent_header);
		assert_eq!(kept.jam_parent, jam_parent);
		assert_eq!(kept.parent_hash, *block.header().parent_hash());
		assert_eq!(kept.number, *block.header().number());
		assert_eq!(kept.spec.hash, spec.hash);
		assert_eq!(kept.spec.len, pov.len() as u32);
		assert_eq!(
			build_pov(
				std::slice::from_ref(&kept.block),
				&kept.compact_proof,
				&kept.parent_header,
				&kept.additional_data,
				Vec::new(),
			),
			pov,
			"the kept entry alone rebuilds the submitted PoV byte for byte",
		);
		assert!(
			package_source().into_block_pov(spec).is_none(),
			"a block without a `JamParent` digest is not kept, as on the import path",
		);
	}

	/// Block `number` on top of `parent_hash`, built against a JAM anchor at `anchor_slot` and
	/// kept in the PoV store the way the authoring and import paths keep one.
	fn kept_block(number: u32, parent_hash: H256, anchor_slot: JamSlot) -> BlockPov<TestBlock> {
		let mut block_header = header(number, parent_hash);
		block_header.digest.push(
			CumulusDigestItem::JamParent {
				anchor: H256::repeat_byte(anchor_slot as u8),
				anchor_slot,
				lookup_anchor: H256::repeat_byte(0x80 | anchor_slot as u8),
				lookup_anchor_slot: anchor_slot.saturating_sub(1),
			}
			.to_digest_item(),
		);
		let mut source = package_source();
		source.blocks = vec![TestBlock::new(block_header, vec![])];
		source.parent_header = header(number - 1, H256::repeat_byte(6));
		let (package, _pov) = source.package(&anchored(11));
		let spec = package.items[0].extrinsics[0].clone();
		source.into_block_pov(spec).expect("the block carries `JamParent`")
	}

	/// The chain a round rebuilds runs from the head's child to the local best block, oldest
	/// first — and only while the head is an ancestor of best: a head on another fork has nothing
	/// above it on the best chain, so the walk yields nothing rather than the other fork's blocks.
	#[test]
	fn the_chain_above_the_head_is_the_best_chains_blocks_past_it() {
		let h0 = header(0, H256::repeat_byte(0));
		let h1 = header(1, h0.hash());
		let h2 = header(2, h1.hash());
		let h3 = header(3, h2.hash());
		let mut fork = header(1, h0.hash());
		fork.state_root = H256::repeat_byte(0xF0);
		let headers = [h0.clone(), h1.clone(), h2.clone(), h3.clone(), fork.clone()];
		let lookup = |hash: H256| headers.iter().find(|h| h.hash() == hash).cloned();

		assert_eq!(
			chain_above(h3.hash(), (h1.hash(), 1), lookup),
			Some(vec![h2.hash(), h3.hash()]),
			"the head's child first, best last",
		);
		assert_eq!(
			chain_above(h3.hash(), (h3.hash(), 3), lookup),
			Some(vec![]),
			"best is the head"
		);
		assert_eq!(chain_above(h3.hash(), (fork.hash(), 1), lookup), None, "head on another fork");
		assert_eq!(chain_above(h1.hash(), (h3.hash(), 3), lookup), None, "best behind the head");
		assert_eq!(chain_above(H256::repeat_byte(0xEE), (h1.hash(), 1), lookup), None, "unknown");
	}

	/// A round rebuilds the leading run of blocks whose PoVs are stored and whose original
	/// packages have expired, oldest first and at most `limit` long; a block failing either test
	/// ends the run, and the blocks behind it wait for a later round.
	#[test]
	fn only_the_leading_run_of_expired_stored_blocks_is_rebuilt() {
		let anchor_slot = 10;
		let a = kept_block(1, H256::repeat_byte(7), anchor_slot);
		let b = kept_block(2, a.block.hash(), anchor_slot + 1);
		let c = kept_block(3, b.block.hash(), anchor_slot + 1);
		let (ha, hb, hc) = (a.block.hash(), b.block.hash(), c.block.hash());
		let store = BlockPovs::<TestBlock>::new(8);
		for kept in [&a, &b, &c] {
			store.insert(kept.block.hash(), kept.clone());
		}
		let above = vec![ha, hb, hc];
		let hashes = |blocks: Vec<(H256, BlockPov<TestBlock>)>| {
			blocks.into_iter().map(|(hash, _)| hash).collect::<Vec<_>>()
		};

		// However long the head has stalled, a block whose first package can still be reported
		// is never rebuilt.
		let still_live = anchor_slot + REPORT_DEADLINE_SLOTS - 1;
		assert!(rebuildable(&above, still_live, |h| store.get(h), MAX_RESUBMIT_BLOCKS).is_empty());
		// A's package has expired, B's and C's have not: the run is A alone.
		let a_expired = anchor_slot + REPORT_DEADLINE_SLOTS;
		assert_eq!(
			hashes(rebuildable(&above, a_expired, |h| store.get(h), MAX_RESUBMIT_BLOCKS)),
			vec![ha],
		);
		// All expired: the whole run, oldest first, under the cap.
		let all_expired = anchor_slot + 1 + REPORT_DEADLINE_SLOTS;
		assert_eq!(
			hashes(rebuildable(&above, all_expired, |h| store.get(h), MAX_RESUBMIT_BLOCKS)),
			vec![ha, hb, hc],
		);
		assert_eq!(hashes(rebuildable(&above, all_expired, |h| store.get(h), 2)), vec![ha, hb]);
		// A gap in the store ends the run: C is not rebuilt past a missing B.
		let gapped = BlockPovs::<TestBlock>::new(8);
		gapped.insert(ha, a.clone());
		gapped.insert(hc, c.clone());
		assert_eq!(
			hashes(rebuildable(&above, all_expired, |h| gapped.get(h), MAX_RESUBMIT_BLOCKS)),
			vec![ha],
		);
		// And a missing first block means nothing is rebuilt at all.
		let empty = BlockPovs::<TestBlock>::new(8);
		assert!(rebuildable(&above, all_expired, |h| empty.get(h), MAX_RESUBMIT_BLOCKS).is_empty());
	}

	/// A resubmission package carries exactly one extrinsic — the rebuilt PoV, whose
	/// `ParachainBlockData` carries the anchor chain — whose spec names the blob it is submitted
	/// with; package k > 0 of a round names package k − 1 as its single prerequisite, and package
	/// 0 names nothing.
	#[test]
	fn resubmission_packages_chain_and_carry_the_anchor_chain_in_the_pov() {
		let first = kept_block(1, H256::repeat_byte(7), 5);
		let second = kept_block(2, first.block.hash(), 6);
		let headers = vec![vec![1u8; 120], vec![2u8; 120]];
		let source = package_source();
		let context = anchored(30).context;
		let assemble = |block_pov: &BlockPov<TestBlock>, prerequisite| {
			resubmission_package(
				block_pov,
				&headers,
				prerequisite,
				source.validation_code_hash,
				&source.params,
				source.authorizer.clone(),
				&context,
			)
		};

		let (package_0, pov_0) = assemble(&first, None);
		let (package_1, pov_1) = assemble(&second, Some(work_package_hash(&package_0)));

		assert!(package_0.context.prerequisites.as_ref().is_empty(), "package 0 starts the chain");
		assert_eq!(
			package_1.context.prerequisites.as_ref(),
			&[work_package_hash(&package_0)],
			"package 1 names exactly package 0",
		);
		for (package, pov, kept) in [(&package_0, &pov_0, &first), (&package_1, &pov_1, &second)] {
			assert_eq!(package.items[0].extrinsics.len(), 1, "the PoV is the one extrinsic");
			let spec = &package.items[0].extrinsics[0];
			assert_eq!(
				(spec.hash, spec.len),
				(jam_types::ExtrinsicHash::from(jam_std_common::hash_raw(pov)), pov.len() as u32),
				"the spec names the PoV's hash and length",
			);
			assert_eq!(
				*pov,
				build_pov(
					std::slice::from_ref(&kept.block),
					&kept.compact_proof,
					&kept.parent_header,
					&kept.additional_data,
					headers.clone(),
				),
				"the extrinsic is the block's rebuilt PoV",
			);
			let decoded = ParachainBlockData::<TestBlock>::decode_all(&mut &pov[..])
				.expect("PoV decodes as ParachainBlockData");
			assert_eq!(decoded.anchor_chain(), &headers[..], "the PoV carries the anchor chain");
			assert_eq!(package.context.anchor, context.anchor, "anchored where the round is");
		}
	}
}
