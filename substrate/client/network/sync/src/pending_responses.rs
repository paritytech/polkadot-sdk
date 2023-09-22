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
//! polling them, The [`Stream`] implemented by [`PendingResponses`] never terminates.

use futures::{channel::oneshot, future::BoxFuture, stream::Stream, FutureExt};
use libp2p::PeerId;
use log::error;
use sc_network::request_responses::RequestFailure;
use sc_network_common::sync::PeerRequest;
use sp_runtime::traits::Block as BlockT;
use std::{
	collections::{HashMap, HashSet, VecDeque},
	task::{Context, Poll, Waker},
};

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

/// A future implementing a pending response.
type ResponseEventFuture<B> = BoxFuture<'static, ResponseEvent<B>>;

/// Stream taking care of polling pending responses.
pub(crate) struct PendingResponses<B: BlockT> {
	/// Pending responses
	pending_responses: HashMap<PeerId, ResponseEventFuture<B>>,
	/// Finished responses
	finished_responses: VecDeque<ResponseEvent<B>>,
	/// Finished responses peers
	finished_peer_ids: HashSet<PeerId>,
	/// Waker to ensure wake-up once a new pending response is added.
	waker: Option<Waker>,
}

impl<B: BlockT> PendingResponses<B> {
	pub fn new() -> Self {
		Self {
			pending_responses: HashMap::new(),
			finished_responses: VecDeque::new(),
			finished_peer_ids: HashSet::new(),
			waker: None,
		}
	}

	pub fn insert(
		&mut self,
		peer_id: PeerId,
		request: PeerRequest<B>,
		response_future: ResponseFuture,
	) {
		let request_type = request.get_type();

		if let Some(_) = self.pending_responses.insert(
			peer_id,
			async move { ResponseEvent { peer_id, request, response: response_future.await } }
				.boxed(),
		) {
			error!(
				target: LOG_TARGET,
				"Discarded pending response from peer {peer_id}, {request_type:?}.",
			);
			debug_assert!(false);
		}

		if self.finished_peer_ids.remove(&peer_id) {
			// This must not happen unless there is a logic error in the code,
			// so performance penalty is not an issue.
			self.finished_responses.retain(|event| event.peer_id != peer_id);

			error!(
				target: LOG_TARGET,
				"Discarded finished response from peer {peer_id}, {request_type:?}.",
			);
			debug_assert!(false);
		}

		if let Some(waker) = self.waker.take() {
			waker.wake();
		}
	}

	pub fn remove(&mut self, peer_id: &PeerId) -> bool {
		match self.pending_responses.remove(peer_id) {
			Some(_) => true,
			None => {
				// then we must look in finished responses
				if self.finished_peer_ids.remove(peer_id) {
					// We do a linear search here. With 10 ms roud-trip network latency and 100
					// active peers this leads to a penalty of up to 10000 `PeerId` comparisons per
					// second. Moreover, we remove an element from a `VecDeque`, what leads to a
					// penalty proportional to the poll iteration duration (and hence the number of
					// elements in a `VecDeque`).
					// TODO: probably optimize this.

					let mut remove_index = None;

					for (i, event) in self.finished_responses.iter().enumerate() {
						if &event.peer_id == peer_id {
							remove_index = Some(i);
							break
						}
					}

					if let Some(index) = remove_index {
						self.finished_responses.remove(index);
						true
					} else {
						error!(
							target: LOG_TARGET,
							"Logic error: {peer_id} is in `finished_peer_ids`, but not in `finished_responses`.",
						);
						debug_assert!(false);
						false
					}
				} else {
					false
				}
			},
		}
	}

	pub fn len(&self) -> usize {
		self.pending_responses.len() + self.finished_responses.len()
	}
}

impl<B: BlockT> Unpin for PendingResponses<B> {}

impl<B: BlockT> Stream for PendingResponses<B> {
	type Item = ResponseEvent<B>;

	fn poll_next(
		mut self: std::pin::Pin<&mut Self>,
		cx: &mut Context<'_>,
	) -> Poll<Option<Self::Item>> {
		let ready_responses = self
			.pending_responses
			.values_mut()
			.filter_map(|future| match future.poll_unpin(cx) {
				Poll::Pending => None,
				Poll::Ready(event) => Some(event),
			})
			.collect::<Vec<_>>();

		for ResponseEvent { peer_id, .. } in ready_responses.iter() {
			self.pending_responses
				.remove(&peer_id)
				.expect("Logic error: peer id from pending response is missing in the map.");
			self.finished_peer_ids.insert(*peer_id);
		}

		self.finished_responses.extend(ready_responses.into_iter());

		if let Some(event) = self.finished_responses.pop_front() {
			self.finished_peer_ids.remove(&event.peer_id);

			Poll::Ready(Some(event))
		} else {
			self.waker = Some(cx.waker().clone());

			Poll::Pending
		}
	}
}
