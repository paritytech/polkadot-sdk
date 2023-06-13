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

use crate::error::Result;

use async_std::{
	channel::{bounded, Receiver, Sender},
	stream::StreamExt,
};
use futures::{FutureExt, Stream};
use sp_runtime::DeserializeOwned;
use std::{
	fmt::Debug,
	pin::Pin,
	task::{Context, Poll},
};

/// Once channel reaches this capacity, the subscription breaks.
const CHANNEL_CAPACITY: usize = 128;

/// Underlying subscription type.
pub type UnderlyingSubscription<T> = Box<dyn Stream<Item = T> + Unpin + Send>;

/// Chainable stream that transforms items of type `Result<T, E>` to items of type `T`.
///
/// If it encounters an item of type `Err`, it returns `Poll::Ready(None)`
/// and terminates the underlying stream.
pub struct Unwrap<S: Stream<Item = std::result::Result<T, E>>, T, E> {
	chain_name: String,
	item_type: String,
	subscription: Option<S>,
}

impl<S: Stream<Item = std::result::Result<T, E>>, T, E> Unwrap<S, T, E> {
	/// Create a new instance of `Unwrap`.
	pub fn new(chain_name: String, item_type: String, subscription: S) -> Self {
		Self { chain_name, item_type, subscription: Some(subscription) }
	}
}

impl<S: Stream<Item = std::result::Result<T, E>> + Unpin, T: DeserializeOwned, E: Debug> Stream
	for Unwrap<S, T, E>
{
	type Item = T;

	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		Poll::Ready(match self.subscription.as_mut() {
			Some(subscription) => match futures::ready!(Pin::new(subscription).poll_next(cx)) {
				Some(Ok(item)) => Some(item),
				Some(Err(e)) => {
					self.subscription.take();
					log::debug!(
						target: "bridge",
						"{} stream of {} has returned error: {:?}. It may need to be restarted",
						self.item_type,
						self.chain_name,
						e,
					);
					None
				},
				None => {
					self.subscription.take();
					log::debug!(
						target: "bridge",
						"{} stream of {} has returned `None`. It may need to be restarted",
						self.item_type,
						self.chain_name,
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
pub struct SharedSubscriptionFactory<T> {
	subscribers_sender: Sender<Sender<Option<T>>>,
}

impl<T: 'static + Clone + DeserializeOwned + Send> SharedSubscriptionFactory<T> {
	/// Create new subscription factory.
	pub async fn new(
		chain_name: String,
		item_type: String,
		subscription: UnderlyingSubscription<std::result::Result<T, jsonrpsee::core::ClientError>>,
	) -> Self {
		let (subscribers_sender, subscribers_receiver) = bounded(CHANNEL_CAPACITY);
		async_std::task::spawn(background_worker(
			chain_name.clone(),
			item_type.clone(),
			Box::new(Unwrap::new(chain_name, item_type, subscription)),
			subscribers_receiver,
		));
		Self { subscribers_sender }
	}

	/// Produce new subscription.
	pub async fn subscribe(&self) -> Result<Subscription<T>> {
		let (items_sender, items_receiver) = bounded(CHANNEL_CAPACITY);
		self.subscribers_sender.try_send(items_sender)?;

		Ok(Subscription { items_receiver, subscribers_sender: self.subscribers_sender.clone() })
	}
}

/// Subscription to some chain events.
pub struct Subscription<T> {
	items_receiver: Receiver<Option<T>>,
	subscribers_sender: Sender<Sender<Option<T>>>,
}

impl<T: 'static + Clone + DeserializeOwned + Send> Subscription<T> {
	/// Create new subscription.
	pub async fn new(
		chain_name: String,
		item_type: String,
		subscription: UnderlyingSubscription<std::result::Result<T, jsonrpsee::core::ClientError>>,
	) -> Result<Self> {
		SharedSubscriptionFactory::<T>::new(chain_name, item_type, subscription)
			.await
			.subscribe()
			.await
	}

	/// Return subscription factory for this subscription.
	pub fn factory(&self) -> SharedSubscriptionFactory<T> {
		SharedSubscriptionFactory { subscribers_sender: self.subscribers_sender.clone() }
	}

	/// Consumes subscription and returns future items stream.
	pub fn into_stream(self) -> impl Stream<Item = T> {
		futures::stream::unfold(self, |mut this| async {
			let item = this.items_receiver.next().await.unwrap_or(None);
			item.map(|i| (i, this))
		})
	}

	/// Return next item from the subscription.
	pub async fn next(&self) -> Result<Option<T>> {
		Ok(self.items_receiver.recv().await?)
	}
}

/// Background worker that is executed in tokio context as `jsonrpsee` requires.
///
/// This task may exit under some circumstances. It'll send the correspondent
/// message (`Err` or `None`) to all known listeners. Also, when it stops, all
/// subsequent reads and new subscribers will get the connection error (`ChannelError`).
async fn background_worker<T: 'static + Clone + DeserializeOwned + Send>(
	chain_name: String,
	item_type: String,
	mut subscription: UnderlyingSubscription<T>,
	mut subscribers_receiver: Receiver<Sender<Option<T>>>,
) {
	fn log_task_exit(chain_name: &str, item_type: &str, reason: &str) {
		log::debug!(
			target: "bridge",
			"Background task of {} subscription of {} has stopped: {}",
			item_type,
			chain_name,
			reason,
		);
	}

	// wait for first subscriber until actually starting subscription
	let subscriber = match subscribers_receiver.next().await {
		Some(subscriber) => subscriber,
		None => {
			// it means that the last subscriber/factory has been dropped, so we need to
			// exit too
			return log_task_exit(&chain_name, &item_type, "client has stopped")
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
						return log_task_exit(&chain_name, &item_type, "client has stopped")
					},
				}
			},
			item = subscription.next().fuse() => {
				let is_stream_finished = item.is_none();
				// notify subscribers
				subscribers.retain(|subscriber| {
					let send_result = subscriber.try_send(item.clone());
					send_result.is_ok()
				});

				// it means that the underlying client has dropped, so we can't do anything here
				// and need to stop the task
				if is_stream_finished {
					return log_task_exit(&chain_name, &item_type, "stream has finished");
				}
			},
		}
	}
}
