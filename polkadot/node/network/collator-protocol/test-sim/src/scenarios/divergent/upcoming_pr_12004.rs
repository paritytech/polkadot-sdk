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
//!   same offer; one fetch must fire (not one per carrier). Pre-#12004 the dedup key
//!   includes `peer_id`, so two carriers → two fetches.

use crate::{
	builders::ProtocolVersion::V2,
	contract::Effect,
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_primitives::{CoreIndex, Id as ParaId};
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
	w.sim.advance(Duration::from_millis(300));

	w.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		1,
		"exactly one fetch for the shared V2 candidate (must NOT fire one per carrier)",
	);
}
