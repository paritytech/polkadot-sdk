// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Peer declares twice for an unneeded para. Each `Declare` triggers `COST_UNNEEDED_COLLATOR`
//! (a `CostMinor`), which the validator's `ReputationAggregator` buffers rather than dispatching
//! immediately. After `REPUTATION_CHANGE_INTERVAL` elapses the aggregator flushes a single
//! batched `ReportPeer` covering both hits.
//!
//! KNOWN-FAIL on experimental: experimental writes reputation directly to a persistent store
//! and never emits a `ReportPeer` bus event (see `project_collator_experimental_no_invalid
//! _reputation_event.md`). The validation still happens, just silently — observable Effect
//! is missing on that side.

use crate::{
	builders::{Peer, ProtocolVersion::V1},
	contract::{Effect, RepBucket},
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_node_subsystem_util::reputation::REPUTATION_CHANGE_INTERVAL;
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::time::Duration;

const SCHEDULED: ParaId = ParaId::new(2000);
const WRONG: ParaId = ParaId::new(69);

#[crate::sim_test]
fn declare_twice_unneeded_para_emits_one_batched_rep_event<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), SCHEDULED)]);

	// Connect once, declare for the unneeded para twice. Both Declare events are buffered
	// by the ReputationAggregator (`COST_UNNEEDED_COLLATOR` is `CostMinor`, not `Malicious`).
	let peer = Peer::new(WRONG, V1);
	w.sim.send(peer.connected());
	w.sim.send(peer.declare());
	w.sim.send(peer.declare());

	// No rep effect yet — buffered.
	w.sim.expect_count(
		|e| matches!(
			e,
			Effect::Reputation { peer: p, bucket: RepBucket::Performance } if *p == peer.peer_id,
		),
		0,
		"no Reputation::Performance before the aggregator flushes",
	);

	// Advance past the flush interval; the aggregator dispatches a single Batch covering
	// both hits → one Performance effect (sum of two CostMinors stays in the
	// Performance bucket; bucket-level granularity is what the test framework records).
	w.sim.advance(REPUTATION_CHANGE_INTERVAL + Duration::from_secs(1));
	w.sim.expect_count(
		|e| matches!(
			e,
			Effect::Reputation { peer: p, bucket: RepBucket::Performance } if *p == peer.peer_id,
		),
		1,
		"exactly one batched Reputation::Performance for the unneeded-para peer",
	);
}
