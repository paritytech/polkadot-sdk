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

//! Subsystem-agnostic test world base + the [`HasBase`] trait that gives every
//! per-tenant `World` the shared scaffolding (`Sim`, `SharedChain`, leaf bookkeeping)
//! plus default-impl methods for activating / deactivating leaves.
//!
//! Per-tenant consumer crates compose `WorldBase` as a field of their own `World`,
//! impl [`HasBase`] (one accessor function), and gain every base method directly:
//!
//! ```ignore
//! pub struct World {
//!     pub base: WorldBase<MySut>,
//!     pub tenant_specific_field: ...,
//! }
//!
//! impl HasBase for World {
//!     type Sut = MySut;
//!     fn base(&mut self) -> &mut WorldBase<Self::Sut> { &mut self.base }
//! }
//!
//! // Tenant scenarios call `world.activate_leaf(&leaf, &params)` directly via
//! // the trait's default impl. No forwarding code required.
//! ```

use crate::{
	chain::{ChainModel, CoreSchedule, SessionInfo, SharedChain},
	harness::{LayeredResponder, Sim, SimConfig, SubsystemUnderTest},
	responder::PanicResponder,
};
use polkadot_node_subsystem::{
	messages::AllMessages, ActiveLeavesUpdate, OverseerSignal,
};
use polkadot_node_subsystem_test_helpers::mock::new_leaf;
use polkadot_overseer::AssociateOutgoing;
use polkadot_primitives::{
	async_backing::{Constraints, InboundHrmpLimitations},
	BlockNumber, CommittedCandidateReceiptV2 as CommittedCandidateReceipt, CoreIndex, HeadData,
	Hash, Id as ParaId, SessionIndex, ValidationCodeHash, DEFAULT_SCHEDULING_LOOKAHEAD,
};
use polkadot_primitives_test_helpers::dummy_validation_code;
use sp_consensus_slots::Slot;
use std::collections::{BTreeMap, VecDeque};

/// Per-relay-parent, per-para state used when activating a leaf: head-data threaded into
/// candidate construction + the candidates the test wants reported as
/// pending-availability under that leaf.
#[derive(Clone, Debug)]
pub struct PerParaData {
	/// Para's head data at this leaf — drives the synthesised
	/// [`Constraints::required_parent`] for the leaf.
	pub head_data: HeadData,
	/// Candidates pending availability for this para at this leaf — written into
	/// [`ChainModel::set_pending_availability_at`] keyed by `(leaf.hash, para)`.
	pub pending_availability:
		Vec<polkadot_primitives::async_backing::CandidatePendingAvailability>,
}

impl PerParaData {
	/// Per-para data with empty pending-availability.
	pub fn new(head_data: HeadData) -> Self {
		Self { head_data, pending_availability: Vec::new() }
	}

	/// Per-para data with a pre-populated pending-availability list.
	pub fn new_with_pending(
		head_data: HeadData,
		pending: Vec<polkadot_primitives::async_backing::CandidatePendingAvailability>,
	) -> Self {
		Self { head_data, pending_availability: pending }
	}
}

/// Configuration for one leaf the test will activate: hash, number, per-para head-data
/// and pending-availability snapshots. Tenants pass instances of this to
/// [`HasBase::activate_leaf`] / [`HasBase::activate_leaf_with_parent_hash_fn`].
pub struct LeafConfig {
	/// Block number.
	pub number: BlockNumber,
	/// Block hash.
	pub hash: Hash,
	/// Per-para state at this leaf.
	pub para_data: Vec<(ParaId, PerParaData)>,
}

impl LeafConfig {
	/// Look up per-para data by para id. Panics if the para isn't in this leaf's list.
	pub fn para_data(&self, para_id: ParaId) -> &PerParaData {
		self.para_data
			.iter()
			.find_map(|(p, d)| (p == &para_id).then_some(d))
			.expect("para_data: missing para")
	}
}

/// Recorded reference to a leaf the harness has signalled `ActiveLeaves::start_work` for.
#[derive(Clone, Copy, Debug)]
pub struct LeafRef {
	/// Leaf hash.
	pub hash: Hash,
	/// Leaf number.
	pub number: BlockNumber,
}

/// Suite-wide world configuration consumed once at [`WorldBase::start`]. Holds the
/// initial chain/runtime state every leaf activation reads from: session index applied
/// to leaf-registered blocks, suite-wide claim queue installed as the global per-core
/// schedule, validation-code hash baked into synthesised backing constraints, optional
/// `min_relay_parent_number` override, and the runtime API version reported by the
/// chain model. Single source of truth — no duplicate passed on each `activate_leaf`
/// call. Mid-test changes flow directly through [`crate::chain::ChainModel`]
/// (`add_session`, `set_claim_queue_at`, `set_runtime_api_version`, etc.).
#[derive(Clone, Debug)]
pub struct WorldConfig {
	/// Session index applied to every block registered while activating leaves. Mid-test
	/// session changes go through `chain.add_session(...)` + per-block session overrides.
	pub session_index: SessionIndex,
	/// Per-core claim queue installed as the global per-core schedule on
	/// [`WorldBase::start`]. Per-leaf overrides go through
	/// [`crate::chain::ChainModel::set_claim_queue_at`].
	pub claim_queue: BTreeMap<CoreIndex, VecDeque<ParaId>>,
	/// Validation-code hash baked into the synthesised backing constraints — must match
	/// what `make_candidate(.., validation_code_hash)` produces or candidates fail
	/// constraints' `ValidationCodeMismatch` check.
	pub validation_code_hash: ValidationCodeHash,
	/// Optional override for `min_relay_parent_number` in the synthesised backing
	/// constraints. Defaults to `leaf.number - (scheduling_lookahead - 1)`.
	pub min_relay_parent_number_override: Option<BlockNumber>,
	/// Runtime API version reported by the chain model. Lets tests exercise the
	/// `NotSupported` fallback path for APIs the configured runtime does not yet
	/// implement (e.g. `AncestorRelayParentInfo`). Defaults to the highest version
	/// the chain model implements end-to-end.
	pub runtime_api_version: u32,
}

impl Default for WorldConfig {
	fn default() -> Self {
		Self {
			session_index: 1,
			claim_queue: BTreeMap::new(),
			validation_code_hash: dummy_validation_code().hash(),
			min_relay_parent_number_override: None,
			runtime_api_version:
				polkadot_node_subsystem::messages::RuntimeApiRequest::ANCESTOR_RELAY_PARENT_INFO_RUNTIME_REQUIREMENT,
		}
	}
}

/// Subsystem-agnostic shared test-world state: the running `Sim`, the chain model, and
/// the list of leaves the harness has signalled `start_work` for.
///
/// Per-tenant `World` types compose this as a field and impl [`HasBase`] to gain the
/// shared default-impl methods (`activate_leaf`, etc.) directly on `world.foo()`.
pub struct WorldBase<S: SubsystemUnderTest>
where
	AllMessages: From<<S::Message as AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	/// The driving simulation.
	pub sim: Sim<S>,
	/// The chain model — answers Runtime/ChainApi queries and is mutated between
	/// activations to seed pending-availability, claim-queue overrides, etc.
	pub chain: SharedChain,
	/// All leaves the harness has signalled `ActiveLeaves::start_work` for, in
	/// activation order.
	pub leaves: Vec<LeafRef>,
	/// Suite-wide config consumed at activation time (validation-code hash baked into
	/// constraints + optional `min_relay_parent_number` override + session index used
	/// when registering leaf-ancestor blocks). Read-only after `start`; mid-test state
	/// changes go through `chain` directly.
	pub config: WorldConfig,
}

impl<S: SubsystemUnderTest> WorldBase<S>
where
	AllMessages: From<<S::Message as AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	/// Start a new world from a [`WorldConfig`]. Seeds the chain with a session at
	/// `config.session_index` (and at session 0 so the genesis-default register-block
	/// path resolves), installs `config.claim_queue` as the global per-core schedule,
	/// and spins up the simulation. No leaves are active until
	/// [`HasBase::activate_leaf`] is called.
	pub fn start(config: WorldConfig) -> Self {
		let mut chain = ChainModel::new(Slot::from(0));
		// Register session info at both `0` (default for genesis-disconnected synthetic
		// ancestors) and `config.session_index` (for blocks the test activates).
		let session_info = SessionInfo {
			validators: Vec::new(),
			validator_groups: Vec::new(),
			group_rotation_info: polkadot_primitives::GroupRotationInfo {
				session_start_block: 0,
				group_rotation_frequency: 1,
				now: 0,
			},
		};
		chain.add_session(0, session_info.clone());
		if config.session_index != 0 {
			chain.add_session(config.session_index, session_info);
		}
		// Apply the suite-wide claim queue as the global per-core schedule.
		for (core, paras) in &config.claim_queue {
			let cycle: Vec<ParaId> = paras.iter().copied().collect();
			if !cycle.is_empty() {
				chain.set_core_schedule(*core, CoreSchedule::cycling(cycle));
			}
		}
		chain.set_runtime_api_version(config.runtime_api_version);
		let chain = SharedChain::new(chain);

		let mut responder = LayeredResponder::new();
		responder.push(chain.clone());
		responder.push(PanicResponder);

		let sim = Sim::<S>::start(SimConfig::default(), responder);
		Self { sim, chain, leaves: Vec::new(), config }
	}

	/// Start a new world with a caller-supplied responder chain. Use when the chain
	/// model isn't enough — typically because a tenant pushes a tenant-specific
	/// `AnswerQuery` layer in front of the chain (e.g. a `CanSecond` script).
	pub fn start_with_responder(
		responder: LayeredResponder,
		chain: SharedChain,
		config: WorldConfig,
	) -> Self {
		let sim = Sim::<S>::start(SimConfig::default(), responder);
		Self { sim, chain, leaves: Vec::new(), config }
	}
}

/// Default parent-hash function: `parent(h) = h + 1` interpreted as low-u64-be. Mirrors
/// the in-crate prospective-parachains test helpers' `get_parent_hash` so synthetic leaf
/// chains line up with the chain model's ancestry walk.
pub fn default_parent_hash(hash: Hash) -> Hash {
	Hash::from_low_u64_be(hash.to_low_u64_be() + 1)
}

/// Synthesise a permissive backing-constraints record. `valid_watermarks = vec![leaf.number]`
/// matches `hrmp_watermark = relay_parent_number` in default `make_candidate(...)` output;
/// `required_parent = head_data` ties acceptance to the leaf-flavoured head_data the test
/// declares.
pub fn synthesise_constraints(
	min_relay_parent_number: BlockNumber,
	valid_watermarks: Vec<BlockNumber>,
	required_parent: HeadData,
	validation_code_hash: ValidationCodeHash,
) -> Constraints {
	const MAX_POV_SIZE: u32 = 1_000_000;
	Constraints {
		min_relay_parent_number,
		max_pov_size: MAX_POV_SIZE,
		max_head_data_size: 20480,
		max_code_size: 1_000_000,
		ump_remaining: 10,
		ump_remaining_bytes: 1_000,
		max_ump_num_per_candidate: 10,
		dmp_remaining_messages: vec![],
		hrmp_inbound: InboundHrmpLimitations { valid_watermarks },
		hrmp_channels_out: vec![],
		max_hrmp_num_per_candidate: 0,
		required_parent,
		validation_code_hash,
		upgrade_restriction: None,
		future_validation_code: None,
	}
}

/// Trait every per-tenant `World` impls. Two accessor methods (`base` + `base_mut`)
/// plus default-impl convenience accessors (`sim_mut`, `chain`, `leaves`) + activation
/// methods that operate via `base_mut`.
///
/// Tenant impl is 4-line boilerplate (identical across all tenants):
///
/// ```ignore
/// impl HasBase for World {
///     type Sut = MySut;
///     fn base(&self) -> &WorldBase<Self::Sut> { &self.base }
///     fn base_mut(&mut self) -> &mut WorldBase<Self::Sut> { &mut self.base }
/// }
/// ```
pub trait HasBase
where
	AllMessages: From<<<Self::Sut as SubsystemUnderTest>::Message as AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<<Self::Sut as SubsystemUnderTest>::Message>,
{
	/// The subsystem-under-test type the `Sim<S>` inside `WorldBase` is parameterised by.
	type Sut: SubsystemUnderTest;

	/// Shared base, immutable.
	fn base(&self) -> &WorldBase<Self::Sut>;

	/// Shared base, mutable.
	fn base_mut(&mut self) -> &mut WorldBase<Self::Sut>;

	// =====================================================================================
	// Leaf-lifecycle helpers. Register the leaf and its ancestor chain on the chain
	// model, seed per-leaf pending-availability + backing constraints, signal `ActiveLeaves`.
	//
	// For direct `Sim` / chain / leaves access, scenarios use `world.base.sim`,
	// `world.base.chain`, `world.base.leaves` — bare field access avoids borrow conflicts
	// in expressions like `world.base.sim.send(peer.advertise(world.leaf(), ...))` where
	// argument evaluation needs an immutable borrow of the world while the receiver expects
	// `&mut`.
	// =====================================================================================

	/// Activate a leaf using the default parent-hash function ([`default_parent_hash`]).
	/// Registers the leaf and its ancestor chain on the chain model, seeds per-leaf
	/// pending-availability + backing constraints, signals
	/// `ActiveLeaves::start_work`, and lets the subsystem settle through its per-leaf
	/// init queries. Reads suite-wide knobs from [`WorldBase::config`].
	fn activate_leaf(&mut self, leaf: &LeafConfig) {
		self.activate_leaf_with_parent_hash_fn(leaf, default_parent_hash);
	}

	/// Activate a leaf with a custom parent-hash function. The function is called for
	/// the leaf hash and each ancestor in turn; tests use this to anchor a leaf to a
	/// specific parent (e.g. a sibling fork sharing a common ancestor).
	fn activate_leaf_with_parent_hash_fn(
		&mut self,
		leaf: &LeafConfig,
		parent_of: impl Fn(Hash) -> Hash,
	) {
		register_leaf_inner(self.base_mut(), leaf, parent_of);
		self.base_mut()
			.sim
			.signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				leaf.hash, leaf.number,
			))));
		self.base_mut().leaves.push(LeafRef { hash: leaf.hash, number: leaf.number });
	}

	/// Deactivate a leaf via `ActiveLeavesUpdate::stop_work`.
	fn deactivate_leaf(&mut self, hash: Hash) {
		self.base_mut()
			.sim
			.signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::stop_work(hash)));
		self.base_mut().leaves.retain(|l| l.hash != hash);
	}

	/// Send a raw `OverseerSignal::ActiveLeaves` update. Used by scenarios that need to
	/// activate + deactivate atomically or send empty updates. The caller must register
	/// each newly-activated leaf on the chain model + per-para constraints beforehand
	/// via [`Self::register_leaf_in_chain`].
	fn signal_active_leaves(&mut self, update: ActiveLeavesUpdate) {
		if let Some(activated) = &update.activated {
			self.base_mut().leaves.push(LeafRef { hash: activated.hash, number: activated.number });
		}
		for hash in update.deactivated.iter() {
			self.base_mut().leaves.retain(|l| l.hash != *hash);
		}
		self.base_mut().sim.signal(OverseerSignal::ActiveLeaves(update));
	}

	/// Register a leaf on the underlying chain model + seed its per-para backing
	/// constraints, *without* sending an `ActiveLeavesUpdate`. Companion to
	/// [`Self::signal_active_leaves`] for tests that drive activation manually.
	fn register_leaf_in_chain(&mut self, leaf: &LeafConfig) {
		register_leaf_inner(self.base_mut(), leaf, default_parent_hash);
	}

	// =====================================================================================
	// Read accessors for suite-wide config — hide field layout of `WorldConfig` so tests
	// don't break when fields are added or moved.
	// =====================================================================================

	/// Validation-code hash baked into the synthesised backing constraints. Reads
	/// [`WorldBase::config`].
	fn validation_code_hash(&self) -> ValidationCodeHash {
		self.base().config.validation_code_hash
	}

	/// Session index applied to every block registered while activating leaves.
	fn session_index(&self) -> SessionIndex {
		self.base().config.session_index
	}

	/// Optional override for `min_relay_parent_number` in the synthesised backing
	/// constraints.
	fn min_relay_parent_number_override(&self) -> Option<BlockNumber> {
		self.base().config.min_relay_parent_number_override
	}
}

/// Shared inner implementation: register leaf + ancestors in the chain model, seed
/// per-RP pending-availability and backing constraints. Used by both `activate_leaf*`
/// and `register_leaf_in_chain`.
fn register_leaf_inner<S: SubsystemUnderTest>(
	base: &mut WorldBase<S>,
	leaf: &LeafConfig,
	parent_of: impl Fn(Hash) -> Hash,
) where
	AllMessages: From<<S::Message as AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	let config = base.config.clone();
	let mut chain = base.chain.lock();
	let ancestry_len = (DEFAULT_SCHEDULING_LOOKAHEAD - 1) as usize;

	// Walk the leaf's ancestor chain, registering each block until we hit a
	// previously-registered hash, the synthetic genesis (parent_hash == zero), or the
	// number-zero floor.
	let mut current_hash = leaf.hash;
	let mut current_number = leaf.number;
	for _ in 0..=ancestry_len + 1 {
		if chain.block(&current_hash).is_some() {
			break;
		}
		if current_number == 0 {
			break;
		}
		let parent_hash = parent_of(current_hash);
		chain.register_block_with_session(
			current_hash,
			parent_hash,
			current_number,
			Some(config.session_index),
		);
		current_hash = parent_hash;
		if current_number == 0 {
			break;
		}
		current_number = current_number.saturating_sub(1);
	}

	// Seed per-leaf-per-para pending availability + synthesise backing constraints.
	let min_relay_parent_number = config
		.min_relay_parent_number_override
		.unwrap_or_else(|| leaf.number.saturating_sub(ancestry_len as u32));
	for (para, data) in &leaf.para_data {
		let receipts: Vec<CommittedCandidateReceipt> = data
			.pending_availability
			.iter()
			.map(|p| CommittedCandidateReceipt {
				descriptor: p.descriptor.clone(),
				commitments: p.commitments.clone(),
			})
			.collect();
		// Always set per-RP (even empty) so sibling-fork tests where one leaf has
		// pending availability and another doesn't don't leak the global table.
		chain.set_pending_availability_at(leaf.hash, *para, receipts);
		let constraints = synthesise_constraints(
			min_relay_parent_number,
			vec![leaf.number],
			data.head_data.clone(),
			config.validation_code_hash,
		);
		chain.set_backing_constraints_at(leaf.hash, *para, constraints);
	}
}
