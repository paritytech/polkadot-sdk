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

//! This examples demonstrates how a custom task executor can be used with litep2p.
//!
//! In general, a custom task executor is not needed and litep2p defaults to calling
//! `tokio::spawn()` for futures that should be run in the background but if user wishes
//! to add some extra features, such as couting how many times each future has been polled
//! and for how long, they can be implemented on top of the custom task executor.
//!
//! Run: `RUST_LOG=info cargo run --example custom_executor`

use litep2p::{
    config::ConfigBuilder,
    executor::Executor,
    protocol::libp2p::ping::{Config as PingConfig, PingEvent},
    transport::tcp::config::Config as TcpConfig,
    Litep2p,
};

use futures::{future::BoxFuture, stream::FuturesUnordered, Stream, StreamExt};
use tokio::sync::mpsc::{channel, Receiver, Sender};

use std::{future::Future, pin::Pin, sync::Arc};

/// Task executor.
///
/// Just a wrapper around `FuturesUnordered` which receives the futures over `mpsc::Receiver`.
struct TaskExecutor {
    rx: Receiver<Pin<Box<dyn Future<Output = ()> + Send>>>,
    futures: FuturesUnordered<BoxFuture<'static, ()>>,
}

impl TaskExecutor {
    /// Create new [`TaskExecutor`].
    fn new() -> (Self, Sender<Pin<Box<dyn Future<Output = ()> + Send>>>) {
        let (tx, rx) = channel(64);

        (
            Self {
                rx,
                futures: FuturesUnordered::new(),
            },
            tx,
        )
    }

    /// Drive the futures forward and poll the receiver for any new futures.
    async fn next(&mut self) {
        loop {
            tokio::select! {
                future = self.rx.recv() => self.futures.push(future.unwrap()),
                _ = self.futures.next(), if !self.futures.is_empty() => {}
            }
        }
    }
}

struct TaskExecutorHandle {
    tx: Sender<Pin<Box<dyn Future<Output = ()> + Send>>>,
}

impl Executor for TaskExecutorHandle {
    fn run(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        let _ = self.tx.try_send(future);
    }

    fn run_with_name(&self, _: &'static str, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        let _ = self.tx.try_send(future);
    }
}

fn make_litep2p() -> (
    Litep2p,
    TaskExecutor,
    Box<dyn Stream<Item = PingEvent> + Send + Unpin>,
) {
    let (executor, sender) = TaskExecutor::new();
    let (ping_config, ping_event_stream) = PingConfig::default();

    let litep2p = Litep2p::new(
        ConfigBuilder::new()
            .with_executor(Arc::new(TaskExecutorHandle { tx: sender.clone() }))
            .with_tcp(TcpConfig {
                listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
                ..Default::default()
            })
            .with_libp2p_ping(ping_config)
            .build(),
    )
    .unwrap();

    (litep2p, executor, ping_event_stream)
}

#[tokio::main]
async fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    // create two identical litep2ps
    let (mut litep2p1, mut executor1, mut ping_event_stream1) = make_litep2p();
    let (mut litep2p2, mut executor2, mut ping_event_stream2) = make_litep2p();

    // dial `litep2p1`
    litep2p2
        .dial_address(litep2p1.listen_addresses().next().unwrap().clone())
        .await
        .unwrap();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = executor1.next() => {}
                _ = litep2p1.next_event() => {},
                _ = ping_event_stream1.next() => {},
            }
        }
    });

    // poll litep2p, task executor and ping event stream all together
    //
    // since a custom task executor was provided, it's now the user's responsibility
    // to actually make sure to poll those futures so that litep2p can make progress
    loop {
        tokio::select! {
            _ = executor2.next() => {}
            _ = litep2p2.next_event() => {},
            event = ping_event_stream2.next() =>
                if let Some(PingEvent::Ping { peer, ping }) = event {
                    tracing::info!("ping time with {peer:?}: {ping:?}")
                }
        }
    }
}
