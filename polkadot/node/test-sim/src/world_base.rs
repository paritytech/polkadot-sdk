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
//! plus a fluent leaf builder.
//!
//! Per-tenant consumer crates compose `WorldBase` as a field of their own `World`,
//! impl [`HasBase`] (two accessor functions), and gain every base method directly:
//!
//! ```ignore
//! pub struct World {
//!     pub base: WorldBase<MySut>,
//!     pub tenant_specific_field: ...,
//! }
//!
//! impl HasBase for World {
//!     type Sut = MySut;
//!     fn base(&self) -> &WorldBase<Self::Sut> { &self.base }
//!     fn base_mut(&mut self) -> &mut WorldBase<Self::Sut> { &mut self.base }
//! }
//!
//! // Tenant scenarios call `world.new_leaf().with_head_data(...).activate()` —
//! // single fluent builder. Forks via `.from_parent(prev_leaf.hash)`.
//! ```

use crate::{
	chain::{ChainModel, CoreSchedule, SessionInfo, SharedChain},
	harness::{LayeredResponder, Sim, SimConfig, SubsystemUnderTest},
	responder::PanicResponder,
};
use polkadot_node_subsystem::{messages::AllMessages, ActiveLeavesUpdate, OverseerSignal};
use polkadot_node_subsystem_test_helpers::mock::new_leaf;
use polkadot_overseer::AssociateOutgoing;
use polkadot_primitives::{
	async_backing::{CandidatePendingAvailability, Constraints, InboundHrmpLimitations},
	BlockNumber, CommittedCandidateReceiptV2 as CommittedCandidateReceipt, CoreIndex, HeadData,
	Hash, Id as ParaId, SessionIndex, ValidationCodeHash, DEFAULT_SCHEDULING_LOOKAHEAD,
};
use polkadot_primitives_test_helpers::dummy_validation_code;
use sp_consensus_slots::Slot;
use std::collections::{BTreeMap, VecDeque};

/// Identity of a leaf the harness has signalled `ActiveLeaves::start_work` for. Returned
/// from [`LeafBuilder::activate`] / [`LeafBuilder::register_only`] and held by tests.
#[derive(Clone, Copy, Debug)]
pub struct LeafRef {
	/// Leaf hash.
	pub hash: Hash,
	/// Leaf number.
	pub number: BlockNumber,
}

/// Suite-wide world configuration consumed once at [`WorldBase::start`]. Mid-test
/// changes flow directly through [`crate::chain::ChainModel`] (`add_session`,
/// `set_claim_queue_at`, `set_runtime_api_version`, etc.) — single source of truth for
/// chain/runtime state.
#[derive(Clone, Debug)]
pub struct WorldConfig {
	/// Session index applied to every block produced via `chain.extend(...)`. Mid-test
	/// session changes go through `chain.add_session(...)` + per-block session overrides.
	pub session_index: SessionIndex,
	/// Per-core claim queue installed as the global per-core schedule on
	/// [`WorldBase::start`]. Per-leaf overrides go through
	/// [`crate::chain::ChainModel::set_claim_queue_at`].
	pub claim_queue: BTreeMap<CoreIndex, VecDeque<ParaId>>,
	/// Validation-code hash baked into the synthesised backing constraints — must
	/// match what `make_candidate(.., validation_code_hash)` produces or candidates
	/// fail constraints' `ValidationCodeMismatch` check.
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

/// Subsystem-agnostic shared test-world state: the running `Sim`, the chain model,
/// the activated leaves, and the suite-wide [`WorldConfig`].
///
/// Per-tenant `World` types compose this as a field and impl [`HasBase`] to gain the
/// shared methods (`new_leaf`, `deactivate_leaf`, etc.) directly on `world.foo()`.
pub struct WorldBase<S: SubsystemUnderTest>
where
	AllMessages: From<<S::Message as AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	/// The driving simulation.
	pub sim: Sim<S>,
	/// The chain model — answers Runtime/ChainApi queries. Mutated via
	/// [`LeafBuilder`] (per-leaf head data + pending availability) and direct
	/// `chain.lock()` (mid-test session changes, claim-queue overrides, etc.).
	pub chain: SharedChain,
	/// All leaves the harness has signalled `ActiveLeaves::start_work` for, in
	/// activation order.
	pub leaves: Vec<LeafRef>,
	/// Suite-wide config. Read-only after `start`; mid-test state changes go through
	/// `chain` directly.
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
	/// and spins up the simulation. No leaves are active until [`HasBase::new_leaf`]
	/// is called.
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
		// Align the genesis block's session with the configured world session so that
		// `chain.extend(...)` (which inherits the parent's session) produces blocks in
		// `config.session_index`. Without this, auto-allocated leaves report session 0,
		// out of sync with `world.session_index()`.
		let genesis = chain.genesis();
		chain.set_block_session(genesis, config.session_index);
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

/// Synthesise a permissive backing-constraints record. `valid_watermarks =
/// vec![leaf.number]` matches `hrmp_watermark = relay_parent_number` in default
/// `make_candidate(...)` output; `required_parent = head_data` ties acceptance to the
/// leaf-flavoured head data the test declares.
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
/// plus default-impl convenience methods: leaf builder, leaf deactivation, raw
/// active-leaves signal, suite-wide config accessors.
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

	/// Begin building a new leaf. By default the leaf extends the chain's current
	/// tip via [`ChainModel::extend`]; use [`LeafBuilder::from_parent`] to fork from
	/// a specific block. Per-(rp, para) head data and pending availability accumulate
	/// on the builder and are written to the chain model on `.activate()` /
	/// `.register_only()`.
	fn new_leaf(&mut self) -> LeafBuilder<'_, Self::Sut> {
		LeafBuilder::new(self.base_mut())
	}

	/// Deactivate a leaf via `ActiveLeavesUpdate::stop_work`.
	fn deactivate_leaf(&mut self, hash: Hash) {
		let base = self.base_mut();
		base.sim.signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::stop_work(hash)));
		base.leaves.retain(|l| l.hash != hash);
	}

	/// Send a raw `OverseerSignal::ActiveLeaves` update. Used by scenarios that need
	/// to activate + deactivate atomically or send empty updates. Newly-activated
	/// leaves must first be registered via [`HasBase::new_leaf`]+`.register_only()`
	/// (or by direct `chain.lock()` mutation for the rare pinned-hash case).
	fn signal_active_leaves(&mut self, update: ActiveLeavesUpdate) {
		let base = self.base_mut();
		if let Some(activated) = &update.activated {
			base.leaves.push(LeafRef { hash: activated.hash, number: activated.number });
		}
		for hash in update.deactivated.iter() {
			base.leaves.retain(|l| l.hash != *hash);
		}
		base.sim.signal(OverseerSignal::ActiveLeaves(update));
	}

	// =====================================================================================
	// Read accessors for suite-wide config — hide field layout of `WorldConfig` so tests
	// don't break when fields are added or moved.
	// =====================================================================================

	/// Validation-code hash baked into the synthesised backing constraints.
	fn validation_code_hash(&self) -> ValidationCodeHash {
		self.base().config.validation_code_hash
	}

	/// Session index applied to every block registered by [`LeafBuilder`].
	fn session_index(&self) -> SessionIndex {
		self.base().config.session_index
	}

	/// Optional override for `min_relay_parent_number` in the synthesised backing
	/// constraints.
	fn min_relay_parent_number_override(&self) -> Option<BlockNumber> {
		self.base().config.min_relay_parent_number_override
	}
}

/// Fluent builder for a new leaf. Accumulates per-para head data + pending
/// availability, then on `.activate()` / `.register_only()`:
///
/// 1. Allocates the leaf hash + number via [`ChainModel::extend`] (or honours an
///    explicitly-pinned hash via [`Self::with_hash_and_number`]).
/// 2. Writes per-para head data + pending availability to the chain model.
/// 3. Synthesises permissive backing constraints (using `world.config`'s
///    `validation_code_hash` + optional `min_relay_parent_number_override`) and writes
///    them to the chain model.
/// 4. (`activate` only) Signals `ActiveLeaves::start_work` and pushes the leaf to
///    `world.leaves`.
///
/// The builder is the **only** path through which leaves should normally be created —
/// it keeps the single-source-of-truth invariant (chain state lives on the chain) and
/// removes the manual ancestor-walk that earlier APIs required. The rare test that
/// genuinely needs pinned hashes + an exotic ancestor chain (e.g. session-boundary
/// edge cases) drops directly into `world.base.chain.lock()` + a raw
/// [`HasBase::signal_active_leaves`].
pub struct LeafBuilder<'w, S: SubsystemUnderTest>
where
	AllMessages: From<<S::Message as AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	base: &'w mut WorldBase<S>,
	/// Optional explicit (hash, number). When `None`, the builder extends the chain
	/// at activation time via `chain.extend(parent)`.
	hash_and_number: Option<(Hash, BlockNumber)>,
	/// Optional explicit parent for the auto-extend path. `None` means "extend the
	/// chain's current tip" — the common linear case.
	parent: Option<Hash>,
	/// `(para, head_data)` pairs to write via
	/// `chain.set_backing_constraints_at(leaf, para, ..)` (head data becomes
	/// `Constraints::required_parent`).
	head_data: Vec<(ParaId, HeadData)>,
	/// `(para, pending)` pairs to write via
	/// `chain.set_pending_availability_at(leaf, para, ..)`.
	pending: Vec<(ParaId, Vec<CandidatePendingAvailability>)>,
}

impl<'w, S: SubsystemUnderTest> LeafBuilder<'w, S>
where
	AllMessages: From<<S::Message as AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	fn new(base: &'w mut WorldBase<S>) -> Self {
		Self {
			base,
			hash_and_number: None,
			parent: None,
			head_data: Vec::new(),
			pending: Vec::new(),
		}
	}

	/// Pin a literal hash + number for this leaf instead of having the chain
	/// auto-allocate one. Use only when the test asserts on a specific hash value
	/// (rare). When pinning, the caller is responsible for any required ancestor
	/// registration via direct `chain.lock()` mutation.
	pub fn with_hash_and_number(mut self, hash: Hash, number: BlockNumber) -> Self {
		self.hash_and_number = Some((hash, number));
		self
	}

	/// Fork this leaf from a specific parent block (instead of the chain's current
	/// tip). Only meaningful in the auto-extend path. For sibling-fork tests the
	/// caller passes a previously-activated leaf's hash here; the chain assigns the
	/// new leaf a sibling-distinct hash via the existing sibling-index mechanism.
	pub fn from_parent(mut self, parent: Hash) -> Self {
		self.parent = Some(parent);
		self
	}

	/// Seed the para's head data at the leaf's relay parent. Becomes
	/// `Constraints::required_parent` in the synthesised backing constraints.
	pub fn with_head_data(mut self, para: ParaId, head_data: HeadData) -> Self {
		self.head_data.push((para, head_data));
		self
	}

	/// Seed the para's pending-availability list at the leaf's relay parent.
	pub fn with_pending(
		mut self,
		para: ParaId,
		candidates: Vec<CandidatePendingAvailability>,
	) -> Self {
		self.pending.push((para, candidates));
		self
	}

	/// Finalise the leaf and signal `ActiveLeaves::start_work`. Returns the leaf's
	/// identity.
	pub fn activate(self) -> LeafRef {
		let (base, leaf) = self.flush_to_chain();
		base.sim
			.signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				leaf.hash, leaf.number,
			))));
		base.leaves.push(leaf);
		leaf
	}

	/// Finalise the leaf without signalling `ActiveLeaves::start_work`. Companion
	/// to [`HasBase::signal_active_leaves`] for tests that bundle a leaf activation
	/// with other deactivations into a single update.
	pub fn register_only(self) -> LeafRef {
		let (_base, leaf) = self.flush_to_chain();
		leaf
	}

	/// Resolve the leaf's hash + number (extending the chain if needed) and write
	/// per-para state + synthesised constraints to the chain model. Returns
	/// `&mut WorldBase` so `.activate()` can re-borrow the sim.
	fn flush_to_chain(self) -> (&'w mut WorldBase<S>, LeafRef) {
		let LeafBuilder { base, hash_and_number, parent, head_data, pending } = self;
		let validation_code_hash = base.config.validation_code_hash;
		let min_relay_parent_number_override = base.config.min_relay_parent_number_override;

		let leaf = {
			let mut chain = base.chain.lock();
			let (hash, number) = if let Some((hash, number)) = hash_and_number {
				(hash, number)
			} else {
				let parent = parent.unwrap_or_else(|| chain.tip());
				let hash = chain.extend(parent);
				let number = chain
					.block(&hash)
					.expect("just-extended block must be registered")
					.number;
				(hash, number)
			};
			let ancestry_len = (DEFAULT_SCHEDULING_LOOKAHEAD - 1) as u32;
			let min_relay_parent_number = min_relay_parent_number_override
				.unwrap_or_else(|| number.saturating_sub(ancestry_len));
			for (para, head_data) in &head_data {
				let constraints = synthesise_constraints(
					min_relay_parent_number,
					vec![number],
					head_data.clone(),
					validation_code_hash,
				);
				chain.set_backing_constraints_at(hash, *para, constraints);
			}
			for (para, candidates) in pending {
				let receipts: Vec<CommittedCandidateReceipt> = candidates
					.iter()
					.map(|p| CommittedCandidateReceipt {
						descriptor: p.descriptor.clone(),
						commitments: p.commitments.clone(),
					})
					.collect();
				chain.set_pending_availability_at(hash, para, receipts);
			}
			LeafRef { hash, number }
		};
		(base, leaf)
	}
}
