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

//! Claim-queue accounting across leaf + ancestor relay parents.
//!
//! Mirrors `validator_side/tests/prospective_parachains.rs::claims_below_are_counted_correctly`,
//! `claims_above_are_counted_correctly`, and `claim_fills_last_free_slot`. With
//! `lookahead=3` and `CQ=[A,A,A]` total per-para capacity is 3 across leaf + ancestor.

use crate::{
	builders::ProtocolVersion::V2,
	harness::CollatorSut,
	scenarios::shared::build_with_ancestors_world,
};
use polkadot_primitives::{CoreIndex, HeadData, Id as ParaId};
use std::time::Duration;

const PARA: ParaId = ParaId::new(2000);

/// 2 seconded at the ancestor + 1 at the leaf = 3 total. 4th at leaf rejected.
#[crate::sim_test]
fn claims_below_are_counted_correctly<S: CollatorSut>() {
	let mut w = build_with_ancestors_world::<S>(1, &[(CoreIndex(0), PARA)]);
	let leaf = w.leaf();
	let ancestor = w.ancestors()[0];

	// 3-candidate chain: parent_head=[i-1] → output=[i]. First 2 at ancestor RP, 3rd at leaf.
	let c1 = w.candidate_at(ancestor)
		.para(PARA).parent_head(HeadData(Vec::new())).head_data(HeadData(vec![1])).build();
	let c2 = w.candidate_at(ancestor)
		.para(PARA).parent_head(c1.output_head()).head_data(HeadData(vec![2])).build();
	let c3 = w.candidate_at(leaf)
		.para(PARA).parent_head(c2.output_head()).head_data(HeadData(vec![3])).build();

	let peer = w.declared_peer(PARA, V2);
	w.full_second(&peer, &c1);
	w.full_second(&peer, &c2);
	w.full_second(&peer, &c3);

	// 4th candidate at the leaf — claim queue full.
	let c4 = w.candidate_at(leaf)
		.para(PARA).parent_head(c3.output_head()).head_data(HeadData(vec![4])).build();
	w.advertise_with_parent_head(&peer, leaf, c4.hash(), c4.parent_head_hash());
	w.no_fetch_for(&c4, Duration::from_millis(150));
}

/// All 3 claims at the leaf → ancestor advertisement rejected (capacity full above).
#[crate::sim_test]
fn claims_above_are_counted_correctly<S: CollatorSut>() {
	let mut w = build_with_ancestors_world::<S>(1, &[(CoreIndex(0), PARA)]);
	let leaf = w.leaf();
	let ancestor = w.ancestors()[0];

	let c1 = w.candidate_at(leaf)
		.para(PARA).parent_head(HeadData(Vec::new())).head_data(HeadData(vec![1])).build();
	let c2 = w.candidate_at(leaf)
		.para(PARA).parent_head(c1.output_head()).head_data(HeadData(vec![2])).build();
	let c3 = w.candidate_at(leaf)
		.para(PARA).parent_head(c2.output_head()).head_data(HeadData(vec![3])).build();

	let peer = w.declared_peer(PARA, V2);
	w.full_second(&peer, &c1);
	w.full_second(&peer, &c2);
	w.full_second(&peer, &c3);

	let c4 = w.candidate_at(ancestor)
		.para(PARA).parent_head(c3.output_head()).head_data(HeadData(vec![4])).build();
	w.advertise_with_parent_head(&peer, ancestor, c4.hash(), c4.parent_head_hash());
	w.no_fetch_for(&c4, Duration::from_millis(150));
}

/// 1 seconded at ancestor + 1 at leaf + 1 at leaf-grandparent (deeper ancestor): final
/// candidate fills the last slot. Subsequent ad rejected.
#[crate::sim_test]
fn claim_fills_last_free_slot<S: CollatorSut>() {
	let mut w = build_with_ancestors_world::<S>(2, &[(CoreIndex(0), PARA)]);
	let leaf = w.leaf();
	let parent = w.ancestors()[0];
	let grandparent = w.ancestors()[1];

	let c1 = w.candidate_at(grandparent)
		.para(PARA).parent_head(HeadData(Vec::new())).head_data(HeadData(vec![1])).build();
	let c2 = w.candidate_at(parent)
		.para(PARA).parent_head(c1.output_head()).head_data(HeadData(vec![2])).build();
	let c3 = w.candidate_at(leaf)
		.para(PARA).parent_head(c2.output_head()).head_data(HeadData(vec![3])).build();

	let peer = w.declared_peer(PARA, V2);
	w.full_second(&peer, &c1);
	w.full_second(&peer, &c2);
	w.full_second(&peer, &c3);

	let c4 = w.candidate_at(leaf)
		.para(PARA).parent_head(c3.output_head()).head_data(HeadData(vec![4])).build();
	w.advertise_with_parent_head(&peer, leaf, c4.hash(), c4.parent_head_hash());
	w.no_fetch_for(&c4, Duration::from_millis(150));
}
