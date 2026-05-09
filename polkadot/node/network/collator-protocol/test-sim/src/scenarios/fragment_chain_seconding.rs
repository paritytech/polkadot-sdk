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

//! Smoke test for the fragment-chain helpers on `World`.
//!
//! Builds two candidates anchored at the same leaf where the second is the child of the
//! first (`parent_head = first.output_head()`), seconds them in order via
//! [`World::full_second`], and asserts both reach the validator's `SecondCandidate` effect.
//!
//! Framework-level proof of correctness for `World::full_second` chained.

use crate::scenarios::shared::WorldExt as _;
use crate::{
	builders::{Candidate, ProtocolVersion::V2},
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_primitives::{CoreIndex, HeadData, Id as ParaId};

const PARA_A: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn parent_then_child_seconds_in_order<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA_A)]);
	let leaf_n = w.leaf_number();

	// Parent: parent_head=empty, output=vec![1]. Child: parent_head=vec![1], output=vec![2].
	let parent = Candidate::builder()
		.para(PARA_A)
		.relay_parent(w.leaf())
		.relay_parent_number(leaf_n)
		.parent_head(HeadData(Vec::new()))
		.head_data(HeadData(vec![1]))
		.build();
	let child = Candidate::builder()
		.para(PARA_A)
		.relay_parent(w.leaf())
		.relay_parent_number(leaf_n)
		.parent_head(parent.output_head())
		.head_data(HeadData(vec![2]))
		.build();

	let peer = w.declared_peer(PARA_A, V2);
	w.full_second(&peer, &parent);
	w.full_second(&peer, &child);
}
