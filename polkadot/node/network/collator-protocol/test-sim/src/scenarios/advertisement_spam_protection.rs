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

//! Scenario: a peer advertises, the validator asks `CanSecond`, the test answers `false`.
//! The peer then re-advertises the same candidate. Validator penalises the peer for sending
//! an unexpected message (spam protection).
//!
//! Mirrors `validator_side/tests/prospective_parachains.rs::advertisement_spam_protection`.
//!
//! KNOWN-FAILING (both impls): the first advertisement does not produce any observable
//! effect in our setup — the validator's CanSecond → stub_false → drop path is supposed to
//! still record the advertisement for spam-detection, but the second (duplicate) ad does
//! not produce a Reputation::Performance hit. Possibly a timing artifact (the recorded
//! sim_t shows ~1.1s elapsed during what should be ~100ms — investigate
//! Sim::advance / drain interaction with subsystem-internal tick streams) or a subtle
//! shape mismatch (e.g. parent_head_data_hash vs candidate.descriptor.para_head, V2 vs V3
//! advertisement framing). Test is left in place as a TODO to investigate; not flagged as
//! a divergence until root cause is known.

use crate::{
	builders::{Candidate, Peer, ProtocolVersion},
	chain::CoreSchedule,
	contract::{Effect, RepBucket},
	harness::SubsystemUnderTest,
	scenarios::shared::ChainConfig,
};
use polkadot_node_subsystem::messages::{AllMessages, CollatorProtocolMessage};
use polkadot_primitives::{CoreIndex, HeadData, Id as ParaId};
use std::time::Duration;

#[crate::sim_test]
fn re_advertising_after_can_second_false_triggers_reputation_hit<S>()
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	let para = ParaId::from(2000);

	// Use a CanSecond=false stub so the validator rejects the first advertisement at
	// the CanSecond gate.
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), CoreSchedule::always(para))
		.with_can_second_stub(false);
	let mut world = crate::scenarios::shared::build_with_ancestors_world_with_config::<S>(0, config);

	let peer = Peer::new(para, ProtocolVersion::V2);
	world.sim.send(peer.connected());
	world.sim.send(peer.declare());

	let candidate = Candidate::for_para_at(para, world.leaf);
	let parent_head_hash = HeadData(Vec::new()).hash();

	// First advertisement: validator queries CanSecond → stub says false → drop.
	world.sim.send(peer.advertise(world.leaf, Some(candidate.hash()), Some(parent_head_hash)));
	world.sim.advance(Duration::from_millis(100));

	// Settle: confirm no fetch fired and no reputation hit yet.
	world.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		0,
		"SendRequest after CanSecond=false (must be zero)",
	);

	// Second (duplicate) advertisement: triggers spam protection — Reputation::Performance
	// (COST_UNEXPECTED_MESSAGE).
	world.sim.send(peer.advertise(world.leaf, Some(candidate.hash()), Some(parent_head_hash)));

	let _ = world.sim.expect(
		|e| matches!(
			e,
			Effect::Reputation { peer: p, bucket: RepBucket::Performance } if *p == peer.peer_id,
		),
		Duration::from_millis(200),
		"Effect::Reputation { Performance } after duplicate advertisement",
	);
}
