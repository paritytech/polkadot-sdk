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

//! What the builder remembers between ticks.

use crate::nodes::jam::{
	builder::parent::{Descendant, ParentSource},
	chain::pools::PoolScan,
	LOG_TARGET,
};
use cumulus_client_jam_authorizer::authorizer::AuraAuthorizer;
use jam_interface::{BlockDesc, Slot as JamSlot};
use sp_consensus_aura::Slot;
use sp_runtime::traits::Header as HeaderT;
use std::collections::VecDeque;

/// What reading the accumulated para head did to the builder's view of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeadMove {
	/// The head is still where it was; nothing of ours accumulated since the last tick.
	Unchanged,
	/// The head moved to a block this collator authored.
	Ours,
	/// The head moved to a block this collator did not author: another collator's, the common
	/// case under slot rotation.
	Foreign,
}

/// Classify a change of the accumulated head, for the log alone.
///
/// Nothing branches on the answer any more: the builder picks the next parent from the local block
/// tree below whatever head JAM accepted, so a head this collator did not author is not a
/// divergence to heal but simply the block the next one is built on.
pub(crate) fn head_move<Hash: PartialEq>(
	before: Option<&Hash>,
	after: Option<&Hash>,
	ours: &VecDeque<Hash>,
) -> HeadMove {
	match after {
		after if after == before => HeadMove::Unchanged,
		Some(after) if ours.contains(after) => HeadMove::Ours,
		_ => HeadMove::Foreign,
	}
}

/// What the builder remembers between ticks.
pub(crate) struct BuilderState<Header: HeaderT> {
	/// The anchor slot last claimed. A turn is one anchor slot: the block's timestamp, and Aura
	/// slot both derive from the anchor, so two turns on one anchor would be two
	/// blocks claiming one Aura slot.
	pub(crate) last_claimed_anchor: Option<JamSlot>,
	/// The hash of the para head JAM has accumulated, as of the last tick that managed to read it.
	pub(crate) included: Option<Header::Hash>,
	/// The blocks this collator authored that are still below the accumulated head, oldest first.
	/// Its only job is to let [`select_parent`](super::parent::select_parent) prefer our own block
	/// over a peer's at the same depth, so [`Self::forget_unreachable`] drops every block that is
	/// no longer among the accumulated head's descendants: it can never be a parent again.
	pub(crate) own_recent: VecDeque<Header::Hash>,
	/// The para slot the accumulated head last moved in, which is what the stall clock counts
	/// from. `None` only before the first tick.
	pub(crate) head_advanced_at: Option<Slot>,
	/// The branch started on a stuck accumulated head, while it is still stuck.
	pub(crate) rerooted: Option<Reroot<Header::Hash>>,
	/// The last pool scan, so a stable core assignment is logged once instead of every tick. The
	/// *absent* core is logged every tick regardless: it is the reason nothing is being
	/// submitted, and it must not scroll out of sight.
	pub(crate) last_pool_scan: Option<PoolScan>,
}

/// The branch this collator started on an accumulated head that would not move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Reroot<Hash> {
	/// The last block authored on that branch — the next one extends it.
	pub(crate) tip: Hash,
	/// Its depth below the accumulated head.
	pub(crate) depth: usize,
}

impl<Header: HeaderT> BuilderState<Header> {
	pub(crate) fn remember_authored(&mut self, block_hash: Header::Hash) {
		self.own_recent.push_back(block_hash);
	}

	/// Forget the authored blocks that are no longer below the accumulated head.
	pub(crate) fn forget_unreachable(&mut self, descendants: &[Descendant<Header>]) {
		self.own_recent
			.retain(|hash| descendants.iter().any(|descendant| descendant.header.hash() == *hash));
	}

	/// Fold this tick's reading of the accumulated head into the stall clock, and say how many
	/// para slots that head has stood still *as of now*.
	///
	/// The clock is restarted before the answer is taken, so the tick that sees the head move
	/// reports no stall at all: the branch above the new head is fresh, and reporting the stall
	/// that just ended would abandon it on the very tick the parachain recovered.
	///
	/// Slot numbers rather than a tick count: a skipped tick — a stale JAM tip, a read that ran
	/// over its budget — must not buy a stuck branch extra time.
	pub(crate) fn note_head(&mut self, moved: HeadMove, para_slot: Slot) -> u64 {
		if moved != HeadMove::Unchanged {
			// The way out worked, or was never needed: release the branch and restart the clock.
			self.rerooted = None;
			self.head_advanced_at = Some(para_slot);
		}
		// The `None` case is the first tick of all: start the clock here rather than count a
		// stall from the epoch.
		let since = *self.head_advanced_at.get_or_insert(para_slot);
		(*para_slot).saturating_sub(*since)
	}

	/// Log this tick's pool scan.
	///
	/// No core holding the para's authorizer is warned about on every single tick: the parachain
	/// keeps producing blocks, so the only visible symptom is an accumulated head that stops
	/// moving, and this line is what tells the two apart. Every other scan is logged when it
	/// *changes*, which is what makes an assignment, a reassignment and a recovery each one line.
	pub(crate) fn note_pool_scan(
		&mut self,
		scan: &PoolScan,
		anchor: &BlockDesc,
		authorizer: &AuraAuthorizer,
	) {
		let previous = self.last_pool_scan.replace(scan.clone());
		match scan.target {
			None => tracing::warn!(
				target: LOG_TARGET,
				anchor = ?anchor.header_hash,
				anchor_slot = anchor.slot,
				authorizer_hash = ?authorizer.hash(),
				previously = ?previous,
				"No core's authorizer pool holds this para's authorizer, so nothing can be \
				 submitted. Authoring continues locally, but the accumulated head will not move \
				 until a core is assigned to this para.",
			),
			Some(core) if previous.as_ref() != Some(scan) => tracing::info!(
				target: LOG_TARGET,
				anchor = ?anchor.header_hash,
				anchor_slot = anchor.slot,
				authorizer_hash = ?authorizer.hash(),
				core,
				also_on = ?scan.also_on,
				previously = ?previous,
				"The cores holding this para's authorizer changed; a turn puts one package on \
				 each of them, lowest index first.",
			),
			Some(_) => {},
		}
	}

	/// Record where an authored block leaves the branch commitment: a block authored on a stuck
	/// head *starts* one — and restarts the stall clock, so the new branch gets a full window
	/// before it is abandoned in turn — while one authored on an existing branch extends it.
	pub(crate) fn note_authored(
		&mut self,
		source: ParentSource,
		block_hash: Header::Hash,
		para_slot: Slot,
	) {
		self.remember_authored(block_hash);
		match source {
			ParentSource::Reroot => {
				self.head_advanced_at = Some(para_slot);
				self.rerooted = Some(Reroot { tip: block_hash, depth: 1 });
			},
			ParentSource::Rerooted => {
				if let Some(reroot) = self.rerooted.as_mut() {
					reroot.tip = block_hash;
					reroot.depth += 1;
				}
			},
			ParentSource::AccumulatedHead | ParentSource::Deepest => {},
		}
	}
}
