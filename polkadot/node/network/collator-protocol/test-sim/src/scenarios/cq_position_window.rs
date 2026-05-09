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

//! Claim-queue position / window arithmetic — coverage ported from upstream PR #11980.
//!
//! These probe the `valid_len = lookahead - offset` boundary for advertisement acceptance:
//! the last reachable CQ position at the leaf is in-window (off-by-one regression guard);
//! seconded candidates count toward per-para capacity (not just in-flight fetches); V1
//! single-shot prevents two concurrent V1 fetches for the same `(sp, para)`.

use crate::scenarios::shared::WorldExt as _;
use crate::{
	builders::{Candidate, ProtocolVersion::V1, ProtocolVersion::V2},
	contract::Effect,
	harness::CollatorSut,
	scenarios::shared::{build_with_ancestors_world_with_config, ChainConfig, LeafSelector},
};
use polkadot_primitives::{CoreIndex, HeadData, Id as ParaId};
use std::{
	collections::{BTreeMap, VecDeque},
	time::Duration,
};

const PARA_A: ParaId = ParaId::new(100);
const PARA_OTHER: ParaId = ParaId::new(200);

/// Off-by-one boundary: the last CQ position at the leaf is reachable (offset 0 → window
/// covers all `lookahead` positions). With CQ `[other, other, A]` on the assigned core,
/// para A at index 2 must accept an advertisement at the leaf.
#[crate::sim_test]
fn last_claim_queue_position_accepted_at_leaf<S: CollatorSut>() {
	let mut leaf_q = BTreeMap::new();
	leaf_q.insert(
		CoreIndex(0),
		VecDeque::from(vec![PARA_OTHER, PARA_OTHER, PARA_A]),
	);
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), crate::chain::CoreSchedule::always(PARA_A))
		.with_claim_queue_at(LeafSelector::Leaf, leaf_q);
	let mut w = build_with_ancestors_world_with_config::<S>(0, config);

	let peer = w.declared_peer(PARA_A, V2);
	let cand = w.advertise(&peer, w.leaf(), PARA_A);
	let _ = w.fetch_request(&cand);
}

/// Seconded candidates count as consumers in the per-core CQ pool. Leaf CQ
/// `[A, other, A]` has exactly 2 slots for A. After two A-candidates are seconded, a third
/// advertisement at the same RP for the same para must NOT trigger a fetch — capacity full.
#[crate::sim_test]
fn seconded_candidates_consume_capacity<S: CollatorSut>() {
	let mut leaf_q = BTreeMap::new();
	leaf_q.insert(CoreIndex(0), VecDeque::from(vec![PARA_A, PARA_OTHER, PARA_A]));
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), crate::chain::CoreSchedule::always(PARA_A))
		.with_claim_queue_at(LeafSelector::Leaf, leaf_q);
	let mut w = build_with_ancestors_world_with_config::<S>(0, config);
	let leaf = w.leaf();

	let peer_a = w.declared_peer(PARA_A, V2);
	let peer_b = w.declared_peer(PARA_A, V2);

	// Chain two candidates by parent_head so prospective accepts both.
	let c1 = w
		.candidate_at(leaf)
		.para(PARA_A)
		.parent_head(HeadData(Vec::new()))
		.head_data(HeadData(vec![1]))
		.build();
	let c2 = w
		.candidate_at(leaf)
		.para(PARA_A)
		.parent_head(c1.output_head())
		.head_data(HeadData(vec![2]))
		.build();

	w.full_second(&peer_a, &c1);
	w.full_second(&peer_b, &c2);

	// Third advertisement: capacity full → no fetch should fire.
	let c3 = w
		.candidate_at(leaf)
		.para(PARA_A)
		.parent_head(c2.output_head())
		.head_data(HeadData(vec![3]))
		.build();
	let peer_c = w.declared_peer(PARA_A, V2);
	w.advertise_with_parent_head(&peer_c, leaf, c3.hash(), c3.parent_head_hash());
	w.no_fetch_for(&c3, Duration::from_millis(300));
}

/// In-window boundary, ancestor side: leaf CQ `[A, other, A]`. Advertise PARA_A at
/// the parent (offset 1, window `[A, other]`). Position 0 is reachable → accepted.
///
/// Marked bug_on=experimental because experimental drops ancestor-RP advertisements
/// (`memory:project_collator_experimental_no_ancestor_rp_advertise`); the test
/// flips green when that bug is fixed.
#[crate::sim_test(
	bug_on = "experimental",
	bug_url = "memory:project_collator_experimental_no_ancestor_rp_advertise"
)]
fn non_obsolete_position_accepted<S: CollatorSut>() {
	let mut leaf_q = BTreeMap::new();
	leaf_q.insert(CoreIndex(0), VecDeque::from(vec![PARA_A, PARA_OTHER, PARA_A]));
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), crate::chain::CoreSchedule::always(PARA_A))
		.with_claim_queue_at(LeafSelector::Leaf, leaf_q);
	let mut w = build_with_ancestors_world_with_config::<S>(1, config);
	let parent = w.ancestors()[0];
	let peer = w.declared_peer(PARA_A, V2);
	let cand = w.candidate_at(parent).para(PARA_A).build();
	w.advertise_with_parent_head(&peer, parent, cand.hash(), cand.parent_head_hash());
	let _ = w.fetch_request(&cand);
}

/// Out-of-window: leaf CQ `[other, other, A]`. Advertise PARA_A at the parent (offset 1,
/// window `[other, other]`). PARA_A not reachable from this SP → rejected, no fetch.
///
/// Both impls reject — but experimental's reason is the ancestor-RP drop bug, not the
/// position check. Mark bug_on=experimental so the test still serves as upcoming-fix
/// coverage; once #11967 lands and the ancestor-RP bug is fixed, experimental's
/// rejection should come from the position check (also correct), and the test stays
/// green.
#[crate::sim_test]
fn obsolete_positions_rejected<S: CollatorSut>() {
	let mut leaf_q = BTreeMap::new();
	leaf_q.insert(
		CoreIndex(0),
		VecDeque::from(vec![PARA_OTHER, PARA_OTHER, PARA_A]),
	);
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), crate::chain::CoreSchedule::always(PARA_A))
		.with_claim_queue_at(LeafSelector::Leaf, leaf_q);
	let mut w = build_with_ancestors_world_with_config::<S>(1, config);
	let parent = w.ancestors()[0];
	let peer = w.declared_peer(PARA_A, V2);
	let cand = w.candidate_at(parent).para(PARA_A).build();
	w.advertise_with_parent_head(&peer, parent, cand.hash(), cand.parent_head_hash());
	w.no_fetch_for(&cand, Duration::from_millis(300));
}

/// V1 single-shot per `(sp, para)` round. CQ has 2 slots for A but two V1 peers
/// advertise at the leaf and only one fetch fires this round (V1 ads carry no
/// `prospective_candidate`, so the validator can't tell them apart and serializes).
#[crate::sim_test]
fn v1_single_shot_per_sp_para_round<S: CollatorSut>() {
	let mut leaf_q = BTreeMap::new();
	leaf_q.insert(CoreIndex(0), VecDeque::from(vec![PARA_A, PARA_A, PARA_OTHER]));
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), crate::chain::CoreSchedule::always(PARA_A))
		.with_claim_queue_at(LeafSelector::Leaf, leaf_q);
	let mut w = build_with_ancestors_world_with_config::<S>(0, config);

	let peer_a = w.declared_peer(PARA_A, V1);
	let peer_b = w.declared_peer(PARA_A, V1);

	w.base.sim.send(peer_a.advertise(w.leaf(), None, None));
	w.base.sim.send(peer_b.advertise(w.leaf(), None, None));

	// Exactly one V1 fetch this round.
	let _ = w.expect_any_fetch();
	let _ = Candidate::for_para_at(PARA_A, w.leaf()); // unused — keeps the explicit "V1 dedup" intent visible
	w.base.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		1,
		"exactly one V1 fetch despite two V1 advertisements at the same (sp, para)",
	);
}
