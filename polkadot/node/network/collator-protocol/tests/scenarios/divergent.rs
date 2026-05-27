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

//! Legacy vs experimental divergences.


mod no_time_based_eviction {
use crate::{
	common::builders::{Peer, ProtocolVersion::V1},
	common::contract::Effect,
	common::harness::CollatorSut,
	common::world::{activated_world, World, WorldExt as _},
};
use polkadot_collator_protocol::CollatorEvictionPolicy;
use polkadot_node_network_protocol::peer_set::PeerSet;
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::time::Duration;

const PARA: ParaId = ParaId::new(2000);

// ---------------------------------------------------------------------------
// Scenario 1: connected-but-undeclared peer
// ---------------------------------------------------------------------------

fn setup_undeclared<S: CollatorSut>() -> (World<S>, Peer) {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let peer = w.connected_peer(PARA, V1);
	(w, peer)
}

#[crate::sim_test(only = "legacy")]
fn undeclared_peer_disconnected_after_window<S: CollatorSut>() {
	let (mut w, peer) = setup_undeclared::<S>();
	w.base.sim.advance(CollatorEvictionPolicy::default().undeclared + Duration::from_millis(500));
	w.expect_disconnect(&peer);
}

#[crate::sim_test(only = "experimental")]
fn undeclared_peer_kept_indefinitely<S: CollatorSut>() {
	let (mut w, peer) = setup_undeclared::<S>();
	// Advance the same distance the legacy variant uses; experimental must not evict.
	let dur = CollatorEvictionPolicy::default().undeclared + Duration::from_millis(500);
	w.expect_no_disconnect(&peer, dur);
}

// ---------------------------------------------------------------------------
// Scenario 2: declared-but-inactive peer
// ---------------------------------------------------------------------------

fn setup_inactive<S: CollatorSut>() -> (World<S>, Peer) {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let peer = w.declared_peer(PARA, V1);
	(w, peer)
}

#[crate::sim_test(only = "legacy")]
fn declared_but_inactive_peer_evicted_after_window<S: CollatorSut>() {
	let (mut w, peer) = setup_inactive::<S>();
	w.base.sim.advance(CollatorEvictionPolicy::default().inactive_collator + Duration::from_secs(1));
	w.expect_disconnect(&peer);
}

#[crate::sim_test(only = "experimental")]
fn declared_but_inactive_peer_kept_indefinitely<S: CollatorSut>() {
	let (mut w, peer) = setup_inactive::<S>();
	let dur = CollatorEvictionPolicy::default().inactive_collator + Duration::from_secs(1);
	w.expect_no_disconnect(&peer, dur);
}

// ---------------------------------------------------------------------------
// Scenario 3: activity extends life (legacy); irrelevant on experimental
// ---------------------------------------------------------------------------

/// On legacy this asserts the activity-resets-timer behaviour: a peer that keeps
/// advertising at sub-window intervals stays connected; once it falls silent, the
/// inactive-collator window kicks in and it gets evicted.
///
/// On experimental there is no inactive-collator window at all (the entire concept is
/// gone), so the "fall silent → eviction" tail has no analogue. Tested via the simpler
/// "declared-but-inactive peer kept indefinitely" above. We document the asymmetry
/// here rather than write a vacuous experimental variant.
#[crate::sim_test(only = "legacy")]
fn activity_extends_life_then_silence_evicts<S: CollatorSut>() {
	use crate::common::chain::CoreSchedule;
	use crate::common::world::{bootstrap_world, collator_world_config};

	// V1 advertisements must reference an *active leaf* (legacy explicitly rejects
	// non-leaf RPs as `ProtocolMisuse`). The original test built a linear chain of
	// three blocks and treated each as an active leaf — production `block_imported`
	// semantics no longer permit that (each child activation deactivates its parent).
	// Three sibling forks of a common non-leaf ancestor preserve the "three coexisting
	// active leaves" intent and let V1 advertisements at all three RPs land.
	let config =
		collator_world_config().with_schedule(CoreIndex(0), CoreSchedule::always(PARA));
	let mut w: World<S> = bootstrap_world::<S>(config, None);
	let common = w.new_block().register();
	let leaf_a = w.new_block().from_parent(common.hash).activate();
	let leaf_b = w.new_block().from_parent(common.hash).activate();
	let leaf_c = w.new_block().from_parent(common.hash).activate();

	let peer = w.declared_peer(PARA, V1);
	let rps = [leaf_a.hash, leaf_b.hash, leaf_c.hash];

	let inactive = CollatorEvictionPolicy::default().inactive_collator;
	let step = inactive * 2 / 3;
	for i in 0..3 {
		w.base.sim.advance(step);
		w.base.sim.send(peer.advertise(rps[i], None, None));
	}

	// After ~2× the window of continuous activity, peer must still be connected.
	w.base.sim.expect_count(
		|e| matches!(
			e,
			Effect::DisconnectPeers { peers, peer_set: PeerSet::Collation } if peers.contains(&peer.peer_id),
		),
		0,
		"DisconnectPeers targeting the actively-advertising peer (must be zero so far)",
	);

	// Fall silent — advance well past the window; peer must be disconnected.
	w.base.sim.advance(inactive + Duration::from_secs(1));
	w.expect_disconnect(&peer);
}
}

mod reputation_behavior {
use crate::{
	common::builders::ProtocolVersion::V2,
	common::contract::Effect,
	common::harness::CollatorSut,
	common::world::activated_world,
};
use crate::common::world::WorldExt as _;
use polkadot_node_subsystem::OverseerSignal;
use polkadot_primitives::{
	CandidateEvent, CoreIndex, GroupIndex, HeadData, Id as ParaId,
};
use std::time::Duration;

const PARA: ParaId = ParaId::new(2000);

/// `MAX_FETCH_DELAY` from `validator_side_experimental/common.rs:113`. Re-exported
/// privately here to avoid a public-API tax on the production crate just for tests.
const MAX_FETCH_DELAY: Duration = Duration::from_millis(300);

/// Higher-scored peer fetches at sim_t = 0 (instant); fresh peer waits the
/// `MAX_FETCH_DELAY` penalty box before its fetch fires.
///
/// The setup ramps peer A's score to 1 via the natural finalization path, then puts A
/// and a fresh peer B in head-to-head competition on the next leaf. Because A's score
/// (1) ≥ `INSTANT_FETCH_REP_THRESHOLD`, A bypasses the delay; B's score (0) is below
/// both the threshold AND the per-para max (= A's 1), so B falls into the 300ms box.
#[crate::sim_test(only = "experimental")]
fn higher_score_peer_fetches_first_fresh_peer_waits_in_penalty_box<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let leaf0 = w.leaf();

	// --- Round 1: peer A advertises + seconds a candidate at leaf0 ---
	let peer_a = w.declared_peer(PARA, V2);
	let cand_a = w
		.candidate_at(leaf0)
		.para(PARA)
		.parent_head(HeadData(Vec::new()))
		.head_data(HeadData(vec![1]))
		.approved_peer(peer_a.peer_id)
		.build();
	w.outputs.insert(cand_a.hash(), cand_a.commitments.clone(), cand_a.pvd.clone());
	w.full_second(&peer_a, &cand_a);

	// --- Drive finalization: candidate is included at leaf0; finalize leaf0 ---
	{
		let mut chain = w.base.chain.lock();
		chain.set_pending_availability(PARA, vec![cand_a.committed()]);
		chain.set_candidate_events(
			leaf0,
			vec![CandidateEvent::CandidateIncluded(
				cand_a.receipt.clone(),
				cand_a.commitments.head_data.clone(),
				CoreIndex(0),
				GroupIndex(0),
			)],
		);
		chain.set_finalized(leaf0);
	}
	w.base.sim.signal(OverseerSignal::BlockFinalized(leaf0, w.leaf_number()));
	// Let the peer_manager finalization handler complete its runtime-API round trip.
	w.base.sim.advance(Duration::from_millis(50));

	// --- Round 2: extend chain, activate new leaf, both peers advertise there ---
	let leaf1 = w.new_block().from_parent(leaf0).activate().hash;

	// Capture sim_t at the moment of advertisement so we can measure the fetch latency
	// against the leaf's activation. (The penalty box delay is measured from
	// `activated_at` of the scheduling parent, not advertisement arrival.)
	let activation_t = w.base.sim.now_sim_t();

	let peer_b = w.declared_peer(PARA, V2); // fresh peer, score = 0
	let cand_a2 = w
		.candidate_at(leaf1)
		.para(PARA)
		.parent_head(cand_a.output_head())
		.head_data(HeadData(vec![2]))
		.build();
	let cand_b = w
		.candidate_at(leaf1)
		.para(PARA)
		.parent_head(cand_a.output_head())
		.head_data(HeadData(vec![3]))
		.build();
	w.outputs.insert(cand_a2.hash(), cand_a2.commitments.clone(), cand_a2.pvd.clone());
	w.outputs.insert(cand_b.hash(), cand_b.commitments.clone(), cand_b.pvd.clone());

	// Both peers advertise immediately at leaf1.
	w.advertise_with_parent_head(&peer_a, leaf1, cand_a2.hash(), cand_a2.parent_head_hash());
	w.advertise_with_parent_head(&peer_b, leaf1, cand_b.hash(), cand_b.parent_head_hash());

	// A's fetch must fire promptly — score ≥ INSTANT_FETCH_REP_THRESHOLD.
	let a_fetch = w.base.sim.expect(
		|e| matches!(
			e,
			Effect::SendRequest { candidate_hash: Some(c), .. } if *c == cand_a2.hash(),
		),
		Duration::from_millis(50),
		"peer A's fetch fires within 50ms (score >= 1 bypasses penalty box)",
	);
	let _ = a_fetch;
	let a_fetch_t = w.base.sim.now_sim_t() - activation_t;
	assert!(
		a_fetch_t < MAX_FETCH_DELAY,
		"peer A's fetch fired at {:?} after leaf activation; expected < {:?}",
		a_fetch_t,
		MAX_FETCH_DELAY,
	);

	// B's fetch must wait at least MAX_FETCH_DELAY (penalty box).
	let _b_fetch = w.base.sim.expect(
		|e| matches!(
			e,
			Effect::SendRequest { candidate_hash: Some(c), .. } if *c == cand_b.hash(),
		),
		MAX_FETCH_DELAY + Duration::from_millis(200),
		"peer B's fetch fires within 500ms (penalty box releases at 300ms)",
	);
	let b_fetch_t = w.base.sim.now_sim_t() - activation_t;
	assert!(
		b_fetch_t >= MAX_FETCH_DELAY,
		"peer B's fetch fired at {:?} after leaf activation; expected >= {:?} (penalty box)",
		b_fetch_t,
		MAX_FETCH_DELAY,
	);
}

/// Sequel to the test above, exercising the slash side of the rep ledger: a peer that
/// gets `FAILED_FETCH_SLASH`-ed loses its priority.
///
/// 1. Ramp peer A AND peer B both to score 1 (two consecutive finalize cycles).
/// 2. At the slashing leaf, A advertises. Validator starts fetching from A. We do not
///    respond → after `MAX_UNSHARED_DOWNLOAD_TIME` the per-fetch deadline expires →
///    A is hit with `FAILED_FETCH_SLASH = 10922`. A's score saturates to 0.
/// 3. At the next leaf, A and B both advertise. A is back in the penalty box
///    (B has score 1, max-for-para = 1, A's = 0); B fetches at sim_t = 0; A's fetch
///    waits `MAX_FETCH_DELAY`.
///
/// Witness: the slash actually demotes fetch priority — not just decrements a counter.
#[crate::sim_test(only = "experimental")]
fn slashed_peer_loses_priority<S: CollatorSut>() {

	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let leaf0 = w.leaf();

	// --- Ramp peer A: full second + finalize at leaf0 ---
	let peer_a = w.declared_peer(PARA, V2);
	let cand_a = w
		.candidate_at(leaf0)
		.para(PARA)
		.parent_head(HeadData(Vec::new()))
		.head_data(HeadData(vec![1]))
		.approved_peer(peer_a.peer_id)
		.build();
	w.outputs.insert(cand_a.hash(), cand_a.commitments.clone(), cand_a.pvd.clone());
	w.full_second(&peer_a, &cand_a);
	{
		let mut chain = w.base.chain.lock();
		chain.set_pending_availability(PARA, vec![cand_a.committed()]);
		chain.set_candidate_events(
			leaf0,
			vec![CandidateEvent::CandidateIncluded(
				cand_a.receipt.clone(),
				cand_a.commitments.head_data.clone(),
				CoreIndex(0),
				GroupIndex(0),
			)],
		);
		chain.set_finalized(leaf0);
	}
	w.base.sim.signal(OverseerSignal::BlockFinalized(leaf0, w.leaf_number()));
	w.base.sim.advance(Duration::from_millis(50));

	// --- Ramp peer B: bypass full_second; seed an "included" candidate and finalize ---
	//
	// We don't need the subsystem to actually second this candidate to bump B's score —
	// experimental's `+VALID_INCLUDED_CANDIDATE_BUMP` path walks finalized-block candidate
	// events and looks up `approved_peer` from the receipt's UMP signals. As long as
	// pending_availability + candidate_events agree on the receipt + parent_rp, the bump
	// fires for whatever peer the receipt names.
	let peer_b = w.declared_peer(PARA, V2);
	let leaf1 = w.new_block().from_parent(leaf0).activate().hash;
	let cand_b_seed = w
		.candidate_at(leaf1)
		.para(PARA)
		.head_data(HeadData(vec![2]))
		.approved_peer(peer_b.peer_id)
		.build();
	{
		let mut chain = w.base.chain.lock();
		chain.set_pending_availability(PARA, vec![cand_b_seed.committed()]);
		chain.set_candidate_events(
			leaf1,
			vec![CandidateEvent::CandidateIncluded(
				cand_b_seed.receipt.clone(),
				cand_b_seed.commitments.head_data.clone(),
				CoreIndex(0),
				GroupIndex(0),
			)],
		);
		chain.set_finalized(leaf1);
	}
	w.base.sim.signal(OverseerSignal::BlockFinalized(leaf1, w.base.leaves[w.base.leaves.len() - 1].number));
	w.base.sim.advance(Duration::from_millis(50));

	// --- Slash leaf: A advertises a new candidate; validator fetches; we don't respond ---
	let leaf2 = w.new_block().from_parent(leaf1).activate().hash;
	let cand_a_slash = w
		.candidate_at(leaf2)
		.para(PARA)
		.parent_head(cand_a.output_head())
		.head_data(HeadData(vec![3]))
		.build();
	w.advertise_with_parent_head(
		&peer_a,
		leaf2,
		cand_a_slash.hash(),
		cand_a_slash.parent_head_hash(),
	);
	let req_id = w.fetch_request(&cand_a_slash);
	// Cancel the fetch — drops the response oneshot, which resolves on the subsystem
	// side as `RequestError::Canceled`. Experimental classifies that as a timeout
	// (`is_timed_out() == true`) and applies `FAILED_FETCH_SLASH` to peer A.
	w.base.sim.cancel_fetch(req_id);
	w.base.sim.advance(Duration::from_millis(50));

	// --- Outcome leaf: A and B both advertise; B wins, A waits in penalty box ---
	let leaf3 = w.new_block().from_parent(leaf2).activate().hash;
	let activation_t = w.base.sim.now_sim_t();
	let cand_a_after = w
		.candidate_at(leaf3)
		.para(PARA)
		.parent_head(cand_a.output_head())
		.head_data(HeadData(vec![4]))
		.build();
	let cand_b_after = w
		.candidate_at(leaf3)
		.para(PARA)
		.parent_head(cand_a.output_head())
		.head_data(HeadData(vec![5]))
		.build();
	w.outputs
		.insert(cand_a_after.hash(), cand_a_after.commitments.clone(), cand_a_after.pvd.clone());
	w.outputs
		.insert(cand_b_after.hash(), cand_b_after.commitments.clone(), cand_b_after.pvd.clone());

	w.advertise_with_parent_head(
		&peer_a,
		leaf3,
		cand_a_after.hash(),
		cand_a_after.parent_head_hash(),
	);
	w.advertise_with_parent_head(
		&peer_b,
		leaf3,
		cand_b_after.hash(),
		cand_b_after.parent_head_hash(),
	);

	// B fetches first — score 1 ≥ INSTANT_FETCH_REP_THRESHOLD.
	let _ = w.base.sim.expect(
		|e| matches!(
			e,
			Effect::SendRequest { candidate_hash: Some(c), .. } if *c == cand_b_after.hash(),
		),
		Duration::from_millis(50),
		"peer B's fetch fires within 50ms (score 1 bypasses penalty box)",
	);
	let b_fetch_t = w.base.sim.now_sim_t() - activation_t;
	assert!(
		b_fetch_t < MAX_FETCH_DELAY,
		"peer B's fetch fired at {:?} after leaf activation; expected < {:?}",
		b_fetch_t,
		MAX_FETCH_DELAY,
	);

	// A's fetch is delayed — score 0 (slashed) and max-for-para = 1.
	let _ = w.base.sim.expect(
		|e| matches!(
			e,
			Effect::SendRequest { candidate_hash: Some(c), .. } if *c == cand_a_after.hash(),
		),
		MAX_FETCH_DELAY + Duration::from_millis(200),
		"peer A's fetch fires within 500ms (penalty box releases at 300ms)",
	);
	let a_fetch_t = w.base.sim.now_sim_t() - activation_t;
	assert!(
		a_fetch_t >= MAX_FETCH_DELAY,
		"peer A's fetch fired at {:?} after leaf activation; expected >= {:?} (slashed → penalty box)",
		a_fetch_t,
		MAX_FETCH_DELAY,
	);
}
}

mod reputation_emission {
use crate::{
	common::builders::{Candidate, Peer, ProtocolVersion::V2},
	common::contract::{Effect, RepBucket},
	common::harness::CollatorSut,
	common::world::activated_world,
};
use crate::common::world::WorldExt as _;
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
fn setup_mismatched_hash<S: CollatorSut>() -> (crate::common::world::World<S>, Peer) {
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
fn setup_declare_twice_unneeded<S: CollatorSut>() -> (crate::common::world::World<S>, Peer) {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA_A)]);
	let peer = Peer::new(WRONG, V2);
	w.base.sim.send(peer.connected());
	w.base.sim.send(peer.declare());
	w.base.sim.send(peer.declare());
	(w, peer)
}

#[crate::sim_test(only = "legacy")]
fn declare_twice_unneeded_emits_one_batched_rep<S: CollatorSut>() {
	let (mut w, peer) = setup_declare_twice_unneeded::<S>();

	// Buffered until the flush.
	w.base.sim.expect_count(
		|e| matches!(
			e,
			Effect::Reputation { peer: p, bucket: RepBucket::Performance } if *p == peer.peer_id,
		),
		0,
		"no Reputation::Performance before the aggregator flushes",
	);

	// Advance past the flush interval; aggregator dispatches one Batch.
	w.base.sim.advance(REPUTATION_CHANGE_INTERVAL + Duration::from_secs(1));
	w.base.sim.expect_count(
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
	w.base.sim.advance(REPUTATION_CHANGE_INTERVAL + Duration::from_secs(1));
	w.base.sim.expect_count(
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
fn setup_wrong_session_index<S: CollatorSut>() -> (crate::common::world::World<S>, Peer) {
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
}

mod upcoming_pr_11967 {
use crate::{
	common::builders::ProtocolVersion::V2,
	common::chain::CoreSchedule,
	common::contract::Effect,
	common::harness::CollatorSut,
	common::world::{bootstrap_world, collator_world_config, World, WorldExt as _},
};
use polkadot_primitives::{CoreIndex, HeadData, Id as ParaId, ValidatorIndex};
use std::time::Duration;

const PARA_A: ParaId = ParaId::new(100);
const PARA_B: ParaId = ParaId::new(600);

/// Group rotation: at leaf 1 (block 1) we own core 2 (PARA_A); at leaf 2 (block 2) we
/// own core 1 (PARA_B). After rotating to leaf 2 a new advertisement for PARA_A at the
/// (now-ancestor) leaf 1 must still fetch — the leaf 1 core's CQ slots are not
/// cancelled by the rotation.
///
/// Pre-#11967: advertisement at the old core silently dropped. Post-#11967: accepted.
#[crate::sim_test(
	bug_on = "experimental",
	bug_url = "github:paritytech/polkadot-sdk#11967"
)]
fn core_rotation_accepts_candidates_for_both_cores<S: CollatorSut>() {
	// 3 validator groups. With `group_rotation_frequency=1` and
	// `group_for_core(c, 3)` at `now=N` returning `(c + N) mod 3`, group 0 owns core
	// `c` iff `(c + N) mod 3 == 0`, i.e. `c == (3 - N mod 3) mod 3`.
	// - block 1: own core 2 (PARA_A)
	// - block 2: own core 1 (PARA_B)
	let validator_groups =
		vec![vec![ValidatorIndex(0)], vec![ValidatorIndex(1)], vec![ValidatorIndex(2)]];
	let config = collator_world_config()
		.with_schedule(CoreIndex(2), CoreSchedule::always(PARA_A))
		.with_schedule(CoreIndex(1), CoreSchedule::always(PARA_B))
		.with_validator_groups(validator_groups)
		.with_group_rotation_frequency(1);
	let mut w: World<S> = bootstrap_world::<S>(config, None);
	for _ in 0..2 {
		w.new_block().activate();
	}

	// Linear chain via repeated `.activate()`: under production `block_imported`
	// semantics, each child activation auto-deactivates its parent — so only the
	// latest block is an active leaf. Block 1 stays in the chain (and in the leaf's
	// implicit view), but it's no longer in `world.base.leaves`. We pull it from
	// the chain ancestry instead.
	let leaf_2 = w.leaf(); // active leaf, block 2 — we own core 1 → PARA_B
	let leaf_1 = w.ancestors()[0]; // chain ancestor, block 1 — we own core 2 → PARA_A

	let peer_a = w.declared_peer(PARA_A, V2);
	let cand_a = w.advertise(&peer_a, leaf_1, PARA_A);
	let _ = w.fetch_request(&cand_a);

	let peer_b = w.declared_peer(PARA_B, V2);
	let cand_b = w.advertise(&peer_b, leaf_2, PARA_B);
	let _ = w.fetch_request(&cand_b);

	// New PARA_A advertisement at the now-ancestor leaf 1: the rotation's owned-core
	// shift must not have orphaned leaf 1's CQ slot. Pre-#11967 silently drops; post-
	// #11967 fetches.
	let peer_a2 = w.declared_peer(PARA_A, V2);
	let cand_a2 = w.advertise(&peer_a2, leaf_1, PARA_A);
	let _ = w.fetch_request(&cand_a2);
}

/// Per-core slot accounting: under group rotation, peer_old declares PARA_X and
/// advertises at leaf_1 (we own core 2). After rotation we own core 1. peer_new
/// advertises PARA_X at leaf_2 (core 1). Both cores carry exactly one PARA_X slot —
/// per-core capacity must not be shared, so both fetch.
///
/// Hits the ancestor-RP drop bug on experimental: peer_old's ad at the (now-ancestor)
/// leaf_1 gets dropped pre-#11967. Mark bug_on.
#[crate::sim_test(
	bug_on = "experimental",
	bug_url = "github:paritytech/polkadot-sdk#11980"
)]
fn cross_core_reservation_does_not_consume_other_cores_slots<S: CollatorSut>() {
	const PARA_X_LOCAL: ParaId = ParaId::new(100);
	const PARA_FILLER: ParaId = ParaId::new(600);
	let validator_groups =
		vec![vec![ValidatorIndex(0)], vec![ValidatorIndex(1)], vec![ValidatorIndex(2)]];
	let config = collator_world_config()
		.with_schedule(CoreIndex(1), CoreSchedule::always(PARA_X_LOCAL))
		.with_schedule(CoreIndex(2), CoreSchedule::always(PARA_X_LOCAL))
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_FILLER))
		.with_validator_groups(validator_groups)
		.with_group_rotation_frequency(1);
	let mut w: World<S> = bootstrap_world::<S>(config, None);
	for _ in 0..2 {
		w.new_block().activate();
	}
	// Linear chain: only the latest block is an active leaf under production semantics.
	// Block 1 lives in the leaf's implicit view via chain ancestry.
	let leaf_2 = w.leaf(); // active leaf, block 2 — we own core 1
	let leaf_1 = w.ancestors()[0]; // chain ancestor, block 1 — we own core 2

	let peer_old = w.declared_peer(PARA_X_LOCAL, V2);
	let cand_old = w.advertise(&peer_old, leaf_1, PARA_X_LOCAL);
	let peer_new = w.declared_peer(PARA_X_LOCAL, V2);
	let cand_new = w.advertise(&peer_new, leaf_2, PARA_X_LOCAL);

	let _ = w.fetch_request(&cand_old);
	let _ = w.fetch_request(&cand_new);
}

/// 3 peers advertise PARA_A at 3 different SPs on a linear path. Leaf CQ has 2 slots
/// for PARA_A → exactly 2 fetches. >2 = over-fetch (third candidate has nowhere to
/// land); <2 = under-fetch (a wide-window candidate stole a slot).
#[crate::sim_test(
	bug_on = "experimental",
	bug_url = "github:paritytech/polkadot-sdk#11967"
)]
fn linear_multi_sp_same_para_capacity_not_double_counted<S: CollatorSut>() {
	let config = collator_world_config()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_A));
	let mut w: World<S> = bootstrap_world::<S>(config, None);
	for _ in 0..2 {
		w.new_block().register();
	}
	w.new_block()
		.with_claim_queue_at(CoreIndex(0), [PARA_A, ParaId::new(200), PARA_A])
		.activate();
	let leaf = w.leaf();
	let parent = w.ancestors()[0];
	let grandparent = w.ancestors()[1];

	// One distinct candidate per SP, all PARA_A.
	let peers = [
		w.declared_peer(PARA_A, V2),
		w.declared_peer(PARA_A, V2),
		w.declared_peer(PARA_A, V2),
	];
	let cands = [
		w.candidate_at(grandparent).para(PARA_A).head_data(HeadData(vec![1])).build(),
		w.candidate_at(parent).para(PARA_A).head_data(HeadData(vec![2])).build(),
		w.candidate_at(leaf).para(PARA_A).head_data(HeadData(vec![3])).build(),
	];
	for (peer, cand) in peers.iter().zip(cands.iter()) {
		w.advertise_with_parent_head(peer, cand.relay_parent(), cand.hash(), cand.parent_head_hash());
	}
	w.base.sim.advance(Duration::from_millis(300));
	w.base.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		2,
		"exactly 2 fetches (leaf CQ has 2 slots for PARA_A)",
	);
}

/// Narrow-window SP (= older ancestor) and wide-window SP (= leaf) both advertise
/// PARA_A. Leaf CQ `[A, other, A]` — narrow can only fill position 0; wide can fill
/// 0 or 2. Both must fetch — wide must not steal position 0.
#[crate::sim_test(
	bug_on = "experimental",
	bug_url = "github:paritytech/polkadot-sdk#11967"
)]
fn linear_multi_sp_no_under_fetch_when_wide_and_narrow_compete<S: CollatorSut>() {
	let config = collator_world_config()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_A));
	let mut w: World<S> = bootstrap_world::<S>(config, None);
	for _ in 0..2 {
		w.new_block().register();
	}
	w.new_block()
		.with_claim_queue_at(CoreIndex(0), [PARA_A, ParaId::new(200), PARA_A])
		.activate();
	let leaf = w.leaf();
	let grandparent = w.ancestors()[1]; // window len 1

	let peer_narrow = w.declared_peer(PARA_A, V2);
	let peer_wide = w.declared_peer(PARA_A, V2);
	let cand_narrow = w.candidate_at(grandparent).para(PARA_A).head_data(HeadData(vec![1])).build();
	let cand_wide = w.candidate_at(leaf).para(PARA_A).head_data(HeadData(vec![2])).build();
	w.advertise_with_parent_head(
		&peer_narrow,
		grandparent,
		cand_narrow.hash(),
		cand_narrow.parent_head_hash(),
	);
	w.advertise_with_parent_head(&peer_wide, leaf, cand_wide.hash(), cand_wide.parent_head_hash());
	w.base.sim.advance(Duration::from_millis(300));
	w.base.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		2,
		"both narrow- and wide-window ads must fetch (no under-fetch)",
	);
}

/// Leaf CQ shorter than the lookahead must not reject valid ancestor advertisements.
/// Setup: lookahead=3 (default), override leaf CQ to `[A]` (length 1). Advertise at
/// grandparent (depth 2): position 0 maps to leaf+2 = within sp's lookahead window.
///
/// Both impls fail this today — both use a cq-length-based reachability check
/// rather than the lookahead-based one. #11967 fixes it on experimental;
/// legacy carries the same bug.
#[crate::sim_test(
	bug_on = "legacy",
	bug_on = "experimental",
	bug_url = "github:paritytech/polkadot-sdk#11967"
)]
fn short_claim_queue_does_not_reject_ancestor_advertisements<S: CollatorSut>() {
	let config = collator_world_config()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_A));
	let mut w: World<S> = bootstrap_world::<S>(config, None);
	for _ in 0..2 {
		w.new_block().register();
	}
	w.new_block().with_claim_queue_at(CoreIndex(0), [PARA_A]).activate();
	let grandparent = w.ancestors()[1];
	let peer = w.declared_peer(PARA_A, V2);
	let cand = w.candidate_at(grandparent).para(PARA_A).build();
	w.advertise_with_parent_head(&peer, grandparent, cand.hash(), cand.parent_head_hash());
	let _ = w.fetch_request(&cand);
}

// --- Multi-fork tests ---
//
// Sibling forks share a common ancestor. In our framework, `build_with_ancestors_world
// _with_config(0, ...)` produces genesis → leaf. Genesis is the common ancestor; leaf is
// fork_a; we extend from genesis again to get fork_b. Sibling support relies on
// `common::chain::ChainModel::extend` mixing a sibling index into the synthetic child hash, so
// two extends from the same parent produce distinct hashes.

const PARA_X: ParaId = ParaId::new(100);
const PARA_Y: ParaId = ParaId::new(200);

/// Sibling forks: fork_a schedules PARA_X (default), fork_b schedules PARA_Y. While
/// both forks are active, both peers stay connected (assignments are the union).
/// After dropping fork_b, peer_y must be disconnected (its para is no longer
/// scheduled at any active leaf); peer_x stays.
#[crate::sim_test(
	bug_on = "experimental",
	bug_url = "github:paritytech/polkadot-sdk#11967"
)]
fn fork_assignments_are_union_of_leaves<S: CollatorSut>() {
	use polkadot_node_subsystem::messages::{CollatorProtocolMessage, NetworkBridgeEvent};

	let config = collator_world_config()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_X));
	let mut w: World<S> = bootstrap_world::<S>(config, None);
	w.new_block().activate();
	let fork_a = w.leaf();
	let common = w.base.chain.lock().genesis();
	let fork_b = w
		.new_block()
		.from_parent(common)
		.with_claim_queue_at(CoreIndex(0), [PARA_Y, PARA_Y, PARA_Y])
		.activate()
		.hash;

	let peer_x = w.declared_peer(PARA_X, V2);
	let peer_y = w.declared_peer(PARA_Y, V2);

	// Both forks active → assignments are the union → neither peer disconnected.
	w.expect_no_disconnect(&peer_x, Duration::from_millis(200));
	w.expect_no_disconnect(&peer_y, Duration::from_millis(200));

	// Drop fork_b: send OurViewChange covering only fork_a.
	w.base.sim.send(CollatorProtocolMessage::NetworkBridgeUpdate(
		NetworkBridgeEvent::OurViewChange(
			polkadot_node_network_protocol::OurView::new(std::iter::once(fork_a), 0),
		),
	));
	let _ = fork_b;

	// peer_y disconnects (its para is no longer scheduled). peer_x stays.
	w.expect_disconnect(&peer_y);
	w.expect_no_disconnect(&peer_x, Duration::from_millis(200));
}

/// Capacity at a shared ancestor uses the longest-reachable window across forks.
/// fork_a is 1 deep from common (window 2 to common); fork_b is 2 deep (window 1 to
/// common). Two PARA_X ads at the common ancestor: both fetched (window 2 wins).
///
/// Both impls fail this today: legacy uses the *shorter* window (1) and only
/// fetches one ad; experimental fails for the same root cause that #11967
/// addresses. Test prompts a fix on both sides.
#[crate::sim_test(
	bug_on = "legacy",
	bug_on = "experimental",
	bug_url = "github:paritytech/polkadot-sdk#11967"
)]
fn fork_capacity_uses_longest_window_across_paths<S: CollatorSut>() {
	let config = collator_world_config()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_X));
	let mut w: World<S> = bootstrap_world::<S>(config, None);
	w.new_block()
		.with_claim_queue_at(CoreIndex(0), [PARA_X, PARA_X, PARA_X])
		.activate();
	let _fork_a = w.leaf();
	let common = w.base.chain.lock().genesis();
	// fork_b at depth 2 from common.
	let fork_b_mid = w
		.new_block()
		.from_parent(common)
		.with_claim_queue_at(CoreIndex(0), [PARA_X, PARA_X, PARA_X])
		.activate()
		.hash;
	let fork_b_tip = w
		.new_block()
		.from_parent(fork_b_mid)
		.with_claim_queue_at(CoreIndex(0), [PARA_X, PARA_X, PARA_X])
		.activate()
		.hash;
	let _ = fork_b_tip;

	let peer_a = w.declared_peer(PARA_X, V2);
	let peer_b = w.declared_peer(PARA_X, V2);
	let cand_a = w.candidate_at(common).para(PARA_X).head_data(HeadData(vec![1])).build();
	let cand_b = w.candidate_at(common).para(PARA_X).head_data(HeadData(vec![2])).build();
	w.advertise_with_parent_head(&peer_a, common, cand_a.hash(), cand_a.parent_head_hash());
	w.advertise_with_parent_head(&peer_b, common, cand_b.hash(), cand_b.parent_head_hash());
	w.base.sim.advance(Duration::from_millis(300));
	w.base.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		2,
		"both ads at common ancestor fetch (longest-window across forks = 2)",
	);
}

/// Shared ancestor's capacity is one bucket across both forks, not doubled. Two
/// sibling forks each with CQ `[X, X, X]`. 4 distinct PARA_X ads at the common
/// ancestor must produce exactly 2 fetches.
///
/// Both impls fail this today: legacy under-fetches (1 instead of 2);
/// experimental fails for #11967's root cause.
#[crate::sim_test(
	bug_on = "legacy",
	bug_on = "experimental",
	bug_url = "github:paritytech/polkadot-sdk#11967"
)]
fn fork_shared_sp_capacity_not_double_counted<S: CollatorSut>() {
	let config = collator_world_config()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_X));
	let mut w: World<S> = bootstrap_world::<S>(config, None);
	w.new_block()
		.with_claim_queue_at(CoreIndex(0), [PARA_X, PARA_X, PARA_X])
		.activate();
	let _fork_a = w.leaf();
	let common = w.base.chain.lock().genesis();
	let _fork_b = w
		.new_block()
		.from_parent(common)
		.with_claim_queue_at(CoreIndex(0), [PARA_X, PARA_X, PARA_X])
		.activate()
		.hash;

	let peers: Vec<_> = (0..4).map(|_| w.declared_peer(PARA_X, V2)).collect();
	let cands: Vec<_> = (0..4)
		.map(|i| w.candidate_at(common).para(PARA_X).head_data(HeadData(vec![i as u8])).build())
		.collect();
	for (peer, cand) in peers.iter().zip(cands.iter()) {
		w.advertise_with_parent_head(peer, common, cand.hash(), cand.parent_head_hash());
	}
	w.base.sim.advance(Duration::from_millis(300));
	w.base.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		2,
		"shared ancestor capacity = 2 (not 4 — one bucket across both forks)",
	);
}

/// Drop a fork while a fetch is in-flight on it: the in-flight fetch must be
/// cancelled (response sender dropped on the wire) AND peers exclusive to that
/// fork's para must disconnect. fork_a schedules PARA_X, fork_b schedules PARA_Y.
/// peer_y declares Y, advertises a candidate at fork_b, validator launches a
/// fetch (we don't respond). Drop fork_b → peer_y disconnects, fetch is
/// cancelled (we observe via no second emitted within a settle window).
#[crate::sim_test(
	bug_on = "experimental",
	bug_url = "github:paritytech/polkadot-sdk#11967"
)]
fn fork_drop_reclaims_capacity_and_disconnects_peers<S: CollatorSut>() {
	use polkadot_node_subsystem::messages::{CollatorProtocolMessage, NetworkBridgeEvent};

	let config = collator_world_config()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_X));
	let mut w: World<S> = bootstrap_world::<S>(config, None);
	w.new_block().activate();
	let fork_a = w.leaf();
	let common = w.base.chain.lock().genesis();
	let fork_b = w
		.new_block()
		.from_parent(common)
		.with_claim_queue_at(CoreIndex(0), [PARA_Y, PARA_Y, PARA_Y])
		.activate()
		.hash;

	let peer_y = w.declared_peer(PARA_Y, V2);

	// Advertise on fork_b; validator launches a fetch — we hold the response.
	let cand_y = w.candidate_at(fork_b).para(PARA_Y).build();
	w.advertise_with_parent_head(&peer_y, fork_b, cand_y.hash(), cand_y.parent_head_hash());
	let _req_id = w.fetch_request(&cand_y);

	// Drop fork_b: send OurViewChange excluding it. The validator should:
	// - cancel the in-flight fetch (no second emitted),
	// - disconnect peer_y (its para no longer scheduled at any active leaf).
	w.base.sim.send(CollatorProtocolMessage::NetworkBridgeUpdate(
		NetworkBridgeEvent::OurViewChange(
			polkadot_node_network_protocol::OurView::new(std::iter::once(fork_a), 0),
		),
	));

	w.expect_disconnect(&peer_y);
	// The pending fetch must NOT be seconded — fork_b is gone, the candidate
	// can no longer land. Settle long enough that any erroneous second would
	// have fired.
	w.expect_no_second(&cand_y, Duration::from_millis(500));
}
}

mod upcoming_pr_11980 {
use crate::{
	common::builders::ProtocolVersion::V2,
	common::contract::Effect,
	common::harness::CollatorSut,
	common::world::{bootstrap_world, collator_world_config, World},
};
use crate::common::world::WorldExt as _;
use polkadot_node_subsystem::OverseerSignal;
use polkadot_primitives::{
	CandidateEvent, CoreIndex, GroupIndex, HeadData, Id as ParaId,
};
use std::time::Duration;

const PARA_A: ParaId = ParaId::new(100);
const PARA_OTHER: ParaId = ParaId::new(200);

/// High-rep peer at an ancestor SP wins the single PARA_A slot over a low-rep peer at
/// the leaf. Setup: leaf CQ `[A, other, other]` → 1 PARA_A slot. peer_low (score 0)
/// at leaf; peer_high (score 1, ramped via finalize) at parent. Single fetch goes to
/// peer_high.
#[crate::sim_test(only = "experimental", bug_on = "experimental",
	bug_url = "github:paritytech/polkadot-sdk#11980")]
fn high_rep_peer_at_ancestor_wins_over_low_rep_at_leaf<S: CollatorSut>() {
	let config = collator_world_config()
		.with_schedule(CoreIndex(0), crate::common::chain::CoreSchedule::always(PARA_A));
	let mut w: World<S> = bootstrap_world::<S>(config, None);
	w.new_block().activate();
	w.new_block()
		.with_claim_queue_at(CoreIndex(0), [PARA_A, PARA_OTHER, PARA_OTHER])
		.activate();
	let leaf0 = w.leaf();
	let parent = w.ancestors()[0];

	// Ramp peer_high to score 1.
	let peer_high = w.declared_peer(PARA_A, V2);
	let cand_seed = w
		.candidate_at(leaf0)
		.para(PARA_A)
		.parent_head(HeadData(Vec::new()))
		.head_data(HeadData(vec![1]))
		.approved_peer(peer_high.peer_id)
		.build();
	w.outputs.insert(cand_seed.hash(), cand_seed.commitments.clone(), cand_seed.pvd.clone());
	w.full_second(&peer_high, &cand_seed);
	{
		let mut chain = w.base.chain.lock();
		chain.set_pending_availability(PARA_A, vec![cand_seed.committed()]);
		chain.set_candidate_events(
			leaf0,
			vec![CandidateEvent::CandidateIncluded(
				cand_seed.receipt.clone(),
				cand_seed.commitments.head_data.clone(),
				CoreIndex(0),
				GroupIndex(0),
			)],
		);
		chain.set_finalized(leaf0);
	}
	w.base.sim.signal(OverseerSignal::BlockFinalized(leaf0, w.leaf_number()));
	w.base.sim.advance(Duration::from_millis(50));

	// New leaf for the arbitration round; rebuild leaf-q on the new leaf too.
	let leaf1 = w
		.new_block()
		.from_parent(leaf0)
		.with_claim_queue_at(CoreIndex(0), [PARA_A, PARA_OTHER, PARA_OTHER])
		.activate()
		.hash;
	let parent_of_leaf1 = leaf0;
	let _ = parent;

	// peer_low joins fresh.
	let peer_low = w.declared_peer(PARA_A, V2);

	// Both advertise PARA_A: peer_high at the now-ancestor (leaf0), peer_low at the leaf.
	// Single PARA_A slot → arbitration kicks in.
	let cand_high = w
		.candidate_at(parent_of_leaf1)
		.para(PARA_A)
		.parent_head(cand_seed.output_head())
		.head_data(HeadData(vec![2]))
		.build();
	let cand_low = w
		.candidate_at(leaf1)
		.para(PARA_A)
		.parent_head(cand_seed.output_head())
		.head_data(HeadData(vec![3]))
		.build();
	w.advertise_with_parent_head(
		&peer_high,
		parent_of_leaf1,
		cand_high.hash(),
		cand_high.parent_head_hash(),
	);
	w.advertise_with_parent_head(&peer_low, leaf1, cand_low.hash(), cand_low.parent_head_hash());
	w.base.sim.advance(Duration::from_millis(50));

	let _ = w.expect_fetch_to(peer_high.peer_id);
	w.base.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		1,
		"single fetch goes to high-rep ancestor peer (slot count = 1)",
	);
}

// TODO: port `high_rep_at_any_sp_wins_for_each_position`. Multi-position arbitration
// where each free CQ position is filled by the rep-best reachable carrier:
//
// - Leaf CQ `[A, other, A]` (positions 0=A, 1=other=Y, 2=A).
// - peer_high_x: ramped score 1, advertises A at leaf (offset 0 → positions 0, 2).
// - peer_low_x: score 0, advertises A at grandparent (depth 2, offset 2 → position 0 only).
// - peer_high_y: ramped score 1, advertises Y at leaf.
//
// Expected outcome: 3 fetches. Position 2 → peer_high_x (rep-best for A there),
// position 1 → peer_high_y (only Y candidate), position 0 → peer_low_x (only carrier
// reachable from grandparent — narrow-only positions don't get stolen by the rep-best
// wide candidate).
//
// Blocked on having a clean way to ramp 2 peers' scores (peer_high_x and peer_high_y)
// in a single test. The current ramp helper uses the leaf+finalize pattern; doing it
// twice for two different peers needs either a shared chain-extension dance or a
// `World::seed_score(peer, para, score)` shortcut. The existing single-ramp tests
// here demonstrate that the rep machinery works; adding the more elaborate
// multi-position arbitration is incremental.
}

mod upcoming_pr_12004 {
use crate::{
	common::builders::{ProtocolVersion::V2, ProtocolVersion::V3},
	common::contract::Effect,
	common::harness::CollatorSut,
	common::world::activated_world,
};
use crate::common::world::WorldExt as _;
use polkadot_collator_protocol::validator_side_consts::MAX_UNSHARED_DOWNLOAD_TIME;
use polkadot_node_subsystem::OverseerSignal;
use polkadot_primitives::{
	CandidateEvent, CoreIndex, GroupIndex, HeadData, Id as ParaId,
};
use std::time::Duration;

const PARA: ParaId = ParaId::new(100);

/// Two V2 peers advertise the same candidate (same hash, same offer); one fetch fires.
/// Pre-#12004: two fetches, because `Advertisement` keys on `(offer, peer_id)`. Post-
/// #12004: dedup keys on the offer alone, with the peer chosen by rep arbitration.
#[crate::sim_test(
	bug_on = "experimental",
	bug_url = "github:paritytech/polkadot-sdk#12004"
)]
fn v2_same_candidate_from_multiple_peers_fetched_once<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let leaf = w.leaf();

	let peer_a = w.declared_peer(PARA, V2);
	let peer_b = w.declared_peer(PARA, V2);
	// Same candidate (hash) advertised by both peers.
	let cand = w.candidate_at(leaf).para(PARA).build();
	w.advertise_with_parent_head(&peer_a, leaf, cand.hash(), cand.parent_head_hash());
	w.advertise_with_parent_head(&peer_b, leaf, cand.hash(), cand.parent_head_hash());

	// Settle long enough that any second concurrent fetch would have fired.
	w.base.sim.advance(Duration::from_millis(300));

	w.base.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		1,
		"exactly one fetch for the shared V2 candidate (must NOT fire one per carrier)",
	);
}

/// V2 peer and V3 peer both carry the same V2-descriptor offer; one fetch fires.
/// V3 protocol may legitimately advertise a V2 descriptor — the validator must dedup
/// by offer (descriptor) regardless of the carrier's protocol version.
#[crate::sim_test(
	bug_on = "experimental",
	bug_url = "github:paritytech/polkadot-sdk#12004"
)]
fn cross_protocol_version_carriers_fetched_once<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let leaf = w.leaf();

	let peer_v2 = w.declared_peer(PARA, V2);
	let peer_v3 = w.declared_peer(PARA, V3);
	let cand = w.candidate_at(leaf).para(PARA).build();
	w.advertise_with_parent_head(&peer_v2, leaf, cand.hash(), cand.parent_head_hash());
	w.advertise_with_parent_head(&peer_v3, leaf, cand.hash(), cand.parent_head_hash());
	w.base.sim.advance(Duration::from_millis(300));
	w.base.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		1,
		"exactly one fetch across V2 + V3 carriers (offer-keyed dedup)",
	);
}

/// Reputation arbitration when multiple carriers offer the same candidate. peer_high
/// has score 1 (one past inclusion); peer_low has 0. Both advertise the same offer;
/// the single fetch must go to peer_high.
#[crate::sim_test(only = "experimental", bug_on = "experimental",
	bug_url = "github:paritytech/polkadot-sdk#12004")]
fn v2_co_carrier_rep_arbitration_picks_high_rep_peer<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let leaf0 = w.leaf();

	// Ramp peer_high to score 1 via finalize-with-included-candidate.
	let peer_high = w.declared_peer(PARA, V2);
	let cand_seed = w
		.candidate_at(leaf0)
		.para(PARA)
		.parent_head(HeadData(Vec::new()))
		.head_data(HeadData(vec![1]))
		.approved_peer(peer_high.peer_id)
		.build();
	w.outputs.insert(cand_seed.hash(), cand_seed.commitments.clone(), cand_seed.pvd.clone());
	w.full_second(&peer_high, &cand_seed);
	{
		let mut chain = w.base.chain.lock();
		chain.set_pending_availability(PARA, vec![cand_seed.committed()]);
		chain.set_candidate_events(
			leaf0,
			vec![CandidateEvent::CandidateIncluded(
				cand_seed.receipt.clone(),
				cand_seed.commitments.head_data.clone(),
				CoreIndex(0),
				GroupIndex(0),
			)],
		);
		chain.set_finalized(leaf0);
	}
	w.base.sim.signal(OverseerSignal::BlockFinalized(leaf0, w.leaf_number()));
	w.base.sim.advance(Duration::from_millis(50));

	// New leaf for the arbitration round.
	let leaf1 = w.new_block().from_parent(leaf0).activate().hash;
	let peer_low = w.declared_peer(PARA, V2);

	// Both carriers offer the same new candidate.
	let cand = w
		.candidate_at(leaf1)
		.para(PARA)
		.parent_head(cand_seed.output_head())
		.head_data(HeadData(vec![2]))
		.build();
	w.advertise_with_parent_head(&peer_high, leaf1, cand.hash(), cand.parent_head_hash());
	w.advertise_with_parent_head(&peer_low, leaf1, cand.hash(), cand.parent_head_hash());
	w.base.sim.advance(Duration::from_millis(50));

	// Exactly one fetch, targeted at peer_high.
	let _ = w.expect_fetch_to(peer_high.peer_id);
	w.base.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		1,
		"exactly one fetch and it goes to the rep-best peer",
	);
}

/// Co-advertiser *fallback*: when several peers carry the same candidate, the validator
/// fetches from one and *parks* the others as fallbacks — and if the in-flight fetch
/// fails, it retries from a parked co-advertiser without waiting for re-advertisement.
///
/// This is the failure-path companion to `v2_same_candidate_from_multiple_peers_fetched_once`
/// (which only checks the happy path fires once). Reviewer ask on #12004:
/// "exercise the fallback of fetching a collation based on a duplicated advertisement from
/// a queued peer."
///
/// Two V2 peers advertise the same candidate; the validator must:
///   1. fire *exactly one* fetch and keep the duplicate parked, then
///   2. on that fetch timing out, fire the fallback fetch to the other carrier.
///
/// `bug_on = "experimental"`: legacy parks the duplicate and re-fetches on timeout. Pre-
/// #12004 experimental keys in-flight dedup on `(offer, peer_id)`, so the two carriers'
/// `Advertisement`s differ and *both* fetches fire at once (bug 1) — there is no parked
/// fallback, and step 1's "exactly one fetch" assertion fails. Post-#12004 the offer-only
/// dedup makes the second carrier a true fallback and this passes on experimental too.
#[crate::sim_test(
	bug_on = "experimental",
	bug_url = "github:paritytech/polkadot-sdk#12004"
)]
fn v2_co_carrier_fallback_fetches_from_second_peer_on_failure<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let leaf = w.leaf();

	let peer_a = w.declared_peer(PARA, V2);
	let peer_b = w.declared_peer(PARA, V2);
	// Same candidate (hash) advertised by both peers.
	let cand = w.candidate_at(leaf).para(PARA).build();
	w.advertise_with_parent_head(&peer_a, leaf, cand.hash(), cand.parent_head_hash());
	w.advertise_with_parent_head(&peer_b, leaf, cand.hash(), cand.parent_head_hash());

	// Settle long enough that any second concurrent fetch would have fired.
	w.base.sim.advance(Duration::from_millis(300));

	// Step 1 — exactly one fetch in flight; the other carrier is parked as a fallback.
	// (This is the assertion pre-#12004 experimental fails: it double-fetches.)
	w.base.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		1,
		"exactly one fetch for the shared V2 candidate; the duplicate must be parked",
	);

	// Identify which carrier got the (single) first fetch, so we can assert the fallback
	// targets the *other* one.
	let (first_peer, _) = w
		.first_fetch_after(0)
		.expect("exactly one fetch fired, so first_fetch_after must find it");
	let other_peer = if first_peer == peer_a.peer_id { peer_b.peer_id } else { peer_a.peer_id };

	// Step 2 — never respond; advance past the per-fetch deadline. The parked co-advertiser
	// must now be used as the fallback, without any new advertisement having arrived.
	w.base.sim.advance(MAX_UNSHARED_DOWNLOAD_TIME + Duration::from_millis(100));
	let _ = w.expect_fetch_to(other_peer);
}
}
