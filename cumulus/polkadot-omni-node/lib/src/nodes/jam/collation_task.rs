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
//! chain ([`decide_link`]): the first one anchors on the head JAM has accumulated, every later
//! one names its parent's package as a prerequisite *and* imports that package's segment 0 (the
//! parent's header), which is how the service learns in-core which unaccumulated block this one
//! builds on. The package goes out as a bundle carrying that segment inline.
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
	ANCHOR_STATE_PROOF_KEY, JAM_SLOT_DURATION_MS, JamCollatorMessage, LOG_TARGET,
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
	Authorization, CodeHash, ImportSpec, RefineContext, RootIdentifier, UnsignedGas, WorkItem,
	WorkPayload,
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

	// The chain starts empty. Rebuilding what is already in flight — after a restart, or when
	// another collator's packages are the ones in the air — comes from JAM's availability
	// assignments and ready queue in 5.4; this is the seam it plugs into.
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

	fn position_of_block(&self, block_hash: Block::Hash) -> Option<usize> {
		self.entries.iter().position(|entry| entry.block_hash == block_hash)
	}

	fn position_of_package(&self, wp_hash: WorkPackageHash) -> Option<usize> {
		self.entries.iter().position(|entry| entry.wp_hash == wp_hash)
	}

	/// Everything up to and including `index` has been accumulated; drop it and return what was
	/// dropped.
	fn pop_through(&mut self, index: usize) -> Vec<Block::Hash> {
		self.entries.drain(..=index).map(|entry| entry.block_hash).collect()
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

/// How a new block's package links into the chain of in-flight packages.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Link<Hash> {
	/// Nothing in flight and the block builds on the accumulated head (or on genesis): no
	/// prerequisite, nothing imported.
	Root,
	/// The block's parent is the chain tip's block: name the tip's package as the prerequisite
	/// and import its exported header.
	Chain(WorkPackageHash),
	/// The block builds on something this task is not tracking. Builder and manager disagree
	/// about the chain, which is a bug on one of the two sides.
	Mismatch { expected: Option<Hash> },
}

/// Decide the link from the chain tip alone — the only thing a new block may extend.
///
/// With an empty chain the block must build on the head JAM has accumulated. The comparison is
/// skipped while that head is unknown (nothing has been observed on the para-head stream yet),
/// because the builder proved the very same head against its anchor before authoring; once it is
/// known, a block naming anything else is a block the builder authored on a chain this task has
/// already abandoned.
fn decide_link<Hash: Copy + PartialEq>(
	tip: Option<(Hash, WorkPackageHash)>,
	included_head: Option<Hash>,
	parent_hash: Hash,
) -> Link<Hash> {
	match tip {
		Some((tip_block, tip_package)) if tip_block == parent_hash => Link::Chain(tip_package),
		Some((tip_block, _)) => Link::Mismatch { expected: Some(tip_block) },
		None => match included_head {
			Some(head) if head != parent_hash => Link::Mismatch { expected: Some(head) },
			_ => Link::Root,
		},
	}
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
			anchor_slot,
			triggered_by,
		} = message;
		let block_hash = block.hash();
		let block_number = *block.header().number();
		let parent_hash = parent_header.hash();

		let link = decide_link(self.chain.tip_link(), self.included_head, parent_hash);
		tracing::debug!(
			target: LOG_TARGET,
			?block_hash,
			%block_number,
			?parent_hash,
			?parent_link,
			included_head = ?self.included_head,
			chain_depth = self.chain.depth(),
			chain_tip = ?self.chain.tip_link().map(|(block, _)| block),
			?link,
			?triggered_by,
			"Linking a new block into the in-flight chain.",
		);
		let prerequisite = match link {
			Link::Root => None,
			Link::Chain(parent_package) => Some(parent_package),
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
		let parent_import = prerequisite
			.and_then(|_| self.chain.tip())
			.map(|parent| ImportData {
				segment: parent.export.segment.to_vec(),
				proof: parent.export.proof.clone(),
			});
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
				let accumulated = self.chain.pop_through(index);
				tracing::info!(
					target: LOG_TARGET,
					block_hash = ?hash,
					block_number = %header.number(),
					?accumulated,
					"Para head advanced in JAM state; our packages accumulated.",
				);
			},
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
		let mut chain = InFlightChain::new();
		let mut parent_hash = H256::repeat_byte(200);
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

	/// Nothing in flight and a block on the accumulated head: the package stands on the
	/// proof-anchored root, with nothing to depend on.
	#[test]
	fn a_block_on_the_accumulated_head_is_a_root_package() {
		let head = H256::repeat_byte(1);
		assert_eq!(decide_link(None, Some(head), head), Link::Root);
		assert_eq!(decide_link(None, None, head), Link::Root, "before any head is known");
	}

	/// The normal pipelined case: the block extends the tip, so the package chains onto the
	/// tip's package.
	#[test]
	fn a_block_on_the_chain_tip_chains_onto_its_package() {
		let tip_block = H256::repeat_byte(2);
		assert_eq!(
			decide_link(Some((tip_block, wp_hash(7))), Some(H256::repeat_byte(1)), tip_block),
			Link::Chain(wp_hash(7)),
		);
	}

	/// The builder authoring on anything else means the two sides disagree about the chain —
	/// after a drop-tail, say — and the package must not be submitted: a link to the wrong
	/// parent is a package the service will reject.
	#[test]
	fn a_block_on_neither_is_a_mismatch() {
		let tip_block = H256::repeat_byte(2);
		let head = H256::repeat_byte(1);
		let stranger = H256::repeat_byte(3);
		assert_eq!(
			decide_link(Some((tip_block, wp_hash(7))), Some(head), stranger),
			Link::Mismatch { expected: Some(tip_block) },
		);
		assert_eq!(
			decide_link(None, Some(head), stranger),
			Link::Mismatch { expected: Some(head) },
		);
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
