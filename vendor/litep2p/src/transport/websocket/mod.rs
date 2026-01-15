// Copyright 2023 litep2p developers
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rigts to use, copy, modify, merge, publish, distribute, sublicense,
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

//! WebSocket transport.

use crate::{
    error::{AddressError, Error, NegotiationError},
    transport::{
        common::listener::{DialAddresses, GetSocketAddr, SocketListener, WebSocketAddress},
        manager::TransportHandle,
        websocket::{
            config::Config,
            connection::{NegotiatedConnection, WebSocketConnection},
        },
        Transport, TransportBuilder, TransportEvent,
    },
    types::ConnectionId,
    utils::futures_stream::FuturesStream,
    DialError, PeerId,
};

use futures::{
    future::BoxFuture,
    stream::{AbortHandle, FuturesUnordered},
    Stream, StreamExt, TryFutureExt,
};
use hickory_resolver::TokioResolver;
use multiaddr::{Multiaddr, Protocol};
use socket2::{Domain, Socket, Type};
use std::{net::SocketAddr, sync::Arc};
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use url::Url;

use std::{
    collections::HashMap,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

pub(crate) use substream::Substream;

mod connection;
mod stream;
mod substream;

pub mod config;

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::websocket";

/// Pending inbound connection.
struct PendingInboundConnection {
    /// Socket address of the remote peer.
    connection: TcpStream,
    /// Address of the remote peer.
    address: SocketAddr,
}

#[derive(Debug)]
enum RawConnectionResult {
    /// The first successful connection.
    Connected {
        negotiated: NegotiatedConnection,
        errors: Vec<(Multiaddr, DialError)>,
    },

    /// All connection attempts failed.
    Failed {
        connection_id: ConnectionId,
        errors: Vec<(Multiaddr, DialError)>,
    },

    /// Future was canceled.
    Canceled { connection_id: ConnectionId },
}

/// WebSocket transport.
pub(crate) struct WebSocketTransport {
    /// Transport context.
    context: TransportHandle,

    /// Transport configuration.
    config: Config,

    /// WebSocket listener.
    listener: SocketListener,

    /// Dial addresses.
    dial_addresses: DialAddresses,

    /// Pending dials.
    pending_dials: HashMap<ConnectionId, Multiaddr>,

    /// Pending inbound connections.
    pending_inbound_connections: HashMap<ConnectionId, PendingInboundConnection>,

    /// Pending connections.
    pending_connections:
        FuturesStream<BoxFuture<'static, Result<NegotiatedConnection, (ConnectionId, DialError)>>>,

    /// Pending raw, unnegotiated connections.
    pending_raw_connections: FuturesStream<BoxFuture<'static, RawConnectionResult>>,

    /// Opened raw connection, waiting for approval/rejection from `TransportManager`.
    opened: HashMap<ConnectionId, NegotiatedConnection>,

    /// Cancel raw connections futures.
    ///
    /// This is cancelling `Self::pending_raw_connections`.
    cancel_futures: HashMap<ConnectionId, AbortHandle>,

    /// Negotiated connections waiting validation.
    pending_open: HashMap<ConnectionId, NegotiatedConnection>,

    /// DNS resolver.
    resolver: Arc<TokioResolver>,
}

impl WebSocketTransport {
    /// Handle inbound connection.
    fn on_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        connection: TcpStream,
        address: SocketAddr,
    ) {
        let keypair = self.context.keypair.clone();
        let yamux_config = self.config.yamux_config.clone();
        let connection_open_timeout = self.config.connection_open_timeout;
        let max_read_ahead_factor = self.config.noise_read_ahead_frame_count;
        let max_write_buffer_size = self.config.noise_write_buffer_size;
        let substream_open_timeout = self.config.substream_open_timeout;
        let address = Multiaddr::empty()
            .with(Protocol::from(address.ip()))
            .with(Protocol::Tcp(address.port()))
            .with(Protocol::Ws(std::borrow::Cow::Borrowed("/")));

        self.pending_connections.push(Box::pin(async move {
            match tokio::time::timeout(connection_open_timeout, async move {
                WebSocketConnection::accept_connection(
                    connection,
                    connection_id,
                    keypair,
                    address,
                    yamux_config,
                    max_read_ahead_factor,
                    max_write_buffer_size,
                    substream_open_timeout,
                )
                .await
                .map_err(|error| (connection_id, error.into()))
            })
            .await
            {
                Err(_) => Err((connection_id, DialError::Timeout)),
                Ok(Err(error)) => Err(error),
                Ok(Ok(result)) => Ok(result),
            }
        }));
    }

    /// Convert `Multiaddr` into `url::Url`
    fn multiaddr_into_url(address: Multiaddr) -> Result<(Url, PeerId), AddressError> {
        let mut protocol_stack = address.iter();

        let dial_address = match protocol_stack.next().ok_or(AddressError::InvalidProtocol)? {
            Protocol::Ip4(address) => address.to_string(),
            Protocol::Ip6(address) => format!("[{address}]"),
            Protocol::Dns(address) | Protocol::Dns4(address) | Protocol::Dns6(address) =>
                address.to_string(),

            _ => return Err(AddressError::InvalidProtocol),
        };

        let url = match protocol_stack.next().ok_or(AddressError::InvalidProtocol)? {
            Protocol::Tcp(port) => match protocol_stack.next() {
                Some(Protocol::Ws(_)) => format!("ws://{dial_address}:{port}/"),
                Some(Protocol::Wss(_)) => format!("wss://{dial_address}:{port}/"),
                _ => return Err(AddressError::InvalidProtocol),
            },
            _ => return Err(AddressError::InvalidProtocol),
        };

        let peer = match protocol_stack.next() {
            Some(Protocol::P2p(multihash)) => PeerId::from_multihash(multihash)?,
            protocol => {
                tracing::warn!(
                    target: LOG_TARGET,
                    ?protocol,
                    "invalid protocol, expected `Protocol::Ws`/`Protocol::Wss`",
                );
                return Err(AddressError::PeerIdMissing);
            }
        };

        tracing::trace!(target: LOG_TARGET, ?url, "parse address");

        url::Url::parse(&url)
            .map(|url| (url, peer))
            .map_err(|_| AddressError::InvalidUrl)
    }

    /// Dial remote peer over `address`.
    async fn dial_peer(
        address: Multiaddr,
        dial_addresses: DialAddresses,
        connection_open_timeout: Duration,
        nodelay: bool,
        resolver: Arc<TokioResolver>,
    ) -> Result<(Multiaddr, WebSocketStream<MaybeTlsStream<TcpStream>>), DialError> {
        let (url, _) = Self::multiaddr_into_url(address.clone())?;

        let (socket_address, _) = WebSocketAddress::multiaddr_to_socket_address(&address)?;
        let remote_address =
            match tokio::time::timeout(connection_open_timeout, socket_address.lookup_ip(resolver))
                .await
            {
                Err(_) => return Err(DialError::Timeout),
                Ok(Err(error)) => return Err(error.into()),
                Ok(Ok(address)) => address,
            };

        let domain = match remote_address.is_ipv4() {
            true => Domain::IPV4,
            false => Domain::IPV6,
        };
        let socket = Socket::new(domain, Type::STREAM, Some(socket2::Protocol::TCP))?;
        if remote_address.is_ipv6() {
            socket.set_only_v6(true)?;
        }
        socket.set_nonblocking(true)?;
        socket.set_nodelay(nodelay)?;

        match dial_addresses.local_dial_address(&remote_address.ip()) {
            Ok(Some(dial_address)) => {
                socket.set_reuse_address(true)?;
                #[cfg(unix)]
                socket.set_reuse_port(true)?;
                socket.bind(&dial_address.into())?;
            }
            Ok(None) => {}
            Err(()) => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?remote_address,
                    "tcp listener not enabled for remote address, using ephemeral port",
                );
            }
        }

        let future = async move {
            match socket.connect(&remote_address.into()) {
                Ok(()) => {}
                Err(error) if error.raw_os_error() == Some(libc::EINPROGRESS) => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(err) => return Err(DialError::from(err)),
            }

            let stream = TcpStream::try_from(Into::<std::net::TcpStream>::into(socket))?;
            stream.writable().await?;
            if let Some(e) = stream.take_error()? {
                return Err(DialError::from(e));
            }

            Ok((
                address,
                tokio_tungstenite::client_async_tls(url, stream)
                    .await
                    .map_err(NegotiationError::WebSocket)?
                    .0,
            ))
        };

        match tokio::time::timeout(connection_open_timeout, future).await {
            Err(_) => Err(DialError::Timeout),
            Ok(Err(error)) => Err(error),
            Ok(Ok((address, stream))) => Ok((address, stream)),
        }
    }
}

impl TransportBuilder for WebSocketTransport {
    type Config = Config;
    type Transport = WebSocketTransport;

    /// Create new [`Transport`] object.
    fn new(
        context: TransportHandle,
        mut config: Self::Config,
        resolver: Arc<TokioResolver>,
    ) -> crate::Result<(Self, Vec<Multiaddr>)>
    where
        Self: Sized,
    {
        tracing::debug!(
            target: LOG_TARGET,
            listen_addresses = ?config.listen_addresses,
            "start websocket transport",
        );
        let (listener, listen_addresses, dial_addresses) = SocketListener::new::<WebSocketAddress>(
            std::mem::take(&mut config.listen_addresses),
            config.reuse_port,
            config.nodelay,
        );

        Ok((
            Self {
                listener,
                config,
                context,
                dial_addresses,
                opened: HashMap::new(),
                pending_open: HashMap::new(),
                pending_dials: HashMap::new(),
                pending_inbound_connections: HashMap::new(),
                pending_connections: FuturesStream::new(),
                pending_raw_connections: FuturesStream::new(),
                cancel_futures: HashMap::new(),
                resolver,
            },
            listen_addresses,
        ))
    }
}

impl Transport for WebSocketTransport {
    fn dial(&mut self, connection_id: ConnectionId, address: Multiaddr) -> crate::Result<()> {
        let yamux_config = self.config.yamux_config.clone();
        let keypair = self.context.keypair.clone();
        let (ws_address, peer) = Self::multiaddr_into_url(address.clone())?;
        let connection_open_timeout = self.config.connection_open_timeout;
        let max_read_ahead_factor = self.config.noise_read_ahead_frame_count;
        let max_write_buffer_size = self.config.noise_write_buffer_size;
        let substream_open_timeout = self.config.substream_open_timeout;
        let dial_addresses = self.dial_addresses.clone();
        let nodelay = self.config.nodelay;
        let resolver = self.resolver.clone();

        self.pending_dials.insert(connection_id, address.clone());

        tracing::debug!(target: LOG_TARGET, ?connection_id, ?address, "open connection");

        let future = async move {
            let (_, stream) = WebSocketTransport::dial_peer(
                address.clone(),
                dial_addresses,
                connection_open_timeout,
                nodelay,
                resolver,
            )
            .await
            .map_err(|error| (connection_id, error))?;

            WebSocketConnection::open_connection(
                connection_id,
                keypair,
                stream,
                address,
                peer,
                ws_address,
                yamux_config,
                max_read_ahead_factor,
                max_write_buffer_size,
                substream_open_timeout,
            )
            .await
            .map_err(|error| (connection_id, error.into()))
        };

        self.pending_connections.push(Box::pin(async move {
            match tokio::time::timeout(connection_open_timeout, future).await {
                Err(_) => Err((connection_id, DialError::Timeout)),
                Ok(Err(error)) => Err(error),
                Ok(Ok(result)) => Ok(result),
            }
        }));

        Ok(())
    }

    fn accept(&mut self, connection_id: ConnectionId) -> crate::Result<()> {
        let context = self
            .pending_open
            .remove(&connection_id)
            .ok_or(Error::ConnectionDoesntExist(connection_id))?;
        let protocol_set = self.context.protocol_set(connection_id);
        let bandwidth_sink = self.context.bandwidth_sink.clone();
        let substream_open_timeout = self.config.substream_open_timeout;

        tracing::trace!(
            target: LOG_TARGET,
            ?connection_id,
            "start connection",
        );

        self.context.executor.run(Box::pin(async move {
            if let Err(error) = WebSocketConnection::new(
                context,
                protocol_set,
                bandwidth_sink,
                substream_open_timeout,
            )
            .start()
            .await
            {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?connection_id,
                    ?error,
                    "connection exited with error",
                );
            }
        }));

        Ok(())
    }

    fn reject(&mut self, connection_id: ConnectionId) -> crate::Result<()> {
        self.pending_open
            .remove(&connection_id)
            .map_or(Err(Error::ConnectionDoesntExist(connection_id)), |_| Ok(()))
    }

    fn accept_pending(&mut self, connection_id: ConnectionId) -> crate::Result<()> {
        let pending = self.pending_inbound_connections.remove(&connection_id).ok_or_else(|| {
            tracing::error!(
                target: LOG_TARGET,
                ?connection_id,
                "Cannot accept non existent pending connection",
            );

            Error::ConnectionDoesntExist(connection_id)
        })?;

        self.on_inbound_connection(connection_id, pending.connection, pending.address);

        Ok(())
    }

    fn reject_pending(&mut self, connection_id: ConnectionId) -> crate::Result<()> {
        self.pending_inbound_connections.remove(&connection_id).map_or_else(
            || {
                tracing::error!(
                    target: LOG_TARGET,
                    ?connection_id,
                    "Cannot reject non existent pending connection",
                );

                Err(Error::ConnectionDoesntExist(connection_id))
            },
            |_| Ok(()),
        )
    }

    fn open(
        &mut self,
        connection_id: ConnectionId,
        addresses: Vec<Multiaddr>,
    ) -> crate::Result<()> {
        let num_addresses = addresses.len();

        let mut futures: FuturesUnordered<_> = addresses
            .into_iter()
            .map(|address| {
                let yamux_config = self.config.yamux_config.clone();
                let keypair = self.context.keypair.clone();
                let connection_open_timeout = self.config.connection_open_timeout;
                let max_read_ahead_factor = self.config.noise_read_ahead_frame_count;
                let max_write_buffer_size = self.config.noise_write_buffer_size;
                let substream_open_timeout = self.config.substream_open_timeout;
                let dial_addresses = self.dial_addresses.clone();
                let nodelay = self.config.nodelay;
                let resolver = self.resolver.clone();

                async move {
                    let (address, stream) = WebSocketTransport::dial_peer(
                        address.clone(),
                        dial_addresses,
                        connection_open_timeout,
                        nodelay,
                        resolver,
                    )
                    .await
                    .map_err(|error| (address, error))?;

                    let open_address = address.clone();
                    let (ws_address, peer) = Self::multiaddr_into_url(address.clone())
                        .map_err(|error| (address.clone(), error.into()))?;

                    WebSocketConnection::open_connection(
                        connection_id,
                        keypair,
                        stream,
                        address,
                        peer,
                        ws_address,
                        yamux_config,
                        max_read_ahead_factor,
                        max_write_buffer_size,
                        substream_open_timeout,
                    )
                    .await
                    .map_err(|error| (open_address, error.into()))
                }
            })
            .collect();

        // Future that will resolve to the first successful connection.
        let future = async move {
            let mut errors = Vec::with_capacity(num_addresses);
            while let Some(result) = futures.next().await {
                match result {
                    Ok(negotiated) => return RawConnectionResult::Connected { negotiated, errors },
                    Err(error) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            ?connection_id,
                            ?error,
                            "failed to open connection",
                        );
                        errors.push(error)
                    }
                }
            }

            RawConnectionResult::Failed {
                connection_id,
                errors,
            }
        };

        let (fut, handle) = futures::future::abortable(future);
        let fut = fut.unwrap_or_else(move |_| RawConnectionResult::Canceled { connection_id });
        self.pending_raw_connections.push(Box::pin(fut));
        self.cancel_futures.insert(connection_id, handle);

        Ok(())
    }

    fn negotiate(&mut self, connection_id: ConnectionId) -> crate::Result<()> {
        let negotiated = self
            .opened
            .remove(&connection_id)
            .ok_or(Error::ConnectionDoesntExist(connection_id))?;

        self.pending_connections.push(Box::pin(async move { Ok(negotiated) }));

        Ok(())
    }

    fn cancel(&mut self, connection_id: ConnectionId) {
        // Cancel the future if it exists.
        // State clean-up happens inside the `poll_next`.
        if let Some(handle) = self.cancel_futures.get(&connection_id) {
            handle.abort();
        }
    }
}

impl Stream for WebSocketTransport {
    type Item = TransportEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(event) = self.listener.poll_next_unpin(cx) {
            return match event {
                None => {
                    tracing::error!(
                        target: LOG_TARGET,
                        "Websocket listener terminated, ignore if the node is stopping",
                    );

                    Poll::Ready(None)
                }
                Some(Err(error)) => {
                    tracing::error!(
                        target: LOG_TARGET,
                        ?error,
                        "Websocket listener terminated with error",
                    );

                    Poll::Ready(None)
                }
                Some(Ok((connection, address))) => {
                    let connection_id = self.context.next_connection_id();
                    tracing::trace!(
                        target: LOG_TARGET,
                        ?connection_id,
                        ?address,
                        "pending inbound Websocket connection",
                    );

                    self.pending_inbound_connections.insert(
                        connection_id,
                        PendingInboundConnection {
                            connection,
                            address,
                        },
                    );

                    Poll::Ready(Some(TransportEvent::PendingInboundConnection {
                        connection_id,
                    }))
                }
            };
        }

        while let Poll::Ready(Some(result)) = self.pending_raw_connections.poll_next_unpin(cx) {
            tracing::trace!(target: LOG_TARGET, ?result, "raw connection result");

            match result {
                RawConnectionResult::Connected { negotiated, errors } => {
                    let Some(handle) = self.cancel_futures.remove(&negotiated.connection_id())
                    else {
                        tracing::warn!(
                            target: LOG_TARGET,
                            connection_id = ?negotiated.connection_id(),
                            address = ?negotiated.endpoint().address(),
                            ?errors,
                            "raw connection without a cancel handle",
                        );
                        continue;
                    };

                    if !handle.is_aborted() {
                        let connection_id = negotiated.connection_id();
                        let address = negotiated.endpoint().address().clone();

                        self.opened.insert(connection_id, negotiated);

                        return Poll::Ready(Some(TransportEvent::ConnectionOpened {
                            connection_id,
                            address,
                        }));
                    }
                }

                RawConnectionResult::Failed {
                    connection_id,
                    errors,
                } => {
                    let Some(handle) = self.cancel_futures.remove(&connection_id) else {
                        tracing::warn!(
                            target: LOG_TARGET,
                            ?connection_id,
                            ?errors,
                            "raw connection without a cancel handle",
                        );
                        continue;
                    };

                    if !handle.is_aborted() {
                        return Poll::Ready(Some(TransportEvent::OpenFailure {
                            connection_id,
                            errors,
                        }));
                    }
                }
                RawConnectionResult::Canceled { connection_id } => {
                    if self.cancel_futures.remove(&connection_id).is_none() {
                        tracing::warn!(
                            target: LOG_TARGET,
                            ?connection_id,
                            "raw cancelled connection without a cancel handle",
                        );
                    }
                }
            }
        }

        while let Poll::Ready(Some(connection)) = self.pending_connections.poll_next_unpin(cx) {
            match connection {
                Ok(connection) => {
                    let peer = connection.peer();
                    let endpoint = connection.endpoint();
                    self.pending_dials.remove(&connection.connection_id());
                    self.pending_open.insert(connection.connection_id(), connection);

                    return Poll::Ready(Some(TransportEvent::ConnectionEstablished {
                        peer,
                        endpoint,
                    }));
                }
                Err((connection_id, error)) => {
                    if let Some(address) = self.pending_dials.remove(&connection_id) {
                        return Poll::Ready(Some(TransportEvent::DialFailure {
                            connection_id,
                            address,
                            error,
                        }));
                    } else {
                        tracing::debug!(target: LOG_TARGET, ?error, ?connection_id, "Pending inbound connection failed");
                    }
                }
            }
        }

        Poll::Pending
    }
}
