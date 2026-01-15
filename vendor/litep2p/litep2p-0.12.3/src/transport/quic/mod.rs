// Copyright 2021 Parity Technologies (UK) Ltd.
// Copyright 2022 Protocol Labs.
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

//! QUIC transport.

use crate::{
    crypto::tls::make_client_config,
    error::{AddressError, DialError, Error, QuicError},
    transport::{
        manager::TransportHandle,
        quic::{config::Config as QuicConfig, connection::QuicConnection, listener::QuicListener},
        Endpoint as Litep2pEndpoint, Transport, TransportBuilder, TransportEvent,
    },
    types::ConnectionId,
    PeerId,
};

use futures::{
    future::BoxFuture,
    stream::{AbortHandle, FuturesUnordered},
    Stream, StreamExt, TryFutureExt,
};
use hickory_resolver::TokioResolver;
use multiaddr::{Multiaddr, Protocol};
use quinn::{ClientConfig, Connecting, Connection, Endpoint, IdleTimeout};

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

pub(crate) use substream::Substream;

mod connection;
mod listener;
mod substream;

pub mod config;

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::quic";

#[derive(Debug)]
struct NegotiatedConnection {
    /// Remote peer ID.
    peer: PeerId,

    /// QUIC connection.
    connection: Connection,
}

#[derive(Debug)]
enum RawConnectionResult {
    /// The first successful connection.
    Connected {
        connection_id: ConnectionId,
        address: Multiaddr,
        stream: NegotiatedConnection,
    },

    /// All connection attempts failed.
    Failed {
        connection_id: ConnectionId,
        errors: Vec<(Multiaddr, DialError)>,
    },

    /// Future was canceled.
    Canceled { connection_id: ConnectionId },
}

/// QUIC transport object.
pub(crate) struct QuicTransport {
    /// Transport handle.
    context: TransportHandle,

    /// Transport config.
    config: QuicConfig,

    /// QUIC listener.
    listener: QuicListener,

    /// Pending dials.
    pending_dials: HashMap<ConnectionId, Multiaddr>,

    /// Pending inbound connections.
    pending_inbound_connections: HashMap<ConnectionId, Connecting>,

    /// Pending connections.
    pending_connections: FuturesUnordered<
        BoxFuture<'static, (ConnectionId, Result<NegotiatedConnection, DialError>)>,
    >,

    /// Negotiated connections waiting for validation.
    pending_open: HashMap<ConnectionId, (NegotiatedConnection, Litep2pEndpoint)>,

    /// Pending raw, unnegotiated connections.
    pending_raw_connections: FuturesUnordered<BoxFuture<'static, RawConnectionResult>>,

    /// Opened raw connection, waiting for approval/rejection from `TransportManager`.
    opened_raw: HashMap<ConnectionId, (NegotiatedConnection, Multiaddr)>,

    /// Cancel raw connections futures.
    ///
    /// This is cancelling `Self::pending_raw_connections`.
    cancel_futures: HashMap<ConnectionId, AbortHandle>,
}

impl QuicTransport {
    /// Attempt to extract `PeerId` from connection certificates.
    fn extract_peer_id(connection: &Connection) -> Option<PeerId> {
        let certificates: Box<Vec<rustls::Certificate>> =
            connection.peer_identity()?.downcast().ok()?;
        let p2p_cert = crate::crypto::tls::certificate::parse(certificates.first()?)
            .expect("the certificate was validated during TLS handshake; qed");

        Some(p2p_cert.peer_id())
    }

    /// Handle inbound accepted connection.
    fn on_inbound_connection(&mut self, connection_id: ConnectionId, connection: Connecting) {
        self.pending_connections.push(Box::pin(async move {
            let connection = match connection.await {
                Ok(connection) => connection,
                Err(error) => return (connection_id, Err(DialError::from(error))),
            };

            let Some(peer) = Self::extract_peer_id(&connection) else {
                return (
                    connection_id,
                    Err(crate::error::NegotiationError::Quic(QuicError::InvalidCertificate).into()),
                );
            };

            (connection_id, Ok(NegotiatedConnection { peer, connection }))
        }));
    }

    /// Handle established connection.
    fn on_connection_established(
        &mut self,
        connection_id: ConnectionId,
        result: Result<NegotiatedConnection, DialError>,
    ) -> Option<TransportEvent> {
        tracing::debug!(target: LOG_TARGET, ?connection_id, success = result.is_ok(), "connection established");

        // `on_connection_established()` is called for both inbound and outbound connections
        // but `pending_dials` will only contain entries for outbound connections.
        let maybe_address = self.pending_dials.remove(&connection_id);

        match result {
            Ok(connection) => {
                let peer = connection.peer;
                let endpoint = maybe_address.map_or(
                    {
                        let address = connection.connection.remote_address();
                        Litep2pEndpoint::listener(
                            Multiaddr::empty()
                                .with(Protocol::from(address.ip()))
                                .with(Protocol::Udp(address.port()))
                                .with(Protocol::QuicV1),
                            connection_id,
                        )
                    },
                    |address| Litep2pEndpoint::dialer(address, connection_id),
                );
                self.pending_open.insert(connection_id, (connection, endpoint.clone()));

                return Some(TransportEvent::ConnectionEstablished { peer, endpoint });
            }
            Err(error) => {
                tracing::debug!(target: LOG_TARGET, ?connection_id, ?error, "failed to establish connection");

                // since the address was found from `pending_dials`,
                // report the error to protocols and `TransportManager`
                if let Some(address) = maybe_address {
                    return Some(TransportEvent::DialFailure {
                        connection_id,
                        address,
                        error,
                    });
                }
            }
        }

        None
    }
}

impl TransportBuilder for QuicTransport {
    type Config = QuicConfig;
    type Transport = QuicTransport;

    /// Create new [`QuicTransport`] object.
    fn new(
        context: TransportHandle,
        mut config: Self::Config,
        _resolver: Arc<TokioResolver>,
    ) -> crate::Result<(Self, Vec<Multiaddr>)>
    where
        Self: Sized,
    {
        tracing::info!(
            target: LOG_TARGET,
            ?config,
            "start quic transport",
        );

        let (listener, listen_addresses) = QuicListener::new(
            &context.keypair,
            std::mem::take(&mut config.listen_addresses),
        )?;

        Ok((
            Self {
                context,
                config,
                listener,
                opened_raw: HashMap::new(),
                pending_open: HashMap::new(),
                pending_dials: HashMap::new(),
                pending_inbound_connections: HashMap::new(),
                pending_raw_connections: FuturesUnordered::new(),
                pending_connections: FuturesUnordered::new(),
                cancel_futures: HashMap::new(),
            },
            listen_addresses,
        ))
    }
}

impl Transport for QuicTransport {
    fn dial(&mut self, connection_id: ConnectionId, address: Multiaddr) -> crate::Result<()> {
        let Ok((socket_address, Some(peer))) = QuicListener::get_socket_address(&address) else {
            return Err(Error::AddressError(AddressError::PeerIdMissing));
        };

        let crypto_config =
            Arc::new(make_client_config(&self.context.keypair, Some(peer)).expect("to succeed"));
        let mut transport_config = quinn::TransportConfig::default();
        let timeout =
            IdleTimeout::try_from(self.config.connection_open_timeout).expect("to succeed");
        transport_config.max_idle_timeout(Some(timeout));
        let mut client_config = ClientConfig::new(crypto_config);
        client_config.transport_config(Arc::new(transport_config));

        let client_listen_address = match address.iter().next() {
            Some(Protocol::Ip6(_)) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
            Some(Protocol::Ip4(_)) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
            _ => return Err(Error::AddressError(AddressError::InvalidProtocol)),
        };

        let client = Endpoint::client(client_listen_address)
            .map_err(|error| Error::Other(error.to_string()))?;
        let connection = client
            .connect_with(client_config, socket_address, "l")
            .map_err(|error| Error::Other(error.to_string()))?;

        tracing::trace!(
            target: LOG_TARGET,
            ?address,
            ?peer,
            ?client_listen_address,
            "dial peer",
        );

        self.pending_dials.insert(connection_id, address);

        self.pending_connections.push(Box::pin(async move {
            let connection = match connection.await {
                Ok(connection) => connection,
                Err(error) => return (connection_id, Err(DialError::from(error))),
            };

            let Some(peer) = Self::extract_peer_id(&connection) else {
                return (
                    connection_id,
                    Err(crate::error::NegotiationError::Quic(QuicError::InvalidCertificate).into()),
                );
            };

            (connection_id, Ok(NegotiatedConnection { peer, connection }))
        }));

        Ok(())
    }

    fn accept(&mut self, connection_id: ConnectionId) -> crate::Result<()> {
        let (connection, endpoint) = self
            .pending_open
            .remove(&connection_id)
            .ok_or(Error::ConnectionDoesntExist(connection_id))?;
        let bandwidth_sink = self.context.bandwidth_sink.clone();
        let protocol_set = self.context.protocol_set(connection_id);
        let substream_open_timeout = self.config.substream_open_timeout;

        tracing::trace!(
            target: LOG_TARGET,
            ?connection_id,
            "start connection",
        );

        self.context.executor.run(Box::pin(async move {
            let _ = QuicConnection::new(
                connection.peer,
                endpoint,
                connection.connection,
                protocol_set,
                bandwidth_sink,
                substream_open_timeout,
            )
            .start()
            .await;
        }));

        Ok(())
    }

    fn reject(&mut self, connection_id: ConnectionId) -> crate::Result<()> {
        self.pending_open
            .remove(&connection_id)
            .map_or(Err(Error::ConnectionDoesntExist(connection_id)), |_| Ok(()))
    }

    fn accept_pending(&mut self, connection_id: ConnectionId) -> crate::Result<()> {
        let connection = self
            .pending_inbound_connections
            .remove(&connection_id)
            .ok_or(Error::ConnectionDoesntExist(connection_id))?;

        self.on_inbound_connection(connection_id, connection);

        Ok(())
    }

    fn reject_pending(&mut self, connection_id: ConnectionId) -> crate::Result<()> {
        self.pending_inbound_connections
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
                let keypair = self.context.keypair.clone();
                let connection_open_timeout = self.config.connection_open_timeout;
                let addr = address.clone();

                let future = async move {
                    let (socket_address, peer) = QuicListener::get_socket_address(&address)
                        .map_err(DialError::AddressError)?;
                    let peer =
                        peer.ok_or_else(|| DialError::AddressError(AddressError::PeerIdMissing))?;

                    let crypto_config =
                        Arc::new(make_client_config(&keypair, Some(peer)).expect("to succeed"));
                    let mut transport_config = quinn::TransportConfig::default();
                    let timeout =
                        IdleTimeout::try_from(connection_open_timeout).expect("to succeed");
                    transport_config.max_idle_timeout(Some(timeout));
                    let mut client_config = ClientConfig::new(crypto_config);
                    client_config.transport_config(Arc::new(transport_config));

                    let client_listen_address = match address.iter().next() {
                        Some(Protocol::Ip6(_)) =>
                            SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
                        Some(Protocol::Ip4(_)) =>
                            SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
                        _ => return Err(AddressError::InvalidProtocol.into()),
                    };

                    let client = match Endpoint::client(client_listen_address) {
                        Ok(client) => client,
                        Err(error) => {
                            return Err(DialError::from(error));
                        }
                    };
                    let connection = match client.connect_with(client_config, socket_address, "l") {
                        Ok(connection) => connection,
                        Err(error) => return Err(DialError::from(error)),
                    };

                    let connection = match connection.await {
                        Ok(connection) => connection,
                        Err(error) => return Err(DialError::from(error)),
                    };

                    let Some(peer) = Self::extract_peer_id(&connection) else {
                        return Err(crate::error::NegotiationError::Quic(
                            QuicError::InvalidCertificate,
                        )
                        .into());
                    };

                    Ok(NegotiatedConnection { peer, connection })
                };

                async move { future.await.map(|ok| (addr.clone(), ok)).map_err(|err| (addr, err)) }
            })
            .collect();

        // Future that will resolve to the first successful connection.
        let future = async move {
            let mut errors = Vec::with_capacity(num_addresses);

            while let Some(result) = futures.next().await {
                match result {
                    Ok((address, stream)) =>
                        return RawConnectionResult::Connected {
                            connection_id,
                            address,
                            stream,
                        },
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
        let (connection, _address) = self
            .opened_raw
            .remove(&connection_id)
            .ok_or(Error::ConnectionDoesntExist(connection_id))?;

        self.pending_connections
            .push(Box::pin(async move { (connection_id, Ok(connection)) }));

        Ok(())
    }

    /// Cancel opening connections.
    fn cancel(&mut self, connection_id: ConnectionId) {
        // Cancel the future if it exists.
        // State clean-up happens inside the `poll_next`.
        if let Some(handle) = self.cancel_futures.get(&connection_id) {
            handle.abort();
        }
    }
}

impl Stream for QuicTransport {
    type Item = TransportEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(Some(connection)) = self.listener.poll_next_unpin(cx) {
            let connection_id = self.context.next_connection_id();

            tracing::trace!(
                target: LOG_TARGET,
                ?connection_id,
                "pending inbound connection",
            );

            self.pending_inbound_connections.insert(connection_id, connection);

            return Poll::Ready(Some(TransportEvent::PendingInboundConnection {
                connection_id,
            }));
        }

        while let Poll::Ready(Some(result)) = self.pending_raw_connections.poll_next_unpin(cx) {
            tracing::trace!(target: LOG_TARGET, ?result, "raw connection result");

            match result {
                RawConnectionResult::Connected {
                    connection_id,
                    address,
                    stream,
                } => {
                    let Some(handle) = self.cancel_futures.remove(&connection_id) else {
                        tracing::warn!(
                            target: LOG_TARGET,
                            ?connection_id,
                            ?address,
                            "raw connection without a cancel handle",
                        );
                        continue;
                    };

                    if !handle.is_aborted() {
                        self.opened_raw.insert(connection_id, (stream, address.clone()));

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
            let (connection_id, result) = connection;

            match self.on_connection_established(connection_id, result) {
                Some(event) => return Poll::Ready(Some(event)),
                None => {}
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
        transport::manager::{ProtocolContext, TransportHandle},
        types::protocol::ProtocolName,
        BandwidthSink,
    };
    use multihash::Multihash;
    use tokio::sync::mpsc::channel;

    #[tokio::test]
    async fn test_quinn() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let keypair1 = Keypair::generate();
        let (tx1, _rx1) = channel(64);
        let (event_tx1, _event_rx1) = channel(64);

        let handle1 = TransportHandle {
            executor: Arc::new(DefaultExecutor {}),
            next_substream_id: Default::default(),
            next_connection_id: Default::default(),
            keypair: keypair1.clone(),
            tx: event_tx1,
            bandwidth_sink: BandwidthSink::new(),

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

        let (mut transport1, listen_addresses) =
            QuicTransport::new(handle1, Default::default(), resolver.clone()).unwrap();
        let listen_address = listen_addresses[0].clone();

        let keypair2 = Keypair::generate();
        let (tx2, _rx2) = channel(64);
        let (event_tx2, _event_rx2) = channel(64);

        let handle2 = TransportHandle {
            executor: Arc::new(DefaultExecutor {}),
            next_substream_id: Default::default(),
            next_connection_id: Default::default(),
            keypair: keypair2.clone(),
            tx: event_tx2,
            bandwidth_sink: BandwidthSink::new(),

            protocols: HashMap::from_iter([(
                ProtocolName::from("/notif/1"),
                ProtocolContext {
                    tx: tx2,
                    codec: ProtocolCodec::Identity(32),
                    fallback_names: Vec::new(),
                },
            )]),
        };

        let (mut transport2, _) =
            QuicTransport::new(handle2, Default::default(), resolver).unwrap();
        let peer1: PeerId = PeerId::from_public_key(&keypair1.public().into());
        let _peer2: PeerId = PeerId::from_public_key(&keypair2.public().into());
        let listen_address = listen_address.with(Protocol::P2p(
            Multihash::from_bytes(&peer1.to_bytes()).unwrap(),
        ));

        transport2.dial(ConnectionId::new(), listen_address).unwrap();

        let event = transport1.next().await.unwrap();
        match event {
            TransportEvent::PendingInboundConnection { connection_id } => {
                transport1.accept_pending(connection_id).unwrap();
            }
            _ => panic!("unexpected event"),
        }

        let (res1, res2) = tokio::join!(transport1.next(), transport2.next());

        assert!(std::matches!(
            res1,
            Some(TransportEvent::ConnectionEstablished { .. })
        ));
        assert!(std::matches!(
            res2,
            Some(TransportEvent::ConnectionEstablished { .. })
        ));
    }
}
