// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Provides mpsc notification channel that can be instantiated
//! _after_ it's been shared to the consumer and producers entities.
//!
//! Useful when building RPC extensions where, at service definition time, we
//! don't know whether the specific interface where the RPC extension will be
//! exposed is safe or not and we want to lazily build the RPC extension
//! whenever we bind the service to an interface.
//!
//! See [`sc-service::builder::RpcExtensionBuilder`] for more details.

use futures::stream::{FusedStream, Stream};
use std::{
	pin::Pin,
	task::{Context, Poll},
};

use crate::pubsub::{Hub, Receiver};

mod registry;
use registry::Registry;

#[cfg(test)]
mod tests;

/// Trait used to define the "tracing key" string used to tag
/// and identify the mpsc channels.
pub trait TracingKeyStr {
	/// Const `str` representing the "tracing key" used to tag and identify
	/// the mpsc channels owned by the object implementing this trait.
	const TRACING_KEY: &'static str;
}

/// The receiving half of the notifications channel.
///
/// The [`NotificationStream`] entity stores the [`Hub`] so it can be
/// used to add more subscriptions.
#[derive(Clone)]
pub struct NotificationStream<Payload, TK: TracingKeyStr> {
	hub: Hub<Payload, Registry>,
	_pd: std::marker::PhantomData<TK>,
}

/// The receiving half of the notifications channel(s).
#[derive(Debug)]
pub struct NotificationReceiver<Payload> {
	receiver: Receiver<Payload, Registry>,
}

/// The sending half of the notifications channel(s).
pub struct NotificationSender<Payload> {
	hub: Hub<Payload, Registry>,
}

impl<Payload, TK: TracingKeyStr> NotificationStream<Payload, TK> {
	/// Creates a new pair of receiver and sender of `Payload` notifications.
	pub fn channel() -> (NotificationSender<Payload>, Self) {
		let hub = Hub::new(TK::TRACING_KEY);
		let sender = NotificationSender { hub: hub.clone() };
		let receiver = NotificationStream { hub, _pd: Default::default() };
		(sender, receiver)
	}

	/// Subscribe to a channel through which the generic payload can be received.
	pub fn subscribe(&self, queue_size_warning: usize) -> NotificationReceiver<Payload> {
		let receiver = self.hub.subscribe((), queue_size_warning);
		NotificationReceiver { receiver }
	}
}

impl<Payload> NotificationSender<Payload> {
	/// Send out a notification to all subscribers that a new payload is available for a
	/// block.
	pub fn notify<Error>(
		&self,
		payload: impl FnOnce() -> Result<Payload, Error>,
	) -> Result<(), Error>
	where
		Payload: Clone,
	{
		self.hub.send(payload)
	}
}

impl<Payload> Clone for NotificationSender<Payload> {
	fn clone(&self) -> Self {
		Self { hub: self.hub.clone() }
	}
}

impl<Payload> Unpin for NotificationReceiver<Payload> {}

impl<Payload> Stream for NotificationReceiver<Payload> {
	type Item = Payload;

	fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Payload>> {
		Pin::new(&mut self.get_mut().receiver).poll_next(cx)
	}
}

impl<Payload> FusedStream for NotificationReceiver<Payload> {
	fn is_terminated(&self) -> bool {
		self.receiver.is_terminated()
	}
}
