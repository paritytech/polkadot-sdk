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

//! V1 advertisement → fetch → second → `Seconded` notification flows back from backing
//! to collator-protocol → validator notifies the original collator with a wire-level
//! `CollationSeconded` message.
//!
//! Both impls send the wire notification — that's the shared spec asserted here. Legacy
//! also emits a `Reputation::Benefit` (`BENEFIT_NOTIFY_GOOD`); experimental updates the
//! persistent rep store silently. The rep emission divergence is documented in
//! [`crate::scenarios::divergent::reputation_emission`].

use crate::scenarios::shared::WorldExt as _;
use crate::{
	builders::{Candidate, ProtocolVersion::V1},
	contract::{Effect, WireMsgKind},
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_primitives::{CoreIndex, HeadData, Id as ParaId};
use std::time::Duration;

const PARA: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn v1_advertise_fetch_second_and_collator_notified<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let leaf = w.leaf();
	let candidate = w.candidate_at(leaf)
		.para(PARA)
		.parent_head(HeadData(Vec::new()))
		.head_data(HeadData(vec![1]))
		.build();
	// Register outputs so validation stub returns matching commitments — fragment chain
	// would reject otherwise (validated head_data ≠ descriptor.para_head).
	w.outputs.insert(candidate.hash(), candidate.commitments.clone(), candidate.pvd.clone());

	let peer = w.declared_peer(PARA, V1);
	w.base.sim.send(peer.advertise(leaf, None, None));
	let (_, request_id, _) = w.expect_any_fetch();
	w.respond_fetch_v1(request_id, candidate.receipt.clone(), Candidate::empty_pov());
	w.expect_second(&candidate);

	// Let the back-notification flow through statement-distribution-noop and back to
	// collator-protocol.
	w.base.sim.advance(Duration::from_millis(100));

	let _ = w.base.sim.expect(
		|e| matches!(
			e,
			Effect::SendCollation {
				peers,
				kind: WireMsgKind::CollationSeconded { .. },
			} if peers.contains(&peer.peer_id),
		),
		Duration::from_millis(500),
		"Effect::SendCollation CollationSeconded targeting the collator peer",
	);
}
