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
//! para-head stream, the status subscriptions of every submitted package, and the JAM best-block
//! stream. Every new JAM block is one look at β (the recent-blocks history): a package named
//! there is reported, and one that has been away too long is resent or forgotten against the
//! anchor deadline.
//!
//! Each block the builder hands over becomes **one work package**: no imported segment,
//! `export_count = 0`, submitted with a plain `submitWorkPackage`. A package names the work
//! package of the block it builds on as its single prerequisite, when this node still remembers
//! that package's hash; a miss starts a new chain. Phase 5a removed the in-core *import* link
//! between a block's package and its parent's, because an import authenticates bytes to a
//! *package*, not to a service, and so never carried the security it appeared to; lineage is
//! declared in the work output (the parent head hash refine derives from the block) and settled
//! by the parachain service at accumulate, which applies a head only if it chains onto the
//! stored one and buffers the rest.
//!
//! What a package still carries is the anchor state proof of the para head, inside the PoV.
//! Each block also carries a `JamParent` digest that names the anchor it was built against;
//! that digest is baked in at authoring and verified at refine. A package re-signed against a
//! *different* anchor hash has a stale digest and can never validate.
//!
//! **Resubmission policy**: at every new JAM block β says whether a package appeared on chain.
//! If it did, the package is reported; if the anchor has expired (the block is past the package's
//! report window) it is forgotten; otherwise, once it has been away for
//! [`RESUBMIT_AFTER_SLOTS`], the byte-identical package is sent again. There is no re-anchoring:
//! a package re-signed against a fresh anchor is rejected by the PVF's `JamParent` digest check,
//! so a package that outlives its anchor is dropped and the builder's stall re-root recovers.
//!
//! Failure handling is per package and cascades: a package that is forgotten takes every package
//! that named it, so no child outlives the prerequisite it named. The block itself stays in the
//! local database, and the next parachain slot authors on whatever is deepest there.
//!
//! Every package runs under the para's own [AURA authorizer](super::authorizer) and carries a
//! token this collator signs with its aura key, so assembling a package and signing it are one
//! step here.
//!
//! Phase-1 simplification that still stands: the PoV is NOT zstd-compressed (parasim rejects
//! compressed PoVs; JIP-2 is silent on compression).

use super::{
	authorizer::AuraAuthorizer, hash_ledger::WpHashLedger, para_head_stream, resubmission::*,
	AuthoringHold, JamCollatorMessage, LOG_TARGET,
};
use crate::common::{
	types::{ParachainBackend, ParachainClient},
	ConstructNodeRuntimeApi, NodeBlock,
};
use codec::{Decode, Encode};
use cumulus_primitives_core::{ParachainBlockData, SchedulingProof};
use futures::{
	channel::mpsc,
	future::AbortHandle,
	stream::{abortable, SelectAll},
	StreamExt,
};
use jam_interface::{
	BlockDesc, BlockInfo, BoxStream, CoreIndex, HeaderHash, JamChainSource, JamStateSource,
	JamWorkPackageSubmission, RecentBlocks, ServiceId, Slot as JamSlot, VersionedParameters,
	WorkPackage, WorkPackageHash, WorkPackageStatus,
};
use jam_types::{
	Authorization, CodeHash, ExtrinsicSpec, MapLike, RefineContext, UnsignedGas, VecSet, WorkItem,
	WorkPayload,
};
use parachain_service_core::{authorizer::Authorizer, candidate::ParachainCandidate};
use polkadot_primitives::Id as ParaId;
use sc_client_api::backend::AuxStore;
use sc_client_db::DbHash;
use sp_additional_data::AdditionalData;
use sp_runtime::traits::{Block as BlockT, HashingFor, Header as HeaderT};
use sp_trie::CompactProof;
use std::{
	collections::{HashMap, VecDeque},
	sync::Arc,
	time::{Duration, Instant},
};

const RETRY_DELAY: Duration = Duration::from_secs(6);

pub(crate) struct CollationTaskParams<Block: NodeBlock, RuntimeApi, Jam> {
	pub para_client: Arc<ParachainClient<Block, RuntimeApi>>,
	pub para_backend: Arc<ParachainBackend<Block>>,
	pub jam: Arc<Jam>,
	pub para_id: ParaId,
	pub service_id: ServiceId,
	pub authorizer: Arc<AuraAuthorizer>,
	pub message_receiver: mpsc::Receiver<JamCollatorMessage<Block>>,
	pub announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	/// Shared with the builder: how many overdue packages the collation task is holding.
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
		hold,
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
		refine_gas_limit,
		accumulate_gas_limit,
		resubmit_after_slots = RESUBMIT_AFTER_SLOTS,
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
		service_id,
		authorizer,
		service_code_hash,
		refine_gas_limit,
		accumulate_gas_limit,
		policy: ResendUntilAnchorExpires,
		announce_block,
		hash_ledger,
		hold,
		packages: InFlightPackages::new(),
		included_head: None,
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
				let Some(head) = head else {
					tracing::error!(target: LOG_TARGET, "Para-head stream ended; stopping.");
					return;
				};
				manager.on_para_head(&head);
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
	/// The package exactly as submitted. A resend replays it verbatim, which is what keeps the
	/// hash JAM knows it by — and therefore the status subscription — unchanged.
	package: WorkPackage,
	/// The PoV, sent as work-item extrinsic 0 and replayed byte-identically with `package` on a
	/// resend. `Arc` keeps the MBs from being copied per entry.
	pov: Arc<[u8]>,
	anchored: Anchored,
	/// The JAM tip slot the package was last sent against: the zero of the resend clock.
	submitted_at: JamSlot,
	reported: bool,
	/// `submit()` calls that returned `true`: real hand-offs to a guarantor.
	sends: u32,
	/// Times the policy said `Resend`, whether or not the hand-off itself succeeded again.
	resends: u32,
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

	/// Packages that were handed to a guarantor at least once, resent at least once, and not yet
	/// reported: the ones the builder must not author past.
	fn overdue(&self) -> usize {
		self.entries
			.iter()
			.filter(|entry| !entry.reported && entry.sends > 0 && entry.resends > 0)
			.count()
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
	/// A resubmission subscribes again for the same package hash; the earlier subscription is
	/// closed here, so a package holds exactly one however often it is resent.
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

/// The JAM block in β whose guarantees name `wp_hash`, if any.
fn appeared_in(history: &RecentBlocks, wp_hash: WorkPackageHash) -> Option<&BlockInfo> {
	history
		.history
		.iter()
		.find(|block| MapLike::contains_key(&block.reported, &wp_hash))
}

/// What to do with each unreported package when a JAM block arrives: the pure heart of
/// [`Manager::on_jam_block`], so the sorting between reported, resent, expired and waiting
/// packages is unit-testable without a chain.
fn plan_on_jam_block<Block: BlockT>(
	packages: &InFlightPackages<Block>,
	tip_slot: JamSlot,
	history: &RecentBlocks,
	policy: &ResendUntilAnchorExpires,
) -> Vec<(WorkPackageHash, PolicyAction, Option<BlockDesc>)> {
	packages
		.entries
		.iter()
		.filter(|entry| !entry.reported)
		.map(|entry| {
			let appeared = appeared_in(history, entry.wp_hash);
			let action = policy.on_jam_block(Observed {
				tip_slot,
				submitted_at: entry.submitted_at,
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
	service_id: ServiceId,
	/// The para's AURA authorizer: what every package here runs under, and what signs it.
	authorizer: Arc<AuraAuthorizer>,
	service_code_hash: CodeHash,
	refine_gas_limit: UnsignedGas,
	accumulate_gas_limit: UnsignedGas,
	policy: ResendUntilAnchorExpires,
	/// Shared with the builder; refreshed by [`Manager::log_state`] on every mutation.
	hold: AuthoringHold,
	announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	/// Which work package this node submitted for which block. Signing is non-deterministic, so
	/// a package's hash can only be remembered, never recomputed — not even by its author.
	hash_ledger: WpHashLedger<ParachainClient<Block, RuntimeApi>>,
	packages: InFlightPackages<Block>,
	/// The para head last seen in JAM state, for the log alone; `None` until the stream reports
	/// one. No decision reads it: it is a strictly later observation than the anchor a package
	/// carries, and letting it override the anchor is exactly the class of race phase 5 spent
	/// three fixes on.
	included_head: Option<Block::Hash>,
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
			self.service_id,
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
			service_id: self.service_id,
			service_code_hash: self.service_code_hash,
			refine_gas_limit: self.refine_gas_limit,
			accumulate_gas_limit: self.accumulate_gas_limit,
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

		// The clock starts at the JAM tip slot the package is sent against, not the wall clock:
		// the resend decision is made when the next JAM block arrives, and it is that chain's
		// slots the deadline is counted in.
		let submitted_at = triggered_by.slot;
		// The entry is recorded even if the submission itself failed: a resend can only repeat a
		// package this task still holds, and the next JAM block re-decides it.
		let sent = self.submit(wp_hash, &package, &pov, &anchored, block_hash).await;
		self.packages.entries.push_back(InFlight {
			block_hash,
			block_number,
			parent_hash,
			wp_hash,
			package,
			pov: pov.into(),
			anchored,
			submitted_at,
			reported: false,
			sends: u32::from(sent),
			resends: 0,
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
	/// The PoV is sent as work-item extrinsic 0, not in the package payload: CE 133 caps the first
	/// message (core index plus package, payloads included) at 200 KiB, while extrinsics ride the
	/// bulk channel, bounded only by `max_input`. With no core holding the para's authorizer there
	/// is nowhere to send it — a guarantor on any other core would refuse it — so it is kept, but
	/// no resend can reach a guarantor until a core holds the para's authorizer again.
	async fn submit(
		&mut self,
		wp_hash: WorkPackageHash,
		package: &WorkPackage,
		pov: &[u8],
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
				 submitted. It stays in flight, but no resend can reach a guarantor until a core \
				 holds this para's authorizer again.",
			);
			return false;
		};

		let started = Instant::now();
		let result = self.jam.submit_work_package(core, package, vec![pov.to_vec()]).await;
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
			PolicyAction::Wait | PolicyAction::Resend => {},
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
		if self.packages.entries.is_empty() {
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
				PolicyAction::Resend => self.resend(index, tip.slot).await,
				PolicyAction::Wait => {},
			}
		}
		self.log_state("a JAM block arrived");
	}

	/// Send the very same package again, against the JAM tip slot that triggered it.
	///
	/// Same bytes means the same work-package hash, so JAM sees one package however often it is
	/// repeated and the status subscription this task already holds keeps naming it.
	async fn resend(&mut self, index: usize, tip_slot: JamSlot) {
		let entry = &mut self.packages.entries[index];
		entry.submitted_at = tip_slot;
		entry.resends += 1;
		let (wp_hash, package, pov, block_hash, block_number, core, sends, resends, anchor_slot) = (
			entry.wp_hash,
			entry.package.clone(),
			entry.pov.clone(),
			entry.block_hash,
			entry.block_number,
			entry.anchored.submit_target,
			entry.sends,
			entry.resends,
			entry.anchored.anchor_slot,
		);
		let anchored = entry.anchored.clone();
		tracing::info!(
			target: LOG_TARGET,
			wp_hash = ?wp_hash,
			block_hash = ?block_hash,
			block_number = %block_number,
			core = ?core,
			tip_slot,
			anchor_slot,
			sends,
			resends,
			"Resending the identical work package; it has not appeared on chain.",
		);
		if self.submit(wp_hash, &package, &pov, &anchored, block_hash).await {
			self.packages.entries[index].sends += 1;
		}
	}

	/// Give up on a package and, transitively, on every package that named it.
	///
	/// The block itself stays in the local database, so the next parachain slot simply authors
	/// on whatever is deepest there. What this does cost is the parachain's progress until
	/// somebody resubmits the missing package: descendants of the lost block sit in the
	/// service's reorder buffer until the buffer evicts them. Resubmission by another collator
	/// is phase-7 work, so this logs loudly enough to be the thing a stalled parachain is
	/// diagnosed from.
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
				resends = entry.resends,
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
		let overdue = self.packages.overdue();
		self.hold.set_overdue(overdue);
		tracing::debug!(
			target: LOG_TARGET,
			after,
			in_flight = self.packages.len(),
			overdue,
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
	service_id: ServiceId,
	service_code_hash: CodeHash,
	refine_gas_limit: UnsignedGas,
	accumulate_gas_limit: UnsignedGas,
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
	/// here, and fixed for the package's life: a resend replays the same anchor.
	submit_target: Option<CoreIndex>,
}

impl<Block: BlockT> PackageSource<Block> {
	/// Assemble the work package for `anchored`, still unauthorized, and hand back the PoV it
	/// carries.
	///
	/// The PoV travels as work-item extrinsic 0, not in the payload: CE 133 caps the first message
	/// (core index plus package, payloads included) at 200 KiB, while extrinsics ride the bulk
	/// channel, bounded only by `max_input`. The payload's `ParachainCandidate` keeps its
	/// `validation_code_hash` — the parachain service still reads that — with an empty `pov`.
	///
	/// The token cannot be built here: it signs a hash of the finished package, so authorizing is
	/// the step after this one ([`AuraAuthorizer::authorize`]).
	fn package(&self, anchored: &Anchored) -> (WorkPackage, Vec<u8>) {
		let pov = build_pov(&self.blocks, &self.proof, &self.parent_header, &self.additional_data);
		let payload = ParachainCandidate {
			validation_code_hash: parachain_service_core::types::ValidationCodeHash(
				self.validation_code_hash.into(),
			),
			pov: Vec::new(),
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
			extrinsics: vec![ExtrinsicSpec {
				hash: jam_std_common::hash_raw(&pov).into(),
				len: pov.len() as u32,
			}]
			.try_into()
			.expect("a single extrinsic always fits; qed"),
			export_count: 0,
		};

		// The parachain service hosts its own authorizer blob, so it is also the service
		// guarantors look that blob's preimage up in.
		let package = WorkPackage {
			authorization: Authorization::default(),
			auth_code_host: self.service_id,
			authorizer: self.authorizer.clone(),
			context: anchored.context.clone(),
			items: vec![work_item].try_into().expect("a single work item always fits; qed"),
		};
		(package, pov)
	}
}

/// The PoV: a V4 [`ParachainBlockData`] carrying the SCALE-encoded parent header of `blocks[0]`
/// and the additional-data map assembled at build time.
///
/// The scheduling proof is empty — JAM has no relay-chain scheduling. The PoV is not
/// zstd-compressed; JIP-2 is silent on compression and the service refuses compressed PoVs.
fn build_pov<Block: BlockT>(
	blocks: &[Block],
	proof: &CompactProof,
	parent_header: &Block::Header,
	additional_data: &AdditionalData,
) -> Vec<u8> {
	ParachainBlockData::new_with_parent_header(
		blocks.to_vec(),
		proof.clone(),
		SchedulingProof::empty(),
		blocks.iter().map(|_| Some(additional_data.clone())).collect(),
		parent_header.encode(),
	)
	.encode()
}

#[cfg(test)]
mod tests {
	use super::{
		super::{authorizer::tests::authorizer_of, hash_ledger::test_support::ledger},
		*,
	};
	use codec::DecodeAll;
	use cumulus_jam_state_reader::JAM_PROOF_KEY;
	use cumulus_test_runtime::{Block as TestBlock, Header as TestHeader};
	use jam_std_common::{build_encoded_bundle, BlockInfo, Mmr, RecentBlocks};
	use jam_types::{BoundedVec, RecentBlockCount, SegmentTreeRoot, VecMap};
	use parachain_authorizer::aura::{signable_work_package_hash, AuthToken as GuestToken};
	use parachain_authorizer_sr25519::Sr25519;
	use parachain_service_core::StateProof;
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
			service_id: 42,
			service_code_hash: CodeHash::from([9u8; 32]),
			refine_gas_limit: 1_000,
			accumulate_gas_limit: 1_000,
			authorizer: aura().authorizer(),
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
			let (package, pov) =
				signed_with_pov(&package_source(), &anchored(JamSlot::from(index)));
			packages.entries.push_back(InFlight {
				block_hash,
				block_number: *block_header.number(),
				parent_hash,
				wp_hash: wp_hash(index),
				package,
				pov: pov.into(),
				anchored: anchored(JamSlot::from(index)),
				submitted_at: JamSlot::from(index),
				reported: false,
				sends: 1,
				resends: 0,
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
	/// is past its deadline is forgotten, one away for the resend window is resent, and the rest
	/// wait.
	#[test]
	fn a_jam_block_sorts_reported_resent_expired_and_waiting() {
		let tip = 100;
		let mut packages = in_flight(4);
		// 0: named in β.
		packages.entries[0].anchored.anchor_slot = tip;
		packages.entries[0].submitted_at = tip;
		// 1: missing, but due for a resend.
		packages.entries[1].anchored.anchor_slot = tip;
		packages.entries[1].submitted_at = tip - RESUBMIT_AFTER_SLOTS;
		// 2: missing and its anchor has expired.
		packages.entries[2].anchored.anchor_slot = tip - REPORT_DEADLINE_SLOTS;
		packages.entries[2].submitted_at = tip - REPORT_DEADLINE_SLOTS;
		// 3: missing, but only just sent.
		packages.entries[3].anchored.anchor_slot = tip;
		packages.entries[3].submitted_at = tip;

		let history = recent_blocks(&[wp_hash(0)]);
		let plan = plan_on_jam_block(&packages, tip, &history, &ResendUntilAnchorExpires);
		let actions: Vec<_> = plan.iter().map(|(hash, action, _)| (*hash, *action)).collect();

		assert_eq!(
			actions,
			vec![
				(wp_hash(0), PolicyAction::Reported),
				(wp_hash(1), PolicyAction::Resend),
				(wp_hash(2), PolicyAction::Forget),
				(wp_hash(3), PolicyAction::Wait),
			],
		);
		assert_eq!(plan[0].2.map(|block| block.slot), Some(0), "the β block travels with it");
	}

	/// A package already known to be reported is filtered out before the clocks are even
	/// consulted: nothing resends or forgets a package JAM has already accepted.
	#[test]
	fn a_reported_package_is_left_alone_by_the_clock() {
		let tip = 100;
		let mut packages = in_flight(2);
		packages.entries[0].reported = true;
		packages.entries[0].anchored.anchor_slot = tip - REPORT_DEADLINE_SLOTS;
		packages.entries[0].submitted_at = tip - REPORT_DEADLINE_SLOTS;
		packages.entries[1].anchored.anchor_slot = tip;
		packages.entries[1].submitted_at = tip;

		let plan =
			plan_on_jam_block(&packages, tip, &recent_blocks(&[]), &ResendUntilAnchorExpires);

		assert_eq!(plan.len(), 1, "the reported package is not re-decided");
		assert_eq!(plan[0].0, wp_hash(1));
		assert_eq!(plan[0].1, PolicyAction::Wait);
	}

	/// Overdue is narrower than "in flight": it needs a real hand-off (`sends > 0`) and at least
	/// one resend decision, and it stops as soon as the package is reported.
	#[test]
	fn only_resent_sent_unreported_packages_are_overdue() {
		let mut packages = in_flight(4);
		packages.entries[0].sends = 1;
		packages.entries[0].resends = 0;
		packages.entries[1].sends = 1;
		packages.entries[1].resends = 1;
		packages.entries[2].sends = 0;
		packages.entries[2].resends = 2;
		packages.entries[3].sends = 1;
		packages.entries[3].resends = 1;
		packages.entries[3].reported = true;

		assert_eq!(packages.overdue(), 1, "only package 1 is overdue");
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

	/// The PoV no longer rides in the payload: `ParachainCandidate.pov` is empty, and the package's
	/// one extrinsic spec names the PoV's hash and length.
	#[test]
	fn the_pov_travels_as_the_only_extrinsic() {
		let (package, pov) = signed_with_pov(&package_source(), &anchored(11));

		let candidate =
			<ParachainCandidate as Decode>::decode(&mut &package.items[0].payload.0[..])
				.expect("the payload is a ParachainCandidate");
		assert!(candidate.pov.is_empty(), "the payload no longer carries the PoV");

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

	/// A resend has to be the *same bytes*: a package rebuilt instead of replayed would hash
	/// differently, and JAM would see a second package where the collator meant to repeat one — a
	/// second refine, a second report, and a status subscription following a hash nothing else
	/// knows about. Rebuilding is not even close to equivalent: everything the source determines
	/// comes back identical, but the token does not, because an sr25519 signature carries a
	/// random nonce. Storing the signed package is the only way to repeat it.
	#[test]
	fn a_resubmission_replays_the_stored_package() {
		let packages = in_flight(1);
		let entry = &packages.entries[0];
		let (rebuilt, rebuilt_pov) = signed_with_pov(&package_source(), &entry.anchored);

		assert_eq!(entry.package.items[0].payload.0, rebuilt.items[0].payload.0, "same block");
		assert_eq!(
			entry.package.items[0].extrinsics[0].hash, rebuilt.items[0].extrinsics[0].hash,
			"the same PoV hash",
		);
		assert_eq!(
			entry.package.items[0].extrinsics[0].len, rebuilt.items[0].extrinsics[0].len,
			"the same PoV length",
		);
		assert_eq!(entry.pov.as_ref(), rebuilt_pov.as_slice(), "the stored PoV is replayed");
		assert_eq!(entry.package.context, rebuilt.context, "same anchor");
		assert_ne!(entry.package.authorization, rebuilt.authorization, "another signature");
		assert_ne!(work_package_hash(&entry.package), work_package_hash(&rebuilt));
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
	/// with it, and a resend must not add a second one.
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
	}
}
