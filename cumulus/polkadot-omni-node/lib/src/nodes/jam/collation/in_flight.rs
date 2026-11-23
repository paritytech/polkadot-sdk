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

//! The packages this collator has in flight, their status subscriptions, and when a held package
//! goes out.

use crate::{
	common::{types::ParachainClient, ConstructNodeRuntimeApi, NodeBlock},
	nodes::jam::{
		collation::{
			manager::Manager,
			policy::{ForgetWhenAnchorExpires, Observed, PolicyAction, ResubmissionPolicy},
		},
		package::source::Anchored,
		store::hash_ledger::WpHashLedger,
		LOG_TARGET,
	},
};
use futures::{future::AbortHandle, stream::abortable, StreamExt};
use jam_interface::{
	BlockDesc, BlockInfo, BoxStream, CoreIndex, JamChainSource, JamStateSource,
	JamWorkPackageSubmission, RecentBlocks, Slot as JamSlot, WorkPackage, WorkPackageHash,
	WorkPackageStatus,
};
use jam_types::MapLike;
use sc_client_api::backend::AuxStore;
use sc_client_db::DbHash;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use std::{
	collections::{HashMap, HashSet, VecDeque},
	time::Instant,
};

/// One work package this collator submitted, still in flight.
pub(crate) struct InFlight<Block: BlockT> {
	pub(crate) block_hash: Block::Hash,
	pub(crate) block_number: <Block::Header as HeaderT>::Number,
	pub(crate) parent_hash: Block::Hash,
	pub(crate) wp_hash: WorkPackageHash,
	/// The package exactly as submitted. Its prerequisite is what a forget cascades along.
	pub(crate) package: WorkPackage,
	pub(crate) anchored: Anchored,
	pub(crate) reported: bool,
	/// Whether this package was created by a resubmission round rather than by the builder.
	pub(crate) resubmission: bool,
	/// `submit()` calls that returned `true`: real hand-offs to a guarantor.
	pub(crate) sends: u32,
	/// The PoV of a package pacing has not released yet; `None` once it was handed to `submit()`.
	pub(crate) unsent: Option<Vec<u8>>,
}

/// The work packages this collator has in flight, in the order they were submitted.
///
/// A plain list, not a chain: a package's prerequisite link lives in its own context, and
/// nothing here walks it.
pub(crate) struct InFlightPackages<Block: BlockT> {
	pub(crate) entries: VecDeque<InFlight<Block>>,
}

impl<Block: BlockT> InFlightPackages<Block> {
	pub(crate) fn new() -> Self {
		Self { entries: VecDeque::new() }
	}

	pub(crate) fn len(&self) -> usize {
		self.entries.len()
	}

	pub(crate) fn position_of_package(&self, wp_hash: WorkPackageHash) -> Option<usize> {
		self.entries.iter().position(|entry| entry.wp_hash == wp_hash)
	}

	pub(crate) fn remove(&mut self, index: usize) -> InFlight<Block> {
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
	pub(crate) fn remove_up_to(
		&mut self,
		number: <Block::Header as HeaderT>::Number,
	) -> Vec<InFlight<Block>> {
		let (settled, remaining): (Vec<_>, Vec<_>) =
			self.entries.drain(..).partition(|entry| entry.block_number <= number);
		self.entries = remaining.into();
		settled
	}

	pub(crate) fn block_hashes(&self) -> Vec<Block::Hash> {
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
pub(crate) struct StatusSubscriptions {
	handles: HashMap<WorkPackageHash, AbortHandle>,
}

impl StatusSubscriptions {
	pub(crate) fn new() -> Self {
		Self { handles: HashMap::new() }
	}

	/// Wrap a package's status stream so it can be ended on demand.
	///
	/// Following a package that is already followed closes the earlier subscription, so a package
	/// never holds two.
	pub(crate) fn follow(
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
	pub(crate) fn close(&mut self, wp_hash: WorkPackageHash) -> bool {
		match self.handles.remove(&wp_hash) {
			Some(handle) => {
				handle.abort();
				true
			},
			None => false,
		}
	}

	pub(crate) fn len(&self) -> usize {
		self.handles.len()
	}
}

/// The JAM block in β whose guarantees name `wp_hash`, if any.
pub(crate) fn appeared_in(history: &RecentBlocks, wp_hash: WorkPackageHash) -> Option<&BlockInfo> {
	history
		.history
		.iter()
		.find(|block| MapLike::contains_key(&block.reported, &wp_hash))
}

/// What to do with each unreported package when a JAM block arrives: the pure heart of
/// [`Manager::on_jam_block`](super::manager::Manager::on_jam_block), so the sorting between
/// reported, expired and waiting packages is unit-testable without a chain.
pub(crate) fn plan_on_jam_block<Block: BlockT>(
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

/// The held packages to send at `tip_slot`, in submission order.
///
/// A core whose last package went out at `tip_slot` or later (`sent_at`) takes none, and within
/// one call a core takes at most one: the first held package for it, so a core's packages leave in
/// the order they were built. A package whose prerequisite is still held waits behind it, still
/// claiming its core. A package with no core is always due: `submit()` only logs that it has
/// nowhere to go.
pub(crate) fn due_for_sending<Block: BlockT>(
	packages: &InFlightPackages<Block>,
	tip_slot: JamSlot,
	sent_at: &HashMap<CoreIndex, JamSlot>,
) -> Vec<WorkPackageHash> {
	let mut taken: HashSet<CoreIndex> = sent_at
		.iter()
		.filter(|(_, slot)| **slot >= tip_slot)
		.map(|(core, _)| *core)
		.collect();
	let mut due = Vec::new();
	for entry in packages.entries.iter().filter(|entry| entry.unsent.is_some()) {
		let prerequisite_held =
			entry.package.context.prerequisites.as_ref().iter().any(|wp_hash| {
				!due.contains(wp_hash) &&
					packages
						.entries
						.iter()
						.any(|other| other.wp_hash == *wp_hash && other.unsent.is_some())
			});
		match entry.anchored.submit_target {
			None => due.push(entry.wp_hash),
			Some(core) => {
				if taken.insert(core) && !prerequisite_held {
					due.push(entry.wp_hash);
				}
			},
		}
	}
	due
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
pub(crate) fn forget_package<Block: BlockT<Hash = DbHash>, C: AuxStore>(
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

impl<Block, RuntimeApi, Jam> Manager<Block, RuntimeApi, Jam>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	Jam: JamChainSource + JamStateSource + JamWorkPackageSubmission + 'static,
{
	/// Hand every package [`due_for_sending`] releases at the current tip to `submit()`.
	pub(crate) async fn send_due(&mut self) {
		for wp_hash in due_for_sending(&self.packages, self.tip_slot, &self.sent_at) {
			let Some(index) = self.packages.position_of_package(wp_hash) else { continue };
			let entry = &mut self.packages.entries[index];
			let Some(pov) = entry.unsent.take() else { continue };
			let (package, anchored, block_hash) =
				(entry.package.clone(), entry.anchored.clone(), entry.block_hash);
			if let Some(core) = anchored.submit_target {
				self.sent_at.insert(core, self.tip_slot);
			}
			let sent = self.submit(wp_hash, &package, pov, &anchored, block_hash).await;
			if let Some(index) = self.packages.position_of_package(wp_hash) {
				self.packages.entries[index].sends += u32::from(sent);
			}
		}
		let held = self.packages.entries.iter().filter(|entry| entry.unsent.is_some()).count();
		if held > 0 {
			tracing::info!(
				target: LOG_TARGET,
				held,
				tip_slot = self.tip_slot,
				"Holding work packages for a later JAM slot: their core already got one at this \
				 slot.",
			);
		}
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
	pub(crate) async fn submit(
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
				authorizer_hash =
					?parachain_service_core::authorizer::authorizer_hash(&package.authorizer),
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
}
