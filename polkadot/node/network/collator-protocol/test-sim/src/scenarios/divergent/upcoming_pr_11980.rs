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

//! Reputation-arbitration tests ported from upstream PR #11980. These probe
//! experimental's score-driven fetch ranking. Score seeding goes through the natural
//! finalization path (see `divergent::reputation_behavior` for the same plumbing).
//!
//! Upstream PR: https://github.com/paritytech/polkadot-sdk/pull/11980
//!
//! All tests here are `only = "experimental"` because legacy has no score-driven
//! ranking. They're additionally `bug_on = "experimental"` until PR #11980 lands —
//! pre-#11980 the wide-window peer wins regardless of score.

use crate::{
	builders::ProtocolVersion::V2,
	contract::Effect,
	harness::CollatorSut,
	scenarios::shared::{build_with_ancestors_world_with_config, ChainConfig, LeafSelector},
};
use polkadot_node_subsystem::OverseerSignal;
use polkadot_primitives::{
	CandidateEvent, CoreIndex, GroupIndex, HeadData, Id as ParaId,
};
use std::{
	collections::{BTreeMap, VecDeque},
	time::Duration,
};

const PARA_A: ParaId = ParaId::new(100);
const PARA_OTHER: ParaId = ParaId::new(200);

/// High-rep peer at an ancestor SP wins the single PARA_A slot over a low-rep peer at
/// the leaf. Setup: leaf CQ `[A, other, other]` → 1 PARA_A slot. peer_low (score 0)
/// at leaf; peer_high (score 1, ramped via finalize) at parent. Single fetch goes to
/// peer_high.
#[crate::sim_test(only = "experimental", bug_on = "experimental",
	bug_url = "github:paritytech/polkadot-sdk#11980")]
fn high_rep_peer_at_ancestor_wins_over_low_rep_at_leaf<S: CollatorSut>() {
	let mut leaf_q = BTreeMap::new();
	leaf_q.insert(CoreIndex(0), VecDeque::from(vec![PARA_A, PARA_OTHER, PARA_OTHER]));
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), crate::chain::CoreSchedule::always(PARA_A))
		.with_claim_queue_at(LeafSelector::Leaf, leaf_q);
	let mut w = build_with_ancestors_world_with_config::<S>(1, config);
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
		let mut chain = w.chain.lock();
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
	w.sim.signal(OverseerSignal::BlockFinalized(leaf0, w.leaf_number()));
	w.sim.advance(Duration::from_millis(50));

	// New leaf for the arbitration round; rebuild leaf-q on the new leaf too.
	let leaf1 = w.extend_and_activate_with(leaf0, &[leaf0], |chain, h, _n| {
		let mut q = BTreeMap::new();
		q.insert(CoreIndex(0), VecDeque::from(vec![PARA_A, PARA_OTHER, PARA_OTHER]));
		chain.set_claim_queue_at(h, q);
	});
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
	w.sim.advance(Duration::from_millis(50));

	let _ = w.expect_fetch_to(peer_high.peer_id);
	w.sim.expect_count(
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
