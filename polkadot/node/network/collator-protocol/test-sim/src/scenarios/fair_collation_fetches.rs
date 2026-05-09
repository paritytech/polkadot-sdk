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

//! Mirrors `validator_side/tests/prospective_parachains.rs::fair_collation_fetches`.
//!
//! Shared-core setup: leaf CQ = `[B, A, A]` (position 0 holds para B, positions 1+2 hold
//! para A). With `lookahead = 3`:
//!
//! - Peer for A seconds 2 candidates (chained: empty→[1], [1]→[2]) — claim slots 1, 2.
//! - Peer for B seconds 1 candidate (parent_head=empty, output=[1]) — claim slot 0.
//!
//! After all 3 slots are full, further advertisements at the leaf get silently dropped —
//! claim queue exhausted.

use crate::scenarios::shared::WorldExt as _;
use crate::{
	builders::{Candidate, ProtocolVersion::V2},
	chain::CoreSchedule,
	harness::CollatorSut,
	scenarios::shared::{build_with_ancestors_world_with_config, ChainConfig, LeafSelector},
};
use polkadot_primitives::{CandidateHash, CoreIndex, HeadData, Hash, Id as ParaId};
use std::{
	collections::{BTreeMap, VecDeque},
	time::Duration,
};

const PARA_A: ParaId = ParaId::new(2000);
const PARA_B: ParaId = ParaId::new(2001);

fn shared_core_world<S: CollatorSut>() -> crate::scenarios::shared::World<S> {
	let mut leaf_q = BTreeMap::new();
	leaf_q.insert(CoreIndex(0), VecDeque::from(vec![PARA_B, PARA_A, PARA_A]));
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_A))
		.with_claim_queue_at(LeafSelector::Leaf, leaf_q);
	build_with_ancestors_world_with_config::<S>(0, config)
}

#[crate::sim_test]
fn shared_core_fills_per_para_lookahead_then_rejects_more<S: CollatorSut>() {
	let mut w = shared_core_world::<S>();
	let leaf = w.leaf();
	let leaf_n = w.leaf_number();

	// Para A: chain of 2 candidates.
	let a1 = Candidate::builder()
		.para(PARA_A)
		.relay_parent(leaf)
		.relay_parent_number(leaf_n)
		.parent_head(HeadData(Vec::new()))
		.head_data(HeadData(vec![1]))
		.build();
	let a2 = Candidate::builder()
		.para(PARA_A)
		.relay_parent(leaf)
		.relay_parent_number(leaf_n)
		.parent_head(a1.output_head())
		.head_data(HeadData(vec![2]))
		.build();

	// Para B: single candidate.
	let b1 = Candidate::builder()
		.para(PARA_B)
		.relay_parent(leaf)
		.relay_parent_number(leaf_n)
		.parent_head(HeadData(Vec::new()))
		.head_data(HeadData(vec![10]))
		.build();

	let peer_a = w.declared_peer(PARA_A, V2);
	let peer_b = w.declared_peer(PARA_B, V2);

	w.full_second(&peer_a, &a1);
	w.full_second(&peer_a, &a2);
	w.full_second(&peer_b, &b1);

	// 4th advertisement on either para must NOT trigger any fetch.
	let extra_a = CandidateHash(Hash::repeat_byte(0xAA));
	w.advertise_with_parent_head(&peer_a, leaf, extra_a, Hash::zero());
	let extra_b = CandidateHash(Hash::repeat_byte(0xBB));
	w.advertise_with_parent_head(&peer_b, leaf, extra_b, Hash::zero());

	w.no_fetch_within(Duration::from_millis(200));
}

/// Para B advertised after para A still gets fetched — earlier claim-queue entry wins.
/// CQ=[B,A,A]: B occupies position 0 (earliest), A at 1+2. Even when peer A starts queue
/// first, peer B's advertisement triggers a fetch for B once B's slot opens.
#[crate::sim_test]
fn shared_core_para_b_can_fetch_alongside_para_a<S: CollatorSut>() {
	let mut w = shared_core_world::<S>();
	let leaf = w.leaf();
	let leaf_n = w.leaf_number();

	let a1 = Candidate::builder()
		.para(PARA_A).relay_parent(leaf).relay_parent_number(leaf_n)
		.parent_head(HeadData(Vec::new())).head_data(HeadData(vec![1])).build();
	let b1 = Candidate::builder()
		.para(PARA_B).relay_parent(leaf).relay_parent_number(leaf_n)
		.parent_head(HeadData(Vec::new())).head_data(HeadData(vec![10])).build();

	let peer_a = w.declared_peer(PARA_A, V2);
	let peer_b = w.declared_peer(PARA_B, V2);

	w.full_second(&peer_a, &a1);
	w.full_second(&peer_b, &b1);
}

/// 4th advertisement for para A on a shared-core CQ where para A holds 2 slots is
/// silently rejected — claim slots full for A. (Also exercised by the headline test
/// `shared_core_fills_per_para_lookahead_then_rejects_more`.)
#[crate::sim_test]
fn shared_core_third_para_a_advertisement_silently_dropped<S: CollatorSut>() {
	let mut w = shared_core_world::<S>();
	let leaf = w.leaf();
	let leaf_n = w.leaf_number();

	let a1 = Candidate::builder()
		.para(PARA_A).relay_parent(leaf).relay_parent_number(leaf_n)
		.parent_head(HeadData(Vec::new())).head_data(HeadData(vec![1])).build();
	let a2 = Candidate::builder()
		.para(PARA_A).relay_parent(leaf).relay_parent_number(leaf_n)
		.parent_head(a1.output_head()).head_data(HeadData(vec![2])).build();

	let peer_a = w.declared_peer(PARA_A, V2);
	w.full_second(&peer_a, &a1);
	w.full_second(&peer_a, &a2);

	let extra_hash = CandidateHash(Hash::repeat_byte(0xCC));
	w.advertise_with_parent_head(&peer_a, leaf, extra_hash, Hash::zero());
	w.no_fetch_within(Duration::from_millis(200));
}
