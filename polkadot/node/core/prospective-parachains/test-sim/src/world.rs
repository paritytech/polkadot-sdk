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

//! Test world for prospective-parachains scenarios.
//!
//! Boots a `Sim::<ProspectiveParachains>` driving the production subsystem against a
//! [`ChainModel`] populated with whatever shape the test wants. Exposes fluent helpers
//! mirroring the existing in-crate test helpers so faithful ports stay short:
//!
//! - [`World::activate_leaf`] — signals `ActiveLeavesUpdate::start_work` and lets the
//!   subsystem settle through its per-leaf init queries.
//! - [`World::introduce_seconded_candidate`] — drives
//!   `ProspectiveParachainsMessage::IntroduceSecondedCandidate`, returns the subsystem's
//!   accept/reject reply.
//! - [`World::back_candidate`] — drives `ProspectiveParachainsMessage::CandidateBacked`.
//! - [`World::get_backable_candidates`] — drives
//!   `ProspectiveParachainsMessage::GetBackableCandidates`, returns the reply.
//! - [`World::get_hypothetical_membership`] — drives the hypothetical-membership query.
//! - [`World::get_pvd`] — drives the prospective-validation-data query.
//!
//! The chain model is exposed via [`World::chain`] for tests that need to mutate it
//! between activations (e.g. to seed pending-availability or override scheduling lookahead
//! at a specific leaf).

use crate::ProspectiveParachains;
use futures::channel::oneshot;
use polkadot_node_subsystem::{
	messages::{
		Ancestors, BackableCandidateRef, HypotheticalCandidate, HypotheticalMembership,
		HypotheticalMembershipRequest, IntroduceSecondedCandidateRequest, ParentHeadData,
		ProspectiveParachainsMessage, ProspectiveValidationDataRequest,
	},
	ActiveLeavesUpdate, OverseerSignal,
};
use polkadot_node_subsystem_test_helpers::mock::new_leaf;
use polkadot_primitives::{
	async_backing::{Constraints, InboundHrmpLimitations},
	BlockNumber, CandidateHash, CommittedCandidateReceiptV2 as CommittedCandidateReceipt, CoreIndex,
	HeadData, Hash, Id as ParaId, PersistedValidationData, SessionIndex, ValidationCodeHash,
	DEFAULT_SCHEDULING_LOOKAHEAD,
};
use polkadot_primitives_test_helpers::dummy_validation_code;

const MAX_POV_SIZE: u32 = 1_000_000;
use polkadot_subsystem_test_sim::{
	chain::{ChainModel, CoreSchedule, SessionInfo, SharedChain},
	harness::{LayeredResponder, Sim, SimConfig},
	responder::PanicResponder,
};
use sp_consensus_slots::Slot;
use std::{
	collections::{BTreeMap, VecDeque},
	sync::Arc,
};

/// Per-para state at a leaf — head_data threaded into candidate construction, plus the
/// `CandidatesPendingAvailability` list. Mirrors the existing in-crate test helpers'
/// `PerParaData`.
#[derive(Clone, Debug)]
pub struct PerParaData {
	/// Para's head data at this leaf.
	pub head_data: HeadData,
	/// Candidates pending availability for this para at this leaf.
	pub pending_availability:
		Vec<polkadot_primitives::async_backing::CandidatePendingAvailability>,
}

impl PerParaData {
	/// New per-para data with empty pending-availability.
	pub fn new(head_data: HeadData) -> Self {
		Self { head_data, pending_availability: Vec::new() }
	}

	/// New per-para data with a pre-populated pending-availability list.
	pub fn new_with_pending(
		head_data: HeadData,
		pending: Vec<polkadot_primitives::async_backing::CandidatePendingAvailability>,
	) -> Self {
		Self { head_data, pending_availability: pending }
	}
}

/// One scenario leaf: hash, block number, per-para data. Mirrors the existing in-crate
/// `TestLeaf` so faithful ports keep using the same shape.
pub struct TestLeaf {
	/// Block number.
	pub number: BlockNumber,
	/// Block hash.
	pub hash: Hash,
	/// Per-para state at this leaf.
	pub para_data: Vec<(ParaId, PerParaData)>,
}

impl TestLeaf {
	/// Look up per-para data by para id. Panics if the para isn't in this leaf's list.
	pub fn para_data(&self, para_id: ParaId) -> &PerParaData {
		self.para_data
			.iter()
			.find_map(|(p, d)| (p == &para_id).then_some(d))
			.expect("para_data: missing para")
	}
}

/// Suite-wide state: claim queue, runtime-api version, validation-code-hash. Tests mutate
/// this directly before activating leaves, mirroring the in-crate `TestState`.
pub struct TestState {
	/// Per-core claim queue, applied to every leaf (overrideable per-leaf via the chain
	/// model directly).
	pub claim_queue: BTreeMap<CoreIndex, VecDeque<ParaId>>,
	/// Validation-code hash baked into every candidate the helpers build.
	pub validation_code_hash: ValidationCodeHash,
	/// Session index applied to every block. Defaults to 1.
	pub session_index: SessionIndex,
	/// Optional override for `min_relay_parent_number` in the synthesised backing
	/// constraints. Defaults to `leaf.number - (scheduling_lookahead - 1)`.
	pub min_relay_parent_number_override: Option<BlockNumber>,
}

impl Default for TestState {
	fn default() -> Self {
		let chain_a = ParaId::from(1);
		let chain_b = ParaId::from(2);
		let mut claim_queue = BTreeMap::new();
		claim_queue.insert(
			CoreIndex(0),
			std::iter::repeat(chain_a).take(DEFAULT_SCHEDULING_LOOKAHEAD as _).collect(),
		);
		claim_queue.insert(
			CoreIndex(1),
			std::iter::repeat(chain_b).take(DEFAULT_SCHEDULING_LOOKAHEAD as _).collect(),
		);
		Self {
			claim_queue,
			validation_code_hash: dummy_validation_code().hash(),
			session_index: 1,
			min_relay_parent_number_override: None,
		}
	}
}

/// Test world for prospective-parachains scenarios. Built via [`World::start`].
pub struct World {
	/// The driving simulation.
	pub sim: Sim<ProspectiveParachains>,
	/// The chain model. Tests mutate it between activations to set pending availability,
	/// claim queue overrides, scheduling lookahead, and so on.
	pub chain: SharedChain,
	/// Currently-active leaves, in activation order.
	pub leaves: Vec<TestLeafRef>,
}

/// Recorded reference to a leaf the test has activated. Test code rarely uses this
/// directly — the original `TestLeaf` value the test held is what scenarios pass to
/// helpers.
#[derive(Clone, Copy, Debug)]
pub struct TestLeafRef {
	/// Leaf hash.
	pub hash: Hash,
	/// Leaf number.
	pub number: BlockNumber,
}

impl World {
	/// Start a new world. Seeds the chain with a default session and the test state's
	/// claim queue, then spins up the simulation. No leaves are active until
	/// [`World::activate_leaf`] is called.
	pub fn start(test_state: &TestState) -> Self {
		let mut chain = ChainModel::new(Slot::from(0));
		// Register session info under both `0` (the default session inherited by
		// register_block when the parent isn't known) and `test_state.session_index`. That
		// way prospective's session-indexed runtime queries resolve regardless of which
		// session the looked-up block ended up tagged with.
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
		if test_state.session_index != 0 {
			chain.add_session(test_state.session_index, session_info);
		}
		// Apply the suite-wide claim queue as the global per-core schedule.
		// `set_core_schedule` cycles a single para per core; for multi-para per core (claim
		// queue depth > 1 with different paras), use `set_claim_queue_at` per leaf instead.
		// For the simple case we just install a uniform schedule per core.
		for (core, paras) in &test_state.claim_queue {
			let cycle: Vec<ParaId> = paras.iter().copied().collect();
			if !cycle.is_empty() {
				chain.set_core_schedule(*core, CoreSchedule::cycling(cycle));
			}
		}
		let chain = SharedChain::new(chain);

		let mut responder = LayeredResponder::new();
		responder.push(chain.clone());
		responder.push(PanicResponder);

		let sim = Sim::<ProspectiveParachains>::start(SimConfig::default(), responder);
		Self { sim, chain, leaves: Vec::new() }
	}

	/// Activate a leaf with a custom parent-hash function. Mirrors the in-crate test's
	/// `activate_leaf_with_parent_hash_fn`. The function is called for the leaf hash and
	/// each ancestor in turn; tests use this to anchor a leaf to a specific parent (e.g.
	/// a sibling fork sharing a common ancestor).
	pub fn activate_leaf_with_parent_hash_fn(
		&mut self,
		leaf: &TestLeaf,
		test_state: &TestState,
		parent_of: impl Fn(Hash) -> Hash,
	) {
		{
			let mut chain = self.chain.lock();
			let ancestry_len = (DEFAULT_SCHEDULING_LOOKAHEAD - 1) as usize;
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
				chain.register_block_with_session(current_hash, parent_hash, current_number, Some(test_state.session_index));
				current_hash = parent_hash;
				if current_number == 0 {
					break;
				}
				current_number = current_number.saturating_sub(1);
			}

			let min_relay_parent_number = test_state
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
				let constraints = dummy_constraints(
					min_relay_parent_number,
					vec![leaf.number],
					data.head_data.clone(),
					test_state.validation_code_hash,
				);
				chain.set_backing_constraints_at(leaf.hash, *para, constraints);
			}
		}

		self.sim
			.signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				leaf.hash, leaf.number,
			))));
		self.leaves.push(TestLeafRef { hash: leaf.hash, number: leaf.number });
	}

	/// Activate a leaf: register it on the chain model, seed pending-availability for each
	/// para, signal `ActiveLeavesUpdate::start_work`, and let prospective settle through
	/// its per-leaf init queries.
	pub fn activate_leaf(&mut self, leaf: &TestLeaf, test_state: &TestState) {
		// Register the leaf and its ancestor chain on the chain model. Prospective walks
		// `(scheduling_lookahead - 1)` ancestors via `ChainApi::Ancestors` plus one
		// `RuntimeApi::SessionIndexForChild` per unique ancestor; the chain model needs to
		// know each ancestor's number.
		{
			let mut chain = self.chain.lock();
			let ancestry_len = (DEFAULT_SCHEDULING_LOOKAHEAD - 1) as usize;
			let mut current_hash = leaf.hash;
			let mut current_number = leaf.number;
			// Insert the leaf itself plus each ancestor (down to genesis).
			for _ in 0..=ancestry_len + 1 {
				if chain.block(&current_hash).is_some() {
					break;
				}
				if current_number == 0 {
					break;
				}
				let parent_hash = get_parent_hash(current_hash);
				let parent_number = current_number.saturating_sub(1);
				chain.register_block_with_session(current_hash, parent_hash, current_number, Some(test_state.session_index));
				current_hash = parent_hash;
				current_number = parent_number;
				if current_number == 0 {
					break;
				}
			}

			// Seed pending availability + per-leaf-per-para backing constraints.
			let ancestry_len = (DEFAULT_SCHEDULING_LOOKAHEAD - 1) as u32;
			let min_relay_parent_number = test_state
				.min_relay_parent_number_override
				.unwrap_or_else(|| leaf.number.saturating_sub(ancestry_len));
			for (para, data) in &leaf.para_data {
				let receipts: Vec<CommittedCandidateReceipt> = data
					.pending_availability
					.iter()
					.map(|p| CommittedCandidateReceipt {
						descriptor: p.descriptor.clone(),
						commitments: p.commitments.clone(),
					})
					.collect();
				chain.set_pending_availability_at(leaf.hash, *para, receipts);
				// Mirror the in-crate test's `dummy_constraints(min_rpn,
				// valid_watermarks=vec![leaf.number], required_parent=head_data, vch)`.
				let constraints = dummy_constraints(
					min_relay_parent_number,
					vec![leaf.number],
					data.head_data.clone(),
					test_state.validation_code_hash,
				);
				chain.set_backing_constraints_at(leaf.hash, *para, constraints);
			}
		}

		self.sim
			.signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				leaf.hash, leaf.number,
			))));

		self.leaves.push(TestLeafRef { hash: leaf.hash, number: leaf.number });

		let _ = test_state; // currently unused; kept in signature for symmetry / future
						 // overrides like min_relay_parent_number_override.
	}

	/// Deactivate a leaf via `ActiveLeavesUpdate::stop_work`.
	pub fn deactivate_leaf(&mut self, hash: Hash) {
		self.sim
			.signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::stop_work(hash)));
		self.leaves.retain(|l| l.hash != hash);
	}

	/// Send a raw `OverseerSignal::ActiveLeaves` update. Used by scenarios that need to
	/// activate + deactivate atomically or send empty updates. The caller must register
	/// each newly-activated leaf on the chain model + per-para constraints beforehand via
	/// [`Self::register_leaf_in_chain`].
	pub fn signal_active_leaves(&mut self, update: ActiveLeavesUpdate) {
		// Track activated leaves so `world.leaves` stays in sync. Caller is responsible
		// for registering blocks on the chain before calling this.
		if let Some(activated) = &update.activated {
			self.leaves.push(TestLeafRef { hash: activated.hash, number: activated.number });
		}
		for hash in update.deactivated.iter() {
			self.leaves.retain(|l| l.hash != *hash);
		}
		self.sim.signal(OverseerSignal::ActiveLeaves(update));
	}

	/// Register a leaf on the underlying chain model + seed its per-para backing
	/// constraints, *without* sending an `ActiveLeavesUpdate`. Companion to
	/// [`Self::signal_active_leaves`] for tests that drive activation manually.
	pub fn register_leaf_in_chain(&mut self, leaf: &TestLeaf, test_state: &TestState) {
		let mut chain = self.chain.lock();
		let ancestry_len = (DEFAULT_SCHEDULING_LOOKAHEAD - 1) as usize;
		let mut current_hash = leaf.hash;
		let mut current_number = leaf.number;
		for _ in 0..=ancestry_len + 1 {
			if chain.block(&current_hash).is_some() {
				break;
			}
			if current_number == 0 {
				break;
			}
			let parent_hash = get_parent_hash(current_hash);
			chain.register_block_with_session(current_hash, parent_hash, current_number, Some(test_state.session_index));
			current_hash = parent_hash;
			if current_number == 0 {
				break;
			}
			current_number = current_number.saturating_sub(1);
		}

		let min_relay_parent_number = test_state
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
			chain.set_pending_availability_at(leaf.hash, *para, receipts);
			let constraints = dummy_constraints(
				min_relay_parent_number,
				vec![leaf.number],
				data.head_data.clone(),
				test_state.validation_code_hash,
			);
			chain.set_backing_constraints_at(leaf.hash, *para, constraints);
		}
	}

	/// Drive `IntroduceSecondedCandidate` and return the subsystem's accept/reject reply.
	pub fn introduce_seconded_candidate(
		&mut self,
		candidate: CommittedCandidateReceipt,
		pvd: PersistedValidationData,
	) -> bool {
		let req = IntroduceSecondedCandidateRequest {
			candidate_para: candidate.descriptor.para_id(),
			candidate_receipt: candidate,
			persisted_validation_data: pvd,
		};
		let (tx, rx) = oneshot::channel();
		self.sim.send(ProspectiveParachainsMessage::IntroduceSecondedCandidate(req, tx));
		// After settling the harness ran the subsystem until parked; the oneshot reply must
		// have arrived.
		rx.now_or_never_ok()
			.expect("subsystem replied to IntroduceSecondedCandidate before parking")
	}

	/// Drive `CandidateBacked`. Fire-and-forget — no reply.
	pub fn back_candidate(&mut self, para: ParaId, candidate_hash: CandidateHash) {
		self.sim.send(ProspectiveParachainsMessage::CandidateBacked(para, candidate_hash));
	}

	/// Drive `GetBackableCandidates` and return the reply.
	pub fn get_backable_candidates(
		&mut self,
		leaf: Hash,
		para_id: ParaId,
		count: u32,
		ancestors: Ancestors,
	) -> Vec<BackableCandidateRef> {
		let (tx, rx) = oneshot::channel();
		self.sim.send(ProspectiveParachainsMessage::GetBackableCandidates {
			leaf,
			para_id,
			count,
			ancestors,
			sender: tx,
		});
		rx.now_or_never_ok().expect("subsystem replied to GetBackableCandidates before parking")
	}

	/// Drive `GetHypotheticalMembership` and return the reply for the single submitted
	/// candidate.
	pub fn get_hypothetical_membership(
		&mut self,
		candidate_hash: CandidateHash,
		receipt: CommittedCandidateReceipt,
		pvd: PersistedValidationData,
	) -> Vec<(HypotheticalCandidate, HypotheticalMembership)> {
		let hypothetical = HypotheticalCandidate::Complete {
			candidate_hash,
			receipt: Arc::new(receipt),
			persisted_validation_data: pvd,
		};
		let request = HypotheticalMembershipRequest {
			candidates: vec![hypothetical],
			fragment_chain_relay_parent: None,
		};
		let (tx, rx) = oneshot::channel();
		self.sim.send(ProspectiveParachainsMessage::GetHypotheticalMembership(request, tx));
		rx.now_or_never_ok().expect("subsystem replied to GetHypotheticalMembership")
	}

	/// Drive `GetProspectiveValidationData` and return the reply.
	pub fn get_pvd(
		&mut self,
		para_id: ParaId,
		candidate_relay_parent: Hash,
		parent_head_data: HeadData,
		session_index: SessionIndex,
	) -> Option<PersistedValidationData> {
		let request = ProspectiveValidationDataRequest {
			para_id,
			candidate_relay_parent,
			session_index,
			parent_head_data: ParentHeadData::OnlyHash(parent_head_data.hash()),
		};
		let (tx, rx) = oneshot::channel();
		self.sim.send(ProspectiveParachainsMessage::GetProspectiveValidationData(request, tx));
		rx.now_or_never_ok().expect("subsystem replied to GetProspectiveValidationData")
	}
}

/// Mirrors the in-crate test's parent-hash function so synthetic leaf chains line up with
/// the chain model's ancestry walk.
pub fn get_parent_hash(hash: Hash) -> Hash {
	Hash::from_low_u64_be(hash.to_low_u64_be() + 1)
}

/// Helper macro mirroring the in-crate test's `make_and_back_candidate!`. Builds a child
/// candidate of `$parent` (a `CommittedCandidateReceipt`), introduces + backs it, and
/// returns `(candidate, candidate_hash)`. The `$index` parameter tags the candidate's
/// `para_head` to keep otherwise-identical receipts unique.
#[macro_export]
macro_rules! make_and_back_candidate {
	($test_state:ident, $world:ident, $leaf:ident, $parent:expr, $index:expr) => {{
		use polkadot_primitives::MutateDescriptorV2;
		let (mut candidate, pvd) = polkadot_primitives_test_helpers::make_candidate(
			$leaf.hash,
			$leaf.number,
			polkadot_primitives::Id::from(1),
			$parent.commitments.head_data.clone(),
			polkadot_primitives::HeadData(vec![$index]),
			$test_state.validation_code_hash,
		);
		candidate
			.descriptor
			.set_para_head(polkadot_primitives::Hash::from_low_u64_le($index));
		let candidate_hash = candidate.hash();
		assert!($world.introduce_seconded_candidate(candidate.clone(), pvd));
		$world.back_candidate(polkadot_primitives::Id::from(1), candidate_hash);
		(candidate, candidate_hash)
	}};
}

/// Mirrors the in-crate test's `dummy_constraints`. Tests pass per-leaf head data as the
/// `required_parent`, and `valid_watermarks = vec![leaf.number]` so candidates whose
/// `hrmp_watermark` equals the relay-parent number pass the check.
fn dummy_constraints(
	min_relay_parent_number: BlockNumber,
	valid_watermarks: Vec<BlockNumber>,
	required_parent: HeadData,
	validation_code_hash: ValidationCodeHash,
) -> Constraints {
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

/// Helper to extract a oneshot::Receiver value that the harness has settled into a ready
/// state (the subsystem replied before parking).
trait OneshotNowOrNever<T> {
	fn now_or_never_ok(self) -> Option<T>;
}

impl<T> OneshotNowOrNever<T> for oneshot::Receiver<T> {
	fn now_or_never_ok(self) -> Option<T> {
		use futures::FutureExt;
		match self.now_or_never() {
			Some(Ok(v)) => Some(v),
			_ => None,
		}
	}
}
