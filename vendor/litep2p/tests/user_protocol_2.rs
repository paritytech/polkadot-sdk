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
    protocol::{TransportEvent, TransportService, UserProtocol},
    transport::tcp::config::Config as TcpConfig,
    types::protocol::ProtocolName,
    Litep2p, Litep2pEvent, PeerId,
};

use futures::StreamExt;
use multiaddr::Multiaddr;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use std::collections::HashSet;

struct CustomProtocol {
    protocol: ProtocolName,
    codec: ProtocolCodec,
    peers: HashSet<PeerId>,
    rx: Receiver<Multiaddr>,
}

impl CustomProtocol {
    pub fn new() -> (Self, Sender<Multiaddr>) {
        let (tx, rx) = channel(64);

        (
            Self {
                rx,
                peers: HashSet::new(),
                protocol: ProtocolName::from("/custom-protocol/1"),
                codec: ProtocolCodec::UnsignedVarint(None),
            },
            tx,
        )
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
            tokio::select! {
                event = service.next() => match event.unwrap() {
                    TransportEvent::ConnectionEstablished { peer, .. } => {
                        self.peers.insert(peer);
                    }
                    TransportEvent::ConnectionClosed { peer: _ } => {}
                    TransportEvent::SubstreamOpened {
                        peer: _,
                        protocol: _,
                        direction: _,
                        substream: _,
                        fallback: _,
                    } => {}
                    TransportEvent::SubstreamOpenFailure {
                        substream: _,
                        error: _,
                    } => {}
                    TransportEvent::DialFailure { .. } => {}
                },
                address = self.rx.recv() => {
                    service.dial_address(address.unwrap()).unwrap();
                }
            }
        }
    }
}

#[tokio::test]
async fn user_protocol_2() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (custom_protocol1, sender1) = CustomProtocol::new();
    let config1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_tcp(TcpConfig {
            ..Default::default()
        })
        .with_user_protocol(Box::new(custom_protocol1))
        .build();

    let (custom_protocol2, _sender2) = CustomProtocol::new();
    let config2 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_tcp(TcpConfig {
            ..Default::default()
        })
        .with_user_protocol(Box::new(custom_protocol2))
        .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();
    let address = litep2p2.listen_addresses().next().unwrap().clone();

    sender1.send(address).await.unwrap();

    let mut litep2p1_ready = false;
    let mut litep2p2_ready = false;

    while !litep2p1_ready && !litep2p2_ready {
        tokio::select! {
            event = litep2p1.next_event() => if let Litep2pEvent::ConnectionEstablished { .. } = event.unwrap() {
                litep2p1_ready = true;
            },
            event = litep2p2.next_event() => if let Litep2pEvent::ConnectionEstablished { .. } = event.unwrap() {
                litep2p2_ready = true;
            }
        }
    }
}
