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

//! WebRTC transport.

use crate::{
    error::{AddressError, Error},
    transport::{
        manager::TransportHandle,
        webrtc::{config::Config, connection::WebRtcConnection, opening::OpeningWebRtcConnection},
        Endpoint, Transport, TransportBuilder, TransportEvent,
    },
    types::ConnectionId,
    PeerId,
};

use futures::{future::BoxFuture, Future, Stream};
use futures_timer::Delay;
use hickory_resolver::TokioResolver;
use multiaddr::{multihash::Multihash, Multiaddr, Protocol};
use socket2::{Domain, Socket, Type};
use str0m::{
    channel::{ChannelConfig, ChannelId},
    config::{CryptoProvider, DtlsCert, DtlsCertOptions},
    ice::IceCreds,
    net::{DatagramRecv, Protocol as Str0mProtocol, Receive},
    Candidate, DtlsCertConfig, Input, Rtc,
};

use tokio::{
    io::ReadBuf,
    net::UdpSocket,
    sync::mpsc::{channel, error::TrySendError, Sender},
};

use std::{
    collections::{HashMap, VecDeque},
    net::{IpAddr, SocketAddr},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::{Duration, Instant},
};

pub(crate) use substream::Substream;

mod connection;
mod opening;
mod substream;
mod util;

pub mod config;

pub(super) mod schema {
    pub(super) mod webrtc {
        include!(concat!(env!("OUT_DIR"), "/webrtc.rs"));
    }

    pub(super) mod noise {
        include!(concat!(env!("OUT_DIR"), "/noise.rs"));
    }
}

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::webrtc";

/// Hardcoded remote fingerprint.
const REMOTE_FINGERPRINT: &str =
    "sha-256 FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF";

/// Connection context.
struct ConnectionContext {
    /// Remote peer ID.
    peer: PeerId,

    /// Connection ID.
    connection_id: ConnectionId,

    /// TX channel for sending datagrams to the connection event loop.
    tx: Sender<Vec<u8>>,
}

/// Events received from opening connections that are handled
/// by the [`WebRtcTransport`] event loop.
enum ConnectionEvent {
    /// Connection established.
    ConnectionEstablished {
        /// Remote peer ID.
        peer: PeerId,

        /// Endpoint.
        endpoint: Endpoint,
    },

    /// Connection to peer closed.
    ConnectionClosed,

    /// Timeout.
    Timeout {
        /// Timeout duration.
        duration: Duration,
    },
}

/// WebRTC transport.
pub(crate) struct WebRtcTransport {
    /// Transport context.
    context: TransportHandle,

    /// UDP socket.
    socket: Arc<UdpSocket>,

    /// DTLS certificate.
    dtls_cert: DtlsCert,

    /// Assigned listen addresss.
    listen_address: SocketAddr,

    /// Datagram buffer size.
    datagram_buffer_size: usize,

    /// Connected peers.
    open: HashMap<SocketAddr, ConnectionContext>,

    /// OpeningWebRtc connections.
    opening: HashMap<SocketAddr, OpeningWebRtcConnection>,

    /// `ConnectionId -> SocketAddr` mappings.
    connections: HashMap<ConnectionId, (PeerId, SocketAddr, Endpoint)>,

    /// Pending timeouts.
    timeouts: HashMap<SocketAddr, BoxFuture<'static, ()>>,

    /// Pending events.
    pending_events: VecDeque<TransportEvent>,
}

impl WebRtcTransport {
    /// Extract socket address and `PeerId`, if found, from `address`.
    fn get_socket_address(address: &Multiaddr) -> crate::Result<(SocketAddr, Option<PeerId>)> {
        tracing::trace!(target: LOG_TARGET, ?address, "parse multi address");

        let mut iter = address.iter();
        let socket_address = match iter.next() {
            Some(Protocol::Ip6(address)) => match iter.next() {
                Some(Protocol::Udp(port)) => SocketAddr::new(IpAddr::V6(address), port),
                protocol => {
                    tracing::error!(
                        target: LOG_TARGET,
                        ?protocol,
                        "invalid transport protocol, expected `Upd`",
                    );
                    return Err(Error::AddressError(AddressError::InvalidProtocol));
                }
            },
            Some(Protocol::Ip4(address)) => match iter.next() {
                Some(Protocol::Udp(port)) => SocketAddr::new(IpAddr::V4(address), port),
                protocol => {
                    tracing::error!(
                        target: LOG_TARGET,
                        ?protocol,
                        "invalid transport protocol, expected `Udp`",
                    );
                    return Err(Error::AddressError(AddressError::InvalidProtocol));
                }
            },
            protocol => {
                tracing::error!(target: LOG_TARGET, ?protocol, "invalid transport protocol");
                return Err(Error::AddressError(AddressError::InvalidProtocol));
            }
        };

        match iter.next() {
            Some(Protocol::WebRTC) => {}
            protocol => {
                tracing::error!(
                    target: LOG_TARGET,
                    ?protocol,
                    "invalid protocol, expected `WebRTC`"
                );
                return Err(Error::AddressError(AddressError::InvalidProtocol));
            }
        }

        let maybe_peer = match iter.next() {
            Some(Protocol::P2p(multihash)) => Some(PeerId::from_multihash(multihash)?),
            None => None,
            protocol => {
                tracing::error!(
                    target: LOG_TARGET,
                    ?protocol,
                    "invalid protocol, expected `P2p` or `None`"
                );
                return Err(Error::AddressError(AddressError::InvalidProtocol));
            }
        };

        Ok((socket_address, maybe_peer))
    }

    /// Create RTC client and open channel for Noise handshake.
    fn make_rtc_client(
        &self,
        ufrag: &str,
        pass: &str,
        source: SocketAddr,
        destination: SocketAddr,
    ) -> (Rtc, ChannelId) {
        let mut rtc = Rtc::builder()
            .set_ice_lite(true)
            .set_dtls_cert_config(DtlsCertConfig::PregeneratedCert(self.dtls_cert.clone()))
            .set_fingerprint_verification(false)
            .build();
        rtc.add_local_candidate(Candidate::host(destination, Str0mProtocol::Udp).unwrap());
        rtc.add_remote_candidate(Candidate::host(source, Str0mProtocol::Udp).unwrap());
        rtc.direct_api()
            .set_remote_fingerprint(REMOTE_FINGERPRINT.parse().expect("parse() to succeed"));
        rtc.direct_api().set_remote_ice_credentials(IceCreds {
            ufrag: ufrag.to_owned(),
            pass: pass.to_owned(),
        });
        rtc.direct_api().set_local_ice_credentials(IceCreds {
            ufrag: ufrag.to_owned(),
            pass: pass.to_owned(),
        });
        rtc.direct_api().set_ice_controlling(false);
        rtc.direct_api().start_dtls(false).unwrap();
        rtc.direct_api().start_sctp(false);

        let noise_channel_id = rtc.direct_api().create_data_channel(ChannelConfig {
            label: "noise".to_string(),
            ordered: false,
            reliability: Default::default(),
            negotiated: Some(0),
            protocol: "".to_string(),
        });

        (rtc, noise_channel_id)
    }

    /// Poll opening connection.
    fn poll_connection(&mut self, source: &SocketAddr) -> ConnectionEvent {
        let Some(connection) = self.opening.get_mut(source) else {
            tracing::warn!(
                target: LOG_TARGET,
                ?source,
                "connection doesn't exist",
            );
            return ConnectionEvent::ConnectionClosed;
        };

        loop {
            match connection.poll_process() {
                opening::WebRtcEvent::Timeout { timeout } => {
                    let duration = timeout - Instant::now();

                    match duration.is_zero() {
                        true => match connection.on_timeout() {
                            Ok(()) => continue,
                            Err(error) => {
                                tracing::debug!(
                                    target: LOG_TARGET,
                                    ?source,
                                    ?error,
                                    "failed to handle timeout",
                                );

                                return ConnectionEvent::ConnectionClosed;
                            }
                        },
                        false => return ConnectionEvent::Timeout { duration },
                    }
                }
                opening::WebRtcEvent::Transmit {
                    destination,
                    datagram,
                } =>
                    if let Err(error) = self.socket.try_send_to(&datagram, destination) {
                        tracing::warn!(
                            target: LOG_TARGET,
                            ?source,
                            ?error,
                            "failed to send datagram",
                        );
                    },
                opening::WebRtcEvent::ConnectionClosed => return ConnectionEvent::ConnectionClosed,
                opening::WebRtcEvent::ConnectionOpened { peer, endpoint } => {
                    return ConnectionEvent::ConnectionEstablished { peer, endpoint };
                }
            }
        }
    }

    /// Handle socket input.
    ///
    /// If the datagram was received from an active client, it's dispatched to the connection
    /// handler, if there is space in the queue. If the datagram opened a new connection or it
    /// belonged to a client who is opening, the event loop is instructed to poll the client
    /// until it timeouts.
    ///
    /// Returns `true` if the client should be polled.
    fn on_socket_input(&mut self, source: SocketAddr, buffer: Vec<u8>) -> crate::Result<bool> {
        if let Some(ConnectionContext {
            peer,
            connection_id,
            tx,
        }) = self.open.get_mut(&source)
        {
            match tx.try_send(buffer) {
                Ok(_) => return Ok(false),
                Err(TrySendError::Full(_)) => {
                    tracing::warn!(
                        target: LOG_TARGET,
                        ?source,
                        ?peer,
                        ?connection_id,
                        "channel full, dropping datagram",
                    );

                    return Ok(false);
                }
                Err(TrySendError::Closed(_)) => return Ok(false),
            }
        }

        if buffer.is_empty() {
            // str0m crate panics if the buffer doesn't contain at least one byte:
            // https://github.com/algesten/str0m/blob/2c5dc8ee8ddead08699dd6852a27476af6992a5c/src/io/mod.rs#L222
            return Err(Error::InvalidData);
        }

        // if the peer doesn't exist, decode the message and expect to receive `Stun`
        // so that a new connection can be initialized
        let contents: DatagramRecv =
            buffer.as_slice().try_into().map_err(|_| Error::InvalidData)?;

        // Handle non stun packets.
        if !is_stun_packet(&buffer) {
            tracing::debug!(
                target: LOG_TARGET,
                ?source,
                "received non-stun message"
            );

            if let Err(error) = self.opening.get_mut(&source).expect("to exist").on_input(contents)
            {
                tracing::error!(
                    target: LOG_TARGET,
                    ?error,
                    ?source,
                    "failed to handle inbound datagram"
                );
            }
            return Ok(true);
        }

        let stun_message =
            str0m::ice::StunMessage::parse(&buffer).map_err(|_| Error::InvalidData)?;
        let Some((ufrag, pass)) = stun_message.split_username() else {
            tracing::warn!(
                target: LOG_TARGET,
                ?source,
                "failed to split username/password",
            );
            return Err(Error::InvalidData);
        };

        tracing::debug!(
            target: LOG_TARGET,
            ?source,
            ?ufrag,
            ?pass,
            "received stun message"
        );

        // create new `Rtc` object for the peer and give it the received STUN message
        let (mut rtc, noise_channel_id) =
            self.make_rtc_client(ufrag, pass, source, self.socket.local_addr().unwrap());

        rtc.handle_input(Input::Receive(
            Instant::now(),
            Receive {
                source,
                proto: Str0mProtocol::Udp,
                destination: self.socket.local_addr().unwrap(),
                contents,
            },
        ))
        .expect("client to handle input successfully");

        let connection_id = self.context.next_connection_id();
        let connection = OpeningWebRtcConnection::new(
            rtc,
            connection_id,
            noise_channel_id,
            self.context.keypair.clone(),
            source,
            self.listen_address,
        );
        self.opening.insert(source, connection);

        Ok(true)
    }
}

impl TransportBuilder for WebRtcTransport {
    type Config = Config;
    type Transport = WebRtcTransport;

    /// Create new [`Transport`] object.
    fn new(
        context: TransportHandle,
        config: Self::Config,
        _resolver: Arc<TokioResolver>,
    ) -> crate::Result<(Self, Vec<Multiaddr>)>
    where
        Self: Sized,
    {
        tracing::info!(
            target: LOG_TARGET,
            listen_addresses = ?config.listen_addresses,
            "start webrtc transport",
        );

        let (listen_address, _) = Self::get_socket_address(&config.listen_addresses[0])?;

        let socket = if listen_address.is_ipv4() {
            let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(socket2::Protocol::UDP))?;
            socket.bind(&listen_address.into())?;
            socket
        } else {
            let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(socket2::Protocol::UDP))?;
            socket.set_only_v6(true)?;
            socket.bind(&listen_address.into())?;
            socket
        };

        socket.set_reuse_address(true)?;
        socket.set_nonblocking(true)?;
        #[cfg(unix)]
        socket.set_reuse_port(true)?;

        let socket = UdpSocket::from_std(socket.into())?;
        let listen_address = socket.local_addr()?;
        let dtls_cert = DtlsCert::new(CryptoProvider::OpenSsl, DtlsCertOptions::default());

        let listen_multi_addresses = {
            let fingerprint = dtls_cert.fingerprint().bytes;

            const MULTIHASH_SHA256_CODE: u64 = 0x12;
            let certificate = Multihash::wrap(MULTIHASH_SHA256_CODE, &fingerprint)
                .expect("fingerprint's len to be 32 bytes");

            vec![Multiaddr::empty()
                .with(Protocol::from(listen_address.ip()))
                .with(Protocol::Udp(listen_address.port()))
                .with(Protocol::WebRTC)
                .with(Protocol::Certhash(certificate))]
        };

        Ok((
            Self {
                context,
                dtls_cert,
                listen_address,
                open: HashMap::new(),
                opening: HashMap::new(),
                connections: HashMap::new(),
                socket: Arc::new(socket),
                timeouts: HashMap::new(),
                pending_events: VecDeque::new(),
                datagram_buffer_size: config.datagram_buffer_size,
            },
            listen_multi_addresses,
        ))
    }
}

impl Transport for WebRtcTransport {
    fn dial(&mut self, connection_id: ConnectionId, address: Multiaddr) -> crate::Result<()> {
        tracing::warn!(
            target: LOG_TARGET,
            ?connection_id,
            ?address,
            "webrtc cannot dial",
        );

        debug_assert!(false);
        Err(Error::NotSupported("webrtc cannot dial peers".to_string()))
    }

    fn accept_pending(&mut self, connection_id: ConnectionId) -> crate::Result<()> {
        tracing::trace!(
            target: LOG_TARGET,
            ?connection_id,
            "webrtc cannot accept pending connections",
        );

        debug_assert!(false);
        Err(Error::NotSupported(
            "webrtc cannot accept pending connections".to_string(),
        ))
    }

    fn reject_pending(&mut self, connection_id: ConnectionId) -> crate::Result<()> {
        tracing::trace!(
            target: LOG_TARGET,
            ?connection_id,
            "webrtc cannot reject pending connections",
        );

        debug_assert!(false);
        Err(Error::NotSupported(
            "webrtc cannot reject pending connections".to_string(),
        ))
    }

    fn accept(&mut self, connection_id: ConnectionId) -> crate::Result<()> {
        tracing::trace!(
            target: LOG_TARGET,
            ?connection_id,
            "inbound connection accepted",
        );

        let (peer, source, endpoint) =
            self.connections.remove(&connection_id).ok_or_else(|| {
                tracing::warn!(
                    target: LOG_TARGET,
                    ?connection_id,
                    "pending connection doens't exist",
                );

                Error::InvalidState
            })?;

        let connection = self.opening.remove(&source).ok_or_else(|| {
            tracing::warn!(
                target: LOG_TARGET,
                ?connection_id,
                "pending connection doens't exist",
            );

            Error::InvalidState
        })?;

        let rtc = connection.on_accept()?;
        let (tx, rx) = channel(self.datagram_buffer_size);
        let protocol_set = self.context.protocol_set(connection_id);
        let connection_id = endpoint.connection_id();

        let connection = WebRtcConnection::new(
            rtc,
            peer,
            source,
            self.listen_address,
            Arc::clone(&self.socket),
            protocol_set,
            endpoint,
            rx,
        );
        self.open.insert(
            source,
            ConnectionContext {
                tx,
                peer,
                connection_id,
            },
        );

        self.context.executor.run(Box::pin(async move {
            connection.run().await;
        }));

        Ok(())
    }

    fn reject(&mut self, connection_id: ConnectionId) -> crate::Result<()> {
        tracing::trace!(
            target: LOG_TARGET,
            ?connection_id,
            "inbound connection rejected",
        );

        let (_, source, _) = self.connections.remove(&connection_id).ok_or_else(|| {
            tracing::warn!(
                target: LOG_TARGET,
                ?connection_id,
                "pending connection doens't exist",
            );

            Error::InvalidState
        })?;

        self.opening
            .remove(&source)
            .ok_or_else(|| {
                tracing::warn!(
                    target: LOG_TARGET,
                    ?connection_id,
                    "pending connection doens't exist",
                );

                Error::InvalidState
            })
            .map(|_| ())
    }

    fn open(
        &mut self,
        _connection_id: ConnectionId,
        _addresses: Vec<Multiaddr>,
    ) -> crate::Result<()> {
        Ok(())
    }

    fn negotiate(&mut self, _connection_id: ConnectionId) -> crate::Result<()> {
        Ok(())
    }

    fn cancel(&mut self, _connection_id: ConnectionId) {}
}

impl Stream for WebRtcTransport {
    type Item = TransportEvent;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = Pin::into_inner(self);

        if let Some(event) = this.pending_events.pop_front() {
            return Poll::Ready(Some(event));
        }

        loop {
            let mut buf = vec![0u8; 16384];
            let mut read_buf = ReadBuf::new(&mut buf);

            match this.socket.poll_recv_from(cx, &mut read_buf) {
                Poll::Pending => break,
                Poll::Ready(Err(error)) => {
                    tracing::info!(
                        target: LOG_TARGET,
                        ?error,
                        "webrtc udp socket closed",
                    );

                    return Poll::Ready(None);
                }
                Poll::Ready(Ok(source)) => {
                    let nread = read_buf.filled().len();
                    buf.truncate(nread);

                    match this.on_socket_input(source, buf) {
                        Ok(false) => {}
                        Ok(true) => loop {
                            match this.poll_connection(&source) {
                                ConnectionEvent::ConnectionEstablished { peer, endpoint } => {
                                    this.connections.insert(
                                        endpoint.connection_id(),
                                        (peer, source, endpoint.clone()),
                                    );

                                    // keep polling the connection until it registers a timeout
                                    this.pending_events.push_back(
                                        TransportEvent::ConnectionEstablished { peer, endpoint },
                                    );
                                }
                                ConnectionEvent::ConnectionClosed => {
                                    this.opening.remove(&source);
                                    this.timeouts.remove(&source);

                                    break;
                                }
                                ConnectionEvent::Timeout { duration } => {
                                    this.timeouts.insert(
                                        source,
                                        Box::pin(async move { Delay::new(duration).await }),
                                    );

                                    break;
                                }
                            }
                        },
                        Err(error) => {
                            tracing::debug!(
                                target: LOG_TARGET,
                                ?source,
                                ?error,
                                "failed to handle datagram",
                            );
                        }
                    }
                }
            }
        }

        // go over all pending timeouts to see if any of them have expired
        // and if any of them have, poll the connection until it registers another timeout
        let pending_events = this
            .timeouts
            .iter_mut()
            .filter_map(|(source, mut delay)| match Pin::new(&mut delay).poll(cx) {
                Poll::Pending => None,
                Poll::Ready(_) => Some(*source),
            })
            .collect::<Vec<_>>()
            .into_iter()
            .filter_map(|source| {
                let mut pending_event = None;

                loop {
                    match this.poll_connection(&source) {
                        ConnectionEvent::ConnectionEstablished { peer, endpoint } => {
                            this.connections
                                .insert(endpoint.connection_id(), (peer, source, endpoint.clone()));

                            // keep polling the connection until it registers a timeout
                            pending_event =
                                Some(TransportEvent::ConnectionEstablished { peer, endpoint });
                        }
                        ConnectionEvent::ConnectionClosed => {
                            this.opening.remove(&source);
                            return None;
                        }
                        ConnectionEvent::Timeout { duration } => {
                            this.timeouts.insert(
                                source,
                                Box::pin(async move {
                                    Delay::new(duration);
                                }),
                            );
                            break;
                        }
                    }
                }

                pending_event
            })
            .collect::<VecDeque<_>>();

        this.timeouts.retain(|source, _| this.opening.contains_key(source));
        this.pending_events.extend(pending_events);
        this.pending_events
            .pop_front()
            .map_or(Poll::Pending, |event| Poll::Ready(Some(event)))
    }
}

/// Check if the packet received is STUN.
///
/// Extracted from the STUN RFC 5389 (<https://datatracker.ietf.org/doc/html/rfc5389#page-10>):
///  All STUN messages MUST start with a 20-byte header followed by zero
///  or more Attributes.  The STUN header contains a STUN message type,
///  magic cookie, transaction ID, and message length.
///
/// ```ignore
///      0                   1                   2                   3
///      0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///     +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///     |0 0|     STUN Message Type     |         Message Length        |
///     +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///     |                         Magic Cookie                          |
///     +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///     |                                                               |
///     |                     Transaction ID (96 bits)                  |
///     |                                                               |
///     +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// ```
fn is_stun_packet(bytes: &[u8]) -> bool {
    // 20 bytes for the header, then follows attributes.
    bytes.len() >= 20 && bytes[0] < 2
}
