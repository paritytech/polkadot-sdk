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
    addresses::PublicAddresses,
    crypto::ed25519::Keypair,
    error::ImmediateDialError,
    executor::Executor,
    protocol::ProtocolSet,
    transport::manager::{
        address::AddressRecord,
        peer_state::StateDialResult,
        types::{PeerContext, SupportedTransport},
        ProtocolContext, TransportManagerEvent, LOG_TARGET,
    },
    types::{protocol::ProtocolName, ConnectionId},
    BandwidthSink, PeerId,
};

use multiaddr::{Multiaddr, Protocol};
use parking_lot::RwLock;
use tokio::sync::mpsc::{error::TrySendError, Sender};

use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

/// Inner commands sent from [`TransportManagerHandle`] to
/// [`crate::transport::manager::TransportManager`].
pub enum InnerTransportManagerCommand {
    /// Dial peer.
    DialPeer {
        /// Remote peer ID.
        peer: PeerId,
    },

    /// Dial address.
    DialAddress {
        /// Remote address.
        address: Multiaddr,
    },

    UnregisterProtocol {
        /// Protocol name.
        protocol: ProtocolName,
    },
}

/// Handle for communicating with [`crate::transport::manager::TransportManager`].
#[derive(Debug, Clone)]
pub struct TransportManagerHandle {
    /// Local peer ID.
    local_peer_id: PeerId,

    /// Peers.
    peers: Arc<RwLock<HashMap<PeerId, PeerContext>>>,

    /// TX channel for sending commands to [`crate::transport::manager::TransportManager`].
    cmd_tx: Sender<InnerTransportManagerCommand>,

    /// Supported transports.
    supported_transport: HashSet<SupportedTransport>,

    /// Local listen addresess.
    listen_addresses: Arc<RwLock<HashSet<Multiaddr>>>,

    /// Public addresses.
    public_addresses: PublicAddresses,
}

impl TransportManagerHandle {
    /// Create new [`TransportManagerHandle`].
    pub fn new(
        local_peer_id: PeerId,
        peers: Arc<RwLock<HashMap<PeerId, PeerContext>>>,
        cmd_tx: Sender<InnerTransportManagerCommand>,
        supported_transport: HashSet<SupportedTransport>,
        listen_addresses: Arc<RwLock<HashSet<Multiaddr>>>,
        public_addresses: PublicAddresses,
    ) -> Self {
        Self {
            peers,
            cmd_tx,
            local_peer_id,
            supported_transport,
            listen_addresses,
            public_addresses,
        }
    }

    /// Register new transport to [`TransportManagerHandle`].
    pub(crate) fn register_transport(&mut self, transport: SupportedTransport) {
        self.supported_transport.insert(transport);
    }

    /// Get the list of public addresses of the node.
    pub(crate) fn public_addresses(&self) -> PublicAddresses {
        self.public_addresses.clone()
    }

    /// Get the list of listen addresses of the node.
    pub(crate) fn listen_addresses(&self) -> HashSet<Multiaddr> {
        self.listen_addresses.read().clone()
    }

    /// Check if `address` is supported by one of the enabled transports.
    pub fn supported_transport(&self, address: &Multiaddr) -> bool {
        let mut iter = address.iter();

        match iter.next() {
            Some(Protocol::Ip4(address)) =>
                if address.is_unspecified() {
                    return false;
                },
            Some(Protocol::Ip6(address)) =>
                if address.is_unspecified() {
                    return false;
                },
            Some(Protocol::Dns(_)) | Some(Protocol::Dns4(_)) | Some(Protocol::Dns6(_)) => {}
            _ => return false,
        }

        match iter.next() {
            None => false,
            Some(Protocol::Tcp(_)) => match (iter.next(), iter.next(), iter.next()) {
                (Some(Protocol::P2p(_)), None, None) =>
                    self.supported_transport.contains(&SupportedTransport::Tcp),
                #[cfg(feature = "websocket")]
                (Some(Protocol::Ws(_)), Some(Protocol::P2p(_)), None) =>
                    self.supported_transport.contains(&SupportedTransport::WebSocket),
                #[cfg(feature = "websocket")]
                (Some(Protocol::Wss(_)), Some(Protocol::P2p(_)), None) =>
                    self.supported_transport.contains(&SupportedTransport::WebSocket),
                _ => false,
            },
            #[cfg(feature = "quic")]
            Some(Protocol::Udp(_)) => match (iter.next(), iter.next(), iter.next()) {
                (Some(Protocol::QuicV1), Some(Protocol::P2p(_)), None) =>
                    self.supported_transport.contains(&SupportedTransport::Quic),
                _ => false,
            },
            _ => false,
        }
    }

    /// Helper to extract IP and Port from a Multiaddr
    fn extract_ip_port(addr: &Multiaddr) -> Option<(IpAddr, u16)> {
        let mut iter = addr.iter();
        let ip = match iter.next() {
            Some(Protocol::Ip4(i)) => IpAddr::V4(i),
            Some(Protocol::Ip6(i)) => IpAddr::V6(i),
            _ => return None,
        };

        let port = match iter.next() {
            Some(Protocol::Tcp(p)) | Some(Protocol::Udp(p)) => p,
            _ => return None,
        };

        Some((ip, port))
    }

    /// Check if the address is a local listen address and if so, discard it.
    fn is_local_address(&self, address: &Multiaddr) -> bool {
        // Strip the peer ID if present.
        let address: Multiaddr = address
            .iter()
            .take_while(|protocol| !std::matches!(protocol, Protocol::P2p(_)))
            .collect();

        // Check for the exact match.
        let listen_addresses = self.listen_addresses.read();
        if listen_addresses.contains(&address) {
            return true;
        }

        let Some((ip, port)) = Self::extract_ip_port(&address) else {
            return false;
        };

        for listen_address in listen_addresses.iter() {
            let Some((listen_ip, listen_port)) = Self::extract_ip_port(listen_address) else {
                continue;
            };

            if port == listen_port {
                // Exact IP match.
                if listen_ip == ip {
                    return true;
                }

                // Check if the listener is binding to any (0.0.0.0) interface
                // and the incoming is a loopback address.
                if listen_ip.is_unspecified() && ip.is_loopback() {
                    return true;
                }

                // Check for ipv4/ipv6 loopback equivalence.
                if listen_ip.is_loopback() && ip.is_loopback() {
                    return true;
                }
            }
        }

        false
    }

    /// Add one or more known addresses for peer.
    ///
    /// If peer doesn't exist, it will be added to known peers.
    ///
    /// Returns the number of added addresses after non-supported transports were filtered out.
    pub fn add_known_address(
        &mut self,
        peer: &PeerId,
        addresses: impl Iterator<Item = Multiaddr>,
    ) -> usize {
        let mut peer_addresses = HashSet::new();

        for address in addresses {
            // There is not supported transport configured that can dial this address.
            if !self.supported_transport(&address) {
                continue;
            }
            if self.is_local_address(&address) {
                continue;
            }

            // Check the peer ID if present.
            if let Some(Protocol::P2p(multihash)) = address.iter().last() {
                // This can correspond to the provided peerID or to a different one.
                if multihash != *peer.as_ref() {
                    tracing::debug!(
                        target: LOG_TARGET,
                        ?peer,
                        ?address,
                        "Refusing to add known address that corresponds to a different peer ID",
                    );

                    continue;
                }

                peer_addresses.insert(address);
            } else {
                // Add the provided peer ID to the address.
                let address = address.with(Protocol::P2p(multihash::Multihash::from(*peer)));
                peer_addresses.insert(address);
            }
        }

        let num_added = peer_addresses.len();

        tracing::trace!(
            target: LOG_TARGET,
            ?peer,
            ?peer_addresses,
            "add known addresses",
        );

        let mut peers = self.peers.write();
        let entry = peers.entry(*peer).or_default();

        // All addresses should be valid at this point, since the peer ID was either added or
        // double checked.
        entry
            .addresses
            .extend(peer_addresses.into_iter().filter_map(AddressRecord::from_multiaddr));

        num_added
    }

    /// Dial peer using `PeerId`.
    ///
    /// Returns an error if the peer is unknown or the peer is already connected.
    pub fn dial(&self, peer: &PeerId) -> Result<(), ImmediateDialError> {
        if peer == &self.local_peer_id {
            return Err(ImmediateDialError::TriedToDialSelf);
        }

        {
            let peers = self.peers.read();
            let Some(PeerContext { state, addresses }) = peers.get(peer) else {
                return Err(ImmediateDialError::NoAddressAvailable);
            };

            match state.can_dial() {
                StateDialResult::AlreadyConnected =>
                    return Err(ImmediateDialError::AlreadyConnected),
                StateDialResult::DialingInProgress => return Ok(()),
                StateDialResult::Ok => {}
            };

            // Check if we have enough addresses to dial.
            if addresses.is_empty() {
                return Err(ImmediateDialError::NoAddressAvailable);
            }
        }

        self.cmd_tx
            .try_send(InnerTransportManagerCommand::DialPeer { peer: *peer })
            .map_err(|error| match error {
                TrySendError::Full(_) => ImmediateDialError::ChannelClogged,
                TrySendError::Closed(_) => ImmediateDialError::TaskClosed,
            })
    }

    /// Dial peer using `Multiaddr`.
    ///
    /// Returns an error if address it not valid.
    pub fn dial_address(&self, address: Multiaddr) -> Result<(), ImmediateDialError> {
        if !address.iter().any(|protocol| std::matches!(protocol, Protocol::P2p(_))) {
            return Err(ImmediateDialError::PeerIdMissing);
        }

        self.cmd_tx
            .try_send(InnerTransportManagerCommand::DialAddress { address })
            .map_err(|error| match error {
                TrySendError::Full(_) => ImmediateDialError::ChannelClogged,
                TrySendError::Closed(_) => ImmediateDialError::TaskClosed,
            })
    }

    /// Dynamically unregister a protocol.
    ///
    /// This must be called when a protocol is no longer needed (e.g. user dropped the protocol
    /// handle).
    pub fn unregister_protocol(&self, protocol: ProtocolName) {
        tracing::info!(
            target: LOG_TARGET,
            ?protocol,
            "Unregistering user protocol on handle drop"
        );

        if let Err(err) = self
            .cmd_tx
            .try_send(InnerTransportManagerCommand::UnregisterProtocol { protocol })
        {
            tracing::error!(
                target: LOG_TARGET,
                ?err,
                "Failed to unregister protocol"
            );
        }
    }
}

pub struct TransportHandle {
    pub keypair: Keypair,
    pub tx: Sender<TransportManagerEvent>,
    pub protocols: HashMap<ProtocolName, ProtocolContext>,
    pub next_connection_id: Arc<AtomicUsize>,
    pub next_substream_id: Arc<AtomicUsize>,
    pub bandwidth_sink: BandwidthSink,
    pub executor: Arc<dyn Executor>,
}

impl TransportHandle {
    pub fn protocol_set(&self, connection_id: ConnectionId) -> ProtocolSet {
        ProtocolSet::new(
            connection_id,
            self.tx.clone(),
            self.next_substream_id.clone(),
            self.protocols.clone(),
        )
    }

    /// Get next connection ID.
    pub fn next_connection_id(&mut self) -> ConnectionId {
        let connection_id = self.next_connection_id.fetch_add(1usize, Ordering::Relaxed);

        ConnectionId::from(connection_id)
    }
}

#[cfg(test)]
mod tests {
    use crate::transport::manager::{
        address::AddressStore,
        peer_state::{ConnectionRecord, PeerState},
    };

    use super::*;
    use multihash::Multihash;
    use parking_lot::lock_api::RwLock;
    use tokio::sync::mpsc::{channel, Receiver};

    fn make_transport_manager_handle() -> (
        TransportManagerHandle,
        Receiver<InnerTransportManagerCommand>,
    ) {
        let (cmd_tx, cmd_rx) = channel(64);

        let local_peer_id = PeerId::random();
        (
            TransportManagerHandle {
                local_peer_id,
                cmd_tx,
                peers: Default::default(),
                supported_transport: HashSet::new(),
                listen_addresses: Default::default(),
                public_addresses: PublicAddresses::new(local_peer_id),
            },
            cmd_rx,
        )
    }

    #[tokio::test]
    async fn tcp_supported() {
        let (mut handle, _rx) = make_transport_manager_handle();
        handle.supported_transport.insert(SupportedTransport::Tcp);

        let address =
            "/dns4/google.com/tcp/24928/p2p/12D3KooWKrUnV42yDR7G6DewmgHtFaVCJWLjQRi2G9t5eJD3BvTy"
                .parse()
                .unwrap();
        assert!(handle.supported_transport(&address));
    }

    #[tokio::test]
    async fn tcp_unsupported() {
        let (handle, _rx) = make_transport_manager_handle();

        let address =
            "/dns4/google.com/tcp/24928/p2p/12D3KooWKrUnV42yDR7G6DewmgHtFaVCJWLjQRi2G9t5eJD3BvTy"
                .parse()
                .unwrap();
        assert!(!handle.supported_transport(&address));
    }

    #[tokio::test]
    async fn tcp_non_terminal_unsupported() {
        let (mut handle, _rx) = make_transport_manager_handle();
        handle.supported_transport.insert(SupportedTransport::Tcp);

        let address =
            "/dns4/google.com/tcp/24928/p2p/12D3KooWKrUnV42yDR7G6DewmgHtFaVCJWLjQRi2G9t5eJD3BvTy/p2p-circuit"
                .parse()
                .unwrap();
        assert!(!handle.supported_transport(&address));
    }

    #[cfg(feature = "websocket")]
    #[tokio::test]
    async fn websocket_supported() {
        let (mut handle, _rx) = make_transport_manager_handle();
        handle.supported_transport.insert(SupportedTransport::WebSocket);

        let address =
            "/dns4/google.com/tcp/24928/ws/p2p/12D3KooWKrUnV42yDR7G6DewmgHtFaVCJWLjQRi2G9t5eJD3BvTy"
                .parse()
                .unwrap();
        assert!(handle.supported_transport(&address));
    }

    #[cfg(feature = "websocket")]
    #[tokio::test]
    async fn websocket_unsupported() {
        let (handle, _rx) = make_transport_manager_handle();

        let address =
            "/dns4/google.com/tcp/24928/ws/p2p/12D3KooWKrUnV42yDR7G6DewmgHtFaVCJWLjQRi2G9t5eJD3BvTy"
                .parse()
                .unwrap();
        assert!(!handle.supported_transport(&address));
    }

    #[cfg(feature = "websocket")]
    #[tokio::test]
    async fn websocket_non_terminal_unsupported() {
        let (mut handle, _rx) = make_transport_manager_handle();
        handle.supported_transport.insert(SupportedTransport::WebSocket);

        let address =
            "/dns4/google.com/tcp/24928/ws/p2p/12D3KooWKrUnV42yDR7G6DewmgHtFaVCJWLjQRi2G9t5eJD3BvTy/p2p-circuit"
                .parse()
                .unwrap();
        assert!(!handle.supported_transport(&address));
    }

    #[cfg(feature = "websocket")]
    #[tokio::test]
    async fn wss_supported() {
        let (mut handle, _rx) = make_transport_manager_handle();
        handle.supported_transport.insert(SupportedTransport::WebSocket);

        let address =
            "/dns4/google.com/tcp/24928/wss/p2p/12D3KooWKrUnV42yDR7G6DewmgHtFaVCJWLjQRi2G9t5eJD3BvTy"
                .parse()
                .unwrap();
        assert!(handle.supported_transport(&address));
    }

    #[cfg(feature = "websocket")]
    #[tokio::test]
    async fn wss_unsupported() {
        let (handle, _rx) = make_transport_manager_handle();

        let address =
            "/dns4/google.com/tcp/24928/wss/p2p/12D3KooWKrUnV42yDR7G6DewmgHtFaVCJWLjQRi2G9t5eJD3BvTy"
                .parse()
                .unwrap();
        assert!(!handle.supported_transport(&address));
    }

    #[cfg(feature = "websocket")]
    #[tokio::test]
    async fn wss_non_terminal_unsupported() {
        let (mut handle, _rx) = make_transport_manager_handle();
        handle.supported_transport.insert(SupportedTransport::WebSocket);

        let address =
            "/dns4/google.com/tcp/24928/wss/p2p/12D3KooWKrUnV42yDR7G6DewmgHtFaVCJWLjQRi2G9t5eJD3BvTy/p2p-circuit"
                .parse()
                .unwrap();
        assert!(!handle.supported_transport(&address));
    }

    #[cfg(feature = "quic")]
    #[tokio::test]
    async fn quic_supported() {
        let (mut handle, _rx) = make_transport_manager_handle();
        handle.supported_transport.insert(SupportedTransport::Quic);

        let address =
            "/dns4/google.com/udp/24928/quic-v1/p2p/12D3KooWKrUnV42yDR7G6DewmgHtFaVCJWLjQRi2G9t5eJD3BvTy"
                .parse()
                .unwrap();
        assert!(handle.supported_transport(&address));
    }

    #[cfg(feature = "quic")]
    #[tokio::test]
    async fn quic_unsupported() {
        let (handle, _rx) = make_transport_manager_handle();

        let address =
            "/dns4/google.com/udp/24928/quic-v1/p2p/12D3KooWKrUnV42yDR7G6DewmgHtFaVCJWLjQRi2G9t5eJD3BvTy"
                .parse()
                .unwrap();
        assert!(!handle.supported_transport(&address));
    }

    #[cfg(feature = "quic")]
    #[tokio::test]
    async fn quic_non_terminal_unsupported() {
        let (mut handle, _rx) = make_transport_manager_handle();
        handle.supported_transport.insert(SupportedTransport::Quic);

        let address =
            "/dns4/google.com/udp/24928/quic-v1/p2p/12D3KooWKrUnV42yDR7G6DewmgHtFaVCJWLjQRi2G9t5eJD3BvTy/p2p-circuit"
                .parse()
                .unwrap();
        assert!(!handle.supported_transport(&address));
    }

    #[test]
    fn transport_not_supported() {
        let (handle, _rx) = make_transport_manager_handle();

        // only peer id (used by Polkadot sometimes)
        assert!(!handle.supported_transport(
            &Multiaddr::empty().with(Protocol::P2p(Multihash::from(PeerId::random())))
        ));

        // only one transport
        assert!(!handle.supported_transport(
            &Multiaddr::empty().with(Protocol::Ip4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
        ));

        // any udp-based protocol other than quic
        assert!(!handle.supported_transport(
            &Multiaddr::empty()
                .with(Protocol::Ip4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
                .with(Protocol::Udp(8888))
                .with(Protocol::Utp)
        ));

        // any other protocol other than tcp
        assert!(!handle.supported_transport(
            &Multiaddr::empty()
                .with(Protocol::Ip4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
                .with(Protocol::Sctp(8888))
        ));
    }

    #[test]
    fn zero_addresses_added() {
        let (mut handle, _rx) = make_transport_manager_handle();
        handle.supported_transport.insert(SupportedTransport::Tcp);

        assert!(
            handle.add_known_address(
                &PeerId::random(),
                vec![
                    Multiaddr::empty()
                        .with(Protocol::Ip4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
                        .with(Protocol::Udp(8888))
                        .with(Protocol::Utp),
                    Multiaddr::empty()
                        .with(Protocol::Ip4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
                        .with(Protocol::Tcp(8888))
                        .with(Protocol::Wss(std::borrow::Cow::Owned("/".to_string()))),
                ]
                .into_iter()
            ) == 0usize
        );
    }

    #[tokio::test]
    async fn dial_already_connected_peer() {
        let (mut handle, _rx) = make_transport_manager_handle();
        handle.supported_transport.insert(SupportedTransport::Tcp);

        let peer = {
            let peer = PeerId::random();
            let mut peers = handle.peers.write();

            peers.insert(
                peer,
                PeerContext {
                    state: PeerState::Connected {
                        record: ConnectionRecord {
                            address: Multiaddr::empty()
                                .with(Protocol::Ip4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
                                .with(Protocol::Tcp(8888))
                                .with(Protocol::P2p(Multihash::from(peer))),
                            connection_id: ConnectionId::from(0),
                        },
                        secondary: None,
                    },

                    addresses: AddressStore::from_iter(
                        vec![Multiaddr::empty()
                            .with(Protocol::Ip4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
                            .with(Protocol::Tcp(8888))
                            .with(Protocol::P2p(Multihash::from(peer)))]
                        .into_iter(),
                    ),
                },
            );
            drop(peers);

            peer
        };

        match handle.dial(&peer) {
            Err(ImmediateDialError::AlreadyConnected) => {}
            _ => panic!("invalid return value"),
        }
    }

    #[tokio::test]
    async fn peer_already_being_dialed() {
        let (mut handle, _rx) = make_transport_manager_handle();
        handle.supported_transport.insert(SupportedTransport::Tcp);

        let peer = {
            let peer = PeerId::random();
            let mut peers = handle.peers.write();

            peers.insert(
                peer,
                PeerContext {
                    state: PeerState::Dialing {
                        dial_record: ConnectionRecord {
                            address: Multiaddr::empty()
                                .with(Protocol::Ip4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
                                .with(Protocol::Tcp(8888))
                                .with(Protocol::P2p(Multihash::from(peer))),
                            connection_id: ConnectionId::from(0),
                        },
                    },

                    addresses: AddressStore::from_iter(
                        vec![Multiaddr::empty()
                            .with(Protocol::Ip4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
                            .with(Protocol::Tcp(8888))
                            .with(Protocol::P2p(Multihash::from(peer)))]
                        .into_iter(),
                    ),
                },
            );
            drop(peers);

            peer
        };

        match handle.dial(&peer) {
            Ok(()) => {}
            _ => panic!("invalid return value"),
        }
    }

    #[tokio::test]
    async fn no_address_available_for_peer() {
        let (mut handle, _rx) = make_transport_manager_handle();
        handle.supported_transport.insert(SupportedTransport::Tcp);

        let peer = {
            let peer = PeerId::random();
            let mut peers = handle.peers.write();

            peers.insert(
                peer,
                PeerContext {
                    state: PeerState::Disconnected { dial_record: None },
                    addresses: AddressStore::new(),
                },
            );
            drop(peers);

            peer
        };

        let err = handle.dial(&peer).unwrap_err();
        assert!(matches!(err, ImmediateDialError::NoAddressAvailable));
    }

    #[tokio::test]
    async fn pending_connection_for_disconnected_peer() {
        let (mut handle, mut rx) = make_transport_manager_handle();
        handle.supported_transport.insert(SupportedTransport::Tcp);

        let peer = {
            let peer = PeerId::random();
            let mut peers = handle.peers.write();

            peers.insert(
                peer,
                PeerContext {
                    state: PeerState::Disconnected {
                        dial_record: Some(ConnectionRecord::new(
                            peer,
                            Multiaddr::empty()
                                .with(Protocol::Ip4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
                                .with(Protocol::Tcp(8888))
                                .with(Protocol::P2p(Multihash::from(peer))),
                            ConnectionId::from(0),
                        )),
                    },

                    addresses: AddressStore::from_iter(
                        vec![Multiaddr::empty()
                            .with(Protocol::Ip4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
                            .with(Protocol::Tcp(8888))
                            .with(Protocol::P2p(Multihash::from(peer)))]
                        .into_iter(),
                    ),
                },
            );
            drop(peers);

            peer
        };

        match handle.dial(&peer) {
            Ok(()) => {}
            _ => panic!("invalid return value"),
        }
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn try_to_dial_self() {
        let (mut handle, mut rx) = make_transport_manager_handle();
        handle.supported_transport.insert(SupportedTransport::Tcp);

        let err = handle.dial(&handle.local_peer_id).unwrap_err();
        assert_eq!(err, ImmediateDialError::TriedToDialSelf);

        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn is_local_address() {
        let (cmd_tx, _cmd_rx) = channel(64);

        let local_peer_id = PeerId::random();
        let specific_bind: Multiaddr = "/ip6/::1/tcp/8888".parse().expect("valid multiaddress");
        let ipv6_bind: Multiaddr = "/ip4/127.0.0.1/tcp/8888".parse().expect("valid multiaddress");
        let wildcard_bind: Multiaddr = "/ip4/0.0.0.0/tcp/9000".parse().unwrap();

        let listen_addresses = Arc::new(RwLock::new(
            [specific_bind, wildcard_bind, ipv6_bind].into_iter().collect(),
        ));
        println!("{:?}", listen_addresses);

        let handle = TransportManagerHandle {
            local_peer_id,
            cmd_tx,
            peers: Default::default(),
            supported_transport: HashSet::new(),
            listen_addresses,
            public_addresses: PublicAddresses::new(local_peer_id),
        };

        // Exact matches
        assert!(handle
            .is_local_address(&"/ip4/127.0.0.1/tcp/8888".parse().expect("valid multiaddress")));
        assert!(handle.is_local_address(
            &"/ip6/::1/tcp/8888".parse::<Multiaddr>().expect("valid multiaddress")
        ));

        // Peer ID stripping
        assert!(handle.is_local_address(
            &"/ip6/::1/tcp/8888/p2p/12D3KooWT2ouvz5uMmCvHJGzAGRHiqDts5hzXR7NdoQ27pGdzp9Q"
                .parse()
                .expect("valid multiaddress")
        ));
        assert!(handle.is_local_address(
            &"/ip4/127.0.0.1/tcp/8888/p2p/12D3KooWT2ouvz5uMmCvHJGzAGRHiqDts5hzXR7NdoQ27pGdzp9Q"
                .parse()
                .expect("valid multiaddress")
        ));
        // same address but different peer id
        assert!(handle.is_local_address(
            &"/ip6/::1/tcp/8888/p2p/12D3KooWPGxxxQiBEBZ52RY31Z2chn4xsDrGCMouZ88izJrak2T1"
                .parse::<Multiaddr>()
                .expect("valid multiaddress")
        ));
        assert!(handle.is_local_address(
            &"/ip4/127.0.0.1/tcp/8888/p2p/12D3KooWPGxxxQiBEBZ52RY31Z2chn4xsDrGCMouZ88izJrak2T1"
                .parse()
                .expect("valid multiaddress")
        ));

        // Port collision protection: we listen on 0.0.0.0:9000 and should match any loopback
        // address on port 9000.
        assert!(
            handle.is_local_address(&"/ip4/127.0.0.1/tcp/9000".parse().unwrap()),
            "Loopback input should satisfy Wildcard (0.0.0.0) listener"
        );
        // 8.8.8.8 is a different IP.
        assert!(
            !handle.is_local_address(&"/ip4/8.8.8.8/tcp/9000".parse().unwrap()),
            "Remote IP with same port should NOT be considered local against Wildcard listener"
        );

        // Port mismatches
        assert!(
            !handle.is_local_address(&"/ip4/127.0.0.1/tcp/1234".parse().unwrap()),
            "Same IP but different port should fail"
        );
        assert!(
            !handle.is_local_address(&"/ip4/0.0.0.0/tcp/1234".parse().unwrap()),
            "Wildcard IP but different port should fail"
        );
        assert!(!handle
            .is_local_address(&"/ip4/127.0.0.1/tcp/9999".parse().expect("valid multiaddress")));
        assert!(!handle
            .is_local_address(&"/ip4/127.0.0.1/tcp/7777".parse().expect("valid multiaddress")));
    }
}
