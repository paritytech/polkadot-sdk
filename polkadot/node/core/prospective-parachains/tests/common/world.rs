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

//! Prospective-parachains-flavoured `World`. Composes [`WorldBase`] for shared
//! scaffolding and adds prospective-specific fluent verbs (introduce / back / queries).
//!
//! Tests boot a `World` via [`World::start`], populate it via the `HasBase`-provided
//! `activate_leaf` / `activate_leaf_with_parent_hash_fn` / `signal_active_leaves` /
//! `register_leaf_in_chain` methods, then drive prospective-flavoured queries directly
//! through this struct's inherent methods.

use super::ProspectiveParachains;
use futures::channel::oneshot;
use polkadot_node_subsystem::messages::{
	Ancestors, BackableCandidateRef, HypotheticalCandidate, HypotheticalMembership,
	HypotheticalMembershipRequest, IntroduceSecondedCandidateRequest, ParentHeadData,
	ProspectiveParachainsMessage, ProspectiveValidationDataRequest,
};
use polkadot_primitives::{
	CandidateHash, CommittedCandidateReceiptV2 as CommittedCandidateReceipt, CoreIndex, HeadData,
	Hash, Id as ParaId, PersistedValidationData, SessionIndex,
	DEFAULT_SCHEDULING_LOOKAHEAD,
};
use polkadot_subsystem_test_sim::world_base::{HasBase, WorldBase, WorldConfig};

// Re-export `HasBase` so tests' `use ...world::HasBase` brings trait methods
// (`sim_mut`, `chain`, `leaves`, `signal_active_leaves`, `deactivate_leaf`,
// `validation_code_hash`, `session_index`, `min_relay_parent_number_override`) into scope.
pub use polkadot_subsystem_test_sim::world_base::HasBase as WorldExt;
use std::{collections::BTreeMap, sync::Arc};

// Re-exports so existing tests' `use ...world::{TestLeaf, PerParaData, get_parent_hash}`
// paths keep resolving. `TestLeaf` and `TestLeafRef` aliases preserve the in-crate
// prospective tests' naming.
pub use polkadot_subsystem_test_sim::world_base::{
	default_parent_hash as get_parent_hash, LeafConfig as TestLeaf,
	PerParaData,
};

/// Suite-wide default [`WorldConfig`] for prospective scenarios — populates the standard
/// two-para claim queue (`chain_a` on core 0, `chain_b` on core 1, depth =
/// [`DEFAULT_SCHEDULING_LOOKAHEAD`]) and leaves the rest at [`WorldConfig::default`].
/// Tests that need a different shape construct their own [`WorldConfig`] inline.
pub fn default_world_config() -> WorldConfig {
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
	WorldConfig { claim_queue, ..WorldConfig::default() }
}

/// Prospective-parachains-flavoured `World`. Composes [`WorldBase`] for shared
/// scaffolding; adds prospective-specific verbs as inherent methods.
pub struct World {
	/// Shared scaffolding: `Sim`, chain model, leaf bookkeeping. Plus default-impl
	/// methods (`activate_leaf`, etc.) reachable directly via `world.activate_leaf(...)`
	/// through the [`HasBase`] trait.
	pub base: WorldBase<ProspectiveParachains>,
}

impl HasBase for World {
	type Sut = ProspectiveParachains;
	fn base(&self) -> &WorldBase<Self::Sut> {
		&self.base
	}
	fn base_mut(&mut self) -> &mut WorldBase<Self::Sut> {
		&mut self.base
	}
}

impl World {
	/// Start a new world from a [`WorldConfig`]. No leaves active until
	/// [`HasBase::activate_leaf`] (or a sibling) is called. Mid-test config / chain
	/// changes go through `world.base.chain.lock()` (e.g. `add_session`,
	/// `set_claim_queue_at`); the [`WorldConfig`] copy on [`WorldBase::config`] stays
	/// frozen as the activation defaults.
	pub fn start(config: WorldConfig) -> Self {
		Self { base: WorldBase::<ProspectiveParachains>::start(config) }
	}

	// =====================================================================================
	// Prospective-flavoured fluent verbs. These don't fit on `WorldBase` because they
	// drive `ProspectiveParachainsMessage` and decode prospective-shaped replies.
	// =====================================================================================

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
		self.base.sim.send(ProspectiveParachainsMessage::IntroduceSecondedCandidate(req, tx));
		rx.now_or_never_ok()
			.expect("subsystem replied to IntroduceSecondedCandidate before parking")
	}

	/// Drive `CandidateBacked`. Fire-and-forget — no reply.
	pub fn back_candidate(&mut self, para: ParaId, candidate_hash: CandidateHash) {
		self.base
			.sim
			.send(ProspectiveParachainsMessage::CandidateBacked(para, candidate_hash));
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
		self.base.sim.send(ProspectiveParachainsMessage::GetBackableCandidates {
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
		self.base
			.sim
			.send(ProspectiveParachainsMessage::GetHypotheticalMembership(request, tx));
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
		self.base
			.sim
			.send(ProspectiveParachainsMessage::GetProspectiveValidationData(request, tx));
		rx.now_or_never_ok().expect("subsystem replied to GetProspectiveValidationData")
	}
}

/// Helper macro mirroring the in-crate test's `make_and_back_candidate!`. Reads the
/// validation-code hash off `$world` (via the [`HasBase`]-trait accessor) so callers
/// don't have to thread a separate state argument through.
#[macro_export]
macro_rules! make_and_back_candidate {
	($world:ident, $leaf:ident, $parent:expr, $index:expr) => {{
		use polkadot_primitives::MutateDescriptorV2;
		let (mut candidate, pvd) = polkadot_primitives_test_helpers::make_candidate(
			$leaf.hash,
			$leaf.number,
			polkadot_primitives::Id::from(1),
			$parent.commitments.head_data.clone(),
			polkadot_primitives::HeadData(vec![$index]),
			$world.validation_code_hash(),
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

/// Helper to extract a oneshot::Receiver value the harness has settled into ready state.
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
