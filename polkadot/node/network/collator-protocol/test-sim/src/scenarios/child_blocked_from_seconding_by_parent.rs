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

//! Mirrors `validator_side/tests/prospective_parachains.rs::child_blocked_from_seconding_by_parent`.
//!
//! Child B (parent_head=[1], output=[2]) advertised before parent A (parent_head=empty,
//! output=[1]). B is fetched but blocks on parent before seconding. After A is advertised,
//! fetched, and seconded, B becomes unblocked and seconds.

use crate::{
	builders::{Candidate, ProtocolVersion::V2},
	contract::Effect,
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_primitives::{CoreIndex, HeadData, Id as ParaId};
use std::time::Duration;

const PARA: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn child_advertised_first_blocks_then_unblocks_after_parent_seconds<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let leaf = w.leaf();
	let leaf_n = w.leaf_number();

	let parent = Candidate::builder()
		.para(PARA)
		.relay_parent(leaf)
		.relay_parent_number(leaf_n)
		.parent_head(HeadData(Vec::new()))
		.head_data(HeadData(vec![1]))
		.build();
	let child = Candidate::builder()
		.para(PARA)
		.relay_parent(leaf)
		.relay_parent_number(leaf_n)
		.parent_head(HeadData(vec![1]))
		.head_data(HeadData(vec![2]))
		.build();

	let peer = w.declared_peer(PARA, V2);

	// Register both candidates' outputs ahead of time so when validation fires the stub has
	// the right answer.
	w.outputs.insert(child.hash(), child.commitments.clone(), child.pvd.clone());

	// Child advertised first. Validator fetches; backing+prospective hold seconding because
	// the parent isn't in the fragment chain yet.
	w.advertise_with_parent_head(&peer, leaf, child.hash(), child.parent_head_hash());
	let child_request = w.fetch_request(&child);
	w.respond_fetch_v2(child_request, child.receipt.clone(), Candidate::empty_pov());

	// Parent now flows through full advertise → fetch → second. Backing seconds parent and
	// notifies prospective — child becomes a fragment-chain member.
	w.full_second(&peer, &parent);

	// Child should be seconded too — it was already fetched and held; the parent's
	// seconding unblocks it.
	w.expect_second(&child);
	let _ = Duration::from_millis(0); // keep import below relevant if elided
}
