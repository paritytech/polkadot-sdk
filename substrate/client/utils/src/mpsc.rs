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

//! Code to meter unbounded channels.

pub use async_channel::{TryRecvError, TrySendError};

use crate::metrics::{
	DROPPED_LABEL, RECEIVED_LABEL, SENT_LABEL, UNBOUNDED_CHANNELS_COUNTER, UNBOUNDED_CHANNELS_SIZE,
};
use async_channel::{Receiver, Sender};
use futures::{
	SinkExt,
	channel::mpsc,
	lock::Mutex,
	stream::{FusedStream, Stream},
	task::{Context, Poll},
};
use log::error;
use sp_arithmetic::traits::SaturatedConversion;
use std::{
	backtrace::Backtrace,
	pin::Pin,
	sync::{
		Arc,
		atomic::{AtomicBool, Ordering},
	},
};

/// Policy for a bounded channel with shared sender capacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelPolicy {
	name: &'static str,
	capacity: usize,
}

impl ChannelPolicy {
	/// Construct a bounded channel policy.
	pub const fn bounded(name: &'static str, capacity: usize) -> Self {
		Self { name, capacity }
	}

	/// Policy name.
	pub const fn name(self) -> &'static str {
		self.name
	}

	/// Bounded channel capacity.
	pub const fn capacity(self) -> usize {
		self.capacity
	}
}

/// A cloneable sender that serializes sends through one bounded sender.
///
/// This is useful for callback APIs that need to clone a sender into many tasks but still want one
/// real bounded queue and one shared backpressure point.
#[derive(Debug)]
pub struct SharedCapacitySender<T> {
	inner: Arc<Mutex<mpsc::Sender<T>>>,
	policy: ChannelPolicy,
}

impl<T> Clone for SharedCapacitySender<T> {
	fn clone(&self) -> Self {
		Self { inner: self.inner.clone(), policy: self.policy }
	}
}

impl<T> SharedCapacitySender<T> {
	/// Send a value according to this sender's channel policy.
	pub async fn send(&self, value: T) -> Result<(), mpsc::SendError> {
		self.inner.lock().await.send(value).await
	}

	/// Channel policy for this sender.
	pub const fn policy(&self) -> ChannelPolicy {
		self.policy
	}
}

/// Construct a bounded channel whose cloned senders share one bounded sender.
pub fn shared_capacity_channel<T>(
	policy: ChannelPolicy,
) -> (SharedCapacitySender<T>, mpsc::Receiver<T>) {
	let (sender, receiver) = bounded_channel(policy);
	(SharedCapacitySender { inner: Arc::new(Mutex::new(sender)), policy }, receiver)
}

/// Construct a bounded futures channel from a named policy.
pub fn bounded_channel<T>(policy: ChannelPolicy) -> (mpsc::Sender<T>, mpsc::Receiver<T>) {
	mpsc::channel(policy.capacity)
}

/// Wrapper Type around [`async_channel::Sender`] that increases the global
/// measure when a message is added.
#[derive(Debug)]
pub struct TracingUnboundedSender<T> {
	inner: Sender<T>,
	name: &'static str,
	queue_size_warning: usize,
	warning_fired: Arc<AtomicBool>,
	creation_backtrace: Arc<Backtrace>,
}

// Strangely, deriving `Clone` requires that `T` is also `Clone`.
impl<T> Clone for TracingUnboundedSender<T> {
	fn clone(&self) -> Self {
		Self {
			inner: self.inner.clone(),
			name: self.name,
			queue_size_warning: self.queue_size_warning,
			warning_fired: self.warning_fired.clone(),
			creation_backtrace: self.creation_backtrace.clone(),
		}
	}
}

/// Wrapper Type around [`async_channel::Receiver`] that decreases the global
/// measure when a message is polled.
#[derive(Debug)]
pub struct TracingUnboundedReceiver<T> {
	inner: Receiver<T>,
	name: &'static str,
}

/// Wrapper around [`async_channel::unbounded`] that tracks the in- and outflow via
/// `UNBOUNDED_CHANNELS_COUNTER` and warns if the message queue grows
/// above the warning threshold.
pub fn tracing_unbounded<T>(
	name: &'static str,
	queue_size_warning: usize,
) -> (TracingUnboundedSender<T>, TracingUnboundedReceiver<T>) {
	let (s, r) = async_channel::unbounded();
	let sender = TracingUnboundedSender {
		inner: s,
		name,
		queue_size_warning,
		warning_fired: Arc::new(AtomicBool::new(false)),
		creation_backtrace: Arc::new(Backtrace::force_capture()),
	};
	let receiver = TracingUnboundedReceiver { inner: r, name: name.into() };
	(sender, receiver)
}

/// Construct an instrumented unbounded channel from a named policy.
pub fn tracing_unbounded_with_policy<T>(
	policy: ChannelPolicy,
) -> (TracingUnboundedSender<T>, TracingUnboundedReceiver<T>) {
	tracing_unbounded(policy.name(), policy.capacity())
}

impl<T> TracingUnboundedSender<T> {
	/// Proxy function to [`async_channel::Sender`].
	pub fn is_closed(&self) -> bool {
		self.inner.is_closed()
	}

	/// Proxy function to [`async_channel::Sender`].
	pub fn close(&self) -> bool {
		self.inner.close()
	}

	/// Proxy function to `async_channel::Sender::try_send`.
	pub fn unbounded_send(&self, msg: T) -> Result<(), TrySendError<T>> {
		self.inner.try_send(msg).inspect(|_| {
			UNBOUNDED_CHANNELS_COUNTER.with_label_values(&[self.name, SENT_LABEL]).inc();
			UNBOUNDED_CHANNELS_SIZE
				.with_label_values(&[self.name])
				.set(self.inner.len().saturated_into());

			if self.inner.len() >= self.queue_size_warning
				&& self
					.warning_fired
					.compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
					.is_ok()
			{
				error!(
					"The number of unprocessed messages in channel `{}` exceeded {}.\n\
					 The channel was created at:\n{}\n
					 Last message was sent from:\n{}",
					self.name,
					self.queue_size_warning,
					self.creation_backtrace,
					Backtrace::force_capture(),
				);
			}
		})
	}

	/// The number of elements in the channel (proxy function to [`async_channel::Sender`]).
	pub fn len(&self) -> usize {
		self.inner.len()
	}
}

impl<T> TracingUnboundedReceiver<T> {
	/// Proxy function to [`async_channel::Receiver`].
	pub fn close(&mut self) -> bool {
		self.inner.close()
	}

	/// Proxy function to [`async_channel::Receiver`]
	/// that discounts the messages taken out.
	pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
		self.inner.try_recv().inspect(|_| {
			UNBOUNDED_CHANNELS_COUNTER.with_label_values(&[self.name, RECEIVED_LABEL]).inc();
			UNBOUNDED_CHANNELS_SIZE
				.with_label_values(&[self.name])
				.set(self.inner.len().saturated_into());
		})
	}

	/// The number of elements in the channel (proxy function to [`async_channel::Receiver`]).
	pub fn len(&self) -> usize {
		self.inner.len()
	}

	/// The name of this receiver
	pub fn name(&self) -> &'static str {
		self.name
	}
}

impl<T> Drop for TracingUnboundedReceiver<T> {
	fn drop(&mut self) {
		// Close the channel to prevent any further messages to be sent into the channel
		self.close();
		// The number of messages about to be dropped
		let count = self.inner.len();
		// Discount the messages
		if count > 0 {
			UNBOUNDED_CHANNELS_COUNTER
				.with_label_values(&[self.name, DROPPED_LABEL])
				.inc_by(count.saturated_into());
		}
		// Reset the size metric to 0
		UNBOUNDED_CHANNELS_SIZE.with_label_values(&[self.name]).set(0);
		// Drain all the pending messages in the channel since they can never be accessed,
		// this can be removed once https://github.com/smol-rs/async-channel/issues/23 is
		// resolved
		while let Ok(_) = self.inner.try_recv() {}
	}
}

impl<T> Unpin for TracingUnboundedReceiver<T> {}

impl<T> Stream for TracingUnboundedReceiver<T> {
	type Item = T;

	fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
		let s = self.get_mut();
		match Pin::new(&mut s.inner).poll_next(cx) {
			Poll::Ready(msg) => {
				if msg.is_some() {
					UNBOUNDED_CHANNELS_COUNTER.with_label_values(&[s.name, RECEIVED_LABEL]).inc();
					UNBOUNDED_CHANNELS_SIZE
						.with_label_values(&[s.name])
						.set(s.inner.len().saturated_into());
				}
				Poll::Ready(msg)
			},
			Poll::Pending => Poll::Pending,
		}
	}
}

impl<T> FusedStream for TracingUnboundedReceiver<T> {
	fn is_terminated(&self) -> bool {
		self.inner.is_terminated()
	}
}

#[cfg(test)]
mod tests {
	use super::{
		ChannelPolicy, bounded_channel, shared_capacity_channel, tracing_unbounded,
		tracing_unbounded_with_policy,
	};
	use async_channel::{self, RecvError, TryRecvError};
	use futures::{StreamExt, executor::block_on, join};

	#[test]
	fn test_tracing_unbounded_receiver_drop() {
		let (tracing_unbounded_sender, tracing_unbounded_receiver) =
			tracing_unbounded("test-receiver-drop", 10);
		let (tx, rx) = async_channel::unbounded::<usize>();

		tracing_unbounded_sender.unbounded_send(tx).unwrap();
		drop(tracing_unbounded_receiver);

		assert_eq!(rx.try_recv(), Err(TryRecvError::Closed));
		assert_eq!(rx.recv_blocking(), Err(RecvError));
	}

	#[test]
	fn shared_capacity_sender_clones_share_one_bounded_queue() {
		block_on(async {
			let (sender, mut receiver) =
				shared_capacity_channel(ChannelPolicy::bounded("test-shared-capacity", 1));
			let cloned_sender = sender.clone();

			assert_eq!(sender.policy(), cloned_sender.policy());
			let (send_result, received) = join!(sender.send(1), receiver.next());
			send_result.expect("receiver is live");
			assert_eq!(received, Some(1));

			let (send_result, received) = join!(cloned_sender.send(2), receiver.next());
			send_result.expect("receiver is live");
			assert_eq!(received, Some(2));
		});
	}

	#[test]
	fn channel_policies_construct_bounded_and_tracing_channels() {
		let policy = ChannelPolicy::bounded("test-policy", 2);
		assert_eq!(policy.name(), "test-policy");
		assert_eq!(policy.capacity(), 2);

		let (mut bounded_sender, _bounded_receiver) = bounded_channel::<usize>(policy);
		bounded_sender.try_send(1).unwrap();

		let (tracing_sender, _tracing_receiver) = tracing_unbounded_with_policy::<usize>(policy);
		assert_eq!(tracing_sender.len(), 0);
	}
}
