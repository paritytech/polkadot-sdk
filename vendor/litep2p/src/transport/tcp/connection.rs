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

use crate::{
    config::Role,
    crypto::{
        ed25519::Keypair,
        noise::{self, NoiseSocket},
    },
    error::{Error, NegotiationError, SubstreamError},
    multistream_select::{dialer_select_proto, listener_select_proto, Negotiated, Version},
    protocol::{Direction, Permit, ProtocolCommand, ProtocolSet},
    substream,
    transport::{
        common::listener::{AddressType, DnsType},
        tcp::substream::Substream,
        Endpoint,
    },
    types::{protocol::ProtocolName, ConnectionId, SubstreamId},
    BandwidthSink, PeerId,
};

use futures::{
    future::BoxFuture,
    stream::{FuturesUnordered, StreamExt},
    AsyncRead, AsyncWrite,
};
use multiaddr::{Multiaddr, Protocol};
use tokio::net::TcpStream;
use tokio_util::compat::{
    Compat, FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt,
};

use std::{
    borrow::Cow,
    fmt,
    net::SocketAddr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::tcp::connection";

#[derive(Debug)]
pub struct NegotiatedSubstream {
    /// Substream direction.
    direction: Direction,

    /// Substream ID.
    substream_id: SubstreamId,

    /// Protocol name.
    protocol: ProtocolName,

    /// Yamux substream.
    io: crate::yamux::Stream,

    /// Permit.
    permit: Permit,
}

/// TCP connection error.
#[derive(Debug)]
enum ConnectionError {
    /// Timeout
    Timeout {
        /// Protocol.
        protocol: Option<ProtocolName>,

        /// Substream ID.
        substream_id: Option<SubstreamId>,
    },

    /// Failed to negotiate connection/substream.
    FailedToNegotiate {
        /// Protocol.
        protocol: Option<ProtocolName>,

        /// Substream ID.
        substream_id: Option<SubstreamId>,

        /// Error.
        error: SubstreamError,
    },
}

/// Connection context for an opened connection that hasn't yet started its event loop.
pub struct NegotiatedConnection {
    /// Yamux connection.
    connection: crate::yamux::ControlledConnection<NoiseSocket<Compat<TcpStream>>>,

    /// Yamux control.
    control: crate::yamux::Control,

    /// Remote peer ID.
    peer: PeerId,

    /// Endpoint.
    endpoint: Endpoint,

    /// Substream open timeout.
    substream_open_timeout: Duration,
}

impl std::fmt::Debug for NegotiatedConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NegotiatedConnection")
            .field("peer", &self.peer)
            .field("endpoint", &self.endpoint)
            .finish()
    }
}

impl NegotiatedConnection {
    /// Get `ConnectionId` of the negotiated connection.
    pub fn connection_id(&self) -> ConnectionId {
        self.endpoint.connection_id()
    }

    /// Get `PeerId` of the negotiated connection.
    pub fn peer(&self) -> PeerId {
        self.peer
    }

    /// Get `Endpoint` of the negotiated connection.
    pub fn endpoint(&self) -> Endpoint {
        self.endpoint.clone()
    }
}

/// TCP connection.
pub struct TcpConnection {
    /// Protocol context.
    protocol_set: ProtocolSet,

    /// Yamux connection.
    connection: crate::yamux::ControlledConnection<NoiseSocket<Compat<TcpStream>>>,

    /// Yamux control.
    control: crate::yamux::Control,

    /// Remote peer ID.
    peer: PeerId,

    /// Endpoint.
    endpoint: Endpoint,

    /// Substream open timeout.
    substream_open_timeout: Duration,

    /// Next substream ID.
    next_substream_id: Arc<AtomicUsize>,

    // Bandwidth sink.
    bandwidth_sink: BandwidthSink,

    /// Pending substreams.
    pending_substreams:
        FuturesUnordered<BoxFuture<'static, Result<NegotiatedSubstream, ConnectionError>>>,
}

impl fmt::Debug for TcpConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TcpConnection")
            .field("peer", &self.peer)
            .field("next_substream_id", &self.next_substream_id)
            .finish()
    }
}

impl TcpConnection {
    /// Create new [`TcpConnection`] from [`NegotiatedConnection`].
    pub(super) fn new(
        context: NegotiatedConnection,
        protocol_set: ProtocolSet,
        bandwidth_sink: BandwidthSink,
        next_substream_id: Arc<AtomicUsize>,
    ) -> Self {
        let NegotiatedConnection {
            connection,
            control,
            peer,
            endpoint,
            substream_open_timeout,
        } = context;

        Self {
            protocol_set,
            connection,
            control,
            peer,
            endpoint,
            bandwidth_sink,
            next_substream_id,
            pending_substreams: FuturesUnordered::new(),
            substream_open_timeout,
        }
    }

    /// Open connection to remote peer at `address`.
    // TODO: https://github.com/paritytech/litep2p/issues/347 this function can be removed
    pub(super) async fn open_connection(
        connection_id: ConnectionId,
        keypair: Keypair,
        stream: TcpStream,
        address: AddressType,
        peer: Option<PeerId>,
        yamux_config: crate::yamux::Config,
        max_read_ahead_factor: usize,
        max_write_buffer_size: usize,
        connection_open_timeout: Duration,
        substream_open_timeout: Duration,
    ) -> Result<NegotiatedConnection, NegotiationError> {
        tracing::debug!(
            target: LOG_TARGET,
            ?address,
            ?peer,
            "open connection to remote peer",
        );

        match tokio::time::timeout(connection_open_timeout, async move {
            Self::negotiate_connection(
                stream,
                peer,
                connection_id,
                keypair,
                Role::Dialer,
                address,
                yamux_config,
                max_read_ahead_factor,
                max_write_buffer_size,
                substream_open_timeout,
            )
            .await
        })
        .await
        {
            Err(_) => {
                tracing::trace!(target: LOG_TARGET, ?connection_id, "connection timed out during negotiation");
                Err(NegotiationError::Timeout)
            }
            Ok(result) => result,
        }
    }

    /// Open substream for `protocol`.
    pub(super) async fn open_substream(
        mut control: crate::yamux::Control,
        substream_id: SubstreamId,
        permit: Permit,
        protocol: ProtocolName,
        fallback_names: Vec<ProtocolName>,
        open_timeout: Duration,
    ) -> Result<NegotiatedSubstream, SubstreamError> {
        tracing::debug!(target: LOG_TARGET, ?protocol, ?substream_id, "open substream");

        let stream = match control.open_stream().await {
            Ok(stream) => {
                tracing::trace!(target: LOG_TARGET, ?substream_id, "substream opened");
                stream
            }
            Err(error) => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?substream_id,
                    ?error,
                    "failed to open substream"
                );
                return Err(SubstreamError::YamuxError(
                    error,
                    Direction::Outbound(substream_id),
                ));
            }
        };

        // TODO: https://github.com/paritytech/litep2p/issues/346 protocols don't change after
        // they've been initialized so this should be done only once
        let protocols = std::iter::once(&*protocol)
            .chain(fallback_names.iter().map(|protocol| &**protocol))
            .collect();

        let (io, protocol) =
            Self::negotiate_protocol(stream, &Role::Dialer, protocols, open_timeout).await?;

        Ok(NegotiatedSubstream {
            io: io.inner(),
            substream_id,
            direction: Direction::Outbound(substream_id),
            protocol,
            permit,
        })
    }

    /// Accept a new connection.
    pub(super) async fn accept_connection(
        stream: TcpStream,
        connection_id: ConnectionId,
        keypair: Keypair,
        address: SocketAddr,
        yamux_config: crate::yamux::Config,
        max_read_ahead_factor: usize,
        max_write_buffer_size: usize,
        connection_open_timeout: Duration,
        substream_open_timeout: Duration,
    ) -> Result<NegotiatedConnection, NegotiationError> {
        tracing::debug!(target: LOG_TARGET, ?address, "accept connection");

        match tokio::time::timeout(connection_open_timeout, async move {
            Self::negotiate_connection(
                stream,
                None,
                connection_id,
                keypair,
                Role::Listener,
                AddressType::Socket(address),
                yamux_config,
                max_read_ahead_factor,
                max_write_buffer_size,
                substream_open_timeout,
            )
            .await
        })
        .await
        {
            Err(_) => Err(NegotiationError::Timeout),
            Ok(result) => result,
        }
    }

    /// Accept substream.
    pub(super) async fn accept_substream(
        stream: crate::yamux::Stream,
        permit: Permit,
        substream_id: SubstreamId,
        protocols: Vec<ProtocolName>,
        open_timeout: Duration,
    ) -> Result<NegotiatedSubstream, NegotiationError> {
        tracing::trace!(
            target: LOG_TARGET,
            ?substream_id,
            "accept inbound substream",
        );

        let protocols = protocols.iter().map(|protocol| &**protocol).collect::<Vec<&str>>();
        let (io, protocol) =
            Self::negotiate_protocol(stream, &Role::Listener, protocols, open_timeout).await?;

        tracing::trace!(
            target: LOG_TARGET,
            ?substream_id,
            "substream accepted and negotiated",
        );

        Ok(NegotiatedSubstream {
            io: io.inner(),
            substream_id,
            direction: Direction::Inbound,
            protocol,
            permit,
        })
    }

    /// Negotiate protocol.
    async fn negotiate_protocol<S: AsyncRead + AsyncWrite + Unpin>(
        stream: S,
        role: &Role,
        protocols: Vec<&str>,
        substream_open_timeout: Duration,
    ) -> Result<(Negotiated<S>, ProtocolName), NegotiationError> {
        tracing::trace!(target: LOG_TARGET, ?protocols, "negotiating protocols");

        match tokio::time::timeout(substream_open_timeout, async move {
            match role {
                Role::Dialer => dialer_select_proto(stream, protocols, Version::V1).await,
                Role::Listener => listener_select_proto(stream, protocols).await,
            }
        })
        .await
        {
            Err(_) => Err(NegotiationError::Timeout),
            Ok(Err(error)) => Err(NegotiationError::MultistreamSelectError(error)),
            Ok(Ok((protocol, socket))) => {
                tracing::trace!(target: LOG_TARGET, ?protocol, "protocol negotiated");

                Ok((socket, ProtocolName::from(protocol.to_string())))
            }
        }
    }

    /// Negotiate noise + yamux for the connection.
    pub(super) async fn negotiate_connection(
        stream: TcpStream,
        dialed_peer: Option<PeerId>,
        connection_id: ConnectionId,
        keypair: Keypair,
        role: Role,
        address: AddressType,
        yamux_config: crate::yamux::Config,
        max_read_ahead_factor: usize,
        max_write_buffer_size: usize,
        substream_open_timeout: Duration,
    ) -> Result<NegotiatedConnection, NegotiationError> {
        tracing::trace!(
            target: LOG_TARGET,
            ?role,
            "negotiate connection",
        );

        let stream = TokioAsyncReadCompatExt::compat(stream).into_inner();
        let stream = TokioAsyncWriteCompatExt::compat_write(stream);

        // negotiate `noise`
        let (stream, _) =
            Self::negotiate_protocol(stream, &role, vec!["/noise"], substream_open_timeout).await?;

        tracing::trace!(
            target: LOG_TARGET,
            "`multistream-select` and `noise` negotiated",
        );

        // perform noise handshake
        let (stream, peer) = noise::handshake(
            stream.inner(),
            &keypair,
            role,
            max_read_ahead_factor,
            max_write_buffer_size,
            substream_open_timeout,
            noise::HandshakeTransport::Tcp,
        )
        .await?;

        if let Some(dialed_peer) = dialed_peer {
            if dialed_peer != peer {
                tracing::debug!(target: LOG_TARGET, ?dialed_peer, ?peer, "peer id mismatch");
                return Err(NegotiationError::PeerIdMismatch(dialed_peer, peer));
            }
        }

        tracing::trace!(target: LOG_TARGET, "noise handshake done");
        let stream: NoiseSocket<Compat<TcpStream>> = stream;

        // negotiate `yamux`
        let (stream, _) =
            Self::negotiate_protocol(stream, &role, vec!["/yamux/1.0.0"], substream_open_timeout)
                .await?;
        tracing::trace!(target: LOG_TARGET, "`yamux` negotiated");

        let connection = crate::yamux::Connection::new(stream.inner(), yamux_config, role.into());
        let (control, connection) = crate::yamux::Control::new(connection);

        let address = match address {
            AddressType::Socket(address) => Multiaddr::empty()
                .with(Protocol::from(address.ip()))
                .with(Protocol::Tcp(address.port())),
            AddressType::Dns {
                address,
                port,
                dns_type,
            } => match dns_type {
                DnsType::Dns => Multiaddr::empty()
                    .with(Protocol::Dns(Cow::Owned(address)))
                    .with(Protocol::Tcp(port)),
                DnsType::Dns4 => Multiaddr::empty()
                    .with(Protocol::Dns4(Cow::Owned(address)))
                    .with(Protocol::Tcp(port)),
                DnsType::Dns6 => Multiaddr::empty()
                    .with(Protocol::Dns6(Cow::Owned(address)))
                    .with(Protocol::Tcp(port)),
            },
        };
        let endpoint = match role {
            Role::Dialer => Endpoint::dialer(address, connection_id),
            Role::Listener => Endpoint::listener(address, connection_id),
        };

        Ok(NegotiatedConnection {
            peer,
            control,
            connection,
            endpoint,
            substream_open_timeout,
        })
    }

    /// Handles the yamux substream.
    ///
    /// Returns `true` if the connection handler should exit.
    async fn handle_yamux_substream(
        &mut self,
        substream: Option<Result<crate::yamux::Stream, crate::yamux::ConnectionError>>,
    ) -> crate::Result<bool> {
        match substream {
            Some(Ok(stream)) => {
                let substream_id = {
                    let substream_id = self.next_substream_id.fetch_add(1usize, Ordering::Relaxed);
                    SubstreamId::from(substream_id)
                };
                let protocols = self.protocol_set.protocols();
                let permit = self.protocol_set.try_get_permit().ok_or(Error::ConnectionClosed)?;
                let open_timeout = self.substream_open_timeout;

                self.pending_substreams.push(Box::pin(async move {
                    match tokio::time::timeout(
                        open_timeout,
                        Self::accept_substream(
                            stream,
                            permit,
                            substream_id,
                            protocols,
                            open_timeout,
                        ),
                    )
                    .await
                    {
                        Ok(Ok(substream)) => Ok(substream),
                        Ok(Err(error)) => Err(ConnectionError::FailedToNegotiate {
                            protocol: None,
                            substream_id: None,
                            error: SubstreamError::NegotiationError(error),
                        }),
                        Err(_) => Err(ConnectionError::Timeout {
                            protocol: None,
                            substream_id: None,
                        }),
                    }
                }));

                Ok(false)
            }
            Some(Err(error)) => {
                tracing::debug!(
                    target: LOG_TARGET,
                    peer = ?self.peer,
                    ?error,
                    "connection closed with error",
                );

                self.protocol_set
                    .report_connection_closed(self.peer, self.endpoint.connection_id())
                    .await?;
                Ok(true)
            }
            None => {
                tracing::debug!(target: LOG_TARGET, peer = ?self.peer, "connection closed");
                self.protocol_set
                    .report_connection_closed(self.peer, self.endpoint.connection_id())
                    .await?;
                Ok(true)
            }
        }
    }

    /// Handles negotiated substream results.
    async fn handle_negotiated_substream(
        &mut self,
        result: Result<NegotiatedSubstream, ConnectionError>,
    ) {
        match result {
            Err(error) => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?error,
                    "failed to accept/open substream",
                );

                let (protocol, substream_id, error) = match error {
                    ConnectionError::Timeout {
                        protocol,
                        substream_id,
                    } => (
                        protocol,
                        substream_id,
                        SubstreamError::NegotiationError(NegotiationError::Timeout),
                    ),
                    ConnectionError::FailedToNegotiate {
                        protocol,
                        substream_id,
                        error,
                    } => (protocol, substream_id, error),
                };

                match (protocol, substream_id) {
                    (Some(protocol), Some(substream_id)) => {
                        if let Err(error) = self
                            .protocol_set
                            .report_substream_open_failure(protocol.clone(), substream_id, error)
                            .await
                        {
                            tracing::error!(
                                target: LOG_TARGET,
                                ?protocol,
                                endpoint = ?self.endpoint,
                                ?error,
                                "failed to register substream open failure to protocol"
                            );
                        }
                    }
                    _ => {}
                }
            }
            Ok(substream) => {
                let protocol = substream.protocol.clone();
                let direction = substream.direction;
                let substream_id = substream.substream_id;
                let socket = FuturesAsyncReadCompatExt::compat(substream.io);
                let bandwidth_sink = self.bandwidth_sink.clone();

                let substream = substream::Substream::new_tcp(
                    self.peer,
                    substream_id,
                    Substream::new(socket, bandwidth_sink, substream.permit),
                    self.protocol_set.protocol_codec(&protocol),
                );

                if let Err(error) = self
                    .protocol_set
                    .report_substream_open(self.peer, protocol.clone(), direction, substream)
                    .await
                {
                    tracing::error!(
                        target: LOG_TARGET,
                        ?protocol,
                        peer = ?self.peer,
                        endpoint = ?self.endpoint,
                        ?error,
                        "failed to register opened substream to protocol",
                    );
                }
            }
        }
    }

    /// Handles protocol command.
    ///
    /// Returns `true` if the connection handler should exit.
    async fn handle_protocol_command(
        &mut self,
        command: Option<ProtocolCommand>,
    ) -> crate::Result<bool> {
        match command {
            Some(ProtocolCommand::OpenSubstream {
                protocol,
                fallback_names,
                substream_id,
                connection_id,
                permit,
            }) => {
                let control = self.control.clone();
                let open_timeout = self.substream_open_timeout;

                tracing::trace!(
                    target: LOG_TARGET,
                    ?protocol,
                    ?substream_id,
                    ?connection_id,
                    "open substream",
                );

                self.pending_substreams.push(Box::pin(async move {
                    match tokio::time::timeout(
                        open_timeout,
                        Self::open_substream(
                            control,
                            substream_id,
                            permit,
                            protocol.clone(),
                            fallback_names,
                            open_timeout,
                        ),
                    )
                    .await
                    {
                        Ok(Ok(substream)) => Ok(substream),
                        Ok(Err(error)) => Err(ConnectionError::FailedToNegotiate {
                            protocol: Some(protocol),
                            substream_id: Some(substream_id),
                            error,
                        }),
                        Err(_) => Err(ConnectionError::Timeout {
                            protocol: Some(protocol),
                            substream_id: Some(substream_id),
                        }),
                    }
                }));

                Ok(false)
            }
            Some(ProtocolCommand::ForceClose) => {
                tracing::debug!(
                    target: LOG_TARGET,
                    peer = ?self.peer,
                    connection_id = ?self.endpoint.connection_id(),
                    "force closing connection",
                );

                self.protocol_set
                    .report_connection_closed(self.peer, self.endpoint.connection_id())
                    .await?;
                Ok(true)
            }
            None => {
                tracing::debug!(target: LOG_TARGET, "protocols have disconnected, closing connection");
                self.protocol_set
                    .report_connection_closed(self.peer, self.endpoint.connection_id())
                    .await?;
                Ok(true)
            }
        }
    }

    /// Start connection event loop.
    pub(crate) async fn start(mut self) -> crate::Result<()> {
        self.protocol_set
            .report_connection_established(self.peer, self.endpoint.clone())
            .await?;

        loop {
            tokio::select! {
                substream = self.connection.next() => {
                    if self.handle_yamux_substream(substream).await? {
                        return Ok(());
                    }
                },
                substream = self.pending_substreams.select_next_some(), if !self.pending_substreams.is_empty() => {
                   self.handle_negotiated_substream(substream).await;
                }
                protocol = self.protocol_set.next() => {
                    if self.handle_protocol_command(protocol).await? {
                        return Ok(())
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::transport::tcp::TcpTransport;

    use super::*;
    use hickory_resolver::{name_server::TokioConnectionProvider, TokioResolver};
    use tokio::{io::AsyncWriteExt, net::TcpListener};

    #[tokio::test]
    async fn multistream_select_not_supported_dialer() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let listener = TcpListener::bind("[::1]:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let _ = stream.write_all(&vec![0x12u8; 256]).await;
        });

        let (_, stream) = TcpTransport::dial_peer(
            Multiaddr::empty()
                .with(Protocol::from(address.ip()))
                .with(Protocol::Tcp(address.port())),
            Default::default(),
            Duration::from_secs(10),
            false,
            Arc::new(
                TokioResolver::builder_with_config(
                    Default::default(),
                    TokioConnectionProvider::default(),
                )
                .build(),
            ),
        )
        .await
        .unwrap();

        match TcpConnection::open_connection(
            ConnectionId::from(0usize),
            Keypair::generate(),
            stream,
            AddressType::Socket(address),
            None,
            Default::default(),
            5,
            2,
            Duration::from_secs(10),
            Duration::from_secs(10),
        )
        .await
        {
            Ok(_) => panic!("connection was supposed to fail"),
            Err(NegotiationError::MultistreamSelectError(
                crate::multistream_select::NegotiationError::ProtocolError(
                    crate::multistream_select::ProtocolError::InvalidMessage,
                ),
            )) => {}
            Err(error) => panic!("invalid error: {error:?}"),
        }
    }

    #[tokio::test]
    async fn multistream_select_not_supported_listener() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let listener = TcpListener::bind("[::1]:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        let (Ok(mut dialer), Ok((stream, dialer_address))) =
            tokio::join!(TcpStream::connect(address), listener.accept(),)
        else {
            panic!("failed to establish connection");
        };

        tokio::spawn(async move {
            let _ = dialer.write_all(&vec![0x12u8; 256]).await;
        });

        match TcpConnection::accept_connection(
            stream,
            ConnectionId::from(0usize),
            Keypair::generate(),
            dialer_address,
            Default::default(),
            5,
            2,
            Duration::from_secs(10),
            Duration::from_secs(10),
        )
        .await
        {
            Ok(_) => panic!("connection was supposed to fail"),
            Err(NegotiationError::MultistreamSelectError(
                crate::multistream_select::NegotiationError::ProtocolError(
                    crate::multistream_select::ProtocolError::InvalidMessage,
                ),
            )) => {}
            Err(error) => panic!("invalid error: {error:?}"),
        }
    }

    #[tokio::test]
    async fn noise_not_supported_dialer() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let listener = TcpListener::bind("[::1]:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let stream = TokioAsyncReadCompatExt::compat(stream).into_inner();
            let stream = TokioAsyncWriteCompatExt::compat_write(stream);

            // attempt to negotiate yamux, skipping noise entirely
            assert!(listener_select_proto(stream, vec!["/yamux/1.0.0"]).await.is_err());
        });

        let (_, stream) = TcpTransport::dial_peer(
            Multiaddr::empty()
                .with(Protocol::from(address.ip()))
                .with(Protocol::Tcp(address.port())),
            Default::default(),
            Duration::from_secs(10),
            false,
            Arc::new(
                TokioResolver::builder_with_config(
                    Default::default(),
                    TokioConnectionProvider::default(),
                )
                .build(),
            ),
        )
        .await
        .unwrap();

        match TcpConnection::open_connection(
            ConnectionId::from(0usize),
            Keypair::generate(),
            stream,
            AddressType::Socket(address),
            None,
            Default::default(),
            5,
            2,
            Duration::from_secs(10),
            Duration::from_secs(10),
        )
        .await
        {
            Ok(_) => panic!("connection was supposed to fail"),
            Err(NegotiationError::MultistreamSelectError(
                crate::multistream_select::NegotiationError::Failed,
            )) => {}
            Err(error) => panic!("invalid error: {error:?}"),
        }
    }

    #[tokio::test]
    async fn noise_not_supported_listener() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let listener = TcpListener::bind("[::1]:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        let (Ok(dialer), Ok((listener, dialer_address))) =
            tokio::join!(TcpStream::connect(address), listener.accept(),)
        else {
            panic!("failed to establish connection");
        };

        tokio::spawn(async move {
            let dialer = TokioAsyncReadCompatExt::compat(dialer).into_inner();
            let dialer = TokioAsyncWriteCompatExt::compat_write(dialer);

            // attempt to negotiate yamux, skipping noise entirely
            assert!(dialer_select_proto(dialer, vec!["/yamux/1.0.0"], Version::V1).await.is_err());
        });

        match TcpConnection::accept_connection(
            listener,
            ConnectionId::from(0usize),
            Keypair::generate(),
            dialer_address,
            Default::default(),
            5,
            2,
            Duration::from_secs(10),
            Duration::from_secs(10),
        )
        .await
        {
            Ok(_) => panic!("connection was supposed to fail"),
            Err(NegotiationError::MultistreamSelectError(
                crate::multistream_select::NegotiationError::Failed,
            )) => {}
            Err(error) => panic!("invalid error: {error:?}"),
        }
    }

    #[tokio::test]
    async fn noise_timeout_listener() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let listener = TcpListener::bind("[::1]:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        let (Ok(dialer), Ok((listener, dialer_address))) =
            tokio::join!(TcpStream::connect(address), listener.accept(),)
        else {
            panic!("failed to establish connection");
        };

        tokio::spawn(async move {
            let dialer = TokioAsyncReadCompatExt::compat(dialer).into_inner();
            let dialer = TokioAsyncWriteCompatExt::compat_write(dialer);

            // attempt to negotiate yamux, skipping noise entirely
            let (_protocol, _socket) =
                dialer_select_proto(dialer, vec!["/noise"], Version::V1).await.unwrap();

            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        });

        match TcpConnection::accept_connection(
            listener,
            ConnectionId::from(0usize),
            Keypair::generate(),
            dialer_address,
            Default::default(),
            5,
            2,
            Duration::from_secs(10),
            Duration::from_secs(10),
        )
        .await
        {
            Ok(_) => panic!("connection was supposed to fail"),
            Err(NegotiationError::Timeout) => {}
            Err(error) => panic!("invalid error: {error:?}"),
        }
    }

    #[tokio::test]
    async fn noise_timeout_dialer() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let listener = TcpListener::bind("[::1]:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let stream = TokioAsyncReadCompatExt::compat(stream).into_inner();
            let stream = TokioAsyncWriteCompatExt::compat_write(stream);

            // negotiate noise but never actually send any handshake data
            let (_protocol, _socket) = listener_select_proto(stream, vec!["/noise"]).await.unwrap();

            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        });

        let (_, stream) = TcpTransport::dial_peer(
            Multiaddr::empty()
                .with(Protocol::from(address.ip()))
                .with(Protocol::Tcp(address.port())),
            Default::default(),
            Duration::from_secs(10),
            false,
            Arc::new(
                TokioResolver::builder_with_config(
                    Default::default(),
                    TokioConnectionProvider::default(),
                )
                .build(),
            ),
        )
        .await
        .unwrap();

        match TcpConnection::open_connection(
            ConnectionId::from(0usize),
            Keypair::generate(),
            stream,
            AddressType::Socket(address),
            None,
            Default::default(),
            5,
            2,
            Duration::from_secs(10),
            Duration::from_secs(10),
        )
        .await
        {
            Ok(_) => panic!("connection was supposed to fail"),
            Err(NegotiationError::Timeout) => {}
            Err(error) => panic!("invalid error: {error:?}"),
        }
    }

    #[tokio::test]
    async fn multistream_select_timeout_dialer() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let listener = TcpListener::bind("[::1]:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let _stream = listener.accept().await.unwrap();

            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        });

        let (_, stream) = TcpTransport::dial_peer(
            Multiaddr::empty()
                .with(Protocol::from(address.ip()))
                .with(Protocol::Tcp(address.port())),
            Default::default(),
            Duration::from_secs(10),
            false,
            Arc::new(
                TokioResolver::builder_with_config(
                    Default::default(),
                    TokioConnectionProvider::default(),
                )
                .build(),
            ),
        )
        .await
        .unwrap();

        match TcpConnection::open_connection(
            ConnectionId::from(0usize),
            Keypair::generate(),
            stream,
            AddressType::Socket(address),
            None,
            Default::default(),
            5,
            2,
            Duration::from_secs(10),
            Duration::from_secs(10),
        )
        .await
        {
            Ok(_) => panic!("connection was supposed to fail"),
            Err(NegotiationError::Timeout) => {}
            Err(error) => panic!("invalid error: {error:?}"),
        }
    }

    #[tokio::test]
    async fn multistream_select_timeout_listener() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let listener = TcpListener::bind("[::1]:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        let (Ok(_dialer), Ok((listener, dialer_address))) =
            tokio::join!(TcpStream::connect(address), listener.accept(),)
        else {
            panic!("failed to establish connection");
        };

        tokio::spawn(async move {
            let _stream = TcpStream::connect(address).await.unwrap();

            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        });

        match TcpConnection::accept_connection(
            listener,
            ConnectionId::from(0usize),
            Keypair::generate(),
            dialer_address,
            Default::default(),
            5,
            2,
            Duration::from_secs(10),
            Duration::from_secs(10),
        )
        .await
        {
            Ok(_) => panic!("connection was supposed to fail"),
            Err(NegotiationError::Timeout) => {}
            Err(error) => panic!("invalid error: {error:?}"),
        }
    }

    #[tokio::test]
    async fn yamux_not_supported_dialer() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let listener = TcpListener::bind("[::1]:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        let (Ok(dialer), Ok((listener, dialer_address))) =
            tokio::join!(TcpStream::connect(address), listener.accept(),)
        else {
            panic!("failed to establish connection");
        };

        tokio::spawn(async move {
            let dialer = TokioAsyncReadCompatExt::compat(dialer).into_inner();
            let dialer = TokioAsyncWriteCompatExt::compat_write(dialer);

            // negotiate noise
            let (_protocol, stream) =
                dialer_select_proto(dialer, vec!["/noise"], Version::V1).await.unwrap();

            let keypair = Keypair::generate();

            // do a noise handshake
            let (stream, _peer) = noise::handshake(
                stream.inner(),
                &keypair,
                Role::Dialer,
                5,
                2,
                std::time::Duration::from_secs(10),
                noise::HandshakeTransport::Tcp,
            )
            .await
            .unwrap();
            let stream: NoiseSocket<Compat<TcpStream>> = stream;

            // after the handshake, try to negotiate some random protocol instead of yamux
            assert!(
                dialer_select_proto(stream, vec!["/unsupported/1"], Version::V1).await.is_err()
            );
        });

        match TcpConnection::accept_connection(
            listener,
            ConnectionId::from(0usize),
            Keypair::generate(),
            dialer_address,
            Default::default(),
            5,
            2,
            Duration::from_secs(10),
            Duration::from_secs(10),
        )
        .await
        {
            Ok(_) => panic!("connection was supposed to fail"),
            Err(NegotiationError::MultistreamSelectError(
                crate::multistream_select::NegotiationError::Failed,
            )) => {}
            Err(error) => panic!("{error:?}"),
        }
    }

    #[tokio::test]
    async fn yamux_not_supported_listener() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let listener = TcpListener::bind("[::1]:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let stream = TokioAsyncReadCompatExt::compat(stream).into_inner();
            let stream = TokioAsyncWriteCompatExt::compat_write(stream);

            // negotiate noise
            let (_protocol, stream) = listener_select_proto(stream, vec!["/noise"]).await.unwrap();

            // do a noise handshake
            let keypair = Keypair::generate();
            let (stream, _peer) = noise::handshake(
                stream.inner(),
                &keypair,
                Role::Listener,
                5,
                2,
                std::time::Duration::from_secs(10),
                noise::HandshakeTransport::Tcp,
            )
            .await
            .unwrap();
            let stream: NoiseSocket<Compat<TcpStream>> = stream;

            // after the handshake, try to negotiate some random protocol instead of yamux
            assert!(listener_select_proto(stream, vec!["/unsupported/1"]).await.is_err());
        });

        let (_, stream) = TcpTransport::dial_peer(
            Multiaddr::empty()
                .with(Protocol::from(address.ip()))
                .with(Protocol::Tcp(address.port())),
            Default::default(),
            Duration::from_secs(10),
            false,
            Arc::new(
                TokioResolver::builder_with_config(
                    Default::default(),
                    TokioConnectionProvider::default(),
                )
                .build(),
            ),
        )
        .await
        .unwrap();

        match TcpConnection::open_connection(
            ConnectionId::from(0usize),
            Keypair::generate(),
            stream,
            AddressType::Socket(address),
            None,
            Default::default(),
            5,
            2,
            Duration::from_secs(10),
            Duration::from_secs(10),
        )
        .await
        {
            Ok(_) => panic!("connection was supposed to fail"),
            Err(NegotiationError::MultistreamSelectError(
                crate::multistream_select::NegotiationError::Failed,
            )) => {}
            Err(error) => panic!("{error:?}"),
        }
    }

    #[tokio::test]
    async fn yamux_timeout_dialer() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let listener = TcpListener::bind("[::1]:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        let (Ok(dialer), Ok((listener, dialer_address))) =
            tokio::join!(TcpStream::connect(address), listener.accept())
        else {
            panic!("failed to establish connection");
        };

        tokio::spawn(async move {
            let dialer = TokioAsyncReadCompatExt::compat(dialer).into_inner();
            let dialer = TokioAsyncWriteCompatExt::compat_write(dialer);

            // negotiate noise
            let (_protocol, stream) =
                dialer_select_proto(dialer, vec!["/noise"], Version::V1).await.unwrap();

            // do a noise handshake
            let keypair = Keypair::generate();
            let (stream, _peer) = noise::handshake(
                stream.inner(),
                &keypair,
                Role::Dialer,
                5,
                2,
                std::time::Duration::from_secs(10),
                noise::HandshakeTransport::Tcp,
            )
            .await
            .unwrap();
            let _stream: NoiseSocket<Compat<TcpStream>> = stream;

            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        });

        match TcpConnection::accept_connection(
            listener,
            ConnectionId::from(0usize),
            Keypair::generate(),
            dialer_address,
            Default::default(),
            5,
            2,
            Duration::from_secs(10),
            Duration::from_secs(10),
        )
        .await
        {
            Ok(_) => panic!("connection was supposed to fail"),
            Err(NegotiationError::Timeout) => {}
            Err(error) => panic!("invalid error: {error:?}"),
        }
    }

    #[tokio::test]
    async fn yamux_timeout_listener() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let listener = TcpListener::bind("[::1]:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let stream = TokioAsyncReadCompatExt::compat(stream).into_inner();
            let stream = TokioAsyncWriteCompatExt::compat_write(stream);

            // negotiate noise
            let (_protocol, stream) = listener_select_proto(stream, vec!["/noise"]).await.unwrap();

            // do a noise handshake
            let keypair = Keypair::generate();
            let (stream, _peer) = noise::handshake(
                stream.inner(),
                &keypair,
                Role::Listener,
                5,
                2,
                std::time::Duration::from_secs(10),
                noise::HandshakeTransport::Tcp,
            )
            .await
            .unwrap();
            let _stream: NoiseSocket<Compat<TcpStream>> = stream;

            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        });

        let (_, stream) = TcpTransport::dial_peer(
            Multiaddr::empty()
                .with(Protocol::from(address.ip()))
                .with(Protocol::Tcp(address.port())),
            Default::default(),
            Duration::from_secs(10),
            false,
            Arc::new(
                TokioResolver::builder_with_config(
                    Default::default(),
                    TokioConnectionProvider::default(),
                )
                .build(),
            ),
        )
        .await
        .unwrap();

        match TcpConnection::open_connection(
            ConnectionId::from(0usize),
            Keypair::generate(),
            stream,
            AddressType::Socket(address),
            None,
            Default::default(),
            5,
            2,
            Duration::from_secs(10),
            Duration::from_secs(10),
        )
        .await
        {
            Ok(_) => panic!("connection was supposed to fail"),
            Err(NegotiationError::Timeout) => {}
            Err(error) => panic!("invalid error: {error:?}"),
        }
    }
}
