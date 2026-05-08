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

//! Reputation **emission** divergence: legacy emits `NetworkBridgeTx::ReportPeer` (a
//! bus event), experimental updates a persistent rep store directly and emits no bus
//! event. The actual rejection happens on both sides — only the *signal* differs.
//!
//! # Why divergent?
//!
//! The two impls share the same spec: misbehaving peers should be penalised, well-behaving
//! ones rewarded. Legacy implements this by sending `NetworkBridgeTxMessage::ReportPeer`
//! to the network bridge subsystem (which the test framework mocks, so the assertion is
//! "did the message get sent?"). Experimental keeps a persistent score table per
//! `(PeerId, ParaId)` and updates that table directly in `state.rs` /
//! `peer_manager/mod.rs` — there is no equivalent bus message.
//!
//! Both architectures reject the misbehaving peer's candidate (the regression suite tests
//! that via `expect_no_second` / `expect_no_fetch`). Only the *observability* of the
//! rejection signal differs.
//!
//! # Why this matters
//!
//! The scoring rules and the inputs that move scores aren't quite the same either:
//!
//! - **Legacy** slashes a wide range of misbehaviors (`COST_UNNEEDED_COLLATOR`,
//!   `COST_UNEXPECTED_MESSAGE`, `COST_INVALID_SIGNATURE`, …). Every slash is a
//!   `ReportPeer` bus event.
//! - **Experimental** only slashes on *outcomes*: `FAILED_FETCH_SLASH` (10922) and
//!   `INVALID_COLLATION_SLASH` (32767) — see `validator_side_experimental/common.rs`.
//!   Cheap-to-fake misbehaviors (declare wrong para, advertise spam) cost the peer
//!   nothing on the rep ledger; the validator just disconnects or drops the
//!   advertisement silently. Comment at `state.rs:261-262`: "advertisements are cheap …
//!   not worth affecting reputations."
//!
//! The behavioural side of experimental's rep model — score-driven fetch ordering, the
//! 300ms penalty box for fresh peers, slot eviction by score — is tested in
//! [`super::reputation_behavior`].
//!
//! # Test layout
//!
//! Each scenario has two filtered variants sitting side-by-side: `__bus_event` (only on
//! legacy) and `__silent` (only on experimental). They share a setup helper so the
//! divergence is purely the assertion.

use crate::{
	builders::{Candidate, Peer, ProtocolVersion::V2},
	contract::{Effect, RepBucket},
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_node_subsystem_util::reputation::REPUTATION_CHANGE_INTERVAL;
use polkadot_primitives::{
	CandidateHash, CandidateReceiptV2, CoreIndex, Hash, HeadData, Id as ParaId,
	MutateDescriptorV2, PersistedValidationData,
};
use polkadot_primitives_test_helpers::dummy_committed_candidate_receipt_v2;
use std::time::Duration;

const PARA_A: ParaId = ParaId::new(2000);
const WRONG: ParaId = ParaId::new(69);

fn empty_parent_pvd(relay_parent_number: u32) -> PersistedValidationData {
	PersistedValidationData {
		parent_head: HeadData(Vec::new()),
		relay_parent_number,
		relay_parent_storage_root: Hash::zero(),
		max_pov_size: 5 * 1024 * 1024,
	}
}

// ---------------------------------------------------------------------------
// Scenario 1: response with mismatched candidate hash → Malicious on legacy
// ---------------------------------------------------------------------------

/// Shared setup for the mismatched-candidate-hash scenario. Returns the world and the
/// peer for impl-specific assertion. The actual *spec* (no `SecondCandidate` for the bad
/// candidate) is asserted by `response_sanity_check::response_with_mismatched_candidate
/// _hash_rejects` in the regular regression suite.
fn setup_mismatched_hash<S: CollatorSut>() -> (crate::scenarios::shared::World<S>, Peer) {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA_A)]);
	let pvd = empty_parent_pvd(w.leaf_number());
	let mut actual = Candidate::for_para_at(PARA_A, w.leaf());
	actual.receipt.descriptor.set_persisted_validation_data_hash(pvd.hash());

	let peer = w.declared_peer(PARA_A, V2);
	let advertised_hash = CandidateHash(Hash::repeat_byte(0xFE));
	w.advertise_with_parent_head(&peer, w.leaf(), advertised_hash, HeadData(Vec::new()).hash());
	let request_id = w.expect_fetch_for_hash(advertised_hash);
	w.respond_fetch_v2(request_id, actual.receipt.clone(), Candidate::empty_pov());
	(w, peer)
}

#[crate::sim_test(only = "legacy")]
fn mismatched_hash_emits_malicious_bus_event<S: CollatorSut>() {
	let (mut w, peer) = setup_mismatched_hash::<S>();
	w.expect_rep(&peer, RepBucket::Malicious);
}

#[crate::sim_test(only = "experimental")]
fn mismatched_hash_no_bus_event<S: CollatorSut>() {
	let (mut w, peer) = setup_mismatched_hash::<S>();
	// Experimental does not emit a bus event; the rep store is updated silently.
	w.expect_no_rep(&peer, RepBucket::Malicious);
}

// ---------------------------------------------------------------------------
// Scenario 2: declare twice for unneeded para → batched Performance on legacy
// ---------------------------------------------------------------------------

/// On legacy this exercises the `ReputationAggregator` batching: two `COST_UNNEEDED
/// _COLLATOR` (CostMinor) hits are buffered, then flushed as one Batch after
/// `REPUTATION_CHANGE_INTERVAL`. Experimental has no equivalent code path —
/// `COST_UNNEEDED_COLLATOR` doesn't exist on experimental at all (see comment at
/// `validator_side_experimental/state.rs:261-262`); the peer is just disconnected on
/// the wrong-para Declare. Different mechanism entirely; same observable outcome
/// (peer doesn't get to keep advertising).
fn setup_declare_twice_unneeded<S: CollatorSut>() -> (crate::scenarios::shared::World<S>, Peer) {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA_A)]);
	let peer = Peer::new(WRONG, V2);
	w.sim.send(peer.connected());
	w.sim.send(peer.declare());
	w.sim.send(peer.declare());
	(w, peer)
}

#[crate::sim_test(only = "legacy")]
fn declare_twice_unneeded_emits_one_batched_rep<S: CollatorSut>() {
	let (mut w, peer) = setup_declare_twice_unneeded::<S>();

	// Buffered until the flush.
	w.sim.expect_count(
		|e| matches!(
			e,
			Effect::Reputation { peer: p, bucket: RepBucket::Performance } if *p == peer.peer_id,
		),
		0,
		"no Reputation::Performance before the aggregator flushes",
	);

	// Advance past the flush interval; aggregator dispatches one Batch.
	w.sim.advance(REPUTATION_CHANGE_INTERVAL + Duration::from_secs(1));
	w.sim.expect_count(
		|e| matches!(
			e,
			Effect::Reputation { peer: p, bucket: RepBucket::Performance } if *p == peer.peer_id,
		),
		1,
		"exactly one batched Reputation::Performance for the unneeded-para peer",
	);
}

#[crate::sim_test(only = "experimental")]
fn declare_twice_unneeded_no_rep_event<S: CollatorSut>() {
	let (mut w, peer) = setup_declare_twice_unneeded::<S>();

	// No rep event ever fires on experimental for "wrong para" misbehaviour — that's
	// just disconnect-without-slash on this side. Advance well past any flush window
	// the legacy side would have used and confirm silence.
	w.sim.advance(REPUTATION_CHANGE_INTERVAL + Duration::from_secs(1));
	w.sim.expect_count(
		|e| matches!(
			e,
			Effect::Reputation { peer: p, .. } if *p == peer.peer_id,
		),
		0,
		"experimental does not slash on `wrong para Declare`; rep is bus-silent",
	);
}

// ---------------------------------------------------------------------------
// Scenario 3: V2 candidate with wrong session_index → Malicious on legacy
// ---------------------------------------------------------------------------

/// Shared setup. Spec is "candidate rejected" (asserted in
/// `v3_session_index_checks::v2_descriptor_with_wrong_session_index_rejects`).
fn setup_wrong_session_index<S: CollatorSut>() -> (crate::scenarios::shared::World<S>, Peer) {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA_A)]);
	let pvd = empty_parent_pvd(w.leaf_number());
	let mut committed = dummy_committed_candidate_receipt_v2(w.leaf());
	committed.descriptor.set_para_id(PARA_A);
	committed.descriptor.set_persisted_validation_data_hash(pvd.hash());
	committed.descriptor.set_core_index(CoreIndex(0));
	committed.descriptor.set_session_index(999);
	let receipt: CandidateReceiptV2 = committed.to_plain();
	let candidate = Candidate::from_receipt(receipt.clone());

	let peer = w.declared_peer(PARA_A, V2);
	w.advertise_with_parent_head(&peer, w.leaf(), candidate.hash(), HeadData(Vec::new()).hash());
	let request_id = w.fetch_request(&candidate);
	w.respond_fetch_v2(request_id, receipt, Candidate::empty_pov());
	(w, peer)
}

#[crate::sim_test(only = "legacy")]
fn wrong_session_index_emits_malicious_bus_event<S: CollatorSut>() {
	let (mut w, peer) = setup_wrong_session_index::<S>();
	w.expect_rep(&peer, RepBucket::Malicious);
}

#[crate::sim_test(only = "experimental")]
fn wrong_session_index_no_bus_event<S: CollatorSut>() {
	let (mut w, peer) = setup_wrong_session_index::<S>();
	w.expect_no_rep(&peer, RepBucket::Malicious);
}
