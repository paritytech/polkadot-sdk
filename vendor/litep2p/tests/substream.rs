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
    error::SubstreamError,
    protocol::{Direction, TransportEvent, TransportService, UserProtocol},
    substream::{Substream, SubstreamSet},
    transport::tcp::config::Config as TcpConfig,
    types::{protocol::ProtocolName, SubstreamId},
    Error, Litep2p, Litep2pEvent, PeerId,
};

#[cfg(feature = "quic")]
use litep2p::transport::quic::config::Config as QuicConfig;
#[cfg(feature = "websocket")]
use litep2p::transport::websocket::config::Config as WebSocketConfig;

use bytes::Bytes;
use futures::{Sink, SinkExt, StreamExt};
use tokio::{
    io::AsyncWrite,
    sync::{
        mpsc::{channel, Receiver, Sender},
        oneshot,
    },
};

use std::{
    collections::{HashMap, HashSet},
    io::ErrorKind,
    sync::Arc,
    task::Poll,
};

enum Transport {
    Tcp(TcpConfig),
    #[cfg(feature = "quic")]
    Quic(QuicConfig),
    #[cfg(feature = "websocket")]
    WebSocket(WebSocketConfig),
}

enum Command {
    SendPayloadFramed(PeerId, Vec<u8>, oneshot::Sender<litep2p::Result<()>>),
    SendPayloadSink(PeerId, Vec<u8>, oneshot::Sender<litep2p::Result<()>>),
    SendPayloadAsyncWrite(PeerId, Vec<u8>, oneshot::Sender<litep2p::Result<()>>),
    OpenSubstream(PeerId, oneshot::Sender<()>),
}

struct CustomProtocol {
    protocol: ProtocolName,
    codec: ProtocolCodec,
    peers: HashSet<PeerId>,
    rx: Receiver<Command>,
    pending_opens: HashMap<SubstreamId, (PeerId, oneshot::Sender<()>)>,
    substreams: SubstreamSet<PeerId, Substream>,
}

impl CustomProtocol {
    pub fn new(codec: ProtocolCodec) -> (Self, Sender<Command>) {
        let protocol: Arc<str> = Arc::from(String::from("/custom-protocol/1"));
        let (tx, rx) = channel(64);

        (
            Self {
                peers: HashSet::new(),
                protocol: ProtocolName::from(protocol),
                codec,
                rx,
                pending_opens: HashMap::new(),
                substreams: SubstreamSet::new(),
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
                    TransportEvent::ConnectionClosed { peer } => {
                        self.peers.remove(&peer);
                    }
                    TransportEvent::SubstreamOpened {
                        peer,
                        substream,
                        direction,
                        ..
                    } => {
                        self.substreams.insert(peer, substream);

                        if let Direction::Outbound(substream_id) = direction {
                            self.pending_opens.remove(&substream_id).unwrap().1.send(()).unwrap();
                        }
                    }
                    _ => {}
                },
                event = self.substreams.next() => match event {
                    None => panic!("`SubstreamSet` returned `None`"),
                    Some((peer, Err(_))) => {
                        if let Some(mut substream) = self.substreams.remove(&peer) {
                            futures::future::poll_fn(|cx| {
                                match futures::ready!(Sink::poll_close(Pin::new(&mut substream), cx)) {
                                    _ => Poll::Ready(()),
                                }
                            }).await;
                        }
                    }
                    Some((peer, Ok(_))) => {
                        if let Some(mut substream) = self.substreams.remove(&peer) {
                            futures::future::poll_fn(|cx| {
                                match futures::ready!(Sink::poll_close(Pin::new(&mut substream), cx)) {
                                    _ => Poll::Ready(()),
                                }
                            }).await;
                        }
                    },
                },
                command = self.rx.recv() => match command.unwrap() {
                    Command::SendPayloadFramed(peer, payload, tx) => {
                        match self.substreams.remove(&peer) {
                            None => {
                                tx.send(Err(Error::PeerDoesntExist(peer))).unwrap();
                            }
                            Some(mut substream) => {
                                let payload = Bytes::from(payload);
                                let res = substream.send_framed(payload).await.map_err(Into::into);
                                tx.send(res).unwrap();
                                let _ = substream.close().await;
                            }
                        }
                    }
                    Command::SendPayloadSink(peer, payload, tx) => {
                        match self.substreams.remove(&peer) {
                            None => {
                                tx.send(Err(Error::PeerDoesntExist(peer))).unwrap();
                            }
                            Some(mut substream) => {
                                let payload = Bytes::from(payload);
                                let res = substream.send(payload).await.map_err(Into::into);
                                tx.send(res).unwrap();
                                let _ = substream.close().await;
                            }
                        }
                    }
                    Command::SendPayloadAsyncWrite(peer, payload, tx) => {
                        match self.substreams.remove(&peer) {
                            None => {
                                tx.send(Err(Error::PeerDoesntExist(peer))).unwrap();
                            }
                            Some(mut substream) => {
                                let res = futures::future::poll_fn(|cx| {
                                    if let Err(error) = futures::ready!(Pin::new(&mut substream).poll_write(cx, &payload)) {
                                        return Poll::Ready(Err(error.into()));
                                    }

                                    if let Err(error) = futures::ready!(tokio::io::AsyncWrite::poll_flush(
                                        Pin::new(&mut substream),
                                        cx
                                    )) {
                                        return Poll::Ready(Err(error.into()));
                                    }

                                    if let Err(error) = futures::ready!(tokio::io::AsyncWrite::poll_shutdown(
                                        Pin::new(&mut substream),
                                        cx
                                    )) {
                                        return Poll::Ready(Err(error.into()));
                                    }

                                    Poll::Ready(Ok(()))
                                })
                                .await;
                                tx.send(res).unwrap();
                            }
                        }
                    }
                    Command::OpenSubstream(peer, tx) => {
                        let substream_id = service.open_substream(peer).unwrap();
                        self.pending_opens.insert(substream_id, (peer, tx));
                    }
                }
            }
        }
    }
}

async fn connect_peers(litep2p1: &mut Litep2p, litep2p2: &mut Litep2p) {
    let listen_address = litep2p1.listen_addresses().next().unwrap().clone();
    litep2p2.dial_address(listen_address).await.unwrap();

    let mut litep2p1_ready = false;
    let mut litep2p2_ready = false;

    while !litep2p1_ready && !litep2p2_ready {
        tokio::select! {
            event = litep2p1.next_event() => if let Litep2pEvent::ConnectionEstablished { .. } = event.unwrap() { litep2p1_ready = true },
            event = litep2p2.next_event() => if let Litep2pEvent::ConnectionEstablished { .. } = event.unwrap() { litep2p2_ready = true },
        }
    }
}

#[tokio::test]
async fn too_big_identity_payload_framed_tcp() {
    too_big_identity_payload_framed(
        Transport::Tcp(Default::default()),
        Transport::Tcp(Default::default()),
    )
    .await;
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn too_big_identity_payload_framed_quic() {
    too_big_identity_payload_framed(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn too_big_identity_payload_framed_websocket() {
    too_big_identity_payload_framed(
        Transport::WebSocket(Default::default()),
        Transport::WebSocket(Default::default()),
    )
    .await;
}

// send too big payload using `Substream::send_framed()` and verify it's rejected
async fn too_big_identity_payload_framed(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (custom_protocol1, tx1) = CustomProtocol::new(ProtocolCodec::Identity(10usize));
    let config1 = match transport1 {
        Transport::Tcp(config) => ConfigBuilder::new().with_tcp(config),
        #[cfg(feature = "quic")]
        Transport::Quic(config) => ConfigBuilder::new().with_quic(config),
        #[cfg(feature = "websocket")]
        Transport::WebSocket(config) => ConfigBuilder::new().with_websocket(config),
    }
    .with_user_protocol(Box::new(custom_protocol1))
    .build();

    let (custom_protocol2, _tx2) = CustomProtocol::new(ProtocolCodec::Identity(10usize));
    let config2 = match transport2 {
        Transport::Tcp(config) => ConfigBuilder::new().with_tcp(config),
        #[cfg(feature = "quic")]
        Transport::Quic(config) => ConfigBuilder::new().with_quic(config),
        #[cfg(feature = "websocket")]
        Transport::WebSocket(config) => ConfigBuilder::new().with_websocket(config),
    }
    .with_user_protocol(Box::new(custom_protocol2))
    .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();
    let peer2 = *litep2p2.local_peer_id();

    // connect peers and start event loops for litep2ps
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _event = litep2p1.next_event() => {}
                _event = litep2p2.next_event() => {}
            }
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    // open substream to peer
    let (tx, rx) = oneshot::channel();
    tx1.send(Command::OpenSubstream(peer2, tx)).await.unwrap();

    let Ok(()) = rx.await else {
        panic!("failed to open substream");
    };

    // send too large paylod to peer
    let (tx, rx) = oneshot::channel();
    tx1.send(Command::SendPayloadFramed(peer2, vec![0u8; 16], tx)).await.unwrap();

    match rx.await {
        Ok(Err(Error::IoError(ErrorKind::PermissionDenied))) => {}
        Ok(Err(Error::SubstreamError(SubstreamError::IoError(ErrorKind::PermissionDenied)))) => {}
        event => panic!("invalid event received: {event:?}"),
    }
}

#[tokio::test]
async fn too_big_identity_payload_sink_tcp() {
    too_big_identity_payload_sink(
        Transport::Tcp(Default::default()),
        Transport::Tcp(Default::default()),
    )
    .await;
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn too_big_identity_payload_sink_quic() {
    too_big_identity_payload_sink(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn too_big_identity_payload_sink_websocket() {
    too_big_identity_payload_sink(
        Transport::WebSocket(Default::default()),
        Transport::WebSocket(Default::default()),
    )
    .await;
}

// send too big payload using `<Substream as Sink>::send()` and verify it's rejected
async fn too_big_identity_payload_sink(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (custom_protocol1, tx1) = CustomProtocol::new(ProtocolCodec::Identity(10usize));
    let config1 = match transport1 {
        Transport::Tcp(config) => ConfigBuilder::new().with_tcp(config),
        #[cfg(feature = "quic")]
        Transport::Quic(config) => ConfigBuilder::new().with_quic(config),
        #[cfg(feature = "websocket")]
        Transport::WebSocket(config) => ConfigBuilder::new().with_websocket(config),
    }
    .with_user_protocol(Box::new(custom_protocol1))
    .build();

    let (custom_protocol2, _tx2) = CustomProtocol::new(ProtocolCodec::Identity(10usize));
    let config2 = match transport2 {
        Transport::Tcp(config) => ConfigBuilder::new().with_tcp(config),
        #[cfg(feature = "quic")]
        Transport::Quic(config) => ConfigBuilder::new().with_quic(config),
        #[cfg(feature = "websocket")]
        Transport::WebSocket(config) => ConfigBuilder::new().with_websocket(config),
    }
    .with_user_protocol(Box::new(custom_protocol2))
    .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();
    let peer2 = *litep2p2.local_peer_id();

    // connect peers and start event loops for litep2ps
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _event = litep2p1.next_event() => {}
                _event = litep2p2.next_event() => {}
            }
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    {
        // open substream to peer
        let (tx, rx) = oneshot::channel();
        tx1.send(Command::OpenSubstream(peer2, tx)).await.unwrap();

        let Ok(()) = rx.await else {
            panic!("failed to open substream");
        };

        // send too large payload to peer
        let (tx, rx) = oneshot::channel();
        tx1.send(Command::SendPayloadSink(peer2, vec![0u8; 16], tx)).await.unwrap();

        match rx.await {
            Ok(Err(Error::IoError(ErrorKind::PermissionDenied))) => {}
            Ok(Err(Error::SubstreamError(SubstreamError::IoError(
                ErrorKind::PermissionDenied,
            )))) => {}
            event => panic!("invalid event received: {event:?}"),
        }
    }
}

#[tokio::test]
async fn correct_payload_size_sink_tcp() {
    correct_payload_size_sink(
        Transport::Tcp(Default::default()),
        Transport::Tcp(Default::default()),
    )
    .await;
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn correct_payload_size_sink_quic() {
    correct_payload_size_sink(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn correct_payload_size_sink_websocket() {
    correct_payload_size_sink(
        Transport::WebSocket(Default::default()),
        Transport::WebSocket(Default::default()),
    )
    .await;
}

// send correctly-sized payload using `<Substream as Sink>::send()`
async fn correct_payload_size_sink(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (custom_protocol1, tx1) = CustomProtocol::new(ProtocolCodec::Identity(10usize));
    let config1 = match transport1 {
        Transport::Tcp(config) => ConfigBuilder::new().with_tcp(config),
        #[cfg(feature = "quic")]
        Transport::Quic(config) => ConfigBuilder::new().with_quic(config),
        #[cfg(feature = "websocket")]
        Transport::WebSocket(config) => ConfigBuilder::new().with_websocket(config),
    }
    .with_user_protocol(Box::new(custom_protocol1))
    .build();

    let (custom_protocol2, _tx2) = CustomProtocol::new(ProtocolCodec::Identity(10usize));
    let config2 = match transport2 {
        Transport::Tcp(config) => ConfigBuilder::new().with_tcp(config),
        #[cfg(feature = "quic")]
        Transport::Quic(config) => ConfigBuilder::new().with_quic(config),
        #[cfg(feature = "websocket")]
        Transport::WebSocket(config) => ConfigBuilder::new().with_websocket(config),
    }
    .with_user_protocol(Box::new(custom_protocol2))
    .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();
    let peer2 = *litep2p2.local_peer_id();

    // connect peers and start event loops for litep2ps
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _event = litep2p1.next_event() => {}
                _event = litep2p2.next_event() => {}
            }
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    // open substream to peer
    let (tx, rx) = oneshot::channel();
    tx1.send(Command::OpenSubstream(peer2, tx)).await.unwrap();

    let Ok(()) = rx.await else {
        panic!("failed to open substream");
    };

    let (tx, rx) = oneshot::channel();
    tx1.send(Command::SendPayloadSink(peer2, vec![0u8; 10], tx)).await.unwrap();

    match rx.await {
        Ok(_) => {}
        event => panic!("invalid event received: {event:?}"),
    }
}

#[tokio::test]
async fn correct_payload_size_async_write_tcp() {
    correct_payload_size_async_write(
        Transport::Tcp(Default::default()),
        Transport::Tcp(Default::default()),
    )
    .await;
}

#[cfg(feature = "quic")]
#[tokio::test]
async fn correct_payload_size_async_write_quic() {
    correct_payload_size_async_write(
        Transport::Quic(Default::default()),
        Transport::Quic(Default::default()),
    )
    .await;
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn correct_payload_size_async_write_websocket() {
    correct_payload_size_async_write(
        Transport::WebSocket(Default::default()),
        Transport::WebSocket(Default::default()),
    )
    .await;
}

// send correctly-sized payload using `<Substream as AsyncRead>::poll_write()`
async fn correct_payload_size_async_write(transport1: Transport, transport2: Transport) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (custom_protocol1, tx1) = CustomProtocol::new(ProtocolCodec::Identity(10usize));
    let config1 = match transport1 {
        Transport::Tcp(config) => ConfigBuilder::new().with_tcp(config),
        #[cfg(feature = "quic")]
        Transport::Quic(config) => ConfigBuilder::new().with_quic(config),
        #[cfg(feature = "websocket")]
        Transport::WebSocket(config) => ConfigBuilder::new().with_websocket(config),
    }
    .with_user_protocol(Box::new(custom_protocol1))
    .build();

    let (custom_protocol2, _tx2) = CustomProtocol::new(ProtocolCodec::Identity(10usize));
    let config2 = match transport2 {
        Transport::Tcp(config) => ConfigBuilder::new().with_tcp(config),
        #[cfg(feature = "quic")]
        Transport::Quic(config) => ConfigBuilder::new().with_quic(config),
        #[cfg(feature = "websocket")]
        Transport::WebSocket(config) => ConfigBuilder::new().with_websocket(config),
    }
    .with_user_protocol(Box::new(custom_protocol2))
    .build();

    let mut litep2p1 = Litep2p::new(config1).unwrap();
    let mut litep2p2 = Litep2p::new(config2).unwrap();
    let peer2 = *litep2p2.local_peer_id();

    // connect peers and start event loops for litep2ps
    connect_peers(&mut litep2p1, &mut litep2p2).await;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _event = litep2p1.next_event() => {}
                _event = litep2p2.next_event() => {}
            }
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    // open substream to peer
    let (tx, rx) = oneshot::channel();
    tx1.send(Command::OpenSubstream(peer2, tx)).await.unwrap();

    let Ok(()) = rx.await else {
        panic!("failed to open substream");
    };

    let (tx, rx) = oneshot::channel();
    tx1.send(Command::SendPayloadAsyncWrite(peer2, vec![0u8; 10], tx))
        .await
        .unwrap();

    match rx.await {
        Ok(_) => {}
        event => panic!("invalid event received: {event:?}"),
    }
}
