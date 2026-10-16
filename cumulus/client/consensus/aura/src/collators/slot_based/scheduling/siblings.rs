// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! Sibling scheduling parents.
//!
//! The scheduling parent (SP) comes from the relay best-block stream, but a relay fork at its
//! height can make a sibling the block validators build on, dropping every segment advertised
//! under ours. A sibling is any imported relay head validators would still accept as SP for the
//! production slot (see `is_scheduling_parent_valid` in the collator protocol).
//!
//! After a segment's main submission the collation task asks [`SiblingResolver`] for the siblings
//! sharing its internal scheduling parent and cores, and submits the segment again under each
//! one's header chain. The signed scheduling info commits to the internal scheduling parent only,
//! so it stays valid.

use crate::{collators::RelayHeader, LOG_TARGET};
use cumulus_primitives_aura::Slot;
use cumulus_primitives_core::SchedulingProof;
use cumulus_relay_chain_interface::RelayChainInterface;
use futures::{
	stream::{Fuse, FusedStream},
	FutureExt, Stream, StreamExt,
};
use polkadot_node_subsystem_util::runtime::ClaimQueueSnapshot;
use polkadot_primitives::{
	Block as RelayBlock, BlockId, CoreIndex, Hash as RelayHash, Id as ParaId,
};
use sp_runtime::traits::Header as HeaderT;
use sp_timestamp::Timestamp;
use std::{
	collections::{BTreeMap, HashMap, HashSet},
	pin::Pin,
	time::{Duration, Instant},
};

/// Most sibling SPs one segment is also submitted under. Each costs a whole extra submission, and
/// the SP plus its siblings must stay inside the network bridge's `MAX_VIEW_HEADS`.
pub(crate) const MAX_SIBLING_SCHEDULING_PARENTS: usize = 2;

/// Rejected candidates tolerated per segment before giving up, bounding the relay lookups a
/// forked height can force.
const MAX_REJECTED_SIBLINGS: usize = 2;

/// BABE slots kept in [`RecentImports`]; enough for the slot before the production slot, the
/// production slot, and parents a few slots back.
const RECENT_SLOTS: usize = 4;

/// Heads kept per slot, so a fork-spammed slot cannot grow the dedup scan.
const MAX_IMPORTS_PER_SLOT: usize = 8;

/// Time kept free before the production slot ends for a sibling rebuild to reach validators;
/// assumes rebuilding and advertising one segment takes well under it.
const SIBLING_SUBMISSION_MARGIN: Duration = Duration::from_secs(1);

/// A relay header's BABE claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Claim {
	pub slot: Slot,
	/// A primary claim outweighs a secondary one in BABE fork choice.
	pub is_primary: bool,
}

impl Claim {
	/// `None` if the header carries no decodable BABE pre-digest.
	pub(crate) fn of(header: &RelayHeader) -> Option<Self> {
		let pre_digest = sc_consensus_babe::find_pre_digest::<RelayBlock>(header).ok()?;
		Some(Self {
			slot: pre_digest.slot(),
			is_primary: matches!(pre_digest, sc_consensus_babe::PreDigest::Primary(_)),
		})
	}
}

/// An imported relay head; hash and claim are decoded once, on arrival.
struct ImportedHeader {
	hash: RelayHash,
	header: RelayHeader,
	claim: Claim,
}

/// Recently imported relay heads by BABE slot, including those that never became best.
#[derive(Default)]
struct RecentImports {
	by_slot: BTreeMap<Slot, Vec<ImportedHeader>>,
}

impl RecentImports {
	/// Record `header`; one without a BABE claim, or older than every kept slot, is ignored.
	fn insert(&mut self, header: RelayHeader) {
		let Some(claim) = Claim::of(&header) else { return };
		let full = self.by_slot.len() >= RECENT_SLOTS;
		if full && self.by_slot.first_key_value().is_some_and(|(oldest, _)| claim.slot < *oldest) {
			return;
		}

		let hash = header.hash();
		let at_slot = self.by_slot.entry(claim.slot).or_default();
		if at_slot.iter().any(|known| known.hash == hash) {
			return;
		}
		if at_slot.len() >= MAX_IMPORTS_PER_SLOT {
			tracing::debug!(
				target: LOG_TARGET,
				slot = ?claim.slot,
				?hash,
				"Per-slot import cap reached; skipping relay header."
			);
			return;
		}
		at_slot.push(ImportedHeader { hash, header, claim });

		while self.by_slot.len() > RECENT_SLOTS {
			self.by_slot.pop_first();
		}
	}

	fn header(&self, hash: &RelayHash) -> Option<&RelayHeader> {
		self.by_slot
			.values()
			.flatten()
			.find(|entry| entry.hash == *hash)
			.map(|entry| &entry.header)
	}

	/// Sibling SP candidates of `chosen` for `production_slot`, best first.
	///
	/// Mirrors `is_scheduling_parent_valid`: a head from the slot before `production_slot` (leaf
	/// case), or the parent of a head claiming `production_slot` (non-leaf case). A secondary
	/// whose parent also has a primary child loses BABE fork choice, so it is dropped unless
	/// extended.
	fn siblings_of(&self, chosen: &RelayHeader, production_slot: Slot) -> Vec<RelayHeader> {
		let Some(chosen_claim) = Claim::of(chosen) else { return Vec::new() };
		let chosen_hash = chosen.hash();
		let leaf_slot = (*production_slot).checked_sub(1).map(Slot::from);
		let entries = || self.by_slot.values().flatten();
		let extended: HashSet<RelayHash> = self
			.by_slot
			.get(&production_slot)
			.into_iter()
			.flatten()
			.map(|entry| *entry.header.parent_hash())
			.collect();
		let mut primary_parents: HashSet<RelayHash> = entries()
			.filter(|entry| entry.claim.is_primary)
			.map(|entry| *entry.header.parent_hash())
			.collect();
		if chosen_claim.is_primary {
			primary_parents.insert(*chosen.parent_hash());
		}
		let is_extended = |entry: &ImportedHeader| extended.contains(&entry.hash);
		let outweighed = |entry: &ImportedHeader| {
			!entry.claim.is_primary &&
				!is_extended(entry) &&
				primary_parents.contains(entry.header.parent_hash())
		};

		let mut siblings: Vec<&ImportedHeader> = entries()
			.filter(|entry| entry.hash != chosen_hash)
			// Only a head at the chosen height reaches the internal scheduling parent.
			.filter(|entry| entry.header.number == chosen.number)
			.filter(|entry| Some(entry.claim.slot) == leaf_slot || is_extended(entry))
			.filter(|entry| !outweighed(entry))
			.collect();
		siblings.sort_by_key(|entry| (!is_extended(entry), !entry.claim.is_primary, entry.hash));
		siblings.into_iter().map(|entry| entry.header.clone()).collect()
	}
}

/// When submissions under a sibling of `slot`'s SP stop being started: the slot's wall-clock end,
/// after which validators reject the sibling, less [`SIBLING_SUBMISSION_MARGIN`].
pub(crate) fn sibling_deadline(slot: Slot, slot_duration: Duration) -> Instant {
	sibling_deadline_at(slot, slot_duration, Timestamp::current().as_duration(), Instant::now())
}

/// [`sibling_deadline`] with the wall clock and monotonic clock read by the caller.
fn sibling_deadline_at(slot: Slot, slot_duration: Duration, now: Duration, at: Instant) -> Instant {
	let end_ms = (*slot).saturating_add(1).saturating_mul(slot_duration.as_millis() as u64);
	let left = Duration::from_millis(end_ms).saturating_sub(now);
	at + left.saturating_sub(SIBLING_SUBMISSION_MARGIN)
}

/// The slot inputs for finding a segment's sibling SPs.
#[derive(Clone)]
pub(crate) struct SiblingParams {
	pub production_slot: Slot,
	/// Cores the slot was planned on; a sibling assigning others cannot carry the segment.
	pub core_indices: Vec<CoreIndex>,
	pub claim_queue_offset: u32,
	pub max_relay_parent_session_age: u32,
	/// From [`sibling_deadline`].
	pub valid_until: Instant,
}

/// Finds a segment's sibling SPs from the relay heads it has seen imported.
pub(crate) struct SiblingResolver<RelayClient> {
	relay_client: RelayClient,
	para_id: ParaId,
	/// All imported heads, including siblings that never become best.
	imports: Fuse<Pin<Box<dyn Stream<Item = RelayHeader> + Send>>>,
	recent: RecentImports,
	memo: SlotMemo,
}

/// Lookups for one production slot.
#[derive(Default)]
struct SlotMemo {
	slot: Slot,
	/// Claim queues per sibling.
	claim_queues: HashMap<RelayHash, ClaimQueueSnapshot>,
	/// Accepted chains per chosen SP, shared by every core of the slot.
	chains: HashMap<RelayHash, Vec<Vec<RelayHeader>>>,
}

impl<RelayClient: RelayChainInterface + 'static> SiblingResolver<RelayClient> {
	/// Unsubscribed until [`Self::ensure_subscribed`].
	pub(crate) fn new(relay_client: RelayClient, para_id: ParaId) -> Self {
		let stream: Pin<Box<dyn Stream<Item = RelayHeader> + Send>> =
			Box::pin(futures::stream::empty());
		let mut imports = stream.fuse();
		imports.next().now_or_never();
		Self {
			relay_client,
			para_id,
			imports,
			recent: Default::default(),
			memo: Default::default(),
		}
	}

	/// A resolver reading imports from `imports`.
	#[cfg(test)]
	pub(crate) fn with_imports(
		relay_client: RelayClient,
		para_id: ParaId,
		imports: Pin<Box<dyn Stream<Item = RelayHeader> + Send>>,
	) -> Self {
		Self { imports: imports.fuse(), ..Self::new(relay_client, para_id) }
	}

	/// How many imported heads are recorded.
	#[cfg(test)]
	pub(crate) fn recorded(&self) -> usize {
		self.recent.by_slot.values().map(Vec::len).sum()
	}

	/// Subscribe to relay imports if not subscribed, or if the subscription ended.
	pub(crate) async fn ensure_subscribed(&mut self) {
		if !self.imports.is_terminated() {
			return;
		}
		match self.relay_client.import_notification_stream().await {
			Ok(imports) => self.imports = imports.fuse(),
			Err(err) => tracing::warn!(
				target: LOG_TARGET,
				?err,
				"Failed to subscribe to relay imports; sibling scheduling parents are missed.",
			),
		}
	}

	/// Record the next imported head; pending forever while unsubscribed.
	pub(crate) async fn absorb_next(&mut self) {
		match self.imports.next().await {
			Some(header) => self.recent.insert(header),
			None => futures::future::pending().await,
		}
	}

	/// Record every import already waiting. Never blocks.
	pub(crate) fn drain(&mut self) {
		while let Some(Some(header)) = self.imports.next().now_or_never() {
			self.recent.insert(header);
		}
	}

	/// Header chains of up to `limit` siblings that can carry the segment proved by `proof`, best
	/// first. Each chain is newest first, as [`SchedulingProof::header_chain`]. Empty at
	/// `relay_parent_offset == 0`, where there is no chain to swap, or once `params.valid_until`
	/// passes, which bounds the relay calls.
	///
	/// Resolved once per slot and SP, at the first core's call, so chained cores share one sibling
	/// set (a prefix of it, per `limit`). A sibling imported or extended after that is missed.
	pub(crate) async fn resolve(
		&mut self,
		proof: &SchedulingProof,
		params: &SiblingParams,
		limit: usize,
	) -> Vec<Vec<RelayHeader>> {
		let deadline = tokio::time::Instant::from_std(params.valid_until);
		tokio::time::timeout_at(deadline, self.resolve_unbounded(proof, params, limit))
			.await
			.unwrap_or_default()
	}

	async fn resolve_unbounded(
		&mut self,
		proof: &SchedulingProof,
		params: &SiblingParams,
		limit: usize,
	) -> Vec<Vec<RelayHeader>> {
		let limit = limit.min(MAX_SIBLING_SCHEDULING_PARENTS);
		let Some(chosen) = proof.header_chain.first().filter(|_| limit > 0) else {
			return Vec::new();
		};

		if self.memo.slot != params.production_slot {
			self.memo = SlotMemo { slot: params.production_slot, ..Default::default() };
		}
		let chosen_hash = chosen.hash();
		if let Some(chains) = self.memo.chains.get(&chosen_hash) {
			return chains.iter().take(limit).cloned().collect();
		}

		self.ensure_subscribed().await;
		self.drain();
		let candidates = self.recent.siblings_of(chosen, params.production_slot);
		let max = MAX_SIBLING_SCHEDULING_PARENTS;
		let mut chains = Vec::new();
		for sibling in candidates.iter().take(max + MAX_REJECTED_SIBLINGS) {
			if chains.len() >= max {
				break;
			}
			chains.extend(self.resolve_sibling(proof, params, sibling.clone()).await);
		}

		let accepted: Vec<_> = chains.iter().filter_map(|c| c.first()).map(|h| h.hash()).collect();
		tracing::debug!(
			target: LOG_TARGET,
			scheduling_parent = ?proof.scheduling_parent(),
			candidates = candidates.len(),
			?accepted,
			"Sibling scheduling parents resolved.",
		);
		self.memo.chains.insert(chosen_hash, chains.clone());
		chains.truncate(limit);
		chains
	}

	/// `Some(header_chain)` if `sibling` walks back to `proof`'s internal scheduling parent within
	/// the session age limit and assigns the planned cores.
	async fn resolve_sibling(
		&mut self,
		proof: &SchedulingProof,
		params: &SiblingParams,
		sibling: RelayHeader,
	) -> Option<Vec<RelayHeader>> {
		let sibling_hash = sibling.hash();
		let reject = |reason: &str| {
			tracing::debug!(target: LOG_TARGET, ?sibling_hash, reason, "Sibling SP rejected.");
			None
		};

		let internal_scheduling_parent = proof.internal_scheduling_parent_header.hash();
		let offset = proof.header_chain.len();
		let mut header_chain = Vec::with_capacity(offset);
		// Bounds the fresh relay parent only: a chain crossing more epoch changes than the chosen
		// one can push a resubmitted part past the limit, failing that rebuild.
		let mut session_age = 0;
		let mut current = sibling;
		loop {
			if sc_consensus_babe::contains_epoch_change::<RelayBlock>(&current) {
				session_age += 1;
			}
			if session_age > params.max_relay_parent_session_age {
				return reject("session age");
			}
			let parent = *current.parent_hash();
			header_chain.push(current);
			if header_chain.len() == offset {
				break;
			}
			current = match self.header(parent).await {
				Some(header) => header,
				None => return reject("missing header"),
			};
		}

		// A sibling forked below the internal scheduling parent cannot carry the executed blocks.
		let parent = header_chain.last().map(|header| *header.parent_hash());
		if parent != Some(internal_scheduling_parent) {
			return reject("internal scheduling parent");
		}

		// Unlike the builder, an epoch change on the extending child is accepted, as validators do.
		// A sibling assigning other cores would map the core selector elsewhere.
		if !self.assigns_cores(sibling_hash, params).await {
			return reject("cores");
		}
		Some(header_chain)
	}

	/// Whether the planned cores lead the para's claims at `sibling`. A failed lookup is not
	/// cached.
	async fn assigns_cores(&mut self, sibling: RelayHash, params: &SiblingParams) -> bool {
		if !self.memo.claim_queues.contains_key(&sibling) {
			match self.relay_client.claim_queue(sibling).await {
				Ok(claim_queue) => {
					self.memo.claim_queues.insert(sibling, claim_queue.into());
				},
				Err(err) => {
					tracing::debug!(target: LOG_TARGET, ?sibling, ?err, "No claim queue.");
					return false;
				},
			}
		}
		let Some(claim_queue) = self.memo.claim_queues.get(&sibling) else { return false };
		let cores: Vec<_> = claim_queue
			.iter_claims_at_depth_for_para(params.claim_queue_offset as usize, self.para_id)
			.collect();
		cores.starts_with(&params.core_indices)
	}

	async fn header(&mut self, hash: RelayHash) -> Option<RelayHeader> {
		if let Some(header) = self.recent.header(&hash) {
			return Some(header.clone());
		}
		self.relay_client.header(BlockId::Hash(hash)).await.ok().flatten()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::collators::slot_based::tests::{
		babe_epoch_change_digest_item, relay_header_primary_with_slot, relay_header_with_slot,
		TestRelayClient,
	};

	/// `header` with a distinct state root: a same-slot twin on the same parent.
	fn twin(header: &RelayHeader, tag: u8) -> RelayHeader {
		let mut twin = header.clone();
		twin.state_root = [tag; 32].into();
		twin
	}

	fn imports(headers: &[&RelayHeader]) -> RecentImports {
		let mut imports = RecentImports::default();
		for header in headers {
			imports.insert((*header).clone());
		}
		imports
	}

	/// Deduplicated by hash, bounded in slots and heads per slot; a head older than every kept
	/// slot, or without a BABE claim, is never recorded.
	#[test]
	fn recent_imports_dedup_and_stay_bounded() {
		let mut imports = RecentImports::default();
		for slot in 1..=(RECENT_SLOTS as u64 + 1) {
			let header = relay_header_with_slot(slot as u32, Default::default(), slot);
			imports.insert(header.clone());
			imports.insert(header);
		}
		assert_eq!(imports.by_slot.len(), RECENT_SLOTS);
		assert!(!imports.by_slot.contains_key(&Slot::from(1)));
		assert!(imports.by_slot.values().all(|at_slot| at_slot.len() == 1));

		imports.insert(relay_header_with_slot(1, Default::default(), 1));
		assert!(!imports.by_slot.contains_key(&Slot::from(1)));

		let mut no_claim = relay_header_with_slot(9, Default::default(), 9);
		no_claim.digest.logs.clear();
		imports.insert(no_claim);
		assert!(!imports.by_slot.contains_key(&Slot::from(9)));

		let top = relay_header_with_slot(5, Default::default(), 5);
		for tag in 0..(MAX_IMPORTS_PER_SLOT as u8 + 5) {
			imports.insert(twin(&top, tag));
		}
		assert_eq!(imports.by_slot[&Slot::from(5)].len(), MAX_IMPORTS_PER_SLOT);
	}

	/// Candidates as `is_scheduling_parent_valid` admits them: a head from `production_slot - 1`,
	/// or the parent of a head at `production_slot`. A secondary next to a primary is dropped
	/// unless extended; extended, then primary, then hash order.
	#[test]
	fn siblings_mirror_scheduling_parent_validity() {
		let production_slot = Slot::from(10);
		let leaf_slot = 9;

		let primary = relay_header_primary_with_slot(7, Default::default(), leaf_slot);
		let secondary = relay_header_with_slot(7, Default::default(), leaf_slot);
		let other_primary = twin(&primary, 9);
		let other_secondary = twin(&secondary, 11);
		let mut imports = imports(&[&primary, &secondary, &other_primary, &other_secondary]);

		// Both secondaries lose to the primaries on their parent, whichever is picked.
		let mut primaries = vec![primary.clone(), other_primary.clone()];
		primaries.sort_by_key(|h| h.hash());
		assert_eq!(imports.siblings_of(&primary, production_slot), vec![other_primary.clone()]);
		assert_eq!(imports.siblings_of(&secondary, production_slot), primaries);

		// Neither a leaf at `production_slot - 1` nor extended.
		imports.insert(relay_header_with_slot(7, Default::default(), 0));
		assert_eq!(imports.siblings_of(&primary, production_slot), vec![other_primary.clone()]);

		// A child at `production_slot` lifts a losing secondary over the weight filter, first.
		imports.insert(relay_header_with_slot(8, secondary.hash(), *production_slot));
		let expected = vec![secondary.clone(), other_primary.clone()];
		assert_eq!(imports.siblings_of(&primary, production_slot), expected);

		// A child of the pick itself changes nothing.
		imports.insert(relay_header_with_slot(8, primary.hash(), *production_slot));
		assert_eq!(imports.siblings_of(&primary, production_slot), expected);

		// No primary on its parent, so it is kept; it sorts last.
		let cross_fork = relay_header_with_slot(7, [3u8; 32].into(), leaf_slot);
		imports.insert(cross_fork.clone());
		let expected = vec![secondary.clone(), other_primary.clone(), cross_fork.clone()];
		assert_eq!(imports.siblings_of(&primary, production_slot), expected);

		// An older head is admitted once a child at `production_slot` is imported.
		let off_slot = relay_header_primary_with_slot(7, Default::default(), 3);
		imports.insert(off_slot.clone());
		assert!(!imports.siblings_of(&primary, production_slot).contains(&off_slot));
		imports.insert(relay_header_with_slot(8, off_slot.hash(), *production_slot));
		let mut expected = expected;
		expected.insert(0, off_slot);
		assert_eq!(imports.siblings_of(&primary, production_slot), expected);

		// A child at another slot does not extend its parent.
		let stale = relay_header_primary_with_slot(7, [5u8; 32].into(), 4);
		imports.insert(stale.clone());
		imports.insert(relay_header_with_slot(8, stale.hash(), 6));
		assert!(!imports.siblings_of(&primary, production_slot).contains(&stale));
	}

	#[test]
	fn sibling_deadline_keeps_a_margin_before_slot_end() {
		let slot_duration = Duration::from_secs(6);
		let at = Instant::now();
		let now = Duration::from_millis(100 * 6_000 + 2_000);
		let deadline = |slot| sibling_deadline_at(Slot::from(slot), slot_duration, now, at);
		assert_eq!(deadline(99), at);
		assert_eq!(deadline(100), at + Duration::from_secs(4) - SIBLING_SUBMISSION_MARGIN);
		assert_eq!(deadline(101), at + Duration::from_secs(10) - SIBLING_SUBMISSION_MARGIN);
	}

	/// Internal scheduling parent `isp` at height 10 and slot 100, with the planned slot 102 on
	/// core 0.
	struct Fixture {
		headers: Vec<RelayHeader>,
		cores: Vec<(RelayHash, CoreIndex)>,
		isp: RelayHeader,
		max_session_age: u32,
	}

	impl Fixture {
		fn new() -> Self {
			let isp = relay_header_with_slot(10, Default::default(), 100);
			Self { headers: vec![isp.clone()], cores: vec![], isp, max_session_age: 0 }
		}

		/// Make `header` known to the relay client, assigning `core` (if any) to the para.
		fn add(&mut self, header: &RelayHeader, core: Option<CoreIndex>) {
			self.headers.push(header.clone());
			self.cores.extend(core.map(|core| (header.hash(), core)));
		}

		/// A proof with `chain` (newest first) over the fixture's internal scheduling parent.
		fn proof(&self, chain: Vec<RelayHeader>) -> SchedulingProof {
			SchedulingProof::new(chain, self.isp.clone(), None)
		}

		/// Resolve `proof`'s siblings, with `imports` seen imported first.
		async fn resolve(
			&self,
			proof: &SchedulingProof,
			imports: &[&RelayHeader],
			limit: usize,
		) -> Vec<Vec<RelayHeader>> {
			let mut client =
				TestRelayClient::new(self.headers.iter().map(|h| (h.hash(), h.clone())).collect());
			// Production reads the claims at the relay parent offset.
			let depth = proof.header_chain.len();
			for (hash, core) in &self.cores {
				client.add_claim(*hash, depth, *core);
			}
			let (tx, rx) = futures::channel::mpsc::unbounded();
			let mut resolver = SiblingResolver::with_imports(client, 1.into(), Box::pin(rx));
			for header in imports {
				tx.unbounded_send((*header).clone()).expect("receiver is alive; qed");
			}
			let params = SiblingParams {
				production_slot: Slot::from(102),
				core_indices: vec![CoreIndex(0)],
				claim_queue_offset: depth as u32,
				max_relay_parent_session_age: self.max_session_age,
				valid_until: Instant::now() + Duration::from_secs(60),
			};
			resolver.resolve(proof, &params, limit).await
		}
	}

	/// Only a sibling on the same cores and internal scheduling parent is kept, at most `limit`,
	/// and none at `relay_parent_offset == 0`.
	#[tokio::test]
	async fn sibling_chains_filter_and_cap() {
		let mut fx = Fixture::new();
		let chosen = relay_header_with_slot(11, fx.isp.hash(), 101);
		let sibling = twin(&chosen, 7);
		let other_core = twin(&chosen, 8);
		let other_base = relay_header_with_slot(10, [1u8; 32].into(), 100);
		let forked = relay_header_with_slot(11, other_base.hash(), 101);
		fx.add(&sibling, Some(CoreIndex(0)));
		fx.add(&other_core, Some(CoreIndex(1)));
		fx.add(&forked, Some(CoreIndex(0)));
		let proof = fx.proof(vec![chosen.clone()]);

		let all = [&chosen, &other_core, &forked, &sibling];
		assert_eq!(fx.resolve(&proof, &all, 2).await, vec![vec![sibling.clone()]]);
		assert!(fx.resolve(&fx.proof(vec![]), &all, 2).await.is_empty());
		assert!(fx.resolve(&proof, &all, 0).await.is_empty());

		let accepted: Vec<_> = (20..23).map(|tag| twin(&chosen, tag)).collect();
		for header in &accepted {
			fx.add(header, Some(CoreIndex(0)));
		}
		let mut expected = accepted.clone();
		expected.sort_by_key(|h| h.hash());
		let expected: Vec<_> = expected
			.into_iter()
			.take(MAX_SIBLING_SCHEDULING_PARENTS)
			.map(|h| vec![h])
			.collect();
		let imports: Vec<_> = accepted.iter().collect();
		assert_eq!(fx.resolve(&proof, &imports, 5).await, expected);
	}

	/// At offset 2 the chain is newest first and ends above the internal scheduling parent; a
	/// sibling forked above it, on another parent, is kept too.
	#[tokio::test]
	async fn sibling_chains_at_offset_two() {
		let mut fx = Fixture::new();
		let mid = relay_header_with_slot(11, fx.isp.hash(), 100);
		let chosen = relay_header_with_slot(12, mid.hash(), 101);
		let twin_sibling = twin(&chosen, 7);
		let other_mid = twin(&mid, 8);
		let cousin = relay_header_with_slot(12, other_mid.hash(), 101);
		fx.add(&mid, None);
		fx.add(&other_mid, None);
		fx.add(&twin_sibling, Some(CoreIndex(0)));
		fx.add(&cousin, Some(CoreIndex(0)));
		let proof = fx.proof(vec![chosen, mid.clone()]);

		let mut expected = vec![vec![twin_sibling.clone(), mid], vec![cousin.clone(), other_mid]];
		expected.sort_by_key(|chain| chain[0].hash());
		assert_eq!(fx.resolve(&proof, &[&twin_sibling, &cousin], 2).await, expected);
	}

	/// Resolution gives up after `MAX_REJECTED_SIBLINGS` rejections past the limit.
	#[tokio::test]
	async fn sibling_chains_bound_rejections() {
		let mut fx = Fixture::new();
		let chosen = relay_header_with_slot(11, fx.isp.hash(), 101);
		// Primaries sort first, so these are checked before `good`; another parent keeps them
		// from outweighing it, and fails them at the internal scheduling parent.
		let primary = relay_header_primary_with_slot(11, [9u8; 32].into(), 101);
		let rejected: Vec<_> = (0..4).map(|tag| twin(&primary, tag)).collect();
		let good = twin(&chosen, 9);
		fx.add(&good, Some(CoreIndex(0)));
		let proof = fx.proof(vec![chosen]);

		let within: Vec<_> = rejected[..MAX_REJECTED_SIBLINGS].iter().chain([&good]).collect();
		assert_eq!(fx.resolve(&proof, &within, 1).await, vec![vec![good.clone()]]);
		let past: Vec<_> = rejected.iter().chain([&good]).collect();
		assert!(fx.resolve(&proof, &past, 1).await.is_empty());
	}

	/// A sibling whose chain to the internal scheduling parent starts a new session is rejected
	/// beyond `max_relay_parent_session_age`.
	#[tokio::test]
	async fn sibling_chains_bound_session_age() {
		let mut fx = Fixture::new();
		let chosen = relay_header_with_slot(11, fx.isp.hash(), 101);
		let mut new_session = twin(&chosen, 7);
		new_session.digest.push(babe_epoch_change_digest_item());
		fx.add(&new_session, Some(CoreIndex(0)));
		let proof = fx.proof(vec![chosen]);

		assert!(fx.resolve(&proof, &[&new_session], 1).await.is_empty());
		fx.max_session_age = 1;
		assert_eq!(fx.resolve(&proof, &[&new_session], 1).await, vec![vec![new_session]]);
	}
}
