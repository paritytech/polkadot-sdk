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
    Litep2p, PeerId,
};

use futures::StreamExt;
use multiaddr::{Multiaddr, Protocol};
use multihash::Multihash;
use tokio::sync::oneshot;

#[derive(Debug)]
struct CustomProtocol {
    dial_address: Multiaddr,
    protocol: ProtocolName,
    codec: ProtocolCodec,
    tx: oneshot::Sender<()>,
}

impl CustomProtocol {
    pub fn new(dial_address: Multiaddr) -> (Self, oneshot::Receiver<()>) {
        let (tx, rx) = oneshot::channel();

        (
            Self {
                dial_address,
                protocol: ProtocolName::from("/custom-protocol/1"),
                codec: ProtocolCodec::UnsignedVarint(None),
                tx,
            },
            rx,
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
        if service.dial_address(self.dial_address.clone()).is_err() {
            self.tx.send(()).unwrap();
            return Ok(());
        }

        loop {
            while let Some(event) = service.next().await {
                if let TransportEvent::DialFailure { .. } = event {
                    self.tx.send(()).unwrap();
                    return Ok(());
                }
            }
        }
    }
}

#[tokio::test]
async fn protocol_dial_invalid_dns_address() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
    let address = Multiaddr::empty()
        .with(Protocol::Dns(std::borrow::Cow::Owned(
            "address.that.doesnt.exist.hopefully.pls".to_string(),
        )))
        .with(Protocol::Tcp(8888))
        .with(Protocol::P2p(
            Multihash::from_bytes(&PeerId::random().to_bytes()).unwrap(),
        ));

    let (custom_protocol, rx) = CustomProtocol::new(address);
    let custom_protocol = Box::new(custom_protocol);
    let config1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_tcp(TcpConfig {
            ..Default::default()
        })
        .with_user_protocol(custom_protocol)
        .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();

    tokio::spawn(async move {
        loop {
            let _ = litep2p1.next_event().await;
        }
    });

    rx.await.unwrap();
}

#[tokio::test]
async fn protocol_dial_peer_id_missing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
    let address = Multiaddr::empty()
        .with(Protocol::Dns(std::borrow::Cow::Owned(
            "google.com".to_string(),
        )))
        .with(Protocol::Tcp(8888));

    let (custom_protocol, rx) = CustomProtocol::new(address);
    let custom_protocol = Box::new(custom_protocol);
    let config1 = ConfigBuilder::new()
        .with_keypair(Keypair::generate())
        .with_tcp(TcpConfig {
            ..Default::default()
        })
        .with_user_protocol(custom_protocol)
        .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();

    tokio::spawn(async move {
        loop {
            let _ = litep2p1.next_event().await;
        }
    });

    rx.await.unwrap();
}
