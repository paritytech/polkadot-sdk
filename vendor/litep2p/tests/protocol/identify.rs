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

use futures::{FutureExt, StreamExt};
use litep2p::{
    config::ConfigBuilder,
    crypto::ed25519::Keypair,
    protocol::libp2p::{
        identify::{Config, IdentifyEvent},
        ping::Config as PingConfig,
    },
    Litep2p, Litep2pEvent,
};

use crate::common::{add_transport, Transport};

#[tokio::test]
async fn identify_supported_tcp() {
    identify_supported(
        Transport::Tcp(Default::default()),
        Transport::Tcp(Default::default()),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn identify_supported_quic() {
    identify_supported(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn identify_supported_websocket() {
    identify_supported(
        Transport::WebSocket(Default::default()),
        Transport::WebSocket(Default::default()),
    )
    .await
}

async fn identify_supported(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (identify_config1, mut identify_event_stream1) =
        Config::new("/proto/1".to_string(), Some("agent v1".to_string()));
    let config_builder1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_libp2p_identify(identify_config1);
    let config1 = add_transport(config_builder1, transport1).build();

    let (identify_config2, mut identify_event_stream2) =
        Config::new("/proto/2".to_string(), Some("agent v2".to_string()));
    let config_builder2 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_libp2p_identify(identify_config2);
    let config2 = add_transport(config_builder2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let address1 = litep2p1.listen_addresses().next().unwrap().clone();
    let address2 = litep2p2.listen_addresses().next().unwrap().clone();

    tracing::info!("listen address of peer1: {address1}");
    tracing::info!("listen address of peer2: {address2}");

    litep2p1.dial_address(address2).await.unwrap();

    let mut litep2p1_done = false;
    let mut litep2p2_done = false;

    loop {
        tokio::select! {
            _event = litep2p1.next_event() => {}
            _event = litep2p2.next_event() => {}
            event = identify_event_stream1.next() => {
                let IdentifyEvent::PeerIdentified { observed_address, protocol_version, user_agent, .. } = event.unwrap();
                tracing::info!("peer2 observed: {observed_address:?}");

                assert_eq!(protocol_version, Some("/proto/2".to_string()));
                assert_eq!(user_agent, Some("agent v2".to_string()));

                litep2p1_done = true;

                if litep2p1_done && litep2p2_done {
                    break
                }
            }
            event = identify_event_stream2.next() => {
                let IdentifyEvent::PeerIdentified { observed_address, protocol_version, user_agent, .. } = event.unwrap();
                tracing::info!("peer1 observed: {observed_address:?}");

                assert_eq!(protocol_version, Some("/proto/1".to_string()));
                assert_eq!(user_agent, Some("agent v1".to_string()));

                litep2p2_done = true;

                if litep2p1_done && litep2p2_done {
                    break
                }
            }
        }
    }

    let mut litep2p1_done = false;
    let mut litep2p2_done = false;

    while !litep2p1_done || !litep2p2_done {
        tokio::select! {
            event = litep2p1.next_event() => if let Litep2pEvent::ConnectionClosed { .. } = event.unwrap() {
                litep2p1_done = true;
            },
            event = litep2p2.next_event() => if let Litep2pEvent::ConnectionClosed { .. } = event.unwrap() {
                litep2p2_done = true;
            }
        }
    }
}

#[tokio::test]
async fn identify_not_supported_tcp() {
    identify_not_supported(
        Transport::Tcp(Default::default()),
        Transport::Tcp(Default::default()),
    )
    .await
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn identify_not_supported_quic() {
    identify_not_supported(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn identify_not_supported_websocket() {
    identify_not_supported(
        Transport::WebSocket(Default::default()),
        Transport::WebSocket(Default::default()),
    )
    .await
}

async fn identify_not_supported(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (ping_config, _event_stream) = PingConfig::default();
    let config_builder1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_libp2p_ping(ping_config);
    let config1 = add_transport(config_builder1, transport1).build();

    let (identify_config2, mut identify_event_stream2) = Config::new("litep2p".to_string(), None);
    let config_builder2 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_libp2p_identify(identify_config2);
    let config2 = add_transport(config_builder2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();
    let address = litep2p2.listen_addresses().next().unwrap().clone();

    litep2p1.dial_address(address).await.unwrap();

    let mut litep2p1_done = false;
    let mut litep2p2_done = false;

    while !litep2p1_done || !litep2p2_done {
        tokio::select! {
            event = litep2p1.next_event() => if let Litep2pEvent::ConnectionEstablished { .. } = event.unwrap() {
                tracing::error!("litep2p1 connection established");
                litep2p1_done = true;
            },
            event = litep2p2.next_event() => if let Litep2pEvent::ConnectionEstablished { .. } = event.unwrap() {
                tracing::error!("litep2p2 connection established");
                litep2p2_done = true;
            }
        }
    }

    let mut litep2p1_done = false;
    let mut litep2p2_done = false;

    while !litep2p1_done || !litep2p2_done {
        tokio::select! {
            event = litep2p1.next_event() => if let Litep2pEvent::ConnectionClosed { .. } = event.unwrap() {
                tracing::error!("litep2p1 connection closed");
                litep2p1_done = true;
            },
            event = litep2p2.next_event() => if let Litep2pEvent::ConnectionClosed { .. } = event.unwrap() {
                tracing::error!("litep2p2 connection closed");
                litep2p2_done = true;
            }
        }
    }

    assert!(identify_event_stream2.next().now_or_never().is_none());
}
