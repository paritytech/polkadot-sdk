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

//! V3 scheduling-parent slot validation.
//!
//! Validator computes `current_slot` from `clock.timestamp_millis() / SLOT_DURATION` and
//! accepts a V3 advertisement only when:
//!
//! * `scheduling_parent` is a leaf with `leaf.slot == current_slot - 1` (finished slot —
//!   the leaf is the previous slot's block), or
//! * `scheduling_parent` is the parent of a leaf whose slot equals `current_slot`
//!   (in-progress — the leaf's parent anchors the previous slot).
//!
//! Each test below tunes `genesis_slot` so the leaf lands on a known offset relative to
//! the framework's `MockClock`-derived `current_slot`.
//!
//! Both impls reject the bad advertisements — that's the shared spec asserted here.
//! Legacy additionally emits `Reputation::Performance` for invalid-scheduling-parent
//! advertisements; experimental does not slash on this class of misbehaviour at all.
//! The rep emission divergence is documented in
//! [`crate::scenarios::divergent::reputation_emission`].

use crate::{
	builders::ProtocolVersion::V3,
	contract::Effect,
	harness::CollatorSut,
	scenarios::shared::{
		build_with_ancestors_world_with_config, ChainConfig,
	},
};
use polkadot_primitives::{
	CandidateDescriptorVersion, CandidateReceiptV2, CoreIndex, HeadData, Hash, Id as ParaId,
	MutateDescriptorV2, PersistedValidationData, RELAY_CHAIN_SLOT_DURATION_MILLIS,
};
use polkadot_primitives_test_helpers::dummy_committed_candidate_receipt_v3;
use sp_consensus_slots::Slot;
use std::time::Duration;

const PARA_A: ParaId = ParaId::new(2000);

/// Wall-clock slot the validator sees, given how many ms have elapsed on the MockClock.
/// `MockClock::wall_clock_ms` starts at 0; `Sim::advance(d)` bumps it by `d.as_millis()`.
/// Tests advance the clock by `target_slot * SLOT_DURATION` before issuing a V3
/// advertisement so that validator's `current_slot` lands where the test expects.
fn slot_to_wall_ms(slot: u64) -> Duration {
	Duration::from_millis(slot * RELAY_CHAIN_SLOT_DURATION_MILLIS)
}

/// Build a V3 candidate at `relay_parent` whose scheduling parent is `scheduling_parent`.
/// Returns the receipt + its hash; tests use both for the advertise step.
fn v3_candidate<S: CollatorSut>(
	w: &crate::scenarios::shared::World<S>,
	relay_parent: Hash,
	scheduling_parent: Hash,
) -> (CandidateReceiptV2, polkadot_primitives::CandidateHash) {
	let pvd = PersistedValidationData {
		parent_head: HeadData(Vec::new()),
		relay_parent_number: w.chain.lock().block(&relay_parent).unwrap().number,
		relay_parent_storage_root: Hash::zero(),
		max_pov_size: 5 * 1024 * 1024,
	};
	let mut committed = dummy_committed_candidate_receipt_v3(relay_parent, scheduling_parent);
	committed.descriptor.set_para_id(PARA_A);
	committed.descriptor.set_persisted_validation_data_hash(pvd.hash());
	committed.descriptor.set_core_index(CoreIndex(0));
	committed.descriptor.set_session_index(0);
	committed.descriptor.set_version(1);
	let receipt: CandidateReceiptV2 = committed.to_plain();
	let hash = receipt.hash();
	(receipt, hash)
}

/// Assert validator rejects the V3 advertisement: no `SendRequest` fires within a
/// settle window. The reputation *signal* of the rejection diverges between impls
/// (legacy emits `Reputation::Performance` on the bus; experimental updates the rep
/// store silently or, for cheap-to-fake misbehaviour like a wrong scheduling_parent,
/// applies no slash at all). The shared invariant — and what we assert here — is the
/// no-fetch outcome.
fn assert_rejected<S: CollatorSut>(
	w: &mut crate::scenarios::shared::World<S>,
	_peer_id: sc_network_types::PeerId,
	_context: &'static str,
) {
	// Settle long enough that any in-flight effects from the advertise step have
	// drained, then assert no fetch was emitted for the rejected advertisement.
	w.sim.advance(Duration::from_millis(200));
	w.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		0,
		"SendRequest after V3 rejection (must be zero)",
	);
}

/// Stalled relay chain: leaf at slot 1 (genesis_slot=0, +1 extend = leaf), validator's
/// `current_slot` advanced to slot 10. V3 advertisement with `scheduling_parent = leaf` is
/// rejected because `leaf.slot + 1 = 2 ≠ current_slot = 10`.
///
/// KNOWN-FAILING (experimental): per `project_collator_experimental_no_invalid_reputation_event.md` —
/// rejection silent on the bus.
/// Helper: build a world configured for V3 scheduling-parent tests with `n_ancestors`
/// blocks under the leaf and the wall-clock advanced to `current_slot`.
fn v3_world<S: CollatorSut>(
	n_ancestors: usize,
	current_slot: u64,
) -> crate::scenarios::shared::World<S> {
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), crate::chain::CoreSchedule::always(PARA_A))
		.with_genesis_slot(Slot::from(0))
		.with_v3_descriptors_enabled();
	let mut w = build_with_ancestors_world_with_config::<S>(n_ancestors, config);
	w.sim.advance(slot_to_wall_ms(current_slot));
	w
}

/// Assert validator accepted the V3 advertisement (SendRequest fires for `candidate_hash`).
fn assert_accepted<S: CollatorSut>(
	w: &mut crate::scenarios::shared::World<S>,
	candidate_hash: polkadot_primitives::CandidateHash,
	context: &'static str,
) {
	let _ = w.sim.expect(
		|e| matches!(
			e,
			Effect::SendRequest { candidate_hash: Some(c), .. } if *c == candidate_hash,
		),
		Duration::from_millis(500),
		context,
	);
}

#[crate::sim_test]
fn v3_scheduling_parent_rejected_on_stalled_relay_chain<S: CollatorSut>() {
	// leaf.slot=1; current_slot=10 → leaf.slot+1=2 ≠ 10 → reject.
	let mut w = v3_world::<S>(0, 10);
	let leaf = w.leaf();
	let (_receipt, candidate_hash) = v3_candidate(&w, leaf, leaf);
	let peer = w.declared_peer(PARA_A, V3);
	w.advertise_v3(
		&peer,
		leaf, // scheduling_parent = leaf (stale)
		leaf,
		candidate_hash,
		HeadData(Vec::new()).hash(),
		CandidateDescriptorVersion::V3,
	);
	assert_rejected(
		&mut w,
		peer.peer_id,
		"Effect::Reputation Performance for V3 ad on stalled relay chain",
	);
}

/// In-progress slot: leaf.slot == current_slot. V3 ad with `scheduling_parent = leaf-parent`
/// (slot = current_slot - 1) is accepted.
///
/// KNOWN BUG (experimental): the advertisement is at the leaf's parent (an ancestor RP) —
/// silently dropped on experimental. Same root cause as the ancestor-RP-drop bug. See
/// `memory:project_collator_experimental_no_ancestor_rp_advertise`.
#[crate::sim_test(
	bug_on = "experimental",
	bug_url = "memory:project_collator_experimental_no_ancestor_rp_advertise"
)]
fn v3_scheduling_parent_in_progress_slot_accepts_leaf_parent<S: CollatorSut>() {
	// 1 ancestor → leaf.slot = 2. current_slot = 2 → in-progress.
	let mut w = v3_world::<S>(1, 2);
	let parent = w.ancestors()[0]; // slot 1 = current_slot - 1
	let leaf = w.leaf();
	let (_receipt, candidate_hash) = v3_candidate(&w, leaf, parent);
	let peer = w.declared_peer(PARA_A, V3);
	w.advertise_v3(
		&peer,
		parent, // scheduling_parent = leaf-parent (in-progress's anchor)
		leaf,
		candidate_hash,
		HeadData(Vec::new()).hash(),
		CandidateDescriptorVersion::V3,
	);
	assert_accepted(&mut w, candidate_hash, "SendRequest for in-progress V3 ad");
}

/// Finished slot: leaf.slot == current_slot - 1. V3 ad with `scheduling_parent = leaf`
/// (just-finished anchor) is accepted.
#[crate::sim_test]
fn v3_scheduling_parent_finished_slot_accepts_leaf<S: CollatorSut>() {
	// 0 ancestors → leaf.slot = 1. current_slot = 2 → finished.
	let mut w = v3_world::<S>(0, 2);
	let leaf = w.leaf();
	let (_receipt, candidate_hash) = v3_candidate(&w, leaf, leaf);
	let peer = w.declared_peer(PARA_A, V3);
	w.advertise_v3(
		&peer,
		leaf, // scheduling_parent = leaf (just finished)
		leaf,
		candidate_hash,
		HeadData(Vec::new()).hash(),
		CandidateDescriptorVersion::V3,
	);
	assert_accepted(&mut w, candidate_hash, "SendRequest for V3 ad on finished-slot leaf");
}

/// In-progress slot: targeting leaf itself as scheduling_parent (instead of leaf-parent) is rejected.
#[crate::sim_test]
fn v3_scheduling_parent_in_progress_slot_rejects_leaf<S: CollatorSut>() {
	// 1 ancestor → leaf.slot = 2. current_slot = 2 → in-progress. Leaf as sched_parent invalid.
	let mut w = v3_world::<S>(1, 2);
	let leaf = w.leaf();
	let (_receipt, candidate_hash) = v3_candidate(&w, leaf, leaf);
	let peer = w.declared_peer(PARA_A, V3);
	w.advertise_v3(
		&peer,
		leaf,
		leaf,
		candidate_hash,
		HeadData(Vec::new()).hash(),
		CandidateDescriptorVersion::V3,
	);
	assert_rejected(
		&mut w,
		peer.peer_id,
		"Reputation Performance for V3 in-progress with leaf as sched_parent",
	);
}

/// Finished slot: targeting leaf-parent as sched_parent is rejected. Valid is `leaf`.
#[crate::sim_test]
fn v3_scheduling_parent_finished_slot_rejects_parent<S: CollatorSut>() {
	// 1 ancestor → leaf.slot = 2. current_slot = 3 → finished. Valid sched_parent = leaf.
	let mut w = v3_world::<S>(1, 3);
	let parent = w.ancestors()[0];
	let leaf = w.leaf();
	let (_receipt, candidate_hash) = v3_candidate(&w, leaf, parent);
	let peer = w.declared_peer(PARA_A, V3);
	w.advertise_v3(
		&peer,
		parent, // invalid for finished slot
		leaf,
		candidate_hash,
		HeadData(Vec::new()).hash(),
		CandidateDescriptorVersion::V3,
	);
	assert_rejected(
		&mut w,
		peer.peer_id,
		"Reputation Performance for V3 finished-slot with parent as sched_parent",
	);
}

/// `scheduling_parent` outside the implicit view's allowed ancestry → rejected.
#[crate::sim_test]
fn v3_scheduling_parent_outside_allowed_ancestry_rejected<S: CollatorSut>() {
	let mut w = v3_world::<S>(0, 1);
	let leaf = w.leaf();
	let (_receipt, candidate_hash) = v3_candidate(&w, leaf, leaf);
	let unknown = Hash::repeat_byte(0x99);
	let peer = w.declared_peer(PARA_A, V3);
	w.advertise_v3(
		&peer,
		unknown,
		leaf,
		candidate_hash,
		HeadData(Vec::new()).hash(),
		CandidateDescriptorVersion::V3,
	);
	assert_rejected(
		&mut w,
		peer.peer_id,
		"Reputation Performance for V3 ad with unknown scheduling parent",
	);
}
