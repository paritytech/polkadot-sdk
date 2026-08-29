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

//! The JAM collation manager (phase 5): the work-package lifecycle for a *chain* of packages.
//!
//! One task owns everything, instead of the phase-1 follower spawned per package. It keeps the
//! [`InFlightChain`] — the packages submitted for the builder's unincluded segment, oldest first
//! — and selects over the builder's channel, the para-head stream, the status subscriptions of
//! every submitted package, and a once-per-JAM-slot timer.
//!
//! For each block the builder hands over, the manager decides how the package links into the
//! chain by *obeying* the [`ParentLink`] the builder resolved at that package's anchor
//! ([`decide_tip_link`]): the first one anchors on the head JAM has accumulated, every later
//! one names its parent's package as a prerequisite *and* imports that package's segment 0 (the
//! parent's header), which is how the service learns in-core which unaccumulated block this one
//! builds on. The package goes out as a bundle carrying that segment inline. A parent whose
//! package this task never submitted — another collator's block, or one of ours from before a
//! restart — is *adopted* instead ([`decide_reported_link`]): the builder found its package hash
//! in JAM's in-flight reports, and the parent's export is reproduced from its header and checked
//! against the segment root that report commits to before anything is linked onto it.
//!
//! Failure handling is drop-tail: a package that can no longer be reported takes every
//! descendant with it (their links name a package that will never exist), and the builder is
//! told to reset its segment so the next block starts from the accumulated head again. The
//! phase-4 re-contexting survives only for a root package with no children, because re-contexting
//! rewrites a package's bytes and therefore its hash — which every child has already named.
//!
//! The PoV is a V3 `ParachainBlockData` whose single additional-data slot carries the anchor
//! state proof. Because that proof is verified in-core against the refine context's state root,
//! anchor and PoV are inseparable: re-contexting a package means re-proving the para head and
//! rebuilding the payload, not just swapping the context out.
//!
//! Phase-1 simplifications that still stand: null authorizer (empty token, nothing to sign),
//! fixed core, PoV is NOT zstd-compressed (parasim rejects compressed PoVs; JIP-2 is silent on
//! compression).

use super::{
	ANCHOR_STATE_PROOF_KEY, JAM_SLOT_DURATION_MS, JamCollatorMessage, LOG_TARGET, ParentLink,
	fetch_anchor_state_proof, jam_slot_at, para_head_stream,
	resubmission::*,
	segments::{Export, export_of},
};
use crate::common::{ConstructNodeRuntimeApi, NodeBlock, types::ParachainClient};
use codec::{Decode, Encode};
use cumulus_primitives_core::{AdditionalData, ParachainBlockData, SchedulingProof};
use futures::{FutureExt, StreamExt, channel::mpsc, stream::SelectAll};
use jam_cumulus_facade::{ParachainCandidate, authorizer::fixed_authorizer};
use jam_interface::{
	BoxStream, CoreIndex, HeaderHash, JamChainSource, JamStateSource,
	JamWorkPackageSubmission, ServiceId, Slot as JamSlot, VersionedParameters, WorkPackage,
	WorkPackageHash, WorkPackageStatus,
};
use jam_state_helpers::StateProof;
use jam_std_common::{ImportData, build_encoded_bundle};
use jam_types::{
	Authorization, CodeHash, ImportSpec, RefineContext, RootIdentifier, SegmentTreeRoot,
	UnsignedGas, WorkItem, WorkPayload,
};
use polkadot_primitives::Id as ParaId;
use sp_runtime::traits::{Block as BlockT, HashingFor, Header as HeaderT};
use sp_timestamp::Timestamp;
use sp_trie::CompactProof;
use std::{
	collections::{HashSet, VecDeque},
	sync::Arc,
	time::{Duration, Instant},
};

const RETRY_DELAY: Duration = Duration::from_secs(6);

/// How long a package has to be reported, counted from its anchor and from its prerequisite's
/// submission. Two clocks, the same length: the anchor must still be in recent history when the
/// package is reported, and so must the prerequisite's own report.
const REPORT_DEADLINE_SLOTS: JamSlot = 8;

/// How many accumulated packages [`RecentlyAccumulated`] keeps. It only has to cover the gap
/// between the builder's tick and the arrival of that tick's block here — milliseconds — so a
/// handful of blocks is already generous.
const RECENTLY_ACCUMULATED_CAP: usize = 8;

pub(crate) struct CollationTaskParams<Block: NodeBlock, RuntimeApi, Jam> {
	pub para_client: Arc<ParachainClient<Block, RuntimeApi>>,
	pub jam: Arc<Jam>,
	pub para_id: ParaId,
	pub service_id: ServiceId,
	pub core: CoreIndex,
	pub message_receiver: mpsc::Receiver<JamCollatorMessage<Block>>,
	/// Ask the builder to drop its unincluded segment and start again from the accumulated head.
	/// Since 5.2 this is the *only* thing that heals a lost package: the builder keeps extending
	/// its own segment and never re-authors a stalled head on its own.
	pub rebuild_sender: mpsc::Sender<()>,
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
		rebuild_sender,
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

	// The chain starts empty after a restart, and stays empty until the builder hands over a
	// block: the builder is the side that reads JAM's availability assignments and ready queue,
	// so the first message re-roots the chain here, on the accumulated head
	// (`ParentLink::Included`) or on a package only JAM state knows about
	// (`ParentLink::Reported`).
	let mut manager = Manager {
		para_client,
		jam,
		para_id: para_id.into(),
		service_id,
		core,
		service_code_hash,
		refine_gas_limit,
		accumulate_gas_limit,
		policy: DropTailOnFailure::new(max_resubmits),
		rebuild_sender,
		announce_block,
		chain: InFlightChain::new(),
		recently_accumulated: RecentlyAccumulated::new(),
		included_head: None,
		submitted_blocks: HashSet::new(),
		statuses: SelectAll::new(),
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

/// One submitted work package, still in flight.
struct InFlight<Block: BlockT> {
	block_hash: Block::Hash,
	block_number: <Block::Header as HeaderT>::Number,
	parent_hash: Block::Hash,
	wp_hash: WorkPackageHash,
	/// The bundle exactly as submitted. A resubmission replays these bytes verbatim.
	bundle: Vec<u8>,
	/// This block's own export — the segment a child's bundle carries inline, and the segment
	/// root a work report for this package will commit to.
	export: Export,
	/// What the package would be rebuilt from around a fresh anchor. Only usable while the entry
	/// has no children: re-contexting changes the hash they name.
	source: PackageSource<Block>,
	anchored: Anchored,
	/// The JAM slot this package was last submitted in, which is both the soft-resubmit timer's
	/// zero and the moment a child's dependency reference started ageing.
	submitted_at: JamSlot,
	reported: bool,
	resubmits: u32,
}

/// The chain of packages submitted for the builder's unincluded segment, oldest first. Entry
/// `k + 1` names entry `k` as its prerequisite and imports its exported header.
struct InFlightChain<Block: BlockT> {
	entries: VecDeque<InFlight<Block>>,
}

impl<Block: BlockT> InFlightChain<Block> {
	fn new() -> Self {
		Self { entries: VecDeque::new() }
	}

	fn depth(&self) -> usize {
		self.entries.len()
	}

	fn tip(&self) -> Option<&InFlight<Block>> {
		self.entries.back()
	}

	/// What [`decide_link`] needs to know about the chain: the tip's block and its package.
	fn tip_link(&self) -> Option<(Block::Hash, WorkPackageHash)> {
		self.tip().map(|tip| (tip.block_hash, tip.wp_hash))
	}

	/// The block the whole chain hangs off: the accumulated head for a chain rooted here, another
	/// collator's in-flight block for one adopted from JAM's reports.
	fn root_parent(&self) -> Option<Block::Hash> {
		self.entries.front().map(|entry| entry.parent_hash)
	}

	fn position_of_block(&self, block_hash: Block::Hash) -> Option<usize> {
		self.entries.iter().position(|entry| entry.block_hash == block_hash)
	}

	fn position_of_package(&self, wp_hash: WorkPackageHash) -> Option<usize> {
		self.entries.iter().position(|entry| entry.wp_hash == wp_hash)
	}

	/// Everything up to and including `index` has been accumulated; drop it and return the
	/// entries, whose packages stay linkable for a short while ([`RecentlyAccumulated`]).
	fn pop_through(&mut self, index: usize) -> Vec<InFlight<Block>> {
		self.entries.drain(..=index).collect()
	}

	/// `index` and every descendant can never be reported; drop them and return what was dropped.
	fn drop_tail(&mut self, index: usize) -> Vec<Block::Hash> {
		self.entries.drain(index..).map(|entry| entry.block_hash).collect()
	}

	fn clear(&mut self) -> Vec<Block::Hash> {
		self.drop_tail(0)
	}

	fn block_hashes(&self) -> Vec<Block::Hash> {
		self.entries.iter().map(|entry| entry.block_hash).collect()
	}
}

/// One package that left the chain because JAM accumulated it.
struct AccumulatedPackage<Hash> {
	block_hash: Hash,
	wp_hash: WorkPackageHash,
	/// The block's export. A child chaining onto it carries this inline, exactly as it would for
	/// a parent still in the chain — which is why the entry is kept at all.
	export: Export,
}

/// The last few packages JAM accumulated, oldest first.
///
/// A parent can accumulate in the moment between the builder's tick and the arrival of the
/// child's message here: the child was anchored where that parent was still in flight, so its
/// package must chain onto the parent's, but the chain no longer holds it. This buffer is what
/// keeps that link available; without it the child looks like a chain root and gets submitted
/// with a link its own anchor proof contradicts.
struct RecentlyAccumulated<Hash> {
	entries: VecDeque<AccumulatedPackage<Hash>>,
}

/// A parent found in [`RecentlyAccumulated`]: everything the chained link needs, plus how far
/// back it now sits.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AccumulatedParent {
	wp_hash: WorkPackageHash,
	export: Export,
	/// How many of our blocks accumulated after this one.
	blocks_ago: usize,
}

impl<Hash: Copy + PartialEq> RecentlyAccumulated<Hash> {
	fn new() -> Self {
		Self { entries: VecDeque::new() }
	}

	fn remember(&mut self, block_hash: Hash, wp_hash: WorkPackageHash, export: Export) {
		self.entries.push_back(AccumulatedPackage { block_hash, wp_hash, export });
		while self.entries.len() > RECENTLY_ACCUMULATED_CAP {
			self.entries.pop_front();
		}
	}

	fn parent(&self, block_hash: Hash) -> Option<AccumulatedParent> {
		let index = self.entries.iter().position(|entry| entry.block_hash == block_hash)?;
		Some(AccumulatedParent {
			wp_hash: self.entries[index].wp_hash,
			export: self.entries[index].export.clone(),
			blocks_ago: self.entries.len() - 1 - index,
		})
	}

	fn len(&self) -> usize {
		self.entries.len()
	}
}

/// How a new block's package links into the chain of in-flight packages.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Link<Hash> {
	/// The block builds on the head JAM has accumulated (or on genesis): no prerequisite,
	/// nothing imported. `drain_accumulated` means this task is still holding the packages for
	/// that head — the anchor proves they accumulated, so they leave the chain here instead of
	/// waiting for the para-head subscription to say the same thing.
	Root { drain_accumulated: bool },
	/// The block's parent is the chain tip's block: name the tip's package as the prerequisite
	/// and import its exported header.
	Chain(WorkPackageHash),
	/// The block's parent accumulated while the block was on its way over from the builder. The
	/// link is the ordinary chained one — prerequisite plus import — served from
	/// [`RecentlyAccumulated`] instead of from the chain.
	RecentlyAccumulated(AccumulatedParent),
	/// The block's parent is in flight but its package was never submitted from here — another
	/// collator's, or ours from before a restart. Start a new chain segment on that package,
	/// whose export has to be reproduced from the parent's header. `drain_accumulated` means
	/// this task is still holding packages the anchor proves accumulated; they leave the chain
	/// here, exactly as they do for [`Link::Root`].
	Adopt { wp_hash: WorkPackageHash, segroot: SegmentTreeRoot, drain_accumulated: bool },
	/// The block builds on something this task is not tracking. Builder and manager disagree
	/// about the chain, which is a bug on one of the two sides.
	Mismatch { expected: Option<Hash> },
}

/// Decide the link for a block whose parent the builder saw as still in flight
/// ([`ParentLink::Tip`]).
///
/// **The anchor state the package carries is the authority here, never this task's newer view
/// of the para head.** The builder resolved the parent against the head *proven at the anchor*,
/// and that proof travels inside the PoV: refine checks the block's parent against exactly that
/// head and takes the import path because of it. A para-head notification that lands here in
/// the meantime says nothing about the anchor — only that this task has since seen further — so
/// letting it override the builder would submit a package whose link contradicts its own proof.
/// That is precisely the failure this function exists to avoid: a root package anchored at a
/// state proving the head *before* the parent makes refine fail with a missing import, which
/// zeroes the exports and cascades down every descendant.
///
/// Chaining onto a parent that accumulated a moment ago is fully valid: the prerequisite and the
/// `Indirect` import resolve against recent history, refine imports the parent's header exactly
/// as it would for a parent still in flight, and accumulate's freshness check finds the stored
/// head this block's parent is. Only a parent that is in neither the chain nor the buffer is a
/// genuine disagreement between the two sides.
fn decide_tip_link<Hash: Copy + PartialEq>(
	tip: Option<(Hash, WorkPackageHash)>,
	included_head: Option<Hash>,
	recently_accumulated: Option<AccumulatedParent>,
	parent_hash: Hash,
) -> Link<Hash> {
	match tip {
		Some((tip_block, tip_package)) if tip_block == parent_hash => Link::Chain(tip_package),
		Some((tip_block, _)) => Link::Mismatch { expected: Some(tip_block) },
		None => match recently_accumulated {
			Some(parent) => Link::RecentlyAccumulated(parent),
			None => Link::Mismatch { expected: included_head },
		},
	}
}

/// Decide the link for a block the builder authored on the para head proven at its anchor
/// ([`ParentLink::Included`]).
///
/// The package is a root: the anchor's own proof shows the parent *is* the accumulated head, so
/// there is nothing in flight left for it to depend on. This task may still be holding the
/// packages for that head, because accumulation reached the anchor before it reached the
/// subscription; the anchor has already proved them accumulated, so they are drained rather
/// than believed. A chain that ends anywhere else holds blocks the anchor says nothing about —
/// unaccumulated descendants of the parent, which the builder has forgotten and this task has
/// not — and rooting a sibling next to them would fork our own chain, so that stays the
/// ordinary mismatch that resynchronises both sides.
fn decide_included_link<Hash: Copy + PartialEq>(
	tip: Option<(Hash, WorkPackageHash)>,
	parent_hash: Hash,
) -> Link<Hash> {
	match tip {
		Some((tip_block, _)) if tip_block == parent_hash => Link::Root { drain_accumulated: true },
		Some((tip_block, _)) => Link::Mismatch { expected: Some(tip_block) },
		None => Link::Root { drain_accumulated: false },
	}
}

/// Decide the link for a parent whose package the builder knows only from JAM's in-flight
/// reports — another collator's block, or one of ours from before a restart.
///
/// Our own ledger still wins wherever it has an opinion: if that parent is the chain tip then we
/// submitted its package ourselves and know its hash first-hand, so the reported hash is only a
/// confirmation.
///
/// A chain that ends at the head the *anchor* proves accumulated is the same anchor-owned
/// invariant [`decide_tip_link`] and [`decide_included_link`] carry, with the two sides the other
/// way round: here the anchor is ahead of this task rather than behind it. The builder saw our
/// own block accumulate and another collator's successor take over as the in-flight tip, and it
/// built on that successor; the para-head notification saying the same thing simply has not
/// arrived yet. The anchor has already proved those entries accumulated, so they are drained on
/// its evidence and the reported parent is adopted, instead of the arrival order deciding whether
/// a perfectly good block survives.
///
/// A chain ending anywhere else holds entries the anchor says nothing about — a genuine
/// disagreement between the two sides, which resynchronising is what resolves.
fn decide_reported_link<Hash: Copy + PartialEq>(
	tip: Option<(Hash, WorkPackageHash)>,
	anchor_included_head: Hash,
	parent_hash: Hash,
	wp_hash: WorkPackageHash,
	segroot: SegmentTreeRoot,
) -> Link<Hash> {
	match tip {
		Some((tip_block, tip_package)) if tip_block == parent_hash => Link::Chain(tip_package),
		Some((tip_block, _)) if tip_block == anchor_included_head =>
			Link::Adopt { wp_hash, segroot, drain_accumulated: true },
		Some((tip_block, _)) => Link::Mismatch { expected: Some(tip_block) },
		None => Link::Adopt { wp_hash, segroot, drain_accumulated: false },
	}
}

/// The parent's export, recomputed from its header and cross-checked against the segment root the
/// on-chain report for its package commits to.
///
/// The recomputation is what makes adopting a package we did not submit possible at all: the
/// collator never decodes the report's work output, it reproduces the bytes the parent's package
/// must have exported and checks they hash to the root the chain has already authenticated. A
/// mismatch means the byte contract and the on-chain reality disagree, and nothing built on that
/// assumption — least of all the child's import proof — would verify.
fn adopted_parent_export<Header: HeaderT>(
	parent_header: &Header,
	segroot: SegmentTreeRoot,
) -> Result<Export, String> {
	let export = export_of(&parent_header.encode())?;
	if export.segroot != segroot {
		return Err(format!(
			"the parent's export root recomputed from its header is {:?}, but its work report \
			 commits to {segroot:?}",
			export.segroot,
		));
	}
	Ok(export)
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
	policy: DropTailOnFailure,
	rebuild_sender: mpsc::Sender<()>,
	announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	chain: InFlightChain<Block>,
	/// The packages that left `chain` because JAM accumulated them, kept just long enough for a
	/// block anchored while they were still in flight to chain onto one of them.
	recently_accumulated: RecentlyAccumulated<Block::Hash>,
	/// The para head last seen in JAM state; `None` until the stream reports one.
	included_head: Option<Block::Hash>,
	/// Every para block whose package was submitted, so an accumulated head can be told apart
	/// from another collator's.
	submitted_blocks: HashSet<Block::Hash>,
	statuses: SelectAll<BoxStream<'static, (WorkPackageHash, WorkPackageStatus)>>,
}

impl<Block, RuntimeApi, Jam> Manager<Block, RuntimeApi, Jam>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	Jam: JamChainSource + JamStateSource + JamWorkPackageSubmission + 'static,
{
	/// A block from the builder: link it into the chain, assemble the bundle, submit, track.
	async fn on_new_block(&mut self, message: JamCollatorMessage<Block>) {
		let JamCollatorMessage {
			parent_header,
			parent_link,
			block,
			proof,
			context,
			anchor_state_root,
			anchor_state_proof,
			anchor_included_head,
			anchor_slot,
			triggered_by,
		} = message;
		let block_hash = block.hash();
		let block_number = *block.header().number();
		let parent_hash = parent_header.hash();

		// The builder's `ParentLink` is obeyed, not second-guessed: it was resolved against the
		// para head proven at this package's anchor, and that proof is what refine will check
		// the block against. Whatever the para-head subscription has told this task since is a
		// *newer* view than the anchor's, and a newer view cannot invalidate a link the anchor
		// still proves — see [`decide_tip_link`].
		let link = match parent_link {
			ParentLink::Reported { wp_hash, segroot } => decide_reported_link(
				self.chain.tip_link(),
				anchor_included_head,
				parent_hash,
				wp_hash,
				segroot,
			),
			ParentLink::Tip => decide_tip_link(
				self.chain.tip_link(),
				self.included_head,
				self.recently_accumulated.parent(parent_hash),
				parent_hash,
			),
			ParentLink::Included => decide_included_link(self.chain.tip_link(), parent_hash),
		};
		tracing::debug!(
			target: LOG_TARGET,
			?block_hash,
			%block_number,
			?parent_hash,
			?parent_link,
			?anchor_included_head,
			included_head = ?self.included_head,
			chain_depth = self.chain.depth(),
			chain_tip = ?self.chain.tip_link().map(|(block, _)| block),
			?link,
			?triggered_by,
			"Linking a new block into the in-flight chain.",
		);
		// The parent's export travels inline with the child's bundle, so it is needed here as
		// well as its package hash: held for a parent we submitted, recomputed for one we are
		// adopting on the strength of its report.
		let (prerequisite, parent_export) = match link {
			Link::Root { drain_accumulated } => {
				if drain_accumulated {
					let drained = self.drain_chain_as_accumulated();
					tracing::info!(
						target: LOG_TARGET,
						?block_hash,
						%block_number,
						?parent_hash,
						?drained,
						"The anchor proves the parent is the accumulated head while this task \
						 still held the packages for it; draining them and rooting the new \
						 package, rather than waiting for the para-head subscription to catch up.",
					);
				}
				(None, None)
			},
			Link::Chain(parent_package) =>
				(Some(parent_package), self.chain.tip().map(|tip| tip.export.clone())),
			Link::RecentlyAccumulated(parent) => {
				tracing::info!(
					target: LOG_TARGET,
					?block_hash,
					%block_number,
					?parent_hash,
					parent_wp_hash = ?parent.wp_hash,
					accumulated_blocks_ago = parent.blocks_ago,
					buffered = self.recently_accumulated.len(),
					"The parent accumulated while this block was on its way over from the \
					 builder; linking onto its package all the same — the anchor this package \
					 carries proves the parent was still in flight, so a chained link is what \
					 refine verifies and a root package is what it would reject.",
				);
				(Some(parent.wp_hash), Some(parent.export))
			},
			Link::Adopt { wp_hash, segroot, drain_accumulated } => {
				// The same invariant the recently-accumulated buffer encodes: this task's
				// subscription view never overrides the anchor. There the anchor was behind us
				// and we kept a popped parent linkable; here it is ahead of us, having proved
				// the entries we still hold accumulated, so they leave on its evidence.
				if drain_accumulated {
					let drained = self.drain_chain_as_accumulated();
					tracing::info!(
						target: LOG_TARGET,
						?block_hash,
						%block_number,
						?parent_hash,
						?drained,
						?anchor_included_head,
						buffered = self.recently_accumulated.len(),
						"The anchor proves the packages this task still held accumulated and \
						 another collator's block took over as the in-flight tip; draining them \
						 and adopting the reported parent the builder picked on that same anchor, \
						 rather than waiting for the para-head subscription to catch up.",
					);
				}
				match adopted_parent_export(&parent_header, segroot) {
					Ok(export) => (Some(wp_hash), Some(export)),
					Err(error) => {
						tracing::error!(
							target: LOG_TARGET,
							?block_hash,
							?parent_hash,
							?wp_hash,
							error,
							"The parent's export cannot be reproduced from the header we hold; \
							 dropping the block rather than submitting a package whose import no \
							 guarantor could verify.",
						);
						return;
					},
				}
			},
			Link::Mismatch { expected } => {
				tracing::error!(
					target: LOG_TARGET,
					?block_hash,
					?parent_hash,
					?expected,
					chain_depth = self.chain.depth(),
					"The builder built on a block this task is not tracking; dropping it and \
					 resynchronising both sides on the accumulated head.",
				);
				self.reset_chain("the builder and the collation manager disagree on the chain");
				return;
			},
		};

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

		// Every package exports its block's header, root case included, so that whoever authors
		// the next block — us or another collator — has something to chain onto.
		let encoded_header = block.header().encode();
		let export = match export_of(&encoded_header) {
			Ok(export) => export,
			Err(error) => {
				tracing::error!(
					target: LOG_TARGET,
					?block_hash,
					error,
					"Failed to build the export segment; dropping the block.",
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
			prerequisite,
		};
		let anchored = Anchored {
			context,
			state_root: anchor_state_root,
			head_proof: anchor_state_proof,
			anchor_slot,
		};
		// The parent's segment is handed over inline with the child's bundle, and that is
		// load-bearing rather than an optimization. Guarantors normally fetch import segments
		// from DA, but a segment only reaches DA through its own package's availability phase —
		// which has not happened one tick after the parent was submitted, so segment 0 of the
		// parent package does not exist in DA at this moment. The submitter is the only party
		// that holds it (we built the parent block), so it travels in the bundle. No trust is
		// added: guarantors verify the segment's proof against the segment root the `Indirect`
		// import resolves to, which the chain itself authenticates through srlookup.
		let parent_import = parent_export
			.map(|export| ImportData { segment: export.segment.to_vec(), proof: export.proof });
		let package = source.package(&anchored);
		let (wp_hash, bundle) = build_encoded_bundle(
			&package,
			Vec::<Vec<u8>>::new(),
			&[parent_import.into_iter().collect::<Vec<_>>()],
		);

		tracing::info!(
			target: LOG_TARGET,
			?block_hash,
			%block_number,
			?wp_hash,
			core = self.core,
			anchor = ?anchored.context.anchor,
			anchor_slot,
			?prerequisite,
			import_spec = ?package.items[0].import_segments.first(),
			exported_header_hash = ?sp_crypto_hashing::blake2_256(&encoded_header),
			segroot = ?export.segroot,
			pov_len = package.items[0].payload.0.len(),
			bundle_len = bundle.len(),
			anchor_proof_nodes = anchored.head_proof.nodes.len(),
			"Assembled the work-package bundle for the block.",
		);

		self.submitted_blocks.insert(block_hash);
		(self.announce_block)(block_hash, None);

		let submitted_at = jam_slot_at(Timestamp::current());
		// The entry is recorded even if the submission itself failed. The builder has this block
		// in its segment either way, so dropping it here would put the two sides on different
		// chains; the soft-resubmit timer sends the very same bundle again instead, and the
		// resubmit budget is what eventually gives up on it.
		self.submit(wp_hash, &bundle, anchored.context.anchor, block_hash).await;
		self.chain.entries.push_back(InFlight {
			block_hash,
			block_number,
			parent_hash,
			wp_hash,
			bundle,
			export,
			source,
			anchored,
			submitted_at,
			reported: false,
			resubmits: 0,
		});
		self.log_chain_state("a package was submitted");
	}

	/// Submit a bundle and subscribe to its status; `false` means the submission itself failed.
	async fn submit(
		&mut self,
		wp_hash: WorkPackageHash,
		bundle: &[u8],
		anchor: HeaderHash,
		block_hash: Block::Hash,
	) -> bool {
		let started = Instant::now();
		let result = self.jam.submit_bundle(self.core, bundle.to_vec()).await;
		let elapsed_ms = started.elapsed().as_millis();
		if let Err(error) = result {
			tracing::warn!(
				target: LOG_TARGET,
				?block_hash,
				?wp_hash,
				bundle_len = bundle.len(),
				elapsed_ms,
				?error,
				"Bundle submission failed.",
			);
			return false;
		}
		tracing::info!(
			target: LOG_TARGET,
			?block_hash,
			?wp_hash,
			core = self.core,
			?anchor,
			bundle_len = bundle.len(),
			elapsed_ms,
			"Submitted the work-package bundle; following its status.",
		);

		match self.jam.work_package_status_stream(wp_hash, anchor, false).await {
			Ok(stream) => self.statuses.push(stream.map(move |status| (wp_hash, status)).boxed()),
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
		let Some(index) = self.chain.position_of_package(wp_hash) else {
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
			block_hash = ?self.chain.entries[index].block_hash,
			block_number = %self.chain.entries[index].block_number,
			index,
			chain_depth = self.chain.depth(),
			?status,
			?action,
			"Work-package status update.",
		);
		match action {
			PolicyAction::Wait => {},
			PolicyAction::Done => self.chain.entries[index].reported = true,
			PolicyAction::Resubmit => self.resubmit(index).await,
			PolicyAction::DropTail => self.handle_failure(index, &format!("{status:?}")).await,
		}
	}

	/// Once per JAM slot: give the policy a look at every package that has not been reported yet.
	async fn on_slot_tick(&mut self) {
		let now = jam_slot_at(Timestamp::current());
		let overdue: Vec<(usize, PolicyAction)> = self
			.chain
			.entries
			.iter()
			.enumerate()
			.filter(|(_, entry)| !entry.reported)
			.map(|(index, entry)| {
				let waiting = now.saturating_sub(entry.submitted_at);
				(index, self.policy.on_silence(waiting, entry.resubmits))
			})
			.filter(|(_, action)| !matches!(action, PolicyAction::Wait))
			.collect();

		for (index, action) in overdue {
			match action {
				PolicyAction::Resubmit => self.resubmit(index).await,
				// A drop takes the rest of the chain with it, so the indices collected above stop
				// meaning anything; the next tick looks at what is left.
				PolicyAction::DropTail => {
					self.handle_failure(index, "no report within the resubmit budget").await;
					break;
				},
				PolicyAction::Wait | PolicyAction::Done => {},
			}
		}
	}

	/// Send the very same bundle again.
	///
	/// Same bytes means the same work-package hash, so the submission is idempotent for JAM and —
	/// this is the point — every child's prerequisite and import still name a package that
	/// exists. Rebuilding anything would re-hash the package and orphan the whole tail.
	async fn resubmit(&mut self, index: usize) {
		let now = jam_slot_at(Timestamp::current());
		let entry = &mut self.chain.entries[index];
		entry.resubmits += 1;
		entry.submitted_at = now;
		let (wp_hash, bundle, anchor, block_hash, resubmits) = (
			entry.wp_hash,
			entry.bundle.clone(),
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
			chain_depth = self.chain.depth(),
			"No report yet; resubmitting the identical bundle.",
		);
		self.submit(wp_hash, &bundle, anchor, block_hash).await;
	}

	/// A package can no longer be reported.
	///
	/// The chain is cut at `index`: every descendant named this package in its prerequisite and
	/// in its import, so none of them can be reported either. Re-contexting the failed package
	/// instead would rewrite its bytes and therefore its hash, which is exactly what the
	/// descendants already committed to — so it is only allowed for a root package that has no
	/// descendants at all.
	async fn handle_failure(&mut self, index: usize, reason: &str) {
		let now = jam_slot_at(Timestamp::current());
		self.log_deadlines(index, now, reason);

		if index == 0 && self.chain.depth() == 1 && self.recontext_root().await {
			return;
		}

		let dropped = self.chain.drop_tail(index);
		tracing::warn!(
			target: LOG_TARGET,
			index,
			reason,
			?dropped,
			remaining = self.chain.depth(),
			"Dropping the tail of the in-flight chain.",
		);
		self.request_rebuild("a package in the chain can no longer be reported");
		self.log_chain_state("a tail was dropped");
	}

	/// Which of the two 8-block clocks ran out.
	fn log_deadlines(&self, index: usize, now: JamSlot, reason: &str) {
		let entry = &self.chain.entries[index];
		let anchor_age = now.saturating_sub(entry.anchored.anchor_slot);
		let dependency_age = index
			.checked_sub(1)
			.map(|parent| now.saturating_sub(self.chain.entries[parent].submitted_at));
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
			dependency_age = ?dependency_age,
			dependency_expired = ?dependency_age.map(|age| age > REPORT_DEADLINE_SLOTS),
			deadline_slots = REPORT_DEADLINE_SLOTS,
			submitted_at = entry.submitted_at,
			resubmits = entry.resubmits,
			"A work package failed; here is what each of the two deadlines had left.",
		);
	}

	/// The phase-4 heal, which survives only for a childless root package: rebuild it around a
	/// fresh anchor and a fresh para-head proof and submit it as a new package.
	async fn recontext_root(&mut self) -> bool {
		let entry = &self.chain.entries[0];
		let block_hash = entry.block_hash;
		let Ok(anchored) =
			recontext(&*self.jam, self.service_id, self.para_id, &entry.anchored, block_hash).await
		else {
			return false;
		};

		let entry = &self.chain.entries[0];
		let package = entry.source.package(&anchored);
		// A root package imports nothing, which is why re-contexting it is safe at all.
		let (wp_hash, bundle) =
			build_encoded_bundle(&package, Vec::<Vec<u8>>::new(), &[Vec::new()]);
		let anchor = anchored.context.anchor;
		tracing::info!(
			target: LOG_TARGET,
			?block_hash,
			old_wp_hash = ?entry.wp_hash,
			new_wp_hash = ?wp_hash,
			?anchor,
			anchor_slot = anchored.anchor_slot,
			bundle_len = bundle.len(),
			"Re-contexted the root package; it has no children whose links this could break.",
		);

		if !self.submit(wp_hash, &bundle, anchor, block_hash).await {
			return false;
		}
		let submitted_at = jam_slot_at(Timestamp::current());
		let entry = &mut self.chain.entries[0];
		entry.wp_hash = wp_hash;
		entry.bundle = bundle;
		entry.anchored = anchored;
		entry.submitted_at = submitted_at;
		entry.resubmits += 1;
		entry.reported = false;
		true
	}

	/// The para head advanced in JAM state.
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
		self.included_head = Some(hash);

		match self.chain.position_of_block(hash) {
			Some(index) => {
				let popped = self.chain.pop_through(index);
				let accumulated = self.remember_accumulated(popped);
				tracing::info!(
					target: LOG_TARGET,
					block_hash = ?hash,
					block_number = %header.number(),
					?accumulated,
					buffered = self.recently_accumulated.len(),
					remaining = self.chain.depth(),
					"Para head advanced in JAM state; our packages accumulated. Their packages \
					 stay linkable for a few more blocks: a block anchored while they were still \
					 in flight may be on its way here right now.",
				);
			},
			// The block our chain hangs off, accumulating exactly as it should. It is not one of
			// ours — we adopted its package from JAM's in-flight reports — but everything we
			// have in flight was waiting for precisely this and is still perfectly live.
			None if self.chain.root_parent() == Some(hash) => tracing::info!(
				target: LOG_TARGET,
				block_hash = ?hash,
				block_number = %header.number(),
				chain_depth = self.chain.depth(),
				"Para head advanced to the block our in-flight chain is rooted on.",
			),
			// Ours, but no longer tracked — a package we dropped the tail around still made it,
			// or the chain was reset under it. Nothing to heal: the builder is already authoring
			// from the accumulated head.
			None if self.submitted_blocks.contains(&hash) => tracing::info!(
				target: LOG_TARGET,
				block_hash = ?hash,
				block_number = %header.number(),
				chain_depth = self.chain.depth(),
				"Para head advanced to a block of ours that is no longer in the in-flight chain.",
			),
			None => {
				let dropped = self.chain.block_hashes();
				tracing::warn!(
					target: LOG_TARGET,
					block_hash = ?hash,
					block_number = %header.number(),
					?dropped,
					"Another collator's block won the para head; our whole in-flight chain is \
					 built on a head JAM did not take.",
				);
				self.reset_chain("another collator's block became the para head");
			},
		}
		self.log_chain_state("the para head advanced");
	}

	/// Move the whole chain into the recently-accumulated buffer, on the strength of the anchor
	/// having proved every block in it accumulated. Returns the drained blocks for logging.
	fn drain_chain_as_accumulated(&mut self) -> Vec<Block::Hash> {
		let Some(last) = self.chain.depth().checked_sub(1) else { return Vec::new() };
		let accumulated = self.chain.pop_through(last);
		self.remember_accumulated(accumulated)
	}

	/// Move packages JAM has accumulated out of the chain and into the buffer that keeps them
	/// linkable, returning their blocks for logging.
	fn remember_accumulated(&mut self, accumulated: Vec<InFlight<Block>>) -> Vec<Block::Hash> {
		accumulated
			.into_iter()
			.map(|entry| {
				self.recently_accumulated.remember(entry.block_hash, entry.wp_hash, entry.export);
				entry.block_hash
			})
			.collect()
	}

	/// Drop everything in flight and ask the builder to start again from the accumulated head.
	fn reset_chain(&mut self, why: &str) {
		let dropped = self.chain.clear();
		if !dropped.is_empty() {
			tracing::warn!(
				target: LOG_TARGET,
				why,
				?dropped,
				"Dropped the whole in-flight chain.",
			);
		}
		self.request_rebuild(why);
	}

	/// Since 5.2 the builder never re-authors a stalled head on its own, so this is the only
	/// thing that gets the collator building again after a package is lost.
	fn request_rebuild(&mut self, why: &str) {
		match self.rebuild_sender.try_send(()) {
			Ok(()) => tracing::info!(
				target: LOG_TARGET,
				why,
				"Asked the builder to reset its unincluded segment.",
			),
			Err(error) => tracing::warn!(
				target: LOG_TARGET,
				why,
				?error,
				"Unable to ask the builder for a segment reset.",
			),
		}
	}

	fn log_chain_state(&self, after: &str) {
		tracing::debug!(
			target: LOG_TARGET,
			after,
			depth = self.chain.depth(),
			tip = ?self.chain.tip().map(|entry| entry.block_hash),
			tip_wp_hash = ?self.chain.tip().map(|entry| entry.wp_hash),
			entries = ?self.chain.block_hashes(),
			included_head = ?self.included_head,
			recently_accumulated = self.recently_accumulated.len(),
			"In-flight chain state.",
		);
	}
}

fn hex_prefix(bytes: &[u8]) -> String {
	bytes.iter().take(32).map(|byte| format!("{byte:02x}")).collect()
}

/// The parts of a work package that survive a change of anchor: the built block(s), the
/// parachain storage proof witnessing them, the work-item settings, and the package this one
/// chains onto.
struct PackageSource<Block> {
	blocks: Vec<Block>,
	proof: CompactProof,
	validation_code_hash: [u8; 32],
	service_id: ServiceId,
	service_code_hash: CodeHash,
	refine_gas_limit: UnsignedGas,
	accumulate_gas_limit: UnsignedGas,
	/// The parent block's package, when the parent is still in flight. `None` for a package
	/// built on the head JAM has already accumulated.
	prerequisite: Option<WorkPackageHash>,
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

		// Same hash in both fields, two of the eight dependencies a package may have: the
		// prerequisite orders us behind the parent's package, the `Indirect` import is what
		// actually delivers the parent's header. `Indirect` rather than `Direct` because the
		// chain validates the work-package-hash-to-segment-root mapping, while a direct root is
		// an unauthenticated claim by whoever submitted the package.
		let import_segments = match self.prerequisite {
			Some(parent) => vec![ImportSpec { root: RootIdentifier::Indirect(parent), index: 0 }]
				.try_into()
				.expect("a single import spec always fits; qed"),
			None => Default::default(),
		};
		let work_item = WorkItem {
			service: self.service_id,
			code_hash: self.service_code_hash,
			payload: WorkPayload(payload),
			refine_gas_limit: self.refine_gas_limit,
			accumulate_gas_limit: self.accumulate_gas_limit,
			import_segments,
			extrinsics: Default::default(),
			// Every block's header is exported, root case included, so that a child always has
			// something to chain onto.
			export_count: 1,
		};

		let mut context = anchored.context.clone();
		context.prerequisites = self.prerequisite.into_iter().collect();
		WorkPackage {
			authorization: Authorization::default(),
			auth_code_host: 0,
			authorizer: fixed_authorizer(),
			context,
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

	fn package_source(prerequisite: Option<WorkPackageHash>) -> PackageSource<TestBlock> {
		PackageSource {
			blocks: vec![TestBlock::new(header(1, H256::repeat_byte(7)), vec![])],
			proof: CompactProof { encoded_nodes: vec![vec![1u8, 2, 3]] },
			validation_code_hash: [8u8; 32],
			service_id: 42,
			service_code_hash: CodeHash::from([9u8; 32]),
			refine_gas_limit: 1_000,
			accumulate_gas_limit: 1_000,
			prerequisite,
		}
	}

	/// A chain of `count` entries whose blocks form a parent/child line, package `k` hashed as
	/// `[k; 32]`, all submitted in slot `k`.
	fn test_chain(count: u8) -> InFlightChain<TestBlock> {
		test_chain_rooted(count, H256::repeat_byte(200))
	}

	/// The same chain, hanging off a named block — the shape an adopted segment has.
	fn test_chain_rooted(count: u8, root_parent: H256) -> InFlightChain<TestBlock> {
		let mut chain = InFlightChain::new();
		let mut parent_hash = root_parent;
		for index in 0..count {
			let block_header = header(u32::from(index) + 1, parent_hash);
			let block_hash = block_header.hash();
			chain.entries.push_back(InFlight {
				block_hash,
				block_number: *block_header.number(),
				parent_hash,
				wp_hash: wp_hash(index),
				bundle: vec![index],
				export: export_of(&block_header.encode()).expect("a header fits a segment"),
				source: package_source(index.checked_sub(1).map(wp_hash)),
				anchored: anchored(JamSlot::from(index)),
				submitted_at: JamSlot::from(index),
				reported: false,
				resubmits: 0,
			});
			parent_hash = block_hash;
		}
		chain
	}

	/// What the manager does with entries the chain pops as accumulated.
	fn remember(buffer: &mut RecentlyAccumulated<H256>, accumulated: Vec<InFlight<TestBlock>>) {
		for entry in accumulated {
			buffer.remember(entry.block_hash, entry.wp_hash, entry.export);
		}
	}

	/// The prerequisite a link turns into at the assembly site.
	fn prerequisite_of(link: &Link<H256>) -> Option<WorkPackageHash> {
		match link {
			Link::Chain(wp_hash) | Link::Adopt { wp_hash, .. } => Some(*wp_hash),
			Link::RecentlyAccumulated(parent) => Some(parent.wp_hash),
			Link::Root { .. } | Link::Mismatch { .. } => None,
		}
	}

	/// Nothing in flight and a block the builder authored on the accumulated head: the package
	/// stands on the proof-anchored root, with nothing to depend on and nothing to drain.
	#[test]
	fn a_block_on_the_accumulated_head_is_a_root_package() {
		assert_eq!(
			decide_included_link::<H256>(None, H256::repeat_byte(1)),
			Link::Root { drain_accumulated: false },
		);
	}

	/// The normal pipelined case: the block extends the tip, so the package chains onto the
	/// tip's package.
	#[test]
	fn a_block_on_the_chain_tip_chains_onto_its_package() {
		let tip_block = H256::repeat_byte(2);
		assert_eq!(
			decide_tip_link(
				Some((tip_block, wp_hash(7))),
				Some(H256::repeat_byte(1)),
				None,
				tip_block,
			),
			Link::Chain(wp_hash(7)),
		);
	}

	/// The builder authoring on a block that is in neither the chain nor the recently-accumulated
	/// buffer means the two sides disagree about the chain — after a drop-tail, say — and the
	/// package must not be submitted: a link to the wrong parent is a package the service will
	/// reject.
	#[test]
	fn a_block_on_neither_is_a_mismatch() {
		let tip_block = H256::repeat_byte(2);
		let head = H256::repeat_byte(1);
		let stranger = H256::repeat_byte(3);
		assert_eq!(
			decide_tip_link(Some((tip_block, wp_hash(7))), Some(head), None, stranger),
			Link::Mismatch { expected: Some(tip_block) },
		);
		assert_eq!(
			decide_tip_link(None, Some(head), None, stranger),
			Link::Mismatch { expected: Some(head) },
			"an empty chain and an empty buffer leave nothing the parent could link onto",
		);
	}

	/// The race this buffer exists for, as observed live: the builder anchored where the parent
	/// was still in flight and said `Tip`, but the parent's accumulation reached the para-head
	/// subscription first, so the chain is already empty when the block lands here. Rooting the
	/// package is the bug — its anchor proof shows the head *before* the parent, so refine finds
	/// a parent it never imported, zeroes the exports and takes every descendant down with it.
	/// The link has to stay chained onto the parent's package.
	#[test]
	fn a_parent_that_accumulated_in_transit_still_chains_onto_its_package() {
		let mut chain = test_chain(1);
		let parent = chain.entries[0].block_hash;
		let mut buffer = RecentlyAccumulated::new();
		remember(&mut buffer, chain.pop_through(0));

		let link = decide_tip_link(chain.tip_link(), Some(parent), buffer.parent(parent), parent);

		let Link::RecentlyAccumulated(accumulated) = &link else {
			panic!("the parent is in the buffer, so the link chains onto it: {link:?}");
		};
		assert_eq!(accumulated.wp_hash, wp_hash(0));
		assert_eq!(accumulated.blocks_ago, 0);
		assert_eq!(
			accumulated.export,
			export_of(&header(1, H256::repeat_byte(200)).encode()).unwrap(),
			"the parent's export travels inline with the child, exactly as from the chain",
		);

		let package = package_source(prerequisite_of(&link)).package(&anchored(11));
		assert_eq!(package.context.prerequisites.as_ref(), &[wp_hash(0)][..]);
		assert_eq!(
			package.items[0].import_segments.first(),
			Some(&ImportSpec { root: RootIdentifier::Indirect(wp_hash(0)), index: 0 }),
		);
	}

	/// The link is decided by the anchor the package carries, so the order the builder's message
	/// and the para-head notification happen to reach the manager in cannot change it. Both
	/// orders name the parent's package; only the source of the parent's export differs.
	#[test]
	fn the_link_is_the_same_whichever_event_reaches_the_manager_first() {
		let message_first = {
			let chain = test_chain(1);
			let parent = chain.entries[0].block_hash;
			decide_tip_link(chain.tip_link(), None, None, parent)
		};
		let pop_first = {
			let mut chain = test_chain(1);
			let parent = chain.entries[0].block_hash;
			let mut buffer = RecentlyAccumulated::new();
			remember(&mut buffer, chain.pop_through(0));
			decide_tip_link(chain.tip_link(), Some(parent), buffer.parent(parent), parent)
		};

		assert_eq!(message_first, Link::Chain(wp_hash(0)));
		assert_eq!(prerequisite_of(&message_first), Some(wp_hash(0)));
		assert_eq!(prerequisite_of(&pop_first), prerequisite_of(&message_first));
	}

	/// The mirror image of the same race: the anchor proves the parent is the accumulated head
	/// while this task is still holding the packages for it. The anchor is the authority, so
	/// they are drained here rather than waited on, and the package is the root the builder said
	/// it was.
	#[test]
	fn an_included_parent_the_chain_still_holds_drains_it_and_roots_the_package() {
		let mut chain = test_chain(3);
		let parent = chain.tip().expect("not empty").block_hash;

		assert_eq!(
			decide_included_link(chain.tip_link(), parent),
			Link::Root { drain_accumulated: true },
		);

		let mut buffer = RecentlyAccumulated::new();
		let last = chain.depth() - 1;
		remember(&mut buffer, chain.pop_through(last));
		assert_eq!(chain.depth(), 0);
		assert_eq!(buffer.parent(parent).map(|parent| parent.wp_hash), Some(wp_hash(2)));
	}

	/// A chain that still holds unaccumulated descendants of the parent is not behind the
	/// anchor — it disagrees with the builder, which has forgotten blocks this task is still
	/// carrying. Rooting a sibling next to them would fork our own chain, so this stays the
	/// mismatch that resynchronises both sides.
	#[test]
	fn an_included_parent_under_live_descendants_is_a_mismatch() {
		let chain = test_chain(2);
		let root_parent = chain.root_parent().expect("not empty");

		assert_eq!(
			decide_included_link(chain.tip_link(), root_parent),
			Link::Mismatch { expected: Some(chain.entries[1].block_hash) },
		);
	}

	/// The buffer covers the transit window between the builder's tick and this task, not the
	/// whole history, so it stays bounded and drops the oldest entries first.
	#[test]
	fn the_recently_accumulated_buffer_keeps_only_the_last_few_packages() {
		let mut chain = test_chain(RECENTLY_ACCUMULATED_CAP as u8 + 2);
		let oldest = chain.entries[0].block_hash;
		let newest = chain.tip().expect("not empty").block_hash;
		let mut buffer = RecentlyAccumulated::new();
		let last = chain.depth() - 1;
		remember(&mut buffer, chain.pop_through(last));

		assert_eq!(buffer.len(), RECENTLY_ACCUMULATED_CAP);
		assert_eq!(buffer.parent(oldest), None, "the oldest packages are evicted first");
		assert_eq!(buffer.parent(newest).map(|parent| parent.blocks_ago), Some(0));
	}

	/// After a restart, or when another collator authored the parent, the chain is empty and the
	/// only thing naming the parent's package is JAM's in-flight report. Adopting it is what lets
	/// the collator keep the pipeline going instead of re-rooting on the accumulated head.
	#[test]
	fn a_parent_known_only_from_a_report_starts_a_new_chain_segment() {
		let parent = H256::repeat_byte(4);
		let segroot = SegmentTreeRoot::from([5u8; 32]);

		assert_eq!(
			decide_reported_link(None, H256::repeat_byte(1), parent, wp_hash(6), segroot),
			Link::Adopt { wp_hash: wp_hash(6), segroot, drain_accumulated: false },
		);
	}

	/// A reported parent we submitted ourselves is not adopted: we know that package's hash
	/// first-hand, and going through the report would drop the entry the chain already holds.
	#[test]
	fn a_reported_parent_we_already_track_stays_on_the_chain_path() {
		let tip_block = H256::repeat_byte(2);
		let segroot = SegmentTreeRoot::from([5u8; 32]);

		assert_eq!(
			decide_reported_link(
				Some((tip_block, wp_hash(7))),
				H256::repeat_byte(1),
				tip_block,
				wp_hash(6),
				segroot,
			),
			Link::Chain(wp_hash(7)),
		);
		assert_eq!(
			decide_reported_link(
				Some((tip_block, wp_hash(7))),
				H256::repeat_byte(1),
				H256::repeat_byte(3),
				wp_hash(6),
				segroot,
			),
			Link::Mismatch { expected: Some(tip_block) },
			"a chain that ends somewhere else is the usual disagreement, not an adoption",
		);
	}

	/// The race observed live twice in one soak (2026-08-29 00:22:24 and 00:23:00): our own
	/// block is the head the anchor proves accumulated, another collator's successor of it is
	/// the in-flight tip the anchor's reports name, and the builder correctly builds on that
	/// successor — but its message reaches this task before the para-head notification for the
	/// pop does. The anchor is ahead of this task, not in disagreement with it, so the entry it
	/// proves accumulated is drained and the reported parent adopted. Deciding this from the
	/// arrival order instead dropped a perfectly good block and resynchronised both sides.
	#[test]
	fn a_reported_parent_above_our_just_accumulated_tip_drains_the_chain_and_adopts() {
		let mut chain = test_chain(1);
		let our_tip = chain.tip().expect("not empty").block_hash;
		let their_successor = header(2, our_tip).hash();
		let segroot = SegmentTreeRoot::from([5u8; 32]);

		assert_eq!(
			decide_reported_link(
				chain.tip_link(),
				our_tip,
				their_successor,
				wp_hash(6),
				segroot,
			),
			Link::Adopt { wp_hash: wp_hash(6), segroot, drain_accumulated: true },
		);

		let mut buffer = RecentlyAccumulated::new();
		let last = chain.depth() - 1;
		remember(&mut buffer, chain.pop_through(last));
		assert_eq!(chain.depth(), 0);
		assert_eq!(
			buffer.parent(our_tip).map(|parent| parent.wp_hash),
			Some(wp_hash(0)),
			"the drained package stays linkable, as it does on the para-head path",
		);
	}

	/// ...and the para-head notification for that same pop, arriving afterwards, must change
	/// nothing. The head is gone from the chain, it is not the block the adopted segment is
	/// rooted on, and it is one of ours — which is the branch `on_para_head` takes and the only
	/// harmless one. The foreign-head branch would reset the chain and throw away the package we
	/// just adopted onto the other collator's block.
	#[test]
	fn the_para_head_pop_after_a_drain_and_adopt_resets_nothing() {
		let mut chain = test_chain(1);
		let our_tip = chain.tip().expect("not empty").block_hash;
		let their_successor = header(2, our_tip).hash();
		let mut submitted: HashSet<H256> = chain.block_hashes().into_iter().collect();
		let mut buffer = RecentlyAccumulated::new();
		let last = chain.depth() - 1;
		remember(&mut buffer, chain.pop_through(last));

		let adopted = test_chain_rooted(1, their_successor);
		submitted.insert(adopted.tip().expect("not empty").block_hash);

		assert_eq!(adopted.position_of_block(our_tip), None, "the drain already removed it");
		assert_eq!(adopted.root_parent(), Some(their_successor));
		assert!(
			submitted.contains(&our_tip),
			"a drained head is still one of ours, so the foreign-head reset cannot fire",
		);
		assert!(buffer.parent(our_tip).is_some(), "and it is still linkable for a late child");
	}

	/// A chain the anchor's included head does *not* cover keeps the old behaviour: the entries
	/// above that head are live packages the anchor says nothing about, and starting an adopted
	/// segment beside them would fork our own chain. That is a real desync, and resynchronising
	/// is what resolves it.
	#[test]
	fn a_reported_parent_the_anchor_does_not_prove_us_past_is_still_a_mismatch() {
		let chain = test_chain(2);
		let tip = chain.tip().expect("not empty").block_hash;
		let root_parent = chain.root_parent().expect("not empty");
		let segroot = SegmentTreeRoot::from([5u8; 32]);

		assert_eq!(
			decide_reported_link(
				chain.tip_link(),
				root_parent,
				H256::repeat_byte(9),
				wp_hash(6),
				segroot,
			),
			Link::Mismatch { expected: Some(tip) },
		);
	}

	/// Adopting a package means trusting that the parent's header is what that package exported.
	/// Recomputing the export root from the header we hold and comparing it with the root the
	/// chain authenticated is the whole check, so it has to accept the real header...
	#[test]
	fn the_adopted_parents_export_is_accepted_when_it_reproduces_the_reported_root() {
		let parent_header = header(4, H256::repeat_byte(7));
		let export = export_of(&parent_header.encode()).expect("a header fits a segment");

		assert_eq!(adopted_parent_export(&parent_header, export.segroot), Ok(export));
	}

	/// ...and reject anything else. A mismatch means the byte contract and the on-chain reality
	/// disagree; the block is dropped because the import proof it would carry could not verify
	/// against the segment root the `Indirect` import resolves to.
	#[test]
	fn an_adopted_parents_export_that_misses_the_reported_root_is_rejected() {
		let parent_header = header(4, H256::repeat_byte(7));

		assert!(adopted_parent_export(&parent_header, SegmentTreeRoot::from([0xaa; 32])).is_err());
	}

	/// A chain adopted onto another collator's block is waiting for exactly that block to
	/// accumulate. Treating its arrival as a foreign head would drop packages that are still in
	/// flight and perfectly able to accumulate next.
	#[test]
	fn the_block_an_adopted_chain_is_rooted_on_is_not_a_foreign_head() {
		let chain = test_chain(2);
		let root_parent = chain.entries[0].parent_hash;

		assert_eq!(chain.position_of_block(root_parent), None);
		assert_eq!(chain.root_parent(), Some(root_parent));
	}

	/// A chained package carries both halves of the link, and the hash the bundle builder
	/// returns is the hash of the package encoding — the hash every child will name.
	#[test]
	fn a_chained_package_carries_the_prerequisite_and_the_import() {
		let parent = wp_hash(3);
		let source = package_source(Some(parent));
		let package = source.package(&anchored(11));

		assert_eq!(package.context.prerequisites.as_ref(), &[parent][..]);
		assert_eq!(
			package.items[0].import_segments.first(),
			Some(&ImportSpec { root: RootIdentifier::Indirect(parent), index: 0 }),
		);
		assert_eq!(package.items[0].import_segments.len(), 1);
		assert_eq!(package.items[0].export_count, 1, "every block exports its header");

		let parent_export = export_of(&header(1, H256::repeat_byte(7)).encode()).unwrap();
		let (hash, bundle) = build_encoded_bundle(
			&package,
			Vec::<Vec<u8>>::new(),
			&[vec![ImportData {
				segment: parent_export.segment.to_vec(),
				proof: parent_export.proof.clone(),
			}]],
		);

		assert_eq!(
			hash,
			WorkPackageHash::from(sp_crypto_hashing::blake2_256(
				&jam_codec::Encode::encode(&package)
			)),
		);
		assert!(
			bundle
				.windows(parent_export.segment.len())
				.any(|window| window == &parent_export.segment[..]),
			"the parent's segment travels inside the bundle; guarantors cannot fetch it from DA \
			 yet",
		);
	}

	/// A root package has neither half of the link and imports nothing.
	#[test]
	fn a_root_package_has_no_prerequisite_and_no_import() {
		let package = package_source(None).package(&anchored(11));

		assert!(package.context.prerequisites.as_ref().is_empty());
		assert!(package.items[0].import_segments.is_empty());
		assert_eq!(package.items[0].export_count, 1, "the root exports its header too");
	}

	/// The para head lands on an entry: that entry and everything older accumulated with it —
	/// JAM applies a chain in order, so an older package cannot still be pending.
	#[test]
	fn accumulating_a_block_pops_it_and_everything_older() {
		let mut chain = test_chain(4);
		let third = chain.entries[2].block_hash;

		let index = chain.position_of_block(third).expect("the block is in flight");
		let popped = chain.pop_through(index);

		assert_eq!(popped.len(), 3);
		assert_eq!(chain.depth(), 1);
		assert_eq!(chain.tip().map(|entry| entry.wp_hash), Some(wp_hash(3)));
	}

	/// The tip accumulating empties the chain.
	#[test]
	fn accumulating_the_tip_empties_the_chain() {
		let mut chain = test_chain(3);
		let tip = chain.tip().expect("not empty").block_hash;

		let index = chain.position_of_block(tip).expect("the block is in flight");

		assert_eq!(chain.pop_through(index).len(), 3);
		assert_eq!(chain.depth(), 0);
	}

	/// A head nobody in the chain authored is another collator's; the whole chain is built on a
	/// head JAM did not take, so none of it can ever accumulate.
	#[test]
	fn a_foreign_head_matches_no_entry() {
		let chain = test_chain(3);
		assert_eq!(chain.position_of_block(H256::repeat_byte(123)), None);
	}

	/// A failure cuts the chain at the failed package: its descendants named it in their
	/// prerequisite and their import, so they die with it, while its ancestors are untouched.
	#[test]
	fn dropping_a_tail_removes_the_failed_package_and_its_descendants_only() {
		let mut chain = test_chain(4);
		let index = chain.position_of_package(wp_hash(2)).expect("the package is in flight");

		let dropped = chain.drop_tail(index);

		assert_eq!(dropped.len(), 2);
		assert_eq!(chain.depth(), 2);
		assert_eq!(chain.tip().map(|entry| entry.wp_hash), Some(wp_hash(1)));
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
