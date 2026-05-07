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

//! Fluent test surface on top of the `World` / `WithAncestorsWorld` builders.
//!
//! Scenarios shouldn't have to spell out
//!
//! ```ignore
//! world.sim.send(peer.connected());
//! world.sim.send(peer.declare());
//! world.sim.send(peer.advertise(rp, Some(c.hash()), Some(parent_head_hash)));
//! let send_request = world.sim.expect(
//!     |e| matches!(e, Effect::SendRequest { .. } if /* ... */),
//!     Duration::from_millis(500),
//!     "...",
//! );
//! let request_id = send_request.request_id().expect("...");
//! ```
//!
//! Five lines of boilerplate to assert "advertise → fetch fired" in every scenario adds up.
//! These methods compress the common patterns down to:
//!
//! ```ignore
//! let peer = w.declared_peer(para, V2);
//! let cand = w.advertise(&peer, w.leaf, para);
//! let _ = w.fetch_request(&cand);
//! ```
//!
//! Scenarios that need the raw API still have `world.sim.send(...)` and `world.sim.expect(...)`
//! available — these helpers are additive, not gating.

use crate::{
	builders::{Candidate, Peer, ProtocolVersion},
	contract::{Effect, RepBucket, ReqKind, RequestId},
	harness::CollatorSut,
	scenarios::shared::World,
};
use codec::Encode;
use polkadot_node_network_protocol::request_response::{v1 as protocol_v1, v2 as protocol_v2};
use polkadot_node_primitives::PoV;
use polkadot_primitives::{
	CandidateHash, CandidateReceiptV2, HeadData, Hash, Id as ParaId,
};
use sc_network::ProtocolName;
use std::time::Duration;

/// Default budget for "happy-path" expectations (advertise → fetch → respond → second).
/// Long enough to cover the few internal future yields the real backing pipeline needs.
const HAPPY_PATH_TIMEOUT: Duration = Duration::from_millis(500);

/// Default budget for negative assertions (`expect_no_*`). Short — we want fast feedback if
/// the effect *does* fire and don't want to wait the full happy-path budget for nothing.
const NEGATIVE_TIMEOUT: Duration = Duration::from_millis(100);

impl<S: CollatorSut> World<S> {
	/// Connect a peer and immediately have it `Declare` for `para` over `version`. Returns
	/// the [`Peer`] for further use (advertise, expect-rep, etc.).
	pub fn declared_peer(&mut self, para: ParaId, version: ProtocolVersion) -> Peer {
		let peer = self.connected_peer(para, version);
		self.sim.send(peer.declare());
		peer
	}

	/// Connect a peer without declaring. Useful for bad-signature tests, undeclared-eviction
	/// tests, and any other scenario that wants to drive the connect/declare boundary by
	/// hand.
	pub fn connected_peer(&mut self, para: ParaId, version: ProtocolVersion) -> Peer {
		let peer = Peer::new(para, version);
		self.sim.send(peer.connected());
		peer
	}

	/// Build a fresh candidate for `para` at `relay_parent`, send a V2 advertisement for it
	/// from `peer` (parent_head_data = empty), and return the candidate. The advertisement
	/// uses `Hash::default()` as parent-head-data hash unless the scenario specifies one
	/// via [`Self::advertise_with_parent_head`].
	///
	/// V1 peers get a V1 advertisement (no candidate hash on the wire); the returned
	/// `Candidate` is still the constructed receipt so test code can use its hash for
	/// later assertions.
	pub fn advertise(&mut self, peer: &Peer, relay_parent: Hash, para: ParaId) -> Candidate {
		let candidate = Candidate::for_para_at(para, relay_parent);
		let parent_head_hash = HeadData(Vec::new()).hash();
		self.sim.send(peer.advertise(
			relay_parent,
			Some(candidate.hash()),
			Some(parent_head_hash),
		));
		candidate
	}

	/// Variant of [`Self::advertise`] that lets the caller specify the parent-head-data hash
	/// on the wire. The returned candidate is unchanged — use [`Candidate::for_para_at`]
	/// directly if a custom candidate shape is needed.
	pub fn advertise_with_parent_head(
		&mut self,
		peer: &Peer,
		relay_parent: Hash,
		candidate_hash: CandidateHash,
		parent_head_hash: Hash,
	) {
		self.sim.send(peer.advertise(
			relay_parent,
			Some(candidate_hash),
			Some(parent_head_hash),
		));
	}

	/// Send a V3 advertisement with full control of `scheduling_parent`, `relay_parent`,
	/// and `descriptor_version`. The `peer` must be V3.
	pub fn advertise_v3(
		&mut self,
		peer: &Peer,
		scheduling_parent: Hash,
		relay_parent: Hash,
		candidate_hash: CandidateHash,
		parent_head_hash: Hash,
		descriptor_version: polkadot_primitives::CandidateDescriptorVersion,
	) {
		self.sim.send(peer.advertise_v3(
			scheduling_parent,
			relay_parent,
			candidate_hash,
			parent_head_hash,
			descriptor_version,
		));
	}

	/// Wait for `Effect::SendRequest CollationFetchingV{1,2}` whose candidate hash matches
	/// `candidate`. Returns the [`RequestId`] so the test can later
	/// [`Self::respond_fetch_collation`] or drop it on the floor.
	pub fn fetch_request(&mut self, candidate: &Candidate) -> RequestId {
		let send_request = self.sim.expect(
			|e| match e {
				Effect::SendRequest {
					kind: ReqKind::CollationFetchingV1, candidate_hash: None, ..
				} => true,
				Effect::SendRequest {
					kind: ReqKind::CollationFetchingV2, candidate_hash: Some(c), ..
				} if *c == candidate.hash() => true,
				_ => false,
			},
			HAPPY_PATH_TIMEOUT,
			"Effect::SendRequest CollationFetching for the advertised candidate",
		);
		send_request.request_id().expect("SendRequest carries a RequestId")
	}

	/// Assert that **no** fetch fires for `candidate` within `within`.
	pub fn no_fetch_for(&mut self, candidate: &Candidate, within: Duration) {
		self.sim.expect_no(
			|e| matches!(
				e,
				Effect::SendRequest { candidate_hash: Some(c), .. } if *c == candidate.hash(),
			),
			within,
			"SendRequest for the candidate (must NOT fire)",
		);
	}

	/// Assert that **no** fetch of any kind fires within `within`. Useful when a scenario's
	/// invariant is "advertisement was rejected; nothing happened downstream."
	pub fn no_fetch_within(&mut self, within: Duration) {
		self.sim.expect_no(
			|e| matches!(e, Effect::SendRequest { .. }),
			within,
			"any SendRequest (must NOT fire)",
		);
	}

	/// Encode a V2 `CollationFetchingResponse::Collation(receipt, pov)` and resolve the
	/// pending fetch identified by `request_id`.
	pub fn respond_fetch_v2(
		&mut self,
		request_id: RequestId,
		receipt: CandidateReceiptV2,
		pov: PoV,
	) {
		let response = protocol_v2::CollationFetchingResponse::Collation(receipt, pov);
		self.sim
			.respond_fetch(request_id, Ok((response.encode(), ProtocolName::from(""))));
	}

	/// V1 variant of [`Self::respond_fetch_v2`].
	pub fn respond_fetch_v1(
		&mut self,
		request_id: RequestId,
		receipt: CandidateReceiptV2,
		pov: PoV,
	) {
		let response = protocol_v1::CollationFetchingResponse::Collation(receipt.into(), pov);
		self.sim
			.respond_fetch(request_id, Ok((response.encode(), ProtocolName::from(""))));
	}

	/// Encode a V2 `CollationFetchingResponse::CollationWithParentHeadData { ... }` and
	/// resolve the fetch. `parent_head_data` is sent as-is — passing a value whose hash
	/// differs from the advertised parent-head hash is the canonical sanity-check failure
	/// scenario.
	pub fn respond_fetch_v2_with_parent_head(
		&mut self,
		request_id: RequestId,
		receipt: CandidateReceiptV2,
		pov: PoV,
		parent_head_data: HeadData,
	) {
		let response = protocol_v2::CollationFetchingResponse::CollationWithParentHeadData {
			receipt,
			pov,
			parent_head_data,
		};
		self.sim
			.respond_fetch(request_id, Ok((response.encode(), ProtocolName::from(""))));
	}

	/// Run a full advertise → fetch → respond → second cycle for `candidate` over V2.
	/// Returns once the validator has emitted `Effect::SecondCandidate` for this hash.
	///
	/// Use in fragment-chain scenarios that need to second N candidates back-to-back. Build
	/// each candidate via [`Candidate::builder`] threading `parent_head` and `head_data`
	/// (the previous candidate's `output_head` becomes the next's `parent_head`), then call
	/// this helper for each in order.
	///
	/// # Panics
	///
	/// On expectation failures (no fetch, no second), the underlying `Sim::expect` calls
	/// dump the timeline and panic — same dev-ex as a hand-rolled scenario.
	pub fn full_second(&mut self, peer: &Peer, candidate: &Candidate) {
		self.outputs.insert(
			candidate.hash(),
			candidate.commitments.clone(),
			candidate.pvd.clone(),
		);
		self.advertise_with_parent_head(
			peer,
			candidate.relay_parent(),
			candidate.hash(),
			candidate.parent_head_hash(),
		);
		let request_id = self.fetch_request(candidate);
		self.respond_fetch_v2(request_id, candidate.receipt.clone(), Candidate::empty_pov());
		self.expect_second(candidate);
		// `Effect::SecondCandidate` is observed at the moment collator-protocol dispatches
		// `CandidateBackingMessage::Second` to backing — *before* backing has run validation
		// and forwarded `IntroduceSecondedCandidate` to prospective. Subsequent calls in a
		// fragment chain need prospective to have absorbed this candidate, so we let the
		// downstream pipeline flush before returning.
		self.sim.advance(Duration::from_millis(200));
	}

	/// Wait for `Effect::SecondCandidate` whose candidate hash equals `candidate`'s.
	pub fn expect_second(&mut self, candidate: &Candidate) {
		let _ = self.sim.expect(
			|e| matches!(
				e,
				Effect::SecondCandidate { candidate_hash, .. } if candidate_hash == &candidate.hash()
			),
			HAPPY_PATH_TIMEOUT,
			"Effect::SecondCandidate for the candidate",
		);
	}

	/// Wait for `Effect::Reputation { peer, bucket }` matching `peer` and `bucket`.
	pub fn expect_rep(&mut self, peer: &Peer, bucket: RepBucket) {
		let _ = self.sim.expect(
			|e| matches!(
				e,
				Effect::Reputation { peer: p, bucket: b } if *p == peer.peer_id && *b == bucket,
			),
			HAPPY_PATH_TIMEOUT,
			"Effect::Reputation for peer (matching bucket)",
		);
	}

	/// Assert that **no** `Effect::Reputation` for `peer` with the given `bucket` fires
	/// within [`NEGATIVE_TIMEOUT`].
	pub fn expect_no_rep(&mut self, peer: &Peer, bucket: RepBucket) {
		self.sim.expect_no(
			|e| matches!(
				e,
				Effect::Reputation { peer: p, bucket: b } if *p == peer.peer_id && *b == bucket,
			),
			NEGATIVE_TIMEOUT,
			"Effect::Reputation for peer (must NOT fire with this bucket)",
		);
	}

	/// Wait for `Effect::DisconnectPeers` on the Collation peer-set carrying `peer`.
	pub fn expect_disconnect(&mut self, peer: &Peer) {
		use polkadot_node_network_protocol::peer_set::PeerSet;
		let _ = self.sim.expect(
			|e| matches!(
				e,
				Effect::DisconnectPeers { peers, peer_set: PeerSet::Collation }
					if peers.contains(&peer.peer_id),
			),
			HAPPY_PATH_TIMEOUT,
			"Effect::DisconnectPeers for peer on the Collation peer-set",
		);
	}

	/// Assert that **no** `Effect::DisconnectPeers` carrying `peer` fires within `within`.
	pub fn expect_no_disconnect(&mut self, peer: &Peer, within: Duration) {
		use polkadot_node_network_protocol::peer_set::PeerSet;
		self.sim.expect_no(
			|e| matches!(
				e,
				Effect::DisconnectPeers { peers, peer_set: PeerSet::Collation }
					if peers.contains(&peer.peer_id),
			),
			within,
			"Effect::DisconnectPeers for peer (must NOT fire)",
		);
	}

}
