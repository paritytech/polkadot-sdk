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

//! Scenario: a peer connects and then sends a `Declare` with a bogus signature. The validator
//! penalises the peer with a `Malicious` reputation hit and emits no further effects.
//!
//! Spec the test is checking, in plain English:
//!
//! - Given a fresh validator side and one connected peer.
//! - When that peer sends a `Declare` whose signature does not verify against the declared
//!   collator key.
//! - Then the validator emits a `Reputation { Malicious }` for the peer.
//!
//! This scenario does not need any `RuntimeApi` / `ChainApi` setup: the bad signature is
//! rejected before the validator looks at chain state, so a panic-on-query responder is the
//! right safety net.

use crate::{
	builders::{Peer, ProtocolVersion},
	contract::{Effect, Query, RepBucket},
	harness::{dispatcher::AnswerQuery, Sim, SimConfig},
	impls::LegacyValidator,
};
use polkadot_primitives::Id as ParaId;
use std::time::Duration;

struct PanicResponder;
impl AnswerQuery for PanicResponder {
	fn answer(&mut self, query: Query) {
		panic!(
			"bad-signature scenario expected no queries before reaching the rejection path; got {:?}",
			query
		);
	}
}

#[crate::sim_test]
fn declare_with_bad_signature_yields_malicious_reputation() {
	let mut sim = Sim::<LegacyValidator>::start(SimConfig::default(), PanicResponder);

	let peer = Peer::new(ParaId::from(2000), ProtocolVersion::V1);

	sim.send(peer.connected());
	sim.send(peer.declare_with_bad_signature());

	let observed = sim.expect(
		|effect| matches!(effect, Effect::Reputation { bucket: RepBucket::Malicious, peer: p } if *p == peer.peer_id),
		Duration::from_millis(50),
		"Effect::Reputation { Malicious } for the bad-signature peer",
	);
	match observed {
		Effect::Reputation { peer: observed_peer, bucket } => {
			assert_eq!(observed_peer, peer.peer_id);
			assert_eq!(bucket, RepBucket::Malicious);
		},
		other => panic!("predicate matched but variant unexpected: {:?}", other),
	}
}
