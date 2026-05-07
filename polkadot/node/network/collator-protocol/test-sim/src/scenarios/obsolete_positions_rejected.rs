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

//! Scenario: a V2 advertisement targets ancestor R, but para A only appears at claim queue
//! position 2. R sits at offset=1 with valid_len=2 → only positions [0, 1] are checked.
//! Para A is outside the valid range → advertisement rejected (no fetch).
//!
//! Mirrors `validator_side/tests/prospective_parachains.rs::obsolete_positions_rejected`.
//!
//! Note: experimental rejects ancestor-RP advertisements for an unrelated reason —
//! project_collator_experimental_no_ancestor_rp_advertise.md. Experimental "passing"
//! this test is incidental, not the same contract being checked. A proper experimental
//! variant would advertise at the leaf with a deliberately obsolete claim queue position;
//! deferred until experimental's allowed-ancestry semantics are clarified.

use crate::{
	builders::{Candidate, Peer, ProtocolVersion},
	chain::CoreSchedule,
	contract::Effect,
	harness::SubsystemUnderTest,
	scenarios::shared::{ChainConfig, LeafSelector},
};
use polkadot_node_subsystem::messages::{AllMessages, CollatorProtocolMessage};
use polkadot_primitives::{CoreIndex, HeadData, Id as ParaId};
use std::{collections::{BTreeMap, VecDeque}, time::Duration};

#[crate::sim_test]
fn ancestor_with_para_at_obsolete_position_rejects<S>()
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	let para_a = ParaId::from(2000);
	let para_b = ParaId::from(999);

	// Pre-activation config: schedule = always(B), override L's queue to [B, B, A].
	// Para A only appears at position 2 of L's queue. Legacy validator's offset
	// arithmetic at R = leaf_parent: valid_len = scheduling_lookahead(3) - offset(1) = 2;
	// para A at position 2 is outside the valid window → advertisement rejected.
	let mut leaf_q = BTreeMap::new();
	leaf_q.insert(CoreIndex(0), VecDeque::from(vec![para_b, para_b, para_a]));
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), CoreSchedule::always(para_b))
		.with_claim_queue_at(LeafSelector::Leaf, leaf_q);
	let mut world =
		crate::scenarios::shared::build_with_ancestors_world_with_config::<S>(1, config);

	let r = world.ancestors[0];

	// Peer declares for para A and advertises at R.
	let peer = Peer::new(para_a, ProtocolVersion::V2);
	world.sim.send(peer.connected());
	world.sim.send(peer.declare());

	let candidate = Candidate::for_para_at(para_a, r);
	let parent_head_hash = HeadData(Vec::new()).hash();
	world.sim.send(peer.advertise(r, Some(candidate.hash()), Some(parent_head_hash)));

	// No fetch should fire for this candidate.
	world.sim.advance(Duration::from_millis(200));
	let fetched = world.sim.recorder().entries().iter().any(|o| match o {
		crate::harness::Observation::Effect(s) => match &s.value {
			Effect::SendRequest { candidate_hash: Some(c), .. } => *c == candidate.hash(),
			_ => false,
		},
	});
	assert!(
		!fetched,
		"validator must not fetch a candidate whose para is outside the ancestor's valid claim-queue window",
	);
}
