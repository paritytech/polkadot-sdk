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
	common::chain::{ChainModel, CoreSchedule, SessionInfo, SharedChain},
	common::harness::{LayeredResponder, Sim, SimConfig, SubsystemUnderTest},
	common::responder::PanicResponder,
};
use polkadot_subsystem_test_sim::world_base::{HasBase, LeafRef, WorldBase};
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

/// Local alias mirroring the previous `Leaf` shape so existing scenarios that destructure
/// `world.base.leaves[i].{hash, number}` keep working unchanged (the field names match
/// `LeafRef`'s).
pub type Leaf = LeafRef;

/// Collator-flavoured test world. Composes [`WorldBase`] for shared scaffolding (`Sim`,
/// `SharedChain`, leaf bookkeeping) and adds the collator-specific `outputs` registry.
///
/// Scenarios access `Sim` via `world.base.sim`, the chain via `world.base.chain`, leaves
/// via `world.base.leaves` (all from the [`HasBase`] trait), and direct collator-specific
/// state via `world.outputs` and the inherent `world.leaf()` / `world.ancestors()`
/// helpers.
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

impl<S: SubsystemUnderTest> World<S>
where
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	/// Hash of the first (and, for most scenarios, only) active leaf.
	pub fn leaf(&self) -> Hash {
		self.base.leaves[0].hash
	}

	/// Block number of the first active leaf.
	pub fn leaf_number(&self) -> u32 {
		self.base.leaves[0].number
	}

	/// Walk back `k` blocks from the first leaf. `ancestors()[0]` is the leaf's parent.
	/// Reads [`SharedChain`] on demand — no cached state.
	pub fn ancestors(&self) -> Vec<Hash> {
		self.ancestors_of(0)
	}

	/// Walk back from `leaves[idx]`. Returns up to
	/// [`crate::common::chain::ChainModel`]'s configured `allowed_ancestry_len` blocks. The result
	/// matches what real `prospective-parachains` would return for
	/// `known_allowed_relay_parents_under(leaf)` (excluding the leaf itself).
	pub fn ancestors_of(&self, idx: usize) -> Vec<Hash> {
		let leaf_hash = self.base.leaves[idx].hash;
		let chain = self.base.chain.lock();
		// Use `allowed_ancestry_len + 1` as the depth budget — chain.ancestors(_, k) returns
		// up to k blocks excluding the queried hash; the implicit view typically resolves
		// `allowed_ancestry_len` of those. We lean on the chain to bound at genesis.
		let k = chain.allowed_ancestry_len() as usize + 1;
		chain.ancestors(leaf_hash, k)
	}
}

/// Re-export `HasBase` so scenarios' `use ...world::WorldExt` brings trait methods
/// (`sim_mut`, `chain`, `leaves`, `signal_active_leaves`, `deactivate_leaf`,
/// `activate_leaf*`, `register_leaf_in_chain`) into scope.
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

/// Variant of [`build_multi_leaf_world`] that takes a [`ChainConfig`]. Lets multi-leaf tests
/// dial in `validator_groups`, `group_rotation_frequency`, etc.
pub fn build_multi_leaf_world_with_config<S>(
	n_leaves: usize,
	config: ChainConfig,
) -> World<S>
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	use polkadot_node_subsystem::messages::NetworkBridgeEvent;

	assert!(n_leaves >= 1, "build_multi_leaf_world requires at least one leaf");

	let mut chain = ChainModel::new(config.genesis_slot);
	chain.add_session(
		0,
		SessionInfo {
			validators: crate::common::builders::fixtures::default_validators(),
			validator_groups: config.validator_groups,
			group_rotation_info: GroupRotationInfo {
				session_start_block: 0,
				group_rotation_frequency: config.group_rotation_frequency,
				now: 0,
			},
		},
	);
	for (core, schedule) in config.schedule {
		chain.set_core_schedule(core, schedule);
	}
	if config.enable_v3_node_feature {
		let mut features = polkadot_primitives::NodeFeatures::EMPTY;
		features.resize(4, false);
		features.set(3, true);
		chain.set_node_features(features);
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

	let outputs = CandidateOutputs::default();
	let cv = CandidateValidationStub::always_valid(&mut sim, outputs.clone());
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

	let leaves: Vec<Leaf> = leaves
		.iter()
		.zip(leaf_numbers.iter())
		.map(|(h, n)| Leaf { hash: *h, number: *n })
		.collect();
	World { base: WorldBase { sim, chain, leaves }, outputs }
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
	/// Slot of the genesis block. Each subsequent extend bumps the slot by one. Defaults
	/// to 0. Increase this for V3 tests that need leaf-parent / leaf at specific slots.
	pub genesis_slot: Slot,
	/// Validator groups. Defaults to `vec![vec![ValidatorIndex(0), ValidatorIndex(1)]]`.
	/// Multiple groups + matching `group_rotation_frequency` enables per-block group rotation
	/// across cores. Used by `group_rotation_uses_correct_core_per_relay_parent`.
	pub validator_groups: Vec<Vec<ValidatorIndex>>,
	/// Group rotation frequency. Defaults to 1 (rotates every block). Set to a large number
	/// to keep group 0 stable across an ancestry chain.
	pub group_rotation_frequency: u32,
	/// Set the `CandidateReceiptV2` node feature flag (`FeatureIndex::CandidateReceiptV2 = 3`).
	/// V3 advertisements / descriptors are gated on this; defaults to `false`.
	pub enable_v3_node_feature: bool,
}

impl Default for ChainConfig {
	fn default() -> Self {
		Self {
			schedule: Vec::new(),
			claim_queue_overrides: Vec::new(),
			can_second_stub: None,
			genesis_slot: Slot::from(0),
			validator_groups: vec![vec![ValidatorIndex(0), ValidatorIndex(1)]],
			group_rotation_frequency: 1,
			enable_v3_node_feature: false,
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

	/// Set the slot of the genesis block. Each `extend` bumps the slot by one.
	pub fn with_genesis_slot(mut self, slot: Slot) -> Self {
		self.genesis_slot = slot;
		self
	}

	/// Override the validator groups list. Used for multi-core, multi-group tests where
	/// per-block group rotation matters (e.g.
	/// `group_rotation_uses_correct_core_per_relay_parent`).
	pub fn with_validator_groups(mut self, groups: Vec<Vec<ValidatorIndex>>) -> Self {
		self.validator_groups = groups;
		self
	}

	/// Override the group rotation frequency.
	pub fn with_group_rotation_frequency(mut self, freq: u32) -> Self {
		self.group_rotation_frequency = freq;
		self
	}

	/// Enable the `CandidateReceiptV2` node feature (FeatureIndex 3) on the chain. Required
	/// for any scenario that exercises a V3 candidate descriptor — the validator gates V3
	/// acceptance on this feature being set in the relay-chain runtime API's `NodeFeatures`
	/// response.
	pub fn with_v3_descriptors_enabled(mut self) -> Self {
		self.enable_v3_node_feature = true;
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

	let mut chain = ChainModel::new(config.genesis_slot);
	chain.add_session(
		0,
		SessionInfo {
			validators: crate::common::builders::fixtures::default_validators(),
			validator_groups: config.validator_groups,
			group_rotation_info: GroupRotationInfo {
				session_start_block: 0,
				group_rotation_frequency: config.group_rotation_frequency,
				now: 0,
			},
		},
	);

	// Apply schedule from config.
	for (core, schedule) in config.schedule {
		chain.set_core_schedule(core, schedule);
	}

	if config.enable_v3_node_feature {
		let mut features = polkadot_primitives::NodeFeatures::EMPTY;
		// FeatureIndex::CandidateReceiptV2 = 3
		features.resize(4, false);
		features.set(3, true);
		chain.set_node_features(features);
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
		sim.register_aux_slot_only(crate::common::aux::CanSecondStub::new(verdict));
	} else {
		let (cb, cb_rx) = CandidateBackingAux::spawn(&mut sim);
		sim.register_aux(cb, cb_rx);
	}

	let outputs = CandidateOutputs::default();
	let cv = CandidateValidationStub::always_valid(&mut sim, outputs.clone());
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

	let _ = ancestors_in_extend_order; // ancestors are derived from the chain on demand.
	let leaves = vec![Leaf { hash: leaf, number: leaf_number }];
	World { base: WorldBase { sim, chain, leaves }, outputs }
}

