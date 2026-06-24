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

//! Affinity gate for locally submitted statements.
//!
//! A local RPC submission goes straight to [`sp_statement_store::StatementStore::submit`], which
//! has no view of the node's DHT topology. This module exposes the orchestrator's affinity
//! decision as a [`LocalRetentionPolicy`] the store can read synchronously, so a local submission
//! is persisted only with DHT or explicit affinity — the same rule applied to network-received
//! statements.

use std::{
	collections::HashSet,
	sync::{Arc, RwLock},
};

use sp_statement_store::{LocalRetentionPolicy, RetentionReasonMask, Statement, Topic};

use super::peers_topology::PeersTopology;

/// A point-in-time copy of the local node's affinity state, enough to decide whether a locally
/// submitted statement is worth persisting.
pub(crate) struct RetentionSnapshot {
	topology: PeersTopology,
	local_topics: HashSet<Topic>,
}

impl RetentionSnapshot {
	pub(crate) fn new(topology: PeersTopology, local_topics: HashSet<Topic>) -> Self {
		Self { topology, local_topics }
	}

	/// Mirror of `V2DhtOrchestrator::retention_reasons_for` over the snapshot's owned state.
	fn retention_reasons_for(&self, statement: &Statement) -> RetentionReasonMask {
		let mut mask = RetentionReasonMask::TRANSIENT;
		if statement.topics().iter().any(|topic| self.topology.is_dht_affine(*topic)) {
			mask.insert(RetentionReasonMask::DHT_AFFINITY);
		}
		if statement.topics().iter().any(|topic| self.local_topics.contains(topic)) {
			mask.insert(RetentionReasonMask::EXPLICIT_AFFINITY);
		}
		mask
	}
}

/// Shared handle the statement store reads on each local submission.
///
/// The statement handler keeps the inner snapshot current on the affinity tick; the store reads it
/// synchronously from `submit` to gate local submissions by affinity.
#[derive(Clone)]
pub(crate) struct SharedRetention(Arc<RwLock<RetentionSnapshot>>);

impl SharedRetention {
	pub(crate) fn new(snapshot: RetentionSnapshot) -> Self {
		Self(Arc::new(RwLock::new(snapshot)))
	}

	/// Replace the snapshot with a freshly computed one.
	pub(crate) fn store(&self, snapshot: RetentionSnapshot) {
		*self.0.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = snapshot;
	}
}

impl LocalRetentionPolicy for SharedRetention {
	fn retention_reasons_for(&self, statement: &Statement) -> RetentionReasonMask {
		self.0
			.read()
			.unwrap_or_else(|poisoned| poisoned.into_inner())
			.retention_reasons_for(statement)
	}
}
