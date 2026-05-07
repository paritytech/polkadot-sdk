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

//! Scenario: a peer connects (after the validator has received its first ActiveLeaves
//! signal) and then sends a `Declare` with a bogus signature. The validator penalises the
//! peer with a `Malicious` reputation hit and emits no further effects.
//!
//! Why the ActiveLeaves preamble? The experimental validator side disconnects every peer
//! that arrives before its first ActiveLeaves notification (initialisation guard). Both
//! implementations need a leaf in scope to behave normally — the ActiveLeaves stimulus is
//! just framework setup, the assertion is still about the bad-signature path.

use crate::{
	aux::{CandidateBackingAux, ProspectiveParachainsAux},
	builders::{Peer, ProtocolVersion},
	chain::{ChainModel, SessionInfo, SharedChain},
	contract::{Effect, RepBucket},
	harness::{LayeredResponder, Sim, SimConfig, SubsystemUnderTest},
};
use polkadot_node_subsystem::{
	messages::{AllMessages, CollatorProtocolMessage},
	OverseerSignal,
};
use polkadot_node_subsystem_test_helpers::mock::new_leaf;
use polkadot_overseer::ActiveLeavesUpdate;
use polkadot_primitives::{GroupRotationInfo, Id as ParaId, ValidatorIndex};
use sp_consensus_slots::Slot;
use std::time::Duration;

// EXPERIMENTAL DIVERGENCE (intentional? bug? — investigate):
// validator_side_experimental/mod.rs:431-435 destructures the Declare signature into
// `_signature` and never verifies it. Bad signatures don't produce Reputation::Malicious
// on the experimental side; this scenario therefore *fails* against ExperimentalValidator
// and that failure is the framework reporting the divergence — see
// memory/project_collator_experimental_skips_declare_sig.md for the full write-up. Do not
// filter the scenario to silence the failure; the failure is the value.
#[crate::sim_test]
fn declare_with_bad_signature_yields_malicious_reputation<S>()
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	// Minimal chain: one leaf on top of genesis with a session info installed. The leaf has
	// no claim queue because the assertion is about the bad-signature path, not assignment.
	let mut chain = ChainModel::new(Slot::from(0));
	chain.add_session(
		0,
		SessionInfo {
			validators: crate::builders::fixtures::default_validators(),
			validator_groups: vec![vec![ValidatorIndex(0), ValidatorIndex(1)]],
			group_rotation_info: GroupRotationInfo {
				session_start_block: 0,
				group_rotation_frequency: 1,
				now: 0,
			},
		},
	);
	let leaf = chain.extend(chain.genesis());
	let leaf_number = chain.block(&leaf).unwrap().number;

	let chain = SharedChain::new(chain);
	let mut responder = LayeredResponder::new();
	responder.push(chain.clone());
	responder.push(crate::scenarios::shared::PanicResponder);

	let mut sim = Sim::<S>::start(SimConfig::default(), responder);
	let (psp, psp_rx) = ProspectiveParachainsAux::spawn(&mut sim);
	let (cb, cb_rx) = CandidateBackingAux::spawn(&mut sim);
	sim.register_aux(psp, psp_rx);
	sim.register_aux(cb, cb_rx);

	sim.signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
		leaf,
		leaf_number,
	))));

	let peer = Peer::new(ParaId::from(2000), ProtocolVersion::V1);

	sim.send(peer.connected());
	sim.send(peer.declare_with_bad_signature());

	let observed = sim.expect(
		|effect| matches!(effect, Effect::Reputation { bucket: RepBucket::Malicious, peer: p } if *p == peer.peer_id),
		Duration::from_millis(50),
		"Effect::Reputation { Malicious } for the bad-signature peer",
	);
	match observed {
		Effect::Reputation { peer: observed_peer, bucket } => {
			assert_eq!(observed_peer, peer.peer_id);
			assert_eq!(bucket, RepBucket::Malicious);
		},
		other => panic!("predicate matched but variant unexpected: {:?}", other),
	}
}

/// Sanity counterpart: a peer with a *valid* signature and a properly scheduled para does
/// NOT receive a malicious reputation hit. Pairs with the bad-signature test to rule out
/// "any declare in this test setup triggers Reputation::Malicious" as a false positive.
#[crate::sim_test]
fn declare_with_valid_signature_does_not_get_malicious_reputation<S>()
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	let mut chain = ChainModel::new(Slot::from(0));
	chain.add_session(
		0,
		SessionInfo {
			validators: crate::builders::fixtures::default_validators(),
			validator_groups: vec![vec![ValidatorIndex(0), ValidatorIndex(1)]],
			group_rotation_info: GroupRotationInfo {
				session_start_block: 0,
				group_rotation_frequency: 1,
				now: 0,
			},
		},
	);
	let para = ParaId::from(2000);
	chain.set_core_schedule(polkadot_primitives::CoreIndex(0), crate::chain::CoreSchedule::always(para));
	let leaf = chain.extend(chain.genesis());
	let leaf_number = chain.block(&leaf).unwrap().number;

	let chain = SharedChain::new(chain);
	let mut responder = LayeredResponder::new();
	responder.push(chain.clone());
	responder.push(crate::scenarios::shared::PanicResponder);

	let mut sim = Sim::<S>::start(SimConfig::default(), responder);
	let (psp, psp_rx) = ProspectiveParachainsAux::spawn(&mut sim);
	let (cb, cb_rx) = CandidateBackingAux::spawn(&mut sim);
	sim.register_aux(psp, psp_rx);
	sim.register_aux(cb, cb_rx);

	sim.signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
		leaf,
		leaf_number,
	))));

	let peer = Peer::new(para, ProtocolVersion::V1);
	sim.send(peer.connected());
	sim.send(peer.declare()); // valid signature

	sim.advance(Duration::from_millis(100));

	let malicious_hit = sim.recorder().entries().iter().any(|o| match o {
		crate::harness::Observation::Effect(s) => matches!(
			&s.value,
			Effect::Reputation { peer: p, bucket: RepBucket::Malicious } if *p == peer.peer_id,
		),
	});
	assert!(
		!malicious_hit,
		"valid declare must NOT receive Reputation::Malicious\n\n{}",
		crate::report::format_timeline(sim.recorder()),
	);
}
