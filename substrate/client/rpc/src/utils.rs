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

//! JSON-RPC helpers.

use crate::SubscriptionTaskExecutor;
use futures::{
	future::{self, Either, Fuse, FusedFuture},
	Future, FutureExt, Stream, StreamExt, TryStream, TryStreamExt,
};
use jsonrpsee::{
	types::SubscriptionId, DisconnectError, PendingSubscriptionSink, SubscriptionMessage,
	SubscriptionSink,
};
use sp_runtime::Serialize;
use std::collections::VecDeque;

const DEFAULT_BUF_SIZE: usize = 16;

/// A trait representing a buffer which may or may not support
/// to replace items when the buffer is full.
pub trait Buffer {
	/// The item type that the buffer holds.
	type Item;

	/// Push an item to the buffer.
	///
	/// Returns `Err` if the buffer doesn't support replacing older items
	fn push(&mut self, item: Self::Item) -> Result<(), ()>;
	/// Pop the next item from the buffer.
	fn pop(&mut self) -> Option<Self::Item>;
}

/// A simple bounded buffer that will terminate the subscription if the buffer becomes full.
pub struct BoundedVecDeque<T> {
	inner: VecDeque<T>,
	max_cap: usize,
}

impl<T> Default for BoundedVecDeque<T> {
	fn default() -> Self {
		Self { inner: VecDeque::with_capacity(DEFAULT_BUF_SIZE), max_cap: DEFAULT_BUF_SIZE }
	}
}

impl<T> BoundedVecDeque<T> {
	/// Create a new bounded VecDeque.
	pub fn new(cap: usize) -> Self {
		Self { inner: VecDeque::with_capacity(cap), max_cap: cap }
	}
}

impl<T> Buffer for BoundedVecDeque<T> {
	type Item = T;

	fn push(&mut self, item: Self::Item) -> Result<(), ()> {
		if self.inner.len() >= self.max_cap {
			Err(())
		} else {
			self.inner.push_back(item);
			Ok(())
		}
	}

	fn pop(&mut self) -> Option<T> {
		self.inner.pop_front()
	}
}

/// Fixed size ring buffer that replaces the oldest item when full.
#[derive(Debug)]
pub struct RingBuffer<T> {
	inner: VecDeque<T>,
	cap: usize,
}

impl<T> RingBuffer<T> {
	/// Create a new ring buffer.
	pub fn new(cap: usize) -> Self {
		Self { inner: VecDeque::with_capacity(cap), cap }
	}
}

impl<T> Buffer for RingBuffer<T> {
	type Item = T;

	fn push(&mut self, item: T) -> Result<(), ()> {
		if self.inner.len() >= self.cap {
			self.inner.pop_front();
		}

		self.inner.push_back(item);

		Ok(())
	}

	fn pop(&mut self) -> Option<T> {
		self.inner.pop_front()
	}
}

/// A pending subscription.
pub struct PendingSubscription(PendingSubscriptionSink);

impl From<PendingSubscriptionSink> for PendingSubscription {
	fn from(p: PendingSubscriptionSink) -> Self {
		Self(p)
	}
}

impl PendingSubscription {
	/// Feed items to the subscription from the underlying stream
	/// with specified buffer strategy.
	pub async fn pipe_from_stream<S, T, B>(self, mut stream: S, mut buf: B)
	where
		S: Stream<Item = T> + Unpin + Send + 'static,
		T: Serialize + Send + 'static,
		B: Buffer<Item = T>,
	{
		let method = self.0.method_name().to_string();
		let conn_id = self.0.connection_id().0;
		let accept_fut = self.0.accept();

		futures::pin_mut!(accept_fut);

		// Poll the stream while waiting for the subscription to be accepted
		//
		// If the `max_cap` is exceeded then the subscription is dropped.
		let sink = loop {
			match future::select(accept_fut, stream.next()).await {
				Either::Left((Ok(sink), _)) => break sink,
				Either::Right((Some(msg), f)) => {
					if buf.push(msg).is_err() {
						log::debug!(target: "rpc", "Subscription::accept buffer full for subscription={method} conn_id={conn_id}; dropping subscription");
						return
					}
					accept_fut = f;
				},
				// The connection was closed or the stream was closed.
				_ => return,
			}
		};

		Subscription(sink).pipe_from_stream(stream, buf).await
	}
}

/// An active subscription.
#[derive(Clone, Debug)]
pub struct Subscription(SubscriptionSink);

impl From<SubscriptionSink> for Subscription {
	fn from(sink: SubscriptionSink) -> Self {
		Self(sink)
	}
}

impl Subscription {
	/// Feed items to the subscription from the underlying stream
	/// with specified buffer strategy.
	pub async fn pipe_from_stream<S, T, B>(&self, stream: S, buf: B)
	where
		S: Stream<Item = T> + Unpin,
		T: Serialize + Send,
		B: Buffer<Item = T>,
	{
		self.pipe_from_try_stream(stream.map(Ok::<T, ()>), buf)
			.await
			.expect("No Err will be ever encountered.qed");
	}

	/// Feed items to the subscription from the underlying stream
	/// with specified buffer strategy.
	pub async fn pipe_from_try_stream<S, T, B, E>(&self, mut stream: S, mut buf: B) -> Result<(), E>
	where
		S: TryStream<Ok = T, Error = E> + Unpin,
		T: Serialize + Send,
		B: Buffer<Item = T>,
	{
		let mut next_fut = Box::pin(Fuse::terminated());
		let mut next_item = stream.try_next();
		let closed = self.0.closed();

		futures::pin_mut!(closed);

		loop {
			if next_fut.is_terminated() {
				if let Some(v) = buf.pop() {
					let val = self.to_sub_message(&v);
					next_fut.set(async { self.0.send(val).await }.fuse());
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
				Either::Right((Either::Right((Ok(Some(v)), n)), c)) => {
					if buf.push(v).is_err() {
						log::debug!(
							target: "rpc",
							"Subscription buffer full for subscription={} conn_id={}; dropping subscription",
							self.0.method_name(),
							self.0.connection_id().0
						);
						return Ok(());
					}

					next_fut = n;
					closed = c;
					next_item = stream.try_next();
				},
				// Error occured while processing the stream.
				//
				// terminate the stream.
				Either::Right((Either::Right((Err(e), _)), _)) => return Err(e),
				// Stream "finished".
				//
				// Process remaining items and terminate.
				Either::Right((Either::Right((Ok(None), pending_fut)), _)) => {
					if !pending_fut.is_terminated() && pending_fut.await.is_err() {
						return Ok(());
					}

					while let Some(v) = buf.pop() {
						if self.send(&v).await.is_err() {
							return Ok(());
						}
					}

					return Ok(());
				},
				// Subscription was closed.
				Either::Left(_) => return Ok(()),
			}
		}
	}

	/// Send a message on the subscription.
	pub async fn send(&self, result: &impl Serialize) -> Result<(), DisconnectError> {
		self.0.send(self.to_sub_message(result)).await
	}

	/// Get the subscription id.
	pub fn subscription_id(&self) -> SubscriptionId {
		self.0.subscription_id()
	}

	/// Completes when the subscription is closed.
	pub async fn closed(&self) {
		self.0.closed().await
	}

	/// Convert a result to a subscription message.
	fn to_sub_message(&self, result: &impl Serialize) -> SubscriptionMessage {
		SubscriptionMessage::new(self.0.method_name(), self.0.subscription_id(), result)
			.expect("Serialize infallible; qed")
	}
}

/// Helper for spawning non-blocking rpc subscription task.
pub fn spawn_subscription_task(
	executor: &SubscriptionTaskExecutor,
	fut: impl Future<Output = ()> + Send + 'static,
) {
	executor.spawn("substrate-rpc-subscription", Some("rpc"), fut.boxed());
}

#[cfg(test)]
mod tests {
	use super::*;
	use futures::StreamExt;
	use jsonrpsee::{core::EmptyServerParams, RpcModule, Subscription};

	async fn subscribe() -> Subscription {
		let mut module = RpcModule::new(());
		module
			.register_subscription("sub", "my_sub", "unsub", |_, pending, _, _| async move {
				let stream = futures::stream::iter([0; 16]);
				PendingSubscription::from(pending)
					.pipe_from_stream(stream, BoundedVecDeque::new(16))
					.await;
				Ok(())
			})
			.unwrap();

		module.subscribe("sub", EmptyServerParams::new(), 1).await.unwrap()
	}

	#[tokio::test]
	async fn pipe_from_stream_works() {
		let mut sub = subscribe().await;
		let mut rx = 0;

		while let Some(Ok(_)) = sub.next::<usize>().await {
			rx += 1;
		}

		assert_eq!(rx, 16);
	}

	#[tokio::test]
	async fn pipe_from_stream_with_bounded_vec() {
		let (tx, mut rx) = futures::channel::mpsc::unbounded::<()>();

		let mut module = RpcModule::new(tx);
		module
			.register_subscription("sub", "my_sub", "unsub", |_, pending, ctx, _| async move {
				let stream = futures::stream::iter([0; 32]);
				PendingSubscription::from(pending)
					.pipe_from_stream(stream, BoundedVecDeque::new(16))
					.await;
				_ = ctx.unbounded_send(());
				Ok(())
			})
			.unwrap();

		let mut sub = module.subscribe("sub", EmptyServerParams::new(), 1).await.unwrap();

		// When the 17th item arrives the subscription is dropped
		_ = rx.next().await.unwrap();
		assert!(sub.next::<usize>().await.is_none());
	}

	#[tokio::test]
	async fn subscription_is_dropped_when_stream_is_empty() {
		let notify_rx = std::sync::Arc::new(tokio::sync::Notify::new());
		let notify_tx = notify_rx.clone();

		let mut module = RpcModule::new(notify_tx);
		module
			.register_subscription(
				"sub",
				"my_sub",
				"unsub",
				|_, pending, notify_tx, _| async move {
					// emulate empty stream for simplicity: otherwise we need some mechanism
					// to sync buffer and channel send operations
					let stream = futures::stream::empty::<()>();
					// this should exit immediately
					PendingSubscription::from(pending)
						.pipe_from_stream(stream, BoundedVecDeque::default())
						.await;
					// notify that the `pipe_from_stream` has returned
					notify_tx.notify_one();
					Ok(())
				},
			)
			.unwrap();
		module.subscribe("sub", EmptyServerParams::new(), 1).await.unwrap();

		// it should fire once `pipe_from_stream` returns
		notify_rx.notified().await;
	}

	#[tokio::test]
	async fn subscription_replace_old_messages() {
		let mut module = RpcModule::new(());
		module
			.register_subscription("sub", "my_sub", "unsub", |_, pending, _, _| async move {
				// Send items 0..20 and ensure that only the last 3 are kept in the buffer.
				let stream = futures::stream::iter(0..20);
				PendingSubscription::from(pending)
					.pipe_from_stream(stream, RingBuffer::new(3))
					.await;
				Ok(())
			})
			.unwrap();

		let mut sub = module.subscribe("sub", EmptyServerParams::new(), 1).await.unwrap();

		// This is a hack simulate a very slow client
		// and all older messages are replaced.
		tokio::time::sleep(std::time::Duration::from_secs(10)).await;

		let mut res = Vec::new();

		while let Some(Ok((v, _))) = sub.next::<usize>().await {
			res.push(v);
		}

		// There is no way to cancel pending send operations so
		// that's why 0 is included here.
		assert_eq!(res, vec![0, 17, 18, 19]);
	}
}
