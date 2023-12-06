// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! [`PendingResponses`] is responsible for keeping track of pending responses and
//! polling them. [`Stream`] implemented by [`PendingResponses`] never terminates.

use crate::types::PeerRequest;
use futures::{
	channel::oneshot,
	future::BoxFuture,
	stream::{BoxStream, FusedStream, Stream},
	FutureExt, StreamExt,
};
use libp2p::PeerId;
use log::error;
use sc_network::request_responses::RequestFailure;
use sp_runtime::traits::Block as BlockT;
use std::task::{Context, Poll, Waker};
use tokio_stream::StreamMap;

/// Log target for this file.
const LOG_TARGET: &'static str = "sync";

/// Response result.
type ResponseResult = Result<Result<Vec<u8>, RequestFailure>, oneshot::Canceled>;

/// A future yielding [`ResponseResult`].
type ResponseFuture = BoxFuture<'static, ResponseResult>;

/// An event we receive once a pending response future resolves.
pub(crate) struct ResponseEvent<B: BlockT> {
	pub peer_id: PeerId,
	pub request: PeerRequest<B>,
	pub response: ResponseResult,
}

/// Stream taking care of polling pending responses.
pub(crate) struct PendingResponses<B: BlockT> {
	/// Pending responses
	pending_responses: StreamMap<PeerId, BoxStream<'static, (PeerRequest<B>, ResponseResult)>>,
	/// Waker to implement never terminating stream
	waker: Option<Waker>,
}

impl<B: BlockT> PendingResponses<B> {
	pub fn new() -> Self {
		Self { pending_responses: StreamMap::new(), waker: None }
	}

	pub fn insert(
		&mut self,
		peer_id: PeerId,
		request: PeerRequest<B>,
		response_future: ResponseFuture,
	) {
		let request_type = request.get_type();

		if self
			.pending_responses
			.insert(
				peer_id,
				Box::pin(async move { (request, response_future.await) }.into_stream()),
			)
			.is_some()
		{
			error!(
				target: LOG_TARGET,
				"Discarded pending response from peer {peer_id}, request type: {request_type:?}.",
			);
			debug_assert!(false);
		}

		if let Some(waker) = self.waker.take() {
			waker.wake();
		}
	}

	pub fn remove(&mut self, peer_id: &PeerId) -> bool {
		self.pending_responses.remove(peer_id).is_some()
	}

	pub fn len(&self) -> usize {
		self.pending_responses.len()
	}
}

impl<B: BlockT> Stream for PendingResponses<B> {
	type Item = ResponseEvent<B>;

	fn poll_next(
		mut self: std::pin::Pin<&mut Self>,
		cx: &mut Context<'_>,
	) -> Poll<Option<Self::Item>> {
		match self.pending_responses.poll_next_unpin(cx) {
			Poll::Ready(Some((peer_id, (request, response)))) => {
				// We need to manually remove the stream, because `StreamMap` doesn't know yet that
				// it's going to yield `None`, so may not remove it before the next request is made
				// to the same peer.
				self.pending_responses.remove(&peer_id);

				Poll::Ready(Some(ResponseEvent { peer_id, request, response }))
			},
			Poll::Ready(None) | Poll::Pending => {
				self.waker = Some(cx.waker().clone());

				Poll::Pending
			},
		}
	}
}

// As [`PendingResponses`] never terminates, we can easily implement [`FusedStream`] for it.
impl<B: BlockT> FusedStream for PendingResponses<B> {
	fn is_terminated(&self) -> bool {
		false
	}
}
