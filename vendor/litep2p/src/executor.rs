// Copyright 2023 litep2p developers
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

//! Behavior defining how futures running in the background should be executed.

use std::{future::Future, pin::Pin};

/// Trait which defines the interface the executor must implement.
pub trait Executor: Send + Sync {
    /// Start executing a future in the background.
    fn run(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>);

    /// Start executing a future in the background and give the future a name;
    fn run_with_name(&self, name: &'static str, future: Pin<Box<dyn Future<Output = ()> + Send>>);
}

/// Default executor, defaults to calling `tokio::spawn()`.
pub(crate) struct DefaultExecutor;

impl Executor for DefaultExecutor {
    fn run(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        tokio::spawn(future);
    }

    fn run_with_name(&self, _: &'static str, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        tokio::spawn(future);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::channel;

    #[tokio::test]
    async fn run_with_name() {
        let executor = DefaultExecutor;
        let (tx, mut rx) = channel(1);

        let sender = tx.clone();
        executor.run(Box::pin(async move {
            sender.send(1337usize).await.unwrap();
        }));

        executor.run_with_name(
            "test",
            Box::pin(async move {
                tx.send(1337usize).await.unwrap();
            }),
        );

        assert_eq!(rx.recv().await.unwrap(), 1337usize);
        assert_eq!(rx.recv().await.unwrap(), 1337usize);
    }
}
