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

//! V1 advertisement → fetch → second → `Seconded` notification flows back from backing to
//! collator-protocol → validator notifies the original collator with a
//! `CollationSeconded` wire message + `BENEFIT_NOTIFY_GOOD` reputation bump.
//!
//! KNOWN-FAILING (both impls): the back-notification round-trip via
//! `IntroduceSecondedCandidate` to real prospective-parachains gets stuck. The
//! `CollatorProtocolMessage::Seconded` notification never reaches the validator, so the
//! `CollationSeconded` wire message is never sent. Either the chain shape is missing
//! something prospective needs, or the back-channel from backing → collator-protocol
//! requires extra plumbing in the harness. Tracked under deferred items — likely fixes
//! itself once the fragment-chain non-empty-heads gap is resolved.

use crate::{
	builders::{Candidate, ProtocolVersion::V1},
	contract::{Effect, RepBucket, ReqKind, WireMsgKind},
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_node_subsystem_util::reputation::REPUTATION_CHANGE_INTERVAL;
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::time::Duration;

const PARA: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn v1_advertise_fetch_second_and_collator_notified<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let leaf = w.leaf();
	let leaf_n = w.leaf_number();

	let candidate = Candidate::builder()
		.para(PARA)
		.relay_parent(leaf)
		.relay_parent_number(leaf_n)
		.parent_head(polkadot_primitives::HeadData(Vec::new()))
		.head_data(polkadot_primitives::HeadData(vec![1]))
		.build();

	// Register so validation stub returns matching commitments. Otherwise fragment chain
	// rejects when the validated outputs disagree with the descriptor's para_head.
	w.outputs.insert(
		candidate.hash(),
		candidate.commitments.clone(),
		candidate.pvd.clone(),
	);

	let peer = w.declared_peer(PARA, V1);
	w.sim.send(peer.advertise(leaf, None, None));
	// V1 fetch (no candidate_hash). Match generic SendRequest.
	let send_request = w.sim.expect(
		|e| matches!(e, Effect::SendRequest { kind: ReqKind::CollationFetchingV1, .. }),
		Duration::from_millis(500),
		"Effect::SendRequest CollationFetchingV1",
	);
	let request_id = send_request.request_id().expect("RequestId");
	w.respond_fetch_v1(request_id, candidate.receipt.clone(), Candidate::empty_pov());
	w.expect_second(&candidate);

	// Backing's `Seconded` notification flows through statement-distribution-noop and back.
	// Give the executor a moment to settle before checking for the wire-side notification.
	w.sim.advance(Duration::from_millis(100));

	let _ = w.sim.expect(
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
	// `BENEFIT_NOTIFY_GOOD` is `BenefitMinor` → buffered by `ReputationAggregator`.
	w.sim.advance(REPUTATION_CHANGE_INTERVAL + Duration::from_secs(1));
	w.expect_rep(&peer, RepBucket::Benefit);
}
