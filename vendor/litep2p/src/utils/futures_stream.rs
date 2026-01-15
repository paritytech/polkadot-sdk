// Copyright 2024 litep2p developers
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

use futures::{stream::FuturesUnordered, Stream, StreamExt};

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

/// Wrapper around [`FuturesUnordered`] that wakes a task up automatically.
/// The [`Stream`] implemented by [`FuturesStream`] never terminates and can be
/// polled when contains no futures.
#[derive(Default)]
pub struct FuturesStream<F> {
    futures: FuturesUnordered<F>,
    waker: Option<Waker>,
}

impl<F> FuturesStream<F> {
    /// Create new [`FuturesStream`].
    pub fn new() -> Self {
        Self {
            futures: FuturesUnordered::new(),
            waker: None,
        }
    }

    /// Number of futures in the stream.
    pub fn len(&self) -> usize {
        self.futures.len()
    }

    /// Check if the stream is empty.
    pub fn is_empty(&self) -> bool {
        self.futures.is_empty()
    }

    /// Push a future for processing.
    pub fn push(&mut self, future: F) {
        self.futures.push(future);

        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

impl<F: Future> Stream for FuturesStream<F> {
    type Item = <F as Future>::Output;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let Poll::Ready(Some(result)) = self.futures.poll_next_unpin(cx) else {
            // We must save the current waker to wake up the task when new futures are inserted.
            //
            // Otherwise, simply returning `Poll::Pending` here would cause the task to never be
            // woken up again.
            //
            // We were previously relying on some other task from the `loop tokio::select!` to
            // finish.
            self.waker = Some(cx.waker().clone());

            return Poll::Pending;
        };

        Poll::Ready(Some(result))
    }
}
