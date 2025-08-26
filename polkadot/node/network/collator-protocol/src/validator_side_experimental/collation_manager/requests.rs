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

use crate::validator_side_experimental::common::{
	Advertisement, CollationFetchError, CollationFetchResponse, ProspectiveCandidate,
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
use std::{collections::HashMap, future::Future, pin::Pin};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub struct PendingRequests {
	futures: FuturesUnordered<CollationFetchRequest>,
	cancellation_tokens: HashMap<Advertisement, CancellationToken>,
}

impl PendingRequests {
	pub fn contains(&self, advertisement: &Advertisement) -> bool {
		self.cancellation_tokens.contains_key(advertisement)
	}

	pub fn launch(&mut self, advertisement: &Advertisement) -> Requests {
		let cancellation_token = CancellationToken::new();

		let (req, response_recv) = match advertisement.prospective_candidate {
			None => {
				let (req, response_recv) = OutgoingRequest::new(
					Recipient::Peer(advertisement.peer_id),
					request_v1::CollationFetchingRequest {
						relay_parent: advertisement.relay_parent,
						para_id: advertisement.para_id,
					},
				);
				let requests = Requests::CollationFetchingV1(req);
				(requests, response_recv.boxed())
			},
			Some(ProspectiveCandidate { candidate_hash, .. }) => {
				let (req, response_recv) = OutgoingRequest::new(
					Recipient::Peer(advertisement.peer_id),
					request_v2::CollationFetchingRequest {
						relay_parent: advertisement.relay_parent,
						para_id: advertisement.para_id,
						candidate_hash,
					},
				);
				let requests = Requests::CollationFetchingV2(req);
				(requests, response_recv.boxed())
			},
		};

		let cancellation_token_copy = cancellation_token.clone();
		self.futures.push(CollationFetchRequest {
			advertisement: *advertisement,
			from_collator: response_recv,
			cancellation_future: cancellation_token.cancelled_owned().boxed(),
		});

		self.cancellation_tokens.insert(*advertisement, cancellation_token_copy);

		req
	}

	pub fn cancel(&mut self, advertisement: &Advertisement) {
		if let Some(cancellation_token) = self.cancellation_tokens.remove(advertisement) {
			cancellation_token.cancel();
		}
	}

	pub fn completed(&mut self, advertisement: &Advertisement) {
		self.cancellation_tokens.remove(advertisement);
	}

	pub fn response_stream(&mut self) -> &mut impl FusedStream<Item = CollationFetchResponse> {
		&mut self.futures
	}
}

/// Future that concludes when the collator has responded to our collation fetch request
/// or the request was cancelled by the validator.
struct CollationFetchRequest {
	/// Info about the requested collation.
	advertisement: Advertisement,
	/// Responses from collator. We can directly use v2 response because the payloads are identical
	/// for v1 and v2.
	from_collator: BoxFuture<'static, OutgoingResult<request_v2::CollationFetchingResponse>>,
	/// Handle used for checking if this request was cancelled.
	cancellation_future: BoxFuture<'static, ()>,
}

impl Future for CollationFetchRequest {
	type Output = CollationFetchResponse;

	fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
		// First check if this fetch request was cancelled.
		let cancelled = self.cancellation_future.poll_unpin(cx).is_ready();

		if cancelled {
			return Poll::Ready((self.advertisement, Err(CollationFetchError::Cancelled)))
		}

		let res = self
			.from_collator
			.poll_unpin(cx)
			.map(|res| (self.advertisement, res.map_err(CollationFetchError::Request)));

		res
	}
}
