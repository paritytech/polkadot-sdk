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

//! Mirrors `validator_side/tests/prospective_parachains.rs::collation_fetching_fairness_handles_old_claims`.
//!
//! Across multi-leaf views with shifting claim queues:
//! - leaf2 CQ=[A,B,A]: second 2× A + 1× B → 3 slots filled.
//! - leaf3 CQ=[B,A,B] activated. With A=2, B=1 already counted, no new ads accepted.
//! - leaf4 CQ=[A,B,A] activated. Old leaf2 ages out → second 1× B + 1× A. Then no more.

use crate::scenarios::shared::WorldExt as _;
use crate::{
	builders::ProtocolVersion::V2,
	chain::CoreSchedule,
	harness::CollatorSut,
	scenarios::shared::{build_with_ancestors_world_with_config, ChainConfig, LeafSelector},
};
use polkadot_primitives::{CoreIndex, HeadData, Id as ParaId};
use std::{
	collections::{BTreeMap, VecDeque},
	time::Duration,
};

const PARA_A: ParaId = ParaId::new(2000);
const PARA_B: ParaId = ParaId::new(2001);

fn cq_for(paras: [ParaId; 3]) -> BTreeMap<CoreIndex, VecDeque<ParaId>> {
	let mut q = BTreeMap::new();
	q.insert(CoreIndex(0), VecDeque::from(paras.to_vec()));
	q
}

/// KNOWN BUG (experimental): the multi-step setup (full-second across view shifts) doesn't
/// complete on experimental — most likely the same view-shift counting bug as
/// `seconded_per_para_counted_across_whole_view` plus the ancestor-RP drop. See
/// `memory:project_collator_experimental_seconded_count_lost_across_view` and
/// `memory:project_collator_experimental_no_ancestor_rp_advertise`.
#[crate::sim_test(
	bug_on = "experimental",
	bug_url = "memory:project_collator_experimental_seconded_count_lost_across_view"
)]
fn old_claims_age_out_only_on_view_shift<S: CollatorSut>() {
	// Initial leaf with CQ=[A,B,A].
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_A))
		.with_claim_queue_at(LeafSelector::Leaf, cq_for([PARA_A, PARA_B, PARA_A]));
	let mut w = build_with_ancestors_world_with_config::<S>(0, config);
	let leaf2 = w.leaf();

	let peer_a = w.declared_peer(PARA_A, V2);
	let peer_b = w.declared_peer(PARA_B, V2);

	// Second 2A + 1B at leaf2 — fills the queue.
	let a1 = w.candidate_at(leaf2)
		.para(PARA_A).parent_head(HeadData(Vec::new())).head_data(HeadData(vec![1])).build();
	let a2 = w.candidate_at(leaf2)
		.para(PARA_A).parent_head(a1.output_head()).head_data(HeadData(vec![2])).build();
	let b1 = w.candidate_at(leaf2)
		.para(PARA_B).parent_head(HeadData(Vec::new())).head_data(HeadData(vec![10])).build();

	w.full_second(&peer_a, &a1);
	w.full_second(&peer_a, &a2);
	w.full_second(&peer_b, &b1);

	// Activate leaf3 (child of leaf2) with CQ=[B,A,B]. With A=2 already seconded and
	// only A=1 in this CQ, A's claim is full; B=1 already → CQ has 2 B slots, 1 free.
	let leaf3 = w.extend_and_activate_with(leaf2, &[leaf2], |chain, h, _n| {
		chain.set_claim_queue_at(h, cq_for([PARA_B, PARA_A, PARA_B]));
	});

	// Per upstream, no new ads should fetch at leaf3 — across the view {leaf2, leaf3} the
	// total seconded count for each para already meets/exceeds the CQ's per-para count.
	let extra_a_at_3 = w.candidate_at(leaf3)
		.para(PARA_A).parent_head(a2.output_head()).head_data(HeadData(vec![20])).build();
	w.advertise_with_parent_head(&peer_a, leaf3, extra_a_at_3.hash(), extra_a_at_3.parent_head_hash());
	w.no_fetch_for(&extra_a_at_3, Duration::from_millis(150));

	// Now activate leaf4 (child of leaf3) with CQ=[A,B,A]. Per upstream, leaf2 ages out
	// of allowed ancestry (depth > allowed_ancestry_len=2) → its seconded count drops.
	// With leaf2 out, only leaf3+leaf4 ancestry counts — fresh budget for B and A.
	let leaf4 = w.extend_and_activate_with(leaf3, &[leaf3], |chain, h, _n| {
		chain.set_claim_queue_at(h, cq_for([PARA_A, PARA_B, PARA_A]));
	});

	let b2 = w.candidate_at(leaf4)
		.para(PARA_B).parent_head(b1.output_head()).head_data(HeadData(vec![11])).build();
	let a3 = w.candidate_at(leaf4)
		.para(PARA_A).parent_head(a2.output_head()).head_data(HeadData(vec![3])).build();
	w.full_second(&peer_b, &b2);
	w.full_second(&peer_a, &a3);

	// Now CQ at leaf4 satisfied — further ads ignored.
	let extra_a = w.candidate_at(leaf4)
		.para(PARA_A).parent_head(a3.output_head()).head_data(HeadData(vec![30])).build();
	w.advertise_with_parent_head(&peer_a, leaf4, extra_a.hash(), extra_a.parent_head_hash());
	w.no_fetch_for(&extra_a, Duration::from_millis(150));
}
