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
    config::ConfigBuilder as Litep2pConfigBuilder,
    crypto::ed25519::Keypair,
    error::Error,
    protocol::notification::{
        Config as NotificationConfig, ConfigBuilder, Direction, NotificationError,
        NotificationEvent, NotificationHandle, ValidationResult,
    },
    transport::tcp::config::Config as TcpConfig,
    types::protocol::ProtocolName,
    Litep2p, Litep2pEvent, PeerId,
};

#[cfg(feature = "websocket")]
use litep2p::transport::websocket::config::Config as WebSocketConfig;

use bytes::BytesMut;
use futures::StreamExt;
use multiaddr::{Multiaddr, Protocol};
use multihash::Multihash;

#[cfg(feature = "quic")]
use std::net::Ipv4Addr;
use std::{net::Ipv6Addr, task::Poll, time::Duration};

use crate::common::{add_transport, Transport};

async fn connect_peers(litep2p1: &mut Litep2p, litep2p2: &mut Litep2p) {
    let address = litep2p2.listen_addresses().next().unwrap().clone();
    litep2p1.dial_address(address).await.unwrap();

    let mut litep2p1_connected = false;
    let mut litep2p2_connected = false;

    // Disarm the first tick to avoid immediate timeouts.
    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(5));
    ticker.tick().await;

    loop {
        tokio::select! {
            event = litep2p1.next_event() => if let Litep2pEvent::ConnectionEstablished { .. } = event.unwrap() {
                litep2p1_connected = true;
            },
            event = litep2p2.next_event() => if let Litep2pEvent::ConnectionEstablished { .. } = event.unwrap() {
                litep2p2_connected = true;
            },
            _ = ticker.tick() => {
                panic!("peers failed to connect within timeout");
            }
        }

        if litep2p1_connected && litep2p2_connected {
            break;
        }
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}

async fn make_default_litep2p(transport: Transport) -> (Litep2p, NotificationHandle) {
    let (notif_config, handle) = NotificationConfig::new(
        ProtocolName::from("/notif/1"),
        1024usize,
        vec![1, 2, 3, 4],
        Vec::new(),
        false,
        64,
        64,
        true,
    );
    let config = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config);

    let config = add_transport(config, transport).build();

    (Litep2p::new(config).unwrap(), handle)
}

#[tokio::test]
async fn open_substreams_tcp() {
    open_substreams(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn open_substreams_quic() {
    open_substreams(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn open_substreams_websocket() {
    open_substreams(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

async fn open_substreams(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

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
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

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
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected and spawn the litep2p objects in the background
    connect_peers(&mut litep2p1, &mut litep2p2).await;
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
}

#[tokio::test]
async fn reject_substream_tcp() {
    reject_substream(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn reject_substream_quic() {
    reject_substream(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn reject_substream_websocket() {
    reject_substream(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

async fn reject_substream(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

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
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

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
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected and spawn the litep2p objects in the background
    connect_peers(&mut litep2p1, &mut litep2p2).await;
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
    handle2.send_validation_result(peer1, ValidationResult::Reject);

    assert_eq!(
        handle1.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpenFailure {
            peer: peer2,
            error: NotificationError::Rejected,
        }
    );
}

#[tokio::test]
async fn notification_stream_closed_tcp() {
    notification_stream_closed(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn notification_stream_closed_quic() {
    notification_stream_closed(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn notification_stream_closed_websocket() {
    notification_stream_closed(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

async fn notification_stream_closed(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

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
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

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
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected and spawn the litep2p objects in the background
    connect_peers(&mut litep2p1, &mut litep2p2).await;
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
            fallback: None,
            direction: Direction::Inbound,
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

    handle1.close_substream(peer2).await;

    match handle2.next().await.unwrap() {
        NotificationEvent::NotificationStreamClosed { peer } => assert_eq!(peer, peer1),
        _ => panic!("invalid event received"),
    }
}

#[tokio::test]
async fn reconnect_after_disconnect_tcp() {
    reconnect_after_disconnect(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn reconnect_after_disconnect_quic() {
    reconnect_after_disconnect(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn reconnect_after_disconnect_websocket() {
    reconnect_after_disconnect(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

async fn reconnect_after_disconnect(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

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
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

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
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected and spawn the litep2p objects in the background
    connect_peers(&mut litep2p1, &mut litep2p2).await;
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

    // accept the inbound substreams
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

    // accept the inbound substreams
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
            fallback: None,
            direction: Direction::Inbound,
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

    // close the substream
    handle2.close_substream(peer1).await;

    match handle2.next().await.unwrap() {
        NotificationEvent::NotificationStreamClosed { peer } => assert_eq!(peer, peer1),
        _ => panic!("invalid event received"),
    }

    match handle1.next().await.unwrap() {
        NotificationEvent::NotificationStreamClosed { peer } => assert_eq!(peer, peer2),
        _ => panic!("invalid event received"),
    }

    // open the substream
    handle2.open_substream(peer1).await.unwrap();

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
        NotificationEvent::ValidateSubstream {
            protocol: ProtocolName::from("/notif/1"),
            fallback: None,
            peer: peer1,
            handshake: vec![1, 2, 3, 4],
        }
    );
    handle2.send_validation_result(peer1, ValidationResult::Accept);

    // verify that both peers get the open event
    assert_eq!(
        handle2.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpened {
            protocol: ProtocolName::from("/notif/1"),
            fallback: None,
            direction: Direction::Outbound,
            peer: peer1,
            handshake: vec![1, 2, 3, 4],
        }
    );
    assert_eq!(
        handle1.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpened {
            protocol: ProtocolName::from("/notif/1"),
            fallback: None,
            direction: Direction::Inbound,
            peer: peer2,
            handshake: vec![1, 2, 3, 4],
        }
    );

    // send notifications to verify that the connection works again
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
}

#[tokio::test]
async fn set_new_handshake_tcp() {
    set_new_handshake(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn set_new_handshake_quic() {
    set_new_handshake(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn set_new_handshake_websocket() {
    set_new_handshake(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

async fn set_new_handshake(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

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
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = match transport1 {
        Transport::Tcp(config) => config1.with_tcp(config),
        #[cfg(feature = "quic")]
        Transport::Quic(config) => config1.with_quic(config),
        #[cfg(feature = "websocket")]
        Transport::WebSocket(config) => config1.with_websocket(config),
    }
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
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected and spawn the litep2p objects in the background
    connect_peers(&mut litep2p1, &mut litep2p2).await;
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

    // accept the substreams
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

    // accept the substreams
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
            fallback: None,
            direction: Direction::Inbound,
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

    // close the substream
    handle2.close_substream(peer1).await;

    match handle2.next().await.unwrap() {
        NotificationEvent::NotificationStreamClosed { peer } => assert_eq!(peer, peer1),
        _ => panic!("invalid event received"),
    }

    match handle1.next().await.unwrap() {
        NotificationEvent::NotificationStreamClosed { peer } => assert_eq!(peer, peer2),
        _ => panic!("invalid event received"),
    }

    // set new handshakes and open the substream
    handle1.set_handshake(vec![5, 5, 5, 5]);
    handle2.set_handshake(vec![6, 6, 6, 6]);
    handle2.open_substream(peer1).await.unwrap();

    // accept the substreams
    assert_eq!(
        handle1.next().await.unwrap(),
        NotificationEvent::ValidateSubstream {
            protocol: ProtocolName::from("/notif/1"),
            fallback: None,
            peer: peer2,
            handshake: vec![6, 6, 6, 6],
        }
    );
    handle1.send_validation_result(peer2, ValidationResult::Accept);

    // accept the substreams
    assert_eq!(
        handle2.next().await.unwrap(),
        NotificationEvent::ValidateSubstream {
            protocol: ProtocolName::from("/notif/1"),
            fallback: None,
            peer: peer1,
            handshake: vec![5, 5, 5, 5],
        }
    );
    handle2.send_validation_result(peer1, ValidationResult::Accept);

    // verify that both peers get the open event
    assert_eq!(
        handle2.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpened {
            protocol: ProtocolName::from("/notif/1"),
            fallback: None,
            direction: Direction::Outbound,
            peer: peer1,
            handshake: vec![5, 5, 5, 5],
        }
    );
    assert_eq!(
        handle1.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpened {
            protocol: ProtocolName::from("/notif/1"),
            fallback: None,
            direction: Direction::Inbound,
            peer: peer2,
            handshake: vec![6, 6, 6, 6],
        }
    );
}

#[tokio::test]
async fn both_nodes_open_substreams_tcp() {
    both_nodes_open_substreams(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn both_nodes_open_substreams_quic() {
    both_nodes_open_substreams(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn both_nodes_open_substreams_websocket() {
    both_nodes_open_substreams(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

async fn both_nodes_open_substreams(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

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
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

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
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected and spawn the litep2p objects in the background
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {},
                _ = litep2p2.next_event() => {},
            }
        }
    });

    // both nodes open a substream at the same time
    handle1.open_substream(peer2).await.unwrap();
    handle2.open_substream(peer1).await.unwrap();

    // accept the substreams
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

    // accept the substreams
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
        handle2.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpened {
            protocol: ProtocolName::from("/notif/1"),
            fallback: None,
            direction: Direction::Outbound,
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
}

#[tokio::test]
#[cfg(debug_assertions)]
async fn both_nodes_open_substream_one_rejects_substreams_tcp() {
    both_nodes_open_substream_one_rejects_substreams(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
#[cfg(debug_assertions)]
async fn both_nodes_open_substream_one_rejects_substreams_quic() {
    both_nodes_open_substream_one_rejects_substreams(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
#[cfg(debug_assertions)]
async fn both_nodes_open_substream_one_rejects_substreams_websocket() {
    both_nodes_open_substream_one_rejects_substreams(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

async fn both_nodes_open_substream_one_rejects_substreams(
    transport1: Transport,
    transport2: Transport,
) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

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
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

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
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected and spawn the litep2p objects in the background
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {},
                _ = litep2p2.next_event() => {},
            }
        }
    });

    // both nodes open a substream at the same time
    handle1.open_substream(peer2).await.unwrap();
    handle2.open_substream(peer1).await.unwrap();

    // first peer accepts the substream
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

    // the second peer rejects the substream
    assert_eq!(
        handle2.next().await.unwrap(),
        NotificationEvent::ValidateSubstream {
            protocol: ProtocolName::from("/notif/1"),
            fallback: None,
            peer: peer1,
            handshake: vec![1, 2, 3, 4],
        }
    );
    handle2.send_validation_result(peer1, ValidationResult::Reject);

    assert_eq!(
        handle1.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpenFailure {
            peer: peer2,
            error: NotificationError::Rejected
        },
    );

    assert!(tokio::time::timeout(Duration::from_secs(5), handle2.next()).await.is_err());
}

#[tokio::test]
async fn send_sync_notification_to_non_existent_peer_tcp() {
    send_sync_notification_to_non_existent_peer(Transport::Tcp(TcpConfig {
        listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
        ..Default::default()
    }))
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn send_sync_notification_to_non_existent_peer_quic() {
    send_sync_notification_to_non_existent_peer(Transport::Quic(Default::default())).await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn send_sync_notification_to_non_existent_peer_websocket() {
    send_sync_notification_to_non_existent_peer(Transport::WebSocket(WebSocketConfig {
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
        ..Default::default()
    }))
    .await;
}

async fn send_sync_notification_to_non_existent_peer(transport1: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

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
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {},
            }
        }
    });

    handle1.send_sync_notification(PeerId::random(), vec![1, 3, 3, 7]).unwrap();
}

#[tokio::test]
async fn send_async_notification_to_non_existent_peer_tcp() {
    send_async_notification_to_non_existent_peer(Transport::Tcp(TcpConfig {
        listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
        ..Default::default()
    }))
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn send_async_notification_to_non_existent_peer_quic() {
    send_async_notification_to_non_existent_peer(Transport::Quic(Default::default())).await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn send_async_notification_to_non_existent_peer_websocket() {
    send_async_notification_to_non_existent_peer(Transport::WebSocket(WebSocketConfig {
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
        ..Default::default()
    }))
    .await;
}

async fn send_async_notification_to_non_existent_peer(transport1: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

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
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {},
            }
        }
    });

    assert!(handle1
        .send_async_notification(PeerId::random(), vec![1, 3, 3, 7])
        .await
        .is_err());
}

#[tokio::test]
async fn try_to_connect_to_non_existent_peer_tcp() {
    try_to_connect_to_non_existent_peer(Transport::Tcp(TcpConfig {
        listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
        ..Default::default()
    }))
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn try_to_connect_to_non_existent_peer_quic() {
    try_to_connect_to_non_existent_peer(Transport::Quic(Default::default())).await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn try_to_connect_to_non_existent_peer_websocket() {
    try_to_connect_to_non_existent_peer(Transport::WebSocket(WebSocketConfig {
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
        ..Default::default()
    }))
    .await;
}

async fn try_to_connect_to_non_existent_peer(transport1: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

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
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {},
            }
        }
    });

    let peer = PeerId::random();
    handle1.open_substream(peer).await.unwrap();
    assert_eq!(
        handle1.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpenFailure {
            peer,
            error: NotificationError::DialFailure,
        }
    );
}

#[tokio::test]
async fn try_to_disconnect_non_existent_peer_tcp() {
    try_to_disconnect_non_existent_peer(Transport::Tcp(TcpConfig {
        listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
        ..Default::default()
    }))
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn try_to_disconnect_non_existent_peer_quic() {
    try_to_disconnect_non_existent_peer(Transport::Quic(Default::default())).await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn try_to_disconnect_non_existent_peer_websocket() {
    try_to_disconnect_non_existent_peer(Transport::WebSocket(WebSocketConfig {
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
        ..Default::default()
    }))
    .await;
}

async fn try_to_disconnect_non_existent_peer(transport1: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (notif_config1, handle1) = NotificationConfig::new(
        ProtocolName::from("/notif/1"),
        1024usize,
        vec![1, 2, 3, 4],
        Vec::new(),
        false,
        64,
        64,
        true,
    );
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {},
            }
        }
    });

    handle1.close_substream(PeerId::random()).await;
}

#[tokio::test]
async fn try_to_reopen_substream_tcp() {
    try_to_reopen_substream(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn try_to_reopen_substream_quic() {
    try_to_reopen_substream(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn try_to_reopen_substream_websocket() {
    try_to_reopen_substream(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

async fn try_to_reopen_substream(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

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
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

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
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected and spawn the litep2p objects in the background
    connect_peers(&mut litep2p1, &mut litep2p2).await;
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
            fallback: None,
            direction: Direction::Inbound,
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

    // open substream for `peer2` and accept it
    match handle1.open_substream(peer2).await {
        Err(Error::PeerAlreadyExists(peer)) => assert_eq!(peer, peer2),
        result => panic!("invalid event received: {result:?}"),
    }
}

#[tokio::test]
async fn substream_validation_timeout_tcp() {
    substream_validation_timeout(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn substream_validation_timeout_quic() {
    substream_validation_timeout(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn substream_validation_timeout_websocket() {
    substream_validation_timeout(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

async fn substream_validation_timeout(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

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
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

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
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected and spawn the litep2p objects in the background
    connect_peers(&mut litep2p1, &mut litep2p2).await;
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

    // don't reject the substream but let it timeout
    assert_eq!(
        handle1.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpenFailure {
            peer: peer2,
            error: NotificationError::Rejected,
        }
    );
}

#[tokio::test]
async fn unsupported_protocol_tcp() {
    unsupported_protocol(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn unsupported_protocol_quic() {
    unsupported_protocol(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn unsupported_protocol_websocket() {
    unsupported_protocol(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

async fn unsupported_protocol(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (notif_config1, mut handle1) = ConfigBuilder::new(ProtocolName::from("/notif/1"))
        .with_max_size(1024usize)
        .with_handshake(vec![1, 2, 3, 4])
        .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

    let (notif_config2, _handle2) = ConfigBuilder::new(ProtocolName::from("/notif/2"))
        .with_max_size(1024usize)
        .with_handshake(vec![1, 2, 3, 4])
        .build();
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected and spawn the litep2p objects in the background
    connect_peers(&mut litep2p1, &mut litep2p2).await;
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
        handle1.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpenFailure {
            peer: peer2,
            error: NotificationError::Rejected
        }
    );
}

#[tokio::test]
async fn dialer_fallback_protocol_works_tcp() {
    dialer_fallback_protocol_works(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn dialer_fallback_protocol_works_quic() {
    dialer_fallback_protocol_works(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn dialer_fallback_protocol_works_websocket() {
    dialer_fallback_protocol_works(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

async fn dialer_fallback_protocol_works(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (notif_config1, mut handle1) = ConfigBuilder::new(ProtocolName::from("/notif/2"))
        .with_max_size(1024usize)
        .with_handshake(vec![1, 2, 3, 4])
        .with_fallback_names(vec![ProtocolName::from("/notif/1")])
        .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

    let (notif_config2, mut handle2) = ConfigBuilder::new(ProtocolName::from("/notif/1"))
        .with_max_size(1024usize)
        .with_handshake(vec![1, 2, 3, 4])
        .build();
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected and spawn the litep2p objects in the background
    connect_peers(&mut litep2p1, &mut litep2p2).await;
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
            protocol: ProtocolName::from("/notif/2"),
            fallback: Some(ProtocolName::from("/notif/1")),
            peer: peer2,
            handshake: vec![1, 2, 3, 4],
        }
    );
    handle1.send_validation_result(peer2, ValidationResult::Accept);

    assert_eq!(
        handle2.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpened {
            protocol: ProtocolName::from("/notif/1"),
            fallback: None,
            direction: Direction::Inbound,
            peer: peer1,
            handshake: vec![1, 2, 3, 4],
        }
    );
    assert_eq!(
        handle1.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpened {
            protocol: ProtocolName::from("/notif/2"),
            fallback: Some(ProtocolName::from("/notif/1")),
            direction: Direction::Outbound,
            peer: peer2,
            handshake: vec![1, 2, 3, 4],
        }
    );
}

#[tokio::test]
async fn zero_byte_handshake_tcp() {
    // Full node role.
    zero_byte_handshake(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        vec![1],
    )
    .await;

    // Invalid role set as `ObservedRole::NONE`.
    zero_byte_handshake(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        vec![0],
    )
    .await;

    // Light client role provided by smoldot.
    zero_byte_handshake(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        vec![],
    )
    .await;
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn zero_byte_handshake_quic() {
    zero_byte_handshake(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
        vec![1],
    )
    .await;

    zero_byte_handshake(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
        vec![0],
    )
    .await;

    zero_byte_handshake(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
        vec![],
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn zero_byte_handshake_websocket() {
    // Full node role.
    zero_byte_handshake(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        vec![1],
    )
    .await;

    // Invalid role set as `ObservedRole::NONE`.
    zero_byte_handshake(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        vec![0],
    )
    .await;

    // Light client role provided by smoldot.
    zero_byte_handshake(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        vec![],
    )
    .await;
}

async fn zero_byte_handshake(transport1: Transport, transport2: Transport, handshake: Vec<u8>) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (notif_config1, mut handle1) = ConfigBuilder::new(ProtocolName::from("/notif/1"))
        .with_max_size(1024usize)
        .with_handshake(handshake.clone())
        .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

    let (notif_config2, mut handle2) = ConfigBuilder::new(ProtocolName::from("/notif/1"))
        .with_max_size(1024usize)
        .with_handshake(handshake.clone())
        .build();
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected and spawn the litep2p objects in the background
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {},
                _ = litep2p2.next_event() => {},
            }
        }
    });

    // open substream for `peer2` and accept it
    tracing::info!("Opening substream handle1 => handle2");
    handle1.open_substream(peer2).await.unwrap();

    tracing::info!("Expecting validate substream event...");
    assert_eq!(
        handle2.next().await.unwrap(),
        NotificationEvent::ValidateSubstream {
            protocol: ProtocolName::from("/notif/1"),
            fallback: None,
            peer: peer1,
            handshake: handshake.clone(),
        }
    );

    tracing::info!("Send validation result... peer2 => peer1");
    handle2.send_validation_result(peer1, ValidationResult::Accept);
    assert_eq!(
        handle1.next().await.unwrap(),
        NotificationEvent::ValidateSubstream {
            protocol: ProtocolName::from("/notif/1"),
            fallback: None,
            peer: peer2,
            handshake: handshake.clone(),
        }
    );

    tracing::info!("Send validation result... peer1 => peer2");
    handle1.send_validation_result(peer2, ValidationResult::Accept);

    tracing::info!("Handle2 expecting notification stream opened event...");
    assert_eq!(
        handle2.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpened {
            protocol: ProtocolName::from("/notif/1"),
            fallback: None,
            direction: Direction::Inbound,
            peer: peer1,
            handshake: handshake.clone(),
        }
    );

    tracing::info!("Handle1 expecting notification stream opened event...");
    assert_eq!(
        handle1.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpened {
            protocol: ProtocolName::from("/notif/1"),
            fallback: None,
            direction: Direction::Outbound,
            peer: peer2,
            handshake: handshake,
        }
    );

    // This step ensures we have not messed with the notification frames.
    tracing::info!("Send sync notification...");
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

    // Ensure the handle can send empty notifications.
    tracing::info!("Send empty sync notification...");
    handle1.send_sync_notification(peer2, vec![]).unwrap();
    handle2.send_sync_notification(peer1, vec![]).unwrap();

    assert_eq!(
        handle2.next().await.unwrap(),
        NotificationEvent::NotificationReceived {
            peer: peer1,
            notification: BytesMut::from(&[][..]),
        }
    );
    assert_eq!(
        handle1.next().await.unwrap(),
        NotificationEvent::NotificationReceived {
            peer: peer2,
            notification: BytesMut::from(&[][..]),
        }
    );

    // Double check non-empty notifications.
    tracing::info!("Send sync notification...");
    handle1.send_sync_notification(peer2, vec![1, 3, 3, 9]).unwrap();
    handle2.send_sync_notification(peer1, vec![1, 3, 3, 4]).unwrap();

    assert_eq!(
        handle2.next().await.unwrap(),
        NotificationEvent::NotificationReceived {
            peer: peer1,
            notification: BytesMut::from(&[1, 3, 3, 9][..]),
        }
    );
    assert_eq!(
        handle1.next().await.unwrap(),
        NotificationEvent::NotificationReceived {
            peer: peer2,
            notification: BytesMut::from(&[1, 3, 3, 4][..]),
        }
    );
}

#[tokio::test]
async fn listener_fallback_protocol_works_tcp() {
    listener_fallback_protocol_works(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn listener_fallback_protocol_works_quic() {
    listener_fallback_protocol_works(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn listener_fallback_protocol_works_websocket() {
    listener_fallback_protocol_works(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

async fn listener_fallback_protocol_works(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (notif_config1, mut handle1) = ConfigBuilder::new(ProtocolName::from("/notif/1"))
        .with_max_size(1024usize)
        .with_handshake(vec![1, 2, 3, 4])
        .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

    let (notif_config2, mut handle2) = ConfigBuilder::new(ProtocolName::from("/notif/2"))
        .with_max_size(1024usize)
        .with_handshake(vec![1, 2, 3, 4])
        .with_fallback_names(vec![ProtocolName::from("/notif/1")])
        .build();
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected and spawn the litep2p objects in the background
    connect_peers(&mut litep2p1, &mut litep2p2).await;
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
            protocol: ProtocolName::from("/notif/2"),
            fallback: Some(ProtocolName::from("/notif/1")),
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
            protocol: ProtocolName::from("/notif/2"),
            fallback: Some(ProtocolName::from("/notif/1")),
            direction: Direction::Inbound,
            peer: peer1,
            handshake: vec![1, 2, 3, 4],
        }
    );
    assert_eq!(
        handle1.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpened {
            protocol: ProtocolName::from("/notif/1"),
            direction: Direction::Outbound,
            fallback: None,
            peer: peer2,
            handshake: vec![1, 2, 3, 4],
        }
    );
}

#[tokio::test]
async fn enable_auto_accept_tcp() {
    enable_auto_accept(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn enable_auto_accept_quic() {
    enable_auto_accept(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn enable_auto_accept_websocket() {
    enable_auto_accept(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

async fn enable_auto_accept(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (notif_config1, mut handle1) = NotificationConfig::new(
        ProtocolName::from("/notif/1"),
        1024usize,
        vec![1, 2, 3, 4],
        Vec::new(),
        true,
        64,
        64,
        true,
    );
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

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
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected and spawn the litep2p objects in the background
    connect_peers(&mut litep2p1, &mut litep2p2).await;
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
            direction: Direction::Outbound,
            fallback: None,
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
}

#[tokio::test]
async fn send_using_notification_sink_tcp() {
    send_using_notification_sink(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn send_using_notification_sink_quic() {
    send_using_notification_sink(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn send_using_notification_sink_websocket() {
    send_using_notification_sink(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

async fn send_using_notification_sink(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

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
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

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
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected and spawn the litep2p objects in the background
    connect_peers(&mut litep2p1, &mut litep2p2).await;
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
            direction: Direction::Outbound,
            fallback: None,
            peer: peer2,
            handshake: vec![1, 2, 3, 4],
        }
    );

    let sink1 = handle1.notification_sink(peer2).unwrap();
    let sink2 = handle2.notification_sink(peer1).unwrap();

    sink1.send_sync_notification(vec![1, 3, 3, 7]).unwrap();
    sink2.send_sync_notification(vec![1, 3, 3, 8]).unwrap();

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

    // close the substream to `peer1` and try to send notification using `sink1`
    handle2.close_substream(peer1).await;

    // allow `peer1` to detect that the substream has been closed
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    assert_eq!(
        sink1.send_sync_notification(vec![1, 3, 3, 7]),
        Err(NotificationError::NoConnection),
    );
}

#[tokio::test]
async fn dial_peer_when_opening_substream_tcp() {
    dial_peer_when_opening_substream(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn dial_peer_when_opening_substream_quic() {
    dial_peer_when_opening_substream(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn dial_peer_when_opening_substream_websocket() {
    dial_peer_when_opening_substream(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

async fn dial_peer_when_opening_substream(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

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
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

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
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    let address = litep2p2.listen_addresses().next().unwrap().clone();
    litep2p1.add_known_address(peer2, std::iter::once(address));

    // add `peer2` known address for `peer1` and spawn the litep2p objects in the background
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
            direction: Direction::Outbound,
            fallback: None,
            peer: peer2,
            handshake: vec![1, 2, 3, 4],
        }
    );

    let sink1 = handle1.notification_sink(peer2).unwrap();
    let sink2 = handle2.notification_sink(peer1).unwrap();

    sink1.send_sync_notification(vec![1, 3, 3, 7]).unwrap();
    sink2.send_sync_notification(vec![1, 3, 3, 8]).unwrap();

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

    // close the substream to `peer1` and try to send notification using `sink1`
    handle2.close_substream(peer1).await;

    // allow `peer1` to detect that the substream has been closed
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    assert_eq!(
        sink1.send_sync_notification(vec![1, 3, 3, 7]),
        Err(NotificationError::NoConnection),
    );
}

#[tokio::test]
async fn open_and_close_batched_tcp() {
    open_and_close_batched(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn open_and_close_batched_quic() {
    open_and_close_batched(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn open_and_close_batched_websocket() {
    open_and_close_batched(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

async fn open_and_close_batched(
    transport1: Transport,
    transport2: Transport,
    transport3: Transport,
) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut litep2p1, mut handle1) = make_default_litep2p(transport1).await;
    let (mut litep2p2, mut handle2) = make_default_litep2p(transport2).await;
    let (mut litep2p3, mut handle3) = make_default_litep2p(transport3).await;

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();
    let peer3 = *litep2p3.local_peer_id();

    let address2 = litep2p2.listen_addresses().next().unwrap().clone();
    let address3 = litep2p3.listen_addresses().next().unwrap().clone();
    litep2p1.add_known_address(peer2, std::iter::once(address2));
    litep2p1.add_known_address(peer3, std::iter::once(address3));

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {},
                _ = litep2p2.next_event() => {},
                _ = litep2p3.next_event() => {},
            }
        }
    });

    // open substreams to `peer2` and `peer3`
    handle1.open_substream_batch(vec![peer3, peer2].into_iter()).await.unwrap();

    // accept for `peer2`
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

    // accept for `peer3`
    assert_eq!(
        handle3.next().await.unwrap(),
        NotificationEvent::ValidateSubstream {
            protocol: ProtocolName::from("/notif/1"),
            fallback: None,
            peer: peer1,
            handshake: vec![1, 2, 3, 4],
        }
    );
    handle3.send_validation_result(peer1, ValidationResult::Accept);

    // accept inbound substream for `peer2` and `peer3`
    let mut peer2_validated = false;
    let mut peer3_validated = false;
    let mut peer2_opened = false;
    let mut peer3_opened = false;

    while !peer2_validated || !peer3_validated || !peer2_opened || !peer3_opened {
        match handle1.next().await.unwrap() {
            NotificationEvent::ValidateSubstream {
                protocol,
                fallback,
                peer,
                handshake,
            } => {
                assert_eq!(protocol, ProtocolName::from("/notif/1"));
                assert_eq!(handshake, vec![1, 2, 3, 4]);
                assert_eq!(fallback, None);

                if peer == peer2 && !peer2_validated {
                    peer2_validated = true;
                } else if peer == peer3 && !peer3_validated {
                    peer3_validated = true;
                } else {
                    panic!("received an event from an unexpected peer");
                }

                handle1.send_validation_result(peer, ValidationResult::Accept);
            }
            NotificationEvent::NotificationStreamOpened { peer, .. } => {
                if peer == peer2 && !peer2_opened {
                    peer2_opened = true;
                } else if peer == peer3 && !peer3_opened {
                    peer3_opened = true;
                } else {
                    panic!("received an event from an unexpected peer");
                }
            }
            _ => panic!("invalid event"),
        }
    }

    // verify the substream is opened for `peer2` and `peer3`
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
        handle3.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpened {
            protocol: ProtocolName::from("/notif/1"),
            direction: Direction::Inbound,
            fallback: None,
            peer: peer1,
            handshake: vec![1, 2, 3, 4],
        }
    );

    // close substreams to `peer2` and `peer3`
    handle1.close_substream_batch(vec![peer2, peer3].into_iter()).await;

    // verify the substream is closed for `peer2` and `peer3`
    assert_eq!(
        handle2.next().await.unwrap(),
        NotificationEvent::NotificationStreamClosed { peer: peer1 }
    );
    assert_eq!(
        handle3.next().await.unwrap(),
        NotificationEvent::NotificationStreamClosed { peer: peer1 }
    );

    // verify `peer1` receives close events for both peers
    let mut peer2_closed = false;
    let mut peer3_closed = false;

    while !peer2_closed || !peer3_closed {
        match handle1.next().await.unwrap() {
            NotificationEvent::NotificationStreamClosed { peer } => {
                if peer == peer2 && !peer2_closed {
                    peer2_closed = true;
                } else if peer == peer3 && !peer3_closed {
                    peer3_closed = true;
                } else {
                    panic!("received an event from an unexpected peer");
                }
            }
            _ => panic!("invalid event"),
        }
    }
}

#[tokio::test]
async fn open_and_close_batched_duplicate_peer_tcp() {
    open_and_close_batched_duplicate_peer(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn open_and_close_batched_duplicate_peer_quic() {
    open_and_close_batched_duplicate_peer(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn open_and_close_batched_duplicate_peer_websocket() {
    open_and_close_batched_duplicate_peer(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

async fn open_and_close_batched_duplicate_peer(
    transport1: Transport,
    transport2: Transport,
    transport3: Transport,
) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut litep2p1, mut handle1) = make_default_litep2p(transport1).await;
    let (mut litep2p2, mut handle2) = make_default_litep2p(transport2).await;
    let (mut litep2p3, mut handle3) = make_default_litep2p(transport3).await;

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();
    let peer3 = *litep2p3.local_peer_id();

    let address2 = litep2p2.listen_addresses().next().unwrap().clone();
    let address3 = litep2p3.listen_addresses().next().unwrap().clone();
    litep2p1.add_known_address(peer2, std::iter::once(address2));
    litep2p1.add_known_address(peer3, std::iter::once(address3));

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {},
                _ = litep2p2.next_event() => {},
                _ = litep2p3.next_event() => {},
            }
        }
    });

    // open substream to `peer2`.
    handle1.open_substream_batch(vec![peer2].into_iter()).await.unwrap();

    // accept for `peer2`
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

    // accept inbound substream for `peer2`
    let mut peer2_validated = false;
    let mut peer2_opened = false;

    while !peer2_validated || !peer2_opened {
        match handle1.next().await.unwrap() {
            NotificationEvent::ValidateSubstream {
                protocol,
                fallback,
                peer,
                handshake,
            } => {
                assert_eq!(protocol, ProtocolName::from("/notif/1"));
                assert_eq!(handshake, vec![1, 2, 3, 4]);
                assert_eq!(fallback, None);
                assert_eq!(peer, peer2);

                if !peer2_validated {
                    peer2_validated = true;
                } else {
                    panic!("received an event from an unexpected peer");
                }

                handle1.send_validation_result(peer, ValidationResult::Accept);
            }
            NotificationEvent::NotificationStreamOpened { peer, .. } => {
                assert_eq!(peer, peer2);

                if !peer2_opened {
                    peer2_opened = true;
                } else {
                    panic!("received an event from an unexpected peer");
                }
            }
            _ => panic!("invalid event"),
        }
    }

    // verify the substream is opened for `peer2`
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

    // batch another substream open command but this time include `peer2` for which
    // a connection is already open
    match handle1.open_substream_batch(vec![peer2, peer3].into_iter()).await {
        Err(ignored) => {
            assert_eq!(ignored.len(), 1);
            assert!(ignored.contains(&peer2));
        }
        _ => panic!("call was supposed to fail"),
    }

    // accept for `peer3`
    assert_eq!(
        handle3.next().await.unwrap(),
        NotificationEvent::ValidateSubstream {
            protocol: ProtocolName::from("/notif/1"),
            fallback: None,
            peer: peer1,
            handshake: vec![1, 2, 3, 4],
        }
    );
    handle3.send_validation_result(peer1, ValidationResult::Accept);

    // accept inbound substream for `peer3`
    let mut peer3_validated = false;
    let mut peer3_opened = false;

    while !peer3_validated || !peer3_opened {
        match handle1.next().await.unwrap() {
            NotificationEvent::ValidateSubstream {
                protocol,
                fallback,
                peer,
                handshake,
            } => {
                assert_eq!(protocol, ProtocolName::from("/notif/1"));
                assert_eq!(handshake, vec![1, 2, 3, 4]);
                assert_eq!(fallback, None);
                assert_eq!(peer, peer3);

                if !peer3_validated {
                    peer3_validated = true;
                } else {
                    panic!("received an event from an unexpected peer");
                }

                handle1.send_validation_result(peer, ValidationResult::Accept);
            }
            NotificationEvent::NotificationStreamOpened { peer, .. } => {
                assert_eq!(peer, peer3);

                if !peer3_opened {
                    peer3_opened = true;
                } else {
                    panic!("received an event from an unexpected peer");
                }
            }
            _ => panic!("invalid event"),
        }
    }

    // verify the substream is opened for `peer2` and `peer3`
    assert_eq!(
        handle3.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpened {
            protocol: ProtocolName::from("/notif/1"),
            direction: Direction::Inbound,
            fallback: None,
            peer: peer1,
            handshake: vec![1, 2, 3, 4],
        }
    );

    // close substreams to `peer2` and `peer3`
    handle1.close_substream_batch(vec![peer2, peer3].into_iter()).await;

    // verify the substream is closed for `peer2` and `peer3`
    assert_eq!(
        handle2.next().await.unwrap(),
        NotificationEvent::NotificationStreamClosed { peer: peer1 }
    );
    assert_eq!(
        handle3.next().await.unwrap(),
        NotificationEvent::NotificationStreamClosed { peer: peer1 }
    );

    // verify `peer1` receives close events for both peers
    let mut peer2_closed = false;
    let mut peer3_closed = false;

    while !peer2_closed || !peer3_closed {
        match handle1.next().await.unwrap() {
            NotificationEvent::NotificationStreamClosed { peer } => {
                if peer == peer2 && !peer2_closed {
                    peer2_closed = true;
                } else if peer == peer3 && !peer3_closed {
                    peer3_closed = true;
                } else {
                    panic!("received an event from an unexpected peer");
                }
            }
            _ => panic!("invalid event"),
        }
    }
}

#[tokio::test]
async fn no_listener_address_for_one_peer_tcp() {
    no_listener_address_for_one_peer(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec![],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn no_listener_address_for_one_peer_quic() {
    no_listener_address_for_one_peer(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn no_listener_address_for_one_peer_websocket() {
    no_listener_address_for_one_peer(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec![],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
    )
    .await;
}

async fn no_listener_address_for_one_peer(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut litep2p1, mut handle1) = make_default_litep2p(transport1).await;
    let (mut litep2p2, mut handle2) = make_default_litep2p(transport2).await;

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    let address2 = litep2p2.listen_addresses().next().unwrap().clone();
    litep2p1.add_known_address(peer2, std::iter::once(address2));

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {},
                _ = litep2p2.next_event() => {},
            }
        }
    });

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
}

#[tokio::test]
async fn auto_accept_inbound_tcp() {
    auto_accept_inbound(
        Transport::Tcp(Default::default()),
        Transport::Tcp(Default::default()),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn auto_accept_inbound_quic() {
    auto_accept_inbound(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn auto_accept_inbound_websocket() {
    auto_accept_inbound(
        Transport::WebSocket(Default::default()),
        Transport::WebSocket(Default::default()),
    )
    .await;
}

async fn auto_accept_inbound(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (notif_config1, mut handle1) = ConfigBuilder::new(ProtocolName::from("/notif/1"))
        .with_max_size(1024usize)
        .with_handshake(vec![1, 2, 3, 4])
        .with_auto_accept_inbound(true)
        .with_sync_channel_size(1024usize)
        .with_async_channel_size(1024usize)
        .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

    let (mut notif_config2, mut handle2) = ConfigBuilder::new(ProtocolName::from("/notif/1"))
        .with_max_size(1024usize)
        .with_handshake(vec![1, 2, 3, 4])
        .with_auto_accept_inbound(true)
        .with_sync_channel_size(1024usize)
        .with_async_channel_size(1024usize)
        .build();

    // set new handshake for the config
    notif_config2.set_handshake(vec![1, 3, 3, 7]);

    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected and spawn the litep2p objects in the background
    connect_peers(&mut litep2p1, &mut litep2p2).await;
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
            handshake: vec![1, 3, 3, 7],
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
}
#[tokio::test]
async fn dial_failure_tcp() {
    dial_failure(
        Transport::Tcp(Default::default()),
        Transport::Tcp(Default::default()),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn dial_failure_quic() {
    dial_failure(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn dial_failure_websocket() {
    dial_failure(
        Transport::WebSocket(Default::default()),
        Transport::WebSocket(Default::default()),
    )
    .await;
}

async fn dial_failure(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (notif_config1, mut handle1) = ConfigBuilder::new(ProtocolName::from("/notif/1"))
        .with_max_size(1024usize)
        .with_handshake(vec![1, 2, 3, 4])
        .with_auto_accept_inbound(true)
        .with_sync_channel_size(1024usize)
        .with_async_channel_size(1024usize)
        .build();
    let (notif_config2, mut handle2) = ConfigBuilder::new(ProtocolName::from("/notif/2"))
        .with_max_size(1024usize)
        .with_handshake(vec![7, 7, 7, 7])
        .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1)
        .with_notification_protocol(notif_config2);

    let config1 = add_transport(config1, transport1).build();

    let (notif_config3, _handle3) = ConfigBuilder::new(ProtocolName::from("/notif/1"))
        .with_max_size(1024usize)
        .with_handshake(vec![1, 2, 3, 4])
        .with_auto_accept_inbound(true)
        .with_sync_channel_size(1024usize)
        .with_async_channel_size(1024usize)
        .build();

    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config3);

    let known_address = match &transport2 {
        Transport::Tcp(_) => Multiaddr::empty()
            .with(Protocol::Ip6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)))
            .with(Protocol::Tcp(5)),
        #[cfg(feature = "quic")]
        Transport::Quic(_) => Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Udp(5))
            .with(Protocol::QuicV1),
        #[cfg(feature = "websocket")]
        Transport::WebSocket(_) => Multiaddr::empty()
            .with(Protocol::Ip6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)))
            .with(Protocol::Tcp(5))
            .with(Protocol::Ws(std::borrow::Cow::Owned("/".to_string()))),
    };

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer2 = *litep2p2.local_peer_id();
    let known_address = known_address.with(Protocol::P2p(Multihash::from(peer2)));

    litep2p1.add_known_address(peer2, vec![known_address].into_iter());

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
        handle1.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpenFailure {
            peer: peer2,
            error: NotificationError::DialFailure,
        }
    );

    futures::future::poll_fn(|cx| match handle2.poll_next_unpin(cx) {
        Poll::Pending => Poll::Ready(()),
        _ => panic!("invalid event"),
    })
    .await;
}

#[tokio::test]
async fn dialing_disabled_tcp() {
    dialing_disabled(
        Transport::Tcp(Default::default()),
        Transport::Tcp(Default::default()),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn dialing_disabled_quic() {
    dialing_disabled(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn dialing_disabled_websocket() {
    dialing_disabled(
        Transport::WebSocket(Default::default()),
        Transport::WebSocket(Default::default()),
    )
    .await;
}

async fn dialing_disabled(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (notif_config1, mut handle1) = ConfigBuilder::new(ProtocolName::from("/notif/1"))
        .with_max_size(1024usize)
        .with_handshake(vec![1, 2, 3, 4])
        .with_auto_accept_inbound(true)
        .with_sync_channel_size(1024usize)
        .with_async_channel_size(1024usize)
        .with_dialing_enabled(false)
        .build();
    let (notif_config2, mut handle2) = ConfigBuilder::new(ProtocolName::from("/notif/2"))
        .with_max_size(1024usize)
        .with_handshake(vec![7, 7, 7, 7])
        .with_dialing_enabled(false)
        .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1)
        .with_notification_protocol(notif_config2);

    let config1 = add_transport(config1, transport1).build();

    let (notif_config3, _handle3) = ConfigBuilder::new(ProtocolName::from("/notif/1"))
        .with_max_size(1024usize)
        .with_handshake(vec![1, 2, 3, 4])
        .with_auto_accept_inbound(true)
        .with_sync_channel_size(1024usize)
        .with_async_channel_size(1024usize)
        .build();

    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config3);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer2 = *litep2p2.local_peer_id();
    let listen_address = litep2p2.listen_addresses().next().unwrap().clone();

    litep2p1.add_known_address(peer2, vec![listen_address].into_iter());

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
        handle1.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpenFailure {
            peer: peer2,
            error: NotificationError::DialFailure,
        }
    );

    futures::future::poll_fn(|cx| match handle2.poll_next_unpin(cx) {
        Poll::Pending => Poll::Ready(()),
        _ => panic!("invalid event"),
    })
    .await;
}

#[tokio::test]
async fn validation_takes_too_long_tcp() {
    validation_takes_too_long(
        Transport::Tcp(Default::default()),
        Transport::Tcp(Default::default()),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn validation_takes_too_long_quic() {
    validation_takes_too_long(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn validation_takes_too_long_websocket() {
    validation_takes_too_long(
        Transport::WebSocket(Default::default()),
        Transport::WebSocket(Default::default()),
    )
    .await;
}

async fn validation_takes_too_long(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (notif_config1, mut handle1) = ConfigBuilder::new(ProtocolName::from("/notif/1"))
        .with_max_size(1024usize)
        .with_handshake(vec![1, 2, 3, 4])
        .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

    let (notif_config3, mut handle2) = ConfigBuilder::new(ProtocolName::from("/notif/1"))
        .with_max_size(1024usize)
        .with_handshake(vec![1, 2, 3, 4])
        .build();

    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config3);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();
    let listen_address = litep2p2.listen_addresses().next().unwrap().clone();

    litep2p1.add_known_address(peer2, vec![listen_address].into_iter());

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

    assert_eq!(
        handle1.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpenFailure {
            peer: peer2,
            error: NotificationError::Rejected,
        }
    );

    // give theh connection a moment to close
    tokio::time::sleep(Duration::from_secs(5)).await;

    handle2.send_validation_result(peer1, ValidationResult::Accept);
    assert_eq!(
        handle2.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpenFailure {
            peer: peer1,
            error: NotificationError::NoConnection,
        }
    );
}

#[tokio::test]
async fn ignored_validation_open_substream_tcp() {
    ignored_validation_open_substream(
        Transport::Tcp(Default::default()),
        Transport::Tcp(Default::default()),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn ignored_validation_open_substream_quic() {
    ignored_validation_open_substream(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn ignored_validation_open_substream_websocket() {
    ignored_validation_open_substream(
        Transport::WebSocket(Default::default()),
        Transport::WebSocket(Default::default()),
    )
    .await;
}

async fn ignored_validation_open_substream(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (notif_config1, mut handle1) = ConfigBuilder::new(ProtocolName::from("/notif/1"))
        .with_max_size(1024usize)
        .with_handshake(vec![1, 2, 3, 4])
        .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

    let (notif_config3, mut handle2) = ConfigBuilder::new(ProtocolName::from("/notif/1"))
        .with_max_size(1024usize)
        .with_handshake(vec![1, 2, 3, 4])
        .build();

    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config3);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();
    let listen_address = litep2p2.listen_addresses().next().unwrap().clone();

    litep2p1.add_known_address(peer2, vec![listen_address].into_iter());

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
    assert_eq!(
        handle1.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpenFailure {
            peer: peer2,
            error: NotificationError::Rejected,
        }
    );

    // wait a moment to allow the connection to close
    tokio::time::sleep(Duration::from_secs(2)).await;

    // verify that there are no events pending
    futures::future::poll_fn(|cx| match handle2.poll_next_unpin(cx) {
        Poll::Pending => Poll::Ready(()),
        event => panic!("invalid event: {event:?}"),
    })
    .await;

    // try to open a substream while the previous validation is still in progress
    // and verify that the substream is rejected with `ValidationPending`
    handle2.open_substream(peer1).await.unwrap();
    assert_eq!(
        handle2.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpenFailure {
            peer: peer1,
            error: NotificationError::ValidationPending,
        }
    );

    // try to open substream as `peer1` and verify the inbound substream gets rejected
    // because the previous substream is still pending
    handle1.open_substream(peer2).await.unwrap();
    assert_eq!(
        handle1.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpenFailure {
            peer: peer2,
            error: NotificationError::Rejected,
        }
    );

    // verify `peer2` is not notified of the new substream
    futures::future::poll_fn(|cx| match handle2.poll_next_unpin(cx) {
        Poll::Pending => Poll::Ready(()),
        event => panic!("invalid event: {event:?}"),
    })
    .await;

    // finally try to accept the original substream and verify it fails to open with `NoConnection`
    handle2.send_validation_result(peer1, ValidationResult::Accept);
    assert_eq!(
        handle2.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpenFailure {
            peer: peer1,
            error: NotificationError::Rejected,
        }
    );
}

#[tokio::test]
async fn clogged_channel_disconnects_peer_tcp() {
    clogged_channel_disconnects_peer(
        Transport::Tcp(Default::default()),
        Transport::Tcp(Default::default()),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn clogged_channel_disconnects_peer_quic() {
    clogged_channel_disconnects_peer(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn clogged_channel_disconnects_peer_websocket() {
    clogged_channel_disconnects_peer(
        Transport::WebSocket(Default::default()),
        Transport::WebSocket(Default::default()),
    )
    .await;
}

async fn clogged_channel_disconnects_peer(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (notif_config1, mut handle1) = ConfigBuilder::new(ProtocolName::from("/notif/1"))
        .with_max_size(100 * 1024)
        .with_handshake(vec![1, 2, 3, 4])
        .with_auto_accept_inbound(true)
        .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config1);

    let config1 = add_transport(config1, transport1).build();

    let (notif_config3, mut handle2) = ConfigBuilder::new(ProtocolName::from("/notif/1"))
        .with_max_size(100 * 1024)
        .with_handshake(vec![1, 2, 3, 4])
        .with_auto_accept_inbound(true)
        .build();

    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_notification_protocol(notif_config3);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();
    let listen_address = litep2p2.listen_addresses().next().unwrap().clone();

    litep2p1.add_known_address(peer2, vec![listen_address].into_iter());

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

    // verify both peers have the substream open
    assert_eq!(
        handle1.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpened {
            protocol: ProtocolName::from("/notif/1"),
            fallback: None,
            peer: peer2,
            handshake: vec![1, 2, 3, 4],
            direction: Direction::Outbound,
        }
    );
    assert_eq!(
        handle2.next().await.unwrap(),
        NotificationEvent::NotificationStreamOpened {
            protocol: ProtocolName::from("/notif/1"),
            fallback: None,
            peer: peer1,
            handshake: vec![1, 2, 3, 4],
            direction: Direction::Inbound,
        }
    );

    // start sending notifications to `peer2` which never reads them,
    // causing `peer1` to consume all available credit
    loop {
        match handle1.send_sync_notification(peer2, vec![0u8; 99 * 1024]) {
            Ok(()) => {}
            Err(NotificationError::ChannelClogged) => break,
            error => panic!("invalid error: {error:?}"),
        }
    }

    // stream closed from `peer1`'s PoV
    assert_eq!(
        handle1.next().await.unwrap(),
        NotificationEvent::NotificationStreamClosed { peer: peer2 },
    );

    // `peer2` is also reported that the substream is closed
    if let Err(_) = tokio::time::timeout(Duration::from_secs(5), async move {
        loop {
            if let Some(NotificationEvent::NotificationStreamClosed { peer }) = handle2.next().await
            {
                assert_eq!(peer, peer1);
                break;
            }
        }
    })
    .await
    {
        panic!("timeout")
    }
}
