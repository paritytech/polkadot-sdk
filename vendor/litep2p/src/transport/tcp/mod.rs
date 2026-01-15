// Copyright 2020 Parity Technologies (UK) Ltd.
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

//! TCP transport.

use crate::{
    error::{DialError, Error},
    transport::{
        common::listener::{DialAddresses, GetSocketAddr, SocketListener, TcpAddress},
        manager::TransportHandle,
        tcp::{
            config::Config,
            connection::{NegotiatedConnection, TcpConnection},
        },
        Transport, TransportBuilder, TransportEvent,
    },
    types::ConnectionId,
    utils::futures_stream::FuturesStream,
};

use futures::{
    future::BoxFuture,
    stream::{AbortHandle, FuturesUnordered, Stream, StreamExt},
    TryFutureExt,
};
use hickory_resolver::TokioResolver;
use multiaddr::Multiaddr;
use socket2::{Domain, Socket, Type};
use tokio::net::TcpStream;

use std::{
    collections::HashMap,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

pub(crate) use substream::Substream;

mod connection;
mod substream;

pub mod config;

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::tcp";

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

/// TCP transport.
pub(crate) struct TcpTransport {
    /// Transport context.
    context: TransportHandle,

    /// Transport configuration.
    config: Config,

    /// TCP listener.
    listener: SocketListener,

    /// Pending dials.
    pending_dials: HashMap<ConnectionId, Multiaddr>,

    /// Dial addresses.
    dial_addresses: DialAddresses,

    /// Pending inbound connections.
    pending_inbound_connections: HashMap<ConnectionId, PendingInboundConnection>,

    /// Pending opening connections.
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

    /// Connections which have been opened and negotiated but are being validated by the
    /// `TransportManager`.
    pending_open: HashMap<ConnectionId, NegotiatedConnection>,

    /// DNS resolver.
    resolver: Arc<TokioResolver>,
}

impl TcpTransport {
    /// Handle inbound TCP connection.
    fn on_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        connection: TcpStream,
        address: SocketAddr,
    ) {
        let yamux_config = self.config.yamux_config.clone();
        let max_read_ahead_factor = self.config.noise_read_ahead_frame_count;
        let max_write_buffer_size = self.config.noise_write_buffer_size;
        let connection_open_timeout = self.config.connection_open_timeout;
        let substream_open_timeout = self.config.substream_open_timeout;
        let keypair = self.context.keypair.clone();

        tracing::trace!(
            target: LOG_TARGET,
            ?connection_id,
            ?address,
            "accept connection",
        );

        self.pending_connections.push(Box::pin(async move {
            TcpConnection::accept_connection(
                connection,
                connection_id,
                keypair,
                address,
                yamux_config,
                max_read_ahead_factor,
                max_write_buffer_size,
                connection_open_timeout,
                substream_open_timeout,
            )
            .await
            .map_err(|error| (connection_id, error.into()))
        }));
    }

    /// Dial remote peer
    async fn dial_peer(
        address: Multiaddr,
        dial_addresses: DialAddresses,
        connection_open_timeout: Duration,
        nodelay: bool,
        resolver: Arc<TokioResolver>,
    ) -> Result<(Multiaddr, TcpStream), DialError> {
        let (socket_address, _) = TcpAddress::multiaddr_to_socket_address(&address)?;

        let remote_address =
            match tokio::time::timeout(connection_open_timeout, socket_address.lookup_ip(resolver))
                .await
            {
                Err(_) => {
                    tracing::debug!(
                        target: LOG_TARGET,
                        ?address,
                        ?connection_open_timeout,
                        "failed to resolve address within timeout",
                    );
                    return Err(DialError::Timeout);
                }
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
                Err(err) if err.raw_os_error() == Some(libc::EINPROGRESS) => {}
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(err) => return Err(err),
            }

            let stream = TcpStream::try_from(Into::<std::net::TcpStream>::into(socket))?;
            stream.writable().await?;

            if let Some(e) = stream.take_error()? {
                return Err(e);
            }

            Ok((address, stream))
        };

        match tokio::time::timeout(connection_open_timeout, future).await {
            Err(_) => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?connection_open_timeout,
                    "failed to connect within timeout",
                );
                Err(DialError::Timeout)
            }
            Ok(Err(error)) => Err(error.into()),
            Ok(Ok((address, stream))) => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?address,
                    "connected",
                );

                Ok((address, stream))
            }
        }
    }
}

impl TransportBuilder for TcpTransport {
    type Config = Config;
    type Transport = TcpTransport;

    /// Create new [`TcpTransport`].
    fn new(
        context: TransportHandle,
        mut config: Self::Config,
        resolver: Arc<TokioResolver>,
    ) -> crate::Result<(Self, Vec<Multiaddr>)> {
        tracing::debug!(
            target: LOG_TARGET,
            listen_addresses = ?config.listen_addresses,
            "start tcp transport",
        );

        // start tcp listeners for all listen addresses
        let (listener, listen_addresses, dial_addresses) = SocketListener::new::<TcpAddress>(
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

impl Transport for TcpTransport {
    fn dial(&mut self, connection_id: ConnectionId, address: Multiaddr) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, ?connection_id, ?address, "open connection");

        let (socket_address, peer) = TcpAddress::multiaddr_to_socket_address(&address)?;
        let yamux_config = self.config.yamux_config.clone();
        let max_read_ahead_factor = self.config.noise_read_ahead_frame_count;
        let max_write_buffer_size = self.config.noise_write_buffer_size;
        let connection_open_timeout = self.config.connection_open_timeout;
        let substream_open_timeout = self.config.substream_open_timeout;
        let dial_addresses = self.dial_addresses.clone();
        let keypair = self.context.keypair.clone();
        let nodelay = self.config.nodelay;
        let resolver = self.resolver.clone();

        self.pending_dials.insert(connection_id, address.clone());
        self.pending_connections.push(Box::pin(async move {
            let (_, stream) = TcpTransport::dial_peer(
                address,
                dial_addresses,
                connection_open_timeout,
                nodelay,
                resolver,
            )
            .await
            .map_err(|error| (connection_id, error))?;

            TcpConnection::open_connection(
                connection_id,
                keypair,
                stream,
                socket_address,
                peer,
                yamux_config,
                max_read_ahead_factor,
                max_write_buffer_size,
                connection_open_timeout,
                substream_open_timeout,
            )
            .await
            .map_err(|error| (connection_id, error.into()))
        }));

        Ok(())
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

    fn accept(&mut self, connection_id: ConnectionId) -> crate::Result<()> {
        let context = self
            .pending_open
            .remove(&connection_id)
            .ok_or(Error::ConnectionDoesntExist(connection_id))?;
        let protocol_set = self.context.protocol_set(connection_id);
        let bandwidth_sink = self.context.bandwidth_sink.clone();
        let next_substream_id = self.context.next_substream_id.clone();

        tracing::trace!(
            target: LOG_TARGET,
            ?connection_id,
            "start connection",
        );

        self.context.executor.run(Box::pin(async move {
            if let Err(error) =
                TcpConnection::new(context, protocol_set, bandwidth_sink, next_substream_id)
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
                let max_read_ahead_factor = self.config.noise_read_ahead_frame_count;
                let max_write_buffer_size = self.config.noise_write_buffer_size;
                let connection_open_timeout = self.config.connection_open_timeout;
                let substream_open_timeout = self.config.substream_open_timeout;
                let dial_addresses = self.dial_addresses.clone();
                let keypair = self.context.keypair.clone();
                let nodelay = self.config.nodelay;
                let resolver = self.resolver.clone();

                async move {
                    let (address, stream) = TcpTransport::dial_peer(
                        address.clone(),
                        dial_addresses,
                        connection_open_timeout,
                        nodelay,
                        resolver,
                    )
                    .await
                    .map_err(|error| (address, error))?;

                    let open_address = address.clone();
                    let (socket_address, peer) = TcpAddress::multiaddr_to_socket_address(&address)
                        .map_err(|error| (address, error.into()))?;

                    TcpConnection::open_connection(
                        connection_id,
                        keypair,
                        stream,
                        socket_address,
                        peer,
                        yamux_config,
                        max_read_ahead_factor,
                        max_write_buffer_size,
                        connection_open_timeout,
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

impl Stream for TcpTransport {
    type Item = TransportEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(event) = self.listener.poll_next_unpin(cx) {
            return match event {
                None => {
                    tracing::error!(
                        target: LOG_TARGET,
                        "TCP listener terminated, ignore if the node is stopping",
                    );

                    Poll::Ready(None)
                }
                Some(Err(error)) => {
                    tracing::error!(
                        target: LOG_TARGET,
                        ?error,
                        "TCP listener terminated with error",
                    );

                    Poll::Ready(None)
                }
                Some(Ok((connection, address))) => {
                    let connection_id = self.context.next_connection_id();
                    tracing::trace!(
                        target: LOG_TARGET,
                        ?connection_id,
                        ?address,
                        "pending inbound TCP connection",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        codec::ProtocolCodec,
        crypto::ed25519::Keypair,
        executor::DefaultExecutor,
        transport::manager::{ProtocolContext, SupportedTransport, TransportManagerBuilder},
        types::protocol::ProtocolName,
        BandwidthSink, PeerId,
    };
    use multiaddr::Protocol;
    use multihash::Multihash;
    use std::sync::Arc;
    use tokio::sync::mpsc::channel;

    #[tokio::test]
    async fn connect_and_accept_works() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let keypair1 = Keypair::generate();
        let (tx1, _rx1) = channel(64);
        let (event_tx1, _event_rx1) = channel(64);
        let bandwidth_sink = BandwidthSink::new();

        let handle1 = crate::transport::manager::TransportHandle {
            executor: Arc::new(DefaultExecutor {}),
            next_substream_id: Default::default(),
            next_connection_id: Default::default(),
            keypair: keypair1.clone(),
            tx: event_tx1,
            bandwidth_sink: bandwidth_sink.clone(),

            protocols: HashMap::from_iter([(
                ProtocolName::from("/notif/1"),
                ProtocolContext {
                    tx: tx1,
                    codec: ProtocolCodec::Identity(32),
                    fallback_names: Vec::new(),
                },
            )]),
        };
        let transport_config1 = Config {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        };
        let resolver = Arc::new(TokioResolver::builder_tokio().unwrap().build());

        let (mut transport1, listen_addresses) =
            TcpTransport::new(handle1, transport_config1, resolver.clone()).unwrap();
        let listen_address = listen_addresses[0].clone();

        let keypair2 = Keypair::generate();
        let (tx2, _rx2) = channel(64);
        let (event_tx2, _event_rx2) = channel(64);

        let handle2 = crate::transport::manager::TransportHandle {
            executor: Arc::new(DefaultExecutor {}),
            next_substream_id: Default::default(),
            next_connection_id: Default::default(),
            keypair: keypair2.clone(),
            tx: event_tx2,
            bandwidth_sink: bandwidth_sink.clone(),

            protocols: HashMap::from_iter([(
                ProtocolName::from("/notif/1"),
                ProtocolContext {
                    tx: tx2,
                    codec: ProtocolCodec::Identity(32),
                    fallback_names: Vec::new(),
                },
            )]),
        };
        let transport_config2 = Config {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        };

        let (mut transport2, _) = TcpTransport::new(handle2, transport_config2, resolver).unwrap();
        transport2.dial(ConnectionId::new(), listen_address).unwrap();

        let (tx, mut from_transport2) = channel(64);
        tokio::spawn(async move {
            let event = transport2.next().await;
            tx.send(event).await.unwrap();
        });

        let event = transport1.next().await.unwrap();
        match event {
            TransportEvent::PendingInboundConnection { connection_id } => {
                transport1.accept_pending(connection_id).unwrap();
            }
            _ => panic!("unexpected event"),
        }

        let event = transport1.next().await;
        assert!(std::matches!(
            event,
            Some(TransportEvent::ConnectionEstablished { .. })
        ));

        let event = from_transport2.recv().await.unwrap();
        assert!(std::matches!(
            event,
            Some(TransportEvent::ConnectionEstablished { .. })
        ));
    }

    #[tokio::test]
    async fn connect_and_reject_works() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let keypair1 = Keypair::generate();
        let (tx1, _rx1) = channel(64);
        let (event_tx1, _event_rx1) = channel(64);
        let bandwidth_sink = BandwidthSink::new();

        let handle1 = crate::transport::manager::TransportHandle {
            executor: Arc::new(DefaultExecutor {}),
            next_substream_id: Default::default(),
            next_connection_id: Default::default(),
            keypair: keypair1.clone(),
            tx: event_tx1,
            bandwidth_sink: bandwidth_sink.clone(),

            protocols: HashMap::from_iter([(
                ProtocolName::from("/notif/1"),
                ProtocolContext {
                    tx: tx1,
                    codec: ProtocolCodec::Identity(32),
                    fallback_names: Vec::new(),
                },
            )]),
        };
        let transport_config1 = Config {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        };
        let resolver = Arc::new(TokioResolver::builder_tokio().unwrap().build());

        let (mut transport1, listen_addresses) =
            TcpTransport::new(handle1, transport_config1, resolver.clone()).unwrap();
        let listen_address = listen_addresses[0].clone();

        let keypair2 = Keypair::generate();
        let (tx2, _rx2) = channel(64);
        let (event_tx2, _event_rx2) = channel(64);

        let handle2 = crate::transport::manager::TransportHandle {
            executor: Arc::new(DefaultExecutor {}),
            next_substream_id: Default::default(),
            next_connection_id: Default::default(),
            keypair: keypair2.clone(),
            tx: event_tx2,
            bandwidth_sink: bandwidth_sink.clone(),

            protocols: HashMap::from_iter([(
                ProtocolName::from("/notif/1"),
                ProtocolContext {
                    tx: tx2,
                    codec: ProtocolCodec::Identity(32),
                    fallback_names: Vec::new(),
                },
            )]),
        };
        let transport_config2 = Config {
            listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
            ..Default::default()
        };

        let (mut transport2, _) = TcpTransport::new(handle2, transport_config2, resolver).unwrap();
        transport2.dial(ConnectionId::new(), listen_address).unwrap();

        let (tx, mut from_transport2) = channel(64);
        tokio::spawn(async move {
            let event = transport2.next().await;
            tx.send(event).await.unwrap();
        });

        // Reject connection.
        let event = transport1.next().await.unwrap();
        match event {
            TransportEvent::PendingInboundConnection { connection_id } => {
                transport1.reject_pending(connection_id).unwrap();
            }
            _ => panic!("unexpected event"),
        }

        let event = from_transport2.recv().await.unwrap();
        assert!(std::matches!(
            event,
            Some(TransportEvent::DialFailure { .. })
        ));
    }

    #[tokio::test]
    async fn dial_failure() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let keypair1 = Keypair::generate();
        let (tx1, _rx1) = channel(64);
        let (event_tx1, mut event_rx1) = channel(64);
        let bandwidth_sink = BandwidthSink::new();

        let handle1 = crate::transport::manager::TransportHandle {
            executor: Arc::new(DefaultExecutor {}),
            next_substream_id: Default::default(),
            next_connection_id: Default::default(),
            keypair: keypair1.clone(),
            tx: event_tx1,
            bandwidth_sink: bandwidth_sink.clone(),

            protocols: HashMap::from_iter([(
                ProtocolName::from("/notif/1"),
                ProtocolContext {
                    tx: tx1,
                    codec: ProtocolCodec::Identity(32),
                    fallback_names: Vec::new(),
                },
            )]),
        };
        let resolver = Arc::new(TokioResolver::builder_tokio().unwrap().build());
        let (mut transport1, _) =
            TcpTransport::new(handle1, Default::default(), resolver.clone()).unwrap();

        tokio::spawn(async move {
            while let Some(event) = transport1.next().await {
                match event {
                    TransportEvent::ConnectionEstablished { .. } => {}
                    TransportEvent::ConnectionClosed { .. } => {}
                    TransportEvent::DialFailure { .. } => {}
                    TransportEvent::ConnectionOpened { .. } => {}
                    TransportEvent::OpenFailure { .. } => {}
                    TransportEvent::PendingInboundConnection { .. } => {}
                }
            }
        });

        let keypair2 = Keypair::generate();
        let (tx2, _rx2) = channel(64);
        let (event_tx2, _event_rx2) = channel(64);

        let handle2 = crate::transport::manager::TransportHandle {
            executor: Arc::new(DefaultExecutor {}),
            next_substream_id: Default::default(),
            next_connection_id: Default::default(),
            keypair: keypair2.clone(),
            tx: event_tx2,
            bandwidth_sink: bandwidth_sink.clone(),

            protocols: HashMap::from_iter([(
                ProtocolName::from("/notif/1"),
                ProtocolContext {
                    tx: tx2,
                    codec: ProtocolCodec::Identity(32),
                    fallback_names: Vec::new(),
                },
            )]),
        };

        let (mut transport2, _) = TcpTransport::new(handle2, Default::default(), resolver).unwrap();

        let peer1: PeerId = PeerId::from_public_key(&keypair1.public().into());
        let peer2: PeerId = PeerId::from_public_key(&keypair2.public().into());

        tracing::info!(target: LOG_TARGET, "peer1 {peer1}, peer2 {peer2}");

        let address = Multiaddr::empty()
            .with(Protocol::Ip6(std::net::Ipv6Addr::new(
                0, 0, 0, 0, 0, 0, 0, 1,
            )))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer1.to_bytes()).unwrap(),
            ));

        transport2.dial(ConnectionId::new(), address).unwrap();

        // spawn the other connection in the background as it won't return anything
        tokio::spawn(async move {
            loop {
                let _ = event_rx1.recv().await;
            }
        });

        assert!(std::matches!(
            transport2.next().await,
            Some(TransportEvent::DialFailure { .. })
        ));
    }

    #[tokio::test]
    async fn dial_error_reported_for_outbound_connections() {
        let mut manager = TransportManagerBuilder::new().build();
        let handle = manager.transport_handle(Arc::new(DefaultExecutor {}));
        let resolver = Arc::new(TokioResolver::builder_tokio().unwrap().build());
        manager.register_transport(
            SupportedTransport::Tcp,
            Box::new(crate::transport::dummy::DummyTransport::new()),
        );
        let (mut transport, _) = TcpTransport::new(
            handle,
            Config {
                listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
                ..Default::default()
            },
            resolver,
        )
        .unwrap();

        let keypair = Keypair::generate();
        let peer_id = PeerId::from_public_key(&keypair.public().into());
        let multiaddr = Multiaddr::empty()
            .with(Protocol::Ip4(std::net::Ipv4Addr::new(255, 254, 253, 252)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer_id.to_bytes()).unwrap(),
            ));
        manager.dial_address(multiaddr.clone()).await.unwrap();

        assert!(transport.pending_dials.is_empty());

        match transport.dial(ConnectionId::from(0usize), multiaddr) {
            Ok(()) => {}
            _ => panic!("invalid result for `on_dial_peer()`"),
        }

        assert!(!transport.pending_dials.is_empty());
        transport.pending_connections.push(Box::pin(async move {
            Err((ConnectionId::from(0usize), DialError::Timeout))
        }));

        assert!(std::matches!(
            transport.next().await,
            Some(TransportEvent::DialFailure { .. })
        ));
        assert!(transport.pending_dials.is_empty());
    }
}
