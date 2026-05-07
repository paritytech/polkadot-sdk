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

//! Helpers shared across scenarios.

use crate::{
	aux::{CandidateBackingAux, ProspectiveParachainsAux},
	chain::{ChainModel, SessionInfo, SharedChain},
	contract::Query,
	harness::{AnswerQuery, LayeredResponder, Sim, SimConfig, SubsystemUnderTest},
};
use polkadot_node_subsystem::{
	messages::{AllMessages, CollatorProtocolMessage},
	OverseerSignal,
};
use polkadot_node_subsystem_test_helpers::mock::new_leaf;
use polkadot_overseer::ActiveLeavesUpdate;
use polkadot_primitives::{
	CoreIndex, GroupRotationInfo, Hash, Id as ParaId, ValidatorIndex,
};
use sp_consensus_slots::Slot;
use std::collections::{BTreeMap, VecDeque};

/// A responder that panics on every query. Pushed onto the tail of a
/// [`crate::harness::LayeredResponder`] to surface any unexpected query family that earlier
/// layers declined.
pub struct PanicResponder;

impl AnswerQuery for PanicResponder {
	fn answer(&mut self, query: Query) {
		panic!("PanicResponder: unhandled query reached the tail of the responder chain: {:?}", query);
	}
}

/// Outcome of [`activated_world`]: a fully wired Sim plus the leaf hash and the SharedChain
/// handle for further mutation.
pub struct World<S: SubsystemUnderTest>
where
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	pub sim: Sim<S>,
	pub leaf: Hash,
	pub chain: SharedChain,
}

/// Build a Sim with the standard validator-side world: one leaf, one session, one validator
/// group containing Alice (validator index 0). The claim queue at the leaf schedules `paras`
/// on `cores` (one core per `paras` entry, depth 3 per core). The activated-leaves signal
/// and OurViewChange are injected; both real prospective-parachains and candidate-backing
/// are spawned.
pub fn activated_world<S>(paras: &[(CoreIndex, ParaId)]) -> World<S>
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	use polkadot_node_subsystem::messages::NetworkBridgeEvent;

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

	let mut queue: BTreeMap<CoreIndex, VecDeque<ParaId>> = BTreeMap::new();
	for (core, para) in paras {
		queue.insert(*core, VecDeque::from_iter(std::iter::repeat(*para).take(3)));
	}
	if !paras.is_empty() {
		chain.set_claim_queue_at(leaf, queue);
	}

	let leaf_number = chain.block(&leaf).unwrap().number;
	let chain = SharedChain::new(chain);

	let mut responder = LayeredResponder::new();
	responder.push(chain.clone());
	responder.push(PanicResponder);

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
		polkadot_node_network_protocol::OurView::new(std::iter::once(leaf), 0),
	)));

	World { sim, leaf, chain }
}
