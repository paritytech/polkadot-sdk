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
    error::{DialError, Error, NegotiationError},
    protocol::libp2p::ping::{Config as PingConfig, PingEvent},
    transport::tcp::config::Config as TcpConfig,
    Litep2p, Litep2pEvent, PeerId,
};

#[cfg(feature = "websocket")]
use litep2p::transport::websocket::config::Config as WebSocketConfig;
#[cfg(feature = "quic")]
use litep2p::{error::AddressError, transport::quic::config::Config as QuicConfig};

use futures::{Stream, StreamExt};
use multiaddr::{Multiaddr, Protocol};
use multihash::Multihash;
use network_interface::{NetworkInterface, NetworkInterfaceConfig};
use tokio::net::TcpListener;
#[cfg(feature = "quic")]
use tokio::net::UdpSocket;

use crate::common::{add_transport, Transport};

#[cfg(feature = "websocket")]
use std::collections::HashSet;

#[cfg(test)]
mod protocol_dial_invalid_address;
#[cfg(test)]
mod stability;

#[tokio::test]
async fn two_litep2ps_work_tcp() {
    two_litep2ps_work(
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
async fn two_litep2ps_work_quic() {
    two_litep2ps_work(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn two_litep2ps_work_websocket() {
    two_litep2ps_work(
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

async fn two_litep2ps_work(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (ping_config1, _ping_event_stream1) = PingConfig::default();
    let config1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_libp2p_ping(ping_config1);

    let config1 = add_transport(config1, transport1).build();

    let (ping_config2, _ping_event_stream2) = PingConfig::default();
    let config2 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_libp2p_ping(ping_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let address = litep2p2.listen_addresses().next().unwrap().clone();
    litep2p1.dial_address(address).await.unwrap();

    let (res1, res2) = tokio::join!(litep2p1.next_event(), litep2p2.next_event());

    assert!(std::matches!(
        res1,
        Some(Litep2pEvent::ConnectionEstablished { .. })
    ));
    assert!(std::matches!(
        res2,
        Some(Litep2pEvent::ConnectionEstablished { .. })
    ));
}

#[tokio::test]
async fn dial_failure_tcp() {
    dial_failure(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        Multiaddr::empty()
            .with(Protocol::Ip6(std::net::Ipv6Addr::new(
                0, 0, 0, 0, 0, 0, 0, 1,
            )))
            .with(Protocol::Tcp(1)),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn dial_failure_quic() {
    dial_failure(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
        Multiaddr::empty()
            .with(Protocol::Ip6(std::net::Ipv6Addr::new(
                0, 0, 0, 0, 0, 0, 0, 1,
            )))
            .with(Protocol::Udp(1))
            .with(Protocol::QuicV1),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn dial_failure_websocket() {
    dial_failure(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        Multiaddr::empty()
            .with(Protocol::Ip6(std::net::Ipv6Addr::new(
                0, 0, 0, 0, 0, 0, 0, 1,
            )))
            .with(Protocol::Tcp(1))
            .with(Protocol::Ws(std::borrow::Cow::Owned("/".to_string()))),
    )
    .await;
}

async fn dial_failure(transport1: Transport, transport2: Transport, dial_address: Multiaddr) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (ping_config1, _ping_event_stream1) = PingConfig::default();
    let config1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_libp2p_ping(ping_config1);

    let config1 = add_transport(config1, transport1).build();

    let (ping_config2, _ping_event_stream2) = PingConfig::default();
    let config2 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_libp2p_ping(ping_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let address = dial_address.with(Protocol::P2p(
        Multihash::from_bytes(&litep2p2.local_peer_id().to_bytes()).unwrap(),
    ));

    litep2p1.dial_address(address).await.unwrap();

    tokio::spawn(async move {
        loop {
            let _ = litep2p2.next_event().await;
        }
    });

    assert!(std::matches!(
        litep2p1.next_event().await,
        Some(Litep2pEvent::DialFailure { .. })
    ));
}

#[tokio::test]
async fn connect_over_dns() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let keypair1 = Keypair::generate();
    let (ping_config1, _ping_event_stream1) = PingConfig::default();

    let config1 = ConfigBuilder::new()
        .with_keypair(keypair1)
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_ping(ping_config1)
        .build();

    let keypair2 = Keypair::generate();
    let (ping_config2, _ping_event_stream2) = PingConfig::default();

    let config2 = ConfigBuilder::new()
        .with_keypair(keypair2)
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_ping(ping_config2)
        .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();
    let peer2 = *litep2p2.local_peer_id();

    let address = litep2p2.listen_addresses().next().unwrap().clone();
    let tcp = address.iter().nth(1).unwrap();

    let mut new_address = Multiaddr::empty();
    new_address.push(Protocol::Dns("localhost".into()));
    new_address.push(tcp);
    new_address.push(Protocol::P2p(
        Multihash::from_bytes(&peer2.to_bytes()).unwrap(),
    ));

    litep2p1.dial_address(new_address).await.unwrap();
    let (res1, res2) = tokio::join!(litep2p1.next_event(), litep2p2.next_event());

    assert!(std::matches!(
        res1,
        Some(Litep2pEvent::ConnectionEstablished { .. })
    ));
    assert!(std::matches!(
        res2,
        Some(Litep2pEvent::ConnectionEstablished { .. })
    ));
}

#[tokio::test]
async fn connection_timeout_tcp() {
    // create tcp listener but don't accept any inbound connections
    let listener = TcpListener::bind("[::1]:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let address = Multiaddr::empty()
        .with(Protocol::from(address.ip()))
        .with(Protocol::Tcp(address.port()))
        .with(Protocol::P2p(
            Multihash::from_bytes(&PeerId::random().to_bytes()).unwrap(),
        ));

    connection_timeout(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        }),
        address,
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn connection_timeout_quic() {
    // create udp socket but don't respond to any inbound datagrams
    let listener = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let address = Multiaddr::empty()
        .with(Protocol::from(address.ip()))
        .with(Protocol::Udp(address.port()))
        .with(Protocol::QuicV1)
        .with(Protocol::P2p(
            Multihash::from_bytes(&PeerId::random().to_bytes()).unwrap(),
        ));

    connection_timeout(Transport::Quic(Default::default()), address).await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn connection_timeout_websocket() {
    // create tcp listener but don't accept any inbound connections
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let address = Multiaddr::empty()
        .with(Protocol::from(address.ip()))
        .with(Protocol::Tcp(address.port()))
        .with(Protocol::Ws(std::borrow::Cow::Owned("/".to_string())))
        .with(Protocol::P2p(
            Multihash::from_bytes(&PeerId::random().to_bytes()).unwrap(),
        ));

    connection_timeout(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        }),
        address,
    )
    .await;
}

async fn connection_timeout(transport: Transport, address: Multiaddr) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (ping_config, _ping_event_stream) = PingConfig::default();
    let litep2p_config = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_libp2p_ping(ping_config);
    let litep2p_config = add_transport(litep2p_config, transport).build();

    let mut litep2p = Litep2p::new(litep2p_config).unwrap();

    litep2p.dial_address(address.clone()).await.unwrap();

    let Some(Litep2pEvent::DialFailure {
        address: dial_address,
        error,
    }) = litep2p.next_event().await
    else {
        panic!("invalid event received");
    };

    assert_eq!(dial_address, address);
    println!("{error:?}");
    match error {
        DialError::Timeout => {}
        DialError::NegotiationError(NegotiationError::Timeout) => {}
        _ => panic!("unexpected error {error:?}"),
    }
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn dial_quic_peer_id_missing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (ping_config, _ping_event_stream) = PingConfig::default();
    let config = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_quic(Default::default())
        .with_libp2p_ping(ping_config)
        .build();

    let mut litep2p = Litep2p::new(config).unwrap();

    // create udp socket but don't respond to any inbound datagrams
    let listener = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let address = Multiaddr::empty()
        .with(Protocol::from(address.ip()))
        .with(Protocol::Udp(address.port()))
        .with(Protocol::QuicV1);

    match litep2p.dial_address(address.clone()).await {
        Err(Error::AddressError(AddressError::PeerIdMissing)) => {}
        state => panic!("dial not supposed to succeed {state:?}"),
    }
}

#[tokio::test]
async fn dial_self_tcp() {
    dial_self(Transport::Tcp(TcpConfig {
        listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
        ..Default::default()
    }))
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn dial_self_quic() {
    dial_self(Transport::Quic(Default::default())).await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn dial_self_websocket() {
    dial_self(Transport::WebSocket(WebSocketConfig {
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
        ..Default::default()
    }))
    .await;
}

async fn dial_self(transport: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (ping_config, _ping_event_stream) = PingConfig::default();
    let litep2p_config = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_libp2p_ping(ping_config);
    let litep2p_config = add_transport(litep2p_config, transport).build();

    let mut litep2p = Litep2p::new(litep2p_config).unwrap();
    let address = litep2p.listen_addresses().next().unwrap().clone();

    // dial without peer id attached
    assert!(std::matches!(
        litep2p.dial_address(address.clone()).await,
        Err(Error::TriedToDialSelf)
    ));
}

#[tokio::test]
async fn attempt_to_dial_using_unsupported_transport_tcp() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (ping_config, _ping_event_stream) = PingConfig::default();
    let config = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_tcp(Default::default())
        .with_libp2p_ping(ping_config)
        .build();

    let mut litep2p = Litep2p::new(config).unwrap();
    let address = Multiaddr::empty()
        .with(Protocol::from(std::net::Ipv4Addr::new(127, 0, 0, 1)))
        .with(Protocol::Tcp(8888))
        .with(Protocol::Ws(std::borrow::Cow::Borrowed("/")))
        .with(Protocol::P2p(
            Multihash::from_bytes(&PeerId::random().to_bytes()).unwrap(),
        ));

    assert!(std::matches!(
        litep2p.dial_address(address.clone()).await,
        Err(Error::TransportNotSupported(_))
    ));
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn attempt_to_dial_using_unsupported_transport_quic() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (ping_config, _ping_event_stream) = PingConfig::default();
    let config = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_quic(Default::default())
        .with_libp2p_ping(ping_config)
        .build();

    let mut litep2p = Litep2p::new(config).unwrap();
    let address = Multiaddr::empty()
        .with(Protocol::from(std::net::Ipv4Addr::new(127, 0, 0, 1)))
        .with(Protocol::Tcp(8888))
        .with(Protocol::P2p(
            Multihash::from_bytes(&PeerId::random().to_bytes()).unwrap(),
        ));

    assert!(std::matches!(
        litep2p.dial_address(address.clone()).await,
        Err(Error::TransportNotSupported(_))
    ));
}

#[tokio::test]
async fn keep_alive_timeout_tcp() {
    keep_alive_timeout(
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
async fn keep_alive_timeout_quic() {
    keep_alive_timeout(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn keep_alive_timeout_websocket() {
    keep_alive_timeout(
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

async fn keep_alive_timeout(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (ping_config1, mut ping_event_stream1) = PingConfig::default();
    let config1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_libp2p_ping(ping_config1);

    let config1 = add_transport(config1, transport1).build();
    let mut litep2p1 = Litep2p::new(config1).unwrap();

    let (ping_config2, mut ping_event_stream2) = PingConfig::default();
    let config2 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_libp2p_ping(ping_config2);

    let config2 = add_transport(config2, transport2).build();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let address1 = litep2p1.listen_addresses().next().unwrap().clone();
    litep2p2.dial_address(address1).await.unwrap();
    let mut litep2p1_ping = false;
    let mut litep2p2_ping = false;

    loop {
        tokio::select! {
            event = litep2p1.next_event() => match event {
                Some(Litep2pEvent::ConnectionClosed { .. }) if litep2p1_ping || litep2p2_ping => {
                    break;
                }
                _ => {}
            },
            event = litep2p2.next_event() => match event {
                Some(Litep2pEvent::ConnectionClosed { .. }) if litep2p1_ping || litep2p2_ping => {
                    break;
                }
                _ => {}
            },
            _event = ping_event_stream1.next() => {
                tracing::warn!("ping1 received");
                litep2p1_ping = true;
            }
            _event = ping_event_stream2.next() => {
                tracing::warn!("ping2 received");
                litep2p2_ping = true;
            }
        }
    }
}

#[tokio::test]
async fn simultaneous_dial_tcp() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (ping_config1, mut ping_event_stream1) = PingConfig::default();
    let config1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_ping(ping_config1)
        .build();
    let mut litep2p1 = Litep2p::new(config1).unwrap();

    let (ping_config2, mut ping_event_stream2) = PingConfig::default();
    let config2 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_ping(ping_config2)
        .build();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let address1 = litep2p1.listen_addresses().next().unwrap().clone();
    let address2 = litep2p2.listen_addresses().next().unwrap().clone();

    let (res1, res2) = tokio::join!(
        litep2p1.dial_address(address2),
        litep2p2.dial_address(address1)
    );
    assert!(std::matches!((res1, res2), (Ok(()), Ok(()))));

    let mut ping_received1 = false;
    let mut ping_received2 = false;

    while !ping_received1 || !ping_received2 {
        tokio::select! {
            _ = litep2p1.next_event() => {}
            _ = litep2p2.next_event() => {}
            event = ping_event_stream1.next() => {
                if event.is_some() {
                    ping_received1 = true;
                }
            }
            event = ping_event_stream2.next() => {
                if event.is_some() {
                    ping_received2 = true;
                }
            }
        }
    }
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn simultaneous_dial_quic() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (ping_config1, mut ping_event_stream1) = PingConfig::default();
    let config1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_quic(Default::default())
        .with_libp2p_ping(ping_config1)
        .build();
    let mut litep2p1 = Litep2p::new(config1).unwrap();

    let (ping_config2, mut ping_event_stream2) = PingConfig::default();
    let config2 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_quic(Default::default())
        .with_libp2p_ping(ping_config2)
        .build();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let address1 = litep2p1.listen_addresses().next().unwrap().clone();
    let address2 = litep2p2.listen_addresses().next().unwrap().clone();

    let (res1, res2) = tokio::join!(
        litep2p1.dial_address(address2),
        litep2p2.dial_address(address1)
    );
    assert!(std::matches!((res1, res2), (Ok(()), Ok(()))));

    let mut ping_received1 = false;
    let mut ping_received2 = false;

    while !ping_received1 || !ping_received2 {
        tokio::select! {
            _ = litep2p1.next_event() => {}
            _ = litep2p2.next_event() => {}
            event = ping_event_stream1.next() => {
                if event.is_some() {
                    ping_received1 = true;
                }
            }
            event = ping_event_stream2.next() => {
                if event.is_some() {
                    ping_received2 = true;
                }
            }
        }
    }
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn simultaneous_dial_ipv6_quic() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (ping_config1, mut ping_event_stream1) = PingConfig::default();
    let config1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_quic(Default::default())
        .with_libp2p_ping(ping_config1)
        .build();
    let mut litep2p1 = Litep2p::new(config1).unwrap();

    let (ping_config2, mut ping_event_stream2) = PingConfig::default();
    let config2 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_quic(Default::default())
        .with_libp2p_ping(ping_config2)
        .build();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let address1 = litep2p1.listen_addresses().next().unwrap().clone();
    let address2 = litep2p2.listen_addresses().next().unwrap().clone();

    let (res1, res2) = tokio::join!(
        litep2p1.dial_address(address2),
        litep2p2.dial_address(address1)
    );
    assert!(std::matches!((res1, res2), (Ok(()), Ok(()))));

    let mut ping_received1 = false;
    let mut ping_received2 = false;

    while !ping_received1 || !ping_received2 {
        tokio::select! {
            _ = litep2p1.next_event() => {}
            _ = litep2p2.next_event() => {}
            event = ping_event_stream1.next() => {
                if event.is_some() {
                    ping_received1 = true;
                }
            }
            event = ping_event_stream2.next() => {
                if event.is_some() {
                    ping_received2 = true;
                }
            }
        }
    }
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn websocket_over_ipv6() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (ping_config1, mut ping_event_stream1) = PingConfig::default();
    let config1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_websocket(WebSocketConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_ping(ping_config1)
        .build();
    let mut litep2p1 = Litep2p::new(config1).unwrap();

    let (ping_config2, mut ping_event_stream2) = PingConfig::default();
    let config2 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_websocket(WebSocketConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_ping(ping_config2)
        .build();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let address2 = litep2p2.listen_addresses().next().unwrap().clone();
    litep2p1.dial_address(address2).await.unwrap();

    let mut ping_received1 = false;
    let mut ping_received2 = false;

    while !ping_received1 || !ping_received2 {
        tokio::select! {
            _ = litep2p1.next_event() => {}
            _ = litep2p2.next_event() => {}
            event = ping_event_stream1.next() => {
                if event.is_some() {
                    ping_received1 = true;
                }
            }
            event = ping_event_stream2.next() => {
                if event.is_some() {
                    ping_received2 = true;
                }
            }
        }
    }
}

#[tokio::test]
async fn tcp_dns_resolution() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (ping_config1, mut ping_event_stream1) = PingConfig::default();
    let config1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_ping(ping_config1)
        .build();
    let mut litep2p1 = Litep2p::new(config1).unwrap();

    let (ping_config2, mut ping_event_stream2) = PingConfig::default();
    let config2 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_ping(ping_config2)
        .build();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let address = litep2p2.listen_addresses().next().unwrap().clone();
    let tcp = address.iter().nth(1).unwrap();
    let peer2 = *litep2p2.local_peer_id();

    let mut new_address = Multiaddr::empty();
    new_address.push(Protocol::Dns("localhost".into()));
    new_address.push(tcp);
    new_address.push(Protocol::P2p(
        Multihash::from_bytes(&peer2.to_bytes()).unwrap(),
    ));
    litep2p1.dial_address(new_address).await.unwrap();

    let mut ping_received1 = false;
    let mut ping_received2 = false;

    while !ping_received1 || !ping_received2 {
        tokio::select! {
            _ = litep2p1.next_event() => {}
            _ = litep2p2.next_event() => {}
            event = ping_event_stream1.next() => {
                if event.is_some() {
                    ping_received1 = true;
                }
            }
            event = ping_event_stream2.next() => {
                if event.is_some() {
                    ping_received2 = true;
                }
            }
        }
    }
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn websocket_dns_resolution() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (ping_config1, mut ping_event_stream1) = PingConfig::default();
    let config1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_websocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_ping(ping_config1)
        .build();
    let mut litep2p1 = Litep2p::new(config1).unwrap();

    let (ping_config2, mut ping_event_stream2) = PingConfig::default();
    let config2 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_websocket(WebSocketConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        })
        .with_libp2p_ping(ping_config2)
        .build();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let address = litep2p2.listen_addresses().next().unwrap().clone();
    let tcp = address.iter().nth(1).unwrap();
    let peer2 = *litep2p2.local_peer_id();

    let mut new_address = Multiaddr::empty();
    new_address.push(Protocol::Dns("localhost".into()));
    new_address.push(tcp);
    new_address.push(Protocol::Ws(std::borrow::Cow::Owned("/".to_string())));
    new_address.push(Protocol::P2p(
        Multihash::from_bytes(&peer2.to_bytes()).unwrap(),
    ));
    litep2p1.dial_address(new_address).await.unwrap();

    let mut ping_received1 = false;
    let mut ping_received2 = false;

    while !ping_received1 || !ping_received2 {
        tokio::select! {
            _ = litep2p1.next_event() => {}
            _ = litep2p2.next_event() => {}
            event = ping_event_stream1.next() => {
                if event.is_some() {
                    ping_received1 = true;
                }
            }
            event = ping_event_stream2.next() => {
                if event.is_some() {
                    ping_received2 = true;
                }
            }
        }
    }
}

#[tokio::test]
async fn multiple_listen_addresses_tcp() {
    multiple_listen_addresses(
        Transport::Tcp(TcpConfig {
            listen_addresses: vec![
                "/ip6/::1/tcp/0".parse().unwrap(),
                "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            ],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec![],
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            listen_addresses: vec![],
            ..Default::default()
        }),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn multiple_listen_addresses_quic() {
    multiple_listen_addresses(
        Transport::Quic(QuicConfig {
            listen_addresses: vec![
                "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap(),
                "/ip6/::1/udp/0/quic-v1".parse().unwrap(),
            ],
            ..Default::default()
        }),
        Transport::Quic(QuicConfig {
            listen_addresses: vec![],
            ..Default::default()
        }),
        Transport::Quic(QuicConfig {
            listen_addresses: vec![],
            ..Default::default()
        }),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn multiple_listen_addresses_websocket() {
    multiple_listen_addresses(
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec![
                "/ip4/127.0.0.1/tcp/0/ws".parse().unwrap(),
                "/ip6/::1/tcp/0/ws".parse().unwrap(),
            ],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec![],
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            listen_addresses: vec![],
            ..Default::default()
        }),
    )
    .await;
}

async fn make_dummy_litep2p(
    transport: Transport,
) -> (Litep2p, Box<dyn Stream<Item = PingEvent> + Send + Unpin>) {
    let (ping_config, ping_event_stream) = PingConfig::default();
    let litep2p_config = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_libp2p_ping(ping_config);

    let litep2p_config = match transport {
        Transport::Tcp(config) => litep2p_config.with_tcp(config),
        #[cfg(feature = "quic")]
        Transport::Quic(config) => litep2p_config.with_quic(config),
        #[cfg(feature = "websocket")]
        Transport::WebSocket(config) => litep2p_config.with_websocket(config),
    }
    .build();

    (Litep2p::new(litep2p_config).unwrap(), ping_event_stream)
}

async fn multiple_listen_addresses(
    transport1: Transport,
    transport2: Transport,
    transport3: Transport,
) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut litep2p1, _event_stream) = make_dummy_litep2p(transport1).await;
    let (mut litep2p2, _event_stream) = make_dummy_litep2p(transport2).await;
    let (mut litep2p3, _event_stream) = make_dummy_litep2p(transport3).await;

    let addresses: Vec<_> = litep2p1.listen_addresses().cloned().collect();
    let address1 = addresses.first().unwrap().clone();
    let address2 = addresses.get(1).unwrap().clone();

    tokio::spawn(async move {
        loop {
            let _ = litep2p1.next_event().await;
        }
    });

    let (res1, res2) = tokio::join!(
        litep2p2.dial_address(address1),
        litep2p3.dial_address(address2),
    );
    assert!(res1.is_ok() && res2.is_ok());

    let (res1, res2) = tokio::join!(litep2p2.next_event(), litep2p3.next_event());

    assert!(std::matches!(
        res1,
        Some(Litep2pEvent::ConnectionEstablished { .. })
    ));
    assert!(std::matches!(
        res2,
        Some(Litep2pEvent::ConnectionEstablished { .. })
    ));
}

#[tokio::test]
async fn port_in_use_tcp() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let listener = TcpListener::bind("[::1]:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    let _litep2p = Litep2p::new(
        ConfigBuilder::new()
            .with_tcp(TcpConfig {
                listen_addresses: vec![Multiaddr::empty()
                    .with(Protocol::from(address.ip()))
                    .with(Protocol::Tcp(address.port()))],
                ..Default::default()
            })
            .build(),
    )
    .unwrap();
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn port_in_use_websocket() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let listener = TcpListener::bind("[::1]:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    let _litep2p = Litep2p::new(
        ConfigBuilder::new()
            .with_websocket(WebSocketConfig {
                listen_addresses: vec![Multiaddr::empty()
                    .with(Protocol::from(address.ip()))
                    .with(Protocol::Tcp(address.port()))
                    .with(Protocol::Ws(std::borrow::Cow::Owned("/".to_string())))],
                ..Default::default()
            })
            .build(),
    )
    .unwrap();
}

#[tokio::test]
async fn dial_over_multiple_addresses() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    // let (mut litep2p1, _event_stream) = make_dummy_litep2p(transport1).await;
    // let (mut litep2p2, _event_stream) = make_dummy_litep2p(transport2).await;
    // let (mut litep2p3, _event_stream) = make_dummy_litep2p(transport3).await;

    // let mut address_iter = litep2p1.listen_addresses();
    // let address1 = address_iter.next().unwrap().clone();
    // let address2 = address_iter.next().unwrap().clone();
    // drop(address_iter);

    // tokio::spawn(async move {
    //     loop {
    //         let _ = litep2p1.next_event().await;
    //     }
    // });

    // let (res1, res2) = tokio::join!(
    //     litep2p2.dial_address(address1),
    //     litep2p3.dial_address(address2),
    // );
    // assert!(res1.is_ok() && res2.is_ok());

    // let (res1, res2) = tokio::join!(litep2p2.next_event(), litep2p3.next_event());

    // assert!(std::matches!(
    //     res1,
    //     Some(Litep2pEvent::ConnectionEstablished { .. })
    // ));
    // assert!(std::matches!(
    //     res2,
    //     Some(Litep2pEvent::ConnectionEstablished { .. })
    // ));
}

#[tokio::test]
async fn unspecified_listen_address_tcp() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (ping_config1, _ping_event_stream1) = PingConfig::default();
    let config1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_tcp(TcpConfig {
            listen_addresses: vec![
                "/ip4/0.0.0.0/tcp/0".parse().unwrap(),
                "/ip6/::/tcp/0".parse().unwrap(),
            ],
            ..Default::default()
        })
        .with_libp2p_ping(ping_config1)
        .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let peer1 = *litep2p1.local_peer_id();

    let listen_address: Vec<_> = litep2p1.listen_addresses().cloned().collect();

    let ip4_port = listen_address.iter().find_map(|address| {
        let mut iter = address.iter();
        match iter.next() {
            Some(Protocol::Ip4(_)) => match iter.next() {
                Some(Protocol::Tcp(port)) => Some(port),
                _ => panic!("invalid protocol"),
            },
            _ => None,
        }
    });
    let ip6_port = listen_address.iter().find_map(|address| {
        let mut iter = address.iter();
        match iter.next() {
            Some(Protocol::Ip6(_)) => match iter.next() {
                Some(Protocol::Tcp(port)) => Some(port),
                _ => panic!("invalid protocol"),
            },
            _ => None,
        }
    });

    tokio::spawn(async move { while let Some(_) = litep2p1.next_event().await {} });

    let network_interfaces = NetworkInterface::show().unwrap();
    for iface in network_interfaces.iter() {
        for address in &iface.addr {
            let (ping_config2, _ping_event_stream2) = PingConfig::default();
            let config = ConfigBuilder::new().with_libp2p_ping(ping_config2);

            let (mut litep2p, dial_address) = match address {
                network_interface::Addr::V4(record) => {
                    if ip4_port.is_none() {
                        continue;
                    }

                    let config = config
                        .with_tcp(TcpConfig {
                            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
                            ..Default::default()
                        })
                        .build();

                    (
                        Litep2p::new(config).unwrap(),
                        Multiaddr::empty()
                            .with(Protocol::Ip4(record.ip))
                            .with(Protocol::Tcp(ip4_port.unwrap()))
                            .with(Protocol::P2p(Multihash::from(peer1))),
                    )
                }
                network_interface::Addr::V6(record) => {
                    if record.ip.segments()[0] == 0xfe80 || ip6_port.is_none() {
                        continue;
                    }

                    let config = config.with_tcp(Default::default()).build();

                    (
                        Litep2p::new(config).unwrap(),
                        Multiaddr::empty()
                            .with(Protocol::Ip6(record.ip))
                            .with(Protocol::Tcp(ip6_port.unwrap()))
                            .with(Protocol::P2p(Multihash::from(peer1))),
                    )
                }
            };

            litep2p.dial_address(dial_address).await.unwrap();
            match litep2p.next_event().await {
                Some(Litep2pEvent::ConnectionEstablished { .. }) => {}
                event => panic!("invalid event: {event:?}"),
            }
        }
    }
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn unspecified_listen_address_websocket() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (ping_config1, _ping_event_stream1) = PingConfig::default();
    let config1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_websocket(WebSocketConfig {
            listen_addresses: vec![
                "/ip4/0.0.0.0/tcp/0/ws".parse().unwrap(),
                "/ip6/::/tcp/0/ws".parse().unwrap(),
            ],
            ..Default::default()
        })
        .with_libp2p_ping(ping_config1)
        .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let peer1 = *litep2p1.local_peer_id();

    let listen_address: Vec<_> = litep2p1.listen_addresses().cloned().collect();

    let ip4_port = listen_address.iter().find_map(|address| {
        let mut iter = address.iter();
        match iter.next() {
            Some(Protocol::Ip4(_)) => match iter.next() {
                Some(Protocol::Tcp(port)) => Some(port),
                _ => panic!("invalid protocol"),
            },
            _ => None,
        }
    });
    let ip6_port = listen_address.iter().find_map(|address| {
        let mut iter = address.iter();
        match iter.next() {
            Some(Protocol::Ip6(_)) => match iter.next() {
                Some(Protocol::Tcp(port)) => Some(port),
                _ => panic!("invalid protocol"),
            },
            _ => None,
        }
    });

    tokio::spawn(async move { while let Some(_) = litep2p1.next_event().await {} });

    let network_interfaces = NetworkInterface::show().unwrap();
    for iface in network_interfaces.iter() {
        for address in &iface.addr {
            let (ping_config2, _ping_event_stream2) = PingConfig::default();
            let config = ConfigBuilder::new().with_libp2p_ping(ping_config2);

            let (mut litep2p, dial_address) = match address {
                network_interface::Addr::V4(record) => {
                    if ip4_port.is_none() {
                        continue;
                    }

                    let config = config
                        .with_websocket(WebSocketConfig {
                            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
                            ..Default::default()
                        })
                        .build();

                    (
                        Litep2p::new(config).unwrap(),
                        Multiaddr::empty()
                            .with(Protocol::Ip4(record.ip))
                            .with(Protocol::Tcp(ip4_port.unwrap()))
                            .with(Protocol::Ws(std::borrow::Cow::Owned("/".to_string())))
                            .with(Protocol::P2p(Multihash::from(peer1))),
                    )
                }
                network_interface::Addr::V6(record) => {
                    if record.ip.segments()[0] == 0xfe80 || ip6_port.is_none() {
                        continue;
                    }

                    let config = config.with_websocket(Default::default()).build();

                    (
                        Litep2p::new(config).unwrap(),
                        Multiaddr::empty()
                            .with(Protocol::Ip6(record.ip))
                            .with(Protocol::Tcp(ip6_port.unwrap()))
                            .with(Protocol::Ws(std::borrow::Cow::Owned("/".to_string())))
                            .with(Protocol::P2p(Multihash::from(peer1))),
                    )
                }
            };

            litep2p.dial_address(dial_address).await.unwrap();
            match litep2p.next_event().await {
                Some(Litep2pEvent::ConnectionEstablished { .. }) => {}
                event => panic!("invalid event: {event:?}"),
            }
        }
    }
}

#[tokio::test]
async fn simultaneous_dial_then_redial_tcp() {
    simultaneous_dial_then_redial(
        Transport::Tcp(TcpConfig {
            reuse_port: false,
            ..Default::default()
        }),
        Transport::Tcp(TcpConfig {
            reuse_port: false,
            ..Default::default()
        }),
    )
    .await
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn simultaneous_dial_then_redial_websocket() {
    simultaneous_dial_then_redial(
        Transport::WebSocket(WebSocketConfig {
            reuse_port: false,
            ..Default::default()
        }),
        Transport::WebSocket(WebSocketConfig {
            reuse_port: false,
            ..Default::default()
        }),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn simultaneous_dial_then_redial_quic() {
    simultaneous_dial_then_redial(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await
}

async fn simultaneous_dial_then_redial(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (ping_config1, _ping_event_stream1) = PingConfig::default();
    let config1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_libp2p_ping(ping_config1);

    let config1 = add_transport(config1, transport1).build();

    let (ping_config2, _ping_event_stream2) = PingConfig::default();
    let config2 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_libp2p_ping(ping_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();
    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    litep2p1.add_known_address(peer2, litep2p2.listen_addresses().cloned());
    litep2p2.add_known_address(peer1, litep2p1.listen_addresses().cloned());

    let (_, _) = tokio::join!(litep2p1.dial(&peer2), litep2p2.dial(&peer1));

    let mut peer1_open = false;
    let mut peer2_open = false;

    while !peer1_open || !peer2_open {
        tokio::select! {
            event = litep2p1.next_event() => if let Litep2pEvent::ConnectionEstablished { .. } = event.unwrap() {
                peer1_open = true;
            },
            event = litep2p2.next_event() => if let Litep2pEvent::ConnectionEstablished { .. } = event.unwrap() {
                peer2_open = true;
            },
        }
    }

    let mut peer1_close = false;
    let mut peer2_close = false;

    while !peer1_close || !peer2_close {
        tokio::select! {
            event = litep2p1.next_event() => if let Litep2pEvent::ConnectionClosed { .. } = event.unwrap() {
                peer1_close = true;
            },
            event = litep2p2.next_event() => if let Litep2pEvent::ConnectionClosed { .. } = event.unwrap() {
                peer2_close = true;
            },
        }
    }

    let (_, _) = tokio::join!(litep2p1.dial(&peer2), litep2p2.dial(&peer1));

    let future = async move {
        let mut peer1_open = false;
        let mut peer2_open = false;

        while !peer1_open || !peer2_open {
            tokio::select! {
                event = litep2p1.next_event() => if let Litep2pEvent::ConnectionEstablished { .. } = event.unwrap() {
                    peer1_open = true;
                },
                event = litep2p2.next_event() => if let Litep2pEvent::ConnectionEstablished { .. } = event.unwrap() {
                    peer2_open = true;
                },
            }
        }
    };

    if let Err(_) = tokio::time::timeout(std::time::Duration::from_secs(10), future).await {
        panic!("failed to open notification stream")
    }
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn check_multi_dial() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    fn build_litep2p() -> Litep2p {
        let (ping_config1, _ping_event_stream1) = PingConfig::default();
        let config = ConfigBuilder::new()
            .with_keypair(Keypair::generate())
            .with_libp2p_ping(ping_config1);

        let tcp_transport = Transport::Tcp(TcpConfig {
            reuse_port: false,
            listen_addresses: vec!["/ip4/0.0.0.0/tcp/0".parse().unwrap()],
            ..Default::default()
        });
        let websocket_transport = Transport::WebSocket(WebSocketConfig {
            reuse_port: false,
            listen_addresses: vec!["/ip4/0.0.0.0/tcp/0/ws".parse().unwrap()],
            ..Default::default()
        });

        let config = add_transport(config, tcp_transport);
        let litep2p_config = add_transport(config, websocket_transport).build();
        Litep2p::new(litep2p_config).unwrap()
    }

    let mut litep2p1 = build_litep2p();
    let mut litep2p2 = build_litep2p();

    let mut litep2p_addresses = litep2p2.listen_addresses().cloned().collect::<Vec<_>>();
    let peer = *litep2p2.local_peer_id();

    tracing::debug!("litep2p2 addresses: {:?}", litep2p_addresses);

    let random_peer = PeerId::random();
    // Replace the PeerId in the multiaddrs with random PeerId to simulate invalid addresses.
    litep2p_addresses.iter_mut().for_each(|addr| {
        addr.pop();
        addr.push(Protocol::P2p(Multihash::from(random_peer)));
    });

    let dialed_addresses: HashSet<_> = litep2p_addresses.clone().into_iter().collect();
    tracing::debug!("dialed  addresses: {:?}", dialed_addresses);

    // All addresses must be added.
    let added = litep2p1.add_known_address(random_peer, litep2p_addresses.into_iter());
    assert_eq!(added, dialed_addresses.len());

    // Dial an unknown peer must return NoAddressAvailable error immediately.
    let result = litep2p1.dial(&peer).await;
    tracing::info!("dial result: {:?}", result);
    match result {
        Err(litep2p::Error::NoAddressAvailable(p)) if p == peer => {}
        _ => panic!("unexpected dial result: {:?}", result),
    }

    // Dial the peer with invalid addresses.
    assert!(litep2p1.dial(&random_peer).await.is_ok());

    loop {
        tokio::select! {
            event = litep2p1.next_event() => {
                tracing::info!("litep2p1 event: {:?}", event);
                if let Some(Litep2pEvent::ListDialFailures { errors }) = event {
                    assert_eq!(errors.len(), dialed_addresses.len());

                    for (addr, error) in errors {
                        assert!(dialed_addresses.contains(&addr));

                        match error {
                            litep2p::error::DialError::NegotiationError(litep2p::error::NegotiationError::PeerIdMismatch(expected, found)) if expected == random_peer && found == peer => {}
                            _ => {
                                panic!("unexpected dial error for address {}: {:?}", addr, error);
                            }
                        }
                    }

                    break;
                }
            }

            event = litep2p2.next_event() => {
                tracing::info!("litep2p2 event: {:?}", event);
            }
        }
    }
}
