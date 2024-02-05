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

//! A wrapper for [`FuturesUnordered`] that wakes the task up once a new future is pushed
//! for it to be polled automatically. It's [`Stream`] never terminates.

use futures::{stream::FuturesUnordered, Future, Stream, StreamExt};
use std::{
	pin::Pin,
	task::{Context, Poll, Waker},
};

/// Wrapper around [`FuturesUnordered`] that wakes a task up automatically.
pub struct FuturesStream<F> {
	futures: FuturesUnordered<F>,
	waker: Option<Waker>,
}

/// Surprizingly, `#[derive(Default)]` doesn't work on [`FuturesStream`].
impl<F> Default for FuturesStream<F> {
	fn default() -> FuturesStream<F> {
		FuturesStream { futures: Default::default(), waker: None }
	}
}

impl<F> FuturesStream<F> {
	/// Push a future for processing.
	pub fn push(&mut self, future: F) {
		self.futures.push(future);

		if let Some(waker) = self.waker.take() {
			waker.wake();
		}
	}

	/// The number of futures in the stream.
	pub fn len(&self) -> usize {
		self.futures.len()
	}
}

impl<F: Future> Stream for FuturesStream<F> {
	type Item = <F as Future>::Output;

	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		let Poll::Ready(Some(result)) = self.futures.poll_next_unpin(cx) else {
			self.waker = Some(cx.waker().clone());

			return Poll::Pending
		};

		Poll::Ready(Some(result))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use futures::future::{BoxFuture, FutureExt};

	/// [`Stream`] implementation for [`FuturesStream`] relies on the undocumented
	/// feature that [`FuturesUnordered`] can be polled and repeatedly yield
	/// `Poll::Ready(None)` before any futures are added into it.
	#[tokio::test]
	async fn empty_futures_unordered_can_be_polled() {
		let mut unordered = FuturesUnordered::<BoxFuture<()>>::default();

		futures::future::poll_fn(|cx| {
			assert_eq!(unordered.poll_next_unpin(cx), Poll::Ready(None));
			assert_eq!(unordered.poll_next_unpin(cx), Poll::Ready(None));

			Poll::Ready(())
		})
		.await;
	}

	/// [`Stream`] implementation for [`FuturesStream`] relies on the undocumented
	/// feature that [`FuturesUnordered`] can be polled and repeatedly yield
	/// `Poll::Ready(None)` after all the futures in it have resolved.
	#[tokio::test]
	async fn deplenished_futures_unordered_can_be_polled() {
		let mut unordered = FuturesUnordered::<BoxFuture<()>>::default();

		unordered.push(futures::future::ready(()).boxed());
		assert_eq!(unordered.next().await, Some(()));

		futures::future::poll_fn(|cx| {
			assert_eq!(unordered.poll_next_unpin(cx), Poll::Ready(None));
			assert_eq!(unordered.poll_next_unpin(cx), Poll::Ready(None));

			Poll::Ready(())
		})
		.await;
	}

	#[tokio::test]
	async fn empty_futures_stream_yields_pending() {
		let mut stream = FuturesStream::<BoxFuture<()>>::default();

		futures::future::poll_fn(|cx| {
			assert_eq!(stream.poll_next_unpin(cx), Poll::Pending);
			Poll::Ready(())
		})
		.await;
	}

	#[tokio::test]
	async fn futures_stream_resolves_futures_and_yields_pending() {
		let mut stream = FuturesStream::default();
		stream.push(futures::future::ready(17));

		futures::future::poll_fn(|cx| {
			assert_eq!(stream.poll_next_unpin(cx), Poll::Ready(Some(17)));
			assert_eq!(stream.poll_next_unpin(cx), Poll::Pending);
			Poll::Ready(())
		})
		.await;
	}
}
