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

//! Behavioural consequences of reputation — experimental-only.
//!
//! Legacy reputation is fire-and-forget to the network bridge: the test framework
//! mocks the bridge, so legacy rep has no behavioural feedback you can observe in this
//! framework. Experimental's rep, by contrast, drives:
//!
//! - **Fetch ranking**: when two peers advertise valid candidates at the same RP, the
//!   higher-scored peer's request goes out first. Ordering is `(score DESC, timestamp
//!   ASC, advertisement ord)` — see `validator_side_experimental/collation_manager
//!   /mod.rs:1009-1017`.
//! - **Penalty box for fresh peers**: when a competitor on the same para has score
//!   ≥ 1, a peer with score 0 must wait `MAX_FETCH_DELAY = 300ms` after scheduling-
//!   parent activation before its fetch fires. A peer with score ≥
//!   `INSTANT_FETCH_REP_THRESHOLD = 1` (one past inclusion) bypasses the delay
//!   immediately. See `calculate_delay` in `collation_manager/mod.rs:664-670`.
//! - **Slot eviction by score**: per-para slot cap is 60 (`CONNECTED_PEERS_PARA_LIMIT`).
//!   When full, a connecting peer with strictly higher score evicts the lowest-scored
//!   incumbent (`peer_manager/connected.rs:257-269`). Score `0` does *not* trigger
//!   floor-disconnect on its own.
//!
//! These tests are `only = "experimental"` — legacy has no equivalent observable
//! mechanism. For the *spec contract* both impls share (penalise misbehaviour, reward
//! good citizenship), see [`super::reputation_emission`] for the bus-event vs silent
//! divergence.
//!
//! # Score-ramping path
//!
//! Experimental's `+VALID_INCLUDED_CANDIDATE_BUMP = +1` triggers via:
//! 1. `OverseerSignal::BlockFinalized(R, n)`.
//! 2. `peer_manager` walks `ancestors(R, n)`, queries
//!    `RuntimeApiRequest::CandidateEvents(rp)` for each.
//! 3. For each `CandidateEvent::CandidateIncluded(receipt)` with v2+ descriptor, looks
//!    up `CandidatesPendingAvailability(parent_rp, para)` and matches by candidate hash.
//! 4. Reads `commitments.ump_signals().approved_peer()` → `+1` for that peer.
//!
//! The chain model gained `set_finalized` and `set_candidate_events` to drive this path
//! end-to-end without test shortcuts.

use crate::{
	builders::ProtocolVersion::V2,
	contract::Effect,
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
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
		let mut chain = w.chain.lock();
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
	w.sim.signal(OverseerSignal::BlockFinalized(leaf0, w.leaf_number()));
	// Let the peer_manager finalization handler complete its runtime-API round trip.
	w.sim.advance(Duration::from_millis(50));

	// --- Round 2: extend chain, activate new leaf, both peers advertise there ---
	let leaf1 = w.extend_and_activate_with(leaf0, &[leaf0], |_chain, _h, _n| {});

	// Capture sim_t at the moment of advertisement so we can measure the fetch latency
	// against the leaf's activation. (The penalty box delay is measured from
	// `activated_at` of the scheduling parent, not advertisement arrival.)
	let activation_t = w.sim.now_sim_t();

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
	let a_fetch = w.sim.expect(
		|e| matches!(
			e,
			Effect::SendRequest { candidate_hash: Some(c), .. } if *c == cand_a2.hash(),
		),
		Duration::from_millis(50),
		"peer A's fetch fires within 50ms (score >= 1 bypasses penalty box)",
	);
	let _ = a_fetch;
	let a_fetch_t = w.sim.now_sim_t() - activation_t;
	assert!(
		a_fetch_t < MAX_FETCH_DELAY,
		"peer A's fetch fired at {:?} after leaf activation; expected < {:?}",
		a_fetch_t,
		MAX_FETCH_DELAY,
	);

	// B's fetch must wait at least MAX_FETCH_DELAY (penalty box).
	let _b_fetch = w.sim.expect(
		|e| matches!(
			e,
			Effect::SendRequest { candidate_hash: Some(c), .. } if *c == cand_b.hash(),
		),
		MAX_FETCH_DELAY + Duration::from_millis(200),
		"peer B's fetch fires within 500ms (penalty box releases at 300ms)",
	);
	let b_fetch_t = w.sim.now_sim_t() - activation_t;
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
		let mut chain = w.chain.lock();
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
	w.sim.signal(OverseerSignal::BlockFinalized(leaf0, w.leaf_number()));
	w.sim.advance(Duration::from_millis(50));

	// --- Ramp peer B: bypass full_second; seed an "included" candidate and finalize ---
	//
	// We don't need the subsystem to actually second this candidate to bump B's score —
	// experimental's `+VALID_INCLUDED_CANDIDATE_BUMP` path walks finalized-block candidate
	// events and looks up `approved_peer` from the receipt's UMP signals. As long as
	// pending_availability + candidate_events agree on the receipt + parent_rp, the bump
	// fires for whatever peer the receipt names.
	let peer_b = w.declared_peer(PARA, V2);
	let leaf1 = w.extend_and_activate_with(leaf0, &[leaf0], |_chain, _h, _n| {});
	let cand_b_seed = w
		.candidate_at(leaf1)
		.para(PARA)
		.head_data(HeadData(vec![2]))
		.approved_peer(peer_b.peer_id)
		.build();
	{
		let mut chain = w.chain.lock();
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
	w.sim.signal(OverseerSignal::BlockFinalized(leaf1, w.leaves[w.leaves.len() - 1].number));
	w.sim.advance(Duration::from_millis(50));

	// --- Slash leaf: A advertises a new candidate; validator fetches; we don't respond ---
	let leaf2 = w.extend_and_activate_with(leaf1, &[leaf0, leaf1], |_chain, _h, _n| {});
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
	w.sim.cancel_fetch(req_id);
	w.sim.advance(Duration::from_millis(50));

	// --- Outcome leaf: A and B both advertise; B wins, A waits in penalty box ---
	let leaf3 = w.extend_and_activate_with(leaf2, &[leaf1, leaf2], |_chain, _h, _n| {});
	let activation_t = w.sim.now_sim_t();
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
	let _ = w.sim.expect(
		|e| matches!(
			e,
			Effect::SendRequest { candidate_hash: Some(c), .. } if *c == cand_b_after.hash(),
		),
		Duration::from_millis(50),
		"peer B's fetch fires within 50ms (score 1 bypasses penalty box)",
	);
	let b_fetch_t = w.sim.now_sim_t() - activation_t;
	assert!(
		b_fetch_t < MAX_FETCH_DELAY,
		"peer B's fetch fired at {:?} after leaf activation; expected < {:?}",
		b_fetch_t,
		MAX_FETCH_DELAY,
	);

	// A's fetch is delayed — score 0 (slashed) and max-for-para = 1.
	let _ = w.sim.expect(
		|e| matches!(
			e,
			Effect::SendRequest { candidate_hash: Some(c), .. } if *c == cand_a_after.hash(),
		),
		MAX_FETCH_DELAY + Duration::from_millis(200),
		"peer A's fetch fires within 500ms (penalty box releases at 300ms)",
	);
	let a_fetch_t = w.sim.now_sim_t() - activation_t;
	assert!(
		a_fetch_t >= MAX_FETCH_DELAY,
		"peer A's fetch fired at {:?} after leaf activation; expected >= {:?} (slashed → penalty box)",
		a_fetch_t,
		MAX_FETCH_DELAY,
	);
}
