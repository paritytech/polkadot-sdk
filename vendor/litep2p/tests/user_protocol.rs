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
    codec::ProtocolCodec,
    config::ConfigBuilder,
    crypto::ed25519::Keypair,
    protocol::{mdns::Config as MdnsConfig, TransportEvent, TransportService, UserProtocol},
    transport::tcp::config::Config as TcpConfig,
    types::protocol::ProtocolName,
    Litep2p, PeerId,
};

use futures::StreamExt;

use std::{collections::HashSet, sync::Arc, time::Duration};

struct CustomProtocol {
    protocol: ProtocolName,
    codec: ProtocolCodec,
    peers: HashSet<PeerId>,
}

impl CustomProtocol {
    pub fn new() -> Self {
        let protocol: Arc<str> = Arc::from(String::from("/custom-protocol/1"));

        Self {
            peers: HashSet::new(),
            protocol: ProtocolName::from(protocol),
            codec: ProtocolCodec::UnsignedVarint(None),
        }
    }
}

#[async_trait::async_trait]
impl UserProtocol for CustomProtocol {
    fn protocol(&self) -> ProtocolName {
        self.protocol.clone()
    }

    fn codec(&self) -> ProtocolCodec {
        self.codec
    }

    async fn run(mut self: Box<Self>, mut service: TransportService) -> litep2p::Result<()> {
        loop {
            while let Some(event) = service.next().await {
                tracing::trace!("received event: {event:?}");

                match event {
                    TransportEvent::ConnectionEstablished { peer, .. } => {
                        self.peers.insert(peer);
                    }
                    TransportEvent::ConnectionClosed { peer } => {
                        self.peers.remove(&peer);
                    }
                    _ => {}
                }
            }
        }
    }
}

#[tokio::test]
async fn user_protocol() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let custom_protocol1 = Box::new(CustomProtocol::new());
    let (mdns_config, _stream) = MdnsConfig::new(Duration::from_secs(30));

    let config1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_tcp(TcpConfig {
            ..Default::default()
        })
        .with_user_protocol(custom_protocol1)
        .with_mdns(mdns_config)
        .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let peer1 = *litep2p1.local_peer_id();
    let listen_address = litep2p1.listen_addresses().next().unwrap().clone();

    let custom_protocol2 = Box::new(CustomProtocol::new());
    let config2 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_tcp(TcpConfig {
            ..Default::default()
        })
        .with_user_protocol(custom_protocol2)
        .with_known_addresses(vec![(peer1, vec![listen_address])].into_iter())
        .with_max_parallel_dials(8usize)
        .build();

    let mut litep2p2 = Litep2p::new(config2).unwrap();
    litep2p2.dial(&peer1).await.unwrap();

    // wait until connection is established
    let mut litep2p1_ready = false;
    let mut litep2p2_ready = false;

    while !litep2p1_ready && !litep2p2_ready {
        tokio::select! {
            event = litep2p1.next_event() => {
                tracing::trace!("litep2p1 event: {event:?}");
                litep2p1_ready = true;
            }
            event = litep2p2.next_event() => {
                tracing::trace!("litep2p2 event: {event:?}");
                litep2p2_ready = true;
            }
        }
    }

    // wait until connection is closed by the keep-alive timeout
    let mut litep2p1_ready = false;
    let mut litep2p2_ready = false;

    while !litep2p1_ready && !litep2p2_ready {
        tokio::select! {
            event = litep2p1.next_event() => {
                tracing::trace!("litep2p1 event: {event:?}");
                litep2p1_ready = true;
            }
            event = litep2p2.next_event() => {
                tracing::trace!("litep2p2 event: {event:?}");
                litep2p2_ready = true;
            }
        }
    }

    let sink = litep2p2.bandwidth_sink();
    tracing::trace!("inbound {}, outbound {}", sink.outbound(), sink.inbound());
}
