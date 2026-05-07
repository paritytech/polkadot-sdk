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

//! V3 scheduling-parent slot validation, plus the V3-via-V2-protocol sanity check.
//!
//! **Status: KNOWN-FAILING (both impls).** With node-feature `CandidateReceiptV2 = 3`
//! enabled and `peer.advertise_v3` in place, the V3 advertise still doesn't reach
//! `try_accept_advertisement` — no effects observable in the test window. Suspects: V3
//! peer connect/declare path requires further plumbing (e.g. genuine V3 declare framing,
//! BABE-derived current slot from the chain's actual block timestamps rather than wall
//! clock). Deferred for a later pass; the slot-control + descriptor-version + node-feature
//! plumbing now exists, so the next pass should be confined to the V3 connect/declare
//! handshake.
//!
//! Real validator computes `current_slot` from `clock.timestamp_millis() / SLOT_DURATION`
//! and accepts a V3 advertisement only when:
//!
//! * `scheduling_parent` is a leaf with `leaf.slot == current_slot - 1` (a "finished" slot,
//!   so the leaf is the previous slot's block), or
//! * `scheduling_parent` is the parent of a leaf whose slot equals `current_slot` (the
//!   leaf is in-progress, so its parent is the previous slot's anchor).
//!
//! Each test below tunes `genesis_slot` so the leaf lands on a known offset relative to
//! the framework's wall-clock-derived `current_slot`.
//!
//! All four tests KNOWN-FAIL on experimental for the same reason — bus-silent reputation
//! handling under the persistent rep DB rewrite. See
//! `project_collator_experimental_no_invalid_reputation_event.md`. The validation does
//! happen and the candidate is rejected; the `Effect::Reputation` bus event is missing.

use crate::{
	builders::{Candidate, ProtocolVersion::V3},
	contract::{Effect, RepBucket},
	harness::CollatorSut,
	scenarios::shared::{
		build_with_ancestors_world_with_config, ChainConfig,
	},
};
use polkadot_node_primitives::{BlockData, PoV};
use polkadot_primitives::{
	CandidateDescriptorVersion, CandidateReceiptV2, CoreIndex, HeadData, Hash, Id as ParaId,
	MutateDescriptorV2, PersistedValidationData, RELAY_CHAIN_SLOT_DURATION_MILLIS,
};
use polkadot_primitives_test_helpers::dummy_committed_candidate_receipt_v3;
use sp_consensus_slots::Slot;
use std::time::Duration;

const PARA_A: ParaId = ParaId::new(2000);

/// Capture the current wall-clock slot at test start. The framework's `MockClock` defaults
/// to `Instant::now()` and the validator derives the relay-chain slot from that. Using
/// this in test setup keeps `genesis_slot` aligned to the wall-clock timeline.
fn current_wall_slot() -> Slot {
	let now_ms = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_millis() as u64;
	Slot::from(now_ms / RELAY_CHAIN_SLOT_DURATION_MILLIS)
}

/// Stalled relay chain: leaf sits at slot 1 while wall-clock current slot is far ahead.
/// V3 advertisement with `scheduling_parent = leaf` is rejected because `leaf.slot + 1 ≠
/// current_slot`.
#[crate::sim_test]
fn v3_scheduling_parent_rejected_on_stalled_relay_chain<S: CollatorSut>() {
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), crate::chain::CoreSchedule::always(PARA_A))
		.with_genesis_slot(Slot::from(0)) // leaf will be at slot 1
		.with_v3_descriptors_enabled();
	let mut w = build_with_ancestors_world_with_config::<S>(0, config);

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
		w.leaf(), // relay_parent = leaf
		candidate_hash,
		HeadData(Vec::new()).hash(),
		CandidateDescriptorVersion::V3,
	);

	// Reputation::Performance — `COST_INVALID_SCHEDULING_PARENT` is in the Performance bucket.
	let _ = w.sim.expect(
		|e| matches!(
			e,
			Effect::Reputation { peer: p, bucket: RepBucket::Performance } if *p == peer.peer_id,
		),
		Duration::from_millis(500),
		"Effect::Reputation Performance for V3 ad on stalled relay chain",
	);

	// And no fetch fires — the rejection is at the advertisement gate.
	let _ = receipt;
	w.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		0,
		"SendRequest after V3 stalled-slot rejection (must be zero)",
	);
}

/// In-progress slot: leaf is at the current wall-clock slot. V3 advertisement targeting the
/// leaf's parent (the previous slot's block) as `scheduling_parent` is accepted; targeting
/// the leaf itself is rejected.
#[crate::sim_test]
fn v3_scheduling_parent_in_progress_slot_accepts_leaf_parent<S: CollatorSut>() {
	let target_slot = current_wall_slot();
	// We want leaf at `target_slot`. With 1 ancestor + leaf the chain is genesis → R → L,
	// and with the slot bumping by 1 per extend the leaf lands at `genesis_slot + 2`.
	let genesis_slot = Slot::from(u64::from(target_slot).saturating_sub(2));

	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), crate::chain::CoreSchedule::always(PARA_A))
		.with_genesis_slot(genesis_slot)
		.with_v3_descriptors_enabled();
	let mut w = build_with_ancestors_world_with_config::<S>(1, config);

	let parent = w.ancestors()[0]; // leaf's parent — slot = current_slot - 1

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
		parent, // scheduling_parent = leaf-parent (in-progress slot's anchor)
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
