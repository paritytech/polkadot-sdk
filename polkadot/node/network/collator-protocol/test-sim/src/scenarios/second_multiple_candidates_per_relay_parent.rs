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

//! Mirrors `validator_side/tests/prospective_parachains.rs::second_multiple_candidates_per_relay_parent`.
//!
//! With `scheduling_lookahead = 3`, the validator can second up to 3 candidates per relay
//! parent. The upstream test issues 3 siblings at the same depth (mocked backing accepts
//! all) — under real backing + real prospective, that fails because siblings at the same
//! depth don't all fit in the fragment chain. The faithful contract under real backing is
//! "3 candidates can be seconded in a chain at one RP" — port that:
//!
//! 1. Build a 3-candidate fragment chain at the leaf (parent → child → grandchild).
//! 2. Drive each through full advertise → fetch → second via `World::full_second`.
//! 3. Issue a 4th advertisement; assert no fetch fires (claim slots exhausted).

use crate::{
	builders::{Candidate, ProtocolVersion::V2},
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_primitives::{CoreIndex, HeadData, Id as ParaId};
use std::time::Duration;

const PARA: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn three_chained_candidates_seconded_then_fourth_rejected<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let leaf = w.leaf();
	let leaf_n = w.leaf_number();

	// Chain of three candidates: parent_head=[i-1] → output_head=[i] for i in 1..=3.
	let chain: Vec<Candidate> = (1..=3u8)
		.map(|i| {
			let parent_head =
				if i == 1 { HeadData(Vec::new()) } else { HeadData(vec![i - 1]) };
			Candidate::builder()
				.para(PARA)
				.relay_parent(leaf)
				.relay_parent_number(leaf_n)
				.parent_head(parent_head)
				.head_data(HeadData(vec![i]))
				.build()
		})
		.collect();

	let peer = w.declared_peer(PARA, V2);
	for cand in &chain {
		w.full_second(&peer, cand);
	}

	// 4th candidate at the same RP — claim slots are full. No fetch should fire.
	let extra = Candidate::builder()
		.para(PARA)
		.relay_parent(leaf)
		.relay_parent_number(leaf_n)
		.parent_head(HeadData(vec![3]))
		.head_data(HeadData(vec![4]))
		.build();
	w.advertise_with_parent_head(&peer, leaf, extra.hash(), extra.parent_head_hash());
	w.no_fetch_for(&extra, Duration::from_millis(200));
}
