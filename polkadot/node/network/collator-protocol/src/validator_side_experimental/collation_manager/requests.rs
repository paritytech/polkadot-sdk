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
		Advertisement, CollationFetchError, CollationFetchResponse, PeerAdvertisement,
		ProspectiveCandidate,
	},
	LOG_TARGET,
};
use futures::{
	future::BoxFuture,
	stream::{FusedStream, FuturesUnordered},
	task::Poll,
	FutureExt,
};
use polkadot_node_network_protocol::{
	request_response::{
		outgoing::Recipient, v1 as request_v1, v2 as request_v2, OutgoingRequest, OutgoingResult,
		Requests,
	},
	PeerId,
};
use polkadot_node_subsystem_util::metrics::prometheus::prometheus::HistogramTimer;
use std::{collections::HashMap, future::Future, pin::Pin};
use tokio_util::sync::CancellationToken;

/// Tracks the in-flight collation fetches the validator has launched.
///
/// Keyed by [`Advertisement`], with the peer we are currently fetching from stored alongside
/// as the value.
///
/// When parallel fetches land ([issue #11023](https://github.com/paritytech/polkadot-sdk/issues/11023)),
/// the value widens to `Vec<(PeerId, CancellationToken)>`. `contains()` still answers
/// "any in-flight fetch for this `Advertisement`?", `iter()` still yields per-peer
/// `PeerAdvertisement`s, so callers don't change.
#[derive(Default)]
pub struct PendingRequests {
	futures: FuturesUnordered<CollationFetchRequest>,
	cancellation_tokens: HashMap<Advertisement, (PeerId, CancellationToken)>,
}

impl PendingRequests {
	/// Whether a fetch is already in flight for this `Advertisement` (from any peer).
	pub fn contains(&self, advertisement: &Advertisement) -> bool {
		self.cancellation_tokens.contains_key(advertisement)
	}

	pub fn launch(
		&mut self,
		peer_adv: &PeerAdvertisement,
		advertisement_lifetime_timer: Option<HistogramTimer>,
	) -> Requests {
		let cancellation_token = CancellationToken::new();

		let (req, response_recv) = match peer_adv.advertisement.prospective_candidate {
			None => {
				let (req, response_recv) = OutgoingRequest::new(
					Recipient::Peer(peer_adv.peer_id),
					request_v1::CollationFetchingRequest {
						scheduling_parent: peer_adv.advertisement.scheduling_parent,
						para_id: peer_adv.advertisement.para_id,
					},
				);
				let requests = Requests::CollationFetchingV1(req);
				(requests, response_recv.boxed())
			},
			Some(ProspectiveCandidate { candidate_hash, .. }) => {
				let (req, response_recv) = OutgoingRequest::new(
					Recipient::Peer(peer_adv.peer_id),
					request_v2::CollationFetchingRequest {
						scheduling_parent: peer_adv.advertisement.scheduling_parent,
						para_id: peer_adv.advertisement.para_id,
						candidate_hash,
					},
				);
				let requests = Requests::CollationFetchingV2(req);
				(requests, response_recv.boxed())
			},
		};

		self.cancellation_tokens
			.insert(peer_adv.advertisement, (peer_adv.peer_id, cancellation_token.clone()));
		self.futures.push(CollationFetchRequest {
			peer_adv: *peer_adv,
			from_collator: response_recv,
			cancellation_future: cancellation_token.cancelled_owned().boxed(),
			_lifetime_timer: advertisement_lifetime_timer,
		});

		req
	}

	/// Iterator over the in-flight (`PeerAdvertisement`s)
	pub fn iter(&self) -> impl Iterator<Item = PeerAdvertisement> + '_ {
		self.cancellation_tokens
			.iter()
			.map(|(advertisement, (peer_id, _))| PeerAdvertisement {
				advertisement: *advertisement,
				peer_id: *peer_id,
			})
	}

	/// Cancel the in-flight fetch for this `Advertisement`, if any.
	pub fn cancel(&mut self, advertisement: &Advertisement) {
		if let Some((peer_id, cancellation_token)) = self.cancellation_tokens.remove(advertisement)
		{
			gum::trace!(
				target: LOG_TARGET,
				?advertisement,
				?peer_id,
				"Cancelling collation fetch request",
			);
			cancellation_token.cancel();
		}
	}

	pub fn note_completed(&mut self, advertisement: &Advertisement) {
		self.cancellation_tokens.remove(advertisement);
	}

	pub fn response_stream(&mut self) -> &mut impl FusedStream<Item = CollationFetchResponse> {
		&mut self.futures
	}
}

/// Future that concludes when the collator has responded to our collation fetch request
/// or the request was cancelled by the validator.
struct CollationFetchRequest {
	/// Info about the requested collation (`Advertisement` + peer we're fetching from).
	peer_adv: PeerAdvertisement,
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
			return Poll::Ready((self.peer_adv, Err(CollationFetchError::Cancelled)));
		}

		let res = self
			.from_collator
			.poll_unpin(cx)
			.map(|res| (self.peer_adv, res.map_err(CollationFetchError::Request)));

		res
	}
}
