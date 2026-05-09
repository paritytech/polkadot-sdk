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

//! Mirrors `validator_side/tests/prospective_parachains.rs::collation_fetching_considers_advertisements_from_the_whole_view`.
//!
//! Shared core CQ=`[B, A, A]`. After 2 paras seconded at the active leaf, view shifts to a
//! deeper child. Earlier seconds remain in the validator's accounting → further ads at
//! the new leaf get queued/rejected up to claim-queue capacity. View shifts further so
//! older relay parents fall out of allowed ancestry → capacity frees again.
//!
//! Boils down to: validator's seconded count is computed across the whole in-view path,
//! not just the current leaf.

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

/// KNOWN BUG (experimental): seconded count from the prior leaf is not preserved when
/// extending into the new leaf's implicit view. Experimental fires a fetch for a
/// candidate that should be CQ-blocked. See
/// `memory:project_collator_experimental_seconded_count_lost_across_view`.
#[crate::sim_test(
	bug_on = "experimental",
	bug_url = "memory:project_collator_experimental_seconded_count_lost_across_view"
)]
fn seconded_per_para_counted_across_whole_view<S: CollatorSut>() {
	let mut leaf_q = BTreeMap::new();
	leaf_q.insert(CoreIndex(0), VecDeque::from(vec![PARA_B, PARA_A, PARA_A]));
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_A))
		.with_claim_queue_at(LeafSelector::Leaf, leaf_q);
	let mut w = build_with_ancestors_world_with_config::<S>(0, config);
	let leaf0 = w.leaf();

	// Second 1× A and 1× B at leaf0.
	let a1 = w.candidate_at(leaf0)
		.para(PARA_A).parent_head(HeadData(Vec::new())).head_data(HeadData(vec![1])).build();
	let b1 = w.candidate_at(leaf0)
		.para(PARA_B).parent_head(HeadData(Vec::new())).head_data(HeadData(vec![10])).build();

	let peer_a = w.declared_peer(PARA_A, V2);
	let peer_b = w.declared_peer(PARA_B, V2);

	w.full_second(&peer_a, &a1);
	w.full_second(&peer_b, &b1);

	// Activate a child of leaf0 — the view becomes {leaf0, leaf1}; previously seconded
	// candidates remain in scope. New leaf inherits same CQ shape [B,A,A].
	let leaf1 = w.extend_and_activate_with(leaf0, &[leaf0], |chain, h, _n| {
		let mut q = std::collections::BTreeMap::new();
		q.insert(
			CoreIndex(0),
			std::collections::VecDeque::from(vec![PARA_B, PARA_A, PARA_A]),
		);
		chain.set_claim_queue_at(h, q);
	});

	// Advertise another A at leaf1 — this lands in CQ position 1 or 2; A still has 1
	// remaining slot (3 total in CQ minus 1 already counted).
	let a2 = w.candidate_at(leaf1)
		.para(PARA_A).parent_head(a1.output_head()).head_data(HeadData(vec![2])).build();
	w.full_second(&peer_a, &a2);

	// 4th A: claim queue full for A (2 already seconded). Reject.
	let a3 = w.candidate_at(leaf1)
		.para(PARA_A).parent_head(a2.output_head()).head_data(HeadData(vec![3])).build();
	w.advertise_with_parent_head(&peer_a, leaf1, a3.hash(), a3.parent_head_hash());
	w.no_fetch_for(&a3, Duration::from_millis(150));

	// 2nd B: B was at CQ pos 0 (1 slot). Already 1 seconded → reject.
	let b2 = w.candidate_at(leaf1)
		.para(PARA_B).parent_head(b1.output_head()).head_data(HeadData(vec![11])).build();
	w.advertise_with_parent_head(&peer_b, leaf1, b2.hash(), b2.parent_head_hash());
	w.no_fetch_for(&b2, Duration::from_millis(50));
}
