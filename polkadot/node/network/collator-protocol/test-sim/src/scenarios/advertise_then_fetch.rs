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

//! Headline scenario: a peer connects, declares for our scheduled para, and advertises a
//! candidate. The validator's CanSecond check (against the real candidate-backing subsystem)
//! passes and the validator fires a `CollationFetchingV2` request at the peer.
//!
//! This is the first scenario that drives the full hybrid harness — chain model answering
//! Runtime/ChainApi, real prospective-parachains, real candidate-backing — through to a
//! `SendRequest` effect.

use crate::{
	aux::{CandidateBackingAux, ProspectiveParachainsAux},
	builders::{Candidate, Peer, ProtocolVersion},
	chain::{ChainModel, SessionInfo, SharedChain},
	contract::{Effect, ReqKind},
	harness::{LayeredResponder, Sim, SimConfig},
	impls::LegacyValidator,
};
use polkadot_node_network_protocol::OurView;
use polkadot_node_subsystem::{
	messages::{CollatorProtocolMessage, NetworkBridgeEvent},
	OverseerSignal,
};
use polkadot_node_subsystem_test_helpers::mock::new_leaf;
use polkadot_overseer::ActiveLeavesUpdate;
use polkadot_primitives::{
	CoreIndex, GroupRotationInfo, Id as ParaId, ValidatorIndex,
};
use sp_consensus_slots::Slot;
use std::{collections::VecDeque, time::Duration};

#[crate::sim_test]
fn valid_advertisement_triggers_fetch() {
	let para = ParaId::from(2000);

	// Build a chain model with one leaf on top of genesis. Para 2000 is scheduled on core 0
	// with depth 3 (matches scheduling lookahead).
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

	// Compose the responder chain: chain model first (handles Runtime/ChainApi), then a
	// panic tail to surface any unexpected query family.
	let mut responder = LayeredResponder::new();
	responder.push(chain.clone());
	responder.push(crate::scenarios::shared::PanicResponder);

	let mut sim = Sim::<LegacyValidator>::start(SimConfig::default(), responder);

	// Wire real prospective-parachains and candidate-backing.
	let (psp, psp_rx) = ProspectiveParachainsAux::spawn(&mut sim);
	let (cb, cb_rx) = CandidateBackingAux::spawn(&mut sim);
	sim.register_aux(psp, psp_rx);
	sim.register_aux(cb, cb_rx);

	// Activate the leaf on every subsystem.
	sim.signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
		leaf,
		leaf_number,
	))));

	// Push the validator's view to include this leaf — this is what triggers the
	// validator-side runtime queries.
	sim.send(CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::OurViewChange(
		OurView::new(std::iter::once(leaf), 0),
	)));

	let peer = Peer::new(para, ProtocolVersion::V2);
	let candidate = Candidate::for_para_at(para, leaf);

	sim.send(peer.connected());
	sim.send(peer.declare());
	sim.send(peer.advertise(
		leaf,
		Some(candidate.hash()),
		Some(candidate.receipt.descriptor.para_head()),
	));

	let observed = sim.expect(
		|effect| {
			matches!(
				effect,
				Effect::SendRequest {
					to,
					kind: ReqKind::CollationFetchingV2,
					candidate_hash: Some(c),
					..
				} if *to == peer.peer_id && c == &candidate.hash()
			)
		},
		Duration::from_millis(200),
		"Effect::SendRequest CollationFetchingV2 for the advertised candidate",
	);
	match observed {
		Effect::SendRequest { kind, candidate_hash, to, .. } => {
			assert_eq!(kind, ReqKind::CollationFetchingV2);
			assert_eq!(candidate_hash, Some(candidate.hash()));
			assert_eq!(to, peer.peer_id);
		},
		other => panic!("predicate matched but variant unexpected: {:?}", other),
	}
}
