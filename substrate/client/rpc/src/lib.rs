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

//! Substrate RPC implementation.
//!
//! A core implementation of Substrate RPC interfaces.

#![warn(missing_docs)]

pub use jsonrpsee::core::{
	id_providers::{
		RandomIntegerIdProvider as RandomIntegerSubscriptionId,
		RandomStringIdProvider as RandomStringSubscriptionId,
	},
	traits::IdProvider as RpcSubscriptionIdProvider,
};
pub use sc_rpc_api::DenyUnsafe;

pub mod author;
pub mod chain;
pub mod dev;
pub mod mixnet;
pub mod offchain;
pub mod state;
pub mod statement;
pub mod system;

#[cfg(any(test, feature = "test-helpers"))]
pub mod testing;

/// Task executor that is being used by RPC subscriptions.
pub type SubscriptionTaskExecutor = std::sync::Arc<dyn sp_core::traits::SpawnNamed>;

/// JSON-RPC helpers.
pub mod utils {
	use crate::SubscriptionTaskExecutor;
	use futures::{
		future::{self, Either, Fuse, FusedFuture},
		Future, FutureExt, Stream, StreamExt,
	};
	use jsonrpsee::{PendingSubscriptionSink, SubscriptionMessage, SubscriptionSink};
	use sp_runtime::Serialize;
	use std::collections::VecDeque;

	/// A simple bounded VecDeque.
	struct BoundedVecDeque<T> {
		inner: VecDeque<T>,
		max_cap: usize,
	}

	impl<T> BoundedVecDeque<T> {
		/// Create a new bounded VecDeque.
		fn new(max_cap: usize) -> Self {
			Self { inner: VecDeque::with_capacity(max_cap), max_cap }
		}

		fn push_back(&mut self, item: T) -> Result<(), ()> {
			if self.inner.len() >= self.max_cap {
				Err(())
			} else {
				self.inner.push_back(item);
				Ok(())
			}
		}

		fn pop_front(&mut self) -> Option<T> {
			self.inner.pop_front()
		}
	}

	/// Feed items to the subscription from the underlying stream.
	///
	/// It's possible configure how many items from the stream
	/// are allowed to be kept in memory until the subscription is dropped.
	///
	/// This is needed because the underlying streams in substrate are
	/// unbounded.
	pub async fn pipe_from_stream<S, T>(
		pending: PendingSubscriptionSink,
		mut stream: S,
		max_cap: usize,
	) where
		S: Stream<Item = T> + Unpin + Send + 'static,
		T: Serialize + Send + 'static,
	{
		let mut buf = BoundedVecDeque::new(max_cap);
		let accept_fut = pending.accept();

		futures::pin_mut!(accept_fut);

		// Poll the stream while waiting for the subscription to be accepted
		//
		// If the `max_cap` is exceeded then the subscription is dropped.
		let sink = loop {
			match future::select(accept_fut, stream.next()).await {
				Either::Left((Ok(sink), _)) => break sink,
				Either::Right((Some(msg), f)) => {
					if buf.push_back(msg).is_err() {
						log::warn!(target: "rpc", "Subscription::accept failed buffer limit={} exceed; dropping subscription", buf.max_cap);
						return
					}
					accept_fut = f;
				},
				// The connection was closed or the stream was closed.
				_ => return,
			}
		};

		inner_pipe_from_stream(sink, stream, buf).await
	}

	async fn inner_pipe_from_stream<S, T>(
		sink: SubscriptionSink,
		mut stream: S,
		mut buf: BoundedVecDeque<T>,
	) where
		S: Stream<Item = T> + Unpin + Send + 'static,
		T: Serialize + Send + 'static,
	{
		let mut next_fut = Box::pin(Fuse::terminated());

		let mut next_item = stream.next();
		let closed = sink.closed();

		futures::pin_mut!(closed);

		loop {
			if next_fut.is_terminated() {
				if let Some(v) = buf.pop_front() {
					let val = to_sub_message(&sink, &v);
					next_fut.set(async { sink.send(val).await }.fuse());
				}
			}

			match future::select(closed, future::select(next_fut, next_item)).await {
				// Send operation finished.
				Either::Right((Either::Left((_, n)), c)) => {
					next_item = n;
					closed = c;
					next_fut = Box::pin(Fuse::terminated());
				},
				// New item from the stream
				Either::Right((Either::Right((Some(v), n)), c)) => {
					if buf.push_back(v).is_err() {
						log::warn!(target: "rpc", "Subscription buffer limit={} exceed; dropping subscription", buf.max_cap);
						return
					}

					next_fut = n;
					closed = c;
					next_item = stream.next();
				},
				// The stream or subscription was closed.
				_ => break,
			}
		}
	}

	/// Builds a subscription message.
	///
	/// # Panics
	///
	/// This function panics `Serialize` fails and is treated a bug.
	pub fn to_sub_message(sink: &SubscriptionSink, result: &impl Serialize) -> SubscriptionMessage {
		SubscriptionMessage::new(sink.method_name(), sink.subscription_id(), result)
			.expect("Serialize infallible; qed")
	}

	/// Helper for spawning non-blocking rpc subscription task.
	pub fn spawn_subscription_task(
		executor: &SubscriptionTaskExecutor,
		fut: impl Future<Output = ()> + Send + 'static,
	) {
		executor.spawn("substrate-rpc-subscription", Some("rpc"), fut.boxed());
	}
}
