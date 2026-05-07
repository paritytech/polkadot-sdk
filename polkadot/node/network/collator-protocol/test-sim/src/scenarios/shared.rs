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
	aux::{
		AvailabilityDistributionNoop, AvailabilityStoreStub, CandidateBackingAux,
		CandidateValidationStub, ProspectiveParachainsAux, ProvisionerNoop,
		StatementDistributionNoop,
	},
	chain::{ChainModel, CoreSchedule, SessionInfo, SharedChain},
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
	let world = build_multi_leaf_world::<S>(1, paras);
	World { sim: world.sim, leaf: world.leaves[0], chain: world.chain }
}

/// Multi-leaf variant of [`World`]. `leaves[0]` is the shallowest (genesis's first child);
/// `leaves[i]` for `i > 0` are progressively deeper blocks in the same chain. The same
/// claim queue is installed at every leaf.
pub struct MultiLeafWorld<S: SubsystemUnderTest>
where
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	pub sim: Sim<S>,
	/// Leaves in extend order: `leaves[0]` is genesis's first child, `leaves[N-1]` is the
	/// deepest. All are signalled active and included in OurView.
	pub leaves: Vec<Hash>,
	pub chain: SharedChain,
}

/// World with a single active leaf `L` plus a chain of `n_ancestors` blocks under it.
/// Returns `(sim, leaf, ancestors, chain)` — `ancestors[0]` is L's parent, `ancestors[N-1]`
/// is the oldest ancestor in scope. The leaf is signalled active; OurView contains only L.
/// Implicit-view machinery in the real prospective subsystem resolves the ancestors via
/// `ChainApi::Ancestors`.
pub struct WithAncestorsWorld<S: SubsystemUnderTest>
where
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	pub sim: Sim<S>,
	pub leaf: Hash,
	/// Ancestors in walk-back order: `ancestors[0]` is L's parent.
	pub ancestors: Vec<Hash>,
	pub chain: SharedChain,
}

/// Build a multi-leaf world: extends the chain `n_leaves` times on top of genesis,
/// installs the claim queue at every leaf, signals `ActiveLeaves::start_work` for each
/// leaf in extend order, and pushes an `OurViewChange` containing all of them.
pub fn build_multi_leaf_world<S>(n_leaves: usize, paras: &[(CoreIndex, ParaId)]) -> MultiLeafWorld<S>
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	use polkadot_node_subsystem::messages::NetworkBridgeEvent;

	assert!(n_leaves >= 1, "build_multi_leaf_world requires at least one leaf");

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
	// Install per-core schedule: each (core, para) pair becomes a static "always this para
	// on this core" rotation. Tests that need rotating cycles should call
	// `chain.set_core_schedule(core, CoreSchedule::cycling(...))` directly afterwards.
	for (core, para) in paras {
		chain.set_core_schedule(*core, CoreSchedule::always(*para));
	}

	let mut leaves = Vec::with_capacity(n_leaves);
	let mut parent = chain.genesis();
	for _ in 0..n_leaves {
		let leaf = chain.extend(parent);
		leaves.push(leaf);
		parent = leaf;
	}

	let leaf_numbers: Vec<u32> =
		leaves.iter().map(|h| chain.block(h).unwrap().number).collect();
	let chain = SharedChain::new(chain);

	let mut responder = LayeredResponder::new();
	responder.push(chain.clone());
	responder.push(PanicResponder);

	let mut sim = Sim::<S>::start(SimConfig::default(), responder);
	let (psp, psp_rx) = ProspectiveParachainsAux::spawn(&mut sim);
	let (cb, cb_rx) = CandidateBackingAux::spawn(&mut sim);
	sim.register_aux(psp, psp_rx);
	sim.register_aux(cb, cb_rx);

	let cv = CandidateValidationStub::always_valid(&mut sim);
	let av = AvailabilityStoreStub::spawn(&mut sim);
	sim.register_aux_slot_only(cv);
	sim.register_aux_slot_only(av);
	sim.register_aux_slot_only(StatementDistributionNoop::new());
	sim.register_aux_slot_only(ProvisionerNoop::new());
	sim.register_aux_slot_only(AvailabilityDistributionNoop::new());

	for (leaf, number) in leaves.iter().zip(leaf_numbers.iter()) {
		sim.signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
			*leaf, *number,
		))));
	}
	sim.send(CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::OurViewChange(
		polkadot_node_network_protocol::OurView::new(leaves.iter().copied(), 0),
	)));

	MultiLeafWorld { sim, leaves, chain }
}

/// Pre-activation chain configuration. Tests construct this, hand it to
/// [`build_with_ancestors_world_with_config`] (or its sibling), and the helper applies all
/// settings *before* signalling `ActiveLeaves` / `OurViewChange` — so the subsystems'
/// caches see the configured shape, not a default-then-overridden one.
pub struct ChainConfig {
	/// Per-core schedule. Each entry installs a [`CoreSchedule`] for that core.
	pub schedule: Vec<(CoreIndex, CoreSchedule)>,
	/// Per-block claim-queue overrides. The hash referenced here may be either the leaf
	/// hash returned by the helper or any of its ancestors.
	pub claim_queue_overrides: Vec<(LeafSelector, BTreeMap<CoreIndex, VecDeque<ParaId>>)>,
	/// If `Some(verdict)`, replace real `candidate-backing` with a `CanSecond`-only stub
	/// that always answers with `verdict`. Drops every other CandidateBacking message.
	/// Use when a scenario specifically needs a `CanSecond=false` (or `=true`) verdict
	/// that real backing wouldn't produce in our minimal chain shape.
	pub can_second_stub: Option<bool>,
}

impl Default for ChainConfig {
	fn default() -> Self {
		Self {
			schedule: Vec::new(),
			claim_queue_overrides: Vec::new(),
			can_second_stub: None,
		}
	}
}

impl ChainConfig {
	/// Add a per-core schedule.
	pub fn with_schedule(mut self, core: CoreIndex, schedule: CoreSchedule) -> Self {
		self.schedule.push((core, schedule));
		self
	}

	/// Override the claim queue at a relative-position selector.
	pub fn with_claim_queue_at(
		mut self,
		at: LeafSelector,
		queue: BTreeMap<CoreIndex, VecDeque<ParaId>>,
	) -> Self {
		self.claim_queue_overrides.push((at, queue));
		self
	}

	/// Replace real `candidate-backing` with a `CanSecond`-only stub.
	pub fn with_can_second_stub(mut self, verdict: bool) -> Self {
		self.can_second_stub = Some(verdict);
		self
	}
}

/// Identifies a block in the configured chain by its position.
#[derive(Clone, Copy, Debug)]
pub enum LeafSelector {
	/// The active leaf.
	Leaf,
	/// `Ancestor(n)`: the n-th ancestor of the leaf. `Ancestor(0)` is the leaf's direct parent.
	Ancestor(usize),
}

/// Build a world with a single active leaf and a chain of ancestors. Same claim queue is
/// installed at the leaf and every ancestor so advertisements at any of them get the same
/// schedule.
pub fn build_with_ancestors_world<S>(
	n_ancestors: usize,
	paras: &[(CoreIndex, ParaId)],
) -> WithAncestorsWorld<S>
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	let mut config = ChainConfig::default();
	for (core, para) in paras {
		config = config.with_schedule(*core, CoreSchedule::always(*para));
	}
	build_with_ancestors_world_with_config::<S>(n_ancestors, config)
}

/// Variant of [`build_with_ancestors_world`] that takes a [`ChainConfig`]. All schedule
/// installs and claim-queue overrides are applied **before** the helper signals
/// `ActiveLeaves` / `OurViewChange` so the subsystems' caches see the configured shape.
pub fn build_with_ancestors_world_with_config<S>(
	n_ancestors: usize,
	config: ChainConfig,
) -> WithAncestorsWorld<S>
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

	// Apply schedule from config.
	for (core, schedule) in config.schedule {
		chain.set_core_schedule(core, schedule);
	}

	// Build a chain: oldest_ancestor → ... → leaf-parent → leaf.
	let mut current = chain.genesis();
	let mut ancestors_in_extend_order = Vec::with_capacity(n_ancestors);
	for _ in 0..n_ancestors {
		current = chain.extend(current);
		ancestors_in_extend_order.push(current);
	}
	let leaf = chain.extend(current);
	let leaf_number = chain.block(&leaf).unwrap().number;

	// Apply per-block claim queue overrides. `LeafSelector::Ancestor(0)` is the leaf's
	// direct parent — i.e. the *last* extend before the leaf.
	for (selector, queue) in config.claim_queue_overrides {
		let target = match selector {
			LeafSelector::Leaf => leaf,
			LeafSelector::Ancestor(n) => {
				let idx = ancestors_in_extend_order.len().checked_sub(1 + n).expect(
					"ChainConfig: Ancestor index out of range; n_ancestors too small",
				);
				ancestors_in_extend_order[idx]
			},
		};
		chain.set_claim_queue_at(target, queue);
	}

	let chain = SharedChain::new(chain);

	let mut responder = LayeredResponder::new();
	responder.push(chain.clone());
	responder.push(PanicResponder);

	let mut sim = Sim::<S>::start(SimConfig::default(), responder);
	let (psp, psp_rx) = ProspectiveParachainsAux::spawn(&mut sim);
	sim.register_aux(psp, psp_rx);

	// Either install a CanSecond stub (registered FIRST so it wins the slot order against
	// any later backing aux) or spawn real candidate-backing.
	if let Some(verdict) = config.can_second_stub {
		sim.register_aux_slot_only(crate::aux::CanSecondStub::new(verdict));
	} else {
		let (cb, cb_rx) = CandidateBackingAux::spawn(&mut sim);
		sim.register_aux(cb, cb_rx);
	}

	let cv = CandidateValidationStub::always_valid(&mut sim);
	let av = AvailabilityStoreStub::spawn(&mut sim);
	sim.register_aux_slot_only(cv);
	sim.register_aux_slot_only(av);
	sim.register_aux_slot_only(StatementDistributionNoop::new());
	sim.register_aux_slot_only(ProvisionerNoop::new());
	sim.register_aux_slot_only(AvailabilityDistributionNoop::new());

	sim.signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
		leaf,
		leaf_number,
	))));
	sim.send(CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::OurViewChange(
		polkadot_node_network_protocol::OurView::new(std::iter::once(leaf), 0),
	)));

	// Reverse ancestors so ancestors[0] = leaf's parent.
	let mut ancestors = ancestors_in_extend_order;
	ancestors.reverse();
	WithAncestorsWorld { sim, leaf, ancestors, chain }
}

