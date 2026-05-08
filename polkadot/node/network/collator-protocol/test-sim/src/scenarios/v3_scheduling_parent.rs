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
//! KNOWN-FAILING on experimental for the same reason as the response-sanity-check
//! family — bus-silent reputation handling under the persistent rep DB rewrite.
//! See `project_collator_experimental_no_invalid_reputation_event.md`.

use crate::{
	builders::ProtocolVersion::V3,
	contract::{Effect, RepBucket},
	harness::CollatorSut,
	scenarios::shared::{
		build_with_ancestors_world_with_config, ChainConfig,
	},
};
use polkadot_node_subsystem_util::reputation::REPUTATION_CHANGE_INTERVAL;
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

/// Stalled relay chain: leaf at slot 1 (genesis_slot=0, +1 extend = leaf), validator's
/// `current_slot` advanced to slot 10. V3 advertisement with `scheduling_parent = leaf` is
/// rejected because `leaf.slot + 1 = 2 ≠ current_slot = 10`.
///
/// KNOWN-FAILING (experimental): per `project_collator_experimental_no_invalid_reputation_event.md` —
/// rejection silent on the bus.
#[crate::sim_test]
fn v3_scheduling_parent_rejected_on_stalled_relay_chain<S: CollatorSut>() {
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), crate::chain::CoreSchedule::always(PARA_A))
		.with_genesis_slot(Slot::from(0)) // leaf lands at slot 1
		.with_v3_descriptors_enabled();
	let mut w = build_with_ancestors_world_with_config::<S>(0, config);

	// Walk the validator's current_slot far ahead of leaf.slot=1.
	w.sim.advance(slot_to_wall_ms(10));

	let pvd = PersistedValidationData {
		parent_head: HeadData(Vec::new()),
		relay_parent_number: w.leaf_number(),
		relay_parent_storage_root: Hash::zero(),
		max_pov_size: 5 * 1024 * 1024,
	};
	let mut committed = dummy_committed_candidate_receipt_v3(w.leaf(), w.leaf());
	committed.descriptor.set_para_id(PARA_A);
	committed.descriptor.set_persisted_validation_data_hash(pvd.hash());
	committed.descriptor.set_core_index(CoreIndex(0));
	committed.descriptor.set_session_index(0);
	committed.descriptor.set_version(1);
	let receipt: CandidateReceiptV2 = committed.to_plain();
	let candidate_hash = receipt.hash();

	let peer = w.declared_peer(PARA_A, V3);
	w.advertise_v3(
		&peer,
		w.leaf(), // scheduling_parent = leaf
		w.leaf(),
		candidate_hash,
		HeadData(Vec::new()).hash(),
		CandidateDescriptorVersion::V3,
	);

	// `COST_INVALID_SCHEDULING_PARENT` is `CostMinor` → `Performance` bucket; flushed
	// via `ReputationAggregator` (30s interval).
	w.sim.advance(REPUTATION_CHANGE_INTERVAL + Duration::from_secs(1));
	let _ = w.sim.expect(
		|e| matches!(
			e,
			Effect::Reputation { peer: p, bucket: RepBucket::Performance } if *p == peer.peer_id,
		),
		Duration::from_millis(500),
		"Effect::Reputation Performance for V3 ad on stalled relay chain",
	);

	// And no fetch fires — rejection at the advertisement gate.
	w.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		0,
		"SendRequest after V3 stalled-slot rejection (must be zero)",
	);
}

/// In-progress slot: leaf.slot == current_slot. V3 advertisement with
/// `scheduling_parent = leaf-parent` (slot = current_slot - 1) is accepted.
///
/// Setup: `genesis_slot=0`, 1 ancestor + leaf so leaf.slot=2. Advance clock to slot 2.
#[crate::sim_test]
fn v3_scheduling_parent_in_progress_slot_accepts_leaf_parent<S: CollatorSut>() {
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), crate::chain::CoreSchedule::always(PARA_A))
		.with_genesis_slot(Slot::from(0))
		.with_v3_descriptors_enabled();
	let mut w = build_with_ancestors_world_with_config::<S>(1, config);

	// leaf.slot = 2 (genesis 0 → ancestor 1 → leaf 2). Set current_slot = 2 (in-progress).
	w.sim.advance(slot_to_wall_ms(2));

	let parent = w.ancestors()[0]; // slot 1 = current_slot - 1

	let pvd = PersistedValidationData {
		parent_head: HeadData(Vec::new()),
		relay_parent_number: w.leaf_number(),
		relay_parent_storage_root: Hash::zero(),
		max_pov_size: 5 * 1024 * 1024,
	};
	let mut committed = dummy_committed_candidate_receipt_v3(w.leaf(), parent);
	committed.descriptor.set_para_id(PARA_A);
	committed.descriptor.set_persisted_validation_data_hash(pvd.hash());
	committed.descriptor.set_core_index(CoreIndex(0));
	committed.descriptor.set_session_index(0);
	committed.descriptor.set_version(1);
	let receipt: CandidateReceiptV2 = committed.to_plain();
	let candidate_hash = receipt.hash();

	let peer = w.declared_peer(PARA_A, V3);
	w.advertise_v3(
		&peer,
		parent, // scheduling_parent = leaf-parent (slot = current_slot - 1)
		w.leaf(),
		candidate_hash,
		HeadData(Vec::new()).hash(),
		CandidateDescriptorVersion::V3,
	);

	let _ = w.sim.expect(
		|e| matches!(
			e,
			Effect::SendRequest { candidate_hash: Some(c), .. } if *c == candidate_hash,
		),
		Duration::from_millis(500),
		"Effect::SendRequest CollationFetching for the V3 advertisement",
	);
}

/// Finished slot: leaf.slot == current_slot - 1. V3 advertisement with
/// `scheduling_parent = leaf` (the just-finished slot's anchor) is accepted.
#[crate::sim_test]
fn v3_scheduling_parent_finished_slot_accepts_leaf<S: CollatorSut>() {
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), crate::chain::CoreSchedule::always(PARA_A))
		.with_genesis_slot(Slot::from(0))
		.with_v3_descriptors_enabled();
	let mut w = build_with_ancestors_world_with_config::<S>(0, config);

	// leaf.slot = 1 (genesis 0 → leaf 1). Set current_slot = 2: leaf.slot + 1 == current_slot.
	w.sim.advance(slot_to_wall_ms(2));

	let pvd = PersistedValidationData {
		parent_head: HeadData(Vec::new()),
		relay_parent_number: w.leaf_number(),
		relay_parent_storage_root: Hash::zero(),
		max_pov_size: 5 * 1024 * 1024,
	};
	// scheduling_parent = leaf. relay_parent = leaf.
	let mut committed = dummy_committed_candidate_receipt_v3(w.leaf(), w.leaf());
	committed.descriptor.set_para_id(PARA_A);
	committed.descriptor.set_persisted_validation_data_hash(pvd.hash());
	committed.descriptor.set_core_index(CoreIndex(0));
	committed.descriptor.set_session_index(0);
	committed.descriptor.set_version(1);
	let receipt: CandidateReceiptV2 = committed.to_plain();
	let candidate_hash = receipt.hash();

	let peer = w.declared_peer(PARA_A, V3);
	w.advertise_v3(
		&peer,
		w.leaf(), // scheduling_parent = leaf (just finished)
		w.leaf(),
		candidate_hash,
		HeadData(Vec::new()).hash(),
		CandidateDescriptorVersion::V3,
	);

	let _ = w.sim.expect(
		|e| matches!(
			e,
			Effect::SendRequest { candidate_hash: Some(c), .. } if *c == candidate_hash,
		),
		Duration::from_millis(500),
		"Effect::SendRequest CollationFetching for V3 ad on finished-slot leaf",
	);
}

/// In-progress slot: targeting the leaf itself as scheduling_parent (instead of leaf-parent)
/// is rejected.
#[crate::sim_test]
fn v3_scheduling_parent_in_progress_slot_rejects_leaf<S: CollatorSut>() {
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), crate::chain::CoreSchedule::always(PARA_A))
		.with_genesis_slot(Slot::from(0))
		.with_v3_descriptors_enabled();
	let mut w = build_with_ancestors_world_with_config::<S>(1, config);

	// leaf.slot=2, current_slot=2 → in-progress. leaf as sched_parent invalid.
	w.sim.advance(slot_to_wall_ms(2));

	let pvd = PersistedValidationData {
		parent_head: HeadData(Vec::new()),
		relay_parent_number: w.leaf_number(),
		relay_parent_storage_root: Hash::zero(),
		max_pov_size: 5 * 1024 * 1024,
	};
	let mut committed = dummy_committed_candidate_receipt_v3(w.leaf(), w.leaf());
	committed.descriptor.set_para_id(PARA_A);
	committed.descriptor.set_persisted_validation_data_hash(pvd.hash());
	committed.descriptor.set_core_index(CoreIndex(0));
	committed.descriptor.set_session_index(0);
	committed.descriptor.set_version(1);
	let receipt: CandidateReceiptV2 = committed.to_plain();
	let candidate_hash = receipt.hash();

	let peer = w.declared_peer(PARA_A, V3);
	w.advertise_v3(
		&peer,
		w.leaf(), // sched_parent = leaf (slot 2 = current_slot, NOT current_slot - 1)
		w.leaf(),
		candidate_hash,
		HeadData(Vec::new()).hash(),
		CandidateDescriptorVersion::V3,
	);

	w.sim.advance(REPUTATION_CHANGE_INTERVAL + Duration::from_secs(1));
	let _ = w.sim.expect(
		|e| matches!(
			e,
			Effect::Reputation { peer: p, bucket: RepBucket::Performance } if *p == peer.peer_id,
		),
		Duration::from_millis(500),
		"Effect::Reputation Performance for V3 in-progress with leaf as sched_parent",
	);
	w.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		0,
		"SendRequest after V3 in-progress leaf-as-sched rejection (must be zero)",
	);
}

/// Finished slot: targeting leaf-parent as sched_parent is rejected. Valid is `leaf`.
#[crate::sim_test]
fn v3_scheduling_parent_finished_slot_rejects_parent<S: CollatorSut>() {
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), crate::chain::CoreSchedule::always(PARA_A))
		.with_genesis_slot(Slot::from(0))
		.with_v3_descriptors_enabled();
	let mut w = build_with_ancestors_world_with_config::<S>(1, config);

	// leaf.slot=2, current_slot=3 → finished. Valid sched_parent = leaf.
	w.sim.advance(slot_to_wall_ms(3));

	let parent = w.ancestors()[0];

	let pvd = PersistedValidationData {
		parent_head: HeadData(Vec::new()),
		relay_parent_number: w.leaf_number(),
		relay_parent_storage_root: Hash::zero(),
		max_pov_size: 5 * 1024 * 1024,
	};
	let mut committed = dummy_committed_candidate_receipt_v3(w.leaf(), parent);
	committed.descriptor.set_para_id(PARA_A);
	committed.descriptor.set_persisted_validation_data_hash(pvd.hash());
	committed.descriptor.set_core_index(CoreIndex(0));
	committed.descriptor.set_session_index(0);
	committed.descriptor.set_version(1);
	let receipt: CandidateReceiptV2 = committed.to_plain();
	let candidate_hash = receipt.hash();

	let peer = w.declared_peer(PARA_A, V3);
	w.advertise_v3(
		&peer,
		parent, // sched_parent = leaf-parent (invalid for finished slot)
		w.leaf(),
		candidate_hash,
		HeadData(Vec::new()).hash(),
		CandidateDescriptorVersion::V3,
	);

	w.sim.advance(REPUTATION_CHANGE_INTERVAL + Duration::from_secs(1));
	let _ = w.sim.expect(
		|e| matches!(
			e,
			Effect::Reputation { peer: p, bucket: RepBucket::Performance } if *p == peer.peer_id,
		),
		Duration::from_millis(500),
		"Effect::Reputation Performance for V3 finished-slot with parent as sched_parent",
	);
	w.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		0,
		"SendRequest after V3 finished-slot parent-as-sched rejection (must be zero)",
	);
}

/// `scheduling_parent` outside the implicit view's allowed ancestry → rejected with
/// `COST_UNEXPECTED_MESSAGE` (Performance bucket).
#[crate::sim_test]
fn v3_scheduling_parent_outside_allowed_ancestry_rejected<S: CollatorSut>() {
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), crate::chain::CoreSchedule::always(PARA_A))
		.with_genesis_slot(Slot::from(0))
		.with_v3_descriptors_enabled();
	let mut w = build_with_ancestors_world_with_config::<S>(0, config);

	let unknown_scheduling_parent = Hash::repeat_byte(0x99);

	let pvd = PersistedValidationData {
		parent_head: HeadData(Vec::new()),
		relay_parent_number: w.leaf_number(),
		relay_parent_storage_root: Hash::zero(),
		max_pov_size: 5 * 1024 * 1024,
	};
	let mut committed = dummy_committed_candidate_receipt_v3(w.leaf(), w.leaf());
	committed.descriptor.set_para_id(PARA_A);
	committed.descriptor.set_persisted_validation_data_hash(pvd.hash());
	committed.descriptor.set_core_index(CoreIndex(0));
	committed.descriptor.set_session_index(0);
	committed.descriptor.set_version(1);
	let receipt: CandidateReceiptV2 = committed.to_plain();
	let candidate_hash = receipt.hash();

	let peer = w.declared_peer(PARA_A, V3);
	w.advertise_v3(
		&peer,
		unknown_scheduling_parent,
		w.leaf(),
		candidate_hash,
		HeadData(Vec::new()).hash(),
		CandidateDescriptorVersion::V3,
	);

	w.sim.advance(REPUTATION_CHANGE_INTERVAL + Duration::from_secs(1));
	let _ = w.sim.expect(
		|e| matches!(
			e,
			Effect::Reputation { peer: p, bucket: RepBucket::Performance } if *p == peer.peer_id,
		),
		Duration::from_millis(500),
		"Effect::Reputation Performance for V3 ad with unknown scheduling parent",
	);
	w.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		0,
		"SendRequest after V3 outside-ancestry rejection (must be zero)",
	);
}
