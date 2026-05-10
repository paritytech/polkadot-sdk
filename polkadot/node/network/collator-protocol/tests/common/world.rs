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

//! Shared world-building primitives. Every scenario boots a [`World`] via one of the
//! `build_*` helpers, then drives the `Sim` through stimuli and assertions on outbound
//! `Effect`s. The fluent surface lives in [`super::world`].

use crate::{
	common::aux::{
		AvailabilityDistributionNoop, AvailabilityStoreStub, CandidateBackingAux,
		CandidateOutputs, CandidateValidationStub, ProspectiveParachainsAux, ProvisionerNoop,
		StatementDistributionNoop,
	},
	common::chain::CoreSchedule,
	common::harness::{LayeredResponder, SubsystemUnderTest},
	common::responder::PanicResponder,
};
use polkadot_subsystem_test_sim::world_base::{
	build_chain_model, HasBase, LeafRef, WorldBase, WorldConfig,
};
use polkadot_node_subsystem::messages::{AllMessages, CollatorProtocolMessage};
use polkadot_node_subsystem_test_helpers::mock::new_leaf;
use polkadot_overseer::ActiveLeavesUpdate;
use polkadot_primitives::{
	CoreIndex, Hash, Id as ParaId, ValidatorIndex,
};
use sp_consensus_slots::Slot;
use std::collections::{BTreeMap, VecDeque};

/// Local alias mirroring the previous `Leaf` shape so existing scenarios that destructure
/// `world.base.leaves[i].{hash, number}` keep working unchanged (the field names match
/// `LeafRef`'s).
pub type Leaf = LeafRef;

/// Collator-flavoured test world. Composes [`WorldBase`] for shared scaffolding (`Sim`,
/// `SharedChain`, leaf bookkeeping) and adds the collator-specific `outputs` registry.
///
/// Scenarios access `Sim` via `world.base.sim`, the chain via `world.base.chain`,
/// leaves via `world.base.leaves`, leaf-derived helpers (`world.leaf()`,
/// `world.ancestors()`) via the [`HasBase`] trait, and direct collator-specific state
/// via `world.outputs`.
pub struct World<S: SubsystemUnderTest>
where
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	/// Shared base: `Sim`, `SharedChain`, leaf bookkeeping. Shared with every other
	/// per-tenant `World` across the workspace via [`WorldBase`] / [`HasBase`].
	pub base: WorldBase<S>,
	/// Validation-stub registry: maps a candidate's hash to the
	/// `(commitments, PVD)` the stub returns when the validator validates that candidate.
	pub outputs: CandidateOutputs,
}

impl<S: SubsystemUnderTest> HasBase for World<S>
where
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	type Sut = S;
	fn base(&self) -> &WorldBase<Self::Sut> {
		&self.base
	}
	fn base_mut(&mut self) -> &mut WorldBase<Self::Sut> {
		&mut self.base
	}
}

/// Re-export `HasBase` so scenarios' `use ...world::WorldExt` brings trait methods
/// (`new_leaf`, `signal_active_leaves`, `deactivate_leaf`, `leaf`, `leaf_number`,
/// `ancestors`, `ancestors_of`, config accessors) into scope.
pub use polkadot_subsystem_test_sim::world_base::HasBase as WorldExt;

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
	build_with_ancestors_world::<S>(0, paras)
}


/// Build a multi-leaf world: extends the chain `n_leaves` times on top of genesis,
/// installs the claim queue at every leaf, signals `ActiveLeaves::start_work` for each
/// leaf in extend order, and pushes an `OurViewChange` containing all of them.
///
/// All leaves form a single linear chain — `leaves[i+1]` is `leaves[i]`'s direct child.
/// Multi-fork scenarios (separate chains under genesis with no shared parent) are not
/// supported; that's a separate extension.
pub fn build_multi_leaf_world<S>(n_leaves: usize, paras: &[(CoreIndex, ParaId)]) -> World<S>
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	let mut config = ChainConfig::default();
	for (core, para) in paras {
		config = config.with_schedule(*core, CoreSchedule::always(*para));
	}
	build_multi_leaf_world_with_config::<S>(n_leaves, config)
}

/// Variant of [`build_multi_leaf_world`] that takes a [`ChainConfig`]. Lets multi-leaf
/// tests dial in `validator_groups`, `group_rotation_frequency`, etc.
pub fn build_multi_leaf_world_with_config<S>(n_leaves: usize, config: ChainConfig) -> World<S>
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	use polkadot_node_subsystem::messages::NetworkBridgeEvent;

	assert!(n_leaves >= 1, "build_multi_leaf_world requires at least one leaf");

	let mut world = bootstrap_world::<S>(&config);
	spawn_default_aux(&mut world, &config);

	// Linear chain: each `new_leaf().activate()` extends the current tip and signals
	// `ActiveLeaves::start_work`. The harness pushes the resulting `LeafRef` onto
	// `world.base.leaves`.
	for _ in 0..n_leaves {
		world.new_block().activate();
	}

	let view: Vec<Hash> = world.base.leaves.iter().map(|l| l.hash).collect();
	world.base.sim.send(CollatorProtocolMessage::NetworkBridgeUpdate(
		NetworkBridgeEvent::OurViewChange(polkadot_node_network_protocol::OurView::new(view, 0)),
	));

	world
}

/// Pre-activation chain configuration. Tests construct this, hand it to
/// [`build_with_ancestors_world_with_config`] (or its sibling), and the helper applies all
/// settings *before* signalling `ActiveLeaves` / `OurViewChange` — so the subsystems'
/// caches see the configured shape, not a default-then-overridden one.
pub struct ChainConfig {
	/// Per-core schedule installed on the chain at startup. Each entry calls
	/// [`crate::common::chain::ChainModel::set_core_schedule`].
	pub schedule: Vec<(CoreIndex, CoreSchedule)>,
	/// Per-block claim-queue overrides. The block referenced here may be either the
	/// leaf returned by the helper or any of its ancestors.
	pub claim_queue_overrides: Vec<(LeafSelector, BTreeMap<CoreIndex, VecDeque<ParaId>>)>,
	/// If `Some(verdict)`, replace real `candidate-backing` with a `CanSecond`-only
	/// stub that always answers with `verdict`. Drops every other `CandidateBacking`
	/// message. Use when a scenario specifically needs a `CanSecond=false` (or `=true`)
	/// verdict that real backing wouldn't produce in our minimal chain shape.
	pub can_second_stub: Option<bool>,
	/// Suite-wide chain/runtime config consumed by the framework's
	/// [`build_chain_model`] / [`WorldBase::start_with_responder`]: validators,
	/// validator groups, genesis slot, V3 node feature flag, etc. Tenants nudge
	/// individual fields rather than the chain-level helpers re-defining them.
	pub world: WorldConfig,
}

impl Default for ChainConfig {
	fn default() -> Self {
		Self {
			schedule: Vec::new(),
			claim_queue_overrides: Vec::new(),
			can_second_stub: None,
			world: WorldConfig {
				// Collator scenarios run their whole synthetic chain in session 0 — the
				// real validator-side infra (validator group rotation, etc.) was tuned
				// against that. Stay there to preserve test semantics.
				session_index: 0,
				validators: crate::common::builders::fixtures::default_validators(),
				validator_groups: vec![vec![ValidatorIndex(0), ValidatorIndex(1)]],
				..WorldConfig::default()
			},
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

	/// Set the slot of the genesis block. Each `chain.extend(...)` bumps the slot by one.
	pub fn with_genesis_slot(mut self, slot: Slot) -> Self {
		self.world.genesis_slot = slot;
		self
	}

	/// Override the validator groups list. Used for multi-core, multi-group tests where
	/// per-block group rotation matters (e.g.
	/// `group_rotation_uses_correct_core_per_relay_parent`).
	pub fn with_validator_groups(mut self, groups: Vec<Vec<ValidatorIndex>>) -> Self {
		self.world.validator_groups = groups;
		self
	}

	/// Override the group rotation frequency.
	pub fn with_group_rotation_frequency(mut self, freq: u32) -> Self {
		self.world.group_rotation_frequency = freq;
		self
	}

	/// Enable the `CandidateReceiptV2` node feature (FeatureIndex 3) on the chain.
	/// Required for any scenario that exercises a V3 candidate descriptor — the
	/// validator gates V3 acceptance on this feature being set in the relay-chain
	/// runtime API's `NodeFeatures` response.
	pub fn with_v3_descriptors_enabled(mut self) -> Self {
		self.world.enable_v3_node_feature = true;
		self
	}
}

/// Identifies a block in the configured chain by its position. Used by
/// [`ChainConfig::with_claim_queue_at`] so tests can install a custom claim queue at
/// either the leaf or one of its ancestors before activation.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // `Ancestor` is part of the public surface; kept for future scenarios.
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
) -> World<S>
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
) -> World<S>
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	use polkadot_node_subsystem::messages::NetworkBridgeEvent;

	let claim_queue_overrides = config.claim_queue_overrides.clone();
	let mut world = bootstrap_world::<S>(&config);
	spawn_default_aux(&mut world, &config);

	// Build the chain: ancestors → leaf. Ancestors are *not* signalled as active
	// leaves — they're just intermediate blocks the chain model answers queries for.
	let mut ancestors_in_extend_order = Vec::with_capacity(n_ancestors);
	{
		let mut chain = world.base.chain.lock();
		let mut current = chain.tip();
		for _ in 0..n_ancestors {
			current = chain.extend(current);
			ancestors_in_extend_order.push(current);
		}
	}

	// Build the leaf without signalling yet — per-leaf claim-queue overrides have to
	// land on the chain BEFORE `ActiveLeaves::start_work` because the subsystem
	// snapshots the schedule at activation. `register_only()` is exactly that:
	// "leaf is on the chain, subsystem doesn't know yet."
	let leaf = world.new_block().register();

	// Per-block claim-queue overrides. `LeafSelector::Ancestor(0)` is the leaf's
	// direct parent — i.e. the *last* extend before the leaf.
	{
		let mut chain = world.base.chain.lock();
		for (selector, queue) in claim_queue_overrides {
			let target = match selector {
				LeafSelector::Leaf => leaf.hash,
				LeafSelector::Ancestor(n) => {
					let idx = ancestors_in_extend_order.len().checked_sub(1 + n).expect(
						"ChainConfig: Ancestor index out of range; n_ancestors too small",
					);
					ancestors_in_extend_order[idx]
				},
			};
			chain.set_claim_queue_at(target, queue);
		}
	}

	world
		.signal_active_leaves(ActiveLeavesUpdate::start_work(new_leaf(leaf.hash, leaf.number)));

	world.base.sim.send(CollatorProtocolMessage::NetworkBridgeUpdate(
		NetworkBridgeEvent::OurViewChange(polkadot_node_network_protocol::OurView::new(
			std::iter::once(leaf.hash),
			0,
		)),
	));

	world
}

/// Bootstrap a `World<S>` from a collator-flavoured [`ChainConfig`]:
/// * delegate chain construction to [`build_chain_model`] (handles validators,
///   groups, genesis slot, runtime API version, V3 node feature);
/// * apply the tenant's per-core schedule on top;
/// * spin up the `Sim` via [`WorldBase::start_with_responder`] (chain + `PanicResponder`).
///
/// Aux subsystems and the `OurViewChange` signal are layered on top by the calling
/// helper via [`spawn_default_aux`] / `world.base.sim.send(...)`.
pub(crate) fn bootstrap_world<S>(config: &ChainConfig) -> World<S>
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	let chain = build_chain_model(&config.world);
	{
		let mut c = chain.lock();
		for (core, schedule) in &config.schedule {
			c.set_core_schedule(*core, schedule.clone());
		}
	}

	let mut responder = LayeredResponder::new();
	responder.push(chain.clone());
	responder.push(PanicResponder);

	let base = WorldBase::<S>::start_with_responder(responder, chain, config.world.clone());
	World { base, outputs: CandidateOutputs::default() }
}

/// Spawn the standard collator-side aux subsystems on `world.base.sim`:
/// real `prospective-parachains`, real `candidate-backing` (or a `CanSecondStub` if
/// `config.can_second_stub` is set), `CandidateValidationStub::always_valid`, an
/// `AvailabilityStoreStub`, and noop stubs for the remaining downstream subsystems.
pub(crate) fn spawn_default_aux<S>(world: &mut World<S>, config: &ChainConfig)
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	let sim = &mut world.base.sim;
	let (psp, psp_rx) = ProspectiveParachainsAux::spawn(sim);
	sim.register_aux(psp, psp_rx);

	// Either install a CanSecond stub (registered FIRST so it wins the slot order
	// against any later backing aux) or spawn real candidate-backing.
	if let Some(verdict) = config.can_second_stub {
		sim.register_aux_slot_only(crate::common::aux::CanSecondStub::new(verdict));
	} else {
		let (cb, cb_rx) = CandidateBackingAux::spawn(sim);
		sim.register_aux(cb, cb_rx);
	}

	let cv = CandidateValidationStub::always_valid(sim, world.outputs.clone());
	let av = AvailabilityStoreStub::spawn(sim);
	sim.register_aux_slot_only(cv);
	sim.register_aux_slot_only(av);
	sim.register_aux_slot_only(StatementDistributionNoop::new());
	sim.register_aux_slot_only(ProvisionerNoop::new());
	sim.register_aux_slot_only(AvailabilityDistributionNoop::new());
}

