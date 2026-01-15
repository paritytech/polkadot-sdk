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

//! This example demonstrates how to implement a custom protocol for litep2p.

use litep2p::{
    codec::ProtocolCodec,
    config::ConfigBuilder,
    protocol::{Direction, TransportEvent, TransportService, UserProtocol},
    types::protocol::ProtocolName,
    Litep2p, PeerId,
};

use bytes::{Buf, BufMut, BytesMut};
use futures::{future::BoxFuture, stream::FuturesUnordered, SinkExt, StreamExt};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio_util::codec::{Decoder, Encoder, Framed};

use std::collections::{hash_map::Entry, HashMap};

#[derive(Debug)]
struct CustomCodec;

impl Decoder for CustomCodec {
    type Item = BytesMut;
    type Error = litep2p::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            return Ok(None);
        }

        let len = src.get_u8() as usize;
        if src.len() >= len {
            let mut out = BytesMut::with_capacity(len);
            out.put_slice(&src[..len]);
            src.advance(len);

            return Ok(Some(out));
        }

        Ok(None)
    }
}

impl Encoder<BytesMut> for CustomCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: BytesMut, dst: &mut BytesMut) -> Result<(), Self::Error> {
        if item.len() > u8::MAX as usize {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }

        dst.put_u8(item.len() as u8);
        dst.extend(&item);

        Ok(())
    }
}

/// Events received from the protocol.
#[derive(Debug)]
enum CustomProtocolEvent {
    /// Received `message` from `peer`.
    MessageReceived {
        /// Peer ID.
        peer: PeerId,

        /// Message.
        message: Vec<u8>,
    },
}

/// Commands sent to the protocol.
#[derive(Debug)]
enum CustomProtocolCommand {
    /// Send `message` to `peer`.
    SendMessage {
        /// Peer ID.
        peer: PeerId,

        /// Message.
        message: Vec<u8>,
    },
}

/// Handle for communicating with the protocol.
#[derive(Debug)]
struct CustomProtocolHandle {
    cmd_tx: Sender<CustomProtocolCommand>,
    event_rx: Receiver<CustomProtocolEvent>,
}

#[derive(Debug)]
struct CustomProtocol {
    /// Channel for receiving commands from user.
    cmd_rx: Receiver<CustomProtocolCommand>,

    /// Channel for sending events to user.
    event_tx: Sender<CustomProtocolEvent>,

    /// Connected peers.
    peers: HashMap<PeerId, Option<Vec<u8>>>,

    /// Active inbound substreams.
    inbound: FuturesUnordered<BoxFuture<'static, (PeerId, Option<litep2p::Result<BytesMut>>)>>,

    /// Active outbound substreams.
    outbound: FuturesUnordered<BoxFuture<'static, litep2p::Result<()>>>,
}

impl CustomProtocol {
    /// Create new [`CustomProtocol`].
    pub fn new() -> (Self, CustomProtocolHandle) {
        let (event_tx, event_rx) = channel(64);
        let (cmd_tx, cmd_rx) = channel(64);

        (
            Self {
                cmd_rx,
                event_tx,
                peers: HashMap::new(),
                inbound: FuturesUnordered::new(),
                outbound: FuturesUnordered::new(),
            },
            CustomProtocolHandle { cmd_tx, event_rx },
        )
    }
}

#[async_trait::async_trait]
impl UserProtocol for CustomProtocol {
    fn protocol(&self) -> ProtocolName {
        ProtocolName::from("/custom-protocol/1")
    }

    // Protocol code is set to `Unspecified` which means that `litep2p` won't provide
    // `Sink + Stream` for the protocol and instead only `AsyncWrite + AsyncRead` are provided.
    // User must implement their custom codec on top of `Substream` using, e.g.,
    // `tokio_codec::Framed` if they want to have message framing.
    fn codec(&self) -> ProtocolCodec {
        ProtocolCodec::Unspecified
    }

    /// Start running event loop for [`CustomProtocol`].
    async fn run(mut self: Box<Self>, mut service: TransportService) -> litep2p::Result<()> {
        loop {
            tokio::select! {
                cmd = self.cmd_rx.recv() => match cmd {
                    Some(CustomProtocolCommand::SendMessage { peer, message }) => {
                        match self.peers.entry(peer) {
                            // peer doens't exist so dial them and save the message
                            Entry::Vacant(entry) => match service.dial(&peer) {
                                Ok(()) => {
                                    entry.insert(Some(message));
                                }
                                Err(error) => {
                                    eprintln!("failed to dial {peer:?}: {error:?}");
                                }
                            }
                            // peer exists so open a new substream
                            Entry::Occupied(mut entry) => match service.open_substream(peer) {
                                Ok(_) => {
                                    entry.insert(Some(message));
                                }
                                Err(error) => {
                                    eprintln!("failed to open substream to {peer:?}: {error:?}");
                                }
                            }
                        }
                    }
                    None => return Err(litep2p::Error::EssentialTaskClosed),
                },
                event = service.next() => match event {
                    // connection established to peer
                    //
                    // check if the peer already exist in the protocol with a pending message
                    // and if yes, open substream to the peer.
                    Some(TransportEvent::ConnectionEstablished { peer, .. }) => {
                        match self.peers.get(&peer) {
                            Some(Some(_)) => {
                                if let Err(error) = service.open_substream(peer) {
                                    println!("failed to open substream to {peer:?}: {error:?}");
                                }
                            }
                            Some(None) => {}
                            None => {
                                self.peers.insert(peer, None);
                            }
                        }
                    }
                    // substream opened
                    //
                    // for inbound substreams, move the substream to `self.inbound` and poll them for messages
                    //
                    // for outbound substreams, move the substream to `self.outbound` and send the saved message to remote peer
                    Some(TransportEvent::SubstreamOpened { peer, substream, direction, .. }) => {
                        match direction {
                            Direction::Inbound => {
                                self.inbound.push(Box::pin(async move {
                                    (peer, Framed::new(substream, CustomCodec).next().await)
                                }));
                            }
                            Direction::Outbound(_) => {
                                let message = self.peers.get_mut(&peer).expect("peer to exist").take().unwrap();

                                self.outbound.push(Box::pin(async move {
                                    let mut framed = Framed::new(substream, CustomCodec);
                                    framed.send(BytesMut::from(&message[..])).await.map_err(From::from)
                                }));
                            }
                        }
                    }
                    // connection closed, remove all peer context
                    Some(TransportEvent::ConnectionClosed { peer }) => {
                        self.peers.remove(&peer);
                    }
                    None => return Err(litep2p::Error::EssentialTaskClosed),
                    _ => {},
                },
                // poll inbound substreams for messages
                event = self.inbound.next(), if !self.inbound.is_empty() => match event {
                    Some((peer, Some(Ok(message)))) => {
                        self.event_tx.send(CustomProtocolEvent::MessageReceived {
                            peer,
                            message: message.into(),
                        }).await.unwrap();
                    }
                    event => eprintln!("failed to read message from an inbound substream: {event:?}"),
                },
                // poll outbound substreams so that they can make progress
                _ = self.outbound.next(), if !self.outbound.is_empty() => {}
            }
        }
    }
}

fn make_litep2p() -> (Litep2p, CustomProtocolHandle) {
    let (custom_protocol, handle) = CustomProtocol::new();

    (
        Litep2p::new(
            ConfigBuilder::new()
                .with_tcp(Default::default())
                .with_user_protocol(Box::new(custom_protocol))
                .build(),
        )
        .unwrap(),
        handle,
    )
}

#[tokio::main]
async fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut litep2p1, handle1) = make_litep2p();
    let (mut litep2p2, mut handle2) = make_litep2p();

    let peer2 = *litep2p2.local_peer_id();
    let listen_address = litep2p2.listen_addresses().next().unwrap().clone();
    litep2p1.add_known_address(peer2, std::iter::once(listen_address));

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = litep2p1.next_event() => {}
                _ = litep2p2.next_event() => {}
            }
        }
    });

    for message in [
        b"hello, world".to_vec(),
        b"testing 123".to_vec(),
        b"goodbye, world".to_vec(),
    ] {
        handle1
            .cmd_tx
            .send(CustomProtocolCommand::SendMessage {
                peer: peer2,
                message,
            })
            .await
            .unwrap();

        let CustomProtocolEvent::MessageReceived { peer, message } =
            handle2.event_rx.recv().await.unwrap();

        println!(
            "received message from {peer:?}: {:?}",
            std::str::from_utf8(&message)
        );
    }
}
