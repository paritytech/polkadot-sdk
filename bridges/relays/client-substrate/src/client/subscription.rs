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
use futures::{future::FutureExt, Stream};
use sp_runtime::DeserializeOwned;
use std::future::Future;

/// Once channel reaches this capacity, the subscription breaks.
const CHANNEL_CAPACITY: usize = 128;

/// Underlying subscription type.
pub type UnderlyingSubscription<T> = Box<dyn Stream<Item = Result<T>> + Unpin + Send>;

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
		subscribe: impl Future<Output = Result<UnderlyingSubscription<T>>> + Send + 'static,
	) -> Self {
		let (subscribers_sender, subscribers_receiver) = bounded(CHANNEL_CAPACITY);
		async_std::task::spawn(background_worker(
			chain_name,
			item_type,
			subscribe,
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
		subscription: UnderlyingSubscription<T>,
	) -> Result<Self> {
		SharedSubscriptionFactory::<T>::new(
			chain_name,
			item_type,
			futures::future::ready(Ok(subscription)),
		)
		.await
		.subscribe()
		.await
	}

	/// Return subscription factory for this subscription.
	pub fn factory(&self) -> SharedSubscriptionFactory<T> {
		SharedSubscriptionFactory { subscribers_sender: self.subscribers_sender.clone() }
	}

	/// Consumes subscription and returns future items stream.
	pub fn into_stream(self) -> impl futures::Stream<Item = T> {
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
	subscribe: impl Future<Output = Result<UnderlyingSubscription<T>>> + Send + 'static,
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

	async fn notify_subscribers<T: Clone>(
		chain_name: &str,
		item_type: &str,
		subscribers: &mut Vec<Sender<Option<T>>>,
		result: Option<Result<T>>,
	) {
		let result_to_send = match result {
			Some(Ok(item)) => Some(item),
			Some(Err(e)) => {
				log::debug!(
					target: "bridge",
					"{} stream of {} has returned error: {:?}. It may need to be restarted",
					item_type,
					chain_name,
					e,
				);
				None
			},
			None => {
				log::debug!(
					target: "bridge",
					"{} stream of {} has returned `None`. It may need to be restarted",
					item_type,
					chain_name,
				);
				None
			},
		};

		let mut i = 0;
		while i < subscribers.len() {
			let result_to_send = result_to_send.clone();
			let send_result = subscribers[i].try_send(result_to_send);
			match send_result {
				Ok(_) => {
					i += 1;
				},
				Err(_) => {
					subscribers.swap_remove(i);
				},
			}
		}
	}

	log::trace!(
		target: "bridge",
		"Starting background task for {} {} subscription stream.",
		chain_name,
		item_type,
	);

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
	let mut jsonrpsee_subscription = match subscribe.await {
		Ok(jsonrpsee_subscription) => jsonrpsee_subscription,
		Err(e) => {
			let reason = format!("failed to subscribe: {:?}", e);
			notify_subscribers(&chain_name, &item_type, &mut subscribers, Some(Err(e))).await;

			// we cant't do anything without underlying subscription, so let's exit
			return log_task_exit(&chain_name, &item_type, &reason)
		},
	};

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
			item = jsonrpsee_subscription.next().fuse() => {
				let is_stream_finished = item.is_none();
				let item = item.map(|r| r.map_err(Into::into));
				notify_subscribers(&chain_name, &item_type, &mut subscribers, item).await;

				// it means that the underlying client has dropped, so we can't do anything here
				// and need to stop the task
				if is_stream_finished {
					return log_task_exit(&chain_name, &item_type, "stream has finished");
				}
			},
		}
	}
}
