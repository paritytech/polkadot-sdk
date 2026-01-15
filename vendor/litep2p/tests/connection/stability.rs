// Copyright 2025 litep2p developers
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
    protocol::{
        libp2p::ping::Config as PingConfig, Direction, TransportEvent, TransportService,
        UserProtocol,
    },
    substream::Substream,
    transport::tcp::config::Config as TcpConfig,
    types::protocol::ProtocolName,
    utils::futures_stream::FuturesStream,
    Litep2p, PeerId,
};

use futures::{future::BoxFuture, StreamExt};

use crate::common::{add_transport, Transport};

const PROTOCOL_NAME: &str = "/litep2p-stability/1.0.0";

const LOG_TARGET: &str = "litep2p::stability";

/// The stability protocol ensures a single transport connection
/// (either TCP or WebSocket) can sustain multiple received packets.
///
/// The scenario puts stress on the internal buffers, ensuring that
/// each layer behave properly.
///
/// ## Protocol Details
///
/// The protocol opens 16 outbound substreams on the connection established event.
/// Therefore, it will handle 16 outbound substreams and 16 inbound substreams
/// (open by the remote).
///
/// The outbound substreams will push a configurable number of packets, each of
/// size 128 bytes, to the remote peer. While the inbound substreams will read
/// the same number of packets from the remote peer.
pub struct StabilityProtocol {
    /// The number of identical packets to send / receive on a substream.
    total_packets: usize,
    inbound: FuturesStream<BoxFuture<'static, Result<(), String>>>,
    outbound: FuturesStream<BoxFuture<'static, Result<(), String>>>,
    /// Peer Id for logging purposes.
    peer_id: PeerId,
    /// The sender to notify the test that the protocol finished.
    tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl StabilityProtocol {
    fn new(total_packets: usize, peer_id: PeerId) -> (Self, tokio::sync::oneshot::Receiver<()>) {
        let (tx, rx) = tokio::sync::oneshot::channel();

        (
            Self {
                total_packets,
                inbound: FuturesStream::new(),
                outbound: FuturesStream::new(),
                peer_id,
                tx: Some(tx),
            },
            rx,
        )
    }

    fn handle_substream(&mut self, mut substream: Substream, direction: Direction) {
        let mut total_packets = self.total_packets;
        match direction {
            Direction::Inbound => {
                self.inbound.push(Box::pin(async move {
                    while total_packets > 0 {
                        let _payload = substream
                            .next()
                            .await
                            .ok_or_else(|| {
                                tracing::warn!(target: LOG_TARGET, "Failed to read None from substream");
                                "Failed to read None from substream".to_string()
                            })?
                            .map_err(|err| {
                                tracing::warn!(target: LOG_TARGET, "Failed to read from substream {:?}", err);
                                "Failed to read from substream".to_string()
                            })?;
                        total_packets -= 1;
                    }

                    Ok(())
                }));
            }
            Direction::Outbound { .. } => {
                self.outbound.push(Box::pin(async move {
                    let mut frame = vec![0; 128];
                    for i in 0..frame.len() {
                        frame[i] = i as u8;
                    }

                    while total_packets > 0 {
                        substream.send_framed(frame.clone().into()).await.map_err(|err| {
                            tracing::warn!("Failed to send to substream {:?}", err);
                            "Failed to send to substream".to_string()
                        })?;
                        total_packets -= 1;
                    }

                    Ok(())
                }));
            }
        }
    }
}

#[async_trait::async_trait]
impl UserProtocol for StabilityProtocol {
    fn protocol(&self) -> ProtocolName {
        PROTOCOL_NAME.into()
    }

    fn codec(&self) -> ProtocolCodec {
        // Similar to the identify payload size.
        ProtocolCodec::UnsignedVarint(Some(4096))
    }

    async fn run(mut self: Box<Self>, mut service: TransportService) -> litep2p::Result<()> {
        let num_substreams = 16;
        let mut handled_substreams = 0;

        loop {
            if handled_substreams == 2 * num_substreams {
                tracing::info!(
                    target: LOG_TARGET,
                    handled_substreams,
                    peer_id = %self.peer_id,
                    "StabilityProtocol finished to handle packets",
                );

                self.tx.take().expect("Send happens only once; qed").send(()).unwrap();
                // If one of the stability protocols finishes, while the
                // the other is still reading data from the stream, the test
                // might race if the substream detects the connection as closed.
                futures::future::pending::<()>().await;
            }

            tokio::select! {
                event = service.next() => match event {
                    Some(TransportEvent::ConnectionEstablished { peer, .. }) => {
                        for i in 0..num_substreams {
                            match service.open_substream(peer) {
                                Ok(_) => {},
                                Err(e) => {
                                    tracing::error!(
                                        target: LOG_TARGET,
                                        ?e,
                                        i,
                                        peer_id = %self.peer_id,
                                        "Failed to open substream"
                                    );
                                    // Drop the tx sender.
                                    return Ok(());
                                }
                            }
                        }
                    }
                    Some(TransportEvent::ConnectionClosed { peer }) => {
                        tracing::error!(
                            target: LOG_TARGET,
                            peer_id = %self.peer_id,
                            "Connection closed unexpectedly: {}",
                            peer
                        );

                        panic!("connection closed");
                    }
                    Some(TransportEvent::SubstreamOpened {
                        substream,
                        direction,
                        ..
                    }) => {
                        self.handle_substream(substream, direction);
                    }
                    _ => {},
                },

                inbound = self.inbound.next(), if !self.inbound.is_empty() => {
                    match inbound {
                        Some(Ok(())) => {
                            handled_substreams += 1;
                        }
                        Some(Err(err)) => {
                            tracing::error!(
                                target: LOG_TARGET,
                                peer_id = %self.peer_id,
                                "Inbound stream failed with error: {}",
                                err
                            );
                            // Drop the tx sender.
                            return Ok(());
                        }
                        None => {
                            tracing::error!(
                                target: LOG_TARGET,
                                peer_id = %self.peer_id,
                                "Inbound stream failed with None",
                            );
                            panic!("Inbound stream failed");
                        }
                    }
                },

                outbound = self.outbound.next(), if !self.outbound.is_empty() => {
                    match outbound {
                        Some(Ok(())) => {
                            handled_substreams += 1;
                        }
                        Some(Err(err)) => {
                            tracing::error!(
                                target: LOG_TARGET,
                                peer_id = %self.peer_id,
                                "Outbound stream failed with error: {}",
                                err
                            );
                            // Drop the tx sender.
                            return Ok(());
                        }
                        None => {
                            tracing::error!(
                                target: LOG_TARGET,
                                peer_id = %self.peer_id,
                                "Outbound stream failed with None",
                            );
                            panic!("Outbound stream failed");
                        }
                    }
                },
            }
        }
    }
}

async fn stability_litep2p_transport(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (ping_config1, _ping_event_stream1) = PingConfig::default();
    let keypair = Keypair::generate();
    let peer_id = keypair.public().to_peer_id();
    let (stability_protocol, mut exit1) = StabilityProtocol::new(1000, peer_id);
    let config1 = ConfigBuilder::new()
        .with_keypair(keypair)
        .with_libp2p_ping(ping_config1)
        .with_user_protocol(Box::new(stability_protocol));

    let config1 = add_transport(config1, transport1).build();

    let (ping_config2, _ping_event_stream2) = PingConfig::default();
    let keypair = Keypair::generate();
    let peer_id = keypair.public().to_peer_id();
    let (stability_protocol, mut exit2) = StabilityProtocol::new(1000, peer_id);
    let config2 = ConfigBuilder::new()
        .with_keypair(keypair)
        .with_libp2p_ping(ping_config2)
        .with_user_protocol(Box::new(stability_protocol));

    let config2 = add_transport(config2, transport2).build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();

    let address = litep2p2.listen_addresses().next().unwrap().clone();
    litep2p1.dial_address(address).await.unwrap();

    let mut litep2p1_exit = false;
    let mut litep2p2_exit = false;
    loop {
        if litep2p1_exit && litep2p2_exit {
            break;
        }

        tokio::select! {
            // Wait for the stability protocols to finish, while keeping
            // the peer connections alive.
            event = &mut exit1, if !litep2p1_exit => {
                if let Ok(()) = event {
                    litep2p1_exit = true;
                } else {
                    panic!("StabilityProtocol 1 failed");
                }
            },
            event = &mut exit2, if !litep2p2_exit => {
                if let Ok(()) = event {
                    litep2p2_exit = true;
                } else {
                    panic!("StabilityProtocol 2 failed");
                }
            },

            // Drive litep2p backends.
            event = litep2p1.next_event() => {
                tracing::info!(target: LOG_TARGET, "litep2p1 event: {:?}", event);
            }
            event = litep2p2.next_event() => {
                tracing::info!(target: LOG_TARGET, "litep2p2 event: {:?}", event);
            }
        }
    }
}

#[tokio::test]
async fn stability_tcp() {
    let transport1 = Transport::Tcp(TcpConfig::default());
    let transport2 = Transport::Tcp(TcpConfig::default());

    stability_litep2p_transport(transport1, transport2).await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn stability_websocket() {
    use litep2p::transport::websocket::config::Config as WebSocketConfig;

    let transport1 = Transport::WebSocket(WebSocketConfig::default());
    let transport2 = Transport::WebSocket(WebSocketConfig::default());

    stability_litep2p_transport(transport1, transport2).await;
}
