// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Per-peer outboxes of statement hashes queued for propagation, and the held dead-end
//! statements.

use crate::config::MAX_PROPAGATION_OUTBOX_LEN;
use sc_network_types::PeerId;
use sp_statement_store::{Hash, Statement, StatementStore, Topic};
use std::{
	collections::{HashMap, HashSet, VecDeque},
	sync::Arc,
};

/// Statement hashes queued for propagation, per peer.
///
/// Each queued hash holds its statement in the store, safe from the retention sweep, until the
/// hash leaves the outbox. Hashes enter and leave only through these methods. A dead-end
/// statement, which has no peer to queue for, is held the same way.
pub(crate) struct PropagationOutboxes {
	store: Option<Arc<dyn StatementStore>>,
	outboxes: HashMap<PeerId, VecDeque<Hash>>,
	/// Admission sequence of each queued hash, in step with `outboxes`.
	seqs: HashMap<PeerId, VecDeque<u64>>,
	/// Statements held in the store until they find a peer to go to.
	dead_ends: Vec<DeadEnd>,
}

/// A statement that no routing target could receive when it was propagated.
pub(crate) struct DeadEnd {
	/// Admission sequence of the statement.
	pub(crate) seq: u64,
	pub(crate) hash: Hash,
	pub(crate) topics: Vec<Topic>,
	/// Unix time in seconds at which the statement expires.
	pub(crate) expires_at: u64,
	/// Peers that have the statement: they sent it to this node, or their outbox took it.
	pub(crate) senders: HashSet<PeerId>,
}

impl PropagationOutboxes {
	pub(crate) fn new(store: Option<Arc<dyn StatementStore>>) -> Self {
		Self { store, outboxes: HashMap::new(), seqs: HashMap::new(), dead_ends: Vec::new() }
	}

	pub(crate) fn get(&self, who: &PeerId) -> Option<&VecDeque<Hash>> {
		self.outboxes.get(who)
	}

	/// Append `hashes`, each with its admission sequence, to the peer's outbox until it is full
	/// and return how many it took.
	pub(crate) fn push(&mut self, who: PeerId, hashes: &[(u64, Hash)]) -> usize {
		let outbox = self.outboxes.entry(who).or_default();
		let taken = hashes.len().min(MAX_PROPAGATION_OUTBOX_LEN.saturating_sub(outbox.len()));
		let (seqs, hashes): (Vec<_>, Vec<_>) = hashes[..taken].iter().copied().unzip();
		if let Some(store) = &self.store {
			store.hold_statements(&hashes);
		}
		outbox.extend(hashes);
		self.seqs.entry(who).or_default().extend(seqs);
		taken
	}

	/// Lowest admission sequence queued for the peer.
	pub(crate) fn lowest_seq(&self, who: &PeerId) -> Option<u64> {
		self.seqs.get(who)?.iter().copied().min()
	}

	/// The peer's queued hashes as one slice, oldest first.
	pub(crate) fn queued(&mut self, who: &PeerId) -> Option<&[Hash]> {
		self.outboxes.get_mut(who).map(|outbox| &*outbox.make_contiguous())
	}

	/// Remove the peer's `count` oldest hashes and return their lowest admission sequence.
	pub(crate) fn consume(&mut self, who: &PeerId, count: usize) -> Option<u64> {
		let consumed: Vec<_> = self.outboxes.get_mut(who)?.drain(..count).collect();
		self.release(&consumed);
		self.seqs.get_mut(who)?.drain(..count).min()
	}

	/// Remove the peer's outbox and return how many hashes it held.
	pub(crate) fn remove(&mut self, who: &PeerId) -> usize {
		self.seqs.remove(who);
		let Some(outbox) = self.outboxes.remove(who) else { return 0 };
		let removed = Vec::from(outbox);
		self.release(&removed);
		removed.len()
	}

	/// Remove the peer's queued hashes except detours, queued under `DETOUR_SEQ`, and return how
	/// many it removed.
	pub(crate) fn retain_detours(&mut self, who: &PeerId) -> usize {
		let (Some(outbox), Some(seqs)) = (self.outboxes.remove(who), self.seqs.remove(who)) else {
			return self.remove(who);
		};
		let (detours, removed): (Vec<_>, Vec<_>) =
			outbox.into_iter().zip(seqs).partition(|(_, seq)| *seq == crate::DETOUR_SEQ);
		let removed: Vec<_> = removed.into_iter().map(|(hash, _)| hash).collect();
		self.release(&removed);
		if !detours.is_empty() {
			let (hashes, seqs) = detours.into_iter().unzip();
			self.outboxes.insert(*who, hashes);
			self.seqs.insert(*who, seqs);
		}
		removed.len()
	}

	/// Hold `dead_end` in the store.
	pub(crate) fn hold_dead_end(&mut self, dead_end: DeadEnd) {
		if let Some(store) = &self.store {
			store.hold_statements(&[dead_end.hash]);
		}
		self.dead_ends.push(dead_end);
	}

	/// The held dead-end statements, oldest first.
	pub(crate) fn dead_ends(&self) -> &[DeadEnd] {
		&self.dead_ends
	}

	/// Record that `peer` has the dead-end statement `hash`, so it is not sent to `peer` again.
	pub(crate) fn add_dead_end_sender(&mut self, hash: &Hash, peer: PeerId) {
		for dead_end in self.dead_ends.iter_mut().filter(|dead_end| dead_end.hash == *hash) {
			dead_end.senders.insert(peer);
		}
	}

	/// Release the dead-end statements that `is_stuck` rejects.
	pub(crate) fn retain_dead_ends(&mut self, mut is_stuck: impl FnMut(&DeadEnd) -> bool) {
		let mut released = Vec::new();
		self.dead_ends.retain(|dead_end| {
			let stuck = is_stuck(dead_end);
			if !stuck {
				released.push(dead_end.hash);
			}
			stuck
		});
		self.release(&released);
	}

	/// Release the holds [`StatementStore::take_recent_statements`] placed, once `statements`
	/// are queued or held as dead ends.
	pub(crate) fn release_taken(&self, statements: &[(u64, Hash, Statement)]) {
		let hashes: Vec<_> = statements.iter().map(|(_, hash, _)| *hash).collect();
		self.release(&hashes);
	}

	fn release(&self, hashes: &[Hash]) {
		match &self.store {
			Some(store) if !hashes.is_empty() => store.release_statements(hashes),
			_ => {},
		}
	}
}

impl Drop for PropagationOutboxes {
	fn drop(&mut self) {
		let queued: Vec<_> = self.outboxes.drain().flat_map(|(_, outbox)| outbox).collect();
		self.release(&queued);
		let dead_ends: Vec<_> = self.dead_ends.drain(..).map(|dead_end| dead_end.hash).collect();
		self.release(&dead_ends);
	}
}
