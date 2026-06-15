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

use crate::{
	validator_side_experimental::common::{
		Advertisement, CollationFetchError, CollationFetchResponse, FetchKey, FetchTarget,
	},
	LOG_TARGET,
};
use futures::{
	future::BoxFuture,
	stream::{FusedStream, FuturesUnordered},
	task::Poll,
	FutureExt,
};
use polkadot_node_network_protocol::request_response::{
	outgoing::Recipient, v1 as request_v1, v2 as request_v2, OutgoingRequest, OutgoingResult,
	Requests,
};
use polkadot_node_subsystem_util::metrics::prometheus::prometheus::HistogramTimer;
use polkadot_primitives::Hash;
use std::{collections::HashMap, future::Future, pin::Pin};
use tokio_util::sync::CancellationToken;

pub struct InFlightInfo {
	token: CancellationToken,
	target: FetchTarget,
}

#[derive(Default)]
pub struct PendingRequests {
	futures: FuturesUnordered<CollationFetchRequest>,
	in_flight: HashMap<FetchKey, InFlightInfo>,
	by_output_head: HashMap<Hash, FetchKey>,
}

impl PendingRequests {
	pub fn contains_key(&self, key: &FetchKey) -> bool {
		self.in_flight.contains_key(key)
	}

	pub fn contains_output_head(&self, output_head: &Hash) -> bool {
		self.by_output_head.contains_key(output_head)
	}

	pub fn contains_advertisement(&self, advertisement: &Advertisement) -> bool {
		self.in_flight
			.get(&advertisement.fetch_key())
			.is_some_and(|info| info.target == FetchTarget::from_advertisement(advertisement))
	}

	pub fn launch(
		&mut self,
		target: &FetchTarget,
		advertisement_lifetime_timer: Option<HistogramTimer>,
	) -> Requests {
		let cancellation_token = CancellationToken::new();

		let (req, response_recv) = match target.candidate_hash {
			None => {
				let (req, response_recv) = OutgoingRequest::new(
					Recipient::Peer(target.peer_id),
					request_v1::CollationFetchingRequest {
						scheduling_parent: target.scheduling_parent,
						para_id: target.para_id,
					},
				);
				let requests = Requests::CollationFetchingV1(req);
				(requests, response_recv.boxed())
			},
			Some(candidate_hash) => {
				let (req, response_recv) = OutgoingRequest::new(
					Recipient::Peer(target.peer_id),
					request_v2::CollationFetchingRequest {
						scheduling_parent: target.scheduling_parent,
						para_id: target.para_id,
						candidate_hash,
					},
				);
				let requests = Requests::CollationFetchingV2(req);
				(requests, response_recv.boxed())
			},
		};

		self.in_flight.insert(
			target.fetch_key(),
			InFlightInfo { token: cancellation_token.clone(), target: *target },
		);
		if let Some(output_head_data_hash) = target.output_head_data_hash {
			self.by_output_head.insert(output_head_data_hash, target.fetch_key());
		}
		self.futures.push(CollationFetchRequest {
			target: *target,
			from_collator: response_recv,
			cancellation_future: cancellation_token.cancelled_owned().boxed(),
			_lifetime_timer: advertisement_lifetime_timer,
		});

		req
	}

	/// Iterator over advertisements currently being fetched.
	pub fn iter(&self) -> impl Iterator<Item = &FetchTarget> {
		self.in_flight.values().map(|in_flight| &in_flight.target)
	}

	pub fn cancel(&mut self, fetch_key: &FetchKey) {
		if let Some(in_flight_info) = self.in_flight.remove(fetch_key) {
			gum::trace!(target: LOG_TARGET, ?fetch_key, "Cancelling collation fetch request");
			in_flight_info.token.cancel();
			if let Some(output_head_data_hash) = in_flight_info.target.output_head_data_hash {
				self.by_output_head.remove(&output_head_data_hash);
			}
		}
	}

	/// Cancel every in-flight fetch rooted at this scheduling parent.
	///
	/// Used on view changes when a scheduling parent goes out of view. Keyed off
	/// `in_flight` rather than stored advertisements, because segment-launched
	/// fetches are removed from storage at launch and have no stored advertisement
	/// to be discovered through.
	pub fn cancel_for_scheduling_parent(&mut self, scheduling_parent: &Hash) {
		let keys: Vec<FetchKey> = self
			.in_flight
			.iter()
			.filter(|(_, info)| &info.target.scheduling_parent == scheduling_parent)
			.map(|(key, _)| *key)
			.collect();
		for key in keys {
			self.cancel(&key);
		}
	}

	pub fn note_completed(&mut self, target: &FetchTarget) {
		let key = target.fetch_key();
		if self.in_flight.get(&key).is_some_and(|info| &info.target == target) {
			self.in_flight.remove(&key);
			if let Some(output_head) = target.output_head_data_hash {
				self.by_output_head.remove(&output_head);
			}
		}
	}

	pub fn response_stream(&mut self) -> &mut impl FusedStream<Item = CollationFetchResponse> {
		&mut self.futures
	}
}

/// Future that concludes when the collator has responded to our collation fetch request
/// or the request was cancelled by the validator.
struct CollationFetchRequest {
	/// Info about the requested collation.
	target: FetchTarget,
	/// Responses from collator. We can directly use v2 response because the payloads are identical
	/// for v1 and v2.
	from_collator: BoxFuture<'static, OutgoingResult<request_v2::CollationFetchingResponse>>,
	/// Handle used for checking if this request was cancelled.
	cancellation_future: BoxFuture<'static, ()>,
	/// A metric histogram for the lifetime of the request
	_lifetime_timer: Option<HistogramTimer>,
}

impl Future for CollationFetchRequest {
	type Output = CollationFetchResponse;

	fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
		// First check if this fetch request was cancelled.
		let cancelled = self.cancellation_future.poll_unpin(cx).is_ready();
		if cancelled {
			return Poll::Ready((self.target, Err(CollationFetchError::Cancelled)));
		}

		let res = self
			.from_collator
			.poll_unpin(cx)
			.map(|res| (self.target, res.map_err(CollationFetchError::Request)));

		res
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::validator_side_experimental::common::ProspectiveCandidate;
	use futures::StreamExt;
	use polkadot_node_network_protocol::PeerId;
	use polkadot_primitives::{CandidateHash, Id as ParaId};

	fn target(
		scheduling_parent: Hash,
		peer_id: PeerId,
		candidate_hash: Option<CandidateHash>,
	) -> FetchTarget {
		FetchTarget {
			peer_id,
			para_id: ParaId::from(100),
			scheduling_parent,
			candidate_hash,
			parent_head_data_hash: candidate_hash.map(|_| Hash::repeat_byte(9)),
			descriptor_version: None,
			output_head_data_hash: None,
			relay_parent: None,
		}
	}

	/// Drain every already-concluded response without blocking on pending ones.
	fn drain_ready(pending: &mut PendingRequests) -> Vec<CollationFetchResponse> {
		let mut out = Vec::new();
		while let Some(Some(res)) = pending.response_stream().next().now_or_never() {
			out.push(res);
		}
		out
	}

	#[test]
	fn launch_wire_format_matches_target_shape() {
		let mut pending = PendingRequests::default();
		let sp = Hash::repeat_byte(1);
		let peer = PeerId::random();

		// No candidate hash → V1 wire request, keyed as V1(sp, para).
		match pending.launch(&target(sp, peer, None), None) {
			Requests::CollationFetchingV1(req) => {
				assert!(matches!(req.peer, Recipient::Peer(p) if p == peer));
				assert_eq!(req.payload.scheduling_parent, sp);
				assert_eq!(req.payload.para_id, ParaId::from(100));
			},
			other => panic!("expected V1 request, got {:?}", other),
		}
		assert!(pending.contains_key(&FetchKey::V1(sp, ParaId::from(100))));

		// Candidate hash → V2 wire request, keyed by candidate.
		let hash = CandidateHash(Hash::repeat_byte(2));
		match pending.launch(&target(sp, peer, Some(hash)), None) {
			Requests::CollationFetchingV2(req) => {
				assert!(matches!(req.peer, Recipient::Peer(p) if p == peer));
				assert_eq!(req.payload.candidate_hash, hash);
			},
			other => panic!("expected V2 request, got {:?}", other),
		}
		assert!(pending.contains_key(&FetchKey::Candidate(hash)));
	}

	#[test]
	fn cancel_for_scheduling_parent_is_sp_selective() {
		let mut pending = PendingRequests::default();
		let dropped = target(
			Hash::repeat_byte(1),
			PeerId::random(),
			Some(CandidateHash(Hash::repeat_byte(3))),
		);
		let kept = target(
			Hash::repeat_byte(2),
			PeerId::random(),
			Some(CandidateHash(Hash::repeat_byte(4))),
		);
		let _dropped_req = pending.launch(&dropped, None);
		let _kept_req = pending.launch(&kept, None);

		pending.cancel_for_scheduling_parent(&dropped.scheduling_parent);

		assert!(!pending.contains_key(&dropped.fetch_key()));
		assert!(pending.contains_key(&kept.fetch_key()));

		// Exactly one future concluded: the dropped SP's, as a Cancelled tombstone.
		let concluded = drain_ready(&mut pending);
		assert_eq!(concluded.len(), 1);
		assert_eq!(concluded[0].0, dropped);
		assert!(matches!(concluded[0].1, Err(CollationFetchError::Cancelled)));
	}

	#[test]
	fn note_completed_only_retires_its_own_launch() {
		let mut pending = PendingRequests::default();
		let launched = target(
			Hash::repeat_byte(1),
			PeerId::random(),
			Some(CandidateHash(Hash::repeat_byte(3))),
		);
		let _ = pending.launch(&launched, None);

		// Same fetch key, different scheduling parent: a stale tombstone draining
		// after a same-candidate relaunch, or an injected response. Must not remove
		// the live entry.
		let imposter = FetchTarget { scheduling_parent: Hash::repeat_byte(2), ..launched };
		pending.note_completed(&imposter);
		assert!(pending.contains_key(&launched.fetch_key()));

		pending.note_completed(&launched);
		assert!(!pending.contains_key(&launched.fetch_key()));
	}

	#[test]
	fn output_head_index_follows_entry_lifecycle() {
		let mut pending = PendingRequests::default();
		let output_head = Hash::repeat_byte(7);
		let mut t = target(
			Hash::repeat_byte(1),
			PeerId::random(),
			Some(CandidateHash(Hash::repeat_byte(3))),
		);
		t.output_head_data_hash = Some(output_head);

		let _ = pending.launch(&t, None);
		assert!(pending.contains_output_head(&output_head));
		pending.note_completed(&t);
		assert!(!pending.contains_output_head(&output_head));

		// Cancellation cleans the index too.
		let _ = pending.launch(&t, None);
		pending.cancel(&t.fetch_key());
		assert!(!pending.contains_output_head(&output_head));
	}

	#[test]
	fn contains_advertisement_is_peer_exact() {
		let mut pending = PendingRequests::default();
		let prospective_candidate = Some(ProspectiveCandidate {
			candidate_hash: CandidateHash(Hash::repeat_byte(3)),
			parent_head_data_hash: Hash::repeat_byte(9),
		});
		let ad = |peer_id| Advertisement {
			scheduling_parent: Hash::repeat_byte(1),
			para_id: ParaId::from(100),
			peer_id,
			prospective_candidate,
			advertised_descriptor_version: None,
		};
		let (peer_a, peer_b) = (PeerId::random(), PeerId::random());

		let _ = pending.launch(&FetchTarget::from_advertisement(&ad(peer_a)), None);

		// The exact in-flight advertisement is a duplicate at acceptance…
		assert!(pending.contains_advertisement(&ad(peer_a)));
		// …but the same candidate from another peer is NOT — it must be parked as
		// the failover pool. Key-level dedup is selection-time only.
		assert!(!pending.contains_advertisement(&ad(peer_b)));
		assert!(pending.contains_key(&ad(peer_b).fetch_key()));
	}
}
