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

//! Differential test: run the same stimulus against both validator implementations and
//! check they reach the same observable milestone.
//!
//! For Phase H this stops at the SendRequest milestone (advertise → fetch). Future versions
//! can run further into the seconding flow as the controllable stubs (av-store, av-dist,
//! statement-dist, provisioner) come online.

use crate::{
	aux::{CandidateBackingAux, ProspectiveParachainsAux},
	builders::{Candidate, Peer, ProtocolVersion},
	chain::{ChainModel, SessionInfo, SharedChain},
	contract::{Effect, ReqKind},
	harness::{LayeredResponder, Sim, SimConfig, SubsystemUnderTest},
	impls::{ExperimentalValidator, LegacyValidator},
};
use polkadot_node_network_protocol::OurView;
use polkadot_node_subsystem::{
	messages::{AllMessages, CollatorProtocolMessage, NetworkBridgeEvent},
	OverseerSignal,
};
use polkadot_node_subsystem_test_helpers::mock::new_leaf;
use polkadot_overseer::ActiveLeavesUpdate;
use polkadot_primitives::{CoreIndex, GroupRotationInfo, Id as ParaId, ValidatorIndex};
use sc_network_types::PeerId;
use sp_consensus_slots::Slot;
use std::{collections::VecDeque, time::Duration};

/// Run the advertise-then-fetch stimulus against `S` with a deterministic peer + candidate.
/// Returns whether the implementation reached the SendRequest milestone for the expected
/// peer + candidate combination.
fn run_reaches_fetch_milestone<S>(peer_id: PeerId) -> bool
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	let para = ParaId::from(2000);

	let mut chain = ChainModel::new(Slot::from(100));
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
	let mut queue = std::collections::BTreeMap::new();
	queue.insert(CoreIndex(0), VecDeque::from_iter(std::iter::repeat(para).take(3)));
	chain.set_claim_queue_at(leaf, queue);
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
	sim.send(CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::OurViewChange(
		OurView::new(std::iter::once(leaf), 0),
	)));

	let peer = Peer::new(para, ProtocolVersion::V2).with_peer_id(peer_id);
	let candidate = Candidate::for_para_at(para, leaf);

	sim.send(peer.connected());
	sim.send(peer.declare());
	sim.send(peer.advertise(
		leaf,
		Some(candidate.hash()),
		Some(candidate.receipt.descriptor.para_head()),
	));

	let _ = sim.expect(
		|effect| {
			matches!(
				effect,
				Effect::SendRequest {
					to,
					kind: ReqKind::CollationFetchingV2,
					candidate_hash: Some(c),
					..
				} if *to == peer_id && c == &candidate.hash()
			)
		},
		Duration::from_millis(500),
		"Effect::SendRequest CollationFetchingV2 for the advertised candidate",
	);
	true
}

#[crate::sim_test]
fn legacy_and_experimental_both_fetch_after_advertise() {
	// Same peer-id and same para input fed to both implementations. The chain model and
	// every other input is reset between runs (each `run_reaches_fetch_milestone` call
	// builds its own world). Both implementations should reach the same observable
	// milestone — the SendRequest effect with a matching candidate hash and peer id —
	// despite diverging in internal reputation/peer-management mechanics.
	let peer_id = PeerId::random();
	assert!(
		run_reaches_fetch_milestone::<LegacyValidator>(peer_id),
		"LegacyValidator did not reach the fetch milestone"
	);
	assert!(
		run_reaches_fetch_milestone::<ExperimentalValidator>(peer_id),
		"ExperimentalValidator did not reach the fetch milestone"
	);
}
