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
    crypto::{ed25519::Keypair, tls::make_server_config},
    error::AddressError,
    PeerId,
};

use futures::{future::BoxFuture, stream::FuturesUnordered, FutureExt, Stream, StreamExt};
use multiaddr::{Multiaddr, Protocol};
use quinn::{Connecting, Endpoint, ServerConfig};

use std::{
    net::{IpAddr, SocketAddr},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::quic::listener";

/// QUIC listener.
pub struct QuicListener {
    /// Listen addresses.
    _listen_addresses: Vec<SocketAddr>,

    /// Listeners.
    listeners: Vec<Endpoint>,

    /// Incoming connections.
    incoming: FuturesUnordered<BoxFuture<'static, Option<(usize, Connecting)>>>,
}

impl QuicListener {
    /// Create new [`QuicListener`].
    pub fn new(
        keypair: &Keypair,
        addresses: Vec<Multiaddr>,
    ) -> crate::Result<(Self, Vec<Multiaddr>)> {
        let mut listeners: Vec<Endpoint> = Vec::new();
        let mut listen_addresses = Vec::new();

        for address in addresses.into_iter() {
            let (listen_address, _) = Self::get_socket_address(&address)?;
            let crypto_config = Arc::new(make_server_config(keypair).expect("to succeed"));
            let server_config = ServerConfig::with_crypto(crypto_config);
            let listener = Endpoint::server(server_config, listen_address).unwrap();

            let listen_address = listener.local_addr()?;
            listen_addresses.push(listen_address);
            listeners.push(listener);
            // );
        }

        let listen_multi_addresses = listen_addresses
            .iter()
            .cloned()
            .map(|address| {
                Multiaddr::empty()
                    .with(Protocol::from(address.ip()))
                    .with(Protocol::Udp(address.port()))
                    .with(Protocol::QuicV1)
            })
            .collect();

        Ok((
            Self {
                incoming: listeners
                    .iter_mut()
                    .enumerate()
                    .map(|(i, listener)| {
                        let inner = listener.clone();
                        async move { inner.accept().await.map(|connecting| (i, connecting)) }
                            .boxed()
                    })
                    .collect(),
                listeners,
                _listen_addresses: listen_addresses,
            },
            listen_multi_addresses,
        ))
    }

    /// Extract socket address and `PeerId`, if found, from `address`.
    pub fn get_socket_address(
        address: &Multiaddr,
    ) -> Result<(SocketAddr, Option<PeerId>), AddressError> {
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
                    return Err(AddressError::InvalidProtocol);
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
                    return Err(AddressError::InvalidProtocol);
                }
            },
            protocol => {
                tracing::error!(target: LOG_TARGET, ?protocol, "invalid transport protocol");
                return Err(AddressError::InvalidProtocol);
            }
        };

        // verify that quic exists
        match iter.next() {
            Some(Protocol::QuicV1) => {}
            _ => return Err(AddressError::InvalidProtocol),
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
                return Err(AddressError::PeerIdMissing);
            }
        };

        Ok((socket_address, maybe_peer))
    }
}

impl Stream for QuicListener {
    type Item = Connecting;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.incoming.is_empty() {
            return Poll::Pending;
        }

        match futures::ready!(self.incoming.poll_next_unpin(cx)) {
            None => Poll::Ready(None),
            Some(None) => Poll::Ready(None),
            Some(Some((listener, future))) => {
                let inner = self.listeners[listener].clone();
                self.incoming.push(
                    async move { inner.accept().await.map(|connecting| (listener, connecting)) }
                        .boxed(),
                );

                Poll::Ready(Some(future))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::crypto::tls::make_client_config;

    use super::*;
    use quinn::ClientConfig;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

    #[test]
    fn parse_multiaddresses() {
        assert!(QuicListener::get_socket_address(
            &"/ip6/::1/udp/8888/quic-v1".parse().expect("valid multiaddress")
        )
        .is_ok());
        assert!(QuicListener::get_socket_address(
            &"/ip4/127.0.0.1/udp/8888/quic-v1".parse().expect("valid multiaddress")
        )
        .is_ok());
        assert!(QuicListener::get_socket_address(
            &"/ip6/::1/udp/8888/quic-v1/p2p/12D3KooWT2ouvz5uMmCvHJGzAGRHiqDts5hzXR7NdoQ27pGdzp9Q"
                .parse()
                .expect("valid multiaddress")
        )
        .is_ok());
        assert!(QuicListener::get_socket_address(
            &"/ip4/127.0.0.1/udp/8888/quic-v1/p2p/12D3KooWT2ouvz5uMmCvHJGzAGRHiqDts5hzXR7NdoQ27pGdzp9Q"
                .parse()
                .expect("valid multiaddress")
        )
        .is_ok());
        assert!(QuicListener::get_socket_address(
            &"/ip6/::1/tcp/8888/quic-v1/p2p/12D3KooWT2ouvz5uMmCvHJGzAGRHiqDts5hzXR7NdoQ27pGdzp9Q"
                .parse()
                .expect("valid multiaddress")
        )
        .is_err());
        assert!(QuicListener::get_socket_address(
            &"/ip4/127.0.0.1/udp/8888/p2p/12D3KooWT2ouvz5uMmCvHJGzAGRHiqDts5hzXR7NdoQ27pGdzp9Q"
                .parse()
                .expect("valid multiaddress")
        )
        .is_err());
        assert!(QuicListener::get_socket_address(
            &"/ip4/127.0.0.1/tcp/8888/p2p/12D3KooWT2ouvz5uMmCvHJGzAGRHiqDts5hzXR7NdoQ27pGdzp9Q"
                .parse()
                .expect("valid multiaddress")
        )
        .is_err());
        assert!(QuicListener::get_socket_address(
            &"/dns/google.com/tcp/8888/p2p/12D3KooWT2ouvz5uMmCvHJGzAGRHiqDts5hzXR7NdoQ27pGdzp9Q"
                .parse()
                .expect("valid multiaddress")
        )
        .is_err());
        assert!(QuicListener::get_socket_address(
            &"/ip6/::1/udp/8888/quic-v1/utp".parse().expect("valid multiaddress")
        )
        .is_err());
    }

    #[tokio::test]
    async fn no_listeners() {
        let (mut listener, _) = QuicListener::new(&Keypair::generate(), Vec::new()).unwrap();

        futures::future::poll_fn(|cx| match listener.poll_next_unpin(cx) {
            Poll::Pending => Poll::Ready(()),
            event => panic!("unexpected event: {event:?}"),
        })
        .await;
    }

    #[tokio::test]
    async fn one_listener() {
        let address: Multiaddr = "/ip6/::1/udp/0/quic-v1".parse().unwrap();
        let keypair = Keypair::generate();
        let peer = PeerId::from_public_key(&keypair.public().into());
        let (mut listener, listen_addresses) =
            QuicListener::new(&keypair, vec![address.clone()]).unwrap();
        let Some(Protocol::Udp(port)) = listen_addresses.first().unwrap().clone().iter().nth(1)
        else {
            panic!("invalid address");
        };

        let crypto_config =
            Arc::new(make_client_config(&Keypair::generate(), Some(peer)).expect("to succeed"));
        let client_config = ClientConfig::new(crypto_config);
        let client =
            Endpoint::client(SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0)).unwrap();
        let connection = client
            .connect_with(client_config, format!("[::1]:{port}").parse().unwrap(), "l")
            .unwrap();

        let (res1, res2) = tokio::join!(
            listener.next(),
            Box::pin(async move {
                match connection.await {
                    Ok(connection) => Ok(connection),
                    Err(error) => Err(error),
                }
            })
        );

        assert!(res1.is_some() && res2.is_ok());
    }

    #[tokio::test]
    async fn two_listeners() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let address1: Multiaddr = "/ip6/::1/udp/0/quic-v1".parse().unwrap();
        let address2: Multiaddr = "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap();
        let keypair = Keypair::generate();
        let peer = PeerId::from_public_key(&keypair.public().into());

        let (mut listener, listen_addresses) =
            QuicListener::new(&keypair, vec![address1, address2]).unwrap();

        let Some(Protocol::Udp(port1)) = listen_addresses.first().unwrap().clone().iter().nth(1)
        else {
            panic!("invalid address");
        };

        let Some(Protocol::Udp(port2)) =
            listen_addresses.iter().nth(1).unwrap().clone().iter().nth(1)
        else {
            panic!("invalid address");
        };

        let crypto_config1 =
            Arc::new(make_client_config(&Keypair::generate(), Some(peer)).expect("to succeed"));
        let client_config1 = ClientConfig::new(crypto_config1);
        let client1 =
            Endpoint::client(SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0)).unwrap();
        let connection1 = client1
            .connect_with(
                client_config1,
                format!("[::1]:{port1}").parse().unwrap(),
                "l",
            )
            .unwrap();

        let crypto_config2 =
            Arc::new(make_client_config(&Keypair::generate(), Some(peer)).expect("to succeed"));
        let client_config2 = ClientConfig::new(crypto_config2);
        let client2 =
            Endpoint::client(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)).unwrap();
        let connection2 = client2
            .connect_with(
                client_config2,
                format!("127.0.0.1:{port2}").parse().unwrap(),
                "l",
            )
            .unwrap();

        tokio::spawn(async move {
            match connection1.await {
                Ok(connection) => Ok(connection),
                Err(error) => Err(error),
            }
        });

        tokio::spawn(async move {
            match connection2.await {
                Ok(connection) => Ok(connection),
                Err(error) => Err(error),
            }
        });

        for _ in 0..2 {
            let _ = listener.next().await;
        }
    }

    #[tokio::test]
    async fn two_clients_dialing_same_address() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let keypair = Keypair::generate();
        let peer = PeerId::from_public_key(&keypair.public().into());

        let (mut listener, listen_addresses) = QuicListener::new(
            &keypair,
            vec![
                "/ip6/::1/udp/0/quic-v1".parse().unwrap(),
                "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap(),
            ],
        )
        .unwrap();

        let Some(Protocol::Udp(port)) = listen_addresses.first().unwrap().clone().iter().nth(1)
        else {
            panic!("invalid address");
        };

        let crypto_config1 =
            Arc::new(make_client_config(&Keypair::generate(), Some(peer)).expect("to succeed"));
        let client_config1 = ClientConfig::new(crypto_config1);
        let client1 =
            Endpoint::client(SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0)).unwrap();
        let connection1 = client1
            .connect_with(
                client_config1,
                format!("[::1]:{port}").parse().unwrap(),
                "l",
            )
            .unwrap();

        let crypto_config2 =
            Arc::new(make_client_config(&Keypair::generate(), Some(peer)).expect("to succeed"));
        let client_config2 = ClientConfig::new(crypto_config2);
        let client2 =
            Endpoint::client(SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0)).unwrap();
        let connection2 = client2
            .connect_with(
                client_config2,
                format!("[::1]:{port}").parse().unwrap(),
                "l",
            )
            .unwrap();

        tokio::spawn(async move {
            match connection1.await {
                Ok(connection) => Ok(connection),
                Err(error) => Err(error),
            }
        });

        tokio::spawn(async move {
            match connection2.await {
                Ok(connection) => Ok(connection),
                Err(error) => Err(error),
            }
        });

        for _ in 0..2 {
            let _ = listener.next().await;
        }
    }
}
