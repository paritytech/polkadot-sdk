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

//! Tests covering invariants that PR #12004 (avoid duplicate fetches; drops the fetch
//! penalty box) introduces. Marked `bug_on = "experimental"` because the assertions fail
//! against pre-#12004 experimental — merging the PR flips the `should_panic`.
//!
//! Upstream PR: https://github.com/paritytech/polkadot-sdk/pull/12004
//!
//! Coverage:
//! - [`v2_same_candidate_from_multiple_peers_fetched_once`] — two V2 peers carry the
//!   same offer; one fetch must fire.
//! - [`cross_protocol_version_carriers_fetched_once`] — V2 carrier and V3 carrier of
//!   the same V2-descriptor offer get deduped to one fetch.
//! - [`v2_co_carrier_rep_arbitration_picks_high_rep_peer`] — when multiple carriers
//!   have the same offer, the rep-best peer serves. (experimental-only; depends on
//!   experimental's score store.)

use crate::scenarios::shared::WorldExt as _;
use crate::{
	builders::{ProtocolVersion::V2, ProtocolVersion::V3},
	contract::Effect,
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_node_subsystem::OverseerSignal;
use polkadot_primitives::{
	CandidateEvent, CoreIndex, GroupIndex, HeadData, Id as ParaId,
};
use std::time::Duration;

const PARA: ParaId = ParaId::new(100);

/// Two V2 peers advertise the same candidate (same hash, same offer); one fetch fires.
/// Pre-#12004: two fetches, because `Advertisement` keys on `(offer, peer_id)`. Post-
/// #12004: dedup keys on the offer alone, with the peer chosen by rep arbitration.
#[crate::sim_test(
	bug_on = "experimental",
	bug_url = "github:paritytech/polkadot-sdk#12004"
)]
fn v2_same_candidate_from_multiple_peers_fetched_once<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let leaf = w.leaf();

	let peer_a = w.declared_peer(PARA, V2);
	let peer_b = w.declared_peer(PARA, V2);
	// Same candidate (hash) advertised by both peers.
	let cand = w.candidate_at(leaf).para(PARA).build();
	w.advertise_with_parent_head(&peer_a, leaf, cand.hash(), cand.parent_head_hash());
	w.advertise_with_parent_head(&peer_b, leaf, cand.hash(), cand.parent_head_hash());

	// Settle long enough that any second concurrent fetch would have fired.
	w.base.sim.advance(Duration::from_millis(300));

	w.base.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		1,
		"exactly one fetch for the shared V2 candidate (must NOT fire one per carrier)",
	);
}

/// V2 peer and V3 peer both carry the same V2-descriptor offer; one fetch fires.
/// V3 protocol may legitimately advertise a V2 descriptor — the validator must dedup
/// by offer (descriptor) regardless of the carrier's protocol version.
#[crate::sim_test(
	bug_on = "experimental",
	bug_url = "github:paritytech/polkadot-sdk#12004"
)]
fn cross_protocol_version_carriers_fetched_once<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let leaf = w.leaf();

	let peer_v2 = w.declared_peer(PARA, V2);
	let peer_v3 = w.declared_peer(PARA, V3);
	let cand = w.candidate_at(leaf).para(PARA).build();
	w.advertise_with_parent_head(&peer_v2, leaf, cand.hash(), cand.parent_head_hash());
	w.advertise_with_parent_head(&peer_v3, leaf, cand.hash(), cand.parent_head_hash());
	w.base.sim.advance(Duration::from_millis(300));
	w.base.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		1,
		"exactly one fetch across V2 + V3 carriers (offer-keyed dedup)",
	);
}

/// Reputation arbitration when multiple carriers offer the same candidate. peer_high
/// has score 1 (one past inclusion); peer_low has 0. Both advertise the same offer;
/// the single fetch must go to peer_high.
#[crate::sim_test(only = "experimental", bug_on = "experimental",
	bug_url = "github:paritytech/polkadot-sdk#12004")]
fn v2_co_carrier_rep_arbitration_picks_high_rep_peer<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let leaf0 = w.leaf();

	// Ramp peer_high to score 1 via finalize-with-included-candidate.
	let peer_high = w.declared_peer(PARA, V2);
	let cand_seed = w
		.candidate_at(leaf0)
		.para(PARA)
		.parent_head(HeadData(Vec::new()))
		.head_data(HeadData(vec![1]))
		.approved_peer(peer_high.peer_id)
		.build();
	w.outputs.insert(cand_seed.hash(), cand_seed.commitments.clone(), cand_seed.pvd.clone());
	w.full_second(&peer_high, &cand_seed);
	{
		let mut chain = w.base.chain.lock();
		chain.set_pending_availability(PARA, vec![cand_seed.committed()]);
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
	w.base.sim.signal(OverseerSignal::BlockFinalized(leaf0, w.leaf_number()));
	w.base.sim.advance(Duration::from_millis(50));

	// New leaf for the arbitration round.
	let leaf1 = w.extend_and_activate_with(leaf0, &[leaf0], |_chain, _h, _n| {});
	let peer_low = w.declared_peer(PARA, V2);

	// Both carriers offer the same new candidate.
	let cand = w
		.candidate_at(leaf1)
		.para(PARA)
		.parent_head(cand_seed.output_head())
		.head_data(HeadData(vec![2]))
		.build();
	w.advertise_with_parent_head(&peer_high, leaf1, cand.hash(), cand.parent_head_hash());
	w.advertise_with_parent_head(&peer_low, leaf1, cand.hash(), cand.parent_head_hash());
	w.base.sim.advance(Duration::from_millis(50));

	// Exactly one fetch, targeted at peer_high.
	let _ = w.expect_fetch_to(peer_high.peer_id);
	w.base.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		1,
		"exactly one fetch and it goes to the rep-best peer",
	);
}
