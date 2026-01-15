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
    crypto::tls::{certificate::generate, TlsProvider},
    error::{AddressError, Error},
    transport::{
        manager::{TransportHandle, TransportManagerCommand},
        quic::{config::Config, connection::QuicConnection},
        Transport,
    },
    types::ConnectionId,
    PeerId,
};

use futures::{future::BoxFuture, stream::FuturesUnordered, StreamExt};
use multiaddr::{Multiaddr, Protocol};
use multihash::Multihash;
use s2n_quic::{
    client::Connect,
    connection::{Connection, Error as ConnectionError},
    Client, Server,
};
use tokio::sync::mpsc::{channel, Receiver, Sender};

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
};

mod connection;

pub mod config;

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::quic";

/// Convert `SocketAddr` to `Multiaddr`
fn socket_addr_to_multi_addr(address: &SocketAddr) -> Multiaddr {
    let mut multiaddr = Multiaddr::from(address.ip());
    multiaddr.push(Protocol::Udp(address.port()));
    multiaddr.push(Protocol::QuicV1);

    multiaddr
}

/// QUIC transport object.
#[derive(Debug)]
pub(crate) struct QuicTransport {
    /// QUIC server.
    server: Server,

    /// Transport context.
    context: TransportHandle,

    /// Assigned listen address.
    listen_address: SocketAddr,

    /// Listen address assigned for clients.
    client_listen_address: SocketAddr,

    /// Pending dials.
    pending_dials: HashMap<ConnectionId, Multiaddr>,

    /// Pending connections.
    pending_connections: FuturesUnordered<
        BoxFuture<'static, (ConnectionId, PeerId, Result<Connection, ConnectionError>)>,
    >,

    /// RX channel for receiving the client `PeerId`.
    rx: Receiver<PeerId>,

    /// TX channel for send the client `PeerId` to server.
    _tx: Sender<PeerId>,
}

impl QuicTransport {
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
                        "invalid transport protocol, expected `QuicV1`",
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
                        "invalid transport protocol, expected `QuicV1`",
                    );
                    return Err(Error::AddressError(AddressError::InvalidProtocol));
                }
            },
            protocol => {
                tracing::error!(target: LOG_TARGET, ?protocol, "invalid transport protocol");
                return Err(Error::AddressError(AddressError::InvalidProtocol));
            }
        };

        // verify that quic exists
        match iter.next() {
            Some(Protocol::QuicV1) => {}
            _ => return Err(Error::AddressError(AddressError::InvalidProtocol)),
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

    /// Accept QUIC conenction.
    async fn accept_connection(&mut self, connection: Connection) -> crate::Result<()> {
        let connection_id = self.context.next_connection_id();
        let address = socket_addr_to_multi_addr(
            &connection.remote_addr().expect("remote address to be known"),
        );

        let Ok(peer) = self.rx.try_recv() else {
            tracing::error!(target: LOG_TARGET, "failed to receive client `PeerId` from tls verifier");
            return Ok(());
        };

        tracing::info!(target: LOG_TARGET, ?address, ?peer, "accepted connection from remote peer");

        // TODO: https://github.com/paritytech/litep2p/issues/349 verify that the peer can actually be accepted
        let mut protocol_set = self.context.protocol_set();
        protocol_set.report_connection_established(connection_id, peer, address).await?;

        tokio::spawn(async move {
            let quic_connection =
                QuicConnection::new(peer, protocol_set, connection, connection_id);

            if let Err(error) = quic_connection.start().await {
                tracing::debug!(target: LOG_TARGET, ?error, "quic connection exited with an error");
            }
        });

        Ok(())
    }

    /// Handle established connection.
    async fn on_connection_established(
        &mut self,
        peer: PeerId,
        connection_id: ConnectionId,
        result: Result<Connection, ConnectionError>,
    ) -> crate::Result<()> {
        match result {
            Ok(connection) => {
                let address = match self.pending_dials.remove(&connection_id) {
                    Some(address) => address,
                    None => {
                        let address = connection
                            .remote_addr()
                            .map_err(|_| Error::AddressError(AddressError::AddressNotAvailable))?;

                        Multiaddr::empty()
                            .with(Protocol::from(address.ip()))
                            .with(Protocol::Udp(address.port()))
                            .with(Protocol::QuicV1)
                            .with(Protocol::P2p(
                                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
                            ))
                    }
                };

                let mut protocol_set = self.context.protocol_set();
                protocol_set.report_connection_established(connection_id, peer, address).await?;

                tokio::spawn(async move {
                    let quic_connection =
                        QuicConnection::new(peer, protocol_set, connection, connection_id);
                    if let Err(error) = quic_connection.start().await {
                        tracing::debug!(target: LOG_TARGET, ?error, "quic connection exited with an error");
                    }
                });

                Ok(())
            }
            Err(error) => match self.pending_dials.remove(&connection_id) {
                Some(address) => {
                    let error = if std::matches!(
                        error,
                        ConnectionError::MaxHandshakeDurationExceeded { .. }
                    ) {
                        Error::Timeout
                    } else {
                        Error::TransportError(error.to_string())
                    };

                    self.context.report_dial_failure(connection_id, address, error).await;
                    Ok(())
                }
                None => {
                    tracing::debug!(
                        target: LOG_TARGET,
                        ?error,
                        "failed to establish connection"
                    );
                    Ok(())
                }
            },
        }
    }

    /// Dial remote peer.
    async fn on_dial_peer(
        &mut self,
        address: Multiaddr,
        connection: ConnectionId,
    ) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, ?address, "open connection");

        let Ok((socket_address, Some(peer))) = Self::get_socket_address(&address) else {
            return Err(Error::AddressError(AddressError::PeerIdMissing));
        };

        let (certificate, key) = generate(&self.context.keypair).unwrap();
        let provider = TlsProvider::new(key, certificate, Some(peer), None);

        let client = Client::builder()
            .with_tls(provider)
            .expect("TLS provider to be enabled successfully")
            .with_io(self.client_listen_address)?
            .start()?;

        let connect = Connect::new(socket_address).with_server_name("localhost");

        self.pending_dials.insert(connection, address);
        self.pending_connections.push(Box::pin(async move {
            (connection, peer, client.connect(connect).await)
        }));

        Ok(())
    }
}

#[async_trait::async_trait]
impl Transport for QuicTransport {
    type Config = Config;

    /// Create new [`QuicTransport`] object.
    async fn new(context: TransportHandle, config: Self::Config) -> crate::Result<Self>
    where
        Self: Sized,
    {
        tracing::info!(
            target: LOG_TARGET,
            listen_address = ?config.listen_address,
            "start quic transport",
        );

        let (listen_address, _) = Self::get_socket_address(&config.listen_address)?;
        let (certificate, key) = generate(&context.keypair)?;
        let (_tx, rx) = channel(1);

        let provider = TlsProvider::new(key, certificate, None, Some(_tx.clone()));
        let server = Server::builder()
            .with_tls(provider)
            .expect("TLS provider to be enabled successfully")
            .with_io(listen_address)?
            .start()?;

        let listen_address = server.local_addr()?;
        let client_listen_address = match listen_address.ip() {
            std::net::IpAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
            std::net::IpAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
        };

        Ok(Self {
            rx,
            _tx,
            server,
            context,
            listen_address,
            client_listen_address,
            pending_dials: HashMap::new(),
            pending_connections: FuturesUnordered::new(),
        })
    }

    /// Get assigned listen address.
    fn listen_address(&self) -> Multiaddr {
        socket_addr_to_multi_addr(&self.listen_address)
    }

    /// Start [`QuicTransport`] event loop.
    async fn start(mut self) -> crate::Result<()> {
        loop {
            tokio::select! {
                connection = self.server.accept() => match connection {
                    Some(connection) => if let Err(error) = self.accept_connection(connection).await {
                        tracing::error!(target: LOG_TARGET, ?error, "failed to accept quic connection");
                        return Err(error);
                    },
                    None => {
                        tracing::error!(target: LOG_TARGET, "failed to accept connection, closing quic transport");
                        return Ok(())
                    }
                },
                connection = self.pending_connections.select_next_some(), if !self.pending_connections.is_empty() => {
                    let (connection_id, peer, result) = connection;

                    if let Err(error) = self.on_connection_established(peer, connection_id, result).await {
                        tracing::debug!(target: LOG_TARGET, ?peer, ?error, "failed to handle established connection");
                    }
                }
                command = self.context.next() => match command.ok_or(Error::EssentialTaskClosed)? {
                    TransportManagerCommand::Dial { address, connection } => {
                        if let Err(error) = self.on_dial_peer(address.clone(), connection).await {
                            tracing::debug!(target: LOG_TARGET, ?address, ?connection, "failed to dial peer");
                            let _ = self.context.report_dial_failure(connection, address, error).await;
                        }
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
        codec::ProtocolCodec,
        crypto::{ed25519::Keypair, PublicKey},
        transport::manager::{
            ProtocolContext, SupportedTransport, TransportHandle, TransportManager,
            TransportManagerCommand, TransportManagerEvent,
        },
        types::protocol::ProtocolName,
    };
    use tokio::sync::mpsc::channel;

    #[tokio::test]
    async fn connect_and_accept_works() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let keypair1 = Keypair::generate();
        let (tx1, _rx1) = channel(64);
        let (event_tx1, mut event_rx1) = channel(64);
        let (_command_tx1, command_rx1) = channel(64);

        let handle1 = TransportHandle {
            protocol_names: Vec::new(),
            next_substream_id: Default::default(),
            next_connection_id: Default::default(),
            tx: event_tx1,
            rx: command_rx1,
            keypair: keypair1.clone(),
            protocols: HashMap::from_iter([(
                ProtocolName::from("/notif/1"),
                ProtocolContext {
                    tx: tx1,
                    codec: ProtocolCodec::Identity(32),
                    fallback_names: Vec::new(),
                },
            )]),
        };
        let transport_config1 = config::Config {
            listen_address: "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap(),
        };

        let transport1 = QuicTransport::new(handle1, transport_config1).await.unwrap();

        let _peer1: PeerId = PeerId::from_public_key(&PublicKey::Ed25519(keypair1.public()));
        let listen_address = Transport::listen_address(&transport1).to_string();
        let listen_address: Multiaddr =
            format!("{}/p2p/{}", listen_address, _peer1.to_string()).parse().unwrap();
        tokio::spawn(transport1.start());

        let keypair2 = Keypair::generate();
        let (tx2, _rx2) = channel(64);
        let (event_tx2, mut event_rx2) = channel(64);
        let (command_tx2, command_rx2) = channel(64);

        let handle2 = TransportHandle {
            protocol_names: Vec::new(),
            next_substream_id: Default::default(),
            next_connection_id: Default::default(),
            tx: event_tx2,
            rx: command_rx2,
            keypair: keypair2.clone(),
            protocols: HashMap::from_iter([(
                ProtocolName::from("/notif/1"),
                ProtocolContext {
                    tx: tx2,
                    codec: ProtocolCodec::Identity(32),
                    fallback_names: Vec::new(),
                },
            )]),
        };
        let transport_config2 = config::Config {
            listen_address: "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap(),
        };

        let transport2 = QuicTransport::new(handle2, transport_config2).await.unwrap();
        tokio::spawn(transport2.start());

        command_tx2
            .send(TransportManagerCommand::Dial {
                address: listen_address,
                connection: ConnectionId::new(),
            })
            .await
            .unwrap();

        let (res1, res2) = tokio::join!(event_rx1.recv(), event_rx2.recv());

        assert!(std::matches!(
            res1,
            Some(TransportManagerEvent::ConnectionEstablished { .. })
        ));
        assert!(std::matches!(
            res2,
            Some(TransportManagerEvent::ConnectionEstablished { .. })
        ));
    }

    #[tokio::test]
    async fn dial_peer_id_missing() {
        let (mut manager, _handle) = TransportManager::new(Keypair::generate());
        let handle = manager.register_transport(SupportedTransport::Quic);
        let mut transport = QuicTransport::new(
            handle,
            Config {
                listen_address: "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap(),
            },
        )
        .await
        .unwrap();

        let address = Multiaddr::empty()
            .with(Protocol::Ip4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Udp(8888));

        match transport.on_dial_peer(address, ConnectionId::from(0usize)).await {
            Err(Error::AddressError(AddressError::PeerIdMissing)) => {}
            _ => panic!("invalid result for `on_dial_peer()`"),
        }
    }

    #[tokio::test]
    async fn dial_failure() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let (mut manager, _handle) = TransportManager::new(Keypair::generate());
        let handle = manager.register_transport(SupportedTransport::Quic);
        let mut transport = QuicTransport::new(
            handle,
            Config {
                listen_address: "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap(),
            },
        )
        .await
        .unwrap();

        let peer = PeerId::random();
        let address = Multiaddr::empty()
            .with(Protocol::from(std::net::Ipv4Addr::new(255, 254, 253, 252)))
            .with(Protocol::Udp(8888))
            .with(Protocol::QuicV1)
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));
        manager.dial_address(address.clone()).await.unwrap();

        assert!(transport.pending_dials.is_empty());

        match transport.on_dial_peer(address, ConnectionId::from(0usize)).await {
            Ok(()) => {}
            _ => panic!("invalid result for `on_dial_peer()`"),
        }

        assert!(!transport.pending_dials.is_empty());

        tokio::spawn(transport.start());

        std::matches!(
            manager.next().await,
            Some(TransportManagerEvent::DialFailure { .. })
        );
    }

    #[tokio::test]
    async fn pending_dial_is_cleaned() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let keypair = Keypair::generate();
        let (mut manager, _handle) = TransportManager::new(keypair.clone());
        let handle = manager.register_transport(SupportedTransport::Quic);
        let mut transport = QuicTransport::new(
            handle,
            Config {
                listen_address: "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap(),
            },
        )
        .await
        .unwrap();

        let peer = PeerId::random();
        let address = Multiaddr::empty()
            .with(Protocol::from(std::net::Ipv4Addr::new(255, 254, 253, 252)))
            .with(Protocol::Udp(8888))
            .with(Protocol::QuicV1)
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));

        assert!(transport.pending_dials.is_empty());

        match transport.on_dial_peer(address.clone(), ConnectionId::from(0usize)).await {
            Ok(()) => {}
            _ => panic!("invalid result for `on_dial_peer()`"),
        }

        assert!(!transport.pending_dials.is_empty());

        let Ok((socket_address, Some(peer))) = QuicTransport::get_socket_address(&address) else {
            panic!("invalid address");
        };

        let (certificate, key) = generate(&keypair).unwrap();
        let provider = TlsProvider::new(key, certificate, Some(peer), None);

        let client = Client::builder()
            .with_tls(provider)
            .expect("TLS provider to be enabled successfully")
            .with_io("0.0.0.0:0")
            .unwrap()
            .start()
            .unwrap();
        let connect = Connect::new(socket_address).with_server_name("localhost");

        let _ = transport
            .on_connection_established(
                peer,
                ConnectionId::from(0usize),
                client.connect(connect).await,
            )
            .await;

        assert!(transport.pending_dials.is_empty());
    }
}
