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

//! Claim-queue window arithmetic across the leaf/ancestor boundary.
//!
//! Three sibling scenarios share the same setup (`leaf CQ = [B, B, A]`, schedule defaults
//! to B everywhere else) and probe the window-arithmetic contract from three angles:
//!
//! * [`para_at_last_claim_queue_position_accepts_at_leaf`] — leaf has offset=0,
//!   valid_len=3 → A at position 2 is in-window → accepts.
//! * [`ancestor_with_para_at_valid_position_accepts`] — ancestor R has offset=1,
//!   valid_len=2; with `leaf CQ = [A, B, A]` para A is at position 0 → in-window → accepts
//!   (mirrors upstream `non_obsolete_position_accepted`).
//! * [`ancestor_with_para_at_obsolete_position_rejects`] — ancestor R has offset=1,
//!   valid_len=2; leaf CQ = [B, B, A] keeps A at position 2 → out-of-window → silently
//!   rejected (mirrors upstream `obsolete_positions_rejected`).
//!
//! The "ancestor accepts" case fails on experimental for an unrelated reason —
//! `claim_queue_state` is keyed by leaf only, so any ancestor-RP advertisement is rejected
//! before the window check runs. See
//! `project_collator_experimental_no_ancestor_rp_advertise.md`.

use crate::scenarios::shared::WorldExt as _;
use crate::{
	builders::ProtocolVersion::V2,
	chain::CoreSchedule,
	harness::CollatorSut,
	scenarios::shared::{
		build_with_ancestors_world, build_with_ancestors_world_with_config, ChainConfig,
		LeafSelector,
	},
};
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::{
	collections::{BTreeMap, VecDeque},
	time::Duration,
};

const PARA_A: ParaId = ParaId::new(2000);
const PARA_B: ParaId = ParaId::new(999);

/// Builds a world with `n_ancestors` ancestors, schedule defaults to B on core 0, and the
/// leaf claim queue overridden to `cq`. Used by all three scenarios in this file.
fn world_with_leaf_cq<S: CollatorSut>(
	n_ancestors: usize,
	cq: [ParaId; 3],
) -> crate::scenarios::shared::World<S> {
	let mut leaf_q = BTreeMap::new();
	leaf_q.insert(CoreIndex(0), VecDeque::from(cq.to_vec()));
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_B))
		.with_claim_queue_at(LeafSelector::Leaf, leaf_q);
	build_with_ancestors_world_with_config::<S>(n_ancestors, config)
}

#[crate::sim_test]
fn para_at_last_claim_queue_position_accepts_at_leaf<S: CollatorSut>() {
	let mut w = world_with_leaf_cq::<S>(0, [PARA_B, PARA_B, PARA_A]);
	let peer = w.declared_peer(PARA_A, V2);
	let cand = w.advertise(&peer, w.leaf(), PARA_A);
	let _ = w.fetch_request(&cand);
}

#[crate::sim_test(
	bug_on = "experimental",
	bug_url = "memory:project_collator_experimental_no_ancestor_rp_advertise"
)]
fn ancestor_with_para_at_valid_position_accepts<S: CollatorSut>() {
	// Plain build_with_ancestors_world: same para A scheduled at every block. Ancestor R
	// has para A at position 0 → in-window for the offset=1 ancestor.
	let mut w = build_with_ancestors_world::<S>(1, &[(CoreIndex(0), PARA_A)]);
	let peer = w.declared_peer(PARA_A, V2);
	let r = w.ancestors()[0];
	let cand = w.advertise(&peer, r, PARA_A);
	let _ = w.fetch_request(&cand);
}

#[crate::sim_test]
fn ancestor_with_para_at_obsolete_position_rejects<S: CollatorSut>() {
	let mut w = world_with_leaf_cq::<S>(1, [PARA_B, PARA_B, PARA_A]);
	let peer = w.declared_peer(PARA_A, V2);
	let r = w.ancestors()[0];
	let cand = w.advertise(&peer, r, PARA_A);
	w.no_fetch_for(&cand, Duration::from_millis(200));
}

/// Two ancestors deep: with `allowed_ancestry_len = 2`, advertisements at both
/// leaf-parent and grandparent must fetch. Mirrors upstream
/// `accept_advertisements_from_implicit_view` (simplified to a single para — the
/// upstream multi-para shape needs validator-group rotation we don't model here, and
/// the property under test is implicit-view ancestor acceptance, not group rotation).
///
/// KNOWN BUG (experimental): same root cause as
/// `ancestor_with_para_at_valid_position_accepts` — `claim_queue_state` keyed by leaf only.
/// See `memory:project_collator_experimental_no_ancestor_rp_advertise`.
#[crate::sim_test(
	bug_on = "experimental",
	bug_url = "memory:project_collator_experimental_no_ancestor_rp_advertise"
)]
fn ancestor_advertisements_at_parent_and_grandparent_both_fetch<S: CollatorSut>() {
	let mut w = build_with_ancestors_world::<S>(2, &[(CoreIndex(0), PARA_A)]);
	let peer = w.declared_peer(PARA_A, V2);
	let parent = w.ancestors()[0];
	let grandparent = w.ancestors()[1];

	let cand_at_parent = w.advertise(&peer, parent, PARA_A);
	let cand_at_grandparent = w.advertise(&peer, grandparent, PARA_A);

	let _ = w.fetch_request(&cand_at_parent);
	let _ = w.fetch_request(&cand_at_grandparent);
}
