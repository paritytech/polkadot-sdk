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

use litep2p::{
    config::ConfigBuilder,
    crypto::ed25519::Keypair,
    executor::Executor,
    protocol::{
        notification::{
            Config as NotificationConfig, Direction, NotificationEvent, ValidationResult,
        },
        request_response::{
            ConfigBuilder as RequestResponseConfigBuilder, DialOptions, RequestResponseEvent,
        },
    },
    transport::tcp::config::Config as TcpConfig,
    types::protocol::ProtocolName,
    Litep2p, Litep2pEvent,
};

use bytes::BytesMut;
use futures::{future::BoxFuture, stream::FuturesUnordered, StreamExt};
use tokio::sync::mpsc::{channel, Receiver, Sender};

use std::{future::Future, pin::Pin, sync::Arc};

struct TaskExecutor {
    rx: Receiver<Pin<Box<dyn Future<Output = ()> + Send>>>,
    futures: FuturesUnordered<BoxFuture<'static, ()>>,
}

impl TaskExecutor {
    pub fn new() -> (Self, Sender<Pin<Box<dyn Future<Output = ()> + Send>>>) {
        let (tx, rx) = channel(64);

        (
            Self {
                rx,
                futures: FuturesUnordered::new(),
            },
            tx,
        )
    }

    async fn next(&mut self) {
        tokio::select! {
            future = self.rx.recv() => {
                self.futures.push(future.unwrap());
            }
            _ = self.futures.next(), if !self.futures.is_empty() => {}
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

#[tokio::test]
async fn custom_executor() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut executor, sender) = TaskExecutor::new();

    tokio::spawn(async move {
        loop {
            executor.next().await
        }
    });

    let (notif_config1, mut handle1) = NotificationConfig::new(
        ProtocolName::from("/notif/1"),
        1024usize,
        vec![1, 2, 3, 4],
        Vec::new(),
        false,
        64,
        64,
        true,
    );
    let (req_resp_config1, mut req_resp_handle1) =
        RequestResponseConfigBuilder::new(ProtocolName::from("/protocol/1"))
            .with_max_size(1024)
            .build();

    let handle = TaskExecutorHandle { tx: sender.clone() };
    let config1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1)
        .with_request_response_protocol(req_resp_config1)
        .with_executor(Arc::new(handle))
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .build();

    let (notif_config2, mut handle2) = NotificationConfig::new(
        ProtocolName::from("/notif/1"),
        1024usize,
        vec![1, 2, 3, 4],
        Vec::new(),
        false,
        64,
        64,
        true,
    );
    let (req_resp_config2, mut req_resp_handle2) =
        RequestResponseConfigBuilder::new(ProtocolName::from("/protocol/1"))
            .with_max_size(1024)
            .build();

    let handle = TaskExecutorHandle { tx: sender };
    let config2 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config2)
        .with_request_response_protocol(req_resp_config2)
        .with_executor(Arc::new(handle))
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected and spawn the litep2p objects in the background
    let address = litep2p2.listen_addresses().next().unwrap().clone();
    litep2p1.dial_address(address).await.unwrap();

    let mut litep2p1_connected = false;
    let mut litep2p2_connected = false;

    loop {
        tokio::select! {
            event = litep2p1.next_event() => if let Litep2pEvent::ConnectionEstablished { .. } = event.unwrap() {
                litep2p1_connected = true;
            },
            event = litep2p2.next_event() => if let Litep2pEvent::ConnectionEstablished { .. } = event.unwrap() {
                litep2p2_connected = true;
            }
        }

        if litep2p1_connected && litep2p2_connected {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            break;
        }
    }
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {},
                _ = litep2p2.next_event() => {},
            }
        }
    });

    // open substream for `peer2` and accept it
    handle1.open_substream(peer2).await.unwrap();
    assert_eq!(
        handle2.next().await.unwrap(),
        NotificationEvent::ValidateSubstream {
            protocol: ProtocolName::from("/notif/1"),
            fallback: None,
            peer: peer1,
            handshake: vec![1, 2, 3, 4],
        }
    );
    handle2.send_validation_result(peer1, ValidationResult::Accept);

    assert_eq!(
        handle1.next().await.unwrap(),
        NotificationEvent::ValidateSubstream {
            protocol: ProtocolName::from("/notif/1"),
            fallback: None,
            peer: peer2,
            handshake: vec![1, 2, 3, 4],
        }
    );
    handle1.send_validation_result(peer2, ValidationResult::Accept);

    assert_eq!(
        handle2.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpened {
            protocol: ProtocolName::from("/notif/1"),
            direction: Direction::Inbound,
            fallback: None,
            peer: peer1,
            handshake: vec![1, 2, 3, 4],
        }
    );
    assert_eq!(
        handle1.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpened {
            protocol: ProtocolName::from("/notif/1"),
            fallback: None,
            direction: Direction::Outbound,
            peer: peer2,
            handshake: vec![1, 2, 3, 4],
        }
    );

    handle1.send_sync_notification(peer2, vec![1, 3, 3, 7]).unwrap();
    handle2.send_sync_notification(peer1, vec![1, 3, 3, 8]).unwrap();

    assert_eq!(
        handle2.next().await.unwrap(),
        NotificationEvent::NotificationReceived {
            peer: peer1,
            notification: BytesMut::from(&[1, 3, 3, 7][..]),
        }
    );
    assert_eq!(
        handle1.next().await.unwrap(),
        NotificationEvent::NotificationReceived {
            peer: peer2,
            notification: BytesMut::from(&[1, 3, 3, 8][..]),
        }
    );

    // verify that the request-response protocol works as well
    req_resp_handle1
        .send_request(peer2, vec![1, 2, 3, 4], DialOptions::Reject)
        .await
        .unwrap();

    match req_resp_handle2.next().await.unwrap() {
        RequestResponseEvent::RequestReceived {
            peer,
            request_id,
            request,
            ..
        } => {
            assert_eq!(peer, peer1);
            assert_eq!(request, vec![1, 2, 3, 4]);
            req_resp_handle2.send_response(request_id, vec![1, 3, 3, 7]);
        }
        event => panic!("unexpected event: {event:?}"),
    }

    match req_resp_handle1.next().await.unwrap() {
        RequestResponseEvent::ResponseReceived { peer, response, .. } => {
            assert_eq!(peer, peer2);
            assert_eq!(response, vec![1, 3, 3, 7]);
        }
        event => panic!("unexpected event: {event:?}"),
    }
}
