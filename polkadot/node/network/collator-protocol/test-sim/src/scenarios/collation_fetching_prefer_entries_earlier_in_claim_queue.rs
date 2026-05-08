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

//! Mirrors `validator_side/tests/prospective_parachains.rs::collation_fetching_prefer_entries_earlier_in_claim_queue`.
//!
//! Shared core CQ=`[B, A, A]`. Peer for para A advertises first → fetched immediately.
//! Peer for A advertises a 2nd → queued. Peer for B advertises → queued. After A1
//! seconds, the next fetch goes to **B** (CQ position 0 — earlier wins), not A2.

use crate::{
	builders::ProtocolVersion::V2,
	chain::CoreSchedule,
	contract::{Effect, ReqKind},
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

#[crate::sim_test]
fn collation_fetching_prefer_entries_earlier_in_claim_queue<S: CollatorSut>() {
	let mut leaf_q = BTreeMap::new();
	leaf_q.insert(CoreIndex(0), VecDeque::from(vec![PARA_B, PARA_A, PARA_A]));
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_A))
		.with_claim_queue_at(LeafSelector::Leaf, leaf_q);
	let mut w = build_with_ancestors_world_with_config::<S>(0, config);
	let leaf = w.leaf();

	let a1 = w.candidate_at(leaf)
		.para(PARA_A).parent_head(HeadData(Vec::new())).head_data(HeadData(vec![1])).build();
	let a2 = w.candidate_at(leaf)
		.para(PARA_A).parent_head(a1.output_head()).head_data(HeadData(vec![2])).build();
	let b1 = w.candidate_at(leaf)
		.para(PARA_B).parent_head(HeadData(Vec::new())).head_data(HeadData(vec![10])).build();

	let peer_a = w.declared_peer(PARA_A, V2);
	let peer_b = w.declared_peer(PARA_B, V2);

	// A1 fetched first.
	w.outputs.insert(a1.hash(), a1.commitments.clone(), a1.pvd.clone());
	w.outputs.insert(a2.hash(), a2.commitments.clone(), a2.pvd.clone());
	w.outputs.insert(b1.hash(), b1.commitments.clone(), b1.pvd.clone());

	w.advertise_with_parent_head(&peer_a, leaf, a1.hash(), a1.parent_head_hash());
	let a1_req = w.fetch_request(&a1);

	// Queue A2 + B1 while A1 is in flight; expect no fetches to fire.
	w.advertise_with_parent_head(&peer_a, leaf, a2.hash(), a2.parent_head_hash());
	w.advertise_with_parent_head(&peer_b, leaf, b1.hash(), b1.parent_head_hash());
	// One fetch in flight.
	w.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { kind: ReqKind::CollationFetchingV2, .. }),
		1,
		"exactly 1 fetch in flight while A1 is being fetched",
	);

	// Resolve A1, then assert the next fetch fired (after this point in the recorder) is
	// for B1, not A2 — earlier CQ position wins.
	let barrier = w.recorder_barrier();
	w.respond_fetch_v2(a1_req, a1.receipt.clone(), crate::builders::Candidate::empty_pov());
	w.expect_second(&a1);
	w.sim.advance(Duration::from_millis(50));

	let next = w.first_fetch_after(barrier).expect("a fetch fires after A1 seconding");
	assert_eq!(
		next.1,
		Some(b1.hash()),
		"first fetch after A1 must be B1 (CQ position 0), not A2",
	);
}
