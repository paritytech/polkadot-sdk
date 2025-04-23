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

use crate::validator_side::common::{Advertisement, CollationFetchError, CollationFetchResponse};
use futures::{
	future::BoxFuture,
	stream::{FusedStream, FuturesUnordered},
	task::Poll,
	FutureExt,
};
use polkadot_node_network_protocol::request_response::{
	outgoing::Recipient, v2 as request_v2, OutgoingRequest, OutgoingResult,
};
use polkadot_primitives::CandidateHash;
use std::{collections::HashMap, future::Future, pin::Pin};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub struct PendingRequests {
	futures: FuturesUnordered<CollationFetchRequest>,
	pub cancellation_tokens: HashMap<CandidateHash, CancellationToken>,
}

impl PendingRequests {
	pub fn contains(&self, candidate_hash: &CandidateHash) -> bool {
		self.cancellation_tokens.contains_key(candidate_hash)
	}

	pub fn launch(
		&mut self,
		advertisement: &Advertisement,
	) -> OutgoingRequest<request_v2::CollationFetchingRequest> {
		let cancellation_token = CancellationToken::new();
		let (req, response_recv) = OutgoingRequest::new(
			Recipient::Peer(advertisement.peer_id),
			request_v2::CollationFetchingRequest {
				relay_parent: advertisement.relay_parent,
				para_id: advertisement.para_id,
				candidate_hash: advertisement.prospective_candidate.candidate_hash,
			},
		);

		self.futures.push(CollationFetchRequest {
			advertisement: *advertisement,
			from_collator: response_recv.boxed(),
			cancellation_token: cancellation_token.clone(),
		});

		self.cancellation_tokens
			.insert(advertisement.prospective_candidate.candidate_hash, cancellation_token);

		req
	}

	pub fn cancel(&mut self, candidate_hash: &CandidateHash) {
		if let Some(cancellation_token) = self.cancellation_tokens.remove(candidate_hash) {
			cancellation_token.cancel();
		}
	}

	pub fn completed(&mut self, candidate_hash: &CandidateHash) {
		self.cancellation_tokens.remove(candidate_hash);
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
	/// Responses from collator.
	from_collator: BoxFuture<'static, OutgoingResult<request_v2::CollationFetchingResponse>>,
	/// Handle used for checking if this request was cancelled.
	cancellation_token: CancellationToken,
}
// TODO: we could augment this with a duration witness, so that once the request finishes, we could
// punish only collators that waste more than X amount of our time.

impl Future for CollationFetchRequest {
	type Output = CollationFetchResponse;

	fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
		// First check if this fetch request was cancelled.
		let cancelled = match std::pin::pin!(self.cancellation_token.cancelled()).poll(cx) {
			Poll::Ready(()) => true,
			Poll::Pending => false,
		};

		if cancelled {
			return Poll::Ready((self.advertisement.clone(), Err(CollationFetchError::Cancelled)))
		}

		let res = self
			.from_collator
			.poll_unpin(cx)
			.map(|res| (self.advertisement.clone(), res.map_err(CollationFetchError::Request)));

		res
	}
}
