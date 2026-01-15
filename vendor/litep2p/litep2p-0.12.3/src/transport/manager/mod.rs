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
    codec::ProtocolCodec,
    crypto::ed25519::Keypair,
    error::{AddressError, DialError, Error},
    executor::Executor,
    protocol::{InnerTransportEvent, TransportService},
    transport::{
        manager::{
            address::AddressRecord,
            handle::InnerTransportManagerCommand,
            peer_state::{ConnectionRecord, PeerState, StateDialResult},
            types::PeerContext,
        },
        Endpoint, Transport, TransportEvent, MAX_PARALLEL_DIALS,
    },
    types::{protocol::ProtocolName, ConnectionId},
    BandwidthSink, PeerId,
};

use address::{scores, AddressStore};
use futures::{Stream, StreamExt};
use indexmap::IndexMap;
use multiaddr::{Multiaddr, Protocol};
use multihash::Multihash;
use parking_lot::RwLock;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use std::{
    collections::{HashMap, HashSet},
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::Duration,
};

pub use handle::{TransportHandle, TransportManagerHandle};
pub use types::SupportedTransport;

pub(crate) mod address;
pub mod limits;
mod peer_state;
mod types;

pub(crate) mod handle;

// TODO: https://github.com/paritytech/litep2p/issues/268 Periodically clean up idle peers.
// TODO: https://github.com/paritytech/litep2p/issues/344 add lots of documentation

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::transport-manager";

/// The connection established result.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ConnectionEstablishedResult {
    /// Accept connection and inform `Litep2p` about the connection.
    Accept,

    /// Reject connection.
    Reject,
}

/// [`crate::transport::manager::TransportManager`] events.
pub enum TransportManagerEvent {
    /// Connection closed to remote peer.
    ConnectionClosed {
        /// Peer ID.
        peer: PeerId,

        /// Connection ID.
        connection: ConnectionId,
    },
}

// Protocol context.
#[derive(Debug, Clone)]
pub struct ProtocolContext {
    /// Codec used by the protocol.
    pub codec: ProtocolCodec,

    /// TX channel for sending events to protocol.
    pub tx: Sender<InnerTransportEvent>,

    /// Fallback names for the protocol.
    pub fallback_names: Vec<ProtocolName>,
}

impl ProtocolContext {
    /// Create new [`ProtocolContext`].
    fn new(
        codec: ProtocolCodec,
        tx: Sender<InnerTransportEvent>,
        fallback_names: Vec<ProtocolName>,
    ) -> Self {
        Self {
            tx,
            codec,
            fallback_names,
        }
    }
}

/// Transport context for enabled transports.
struct TransportContext {
    /// Polling index.
    index: usize,

    /// Registered transports.
    transports: IndexMap<SupportedTransport, Box<dyn Transport<Item = TransportEvent>>>,
}

impl TransportContext {
    /// Create new [`TransportContext`].
    pub fn new() -> Self {
        Self {
            index: 0usize,
            transports: IndexMap::new(),
        }
    }

    /// Get an iterator of supported transports.
    pub fn keys(&self) -> impl Iterator<Item = &SupportedTransport> {
        self.transports.keys()
    }

    /// Get mutable access to transport.
    pub fn get_mut(
        &mut self,
        key: &SupportedTransport,
    ) -> Option<&mut Box<dyn Transport<Item = TransportEvent>>> {
        self.transports.get_mut(key)
    }

    /// Register `transport` to `TransportContext`.
    pub fn register_transport(
        &mut self,
        name: SupportedTransport,
        transport: Box<dyn Transport<Item = TransportEvent>>,
    ) {
        assert!(self.transports.insert(name, transport).is_none());
    }
}

impl Stream for TransportContext {
    type Item = (SupportedTransport, TransportEvent);

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.transports.is_empty() {
            // Terminate if we don't have any transports installed.
            return Poll::Ready(None);
        }

        let len = self.transports.len();
        for _ in 0..len {
            let current = self.index;
            self.index = (current + 1) % len;
            let (key, stream) = self.transports.get_index_mut(current).expect("transport to exist");
            match stream.poll_next_unpin(cx) {
                Poll::Pending => {}
                Poll::Ready(None) => {
                    return Poll::Ready(None);
                }
                Poll::Ready(Some(event)) => {
                    let event = Some((*key, event));
                    return Poll::Ready(event);
                }
            }
        }

        Poll::Pending
    }
}

/// Litep2p connection manager.
pub struct TransportManager {
    /// Local peer ID.
    local_peer_id: PeerId,

    /// Keypair.
    keypair: Keypair,

    /// Bandwidth sink.
    bandwidth_sink: BandwidthSink,

    /// Maximum parallel dial attempts per peer.
    max_parallel_dials: usize,

    /// Installed protocols.
    protocols: HashMap<ProtocolName, ProtocolContext>,

    /// All names (main and fallback(s)) of the installed protocols.
    protocol_names: HashSet<ProtocolName>,

    /// Listen addresses.
    listen_addresses: Arc<RwLock<HashSet<Multiaddr>>>,

    /// Listen addresses.
    public_addresses: PublicAddresses,

    /// Next connection ID.
    next_connection_id: Arc<AtomicUsize>,

    /// Next substream ID.
    next_substream_id: Arc<AtomicUsize>,

    /// Installed transports.
    transports: TransportContext,

    /// Peers
    peers: Arc<RwLock<HashMap<PeerId, PeerContext>>>,

    /// Handle to [`crate::transport::manager::TransportManager`].
    transport_manager_handle: TransportManagerHandle,

    /// RX channel for receiving events from installed transports.
    event_rx: Receiver<TransportManagerEvent>,

    /// RX channel for receiving commands from installed protocols.
    cmd_rx: Receiver<InnerTransportManagerCommand>,

    /// TX channel for transport events that is given to installed transports.
    event_tx: Sender<TransportManagerEvent>,

    /// Pending connections.
    pending_connections: HashMap<ConnectionId, PeerId>,

    /// Connection limits.
    connection_limits: limits::ConnectionLimits,

    /// Opening connections errors.
    opening_errors: HashMap<ConnectionId, Vec<(Multiaddr, DialError)>>,
}

/// Builder for [`crate::transport::manager::TransportManager`].
pub struct TransportManagerBuilder {
    /// Keypair.
    keypair: Option<Keypair>,

    /// Supported transports.
    supported_transports: HashSet<SupportedTransport>,

    /// Bandwidth sink.
    bandwidth_sink: Option<BandwidthSink>,

    /// Maximum parallel dial attempts per peer.
    max_parallel_dials: usize,

    /// Connection limits config.
    connection_limits_config: limits::ConnectionLimitsConfig,
}

impl Default for TransportManagerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TransportManagerBuilder {
    /// Create new [`crate::transport::manager::TransportManagerBuilder`].
    pub fn new() -> Self {
        Self {
            keypair: None,
            supported_transports: HashSet::new(),
            bandwidth_sink: None,
            max_parallel_dials: MAX_PARALLEL_DIALS,
            connection_limits_config: limits::ConnectionLimitsConfig::default(),
        }
    }

    /// Set the keypair
    pub fn with_keypair(mut self, keypair: Keypair) -> Self {
        self.keypair = Some(keypair);
        self
    }

    /// Set the supported transports
    pub fn with_supported_transports(
        mut self,
        supported_transports: HashSet<SupportedTransport>,
    ) -> Self {
        self.supported_transports = supported_transports;
        self
    }

    /// Set the bandwidth sink
    pub fn with_bandwidth_sink(mut self, bandwidth_sink: BandwidthSink) -> Self {
        self.bandwidth_sink = Some(bandwidth_sink);
        self
    }

    /// Set the maximum parallel dials per peer
    pub fn with_max_parallel_dials(mut self, max_parrallel_dials: usize) -> Self {
        self.max_parallel_dials = max_parrallel_dials;
        self
    }

    /// Set connection limits configuration.
    pub fn with_connection_limits_config(
        mut self,
        connection_limits_config: limits::ConnectionLimitsConfig,
    ) -> Self {
        self.connection_limits_config = connection_limits_config;
        self
    }

    /// Build [`TransportManager`].
    pub fn build(self) -> TransportManager {
        let keypair = self.keypair.unwrap_or_else(Keypair::generate);
        let local_peer_id = PeerId::from_public_key(&keypair.public().into());
        let peers = Arc::new(RwLock::new(HashMap::new()));
        let (cmd_tx, cmd_rx) = channel(256);
        let (event_tx, event_rx) = channel(256);
        let listen_addresses = Arc::new(RwLock::new(HashSet::new()));
        let public_addresses = PublicAddresses::new(local_peer_id);

        let handle = TransportManagerHandle::new(
            local_peer_id,
            peers.clone(),
            cmd_tx,
            self.supported_transports,
            listen_addresses.clone(),
            public_addresses.clone(),
        );

        TransportManager {
            local_peer_id,
            keypair,
            bandwidth_sink: self.bandwidth_sink.unwrap_or_else(BandwidthSink::new),
            max_parallel_dials: self.max_parallel_dials,
            protocols: HashMap::new(),
            protocol_names: HashSet::new(),
            listen_addresses,
            public_addresses,
            next_connection_id: Arc::new(AtomicUsize::new(0usize)),
            next_substream_id: Arc::new(AtomicUsize::new(0usize)),
            transports: TransportContext::new(),
            peers,
            transport_manager_handle: handle,
            event_rx,
            cmd_rx,
            event_tx,
            pending_connections: HashMap::new(),
            connection_limits: limits::ConnectionLimits::new(self.connection_limits_config),
            opening_errors: HashMap::new(),
        }
    }
}

impl TransportManager {
    /// Get iterator to installed protocols.
    pub fn protocols(&self) -> impl Iterator<Item = &ProtocolName> {
        self.protocols.keys()
    }

    /// Get iterator to installed transports
    pub fn installed_transports(&self) -> impl Iterator<Item = &SupportedTransport> {
        self.transports.keys()
    }

    /// Get next connection ID.
    fn next_connection_id(&self) -> ConnectionId {
        let connection_id = self.next_connection_id.fetch_add(1usize, Ordering::Relaxed);

        ConnectionId::from(connection_id)
    }

    /// Get the transport manager handle
    pub fn transport_manager_handle(&self) -> TransportManagerHandle {
        self.transport_manager_handle.clone()
    }

    /// Register protocol to the [`crate::transport::manager::TransportManager`].
    ///
    /// This allocates new context for the protocol and returns a handle
    /// which the protocol can use the interact with the transport subsystem.
    pub fn register_protocol(
        &mut self,
        protocol: ProtocolName,
        fallback_names: Vec<ProtocolName>,
        codec: ProtocolCodec,
        keep_alive_timeout: Duration,
    ) -> TransportService {
        assert!(!self.protocol_names.contains(&protocol));

        for fallback in &fallback_names {
            if self.protocol_names.contains(fallback) {
                panic!("duplicate fallback protocol given: {fallback:?}");
            }
        }

        let (service, sender) = TransportService::new(
            self.local_peer_id,
            protocol.clone(),
            fallback_names.clone(),
            self.next_substream_id.clone(),
            self.transport_manager_handle(),
            keep_alive_timeout,
        );

        self.protocols.insert(
            protocol.clone(),
            ProtocolContext::new(codec, sender, fallback_names.clone()),
        );
        self.protocol_names.insert(protocol);
        self.protocol_names.extend(fallback_names);

        service
    }

    /// Unregister a protocol in response of the user dropping the protocol handle.
    fn unregister_protocol(&mut self, protocol: ProtocolName) {
        let Some(context) = self.protocols.remove(&protocol) else {
            tracing::error!(target: LOG_TARGET, ?protocol, "Cannot unregister protocol, not registered");
            return;
        };

        for fallback in &context.fallback_names {
            if !self.protocol_names.remove(fallback) {
                tracing::error!(target: LOG_TARGET, ?fallback, ?protocol, "Cannot unregister fallback protocol, not registered");
            }
        }

        tracing::info!(
            target: LOG_TARGET,
            ?protocol,
            "Protocol fully unregistered"
        );
    }

    /// Acquire `TransportHandle`.
    pub fn transport_handle(&self, executor: Arc<dyn Executor>) -> TransportHandle {
        TransportHandle {
            tx: self.event_tx.clone(),
            executor,
            keypair: self.keypair.clone(),
            protocols: self.protocols.clone(),
            bandwidth_sink: self.bandwidth_sink.clone(),
            next_substream_id: self.next_substream_id.clone(),
            next_connection_id: self.next_connection_id.clone(),
        }
    }

    /// Register transport to `TransportManager`.
    pub(crate) fn register_transport(
        &mut self,
        name: SupportedTransport,
        transport: Box<dyn Transport<Item = TransportEvent>>,
    ) {
        tracing::debug!(target: LOG_TARGET, transport = ?name, "register transport");

        self.transports.register_transport(name, transport);
        self.transport_manager_handle.register_transport(name);
    }

    /// Get the list of public addresses of the node.
    pub(crate) fn public_addresses(&self) -> PublicAddresses {
        self.public_addresses.clone()
    }

    /// Register local listen address.
    pub fn register_listen_address(&mut self, address: Multiaddr) {
        assert!(!address.iter().any(|protocol| std::matches!(protocol, Protocol::P2p(_))));

        let mut listen_addresses = self.listen_addresses.write();

        listen_addresses.insert(address.clone());
        listen_addresses.insert(address.with(Protocol::P2p(
            Multihash::from_bytes(&self.local_peer_id.to_bytes()).unwrap(),
        )));
    }

    /// Add one or more known addresses for `peer`.
    pub fn add_known_address(
        &mut self,
        peer: PeerId,
        address: impl Iterator<Item = Multiaddr>,
    ) -> usize {
        self.transport_manager_handle.add_known_address(&peer, address)
    }

    /// Return multiple addresses to dial on supported protocols.
    fn supported_transports_addresses(
        addresses: &[Multiaddr],
    ) -> HashMap<SupportedTransport, Vec<Multiaddr>> {
        let mut transports = HashMap::<SupportedTransport, Vec<Multiaddr>>::new();

        for address in addresses.iter().cloned() {
            #[cfg(feature = "quic")]
            if address.iter().any(|p| std::matches!(&p, Protocol::QuicV1)) {
                transports.entry(SupportedTransport::Quic).or_default().push(address);
                continue;
            }

            #[cfg(feature = "websocket")]
            if address.iter().any(|p| std::matches!(&p, Protocol::Ws(_) | Protocol::Wss(_))) {
                transports.entry(SupportedTransport::WebSocket).or_default().push(address);
                continue;
            }

            transports.entry(SupportedTransport::Tcp).or_default().push(address);
        }

        transports
    }

    /// Dial peer using `PeerId`.
    ///
    /// Returns an error if the peer is unknown or the peer is already connected.
    pub async fn dial(&mut self, peer: PeerId) -> crate::Result<()> {
        // Don't alter the peer state if there's no capacity to dial.
        let available_capacity = self.connection_limits.on_dial_address()?;
        // The available capacity is the maximum number of connections that can be established,
        // so we limit the number of parallel dials to the minimum of these values.
        let limit = available_capacity.min(self.max_parallel_dials);

        if peer == self.local_peer_id {
            return Err(Error::TriedToDialSelf);
        }
        let mut peers = self.peers.write();

        let context = peers.entry(peer).or_default();

        // Check if dialing is possible before allocating addresses.
        match context.state.can_dial() {
            StateDialResult::AlreadyConnected => return Err(Error::AlreadyConnected),
            StateDialResult::DialingInProgress => return Ok(()),
            StateDialResult::Ok => {}
        };

        // The addresses are sorted by score and contain the remote peer ID.
        // We double checked above that the remote peer is not the local peer.
        let dial_addresses = context.addresses.addresses(limit);
        if dial_addresses.is_empty() {
            return Err(Error::NoAddressAvailable(peer));
        }
        let connection_id = self.next_connection_id();

        tracing::debug!(
            target: LOG_TARGET,
            ?connection_id,
            addresses = ?dial_addresses,
            "dial remote peer",
        );

        let transports = Self::supported_transports_addresses(&dial_addresses);

        // Dialing addresses will succeed because the `context.state.can_dial()` returned `Ok`.
        let result = context.state.dial_addresses(
            connection_id,
            dial_addresses.iter().cloned().collect(),
            transports.keys().cloned().collect(),
        );
        if result != StateDialResult::Ok {
            tracing::warn!(
                target: LOG_TARGET,
                ?peer,
                ?connection_id,
                state = ?context.state,
                "invalid state for dialing",
            );
        }

        for (transport, addresses) in transports {
            if addresses.is_empty() {
                continue;
            }

            let Some(installed_transport) = self.transports.get_mut(&transport) else {
                continue;
            };

            installed_transport.open(connection_id, addresses)?;
        }

        self.pending_connections.insert(connection_id, peer);

        Ok(())
    }

    /// Dial peer using `Multiaddr`.
    ///
    /// Returns an error if address it not valid.
    pub async fn dial_address(&mut self, address: Multiaddr) -> crate::Result<()> {
        self.connection_limits.on_dial_address()?;

        let address_record = AddressRecord::from_multiaddr(address)
            .ok_or(Error::AddressError(AddressError::PeerIdMissing))?;

        if self.listen_addresses.read().contains(address_record.as_ref()) {
            return Err(Error::TriedToDialSelf);
        }

        tracing::debug!(target: LOG_TARGET, address = ?address_record.address(), "dial address");

        let mut protocol_stack = address_record.as_ref().iter();
        match protocol_stack
            .next()
            .ok_or_else(|| Error::TransportNotSupported(address_record.address().clone()))?
        {
            Protocol::Ip4(_) | Protocol::Ip6(_) => {}
            Protocol::Dns(_) | Protocol::Dns4(_) | Protocol::Dns6(_) => {}
            transport => {
                tracing::error!(
                    target: LOG_TARGET,
                    ?transport,
                    "invalid transport, expected `ip4`/`ip6`"
                );
                return Err(Error::TransportNotSupported(
                    address_record.address().clone(),
                ));
            }
        };

        let supported_transport = match protocol_stack
            .next()
            .ok_or_else(|| Error::TransportNotSupported(address_record.address().clone()))?
        {
            Protocol::Tcp(_) => match protocol_stack.next() {
                #[cfg(feature = "websocket")]
                Some(Protocol::Ws(_)) | Some(Protocol::Wss(_)) => SupportedTransport::WebSocket,
                Some(Protocol::P2p(_)) => SupportedTransport::Tcp,
                _ =>
                    return Err(Error::TransportNotSupported(
                        address_record.address().clone(),
                    )),
            },
            #[cfg(feature = "quic")]
            Protocol::Udp(_) => match protocol_stack
                .next()
                .ok_or_else(|| Error::TransportNotSupported(address_record.address().clone()))?
            {
                Protocol::QuicV1 => SupportedTransport::Quic,
                _ => {
                    tracing::debug!(target: LOG_TARGET, address = ?address_record.address(), "expected `quic-v1`");
                    return Err(Error::TransportNotSupported(
                        address_record.address().clone(),
                    ));
                }
            },
            protocol => {
                tracing::error!(
                    target: LOG_TARGET,
                    ?protocol,
                    "invalid protocol"
                );

                return Err(Error::TransportNotSupported(
                    address_record.address().clone(),
                ));
            }
        };

        // when constructing `AddressRecord`, `PeerId` was verified to be part of the address
        let remote_peer_id =
            PeerId::try_from_multiaddr(address_record.address()).expect("`PeerId` to exist");

        // set connection id for the address record and put peer into `Dialing` state
        let connection_id = self.next_connection_id();
        let dial_record = ConnectionRecord {
            address: address_record.address().clone(),
            connection_id,
        };

        {
            let mut peers = self.peers.write();

            let context = peers.entry(remote_peer_id).or_default();

            // Keep the provided record around for possible future dials.
            context.addresses.insert(address_record.clone());

            match context.state.dial_single_address(dial_record) {
                StateDialResult::AlreadyConnected => return Err(Error::AlreadyConnected),
                StateDialResult::DialingInProgress => return Ok(()),
                StateDialResult::Ok => {}
            };
        }

        self.transports
            .get_mut(&supported_transport)
            .ok_or(Error::TransportNotSupported(
                address_record.address().clone(),
            ))?
            .dial(connection_id, address_record.address().clone())?;
        self.pending_connections.insert(connection_id, remote_peer_id);

        Ok(())
    }

    // Update the address on a dial failure.
    fn update_address_on_dial_failure(&mut self, address: Multiaddr, error: &DialError) {
        let mut peers = self.peers.write();

        let score = AddressStore::error_score(error);

        // Extract the peer ID at this point to give `NegotiationError::PeerIdMismatch` a chance to
        // propagate.
        let peer_id = match address.iter().last() {
            Some(Protocol::P2p(hash)) => PeerId::from_multihash(hash).ok(),
            _ => None,
        };
        let Some(peer_id) = peer_id else {
            return;
        };

        // We need a valid context for this peer to keep track of failed addresses.
        let context = peers.entry(peer_id).or_default();
        context.addresses.insert(AddressRecord::new(&peer_id, address.clone(), score));
    }

    /// Handle dial failure.
    ///
    /// The main purpose of this function is to advance the internal `PeerState`.
    fn on_dial_failure(&mut self, connection_id: ConnectionId) -> crate::Result<()> {
        tracing::trace!(target: LOG_TARGET, ?connection_id, "on dial failure");

        let peer = self.pending_connections.remove(&connection_id).ok_or_else(|| {
            tracing::error!(
                target: LOG_TARGET,
                ?connection_id,
                "dial failed for a connection that doesn't exist",
            );
            Error::InvalidState
        })?;

        let mut peers = self.peers.write();
        let context = peers.entry(peer).or_default();
        let previous_state = context.state.clone();

        if !context.state.on_dial_failure(connection_id) {
            tracing::warn!(
                target: LOG_TARGET,
                ?peer,
                ?connection_id,
                state = ?context.state,
                "invalid state for dial failure",
            );
        } else {
            tracing::trace!(
                target: LOG_TARGET,
                ?peer,
                ?connection_id,
                ?previous_state,
                state = ?context.state,
                "on dial failure completed"
            );
        }

        Ok(())
    }

    fn on_pending_incoming_connection(&mut self) -> crate::Result<()> {
        self.connection_limits.on_incoming()?;
        Ok(())
    }

    /// Handle closed connection.
    fn on_connection_closed(
        &mut self,
        peer: PeerId,
        connection_id: ConnectionId,
    ) -> Option<TransportEvent> {
        tracing::trace!(target: LOG_TARGET, ?peer, ?connection_id, "connection closed");

        self.connection_limits.on_connection_closed(connection_id);

        let mut peers = self.peers.write();
        let context = peers.entry(peer).or_default();

        let previous_state = context.state.clone();
        let connection_closed = context.state.on_connection_closed(connection_id);

        if context.state == previous_state {
            tracing::warn!(
                target: LOG_TARGET,
                ?peer,
                ?connection_id,
                state = ?context.state,
                "invalid state for a closed connection",
            );
        } else {
            tracing::trace!(
                target: LOG_TARGET,
                ?peer,
                ?connection_id,
                ?previous_state,
                state = ?context.state,
                "on connection closed completed"
            );
        }

        connection_closed.then_some(TransportEvent::ConnectionClosed {
            peer,
            connection_id,
        })
    }

    /// Update the address on a connection established.
    fn update_address_on_connection_established(&mut self, peer: PeerId, endpoint: &Endpoint) {
        // The connection can be inbound or outbound.
        // For the inbound connection type, in most cases, the remote peer dialed
        // with an ephemeral port which it might not be listening on.
        // Therefore, we only insert the address into the store if we're the dialer.
        if endpoint.is_listener() {
            return;
        }

        let mut peers = self.peers.write();

        let record = AddressRecord::new(
            &peer,
            endpoint.address().clone(),
            scores::CONNECTION_ESTABLISHED,
        );

        let context = peers.entry(peer).or_default();
        context.addresses.insert(record);
    }

    fn on_connection_established(
        &mut self,
        peer: PeerId,
        endpoint: &Endpoint,
    ) -> crate::Result<ConnectionEstablishedResult> {
        self.update_address_on_connection_established(peer, endpoint);

        if let Some(dialed_peer) = self.pending_connections.remove(&endpoint.connection_id()) {
            if dialed_peer != peer {
                tracing::warn!(
                    target: LOG_TARGET,
                    ?dialed_peer,
                    ?peer,
                    ?endpoint,
                    "peer ids do not match but transport was supposed to reject connection"
                );
                debug_assert!(false);
                return Err(Error::InvalidState);
            }
        };

        // Reject the connection if exceeded limits.
        if let Err(error) = self.connection_limits.can_accept_connection(endpoint.is_listener()) {
            tracing::debug!(
                target: LOG_TARGET,
                ?peer,
                ?endpoint,
                ?error,
                "connection limit exceeded, rejecting connection",
            );
            return Ok(ConnectionEstablishedResult::Reject);
        }

        let mut peers = self.peers.write();
        let context = peers.entry(peer).or_default();

        let previous_state = context.state.clone();
        let connection_accepted = context
            .state
            .on_connection_established(ConnectionRecord::from_endpoint(peer, endpoint));

        tracing::trace!(
            target: LOG_TARGET,
            ?peer,
            ?endpoint,
            ?previous_state,
            state = ?context.state,
            "on connection established completed"
        );

        if connection_accepted {
            self.connection_limits
                .accept_established_connection(endpoint.connection_id(), endpoint.is_listener());

            // Cancel all pending dials if the connection was established.
            if let PeerState::Opening {
                connection_id,
                transports,
                ..
            } = previous_state
            {
                // cancel all pending dials
                transports.iter().for_each(|transport| {
                    self.transports
                        .get_mut(transport)
                        .expect("transport to exist")
                        .cancel(connection_id);
                });

                // since an inbound connection was removed, the outbound connection can be
                // removed from pending dials
                //
                // This may race in the following scenario:
                //
                // T0: we open address X on protocol TCP
                // T1: remote peer opens a connection with us
                // T2: address X is dialed and event is propagated from TCP to transport manager
                // T3: `on_connection_established` is called for T1 and pending connections cleared
                // T4: event from T2 is delivered.
                //
                // TODO: see https://github.com/paritytech/litep2p/issues/276 for more details.
                self.pending_connections.remove(&connection_id);
            }

            return Ok(ConnectionEstablishedResult::Accept);
        }

        Ok(ConnectionEstablishedResult::Reject)
    }

    fn on_connection_opened(
        &mut self,
        transport: SupportedTransport,
        connection_id: ConnectionId,
        address: Multiaddr,
    ) -> crate::Result<()> {
        let Some(peer) = self.pending_connections.remove(&connection_id) else {
            tracing::warn!(
                target: LOG_TARGET,
                ?connection_id,
                ?transport,
                ?address,
                "connection opened but dial record doesn't exist",
            );

            debug_assert!(false);
            return Err(Error::InvalidState);
        };

        let mut peers = self.peers.write();
        let context = peers.entry(peer).or_default();

        // Keep track of the address.
        context.addresses.insert(AddressRecord::new(
            &peer,
            address.clone(),
            scores::CONNECTION_ESTABLISHED,
        ));

        let previous_state = context.state.clone();
        let record = ConnectionRecord::new(peer, address.clone(), connection_id);
        let state_advanced = context.state.on_connection_opened(record);
        if !state_advanced {
            tracing::warn!(
                target: LOG_TARGET,
                ?peer,
                ?connection_id,
                state = ?context.state,
                "connection opened but `PeerState` is not `Opening`",
            );
            return Err(Error::InvalidState);
        }

        // State advanced from `Opening` to `Dialing`.
        let PeerState::Opening {
            connection_id,
            transports,
            ..
        } = previous_state
        else {
            tracing::warn!(
                target: LOG_TARGET,
                ?peer,
                ?connection_id,
                state = ?context.state,
                "State mismatch in opening expected by peer state transition",
            );
            return Err(Error::InvalidState);
        };

        // Cancel open attempts for other transports as connection already exists.
        for transport in transports.iter() {
            self.transports
                .get_mut(transport)
                .expect("transport to exist")
                .cancel(connection_id);
        }

        let negotiation = self
            .transports
            .get_mut(&transport)
            .expect("transport to exist")
            .negotiate(connection_id);

        match negotiation {
            Ok(()) => {
                tracing::trace!(
                    target: LOG_TARGET,
                    ?peer,
                    ?connection_id,
                    ?transport,
                    "negotiation started"
                );

                self.pending_connections.insert(connection_id, peer);

                Ok(())
            }
            Err(err) => {
                tracing::warn!(
                    target: LOG_TARGET,
                    ?peer,
                    ?connection_id,
                    ?err,
                    "failed to negotiate connection",
                );
                context.state = PeerState::Disconnected { dial_record: None };
                Err(Error::InvalidState)
            }
        }
    }

    /// Handle open failure for dialing attempt for `transport`
    fn on_open_failure(
        &mut self,
        transport: SupportedTransport,
        connection_id: ConnectionId,
    ) -> crate::Result<Option<PeerId>> {
        let Some(peer) = self.pending_connections.get(&connection_id).copied() else {
            tracing::warn!(
                target: LOG_TARGET,
                ?connection_id,
                "open failure but dial record doesn't exist",
            );
            return Err(Error::InvalidState);
        };

        let mut peers = self.peers.write();
        let context = peers.entry(peer).or_default();

        let previous_state = context.state.clone();
        let last_transport = context.state.on_open_failure(transport);

        if context.state == previous_state {
            tracing::warn!(
                target: LOG_TARGET,
                ?peer,
                ?connection_id,
                ?transport,
                state = ?context.state,
                "invalid state for a open failure",
            );

            return Err(Error::InvalidState);
        }

        tracing::trace!(
            target: LOG_TARGET,
            ?peer,
            ?connection_id,
            ?transport,
            ?previous_state,
            state = ?context.state,
            "on open failure transition completed"
        );

        if last_transport {
            tracing::trace!(target: LOG_TARGET, ?peer, ?connection_id, "open failure for last transport");
            // Remove the pending connection.
            self.pending_connections.remove(&connection_id);
            // Provide the peer to notify the open failure.
            return Ok(Some(peer));
        }

        Ok(None)
    }

    /// Poll next event from [`crate::transport::manager::TransportManager`].
    pub async fn next(&mut self) -> Option<TransportEvent> {
        loop {
            tokio::select! {
                event = self.event_rx.recv() => {
                    let Some(event) = event else {
                        tracing::error!(
                            target: LOG_TARGET,
                            "Installed protocols terminated, ignore if the node is stopping"
                        );

                        return None;
                    };

                    match event {
                        TransportManagerEvent::ConnectionClosed {
                            peer,
                            connection: connection_id,
                        } => if let Some(event) = self.on_connection_closed(peer, connection_id) {
                            return Some(event);
                        }
                    };
                },

                command = self.cmd_rx.recv() =>{
                    let Some(command) = command else {
                        tracing::error!(
                            target: LOG_TARGET,
                            "User command terminated, ignore if the node is stopping"
                        );

                        return None;
                    };

                    match command {
                        InnerTransportManagerCommand::DialPeer { peer } => {
                            if let Err(error) = self.dial(peer).await {
                                tracing::debug!(target: LOG_TARGET, ?peer, ?error, "failed to dial peer")
                            }
                        }
                        InnerTransportManagerCommand::DialAddress { address } => {
                            if let Err(error) = self.dial_address(address).await {
                                tracing::debug!(target: LOG_TARGET, ?error, "failed to dial peer")
                            }
                        }
                        InnerTransportManagerCommand::UnregisterProtocol { protocol } => {
                            self.unregister_protocol(protocol);
                        }
                    }
                },

                event = self.transports.next() => {
                    let Some((transport, event)) = event else {
                        tracing::error!(
                            target: LOG_TARGET,
                            "Installed transports terminated, ignore if the node is stopping"
                        );

                        return None;
                    };


                    match event {
                        TransportEvent::DialFailure { connection_id, address, error } => {
                            tracing::debug!(
                                target: LOG_TARGET,
                                ?connection_id,
                                ?address,
                                ?error,
                                "failed to dial peer",
                            );

                            // Update the addresses on dial failure regardless of the
                            // internal peer context state. This ensures a robust address tracking
                            // while taking into account the error type.
                            self.update_address_on_dial_failure(address.clone(), &error);

                            if let Ok(()) = self.on_dial_failure(connection_id) {
                                match address.iter().last() {
                                    Some(Protocol::P2p(hash)) => match PeerId::from_multihash(hash) {
                                        Ok(peer) => {
                                            tracing::trace!(
                                                target: LOG_TARGET,
                                                ?connection_id,
                                                ?error,
                                                ?address,
                                                num_protocols = self.protocols.len(),
                                                "dial failure, notify protocols",
                                            );

                                            for (protocol, context) in &self.protocols {
                                                tracing::trace!(
                                                    target: LOG_TARGET,
                                                    ?connection_id,
                                                    ?error,
                                                    ?address,
                                                    ?protocol,
                                                    "dial failure, notify protocol",
                                                );
                                                match context.tx.try_send(InnerTransportEvent::DialFailure {
                                                    peer,
                                                    addresses: vec![address.clone()],
                                                }) {
                                                    Ok(()) => {}
                                                    Err(_) => {
                                                        tracing::trace!(
                                                            target: LOG_TARGET,
                                                            ?connection_id,
                                                            ?error,
                                                            ?address,
                                                            ?protocol,
                                                            "dial failure, channel to protocol clogged, use await",
                                                        );
                                                        let _ = context
                                                            .tx
                                                            .send(InnerTransportEvent::DialFailure {
                                                                peer,
                                                                addresses: vec![address.clone()],
                                                            })
                                                            .await;
                                                    }
                                                }
                                            }

                                            tracing::trace!(
                                                target: LOG_TARGET,
                                                ?connection_id,
                                                ?error,
                                                ?address,
                                                "all protocols notified",
                                            );
                                        }
                                        Err(error) => {
                                            tracing::warn!(
                                                target: LOG_TARGET,
                                                ?address,
                                                ?connection_id,
                                                ?error,
                                                "failed to parse `PeerId` from `Multiaddr`",
                                            );
                                            debug_assert!(false);
                                        }
                                    },
                                    _ => {
                                        tracing::warn!(target: LOG_TARGET, ?address, ?connection_id, "address doesn't contain `PeerId`");
                                        debug_assert!(false);
                                    }
                                }

                                return Some(TransportEvent::DialFailure {
                                    connection_id,
                                    address,
                                    error,
                                })
                            }
                        }
                        TransportEvent::ConnectionEstablished { peer, endpoint } => {
                            self.opening_errors.remove(&endpoint.connection_id());

                            match self.on_connection_established(peer, &endpoint) {
                                Err(error) => {
                                    tracing::debug!(
                                        target: LOG_TARGET,
                                        ?peer,
                                        ?endpoint,
                                        ?error,
                                        "failed to handle established connection",
                                    );

                                    let _ = self
                                        .transports
                                        .get_mut(&transport)
                                        .expect("transport to exist")
                                        .reject(endpoint.connection_id());
                                }
                                Ok(ConnectionEstablishedResult::Accept) => {
                                    tracing::trace!(
                                        target: LOG_TARGET,
                                        ?peer,
                                        ?endpoint,
                                        "accept connection",
                                    );

                                    let _ = self
                                        .transports
                                        .get_mut(&transport)
                                        .expect("transport to exist")
                                        .accept(endpoint.connection_id());

                                    return Some(TransportEvent::ConnectionEstablished {
                                        peer,
                                        endpoint,
                                    });
                                }
                                Ok(ConnectionEstablishedResult::Reject) => {
                                    tracing::trace!(
                                        target: LOG_TARGET,
                                        ?peer,
                                        ?endpoint,
                                        "reject connection",
                                    );

                                    let _ = self
                                        .transports
                                        .get_mut(&transport)
                                        .expect("transport to exist")
                                        .reject(endpoint.connection_id());
                                }
                            }
                        }
                        TransportEvent::ConnectionOpened { connection_id, address } => {
                            self.opening_errors.remove(&connection_id);

                            if let Err(error) = self.on_connection_opened(transport, connection_id, address) {
                                tracing::debug!(
                                    target: LOG_TARGET,
                                    ?connection_id,
                                    ?error,
                                    "failed to handle opened connection",
                                );
                            }
                        }
                        TransportEvent::OpenFailure { connection_id, errors } => {
                            for (address, error) in &errors {
                                self.update_address_on_dial_failure(address.clone(), error);
                            }

                            match self.on_open_failure(transport, connection_id) {
                                Err(error) => tracing::debug!(
                                    target: LOG_TARGET,
                                    ?connection_id,
                                    ?error,
                                    "failed to handle opened connection",
                                ),
                                Ok(Some(peer)) => {
                                    tracing::trace!(
                                        target: LOG_TARGET,
                                        ?peer,
                                        ?connection_id,
                                        num_protocols = self.protocols.len(),
                                        "inform protocols about open failure",
                                    );

                                    let addresses = errors
                                        .iter()
                                        .map(|(address, _)| address.clone())
                                        .collect::<Vec<_>>();

                                    for (protocol, context) in &self.protocols {
                                        let _ = match context
                                            .tx
                                            .try_send(InnerTransportEvent::DialFailure {
                                                peer,
                                                addresses: addresses.clone(),
                                            }) {
                                            Ok(_) => Ok(()),
                                            Err(_) => {
                                                tracing::trace!(
                                                    target: LOG_TARGET,
                                                    ?peer,
                                                    %protocol,
                                                    ?connection_id,
                                                    "call to protocol would block try sending in a blocking way",
                                                );

                                                context
                                                    .tx
                                                    .send(InnerTransportEvent::DialFailure {
                                                        peer,
                                                        addresses: addresses.clone(),
                                                    })
                                                    .await
                                            }
                                        };
                                    }

                                    let mut grouped_errors = self.opening_errors.remove(&connection_id).unwrap_or_default();
                                    grouped_errors.extend(errors);
                                    return Some(TransportEvent::OpenFailure { connection_id, errors: grouped_errors });
                                }
                                Ok(None) => {
                                    tracing::trace!(
                                        target: LOG_TARGET,
                                        ?connection_id,
                                        "open failure, but not the last transport",
                                    );

                                    self.opening_errors.entry(connection_id).or_default().extend(errors);
                                }
                            }
                        },
                        TransportEvent::PendingInboundConnection { connection_id } => {
                            if self.on_pending_incoming_connection().is_ok() {
                                tracing::trace!(
                                    target: LOG_TARGET,
                                    ?connection_id,
                                    "accept pending incoming connection",
                                );

                                let _ = self
                                    .transports
                                    .get_mut(&transport)
                                    .expect("transport to exist")
                                    .accept_pending(connection_id);
                            } else {
                                tracing::debug!(
                                    target: LOG_TARGET,
                                    ?connection_id,
                                    "reject pending incoming connection",
                                );

                                let _ = self
                                    .transports
                                    .get_mut(&transport)
                                    .expect("transport to exist")
                                    .reject_pending(connection_id);
                            }
                        },
                        event => panic!("event not supported: {event:?}"),
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::transport::manager::{address::AddressStore, peer_state::SecondaryOrDialing};
    use limits::ConnectionLimitsConfig;

    use multihash::Multihash;

    use super::*;
    use crate::{
        crypto::ed25519::Keypair,
        executor::DefaultExecutor,
        transport::{dummy::DummyTransport, KEEP_ALIVE_TIMEOUT},
    };
    #[cfg(feature = "websocket")]
    use std::borrow::Cow;
    use std::{
        net::{Ipv4Addr, Ipv6Addr},
        sync::Arc,
        usize,
    };

    /// Setup TCP address and connection id.
    fn setup_dial_addr(peer: PeerId, connection_id: u16) -> (Multiaddr, ConnectionId) {
        let dial_address = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Tcp(8888 + connection_id))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));
        let connection_id = ConnectionId::from(connection_id as usize);

        (dial_address, connection_id)
    }

    #[tokio::test]
    #[cfg(feature = "websocket")]
    #[cfg(feature = "quic")]
    async fn transport_events() {
        struct MockTransport {
            rx: tokio::sync::mpsc::Receiver<TransportEvent>,
        }

        impl MockTransport {
            fn new(rx: tokio::sync::mpsc::Receiver<TransportEvent>) -> Self {
                Self { rx }
            }
        }

        impl Transport for MockTransport {
            fn dial(
                &mut self,
                _connection_id: ConnectionId,
                _address: Multiaddr,
            ) -> crate::Result<()> {
                Ok(())
            }

            fn accept(&mut self, _connection_id: ConnectionId) -> crate::Result<()> {
                Ok(())
            }

            fn accept_pending(&mut self, _connection_id: ConnectionId) -> crate::Result<()> {
                Ok(())
            }

            fn reject_pending(&mut self, _connection_id: ConnectionId) -> crate::Result<()> {
                Ok(())
            }

            fn reject(&mut self, _connection_id: ConnectionId) -> crate::Result<()> {
                Ok(())
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

        impl Stream for MockTransport {
            type Item = TransportEvent;
            fn poll_next(
                mut self: Pin<&mut Self>,
                cx: &mut Context<'_>,
            ) -> Poll<Option<Self::Item>> {
                self.rx.poll_recv(cx)
            }
        }

        let mut transports = TransportContext::new();

        let (tx_tcp, rx) = tokio::sync::mpsc::channel(8);
        let transport = MockTransport::new(rx);
        transports.register_transport(SupportedTransport::Tcp, Box::new(transport));

        let (tx_ws, rx) = tokio::sync::mpsc::channel(8);
        let transport = MockTransport::new(rx);
        transports.register_transport(SupportedTransport::WebSocket, Box::new(transport));

        let (tx_quic, rx) = tokio::sync::mpsc::channel(8);
        let transport = MockTransport::new(rx);
        transports.register_transport(SupportedTransport::Quic, Box::new(transport));

        assert_eq!(transports.index, 0);
        assert_eq!(transports.transports.len(), 3);
        // No items.
        futures::future::poll_fn(|cx| match transports.poll_next_unpin(cx) {
            std::task::Poll::Ready(_) => panic!("didn't expect event from `TransportService`"),
            std::task::Poll::Pending => std::task::Poll::Ready(()),
        })
        .await;
        assert_eq!(transports.index, 0);

        // Websocket events.
        tx_ws
            .send(TransportEvent::PendingInboundConnection {
                connection_id: ConnectionId::from(1),
            })
            .await
            .expect("channel to be open");

        let event = futures::future::poll_fn(|cx| transports.poll_next_unpin(cx))
            .await
            .expect("expected event");
        assert_eq!(event.0, SupportedTransport::WebSocket);
        assert!(std::matches!(
            event.1,
            TransportEvent::PendingInboundConnection { .. }
        ));
        assert_eq!(transports.index, 2);

        // TCP events.
        tx_tcp
            .send(TransportEvent::PendingInboundConnection {
                connection_id: ConnectionId::from(2),
            })
            .await
            .expect("channel to be open");

        let event = futures::future::poll_fn(|cx| transports.poll_next_unpin(cx))
            .await
            .expect("expected event");
        assert_eq!(event.0, SupportedTransport::Tcp);
        assert!(std::matches!(
            event.1,
            TransportEvent::PendingInboundConnection { .. }
        ));
        assert_eq!(transports.index, 1);

        // QUIC events
        tx_quic
            .send(TransportEvent::PendingInboundConnection {
                connection_id: ConnectionId::from(3),
            })
            .await
            .expect("channel to be open");

        let event = futures::future::poll_fn(|cx| transports.poll_next_unpin(cx))
            .await
            .expect("expected event");
        assert_eq!(event.0, SupportedTransport::Quic);
        assert!(std::matches!(
            event.1,
            TransportEvent::PendingInboundConnection { .. }
        ));
        assert_eq!(transports.index, 0);

        // All three transports produce events.
        tx_ws
            .send(TransportEvent::PendingInboundConnection {
                connection_id: ConnectionId::from(4),
            })
            .await
            .expect("channel to be open");
        tx_tcp
            .send(TransportEvent::PendingInboundConnection {
                connection_id: ConnectionId::from(5),
            })
            .await
            .expect("channel to be open");
        tx_quic
            .send(TransportEvent::PendingInboundConnection {
                connection_id: ConnectionId::from(6),
            })
            .await
            .expect("channel to be open");

        let event = futures::future::poll_fn(|cx| transports.poll_next_unpin(cx))
            .await
            .expect("expected event");
        assert_eq!(event.0, SupportedTransport::Tcp);
        assert!(std::matches!(
            event.1,
            TransportEvent::PendingInboundConnection { .. }
        ));
        assert_eq!(transports.index, 1);

        let event = futures::future::poll_fn(|cx| transports.poll_next_unpin(cx))
            .await
            .expect("expected event");
        assert_eq!(event.0, SupportedTransport::WebSocket);
        assert!(std::matches!(
            event.1,
            TransportEvent::PendingInboundConnection { .. }
        ));
        assert_eq!(transports.index, 2);

        let event = futures::future::poll_fn(|cx| transports.poll_next_unpin(cx))
            .await
            .expect("expected event");
        assert_eq!(event.0, SupportedTransport::Quic);
        assert!(std::matches!(
            event.1,
            TransportEvent::PendingInboundConnection { .. }
        ));
        assert_eq!(transports.index, 0);
    }

    #[test]
    #[should_panic]
    #[cfg(debug_assertions)]
    fn duplicate_protocol() {
        let mut manager = TransportManagerBuilder::new().build();

        manager.register_protocol(
            ProtocolName::from("/notif/1"),
            Vec::new(),
            ProtocolCodec::UnsignedVarint(None),
            KEEP_ALIVE_TIMEOUT,
        );
        manager.register_protocol(
            ProtocolName::from("/notif/1"),
            Vec::new(),
            ProtocolCodec::UnsignedVarint(None),
            KEEP_ALIVE_TIMEOUT,
        );
    }

    #[test]
    #[should_panic]
    #[cfg(debug_assertions)]
    fn fallback_protocol_as_duplicate_main_protocol() {
        let mut manager = TransportManagerBuilder::new().build();

        manager.register_protocol(
            ProtocolName::from("/notif/1"),
            Vec::new(),
            ProtocolCodec::UnsignedVarint(None),
            KEEP_ALIVE_TIMEOUT,
        );
        manager.register_protocol(
            ProtocolName::from("/notif/2"),
            vec![
                ProtocolName::from("/notif/2/new"),
                ProtocolName::from("/notif/1"),
            ],
            ProtocolCodec::UnsignedVarint(None),
            KEEP_ALIVE_TIMEOUT,
        );
    }

    #[test]
    #[should_panic]
    #[cfg(debug_assertions)]
    fn duplicate_fallback_protocol() {
        let mut manager = TransportManagerBuilder::new().build();

        manager.register_protocol(
            ProtocolName::from("/notif/1"),
            vec![
                ProtocolName::from("/notif/1/new"),
                ProtocolName::from("/notif/1"),
            ],
            ProtocolCodec::UnsignedVarint(None),
            KEEP_ALIVE_TIMEOUT,
        );
        manager.register_protocol(
            ProtocolName::from("/notif/2"),
            vec![
                ProtocolName::from("/notif/2/new"),
                ProtocolName::from("/notif/1/new"),
            ],
            ProtocolCodec::UnsignedVarint(None),
            KEEP_ALIVE_TIMEOUT,
        );
    }

    #[test]
    #[should_panic]
    #[cfg(debug_assertions)]
    fn duplicate_transport() {
        let mut manager = TransportManagerBuilder::new().build();

        manager.register_transport(SupportedTransport::Tcp, Box::new(DummyTransport::new()));
        manager.register_transport(SupportedTransport::Tcp, Box::new(DummyTransport::new()));
    }

    #[tokio::test]
    async fn tried_to_self_using_peer_id() {
        let keypair = Keypair::generate();
        let local_peer_id = PeerId::from_public_key(&keypair.public().into());
        let mut manager = TransportManagerBuilder::new().with_keypair(keypair).build();

        assert!(manager.dial(local_peer_id).await.is_err());
    }

    #[tokio::test]
    async fn try_to_dial_over_disabled_transport() {
        let mut manager = TransportManagerBuilder::new().build();
        let _handle = manager.transport_handle(Arc::new(DefaultExecutor {}));
        manager.register_transport(SupportedTransport::Tcp, Box::new(DummyTransport::new()));

        let address = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Udp(8888))
            .with(Protocol::QuicV1)
            .with(Protocol::P2p(
                Multihash::from_bytes(&PeerId::random().to_bytes()).unwrap(),
            ));

        assert!(std::matches!(
            manager.dial_address(address).await,
            Err(Error::TransportNotSupported(_))
        ));
    }

    #[tokio::test]
    async fn successful_dial_reported_to_transport_manager() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        let peer = PeerId::random();
        let dial_address = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));

        let transport = Box::new({
            let mut transport = DummyTransport::new();
            transport.inject_event(TransportEvent::ConnectionEstablished {
                peer,
                endpoint: Endpoint::dialer(dial_address.clone(), ConnectionId::from(0usize)),
            });
            transport
        });
        manager.register_transport(SupportedTransport::Tcp, transport);

        assert!(manager.dial_address(dial_address.clone()).await.is_ok());
        assert!(!manager.pending_connections.is_empty());

        {
            let peers = manager.peers.read();

            match peers.get(&peer) {
                Some(PeerContext {
                    state: PeerState::Dialing { .. },
                    ..
                }) => {}
                state => panic!("invalid state for peer: {state:?}"),
            }
        }

        match manager.next().await.unwrap() {
            TransportEvent::ConnectionEstablished {
                peer: event_peer,
                endpoint: event_endpoint,
                ..
            } => {
                assert_eq!(peer, event_peer);
                assert_eq!(
                    event_endpoint,
                    Endpoint::dialer(dial_address.clone(), ConnectionId::from(0usize))
                )
            }
            event => panic!("invalid event: {event:?}"),
        }
    }

    #[tokio::test]
    async fn try_to_dial_same_peer_twice() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        let _handle = manager.transport_handle(Arc::new(DefaultExecutor {}));
        manager.register_transport(SupportedTransport::Tcp, Box::new(DummyTransport::new()));

        let peer = PeerId::random();
        let dial_address = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));

        assert!(manager.dial_address(dial_address.clone()).await.is_ok());
        assert_eq!(manager.pending_connections.len(), 1);

        assert!(manager.dial_address(dial_address.clone()).await.is_ok());
        assert_eq!(manager.pending_connections.len(), 1);
    }

    #[tokio::test]
    async fn try_to_dial_same_peer_twice_diffrent_address() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        let _handle = manager.transport_handle(Arc::new(DefaultExecutor {}));
        manager.register_transport(SupportedTransport::Tcp, Box::new(DummyTransport::new()));

        let peer = PeerId::random();

        assert!(manager
            .dial_address(
                Multiaddr::empty()
                    .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
                    .with(Protocol::Tcp(8888))
                    .with(Protocol::P2p(
                        Multihash::from_bytes(&peer.to_bytes()).unwrap(),
                    ))
            )
            .await
            .is_ok());
        assert_eq!(manager.pending_connections.len(), 1);

        assert!(manager
            .dial_address(
                Multiaddr::empty()
                    .with(Protocol::Ip6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)))
                    .with(Protocol::Tcp(8888))
                    .with(Protocol::P2p(
                        Multihash::from_bytes(&peer.to_bytes()).unwrap(),
                    ))
            )
            .await
            .is_ok());
        assert_eq!(manager.pending_connections.len(), 1);
    }

    #[tokio::test]
    async fn dial_non_existent_peer() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        let _handle = manager.transport_handle(Arc::new(DefaultExecutor {}));
        manager.register_transport(SupportedTransport::Tcp, Box::new(DummyTransport::new()));

        assert!(manager.dial(PeerId::random()).await.is_err());
    }

    #[tokio::test]
    async fn dial_non_peer_with_no_known_addresses() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        let _handle = manager.transport_handle(Arc::new(DefaultExecutor {}));
        manager.register_transport(SupportedTransport::Tcp, Box::new(DummyTransport::new()));

        let peer = PeerId::random();
        manager.peers.write().insert(
            peer,
            PeerContext {
                state: PeerState::Disconnected { dial_record: None },
                addresses: AddressStore::new(),
            },
        );

        assert!(manager.dial(peer).await.is_err());
    }

    #[tokio::test]
    async fn check_supported_transport_when_adding_known_address() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut transports = HashSet::new();
        transports.insert(SupportedTransport::Tcp);
        #[cfg(feature = "quic")]
        transports.insert(SupportedTransport::Quic);

        let manager = TransportManagerBuilder::new().with_supported_transports(transports).build();

        let handle = manager.transport_manager_handle;

        // ipv6
        let address = Multiaddr::empty()
            .with(Protocol::Ip6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&PeerId::random().to_bytes()).unwrap(),
            ));
        assert!(handle.supported_transport(&address));

        // ipv4
        let address = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&PeerId::random().to_bytes()).unwrap(),
            ));
        assert!(handle.supported_transport(&address));

        // quic
        let address = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Udp(8888))
            .with(Protocol::QuicV1)
            .with(Protocol::P2p(
                Multihash::from_bytes(&PeerId::random().to_bytes()).unwrap(),
            ));
        #[cfg(feature = "quic")]
        assert!(handle.supported_transport(&address));
        #[cfg(not(feature = "quic"))]
        assert!(!handle.supported_transport(&address));

        // websocket
        let address = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::Ws(std::borrow::Cow::Owned("/".to_string())));
        assert!(!handle.supported_transport(&address));

        // websocket secure
        let address = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::Wss(std::borrow::Cow::Owned("/".to_string())));
        assert!(!handle.supported_transport(&address));
    }

    // local node tried to dial a node and it failed but in the mean
    // time the remote node dialed local node and that succeeded.
    #[tokio::test]
    async fn on_dial_failure_already_connected() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        let _handle = manager.transport_handle(Arc::new(DefaultExecutor {}));
        manager.register_transport(SupportedTransport::Tcp, Box::new(DummyTransport::new()));

        let peer = PeerId::random();
        let dial_address = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));
        let connect_address = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(192, 168, 1, 173)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));
        assert!(manager.dial_address(dial_address.clone()).await.is_ok());
        assert_eq!(manager.pending_connections.len(), 1);

        match &manager.peers.read().get(&peer).unwrap().state {
            PeerState::Dialing { dial_record } => {
                assert_eq!(dial_record.address, dial_address);
            }
            state => panic!("invalid state for peer: {state:?}"),
        }

        // remote peer connected to local node from a different address that was dialed
        manager
            .on_connection_established(
                peer,
                &Endpoint::dialer(connect_address, ConnectionId::from(1usize)),
            )
            .unwrap();

        // dialing the peer failed
        manager.on_dial_failure(ConnectionId::from(0usize)).unwrap();

        let peers = manager.peers.read();
        let peer = peers.get(&peer).unwrap();

        match &peer.state {
            PeerState::Connected { secondary, .. } => {
                assert!(secondary.is_none());
                assert!(peer.addresses.addresses.contains_key(&dial_address));
            }
            state => panic!("invalid state: {state:?}"),
        }
    }

    // local node tried to dial a node and it failed but in the mean
    // time the remote node dialed local node and that succeeded.
    //
    // while the dial was still in progresss, the remote node disconnected after which
    // the dial failure was reported.
    #[tokio::test]
    async fn on_dial_failure_already_connected_and_disconnected() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        let _handle = manager.transport_handle(Arc::new(DefaultExecutor {}));
        manager.register_transport(SupportedTransport::Tcp, Box::new(DummyTransport::new()));

        let peer = PeerId::random();
        let dial_address = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));
        let connect_address = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(192, 168, 1, 173)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));
        assert!(manager.dial_address(dial_address.clone()).await.is_ok());
        assert_eq!(manager.pending_connections.len(), 1);

        match &manager.peers.read().get(&peer).unwrap().state {
            PeerState::Dialing { dial_record } => {
                assert_eq!(dial_record.address, dial_address);
            }
            state => panic!("invalid state for peer: {state:?}"),
        }

        // remote peer connected to local node from a different address that was dialed
        manager
            .on_connection_established(
                peer,
                &Endpoint::listener(connect_address, ConnectionId::from(1usize)),
            )
            .unwrap();

        // connection to remote was closed while the dial was still in progress
        manager.on_connection_closed(peer, ConnectionId::from(1usize)).unwrap();

        // verify that the peer state is `Disconnected`
        {
            let peers = manager.peers.read();
            let peer = peers.get(&peer).unwrap();

            match &peer.state {
                PeerState::Disconnected {
                    dial_record: Some(dial_record),
                    ..
                } => {
                    assert_eq!(dial_record.address, dial_address);
                }
                state => panic!("invalid state: {state:?}"),
            }
        }

        // dialing the peer failed
        manager.on_dial_failure(ConnectionId::from(0usize)).unwrap();

        let peers = manager.peers.read();
        let peer = peers.get(&peer).unwrap();

        match &peer.state {
            PeerState::Disconnected {
                dial_record: None, ..
            } => {
                assert!(peer.addresses.addresses.contains_key(&dial_address));
            }
            state => panic!("invalid state: {state:?}"),
        }
    }

    // local node tried to dial a node and it failed but in the mean
    // time the remote node dialed local node and that succeeded.
    //
    // while the dial was still in progresss, the remote node disconnected after which
    // the dial failure was reported.
    #[tokio::test]
    async fn on_dial_success_while_connected_and_disconnected() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        let _handle = manager.transport_handle(Arc::new(DefaultExecutor {}));
        manager.register_transport(SupportedTransport::Tcp, Box::new(DummyTransport::new()));

        let peer = PeerId::random();
        let dial_address = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));
        let connect_address = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(192, 168, 1, 173)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));
        assert!(manager.dial_address(dial_address.clone()).await.is_ok());
        assert_eq!(manager.pending_connections.len(), 1);

        match &manager.peers.read().get(&peer).unwrap().state {
            PeerState::Dialing { dial_record } => {
                assert_eq!(dial_record.address, dial_address);
            }
            state => panic!("invalid state for peer: {state:?}"),
        }

        // remote peer connected to local node from a different address that was dialed
        manager
            .on_connection_established(
                peer,
                &Endpoint::listener(connect_address, ConnectionId::from(1usize)),
            )
            .unwrap();

        // connection to remote was closed while the dial was still in progress
        manager.on_connection_closed(peer, ConnectionId::from(1usize)).unwrap();

        // verify that the peer state is `Disconnected`
        {
            let peers = manager.peers.read();
            let peer = peers.get(&peer).unwrap();

            match &peer.state {
                PeerState::Disconnected {
                    dial_record: Some(dial_record),
                    ..
                } => {
                    assert_eq!(dial_record.address, dial_address);
                }
                state => panic!("invalid state: {state:?}"),
            }
        }

        // the original dial succeeded
        manager
            .on_connection_established(
                peer,
                &Endpoint::dialer(dial_address, ConnectionId::from(0usize)),
            )
            .unwrap();

        let peers = manager.peers.read();
        let peer = peers.get(&peer).unwrap();

        match &peer.state {
            PeerState::Connected {
                secondary: None, ..
            } => {}
            state => panic!("invalid state: {state:?}"),
        }
    }

    #[tokio::test]
    async fn secondary_connection_is_tracked() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        manager.register_transport(SupportedTransport::Tcp, Box::new(DummyTransport::new()));

        let peer = PeerId::random();
        let address1 = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));
        let address2 = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(192, 168, 1, 173)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));
        let address3 = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(192, 168, 10, 64)))
            .with(Protocol::Tcp(9999))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));

        // remote peer connected to local node
        let established_result = manager
            .on_connection_established(
                peer,
                &Endpoint::dialer(address1.clone(), ConnectionId::from(0usize)),
            )
            .unwrap();
        assert_eq!(established_result, ConnectionEstablishedResult::Accept);

        // verify that the peer state is `Connected` with no secondary connection
        {
            let peers = manager.peers.read();
            let peer = peers.get(&peer).unwrap();

            match &peer.state {
                PeerState::Connected {
                    secondary: None, ..
                } => {}
                state => panic!("invalid state: {state:?}"),
            }
        }

        // second connection is established, verify that the secondary connection is tracked
        let established_result = manager
            .on_connection_established(
                peer,
                &Endpoint::listener(address2.clone(), ConnectionId::from(1usize)),
            )
            .unwrap();
        assert_eq!(established_result, ConnectionEstablishedResult::Accept);

        let peers = manager.peers.read();
        let context = peers.get(&peer).unwrap();

        match &context.state {
            PeerState::Connected {
                secondary: Some(SecondaryOrDialing::Secondary(secondary_connection)),
                ..
            } => {
                assert_eq!(secondary_connection.address, address2);
            }
            state => panic!("invalid state: {state:?}"),
        }
        drop(peers);

        // tertiary connection is ignored
        let established_result = manager
            .on_connection_established(
                peer,
                &Endpoint::listener(address3.clone(), ConnectionId::from(2usize)),
            )
            .unwrap();
        assert_eq!(established_result, ConnectionEstablishedResult::Reject);

        let peers = manager.peers.read();
        let peer = peers.get(&peer).unwrap();

        match &peer.state {
            PeerState::Connected {
                secondary: Some(SecondaryOrDialing::Secondary(secondary_connection)),
                ..
            } => {
                assert_eq!(secondary_connection.address, address2);
                // Endpoint::listener addresses are not tracked.
                assert!(!peer.addresses.addresses.contains_key(&address2));
                assert!(!peer.addresses.addresses.contains_key(&address3));
                assert_eq!(
                    peer.addresses.addresses.get(&address1).unwrap().score(),
                    scores::CONNECTION_ESTABLISHED
                );
            }
            state => panic!("invalid state: {state:?}"),
        }
    }
    #[tokio::test]
    async fn secondary_connection_with_different_dial_endpoint_is_rejected() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        manager.register_transport(SupportedTransport::Tcp, Box::new(DummyTransport::new()));

        let peer = PeerId::random();
        let address1 = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));
        let address2 = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(192, 168, 1, 173)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));

        // remote peer connected to local node
        let established_result = manager
            .on_connection_established(
                peer,
                &Endpoint::listener(address1, ConnectionId::from(0usize)),
            )
            .unwrap();
        assert_eq!(established_result, ConnectionEstablishedResult::Accept);

        // verify that the peer state is `Connected` with no secondary connection
        {
            let peers = manager.peers.read();
            let peer = peers.get(&peer).unwrap();

            match &peer.state {
                PeerState::Connected {
                    secondary: None, ..
                } => {}
                state => panic!("invalid state: {state:?}"),
            }
        }

        // Add a dial record for the peer.
        {
            let mut peers = manager.peers.write();
            let peer_context = peers.get_mut(&peer).unwrap();

            let record = match &peer_context.state {
                PeerState::Connected { record, .. } => record.clone(),
                state => panic!("invalid state: {state:?}"),
            };

            let dial_record = ConnectionRecord::new(peer, address2.clone(), ConnectionId::from(0));
            peer_context.state = PeerState::Connected {
                record,
                secondary: Some(SecondaryOrDialing::Dialing(dial_record)),
            };
        }

        // second connection is from a different endpoint should fail.
        let established_result = manager
            .on_connection_established(
                peer,
                &Endpoint::listener(address2.clone(), ConnectionId::from(1usize)),
            )
            .unwrap();
        assert_eq!(established_result, ConnectionEstablishedResult::Reject);

        // Multiple secondary connections should also fail.
        let established_result = manager
            .on_connection_established(
                peer,
                &Endpoint::listener(address2.clone(), ConnectionId::from(1usize)),
            )
            .unwrap();
        assert_eq!(established_result, ConnectionEstablishedResult::Reject);

        // Accept the proper connection ID.
        let established_result = manager
            .on_connection_established(
                peer,
                &Endpoint::listener(address2.clone(), ConnectionId::from(0usize)),
            )
            .unwrap();
        assert_eq!(established_result, ConnectionEstablishedResult::Accept);
    }

    #[tokio::test]
    async fn secondary_connection_closed() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        manager.register_transport(SupportedTransport::Tcp, Box::new(DummyTransport::new()));

        let peer = PeerId::random();
        let address1 = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));
        let address2 = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(192, 168, 1, 173)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));

        // remote peer connected to local node
        let emit_event = manager
            .on_connection_established(
                peer,
                &Endpoint::listener(address1, ConnectionId::from(0usize)),
            )
            .unwrap();
        assert!(std::matches!(
            emit_event,
            ConnectionEstablishedResult::Accept
        ));

        // verify that the peer state is `Connected` with no seconary connection
        {
            let peers = manager.peers.read();
            let peer = peers.get(&peer).unwrap();

            match &peer.state {
                PeerState::Connected {
                    record,
                    secondary: None,
                    ..
                } => {
                    // Primary connection is established.
                    assert_eq!(record.connection_id, ConnectionId::from(0usize));
                }
                state => panic!("invalid state: {state:?}"),
            }
        }

        // second connection is established, verify that the secondary connection is tracked
        let emit_event = manager
            .on_connection_established(
                peer,
                &Endpoint::dialer(address2.clone(), ConnectionId::from(1usize)),
            )
            .unwrap();
        assert!(std::matches!(
            emit_event,
            ConnectionEstablishedResult::Accept
        ));

        let peers = manager.peers.read();
        let context = peers.get(&peer).unwrap();

        match &context.state {
            PeerState::Connected {
                secondary: Some(SecondaryOrDialing::Secondary(secondary_connection)),
                ..
            } => {
                assert_eq!(secondary_connection.address, address2);
            }
            state => panic!("invalid state: {state:?}"),
        }
        drop(peers);

        // close the secondary connection and verify that the peer remains connected
        let emit_event = manager.on_connection_closed(peer, ConnectionId::from(1usize));
        assert!(emit_event.is_none());

        let peers = manager.peers.read();
        let context = peers.get(&peer).unwrap();

        match &context.state {
            PeerState::Connected {
                secondary: None,
                record,
            } => {
                assert!(context.addresses.addresses.contains_key(&address2));
                assert_eq!(
                    context.addresses.addresses.get(&address2).unwrap().score(),
                    scores::CONNECTION_ESTABLISHED
                );
                // Primary remains opened.
                assert_eq!(record.connection_id, ConnectionId::from(0usize));
            }
            state => panic!("invalid state: {state:?}"),
        }
    }

    #[tokio::test]
    async fn switch_to_secondary_connection() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        manager.register_transport(SupportedTransport::Tcp, Box::new(DummyTransport::new()));

        let peer = PeerId::random();
        let address1 = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));
        let address2 = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(192, 168, 1, 173)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));

        // remote peer connected to local node
        let emit_event = manager
            .on_connection_established(
                peer,
                &Endpoint::listener(address1.clone(), ConnectionId::from(0usize)),
            )
            .unwrap();
        assert!(std::matches!(
            emit_event,
            ConnectionEstablishedResult::Accept
        ));

        // verify that the peer state is `Connected` with no secondary connection
        {
            let peers = manager.peers.read();
            let peer = peers.get(&peer).unwrap();

            match &peer.state {
                PeerState::Connected {
                    secondary: None, ..
                } => {}
                state => panic!("invalid state: {state:?}"),
            }
        }

        // second connection is established, verify that the secondary connection is tracked
        let emit_event = manager
            .on_connection_established(
                peer,
                &Endpoint::dialer(address2.clone(), ConnectionId::from(1usize)),
            )
            .unwrap();
        assert!(std::matches!(
            emit_event,
            ConnectionEstablishedResult::Accept
        ));

        let peers = manager.peers.read();
        let context = peers.get(&peer).unwrap();

        match &context.state {
            PeerState::Connected {
                secondary: Some(SecondaryOrDialing::Secondary(secondary_connection)),
                ..
            } => {
                assert_eq!(secondary_connection.address, address2);
            }
            state => panic!("invalid state: {state:?}"),
        }
        drop(peers);

        // close the primary connection and verify that the peer remains connected
        // while the primary connection address is stored in peer addresses
        let emit_event = manager.on_connection_closed(peer, ConnectionId::from(0usize));
        assert!(emit_event.is_none());

        let peers = manager.peers.read();
        let context = peers.get(&peer).unwrap();

        match &context.state {
            PeerState::Connected {
                secondary: None,
                record,
            } => {
                assert!(!context.addresses.addresses.contains_key(&address1));
                assert!(context.addresses.addresses.contains_key(&address2));
                assert_eq!(record.connection_id, ConnectionId::from(1usize));
            }
            state => panic!("invalid state: {state:?}"),
        }
    }

    // two connections already exist and a third was opened which is ignored by
    // `on_connection_established()`, when that connection is closed, verify that
    // it's handled gracefully
    #[tokio::test]
    async fn tertiary_connection_closed() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        manager.register_transport(SupportedTransport::Tcp, Box::new(DummyTransport::new()));

        let peer = PeerId::random();
        let address1 = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));
        let address2 = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(192, 168, 1, 173)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));
        let address3 = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(192, 168, 1, 173)))
            .with(Protocol::Tcp(9999))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));

        // remote peer connected to local node
        let emit_event = manager
            .on_connection_established(
                peer,
                &Endpoint::listener(address1.clone(), ConnectionId::from(0usize)),
            )
            .unwrap();
        assert!(std::matches!(
            emit_event,
            ConnectionEstablishedResult::Accept
        ));

        // The address1 should be ignored because it is an inbound connection
        // initiated from an ephemeral port.
        let peers = manager.peers.read();
        let context = peers.get(&peer).unwrap();
        assert!(!context.addresses.addresses.contains_key(&address1));
        drop(peers);

        // verify that the peer state is `Connected` with no seconary connection
        {
            let peers = manager.peers.read();
            let peer = peers.get(&peer).unwrap();

            match &peer.state {
                PeerState::Connected {
                    secondary: None, ..
                } => {}
                state => panic!("invalid state: {state:?}"),
            }
        }

        // second connection is established, verify that the seconary connection is tracked
        let emit_event = manager
            .on_connection_established(
                peer,
                &Endpoint::dialer(address2.clone(), ConnectionId::from(1usize)),
            )
            .unwrap();
        assert!(std::matches!(
            emit_event,
            ConnectionEstablishedResult::Accept
        ));

        // Ensure we keep track of this address.
        let peers = manager.peers.read();
        let context = peers.get(&peer).unwrap();
        assert!(context.addresses.addresses.contains_key(&address2));
        drop(peers);

        let peers = manager.peers.read();
        let context = peers.get(&peer).unwrap();

        match &context.state {
            PeerState::Connected {
                secondary: Some(SecondaryOrDialing::Secondary(secondary_connection)),
                ..
            } => {
                assert_eq!(secondary_connection.address, address2);
            }
            state => panic!("invalid state: {state:?}"),
        }
        drop(peers);

        // third connection is established, verify that it's discarded
        let emit_event = manager
            .on_connection_established(
                peer,
                &Endpoint::listener(address3.clone(), ConnectionId::from(2usize)),
            )
            .unwrap();
        assert!(std::matches!(
            emit_event,
            ConnectionEstablishedResult::Reject
        ));

        let peers = manager.peers.read();
        let context = peers.get(&peer).unwrap();
        // The tertiary connection should be ignored because it is an inbound connection
        // initiated from an ephemeral port.
        assert!(!context.addresses.addresses.contains_key(&address3));
        drop(peers);

        // close the tertiary connection that was ignored
        let emit_event = manager.on_connection_closed(peer, ConnectionId::from(2usize));
        assert!(emit_event.is_none());

        // verify that the state remains unchanged
        let peers = manager.peers.read();
        let context = peers.get(&peer).unwrap();

        match &context.state {
            PeerState::Connected {
                secondary: Some(SecondaryOrDialing::Secondary(secondary_connection)),
                ..
            } => {
                assert_eq!(secondary_connection.address, address2);
                assert_eq!(
                    context.addresses.addresses.get(&address2).unwrap().score(),
                    scores::CONNECTION_ESTABLISHED
                );
            }
            state => panic!("invalid state: {state:?}"),
        }

        drop(peers);
    }
    #[tokio::test]
    #[cfg(debug_assertions)]
    #[should_panic]
    async fn dial_failure_for_unknow_connection() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();

        manager.on_dial_failure(ConnectionId::random()).unwrap();
    }

    #[tokio::test]
    #[cfg(debug_assertions)]
    #[should_panic]
    async fn connection_closed_for_unknown_peer() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        manager.on_connection_closed(PeerId::random(), ConnectionId::random()).unwrap();
    }

    #[tokio::test]
    #[cfg(debug_assertions)]
    #[should_panic]
    async fn unknown_connection_opened() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        manager
            .on_connection_opened(
                SupportedTransport::Tcp,
                ConnectionId::random(),
                Multiaddr::empty(),
            )
            .unwrap();
    }

    #[tokio::test]
    #[cfg(debug_assertions)]
    #[should_panic]
    async fn connection_opened_for_unknown_peer() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        let connection_id = ConnectionId::random();
        let peer = PeerId::random();

        manager.pending_connections.insert(connection_id, peer);
        manager
            .on_connection_opened(SupportedTransport::Tcp, connection_id, Multiaddr::empty())
            .unwrap();
    }

    #[tokio::test]
    #[cfg(debug_assertions)]
    #[should_panic]
    async fn connection_established_for_wrong_peer() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        let connection_id = ConnectionId::random();
        let peer = PeerId::random();

        manager.pending_connections.insert(connection_id, peer);
        manager
            .on_connection_established(
                PeerId::random(),
                &Endpoint::dialer(Multiaddr::empty(), connection_id),
            )
            .unwrap();
    }

    #[tokio::test]
    #[cfg(debug_assertions)]
    #[should_panic]
    async fn open_failure_unknown_connection() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();

        manager
            .on_open_failure(SupportedTransport::Tcp, ConnectionId::random())
            .unwrap();
    }

    #[tokio::test]
    #[cfg(debug_assertions)]
    #[should_panic]
    async fn open_failure_unknown_peer() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        let connection_id = ConnectionId::random();
        let peer = PeerId::random();

        manager.pending_connections.insert(connection_id, peer);
        manager.on_open_failure(SupportedTransport::Tcp, connection_id).unwrap();
    }

    #[tokio::test]
    async fn no_transports() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();

        assert!(manager.next().await.is_none());
    }

    #[tokio::test]
    async fn dial_already_connected_peer() {
        let mut manager = TransportManagerBuilder::new().build();

        let peer = {
            let peer = PeerId::random();
            let mut peers = manager.peers.write();

            peers.insert(
                peer,
                PeerContext {
                    state: PeerState::Connected {
                        record: ConnectionRecord {
                            address: Multiaddr::empty()
                                .with(Protocol::Ip4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
                                .with(Protocol::Tcp(8888))
                                .with(Protocol::P2p(Multihash::from(peer))),
                            connection_id: ConnectionId::from(0usize),
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

        match manager.dial(peer).await {
            Err(Error::AlreadyConnected) => {}
            _ => panic!("invalid return value"),
        }
    }

    #[tokio::test]
    async fn peer_already_being_dialed() {
        let mut manager = TransportManagerBuilder::new().build();

        let peer = {
            let peer = PeerId::random();
            let mut peers = manager.peers.write();

            peers.insert(
                peer,
                PeerContext {
                    state: PeerState::Dialing {
                        dial_record: ConnectionRecord {
                            address: Multiaddr::empty()
                                .with(Protocol::Ip4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
                                .with(Protocol::Tcp(8888))
                                .with(Protocol::P2p(Multihash::from(peer))),
                            connection_id: ConnectionId::from(0usize),
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

        manager.dial(peer).await.unwrap();

        // Check state is unaltered.
        {
            let peers = manager.peers.read();
            let peer_context = peers.get(&peer).unwrap();

            match &peer_context.state {
                PeerState::Dialing { dial_record } => {
                    assert_eq!(
                        dial_record.address,
                        Multiaddr::empty()
                            .with(Protocol::Ip4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
                            .with(Protocol::Tcp(8888))
                            .with(Protocol::P2p(Multihash::from(peer)))
                    );
                }
                state => panic!("invalid state: {state:?}"),
            }
        }
    }

    #[tokio::test]
    async fn pending_connection_for_disconnected_peer() {
        let mut manager = TransportManagerBuilder::new().build();

        let peer = {
            let peer = PeerId::random();
            let mut peers = manager.peers.write();

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

                    addresses: AddressStore::new(),
                },
            );
            drop(peers);

            peer
        };

        manager.dial(peer).await.unwrap();
    }

    #[tokio::test]
    async fn dial_address_invalid_transport() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();

        // transport doesn't start with ip/dns
        {
            let address = Multiaddr::empty().with(Protocol::P2p(Multihash::from(PeerId::random())));
            match manager.dial_address(address.clone()).await {
                Err(Error::TransportNotSupported(dial_address)) => {
                    assert_eq!(dial_address, address);
                }
                _ => panic!("invalid return value"),
            }
        }

        {
            // upd-based protocol but not quic
            let address = Multiaddr::empty()
                .with(Protocol::Ip4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
                .with(Protocol::Udp(8888))
                .with(Protocol::Utp)
                .with(Protocol::P2p(Multihash::from(PeerId::random())));
            match manager.dial_address(address.clone()).await {
                Err(Error::TransportNotSupported(dial_address)) => {
                    assert_eq!(dial_address, address);
                }
                res => panic!("invalid return value: {res:?}"),
            }
        }

        // not tcp nor udp
        {
            let address = Multiaddr::empty()
                .with(Protocol::Ip4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
                .with(Protocol::Sctp(8888))
                .with(Protocol::P2p(Multihash::from(PeerId::random())));
            match manager.dial_address(address.clone()).await {
                Err(Error::TransportNotSupported(dial_address)) => {
                    assert_eq!(dial_address, address);
                }
                _ => panic!("invalid return value"),
            }
        }

        // random protocol after tcp
        {
            let address = Multiaddr::empty()
                .with(Protocol::Ip4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
                .with(Protocol::Tcp(8888))
                .with(Protocol::Utp)
                .with(Protocol::P2p(Multihash::from(PeerId::random())));
            match manager.dial_address(address.clone()).await {
                Err(Error::TransportNotSupported(dial_address)) => {
                    assert_eq!(dial_address, address);
                }
                _ => panic!("invalid return value"),
            }
        }
    }

    #[tokio::test]
    async fn dial_address_peer_id_missing() {
        let mut manager = TransportManagerBuilder::new().build();

        async fn call_manager(manager: &mut TransportManager, address: Multiaddr) {
            match manager.dial_address(address).await {
                Err(Error::AddressError(AddressError::PeerIdMissing)) => {}
                _ => panic!("invalid return value"),
            }
        }

        {
            call_manager(
                &mut manager,
                Multiaddr::empty()
                    .with(Protocol::Ip4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
                    .with(Protocol::Tcp(8888)),
            )
            .await;
        }

        {
            call_manager(
                &mut manager,
                Multiaddr::empty()
                    .with(Protocol::Ip4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
                    .with(Protocol::Tcp(8888))
                    .with(Protocol::Wss(std::borrow::Cow::Owned("".to_string()))),
            )
            .await;
        }

        {
            call_manager(
                &mut manager,
                Multiaddr::empty()
                    .with(Protocol::Ip4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
                    .with(Protocol::Udp(8888))
                    .with(Protocol::QuicV1),
            )
            .await;
        }
    }

    #[tokio::test]
    async fn inbound_connection_while_dialing() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        let peer = PeerId::random();
        let dial_address = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));

        let connection_id = ConnectionId::random();
        let transport = Box::new({
            let mut transport = DummyTransport::new();
            transport.inject_event(TransportEvent::ConnectionEstablished {
                peer,
                endpoint: Endpoint::listener(dial_address.clone(), connection_id),
            });
            transport
        });
        manager.register_transport(SupportedTransport::Tcp, transport);
        manager.add_known_address(
            peer,
            vec![Multiaddr::empty()
                .with(Protocol::Ip4(Ipv4Addr::new(192, 168, 1, 5)))
                .with(Protocol::Tcp(8888))
                .with(Protocol::P2p(Multihash::from(peer)))]
            .into_iter(),
        );

        assert!(manager.dial(peer).await.is_ok());
        assert!(!manager.pending_connections.is_empty());

        {
            let peers = manager.peers.read();

            match peers.get(&peer) {
                Some(PeerContext {
                    state: PeerState::Opening { .. },
                    ..
                }) => {}
                state => panic!("invalid state for peer: {state:?}"),
            }
        }

        match manager.next().await.unwrap() {
            TransportEvent::ConnectionEstablished {
                peer: event_peer,
                endpoint: event_endpoint,
                ..
            } => {
                assert_eq!(peer, event_peer);
                assert_eq!(
                    event_endpoint,
                    Endpoint::listener(dial_address.clone(), connection_id),
                );
            }
            event => panic!("invalid event: {event:?}"),
        }
        assert!(manager.pending_connections.is_empty());

        let peers = manager.peers.read();
        match peers.get(&peer).unwrap() {
            PeerContext {
                state: PeerState::Connected { record, secondary },
                addresses,
            } => {
                assert!(!addresses.addresses.contains_key(&record.address));
                assert!(secondary.is_none());
                assert_eq!(record.address, dial_address);
                assert_eq!(record.connection_id, connection_id);
            }
            state => panic!("invalid peer state: {state:?}"),
        }
    }

    #[tokio::test]
    async fn inbound_connection_for_same_address_while_dialing() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        let peer = PeerId::random();
        let dial_address = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));

        let connection_id = ConnectionId::random();
        let transport = Box::new({
            let mut transport = DummyTransport::new();
            transport.inject_event(TransportEvent::ConnectionEstablished {
                peer,
                endpoint: Endpoint::listener(dial_address.clone(), connection_id),
            });
            transport
        });
        manager.register_transport(SupportedTransport::Tcp, transport);
        manager.add_known_address(
            peer,
            vec![Multiaddr::empty()
                .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
                .with(Protocol::Tcp(8888))
                .with(Protocol::P2p(Multihash::from(peer)))]
            .into_iter(),
        );

        assert!(manager.dial(peer).await.is_ok());
        assert!(!manager.pending_connections.is_empty());

        {
            let peers = manager.peers.read();

            match peers.get(&peer) {
                Some(PeerContext {
                    state: PeerState::Opening { .. },
                    ..
                }) => {}
                state => panic!("invalid state for peer: {state:?}"),
            }
        }

        match manager.next().await.unwrap() {
            TransportEvent::ConnectionEstablished {
                peer: event_peer,
                endpoint: event_endpoint,
                ..
            } => {
                assert_eq!(peer, event_peer);
                assert_eq!(
                    event_endpoint,
                    Endpoint::listener(dial_address.clone(), connection_id),
                );
            }
            event => panic!("invalid event: {event:?}"),
        }
        assert!(manager.pending_connections.is_empty());

        let peers = manager.peers.read();
        match peers.get(&peer).unwrap() {
            PeerContext {
                state: PeerState::Connected { record, secondary },
                addresses,
            } => {
                // Saved from the dial attempt.
                assert_eq!(addresses.addresses.get(&dial_address).unwrap().score(), 0);

                assert!(secondary.is_none());
                assert_eq!(record.address, dial_address);
                assert_eq!(record.connection_id, connection_id);
            }
            state => panic!("invalid peer state: {state:?}"),
        }
    }

    #[tokio::test]
    async fn manager_limits_incoming_connections() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new()
            .with_connection_limits_config(
                ConnectionLimitsConfig::default()
                    .max_incoming_connections(Some(3))
                    .max_outgoing_connections(Some(2)),
            )
            .build();
        // The connection limit is agnostic of the underlying transports.
        manager.register_transport(SupportedTransport::Tcp, Box::new(DummyTransport::new()));

        let peer = PeerId::random();
        let second_peer = PeerId::random();

        // Setup addresses.
        let (first_addr, first_connection_id) = setup_dial_addr(peer, 0);
        let (second_addr, second_connection_id) = setup_dial_addr(second_peer, 1);
        let (_, third_connection_id) = setup_dial_addr(peer, 2);
        let (_, remote_connection_id) = setup_dial_addr(peer, 3);

        // Peer established the first inbound connection.
        let result = manager
            .on_connection_established(
                peer,
                &Endpoint::listener(first_addr.clone(), first_connection_id),
            )
            .unwrap();
        assert_eq!(result, ConnectionEstablishedResult::Accept);

        // The peer is allowed to dial us a second time.
        let result = manager
            .on_connection_established(
                peer,
                &Endpoint::listener(first_addr.clone(), second_connection_id),
            )
            .unwrap();
        assert_eq!(result, ConnectionEstablishedResult::Accept);

        // Second peer calls us.
        let result = manager
            .on_connection_established(
                second_peer,
                &Endpoint::listener(second_addr.clone(), third_connection_id),
            )
            .unwrap();
        assert_eq!(result, ConnectionEstablishedResult::Accept);

        // Limits of inbound connections are reached.
        let result = manager
            .on_connection_established(
                second_peer,
                &Endpoint::listener(second_addr.clone(), remote_connection_id),
            )
            .unwrap();
        assert_eq!(result, ConnectionEstablishedResult::Reject);

        // Close one connection.
        assert!(manager.on_connection_closed(peer, first_connection_id).is_none());

        // The second peer can establish 2 inbounds now.
        let result = manager
            .on_connection_established(
                second_peer,
                &Endpoint::listener(second_addr.clone(), remote_connection_id),
            )
            .unwrap();
        assert_eq!(result, ConnectionEstablishedResult::Accept);
    }

    #[tokio::test]
    async fn manager_limits_outbound_connections() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new()
            .with_connection_limits_config(
                ConnectionLimitsConfig::default()
                    .max_incoming_connections(Some(3))
                    .max_outgoing_connections(Some(2)),
            )
            .build();
        // The connection limit is agnostic of the underlying transports.
        manager.register_transport(SupportedTransport::Tcp, Box::new(DummyTransport::new()));

        let peer = PeerId::random();
        let second_peer = PeerId::random();
        let third_peer = PeerId::random();

        // Setup addresses.
        let (first_addr, first_connection_id) = setup_dial_addr(peer, 0);
        let (second_addr, second_connection_id) = setup_dial_addr(second_peer, 1);
        let (third_addr, third_connection_id) = setup_dial_addr(third_peer, 2);

        // First dial.
        manager.dial_address(first_addr.clone()).await.unwrap();

        // Second dial.
        manager.dial_address(second_addr.clone()).await.unwrap();

        // Third dial, we have a limit on 2 outbound connections.
        manager.dial_address(third_addr.clone()).await.unwrap();

        let result = manager
            .on_connection_established(
                peer,
                &Endpoint::dialer(first_addr.clone(), first_connection_id),
            )
            .unwrap();

        assert_eq!(result, ConnectionEstablishedResult::Accept);

        let result = manager
            .on_connection_established(
                second_peer,
                &Endpoint::dialer(second_addr.clone(), second_connection_id),
            )
            .unwrap();
        assert_eq!(result, ConnectionEstablishedResult::Accept);

        // We have reached the limit now.
        let result = manager
            .on_connection_established(
                third_peer,
                &Endpoint::dialer(third_addr.clone(), third_connection_id),
            )
            .unwrap();
        assert_eq!(result, ConnectionEstablishedResult::Reject);

        // While we have 2 outbound connections active, any dials will fail immediately.
        // We cannot perform this check for the non negotiated inbound connections yet,
        // since the transport will eagerly accept and negotiate them. This requires
        // a refactor into the transport manager, to not waste resources on
        // negotiating connections that will be rejected.
        let result = manager.dial(peer).await.unwrap_err();
        assert!(std::matches!(
            result,
            Error::ConnectionLimit(limits::ConnectionLimitsError::MaxOutgoingConnectionsExceeded)
        ));
        let result = manager.dial_address(first_addr.clone()).await.unwrap_err();
        assert!(std::matches!(
            result,
            Error::ConnectionLimit(limits::ConnectionLimitsError::MaxOutgoingConnectionsExceeded)
        ));

        // Close one connection.
        assert!(manager.on_connection_closed(peer, first_connection_id).is_some());
        // We can now dial again.
        manager.dial_address(first_addr.clone()).await.unwrap();

        let result = manager
            .on_connection_established(peer, &Endpoint::dialer(first_addr, first_connection_id))
            .unwrap();
        assert_eq!(result, ConnectionEstablishedResult::Accept);
    }

    #[tokio::test]
    async fn reject_unknown_secondary_connections_with_different_connection_ids() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        manager.register_transport(SupportedTransport::Tcp, Box::new(DummyTransport::new()));

        // Random peer ID.
        let peer = PeerId::random();
        let (first_addr, _first_connection_id) = setup_dial_addr(peer, 0);
        let second_connection_id = ConnectionId::from(1);
        let different_connection_id = ConnectionId::from(2);

        // Setup a connected peer with a dial record active.
        {
            let mut peers = manager.peers.write();

            let state = PeerState::Connected {
                record: ConnectionRecord::new(peer, first_addr.clone(), ConnectionId::from(0)),
                secondary: Some(SecondaryOrDialing::Dialing(ConnectionRecord::new(
                    peer,
                    first_addr.clone(),
                    second_connection_id,
                ))),
            };

            let peer_context = PeerContext {
                state,
                addresses: AddressStore::from_iter(vec![first_addr.clone()].into_iter()),
            };

            peers.insert(peer, peer_context);
        }

        // Establish a connection, however the connection ID is different.
        let result = manager
            .on_connection_established(
                peer,
                &Endpoint::dialer(first_addr.clone(), different_connection_id),
            )
            .unwrap();
        assert_eq!(result, ConnectionEstablishedResult::Reject);
    }

    #[tokio::test]
    async fn guard_against_secondary_connections_with_different_connection_ids() {
        // This is the repro case for https://github.com/paritytech/litep2p/issues/172.
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        manager.register_transport(SupportedTransport::Tcp, Box::new(DummyTransport::new()));

        // Random peer ID.
        let peer = PeerId::random();

        let setup_dial_addr = |connection_id: u16| {
            let dial_address = Multiaddr::empty()
                .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
                .with(Protocol::Tcp(8888 + connection_id))
                .with(Protocol::P2p(
                    Multihash::from_bytes(&peer.to_bytes()).unwrap(),
                ));
            let connection_id = ConnectionId::from(connection_id as usize);

            (dial_address, connection_id)
        };

        // Setup addresses.
        let (first_addr, first_connection_id) = setup_dial_addr(0);
        let (second_addr, _second_connection_id) = setup_dial_addr(1);
        let (remote_addr, remote_connection_id) = setup_dial_addr(2);

        // Step 1. Dialing state to peer.
        manager.dial_address(first_addr.clone()).await.unwrap();
        {
            let peers = manager.peers.read();
            let peer_context = peers.get(&peer).unwrap();
            match &peer_context.state {
                PeerState::Dialing { dial_record } => {
                    assert_eq!(dial_record.address, first_addr);
                }
                state => panic!("invalid state: {state:?}"),
            }
        }

        // Step 2. Connection established by the remote peer.
        let result = manager
            .on_connection_established(
                peer,
                &Endpoint::listener(remote_addr.clone(), remote_connection_id),
            )
            .unwrap();
        assert_eq!(result, ConnectionEstablishedResult::Accept);
        {
            let peers = manager.peers.read();
            let peer_context = peers.get(&peer).unwrap();
            match &peer_context.state {
                PeerState::Connected {
                    record,
                    secondary: Some(SecondaryOrDialing::Dialing(dial_record)),
                } => {
                    assert_eq!(record.address, remote_addr);
                    assert_eq!(record.connection_id, remote_connection_id);

                    assert_eq!(dial_record.address, first_addr);
                    assert_eq!(dial_record.connection_id, first_connection_id)
                }
                state => panic!("invalid state: {state:?}"),
            }
        }

        // Step 3. The peer disconnects while we have a dialing in flight.
        let event = manager.on_connection_closed(peer, remote_connection_id).unwrap();
        match event {
            TransportEvent::ConnectionClosed {
                peer: event_peer,
                connection_id: event_connection_id,
            } => {
                assert_eq!(peer, event_peer);
                assert_eq!(event_connection_id, remote_connection_id);
            }
            event => panic!("invalid event: {event:?}"),
        }
        {
            let peers = manager.peers.read();
            let peer_context = peers.get(&peer).unwrap();
            match &peer_context.state {
                PeerState::Disconnected { dial_record } => {
                    let dial_record = dial_record.as_ref().unwrap();
                    assert_eq!(dial_record.address, first_addr);
                    assert_eq!(dial_record.connection_id, first_connection_id);
                }
                state => panic!("invalid state: {state:?}"),
            }
        }

        // Step 4. Dial by the second address and expect to not overwrite the state.
        manager.dial_address(second_addr.clone()).await.unwrap();
        // The state remains unchanged since we already have a dialing in flight.
        {
            let peers = manager.peers.read();
            let peer_context = peers.get(&peer).unwrap();
            match &peer_context.state {
                PeerState::Disconnected { dial_record } => {
                    let dial_record = dial_record.as_ref().unwrap();
                    assert_eq!(dial_record.address, first_addr);
                    assert_eq!(dial_record.connection_id, first_connection_id);
                }
                state => panic!("invalid state: {state:?}"),
            }
        }

        // Step 5. Remote peer reconnects again.
        let result = manager
            .on_connection_established(
                peer,
                &Endpoint::listener(remote_addr.clone(), remote_connection_id),
            )
            .unwrap();
        assert_eq!(result, ConnectionEstablishedResult::Accept);
        {
            let peers = manager.peers.read();
            let peer_context = peers.get(&peer).unwrap();
            match &peer_context.state {
                PeerState::Connected {
                    record,
                    secondary: Some(SecondaryOrDialing::Dialing(dial_record)),
                } => {
                    assert_eq!(record.address, remote_addr);
                    assert_eq!(record.connection_id, remote_connection_id);

                    // We have not overwritten the first dial record in step 4.
                    assert_eq!(dial_record.address, first_addr);
                    assert_eq!(dial_record.connection_id, first_connection_id);
                }
                state => panic!("invalid state: {state:?}"),
            }
        }

        // Step 6. First dial responds.
        let result = manager
            .on_connection_established(
                peer,
                &Endpoint::dialer(first_addr.clone(), first_connection_id),
            )
            .unwrap();
        assert_eq!(result, ConnectionEstablishedResult::Accept);
    }

    #[tokio::test]
    async fn persist_dial_addresses() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        let peer = PeerId::random();
        let dial_address = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));

        let connection_id = ConnectionId::from(0);
        let transport = Box::new({
            let mut transport = DummyTransport::new();
            transport.inject_event(TransportEvent::ConnectionEstablished {
                peer,
                endpoint: Endpoint::listener(dial_address.clone(), connection_id),
            });
            transport
        });
        manager.register_transport(SupportedTransport::Tcp, transport);

        // First dial attempt.
        manager.dial_address(dial_address.clone()).await.unwrap();
        // check the state of the peer.
        {
            let peers = manager.peers.read();
            let peer_context = peers.get(&peer).unwrap();
            match &peer_context.state {
                PeerState::Dialing { dial_record } => {
                    assert_eq!(dial_record.address, dial_address);
                }
                state => panic!("invalid state: {state:?}"),
            }

            // The address is saved for future dials.
            assert_eq!(
                peer_context.addresses.addresses.get(&dial_address).unwrap().score(),
                0
            );
        }

        let second_address = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Tcp(8889))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));

        // Second dial attempt with different address.
        manager.dial_address(second_address.clone()).await.unwrap();
        // check the state of the peer.
        {
            let peers = manager.peers.read();
            let peer_context = peers.get(&peer).unwrap();
            match &peer_context.state {
                // Must still be dialing the first address.
                PeerState::Dialing { dial_record } => {
                    assert_eq!(dial_record.address, dial_address);
                }
                state => panic!("invalid state: {state:?}"),
            }

            // The address is still saved, even if a second dial is not initiated.
            assert_eq!(
                peer_context.addresses.addresses.get(&dial_address).unwrap().score(),
                0
            );
            assert_eq!(
                peer_context.addresses.addresses.get(&second_address).unwrap().score(),
                0
            );
        }
    }

    #[cfg(feature = "websocket")]
    #[tokio::test]
    async fn opening_errors_are_reported() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut manager = TransportManagerBuilder::new().build();
        let peer = PeerId::random();
        let connection_id = ConnectionId::from(0);

        // Setup TCP transport.
        let dial_address_tcp = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Tcp(8888))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));
        let transport = Box::new({
            let mut transport = DummyTransport::new();
            transport.inject_event(TransportEvent::OpenFailure {
                connection_id,
                errors: vec![(dial_address_tcp.clone(), DialError::Timeout)],
            });
            transport
        });
        manager.register_transport(SupportedTransport::Tcp, transport);
        manager.add_known_address(
            peer,
            vec![Multiaddr::empty()
                .with(Protocol::Ip4(Ipv4Addr::new(192, 168, 1, 5)))
                .with(Protocol::Tcp(8888))
                .with(Protocol::P2p(Multihash::from(peer)))]
            .into_iter(),
        );

        // Setup WebSockets transport.
        let dial_address_ws = Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
            .with(Protocol::Tcp(8889))
            .with(Protocol::Ws(Cow::Borrowed("/")))
            .with(Protocol::P2p(
                Multihash::from_bytes(&peer.to_bytes()).unwrap(),
            ));

        let transport = Box::new({
            let mut transport = DummyTransport::new();
            transport.inject_event(TransportEvent::OpenFailure {
                connection_id,
                errors: vec![(dial_address_ws.clone(), DialError::Timeout)],
            });
            transport
        });
        manager.register_transport(SupportedTransport::WebSocket, transport);
        manager.add_known_address(
            peer,
            vec![Multiaddr::empty()
                .with(Protocol::Ip4(Ipv4Addr::new(192, 168, 1, 5)))
                .with(Protocol::Tcp(8889))
                .with(Protocol::Ws(Cow::Borrowed("/")))
                .with(Protocol::P2p(
                    Multihash::from_bytes(&peer.to_bytes()).unwrap(),
                ))]
            .into_iter(),
        );

        // Dial the peer on both transports.
        assert!(manager.dial(peer).await.is_ok());
        assert!(!manager.pending_connections.is_empty());

        {
            let peers = manager.peers.read();

            match peers.get(&peer) {
                Some(PeerContext {
                    state: PeerState::Opening { .. },
                    ..
                }) => {}
                state => panic!("invalid state for peer: {state:?}"),
            }
        }

        match manager.next().await.unwrap() {
            TransportEvent::OpenFailure {
                connection_id,
                errors,
            } => {
                assert_eq!(connection_id, ConnectionId::from(0));
                assert_eq!(errors.len(), 2);
                let tcp = errors.iter().find(|(addr, _)| addr == &dial_address_tcp).unwrap();
                assert!(std::matches!(tcp.1, DialError::Timeout));

                let ws = errors.iter().find(|(addr, _)| addr == &dial_address_ws).unwrap();
                assert!(std::matches!(ws.1, DialError::Timeout));
            }
            event => panic!("invalid event: {event:?}"),
        }
        assert!(manager.pending_connections.is_empty());
        assert!(manager.opening_errors.is_empty());
    }
}
