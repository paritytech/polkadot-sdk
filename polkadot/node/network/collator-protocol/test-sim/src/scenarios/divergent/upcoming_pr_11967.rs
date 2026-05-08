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

//! Tests covering invariants that PR #11967 (rotation bug fix + capacity-tracking
//! simplification) introduces. Marked `bug_on = "experimental"` because the assertions
//! fail against pre-#11967 experimental — merging the PR flips the `should_panic`,
//! turns the test red, and prompts removal of the marker.
//!
//! Upstream PR: https://github.com/paritytech/polkadot-sdk/pull/11967
//!
//! Coverage:
//! - [`core_rotation_accepts_candidates_for_both_cores`] — under group rotation, an
//!   advertisement at an ancestor whose owned core differs from the leaf's owned core
//!   must still be accepted.
//! - [`linear_multi_sp_same_para_capacity_not_double_counted`] — three peers advertise
//!   para A at three different SPs on a linear path; leaf CQ has 2 slots for A;
//!   exactly 2 fetches.
//! - [`linear_multi_sp_no_under_fetch_when_wide_and_narrow_compete`] — narrow-window
//!   SP and wide-window SP both advertise; the wide-window one must not steal a slot
//!   the narrow-window one is the only candidate for.
//! - [`short_claim_queue_does_not_reject_ancestor_advertisements`] — leaf CQ shorter
//!   than the lookahead must not cause valid ancestor SP advertisements to be rejected.
//! - [`fork_assignments_are_union_of_leaves`] — sibling forks; assignments are the
//!   union of both leaves; dropping one fork drops its peer.
//! - [`fork_capacity_uses_longest_window_across_paths`] — capacity at a shared ancestor
//!   uses the longest reachable window across all extant leaves.
//! - [`fork_shared_sp_capacity_not_double_counted`] — shared ancestor's capacity is one
//!   bucket across both leaves, not two.
//! - [`fork_drop_reclaims_capacity_and_disconnects_peers`] — dropping a leaf reclaims
//!   its capacity and disconnects its peer.

use crate::{
	builders::ProtocolVersion::V2,
	chain::CoreSchedule,
	contract::Effect,
	harness::CollatorSut,
	scenarios::shared::{
		build_multi_leaf_world_with_config, build_with_ancestors_world_with_config,
		ChainConfig, LeafSelector,
	},
};
use polkadot_primitives::{CoreIndex, HeadData, Id as ParaId, ValidatorIndex};
use std::{
	collections::{BTreeMap, VecDeque},
	time::Duration,
};

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
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(2), CoreSchedule::always(PARA_A))
		.with_schedule(CoreIndex(1), CoreSchedule::always(PARA_B))
		.with_validator_groups(validator_groups)
		.with_group_rotation_frequency(1);
	let mut w = build_multi_leaf_world_with_config::<S>(2, config);

	let leaf_1 = w.leaves[0].hash; // we own core 2 → PARA_A
	let leaf_2 = w.leaves[1].hash; // we own core 1 → PARA_B

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
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(1), CoreSchedule::always(PARA_X_LOCAL))
		.with_schedule(CoreIndex(2), CoreSchedule::always(PARA_X_LOCAL))
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_FILLER))
		.with_validator_groups(validator_groups)
		.with_group_rotation_frequency(1);
	let mut w = build_multi_leaf_world_with_config::<S>(2, config);
	let leaf_1 = w.leaves[0].hash; // own core 2
	let leaf_2 = w.leaves[1].hash; // own core 1

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
	let mut leaf_q = BTreeMap::new();
	leaf_q.insert(CoreIndex(0), VecDeque::from(vec![PARA_A, ParaId::new(200), PARA_A]));
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_A))
		.with_claim_queue_at(LeafSelector::Leaf, leaf_q);
	let mut w = build_with_ancestors_world_with_config::<S>(2, config);
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
	w.sim.advance(Duration::from_millis(300));
	w.sim.expect_count(
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
	let mut leaf_q = BTreeMap::new();
	leaf_q.insert(CoreIndex(0), VecDeque::from(vec![PARA_A, ParaId::new(200), PARA_A]));
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_A))
		.with_claim_queue_at(LeafSelector::Leaf, leaf_q);
	let mut w = build_with_ancestors_world_with_config::<S>(2, config);
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
	w.sim.advance(Duration::from_millis(300));
	w.sim.expect_count(
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
	let mut leaf_q = BTreeMap::new();
	leaf_q.insert(CoreIndex(0), VecDeque::from(vec![PARA_A]));
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_A))
		.with_claim_queue_at(LeafSelector::Leaf, leaf_q);
	let mut w = build_with_ancestors_world_with_config::<S>(2, config);
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
// `chain::ChainModel::extend` mixing a sibling index into the synthetic child hash, so
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

	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_X));
	let mut w = build_with_ancestors_world_with_config::<S>(0, config);
	let fork_a = w.leaf();
	let common = w.chain.lock().genesis();
	let fork_b = w.extend_and_activate_with(common, &[fork_a], |chain, h, _n| {
		let mut q = BTreeMap::new();
		q.insert(CoreIndex(0), VecDeque::from(vec![PARA_Y, PARA_Y, PARA_Y]));
		chain.set_claim_queue_at(h, q);
	});

	let peer_x = w.declared_peer(PARA_X, V2);
	let peer_y = w.declared_peer(PARA_Y, V2);

	// Both forks active → assignments are the union → neither peer disconnected.
	w.expect_no_disconnect(&peer_x, Duration::from_millis(200));
	w.expect_no_disconnect(&peer_y, Duration::from_millis(200));

	// Drop fork_b: send OurViewChange covering only fork_a.
	w.sim.send(CollatorProtocolMessage::NetworkBridgeUpdate(
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
	let mut leaf_q = BTreeMap::new();
	leaf_q.insert(CoreIndex(0), VecDeque::from(vec![PARA_X, PARA_X, PARA_X]));
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_X))
		.with_claim_queue_at(LeafSelector::Leaf, leaf_q.clone());
	let mut w = build_with_ancestors_world_with_config::<S>(0, config);
	let fork_a = w.leaf();
	let common = w.chain.lock().genesis();
	// fork_b at depth 2 from common.
	let fork_b_mid = w.extend_and_activate_with(common, &[fork_a], |chain, h, _n| {
		chain.set_claim_queue_at(h, leaf_q.clone());
	});
	let fork_b_tip = w.extend_and_activate_with(fork_b_mid, &[fork_a, fork_b_mid], |chain, h, _n| {
		chain.set_claim_queue_at(h, leaf_q.clone());
	});
	let _ = fork_b_tip;

	let peer_a = w.declared_peer(PARA_X, V2);
	let peer_b = w.declared_peer(PARA_X, V2);
	let cand_a = w.candidate_at(common).para(PARA_X).head_data(HeadData(vec![1])).build();
	let cand_b = w.candidate_at(common).para(PARA_X).head_data(HeadData(vec![2])).build();
	w.advertise_with_parent_head(&peer_a, common, cand_a.hash(), cand_a.parent_head_hash());
	w.advertise_with_parent_head(&peer_b, common, cand_b.hash(), cand_b.parent_head_hash());
	w.sim.advance(Duration::from_millis(300));
	w.sim.expect_count(
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
	let mut leaf_q = BTreeMap::new();
	leaf_q.insert(CoreIndex(0), VecDeque::from(vec![PARA_X, PARA_X, PARA_X]));
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_X))
		.with_claim_queue_at(LeafSelector::Leaf, leaf_q.clone());
	let mut w = build_with_ancestors_world_with_config::<S>(0, config);
	let fork_a = w.leaf();
	let common = w.chain.lock().genesis();
	let _fork_b = w.extend_and_activate_with(common, &[fork_a], |chain, h, _n| {
		chain.set_claim_queue_at(h, leaf_q);
	});

	let peers: Vec<_> = (0..4).map(|_| w.declared_peer(PARA_X, V2)).collect();
	let cands: Vec<_> = (0..4)
		.map(|i| w.candidate_at(common).para(PARA_X).head_data(HeadData(vec![i as u8])).build())
		.collect();
	for (peer, cand) in peers.iter().zip(cands.iter()) {
		w.advertise_with_parent_head(peer, common, cand.hash(), cand.parent_head_hash());
	}
	w.sim.advance(Duration::from_millis(300));
	w.sim.expect_count(
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

	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_X));
	let mut w = build_with_ancestors_world_with_config::<S>(0, config);
	let fork_a = w.leaf();
	let common = w.chain.lock().genesis();
	let fork_b = w.extend_and_activate_with(common, &[fork_a], |chain, h, _n| {
		let mut q = BTreeMap::new();
		q.insert(CoreIndex(0), VecDeque::from(vec![PARA_Y, PARA_Y, PARA_Y]));
		chain.set_claim_queue_at(h, q);
	});

	let peer_y = w.declared_peer(PARA_Y, V2);

	// Advertise on fork_b; validator launches a fetch — we hold the response.
	let cand_y = w.candidate_at(fork_b).para(PARA_Y).build();
	w.advertise_with_parent_head(&peer_y, fork_b, cand_y.hash(), cand_y.parent_head_hash());
	let _req_id = w.fetch_request(&cand_y);

	// Drop fork_b: send OurViewChange excluding it. The validator should:
	// - cancel the in-flight fetch (no second emitted),
	// - disconnect peer_y (its para no longer scheduled at any active leaf).
	w.sim.send(CollatorProtocolMessage::NetworkBridgeUpdate(
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
