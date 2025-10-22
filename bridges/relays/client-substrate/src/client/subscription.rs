// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

use crate::error::Result as ClientResult;

use async_std::{
	channel::{bounded, Receiver, Sender},
	stream::StreamExt,
};
use futures::{FutureExt, Stream};
use sp_runtime::DeserializeOwned;
use std::{
	fmt::Debug,
	pin::Pin,
	result::Result as StdResult,
	task::{Context, Poll},
};

/// Once channel reaches this capacity, the subscription breaks.
const CHANNEL_CAPACITY: usize = 128;

/// Structure describing a stream.
#[derive(Clone)]
pub struct StreamDescription {
	stream_name: String,
	chain_name: String,
}

impl StreamDescription {
	/// Create a new instance of `StreamDescription`.
	pub fn new(stream_name: String, chain_name: String) -> Self {
		Self { stream_name, chain_name }
	}

	/// Get a stream description.
	fn get(&self) -> String {
		format!("{} stream of {}", self.stream_name, self.chain_name)
	}
}

/// Chainable stream that transforms items of type `Result<T, E>` to items of type `T`.
///
/// If it encounters an item of type `Err`, it returns `Poll::Ready(None)`
/// and terminates the underlying stream.
struct Unwrap<S: Stream<Item = StdResult<T, E>>, T, E> {
	desc: StreamDescription,
	stream: Option<S>,
}

impl<S: Stream<Item = StdResult<T, E>>, T, E> Unwrap<S, T, E> {
	/// Create a new instance of `Unwrap`.
	pub fn new(desc: StreamDescription, stream: S) -> Self {
		Self { desc, stream: Some(stream) }
	}
}

impl<S: Stream<Item = StdResult<T, E>> + Unpin, T: DeserializeOwned, E: Debug> Stream
	for Unwrap<S, T, E>
{
	type Item = T;

	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		Poll::Ready(match self.stream.as_mut() {
			Some(subscription) => match futures::ready!(Pin::new(subscription).poll_next(cx)) {
				Some(Ok(item)) => Some(item),
				Some(Err(e)) => {
					self.stream.take();
					log::debug!(
						target: "bridge",
						"{} has returned error: {:?}. It may need to be restarted",
						self.desc.get(),
						e,
					);
					None
				},
				None => {
					self.stream.take();
					log::debug!(
						target: "bridge",
						"{} has returned `None`. It may need to be restarted",
						self.desc.get()
					);
					None
				},
			},
			None => None,
		})
	}
}

/// Subscription factory that produces subscriptions, sharing the same background thread.
#[derive(Clone)]
pub struct SubscriptionBroadcaster<T> {
	desc: StreamDescription,
	subscribers_sender: Sender<Sender<T>>,
}

impl<T: 'static + Clone + DeserializeOwned + Send> SubscriptionBroadcaster<T> {
	/// Create new subscription factory.
	pub fn new(subscription: Subscription<T>) -> StdResult<Self, Subscription<T>> {
		// It doesn't make sense to further broadcast a broadcasted subscription.
		if subscription.is_broadcasted {
			return Err(subscription)
		}

		let desc = subscription.desc().clone();
		let (subscribers_sender, subscribers_receiver) = bounded(CHANNEL_CAPACITY);
		async_std::task::spawn(background_worker(subscription, subscribers_receiver));
		Ok(Self { desc, subscribers_sender })
	}

	/// Produce new subscription.
	pub async fn subscribe(&self) -> ClientResult<Subscription<T>> {
		let (items_sender, items_receiver) = bounded(CHANNEL_CAPACITY);
		self.subscribers_sender.try_send(items_sender)?;

		Ok(Subscription::new_broadcasted(self.desc.clone(), items_receiver))
	}
}

/// Subscription to some chain events.
pub struct Subscription<T> {
	desc: StreamDescription,
	subscription: Box<dyn Stream<Item = T> + Unpin + Send>,
	is_broadcasted: bool,
}

impl<T: 'static + Clone + DeserializeOwned + Send> Subscription<T> {
	/// Create new forwarded subscription.
	pub fn new_forwarded(
		desc: StreamDescription,
		subscription: impl Stream<Item = StdResult<T, serde_json::Error>> + Unpin + Send + 'static,
	) -> Self {
		Self {
			desc: desc.clone(),
			subscription: Box::new(Unwrap::new(desc, subscription)),
			is_broadcasted: false,
		}
	}

	/// Create new broadcasted subscription.
	pub fn new_broadcasted(
		desc: StreamDescription,
		subscription: impl Stream<Item = T> + Unpin + Send + 'static,
	) -> Self {
		Self { desc, subscription: Box::new(subscription), is_broadcasted: true }
	}

	/// Get the description of the underlying stream
	pub fn desc(&self) -> &StreamDescription {
		&self.desc
	}
}

impl<T> Stream for Subscription<T> {
	type Item = T;

	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		Poll::Ready(futures::ready!(Pin::new(&mut self.subscription).poll_next(cx)))
	}
}

/// Background worker that is executed in tokio context as `jsonrpsee` requires.
///
/// This task may exit under some circumstances. It'll send the correspondent
/// message (`Err` or `None`) to all known listeners. Also, when it stops, all
/// subsequent reads and new subscribers will get the connection error (`ChannelError`).
async fn background_worker<T: 'static + Clone + DeserializeOwned + Send>(
	mut subscription: Subscription<T>,
	mut subscribers_receiver: Receiver<Sender<T>>,
) {
	fn log_task_exit(desc: &StreamDescription, reason: &str) {
		log::debug!(
			target: "bridge",
			"Background task of subscription broadcaster for {} has stopped: {}",
			desc.get(),
			reason,
		);
	}

	// wait for first subscriber until actually starting subscription
	let subscriber = match subscribers_receiver.next().await {
		Some(subscriber) => subscriber,
		None => {
			// it means that the last subscriber/factory has been dropped, so we need to
			// exit too
			return log_task_exit(subscription.desc(), "client has stopped")
		},
	};

	// actually subscribe
	let mut subscribers = vec![subscriber];

	// start listening for new items and receivers
	loop {
		futures::select! {
			subscriber = subscribers_receiver.next().fuse() => {
				match subscriber {
					Some(subscriber) => subscribers.push(subscriber),
					None => {
						// it means that the last subscriber/factory has been dropped, so we need to
						// exit too
						return log_task_exit(subscription.desc(), "client has stopped")
					},
				}
			},
			maybe_item = subscription.subscription.next().fuse() => {
				match maybe_item {
					Some(item) => {
						// notify subscribers
						subscribers.retain(|subscriber| {
							let send_result = subscriber.try_send(item.clone());
							send_result.is_ok()
						});
					}
					None => {
						// The underlying client has dropped, so we can't do anything here
						// and need to stop the task.
						return log_task_exit(subscription.desc(), "stream has finished");
					}
				}
			},
		}
	}
}
