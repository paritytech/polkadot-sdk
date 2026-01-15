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

//! This example demonstrates a simple echo server using the notification protocol
//! in which client connects to server and sends a message to server every 3 seconds
//!
//! Run: `cargo run --example echo_notification`

use litep2p::{
    config::ConfigBuilder,
    protocol::notification::{
        ConfigBuilder as NotificationConfigBuilder, NotificationEvent, NotificationHandle,
        ValidationResult,
    },
    transport::quic::config::Config as QuicConfig,
    types::protocol::ProtocolName,
    Litep2p, PeerId,
};

use futures::StreamExt;

use std::time::Duration;

/// event loop for the client
async fn client_event_loop(mut litep2p: Litep2p, mut handle: NotificationHandle, peer: PeerId) {
    // open substream to `peer`
    //
    // if `litep2p` is not connected to `peer` but it has at least one known address,
    // `NotifcationHandle::open_substream()` will automatically dial `peer`
    handle.open_substream(peer).await.unwrap();

    // wait until the substream is opened
    loop {
        tokio::select! {
            _ = litep2p.next_event() => {}
            event = handle.next() =>
                if let NotificationEvent::NotificationStreamOpened { .. } = event.unwrap() {
                    break
                }
        }
    }

    // after the substream is open, send notification to server and print the response to stdout
    loop {
        tokio::select! {
            _ = litep2p.next_event() => {}
            event = handle.next() =>
                if let NotificationEvent::NotificationReceived { peer, notification } = event.unwrap() {
                    println!("received response from server ({peer:?}): {notification:?}");
                },
            _ = tokio::time::sleep(Duration::from_secs(3)) => {
                handle.send_sync_notification(peer, vec![1, 3, 3, 7]).unwrap();
            }
        }
    }
}

/// event loop for the server
async fn server_event_loop(mut litep2p: Litep2p, mut handle: NotificationHandle) {
    loop {
        tokio::select! {
            _ = litep2p.next_event() => {}
            event = handle.next() => match event.unwrap() {
                NotificationEvent::ValidateSubstream { peer, .. } => {
                    handle.send_validation_result(peer, ValidationResult::Accept);
                }
                NotificationEvent::NotificationReceived { peer, notification } => {
                    handle.send_async_notification(peer, notification.freeze().into()).await.unwrap();
                }
                _ => {},
            },
        }
    }
}

/// helper function for creating `Litep2p` object
fn make_litep2p() -> (Litep2p, NotificationHandle) {
    // build notification config for the notification protocol
    let (echo_config, echo_handle) = NotificationConfigBuilder::new(ProtocolName::from("/echo/1"))
        .with_max_size(256)
        .with_auto_accept_inbound(true)
        .with_handshake(vec![1, 3, 3, 7])
        .build();

    // build `Litep2p` object and return it + notification handle
    (
        Litep2p::new(
            ConfigBuilder::new()
                .with_quic(QuicConfig {
                    listen_addresses: vec!["/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap()],
                    ..Default::default()
                })
                .with_notification_protocol(echo_config)
                .build(),
        )
        .unwrap(),
        echo_handle,
    )
}

#[tokio::main]
async fn main() {
    // build `Litep2p` objects for both peers
    let (mut litep2p1, echo_handle1) = make_litep2p();
    let (litep2p2, echo_handle2) = make_litep2p();

    // get the first (and only) listen address for the second peer
    // and add it as a known address for `litep2p1`
    let listen_address = litep2p2.listen_addresses().next().unwrap().clone();
    let peer = *litep2p2.local_peer_id();

    litep2p1.add_known_address(peer, vec![listen_address].into_iter());

    // start event loops for client and server
    tokio::spawn(client_event_loop(litep2p1, echo_handle1, peer));
    tokio::spawn(server_event_loop(litep2p2, echo_handle2));

    loop {
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}
