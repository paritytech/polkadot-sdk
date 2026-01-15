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
    protocol::request_response::{
        Config as RequestResponseConfig, ConfigBuilder, DialOptions, RejectReason,
        RequestResponseError, RequestResponseEvent,
    },
    transport::tcp::config::Config as TcpConfig,
    types::{protocol::ProtocolName, RequestId},
    Litep2p, Litep2pEvent, PeerId,
};

#[cfg(feature = "websocket")]
use litep2p::transport::websocket::config::Config as WebSocketConfig;

use futures::{channel, StreamExt};
use multiaddr::{Multiaddr, Protocol};
use multihash::Multihash;
use tokio::time::sleep;

#[cfg(feature = "quic")]
use std::net::Ipv4Addr;
use std::{
    collections::{HashMap, HashSet},
    net::Ipv6Addr,
    task::Poll,
    time::Duration,
};

use crate::common::{add_transport, Transport};

async fn connect_peers(litep2p1: &mut Litep2p, litep2p2: &mut Litep2p) {
    let address = litep2p2.listen_addresses().next().unwrap().clone();
    tracing::info!("address: {address}");
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
            break;
        }
    }

    sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn send_request_receive_response_tcp() {
    send_request_receive_response(
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
async fn send_request_receive_response_quic() {
    send_request_receive_response(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn send_request_receive_response_websocket() {
    send_request_receive_response(
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

async fn send_request_receive_response(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, mut handle2) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {},
                _ = litep2p2.next_event() => {},
            }
        }
    });

    // send request to remote peer
    let request_id = handle1
        .send_request(peer2, vec![1, 3, 3, 7], DialOptions::Reject)
        .await
        .unwrap();
    assert_eq!(
        handle2.next().await.unwrap(),
        RequestResponseEvent::RequestReceived {
            peer: peer1,
            fallback: None,
            request_id,
            request: vec![1, 3, 3, 7],
        }
    );

    // send response to the received request
    handle2.send_response(request_id, vec![1, 3, 3, 8]);
    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::ResponseReceived {
            peer: peer2,
            request_id,
            response: vec![1, 3, 3, 8],
            fallback: None,
        }
    );
}

#[tokio::test]
async fn reject_request_tcp() {
    reject_request(
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
async fn reject_request_quic() {
    reject_request(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn reject_request_websocket() {
    reject_request(
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

async fn reject_request(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, mut handle2) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {},
                _ = litep2p2.next_event() => {},
            }
        }
    });

    // send request to remote peer
    let request_id = handle1
        .send_request(peer2, vec![1, 3, 3, 7], DialOptions::Reject)
        .await
        .unwrap();
    if let RequestResponseEvent::RequestReceived {
        peer,
        fallback: None,
        request_id,
        request,
    } = handle2.next().await.unwrap()
    {
        assert_eq!(peer, peer1);
        assert_eq!(request, vec![1, 3, 3, 7]);
        handle2.reject_request(request_id);
    } else {
        panic!("invalid event received");
    };

    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::RequestFailed {
            peer: peer2,
            request_id,
            error: RequestResponseError::Rejected(RejectReason::SubstreamClosed)
        }
    );
}

#[tokio::test]
async fn multiple_simultaneous_requests_tcp() {
    multiple_simultaneous_requests(
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
async fn multiple_simultaneous_requests_quic() {
    multiple_simultaneous_requests(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn multiple_simultaneous_requests_websocket() {
    multiple_simultaneous_requests(
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

async fn multiple_simultaneous_requests(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, mut handle2) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {},
                _ = litep2p2.next_event() => {},
            }
        }
    });

    // send multiple requests to remote peer
    let request_id1 = handle1
        .send_request(peer2, vec![1, 3, 3, 6], DialOptions::Reject)
        .await
        .unwrap();
    let request_id2 = handle1
        .send_request(peer2, vec![1, 3, 3, 7], DialOptions::Reject)
        .await
        .unwrap();
    let request_id3 = handle1
        .send_request(peer2, vec![1, 3, 3, 8], DialOptions::Reject)
        .await
        .unwrap();
    let request_id4 = handle1
        .send_request(peer2, vec![1, 3, 3, 9], DialOptions::Reject)
        .await
        .unwrap();
    let expected: HashMap<RequestId, Vec<u8>> = HashMap::from_iter([
        (request_id1, vec![2, 3, 3, 6]),
        (request_id2, vec![2, 3, 3, 7]),
        (request_id3, vec![2, 3, 3, 8]),
        (request_id4, vec![2, 3, 3, 9]),
    ]);
    let expected_requests: Vec<Vec<u8>> = vec![
        vec![1, 3, 3, 6],
        vec![1, 3, 3, 7],
        vec![1, 3, 3, 8],
        vec![1, 3, 3, 9],
    ];

    for _ in 0..4 {
        if let RequestResponseEvent::RequestReceived {
            peer,
            fallback: None,
            request_id,
            mut request,
        } = handle2.next().await.unwrap()
        {
            assert_eq!(peer, peer1);
            if expected_requests.iter().any(|req| req == &request) {
                request[0] = 2;
                handle2.send_response(request_id, request);
            } else {
                panic!("invalid request received");
            }
        } else {
            panic!("invalid event received");
        };
    }

    for _ in 0..4 {
        if let RequestResponseEvent::ResponseReceived {
            peer,
            request_id,
            response,
            ..
        } = handle1.next().await.unwrap()
        {
            assert_eq!(peer, peer2);
            assert_eq!(response, expected.get(&request_id).unwrap().to_vec());
        } else {
            panic!("invalid event received");
        };
    }
}

#[tokio::test]
async fn request_timeout_tcp() {
    request_timeout(
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
async fn request_timeout_quic() {
    request_timeout(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn request_timeout_websocket() {
    request_timeout(
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

async fn request_timeout(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, _handle2) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let _peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {},
                _ = litep2p2.next_event() => {},
            }
        }
    });

    // send request to remote peer and wait until the requet timeout occurs
    let request_id = handle1
        .send_request(peer2, vec![1, 3, 3, 7], DialOptions::Reject)
        .await
        .unwrap();

    sleep(Duration::from_secs(7)).await;

    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::RequestFailed {
            peer: peer2,
            request_id,
            error: RequestResponseError::Timeout,
        }
    );
}

#[tokio::test]
async fn protocol_not_supported_tcp() {
    protocol_not_supported(
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
async fn protocol_not_supported_quic() {
    protocol_not_supported(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn protocol_not_supported_websocket() {
    protocol_not_supported(
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

async fn protocol_not_supported(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, _handle2) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/2"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let _peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {},
                _ = litep2p2.next_event() => {},
            }
        }
    });

    // send request to remote peer and wait until the requet timeout occurs
    let request_id = handle1
        .send_request(peer2, vec![1, 3, 3, 7], DialOptions::Reject)
        .await
        .unwrap();

    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::RequestFailed {
            peer: peer2,
            request_id,
            error: RequestResponseError::UnsupportedProtocol,
        }
    );
}

#[tokio::test]
async fn connection_close_while_request_is_pending_tcp() {
    connection_close_while_request_is_pending(
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
async fn connection_close_while_request_is_pending_quic() {
    connection_close_while_request_is_pending(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn connection_close_while_request_is_pending_websocket() {
    connection_close_while_request_is_pending(
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

async fn connection_close_while_request_is_pending(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, handle2) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let _peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            let _ = litep2p1.next_event().await;
        }
    });

    // send request to remote peer and wait until the requet timeout occurs
    let request_id = handle1
        .send_request(peer2, vec![1, 3, 3, 7], DialOptions::Reject)
        .await
        .unwrap();

    drop(handle2);
    drop(litep2p2);

    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::RequestFailed {
            peer: peer2,
            request_id,
            error: RequestResponseError::Rejected(RejectReason::ConnectionClosed),
        }
    );
}

#[tokio::test]
async fn request_too_big_tcp() {
    request_too_big(
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
async fn request_too_big_quic() {
    request_too_big(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn request_too_big_websocket() {
    request_too_big(
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

async fn request_too_big(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        256,
        Duration::from_secs(5),
        None,
    );
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, _handle2) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {},
                _ = litep2p2.next_event() => {},
            }
        }
    });

    // try to send too large request to remote peer
    let request_id =
        handle1.send_request(peer2, vec![0u8; 257], DialOptions::Reject).await.unwrap();
    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::RequestFailed {
            peer: peer2,
            request_id,
            error: RequestResponseError::TooLargePayload,
        }
    );
}

#[tokio::test]
async fn response_too_big_tcp() {
    response_too_big(
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
async fn response_too_big_quic() {
    response_too_big(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn response_too_big_websocket() {
    response_too_big(
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

async fn response_too_big(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        256,
        Duration::from_secs(5),
        None,
    );
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, mut handle2) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        256,
        Duration::from_secs(5),
        None,
    );
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {},
                _ = litep2p2.next_event() => {},
            }
        }
    });

    // send request to remote peer
    let request_id =
        handle1.send_request(peer2, vec![0u8; 256], DialOptions::Reject).await.unwrap();
    assert_eq!(
        handle2.next().await.unwrap(),
        RequestResponseEvent::RequestReceived {
            peer: peer1,
            fallback: None,
            request_id,
            request: vec![0u8; 256],
        }
    );

    // try to send too large response to the received request
    handle2.send_response(request_id, vec![0u8; 257]);

    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::RequestFailed {
            peer: peer2,
            request_id,
            error: RequestResponseError::Rejected(RejectReason::SubstreamClosed),
        }
    );
}

#[tokio::test]
async fn too_many_pending_requests() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );
    let mut yamux_config = litep2p::yamux::Config::default();
    yamux_config.set_max_num_streams(4);

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_request_response_protocol(req_resp_config1)
        .build();

    let (req_resp_config2, _handle2) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );
    let mut yamux_config = litep2p::yamux::Config::default();
    yamux_config.set_max_num_streams(4);

    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_tcp(TcpConfig {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        })
        .with_request_response_protocol(req_resp_config2)
        .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;

    // send one over the max requests to remote peer
    let mut request_ids = HashSet::new();

    request_ids.insert(
        handle1
            .send_request(peer2, vec![1, 3, 3, 6], DialOptions::Reject)
            .await
            .unwrap(),
    );
    request_ids.insert(
        handle1
            .send_request(peer2, vec![1, 3, 3, 7], DialOptions::Reject)
            .await
            .unwrap(),
    );
    request_ids.insert(
        handle1
            .send_request(peer2, vec![1, 3, 3, 8], DialOptions::Reject)
            .await
            .unwrap(),
    );
    request_ids.insert(
        handle1
            .send_request(peer2, vec![1, 3, 3, 9], DialOptions::Reject)
            .await
            .unwrap(),
    );
    request_ids.insert(
        handle1
            .send_request(peer2, vec![1, 3, 3, 9], DialOptions::Reject)
            .await
            .unwrap(),
    );

    let mut litep2p1_closed = false;
    let mut litep2p2_closed = false;

    while !litep2p1_closed || !litep2p2_closed || !request_ids.is_empty() {
        tokio::select! {
            event = litep2p1.next_event() => if let Some(Litep2pEvent::ConnectionClosed { .. }) = event {
                litep2p1_closed = true;
            },
            event = litep2p2.next_event() => if let Some(Litep2pEvent::ConnectionClosed { .. }) = event {
                litep2p2_closed = true;
            },
            event = handle1.next() => if let Some(RequestResponseEvent::RequestFailed {
                    request_id,
                    ..
                }) = event {
                request_ids.remove(&request_id);
            }
        }
    }
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

    let (req_resp_config1, mut handle1) =
        ConfigBuilder::new(ProtocolName::from("/protocol/1/improved"))
            .with_max_size(1024usize)
            .with_fallback_names(vec![ProtocolName::from("/protocol/1")])
            .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, mut handle2) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {},
                _ = litep2p2.next_event() => {},
            }
        }
    });

    // send request to remote peer
    let request_id = handle1
        .send_request(peer2, vec![1, 3, 3, 7], DialOptions::Reject)
        .await
        .unwrap();
    assert_eq!(
        handle2.next().await.unwrap(),
        RequestResponseEvent::RequestReceived {
            peer: peer1,
            fallback: None,
            request_id,
            request: vec![1, 3, 3, 7],
        }
    );

    // send response to the received request
    handle2.send_response(request_id, vec![1, 3, 3, 8]);
    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::ResponseReceived {
            peer: peer2,
            request_id,
            response: vec![1, 3, 3, 8],
            fallback: Some(ProtocolName::from("/protocol/1")),
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

    let (req_resp_config1, mut handle1) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, mut handle2) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1/improved"),
        vec![ProtocolName::from("/protocol/1")],
        1024,
        Duration::from_secs(5),
        None,
    );
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {},
                _ = litep2p2.next_event() => {},
            }
        }
    });

    // send request to remote peer
    let request_id = handle1
        .send_request(peer2, vec![1, 3, 3, 7], DialOptions::Reject)
        .await
        .unwrap();
    assert_eq!(
        handle2.next().await.unwrap(),
        RequestResponseEvent::RequestReceived {
            peer: peer1,
            fallback: Some(ProtocolName::from("/protocol/1")),
            request_id,
            request: vec![1, 3, 3, 7],
        }
    );

    // send response to the received request
    handle2.send_response(request_id, vec![1, 3, 3, 8]);
    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::ResponseReceived {
            peer: peer2,
            request_id,
            response: vec![1, 3, 3, 8],
            fallback: None,
        }
    );
}

#[tokio::test]
async fn dial_peer_when_sending_request_tcp() {
    dial_peer_when_sending_request(
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
async fn dial_peer_when_sending_request_quic() {
    dial_peer_when_sending_request(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn dial_peer_when_sending_request_websocket() {
    dial_peer_when_sending_request(
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

async fn dial_peer_when_sending_request(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, mut handle2) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1/improved"),
        vec![ProtocolName::from("/protocol/1")],
        1024,
        Duration::from_secs(5),
        None,
    );
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();
    let address = litep2p2.listen_addresses().next().unwrap().clone();

    // add known address for `peer2` and start event loop for both litep2ps
    litep2p1.add_known_address(peer2, std::iter::once(address));

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {}
                _ = litep2p2.next_event() => {}
            }
        }
    });

    // send request to remote peer
    let request_id =
        handle1.send_request(peer2, vec![1, 3, 3, 7], DialOptions::Dial).await.unwrap();
    assert_eq!(
        handle2.next().await.unwrap(),
        RequestResponseEvent::RequestReceived {
            peer: peer1,
            fallback: Some(ProtocolName::from("/protocol/1")),
            request_id,
            request: vec![1, 3, 3, 7],
        }
    );

    // send response to the received request
    handle2.send_response(request_id, vec![1, 3, 3, 8]);
    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::ResponseReceived {
            peer: peer2,
            request_id,
            response: vec![1, 3, 3, 8],
            fallback: None,
        }
    );
}

#[tokio::test]
async fn dial_peer_but_no_known_address_tcp() {
    dial_peer_but_no_known_address(
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
async fn dial_peer_but_no_known_address_quic() {
    dial_peer_but_no_known_address(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn dial_peer_but_no_known_address_websocket() {
    dial_peer_but_no_known_address(
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

async fn dial_peer_but_no_known_address(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, _handle2) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1/improved"),
        vec![ProtocolName::from("/protocol/1")],
        1024,
        Duration::from_secs(5),
        None,
    );
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer2 = *litep2p2.local_peer_id();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {}
                _ = litep2p2.next_event() => {}
            }
        }
    });

    // send request to remote peer
    let request_id =
        handle1.send_request(peer2, vec![1, 3, 3, 7], DialOptions::Dial).await.unwrap();
    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::RequestFailed {
            peer: peer2,
            request_id,
            error: RequestResponseError::Rejected(RejectReason::DialFailed(Some(
                litep2p::error::ImmediateDialError::NoAddressAvailable
            ))),
        }
    );
}

#[tokio::test]
async fn cancel_request_tcp() {
    cancel_request(
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
async fn cancel_request_quic() {
    cancel_request(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn cancel_request_websocket() {
    cancel_request(
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

async fn cancel_request(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, mut handle2) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {},
                _ = litep2p2.next_event() => {},
            }
        }
    });

    // send request to remote peer
    let request_id = handle1
        .send_request(peer2, vec![1, 3, 3, 7], DialOptions::Reject)
        .await
        .unwrap();
    assert_eq!(
        handle2.next().await.unwrap(),
        RequestResponseEvent::RequestReceived {
            peer: peer1,
            fallback: None,
            request_id,
            request: vec![1, 3, 3, 7],
        }
    );

    // cancel request
    handle1.cancel_request(request_id).await;

    // try to send response to the canceled request
    handle2.send_response(request_id, vec![1, 3, 3, 8]);

    // verify that nothing is receieved since the request was canceled
    match tokio::time::timeout(Duration::from_secs(2), handle1.next()).await {
        Err(_) => {}
        Ok(event) => panic!("invalid event received: {event:?}"),
    }
}

#[tokio::test]
async fn substream_open_failure_reported_once_tcp() {
    substream_open_failure_reported_once(
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
async fn substream_open_failure_reported_once_quic() {
    substream_open_failure_reported_once(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn substream_open_failure_reported_once_websocket() {
    substream_open_failure_reported_once(
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

async fn substream_open_failure_reported_once(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/1"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );
    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, _handle2) = RequestResponseConfig::new(
        ProtocolName::from("/protocol/2"),
        Vec::new(),
        1024,
        Duration::from_secs(5),
        None,
    );
    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p2.next_event() => {},
            }
        }
    });

    // send request to remote peer
    let request_id = handle1
        .send_request(peer2, vec![1, 3, 3, 7], DialOptions::Reject)
        .await
        .unwrap();

    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::RequestFailed {
            peer: peer2,
            request_id,
            error: RequestResponseError::UnsupportedProtocol,
        }
    );

    loop {
        match litep2p1.next_event().await {
            Some(Litep2pEvent::ConnectionClosed { peer, .. }) => {
                assert_eq!(peer, peer2);
                break;
            }
            event => panic!("invalid event received: {event:?}"),
        }
    }

    // verify that nothing is received from the handle as the request failure was already reported
    if let Ok(event) = tokio::time::timeout(Duration::from_secs(5), handle1.next()).await {
        panic!("didn't expect to receive event: {event:?}");
    }
}

#[tokio::test]
async fn excess_inbound_request_rejected_tcp() {
    excess_inbound_request_rejected(
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
async fn excess_inbound_request_rejected_quic() {
    excess_inbound_request_rejected(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn excess_inbound_request_rejected_websocket() {
    excess_inbound_request_rejected(
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

async fn excess_inbound_request_rejected(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) = ConfigBuilder::new(ProtocolName::from("/protocol/1"))
        .with_max_size(1024)
        .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, _handle2) = ConfigBuilder::new(ProtocolName::from("/protocol/1"))
        .with_max_size(1024)
        .with_max_concurrent_inbound_requests(2)
        .build();

    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p2.next_event() => {},
                _ = litep2p1.next_event() => {},
            }
        }
    });

    // send two requests and verify that nothing is returned back (yet)
    for _ in 0..2 {
        let _ = handle1
            .send_request(peer2, vec![1, 3, 3, 7], DialOptions::Reject)
            .await
            .unwrap();
    }

    futures::future::poll_fn(|cx| match handle1.poll_next_unpin(cx) {
        Poll::Pending => Poll::Ready(()),
        Poll::Ready(_) => panic!("didn't expect an event"),
    })
    .await;

    // send another request to peer and since there's two requests already pending
    // and the limit was set at 2, the third request must be rejeced
    let request_id = handle1
        .send_request(peer2, vec![1, 3, 3, 7], DialOptions::Reject)
        .await
        .unwrap();

    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::RequestFailed {
            peer: peer2,
            request_id,
            error: RequestResponseError::Rejected(RejectReason::SubstreamClosed)
        }
    );
}

#[tokio::test]
async fn feedback_received_for_succesful_response_tcp() {
    feedback_received_for_succesful_response(
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
async fn feedback_received_for_succesful_response_quic() {
    feedback_received_for_succesful_response(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn feedback_received_for_succesful_response_websocket() {
    feedback_received_for_succesful_response(
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

async fn feedback_received_for_succesful_response(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) = ConfigBuilder::new(ProtocolName::from("/protocol/1"))
        .with_max_size(1024)
        .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, mut handle2) = ConfigBuilder::new(ProtocolName::from("/protocol/1"))
        .with_max_size(1024)
        .build();

    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();
    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p2.next_event() => {},
                _ = litep2p1.next_event() => {},
            }
        }
    });

    let request_id = handle1
        .send_request(peer2, vec![1, 3, 3, 7], DialOptions::Reject)
        .await
        .unwrap();

    assert_eq!(
        handle2.next().await.unwrap(),
        RequestResponseEvent::RequestReceived {
            peer: peer1,
            fallback: None,
            request_id,
            request: vec![1, 3, 3, 7]
        },
    );

    // send response with feedback and verify that the response was sent successfully
    let (feedback_tx, feedback_rx) = channel::oneshot::channel();
    handle2.send_response_with_feedback(request_id, vec![1, 3, 3, 8], feedback_tx);

    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::ResponseReceived {
            peer: peer2,
            request_id,
            response: vec![1, 3, 3, 8],
            fallback: None,
        }
    );
    assert!(feedback_rx.await.is_ok());
}

// #[tokio::test]
// async fn feedback_not_received_for_failed_response_tcp() {
//     feedback_not_received_for_failed_response(
//         Transport::Tcp(TcpConfig {
//             listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
//             ..Default::default()
//         }),
//         Transport::Tcp(TcpConfig {
//             listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
//             ..Default::default()
//         }),
//     )
//     .await;
// }

#[cfg(feature = "quic")]
#[tokio::test]
async fn feedback_not_received_for_failed_response_quic() {
    feedback_not_received_for_failed_response(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

// #[tokio::test]
// async fn feedback_not_received_for_failed_response_websocket() {
//     feedback_not_received_for_failed_response(
//         Transport::WebSocket(WebSocketConfig {
//             listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
//             ..Default::default()
//         }),
//         Transport::WebSocket(WebSocketConfig {
//             listen_addresses: vec!["/ip4/127.0.0.1/tcp/0/ws".parse().unwrap()],
//             ..Default::default()
//         }),
//     )
//     .await;
// }

#[cfg(feature = "quic")]
async fn feedback_not_received_for_failed_response(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) = ConfigBuilder::new(ProtocolName::from("/protocol/1"))
        .with_max_size(1024)
        .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, mut handle2) = ConfigBuilder::new(ProtocolName::from("/protocol/1"))
        .with_max_size(1024)
        .build();

    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();
    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p2.next_event() => {},
                _ = litep2p1.next_event() => {},
            }
        }
    });

    let request_id = handle1
        .send_request(peer2, vec![1, 3, 3, 7], DialOptions::Reject)
        .await
        .unwrap();

    assert_eq!(
        handle2.next().await.unwrap(),
        RequestResponseEvent::RequestReceived {
            peer: peer1,
            fallback: None,
            request_id,
            request: vec![1, 3, 3, 7]
        },
    );

    // cancel the request and give a moment to register
    handle1.cancel_request(request_id).await;
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // send response with feedback and verify that sending the response fails
    let (feedback_tx, feedback_rx) = channel::oneshot::channel();
    handle2.send_response_with_feedback(request_id, vec![1, 3, 3, 8], feedback_tx);

    assert!(feedback_rx.await.is_err());
}

#[tokio::test]
async fn custom_timeout_tcp() {
    custom_timeout(
        Transport::Tcp(Default::default()),
        Transport::Tcp(Default::default()),
    )
    .await;
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn custom_timeout_quic() {
    custom_timeout(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn custom_timeout_websocket() {
    custom_timeout(
        Transport::WebSocket(Default::default()),
        Transport::WebSocket(Default::default()),
    )
    .await;
}

async fn custom_timeout(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) = ConfigBuilder::new(ProtocolName::from("/protocol/1"))
        .with_max_size(1024)
        .with_timeout(Duration::from_secs(8))
        .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, _handle2) = ConfigBuilder::new(ProtocolName::from("/protocol/1"))
        .with_max_size(1024)
        .build();

    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p2.next_event() => {},
                _ = litep2p1.next_event() => {},
            }
        }
    });

    let request_id =
        handle1.try_send_request(peer2, vec![1, 3, 3, 7], DialOptions::Reject).unwrap();

    // verify that the request doesn't timeout after the default timeout
    match tokio::time::timeout(Duration::from_secs(5), handle1.next()).await {
        Err(_) => {}
        Ok(_) => panic!("expected request to timeout"),
    };

    // verify that the request times out
    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::RequestFailed {
            peer: peer2,
            request_id,
            error: RequestResponseError::Timeout
        }
    );
}

#[tokio::test]
async fn outbound_request_for_unconnected_peer_tcp() {
    outbound_request_for_unconnected_peer(Transport::Tcp(Default::default())).await;
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn outbound_request_for_unconnected_peer_quic() {
    outbound_request_for_unconnected_peer(Transport::Quic(Default::default())).await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn outbound_request_for_unconnected_peer_websocket() {
    outbound_request_for_unconnected_peer(Transport::WebSocket(Default::default())).await;
}

async fn outbound_request_for_unconnected_peer(transport1: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) = ConfigBuilder::new(ProtocolName::from("/protocol/1"))
        .with_max_size(1024)
        .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    tokio::spawn(async move {
        let mut litep2p1 = Litep2p::new(config1).unwrap();
        while let Some(_) = litep2p1.next_event().await {}
    });

    let peer2 = PeerId::random();
    let request_id = handle1
        .send_request(peer2, vec![1, 3, 3, 7], DialOptions::Reject)
        .await
        .unwrap();

    // verify that the request times out
    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::RequestFailed {
            peer: peer2,
            request_id,
            error: RequestResponseError::NotConnected
        }
    );
}

#[tokio::test]
async fn dial_failure_tcp() {
    dial_failure(Transport::Tcp(Default::default())).await;
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn dial_failure_quic() {
    dial_failure(Transport::Quic(Default::default())).await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn dial_failure_websocket() {
    dial_failure(Transport::WebSocket(Default::default())).await;
}

async fn dial_failure(transport: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config, mut handle) = ConfigBuilder::new(ProtocolName::from("/protocol/1"))
        .with_max_size(1024)
        .build();

    let litep2p_config = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config);

    let peer = PeerId::random();
    let known_address = match &transport {
        Transport::Tcp(_) => Multiaddr::empty()
            .with(Protocol::Ip6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)))
            .with(Protocol::Tcp(5))
            .with(Protocol::P2p(Multihash::from(peer))),
        #[cfg(feature = "quic")]
        Transport::Quic(_) => Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Udp(5))
            .with(Protocol::QuicV1)
            .with(Protocol::P2p(Multihash::from(peer))),
        #[cfg(feature = "websocket")]
        Transport::WebSocket(_) => Multiaddr::empty()
            .with(Protocol::Ip6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)))
            .with(Protocol::Tcp(5))
            .with(Protocol::Ws(std::borrow::Cow::Owned("/".to_string())))
            .with(Protocol::P2p(Multihash::from(peer))),
    };

    let config = add_transport(litep2p_config, transport).build();

    let mut litep2p = Litep2p::new(config).unwrap();
    litep2p.add_known_address(peer, vec![known_address].into_iter());
    tokio::spawn(async move { while let Some(_) = litep2p.next_event().await {} });

    let request_id = handle.send_request(peer, vec![1, 3, 3, 7], DialOptions::Dial).await.unwrap();

    // verify that the request is reported as rejected since the dial failed
    assert_eq!(
        handle.next().await.unwrap(),
        RequestResponseEvent::RequestFailed {
            peer,
            request_id,
            error: RequestResponseError::Rejected(RejectReason::DialFailed(None))
        }
    );
}

#[tokio::test]
async fn large_response_tcp() {
    large_response(
        Transport::Tcp(Default::default()),
        Transport::Tcp(Default::default()),
    )
    .await;
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn large_response_quic() {
    large_response(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn large_response_websocket() {
    large_response(
        Transport::WebSocket(Default::default()),
        Transport::WebSocket(Default::default()),
    )
    .await;
}

async fn large_response(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) = ConfigBuilder::new(ProtocolName::from("/protocol/1"))
        .with_max_size(16 * 1024 * 1024)
        .with_timeout(Duration::from_secs(8))
        .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, mut handle2) = ConfigBuilder::new(ProtocolName::from("/protocol/1"))
        .with_max_size(16 * 1024 * 1024)
        .build();

    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();
    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p2.next_event() => {},
                _ = litep2p1.next_event() => {},
            }
        }
    });

    let response: Vec<u8> = vec![1; 15 * 1024 * 1024];

    let request_id =
        handle1.try_send_request(peer2, vec![1, 3, 3, 7], DialOptions::Reject).unwrap();

    assert_eq!(
        handle2.next().await.unwrap(),
        RequestResponseEvent::RequestReceived {
            peer: peer1,
            fallback: None,
            request_id,
            request: vec![1, 3, 3, 7],
        }
    );

    // send response to the received request
    handle2.send_response(request_id, response.clone());
    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::ResponseReceived {
            peer: peer2,
            request_id,
            response,
            fallback: None,
        }
    );
}

#[tokio::test]
async fn binary_incompatible_fallback_tcp() {
    binary_incompatible_fallback(
        Transport::Tcp(Default::default()),
        Transport::Tcp(Default::default()),
    )
    .await;
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn binary_incompatible_fallback_quic() {
    binary_incompatible_fallback(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn binary_incompatible_fallback_websocket() {
    binary_incompatible_fallback(
        Transport::WebSocket(Default::default()),
        Transport::WebSocket(Default::default()),
    )
    .await;
}

async fn binary_incompatible_fallback(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) = ConfigBuilder::new(ProtocolName::from("/protocol/2"))
        .with_max_size(16 * 1024 * 1024)
        .with_fallback_names(vec![ProtocolName::from("/protocol/1")])
        .with_timeout(Duration::from_secs(8))
        .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, mut handle2) = ConfigBuilder::new(ProtocolName::from("/protocol/1"))
        .with_max_size(16 * 1024 * 1024)
        .build();

    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();
    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p2.next_event() => {},
                _ = litep2p1.next_event() => {},
            }
        }
    });

    let request_id = handle1
        .send_request_with_fallback(
            peer2,
            vec![1, 2, 3, 4],
            (ProtocolName::from("/protocol/1"), vec![5, 6, 7, 8]),
            DialOptions::Reject,
        )
        .await
        .unwrap();

    assert_eq!(
        handle2.next().await.unwrap(),
        RequestResponseEvent::RequestReceived {
            peer: peer1,
            fallback: None,
            request_id,
            request: vec![5, 6, 7, 8],
        }
    );

    handle2.send_response(request_id, vec![1, 3, 3, 7]);

    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::ResponseReceived {
            peer: peer2,
            request_id,
            response: vec![1, 3, 3, 7],
            fallback: Some(ProtocolName::from("/protocol/1")),
        }
    );
}

#[tokio::test]
async fn binary_incompatible_fallback_inbound_request_tcp() {
    binary_incompatible_fallback_inbound_request(
        Transport::Tcp(Default::default()),
        Transport::Tcp(Default::default()),
    )
    .await;
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn binary_incompatible_fallback_inbound_request_quic() {
    binary_incompatible_fallback_inbound_request(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn binary_incompatible_fallback_inbound_request_websocket() {
    binary_incompatible_fallback_inbound_request(
        Transport::WebSocket(Default::default()),
        Transport::WebSocket(Default::default()),
    )
    .await;
}

async fn binary_incompatible_fallback_inbound_request(
    transport1: Transport,
    transport2: Transport,
) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) = ConfigBuilder::new(ProtocolName::from("/protocol/2"))
        .with_max_size(16 * 1024 * 1024)
        .with_fallback_names(vec![ProtocolName::from("/protocol/1")])
        .with_timeout(Duration::from_secs(8))
        .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, mut handle2) = ConfigBuilder::new(ProtocolName::from("/protocol/1"))
        .with_max_size(16 * 1024 * 1024)
        .build();

    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();
    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p2.next_event() => {},
                _ = litep2p1.next_event() => {},
            }
        }
    });

    let request_id = handle2
        .send_request(peer1, vec![1, 2, 3, 4], DialOptions::Reject)
        .await
        .unwrap();

    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::RequestReceived {
            peer: peer2,
            fallback: Some(ProtocolName::from("/protocol/1")),
            request_id,
            request: vec![1, 2, 3, 4],
        }
    );

    handle1.send_response(request_id, vec![1, 3, 3, 8]);

    assert_eq!(
        handle2.next().await.unwrap(),
        RequestResponseEvent::ResponseReceived {
            peer: peer1,
            request_id,
            response: vec![1, 3, 3, 8],
            fallback: None,
        }
    );
}

#[tokio::test]
async fn binary_incompatible_fallback_two_fallback_protocols_tcp() {
    binary_incompatible_fallback_two_fallback_protocols(
        Transport::Tcp(Default::default()),
        Transport::Tcp(Default::default()),
    )
    .await;
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn binary_incompatible_fallback_two_fallback_protocols_quic() {
    binary_incompatible_fallback_two_fallback_protocols(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn binary_incompatible_fallback_two_fallback_protocols_websocket() {
    binary_incompatible_fallback_two_fallback_protocols(
        Transport::WebSocket(Default::default()),
        Transport::WebSocket(Default::default()),
    )
    .await;
}

async fn binary_incompatible_fallback_two_fallback_protocols(
    transport1: Transport,
    transport2: Transport,
) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) =
        ConfigBuilder::new(ProtocolName::from("/genesis/protocol/2"))
            .with_max_size(16 * 1024 * 1024)
            .with_fallback_names(vec![
                ProtocolName::from("/genesis/protocol/1"),
                ProtocolName::from("/dot/protocol/1"),
            ])
            .with_timeout(Duration::from_secs(8))
            .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, mut handle2) =
        ConfigBuilder::new(ProtocolName::from("/genesis/protocol/1"))
            .with_fallback_names(vec![ProtocolName::from("/dot/protocol/1")])
            .with_max_size(16 * 1024 * 1024)
            .build();

    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();
    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p2.next_event() => {},
                _ = litep2p1.next_event() => {},
            }
        }
    });

    let request_id = handle1
        .send_request_with_fallback(
            peer2,
            vec![1, 2, 3, 4],
            (ProtocolName::from("/genesis/protocol/1"), vec![5, 6, 7, 8]),
            DialOptions::Reject,
        )
        .await
        .unwrap();

    assert_eq!(
        handle2.next().await.unwrap(),
        RequestResponseEvent::RequestReceived {
            peer: peer1,
            fallback: None,
            request_id,
            request: vec![5, 6, 7, 8],
        }
    );

    handle2.send_response(request_id, vec![1, 3, 3, 7]);

    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::ResponseReceived {
            peer: peer2,
            request_id,
            response: vec![1, 3, 3, 7],
            fallback: Some(ProtocolName::from("/genesis/protocol/1")),
        }
    );
}

#[tokio::test]
async fn binary_incompatible_fallback_two_fallback_protocols_inbound_request_tcp() {
    binary_incompatible_fallback_two_fallback_protocols_inbound_request(
        Transport::Tcp(Default::default()),
        Transport::Tcp(Default::default()),
    )
    .await;
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn binary_incompatible_fallback_two_fallback_protocols_inbound_request_quic() {
    binary_incompatible_fallback_two_fallback_protocols_inbound_request(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn binary_incompatible_fallback_two_fallback_protocols_inbound_request_websocket() {
    binary_incompatible_fallback_two_fallback_protocols_inbound_request(
        Transport::WebSocket(Default::default()),
        Transport::WebSocket(Default::default()),
    )
    .await;
}

async fn binary_incompatible_fallback_two_fallback_protocols_inbound_request(
    transport1: Transport,
    transport2: Transport,
) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) =
        ConfigBuilder::new(ProtocolName::from("/genesis/protocol/2"))
            .with_max_size(16 * 1024 * 1024)
            .with_fallback_names(vec![
                ProtocolName::from("/genesis/protocol/1"),
                ProtocolName::from("/dot/protocol/1"),
            ])
            .with_timeout(Duration::from_secs(8))
            .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, mut handle2) =
        ConfigBuilder::new(ProtocolName::from("/genesis/protocol/1"))
            .with_fallback_names(vec![ProtocolName::from("/dot/protocol/1")])
            .with_max_size(16 * 1024 * 1024)
            .build();

    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();
    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p2.next_event() => {},
                _ = litep2p1.next_event() => {},
            }
        }
    });

    let request_id = handle2
        .send_request(peer1, vec![1, 2, 3, 4], DialOptions::Reject)
        .await
        .unwrap();

    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::RequestReceived {
            peer: peer2,
            fallback: Some(ProtocolName::from("/genesis/protocol/1")),
            request_id,
            request: vec![1, 2, 3, 4],
        }
    );

    handle1.send_response(request_id, vec![1, 3, 3, 7]);

    assert_eq!(
        handle2.next().await.unwrap(),
        RequestResponseEvent::ResponseReceived {
            peer: peer1,
            request_id,
            response: vec![1, 3, 3, 7],
            fallback: None,
        }
    );
}

#[tokio::test]
async fn binary_incompatible_fallback_compatible_nodes_tcp() {
    binary_incompatible_fallback_compatible_nodes(
        Transport::Tcp(Default::default()),
        Transport::Tcp(Default::default()),
    )
    .await;
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn binary_incompatible_fallback_compatible_nodes_quic() {
    binary_incompatible_fallback_compatible_nodes(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn binary_incompatible_fallback_compatible_nodes_websocket() {
    binary_incompatible_fallback_compatible_nodes(
        Transport::WebSocket(Default::default()),
        Transport::WebSocket(Default::default()),
    )
    .await;
}

async fn binary_incompatible_fallback_compatible_nodes(
    transport1: Transport,
    transport2: Transport,
) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (req_resp_config1, mut handle1) =
        ConfigBuilder::new(ProtocolName::from("/genesis/protocol/2"))
            .with_max_size(16 * 1024 * 1024)
            .with_fallback_names(vec![
                ProtocolName::from("/genesis/protocol/1"),
                ProtocolName::from("/dot/protocol/1"),
            ])
            .with_timeout(Duration::from_secs(8))
            .build();

    let config1 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config1);

    let config1 = add_transport(config1, transport1).build();

    let (req_resp_config2, mut handle2) =
        ConfigBuilder::new(ProtocolName::from("/genesis/protocol/2"))
            .with_max_size(16 * 1024 * 1024)
            .with_fallback_names(vec![
                ProtocolName::from("/genesis/protocol/1"),
                ProtocolName::from("/dot/protocol/1"),
            ])
            .with_timeout(Duration::from_secs(8))
            .build();

    let config2 = Litep2pConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_request_response_protocol(req_resp_config2);

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();
    let peer1 = *litep2p1.local_peer_id();
    let peer2 = *litep2p2.local_peer_id();

    // wait until peers have connected
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p2.next_event() => {},
                _ = litep2p1.next_event() => {},
            }
        }
    });

    let request_id = handle1
        .send_request_with_fallback(
            peer2,
            vec![1, 2, 3, 4],
            (ProtocolName::from("/genesis/protocol/1"), vec![5, 6, 7, 8]),
            DialOptions::Reject,
        )
        .await
        .unwrap();

    assert_eq!(
        handle2.next().await.unwrap(),
        RequestResponseEvent::RequestReceived {
            peer: peer1,
            fallback: None,
            request_id,
            request: vec![1, 2, 3, 4],
        }
    );

    handle2.send_response(request_id, vec![1, 3, 3, 7]);

    assert_eq!(
        handle1.next().await.unwrap(),
        RequestResponseEvent::ResponseReceived {
            peer: peer2,
            request_id,
            response: vec![1, 3, 3, 7],
            fallback: None,
        }
    );
}
