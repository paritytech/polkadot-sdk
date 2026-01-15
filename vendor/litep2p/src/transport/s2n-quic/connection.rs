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
    codec::{
        generic::Unspecified, identity::Identity, unsigned_varint::UnsignedVarint, ProtocolCodec,
    },
    config::Role,
    error::Error,
    multistream_select::{dialer_select_proto, listener_select_proto, Negotiated, Version},
    protocol::{Direction, Permit, ProtocolCommand, ProtocolSet},
    substream::Substream as SubstreamT,
    transport::substream::Substream,
    types::{protocol::ProtocolName, ConnectionId, SubstreamId},
    PeerId,
};

use futures::{future::BoxFuture, stream::FuturesUnordered, AsyncRead, AsyncWrite, StreamExt};
use s2n_quic::{
    connection::{Connection, Handle},
    stream::BidirectionalStream,
};
use tokio_util::codec::Framed;

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::quic::connection";

/// QUIC connection error.
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
        error: Error,
    },
}

/// QUIC connection.
pub(crate) struct QuicConnection {
    /// Inner QUIC connection.
    connection: Connection,

    /// Remote peer ID.
    peer: PeerId,

    /// Connection ID.
    connection_id: ConnectionId,

    /// Transport context.
    protocol_set: ProtocolSet,

    /// Pending substreams.
    pending_substreams:
        FuturesUnordered<BoxFuture<'static, Result<NegotiatedSubstream, ConnectionError>>>,
}

#[derive(Debug)]
pub struct NegotiatedSubstream {
    /// Substream direction.
    direction: Direction,

    /// Protocol name.
    protocol: ProtocolName,

    /// `s2n-quic` stream.
    io: BidirectionalStream,

    /// Permit.
    permit: Permit,
}

impl QuicConnection {
    /// Create new [`QuiConnection`].
    pub(crate) fn new(
        peer: PeerId,
        protocol_set: ProtocolSet,
        connection: Connection,
        connection_id: ConnectionId,
    ) -> Self {
        Self {
            peer,
            connection,
            connection_id,
            pending_substreams: FuturesUnordered::new(),
            protocol_set,
        }
    }

    /// Negotiate protocol.
    async fn negotiate_protocol<S: AsyncRead + AsyncWrite + Unpin>(
        stream: S,
        role: &Role,
        protocols: Vec<&str>,
    ) -> crate::Result<(Negotiated<S>, ProtocolName)> {
        tracing::trace!(target: LOG_TARGET, ?protocols, "negotiating protocols");

        let (protocol, socket) = match role {
            Role::Dialer => dialer_select_proto(stream, protocols, Version::V1).await?,
            Role::Listener => listener_select_proto(stream, protocols).await?,
        };

        tracing::trace!(target: LOG_TARGET, ?protocol, "protocol negotiated");

        Ok((socket, ProtocolName::from(protocol.to_string())))
    }

    /// Open substream for `protocol`.
    pub async fn open_substream(
        mut handle: Handle,
        permit: Permit,
        direction: Direction,
        protocol: ProtocolName,
        fallback_names: Vec<ProtocolName>,
    ) -> crate::Result<NegotiatedSubstream> {
        tracing::debug!(target: LOG_TARGET, ?protocol, ?direction, "open substream");

        let stream = match handle.open_bidirectional_stream().await {
            Ok(stream) => {
                tracing::trace!(
                    target: LOG_TARGET,
                    ?protocol,
                    ?direction,
                    id = ?stream.id(),
                    "substream opened"
                );
                stream
            }
            Err(error) => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?direction,
                    ?error,
                    "failed to open substream"
                );
                return Err(Error::Unknown);
            }
        };

        // TODO: https://github.com/paritytech/litep2p/issues/346 protocols don't change after
        // they've been initialized so this should be done only once.
        let protocols = std::iter::once(&*protocol)
            .chain(fallback_names.iter().map(|protocol| &**protocol))
            .collect();

        let (io, protocol) = Self::negotiate_protocol(stream, &Role::Dialer, protocols).await?;

        Ok(NegotiatedSubstream {
            io: io.inner(),
            direction,
            permit,
            protocol,
        })
    }

    /// Accept substream.
    pub async fn accept_substream(
        stream: BidirectionalStream,
        permit: Permit,
        substream_id: SubstreamId,
        protocols: Vec<ProtocolName>,
    ) -> crate::Result<NegotiatedSubstream> {
        tracing::trace!(
            target: LOG_TARGET,
            ?substream_id,
            quic_id = ?stream.id(),
            "accept inbound substream"
        );

        let protocols = protocols.iter().map(|protocol| &**protocol).collect::<Vec<&str>>();
        let (io, protocol) = Self::negotiate_protocol(stream, &Role::Listener, protocols).await?;

        tracing::trace!(
            target: LOG_TARGET,
            ?substream_id,
            ?protocol,
            "substream accepted and negotiated"
        );

        Ok(NegotiatedSubstream {
            io: io.inner(),
            direction: Direction::Inbound,
            protocol,
            permit,
        })
    }

    /// Start [`QuicConnection`] event loop.
    pub(crate) async fn start(mut self) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, "starting quic connection handler");

        loop {
            tokio::select! {
                substream = self.connection.accept_bidirectional_stream() => match substream {
                    Ok(Some(stream)) => {
                        let substream = self.protocol_set.next_substream_id();
                        let protocols = self.protocol_set.protocols();
                        let permit = self.protocol_set.try_get_permit().ok_or(Error::ConnectionClosed)?;

                        self.pending_substreams.push(Box::pin(async move {
                            match tokio::time::timeout(
                                std::time::Duration::from_secs(5), // TODO: https://github.com/paritytech/litep2p/issues/348 make this configurable
                                Self::accept_substream(stream, permit, substream, protocols),
                            )
                            .await
                            {
                                Ok(Ok(substream)) => Ok(substream),
                                Ok(Err(error)) => Err(ConnectionError::FailedToNegotiate {
                                    protocol: None,
                                    substream_id: None,
                                    error,
                                }),
                                Err(_) => Err(ConnectionError::Timeout {
                                    protocol: None,
                                    substream_id: None
                                }),
                            }
                        }));
                    }
                    Ok(None) => {
                        tracing::debug!(target: LOG_TARGET, peer = ?self.peer, "connection closed");
                        self.protocol_set.report_connection_closed(self.peer, self.connection_id).await?;

                        return Ok(())
                    }
                    Err(error) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            peer = ?self.peer,
                            ?error,
                            "connection closed with error"
                        );
                        self.protocol_set.report_connection_closed(self.peer, self.connection_id).await?;

                        return Ok(())
                    }
                },
                substream = self.pending_substreams.select_next_some(), if !self.pending_substreams.is_empty() => {
                    match substream {
                        Err(error) => {
                            tracing::debug!(
                                target: LOG_TARGET,
                                ?error,
                                "failed to accept/open substream",
                            );

                            let (protocol, substream_id, error) = match error {
                                ConnectionError::Timeout { protocol, substream_id } => {
                                    (protocol, substream_id, Error::Timeout)
                                }
                                ConnectionError::FailedToNegotiate { protocol, substream_id, error } => {
                                    (protocol, substream_id, error)
                                }
                            };

                            if let (Some(protocol), Some(substream_id)) = (protocol, substream_id) {
                                if let Err(error) = self.protocol_set
                                    .report_substream_open_failure(protocol, substream_id, error)
                                    .await
                                {
                                    tracing::error!(
                                        target: LOG_TARGET,
                                        ?error,
                                        "failed to register opened substream to protocol"
                                    );
                                }
                            }
                        }
                        Ok(substream) => {
                            let protocol = substream.protocol.clone();
                            let direction = substream.direction;
                            let substream = Substream::new(substream.io, substream.permit);
                            let substream: Box<dyn SubstreamT> = match self.protocol_set.protocol_codec(&protocol) {
                                ProtocolCodec::Identity(payload_size) => {
                                    Box::new(Framed::new(substream, Identity::new(payload_size)))
                                }
                                ProtocolCodec::UnsignedVarint(max_size) => {
                                    Box::new(Framed::new(substream, UnsignedVarint::new(max_size)))
                                }
                                ProtocolCodec::Unspecified => {
                                    Box::new(Framed::new(substream, Generic::new()))
                                }
                            };

                            if let Err(error) = self.protocol_set
                                .report_substream_open(self.peer, protocol, direction, substream)
                                .await
                            {
                                tracing::error!(
                                    target: LOG_TARGET,
                                    ?error,
                                    "failed to register opened substream to protocol"
                                );
                            }
                        }
                    }
                }
                protocol = self.protocol_set.next_event() => match protocol {
                    Some(ProtocolCommand::OpenSubstream { protocol, fallback_names, substream_id, permit, .. }) => {
                        let handle = self.connection.handle();

                        tracing::trace!(
                            target: LOG_TARGET,
                            ?protocol,
                            ?fallback_names,
                            ?substream_id,
                            "open substream"
                        );

                        self.pending_substreams.push(Box::pin(async move {
                            match tokio::time::timeout(
                                std::time::Duration::from_secs(5), // TODO: https://github.com/paritytech/litep2p/issues/348 make this configurable
                                Self::open_substream(
                                    handle,
                                    permit,
                                    Direction::Outbound(substream_id),
                                    protocol.clone(),
                                    fallback_names
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
                                    substream_id: Some(substream_id)
                                }),
                            }
                        }));
                    }
                    None => {
                        tracing::debug!(target: LOG_TARGET, "protocols have exited, shutting down connection");
                        return self.protocol_set.report_connection_closed(self.peer, self.connection_id).await
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        crypto::{
            ed25519::Keypair,
            tls::{certificate::generate, TlsProvider},
            PublicKey,
        },
        protocol::{Transport, TransportEvent},
        transport::manager::{SupportedTransport, TransportManager, TransportManagerEvent},
    };
    use multiaddr::Multiaddr;
    use s2n_quic::{client::Connect, Client, Server};
    use tokio::sync::mpsc::{channel, Receiver};

    // context for testing
    struct QuicContext {
        manager: TransportManager,
        peer: PeerId,
        server: Server,
        client: Client,
        rx: Receiver<PeerId>,
        connect: Connect,
    }

    // prepare quic context for testing
    fn prepare_quic_context() -> QuicContext {
        let keypair = Keypair::generate();
        let (certificate, key) = generate(&keypair).unwrap();
        let (tx, rx) = channel(1);
        let peer = PeerId::from_public_key(&PublicKey::Ed25519(keypair.public()));

        let provider = TlsProvider::new(key, certificate, None, Some(tx.clone()));
        let server = Server::builder()
            .with_tls(provider)
            .expect("TLS provider to be enabled successfully")
            .with_io("127.0.0.1:0")
            .unwrap()
            .start()
            .unwrap();
        let listen_address = server.local_addr().unwrap();

        let keypair = Keypair::generate();
        let (certificate, key) = generate(&keypair).unwrap();
        let provider = TlsProvider::new(key, certificate, Some(peer), None);

        let client = Client::builder()
            .with_tls(provider)
            .expect("TLS provider to be enabled successfully")
            .with_io("0.0.0.0:0")
            .unwrap()
            .start()
            .unwrap();

        let connect = Connect::new(listen_address).with_server_name("localhost");
        let (manager, _handle) = TransportManager::new(keypair.clone());

        QuicContext {
            manager,
            peer,
            server,
            client,
            connect,
            rx,
        }
    }

    #[tokio::test]
    async fn connection_closed() {
        let QuicContext {
            mut manager,
            mut server,
            peer,
            client,
            connect,
            rx: _rx,
        } = prepare_quic_context();

        let res = tokio::join!(server.accept(), client.connect(connect));
        let (Some(connection1), Ok(connection2)) = res else {
            panic!("failed to establish connection");
        };

        let mut service1 = manager.register_protocol(
            ProtocolName::from("/notif/1"),
            Vec::new(),
            ProtocolCodec::UnsignedVarint(None),
        );
        let mut service2 = manager.register_protocol(
            ProtocolName::from("/notif/2"),
            Vec::new(),
            ProtocolCodec::UnsignedVarint(None),
        );
        let transport_handle = manager.register_transport(SupportedTransport::Quic);
        let mut protocol_set = transport_handle.protocol_set();
        protocol_set
            .report_connection_established(ConnectionId::from(0usize), peer, Multiaddr::empty())
            .await
            .unwrap();

        // ignore connection established events
        let _ = service1.next_event().await.unwrap();
        let _ = service2.next_event().await.unwrap();
        let _ = manager.next().await.unwrap();

        tokio::spawn(async move {
            let _ =
                QuicConnection::new(peer, protocol_set, connection1, ConnectionId::from(0usize))
                    .start()
                    .await;
        });

        // drop connection and verify that both protocols are notified of it
        drop(connection2);

        let (
            Some(TransportEvent::ConnectionClosed { .. }),
            Some(TransportEvent::ConnectionClosed { .. }),
        ) = tokio::join!(service1.next_event(), service2.next_event())
        else {
            panic!("invalid event received");
        };

        // verify that the `TransportManager` is also notified about the closed connection
        let Some(TransportManagerEvent::ConnectionClosed { .. }) = manager.next().await else {
            panic!("invalid event received");
        };
    }

    #[tokio::test]
    async fn outbound_substream_timeouts() {
        let QuicContext {
            mut manager,
            mut server,
            peer,
            client,
            connect,
            rx: _rx,
        } = prepare_quic_context();

        let res = tokio::join!(server.accept(), client.connect(connect));
        let (Some(connection1), Ok(_connection2)) = res else {
            panic!("failed to establish connection");
        };

        let mut service1 = manager.register_protocol(
            ProtocolName::from("/notif/1"),
            Vec::new(),
            ProtocolCodec::UnsignedVarint(None),
        );
        let mut service2 = manager.register_protocol(
            ProtocolName::from("/notif/2"),
            Vec::new(),
            ProtocolCodec::UnsignedVarint(None),
        );
        let transport_handle = manager.register_transport(SupportedTransport::Quic);
        let mut protocol_set = transport_handle.protocol_set();
        protocol_set
            .report_connection_established(ConnectionId::from(0usize), peer, Multiaddr::empty())
            .await
            .unwrap();

        // ignore connection established events
        let _ = service1.next_event().await.unwrap();
        let _ = service2.next_event().await.unwrap();
        let _ = manager.next().await.unwrap();

        tokio::spawn(async move {
            let _ =
                QuicConnection::new(peer, protocol_set, connection1, ConnectionId::from(0usize))
                    .start()
                    .await;
        });

        let _ = service1.open_substream(peer).await.unwrap();

        let Some(TransportEvent::SubstreamOpenFailure { .. }) = service1.next_event().await else {
            panic!("invalid event received");
        };
    }

    #[tokio::test]
    async fn outbound_substream_protocol_not_supported() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let QuicContext {
            mut manager,
            mut server,
            peer,
            client,
            connect,
            rx: _rx,
        } = prepare_quic_context();

        let res = tokio::join!(server.accept(), client.connect(connect));
        let (Some(connection1), Ok(mut connection2)) = res else {
            panic!("failed to establish connection");
        };

        let mut service1 = manager.register_protocol(
            ProtocolName::from("/notif/1"),
            Vec::new(),
            ProtocolCodec::UnsignedVarint(None),
        );
        let mut service2 = manager.register_protocol(
            ProtocolName::from("/notif/2"),
            Vec::new(),
            ProtocolCodec::UnsignedVarint(None),
        );
        let transport_handle = manager.register_transport(SupportedTransport::Quic);
        let mut protocol_set = transport_handle.protocol_set();
        protocol_set
            .report_connection_established(ConnectionId::from(0usize), peer, Multiaddr::empty())
            .await
            .unwrap();

        // ignore connection established events
        let _ = service1.next_event().await.unwrap();
        let _ = service2.next_event().await.unwrap();
        let _ = manager.next().await.unwrap();

        tokio::spawn(async move {
            let _ =
                QuicConnection::new(peer, protocol_set, connection1, ConnectionId::from(0usize))
                    .start()
                    .await;
        });

        let _ = service1.open_substream(peer).await.unwrap();

        let stream = connection2.accept_bidirectional_stream().await.unwrap().unwrap();

        assert!(
            listener_select_proto(stream, vec!["/unsupported/1", "/unsupported/2"])
                .await
                .is_err()
        );

        let Some(TransportEvent::SubstreamOpenFailure { .. }) = service1.next_event().await else {
            panic!("invalid event received");
        };
    }

    #[tokio::test]
    async fn connection_closed_while_negotiating_protocol() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let QuicContext {
            mut manager,
            mut server,
            peer,
            client,
            connect,
            rx: _rx,
        } = prepare_quic_context();

        let res = tokio::join!(server.accept(), client.connect(connect));
        let (Some(connection1), Ok(mut connection2)) = res else {
            panic!("failed to establish connection");
        };

        let mut service1 = manager.register_protocol(
            ProtocolName::from("/notif/1"),
            Vec::new(),
            ProtocolCodec::UnsignedVarint(None),
        );
        let mut service2 = manager.register_protocol(
            ProtocolName::from("/notif/2"),
            Vec::new(),
            ProtocolCodec::UnsignedVarint(None),
        );
        let transport_handle = manager.register_transport(SupportedTransport::Quic);
        let mut protocol_set = transport_handle.protocol_set();
        protocol_set
            .report_connection_established(ConnectionId::from(0usize), peer, Multiaddr::empty())
            .await
            .unwrap();

        // ignore connection established events
        let _ = service1.next_event().await.unwrap();
        let _ = service2.next_event().await.unwrap();
        let _ = manager.next().await.unwrap();

        tokio::spawn(async move {
            let _ =
                QuicConnection::new(peer, protocol_set, connection1, ConnectionId::from(0usize))
                    .start()
                    .await;
        });

        let _ = service1.open_substream(peer).await.unwrap();
        let stream = connection2.accept_bidirectional_stream().await.unwrap().unwrap();

        drop(stream);
        drop(connection2);

        let Some(TransportEvent::SubstreamOpenFailure { .. }) = service1.next_event().await else {
            panic!("invalid event received");
        };
    }

    #[tokio::test]
    async fn outbound_substream_opened_and_negotiated() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let QuicContext {
            mut manager,
            mut server,
            peer,
            client,
            connect,
            rx: _rx,
        } = prepare_quic_context();

        let res = tokio::join!(server.accept(), client.connect(connect));
        let (Some(connection1), Ok(mut connection2)) = res else {
            panic!("failed to establish connection");
        };

        let mut service1 = manager.register_protocol(
            ProtocolName::from("/notif/1"),
            Vec::new(),
            ProtocolCodec::UnsignedVarint(None),
        );
        let mut service2 = manager.register_protocol(
            ProtocolName::from("/notif/2"),
            Vec::new(),
            ProtocolCodec::UnsignedVarint(None),
        );
        let transport_handle = manager.register_transport(SupportedTransport::Quic);
        let mut protocol_set = transport_handle.protocol_set();
        protocol_set
            .report_connection_established(ConnectionId::from(0usize), peer, Multiaddr::empty())
            .await
            .unwrap();

        // ignore connection established events
        let _ = service1.next_event().await.unwrap();
        let _ = service2.next_event().await.unwrap();
        let _ = manager.next().await.unwrap();

        tokio::spawn(async move {
            let _ =
                QuicConnection::new(peer, protocol_set, connection1, ConnectionId::from(0usize))
                    .start()
                    .await;
        });

        let _ = service1.open_substream(peer).await.unwrap();

        let stream = connection2.accept_bidirectional_stream().await.unwrap().unwrap();

        let (_io, _proto) =
            listener_select_proto(stream, vec!["/notif/1", "/notif/2"]).await.unwrap();

        let Some(TransportEvent::SubstreamOpened { .. }) = service1.next_event().await else {
            panic!("invalid event received");
        };
    }
}
