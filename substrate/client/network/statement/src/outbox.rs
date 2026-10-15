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

//! Per-peer outboxes of statement hashes queued for propagation.

use crate::config::MAX_PROPAGATION_OUTBOX_LEN;
use sc_network_types::PeerId;
use sp_statement_store::{Hash, Statement, StatementStore};
use std::{
	collections::{HashMap, VecDeque},
	sync::Arc,
};

/// Statement hashes queued for propagation, per peer.
///
/// Each queued hash holds its statement in the store, safe from the retention sweep, until the
/// hash leaves the outbox. Hashes enter and leave only through these methods.
pub(crate) struct PropagationOutboxes {
	store: Option<Arc<dyn StatementStore>>,
	outboxes: HashMap<PeerId, VecDeque<Hash>>,
}

impl PropagationOutboxes {
	pub(crate) fn new(store: Option<Arc<dyn StatementStore>>) -> Self {
		Self { store, outboxes: HashMap::new() }
	}

	pub(crate) fn get(&self, who: &PeerId) -> Option<&VecDeque<Hash>> {
		self.outboxes.get(who)
	}

	/// Append `hashes` to the peer's outbox and return how many queued hashes overflow dropped.
	pub(crate) fn push(&mut self, who: PeerId, hashes: Vec<Hash>) -> usize {
		if let Some(store) = &self.store {
			store.hold_statements(&hashes);
		}
		let outbox = self.outboxes.entry(who).or_default();
		let mut dropped = Vec::new();
		for hash in hashes {
			// The freshest statements are the ones still worth delivering, so an
			// overflowing outbox drops from the front.
			if outbox.len() == MAX_PROPAGATION_OUTBOX_LEN {
				dropped.extend(outbox.pop_front());
			}
			outbox.push_back(hash);
		}
		self.release(&dropped);
		dropped.len()
	}

	/// The peer's queued hashes as one slice, oldest first.
	pub(crate) fn queued(&mut self, who: &PeerId) -> Option<&[Hash]> {
		self.outboxes.get_mut(who).map(|outbox| &*outbox.make_contiguous())
	}

	/// Remove the peer's `count` oldest hashes.
	pub(crate) fn consume(&mut self, who: &PeerId, count: usize) {
		let Some(outbox) = self.outboxes.get_mut(who) else { return };
		let consumed: Vec<_> = outbox.drain(..count).collect();
		self.release(&consumed);
	}

	/// Remove the peer's outbox and return how many hashes it held.
	pub(crate) fn remove(&mut self, who: &PeerId) -> usize {
		let Some(outbox) = self.outboxes.remove(who) else { return 0 };
		let removed = Vec::from(outbox);
		self.release(&removed);
		removed.len()
	}

	/// Release the holds [`StatementStore::take_recent_statements`] placed, once `statements`
	/// are queued.
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
	}
}
