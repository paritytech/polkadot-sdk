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
    error::{Error, ImmediateDialError, SubstreamError},
    protocol::{connection::ConnectionHandle, InnerTransportEvent, TransportEvent},
    transport::{manager::TransportManagerHandle, Endpoint},
    types::{protocol::ProtocolName, ConnectionId, SubstreamId},
    PeerId, DEFAULT_CHANNEL_SIZE,
};

use futures::{future::BoxFuture, stream::FuturesUnordered, Stream, StreamExt};
use multiaddr::{Multiaddr, Protocol};
use multihash::Multihash;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::transport-service";

/// Connection context for the peer.
///
/// Each peer is allowed to have at most two connections open. The first open connection is the
/// primary connections which the local node uses to open substreams to remote. Secondary connection
/// may be open if local and remote opened connections at the same time.
///
/// Secondary connection may be promoted to a primary connection if the primary connections closes
/// while the secondary connections remains open.
#[derive(Debug)]
struct ConnectionContext {
    /// Primary connection.
    primary: ConnectionHandle,

    /// Secondary connection, if it exists.
    secondary: Option<ConnectionHandle>,
}

impl ConnectionContext {
    /// Create new [`ConnectionContext`].
    fn new(primary: ConnectionHandle) -> Self {
        Self {
            primary,
            secondary: None,
        }
    }

    /// Downgrade connection to non-active which means it will be closed
    /// if there are no substreams open over it.
    fn downgrade(&mut self, connection_id: &ConnectionId) {
        if self.primary.connection_id() == connection_id {
            self.primary.close();
            return;
        }

        if let Some(handle) = &mut self.secondary {
            if handle.connection_id() == connection_id {
                handle.close();
                return;
            }
        }

        tracing::debug!(
            target: LOG_TARGET,
            primary = ?self.primary.connection_id(),
            secondary = ?self.secondary.as_ref().map(|handle| handle.connection_id()),
            ?connection_id,
            "connection doesn't exist, cannot downgrade",
        );
    }

    /// Try to upgrade the connection to active state.
    fn try_upgrade(&mut self, connection_id: &ConnectionId) {
        if self.primary.connection_id() == connection_id {
            self.primary.try_upgrade();
            return;
        }

        if let Some(handle) = &mut self.secondary {
            if handle.connection_id() == connection_id {
                handle.try_upgrade();
                return;
            }
        }

        tracing::debug!(
            target: LOG_TARGET,
            primary = ?self.primary.connection_id(),
            secondary = ?self.secondary.as_ref().map(|handle| handle.connection_id()),
            ?connection_id,
            "connection doesn't exist, cannot upgrade",
        );
    }
}

/// Tracks connection keep-alive timeouts.
///
/// A connection keep-alive timeout is started when a connection is established.
/// If no substreams are opened over the connection within the timeout,
/// the connection is downgraded. However, if a substream is opened over the connection,
/// the timeout is reset.
#[derive(Debug)]
struct KeepAliveTracker {
    /// Close the connection if no substreams are open within this time frame.
    keep_alive_timeout: Duration,

    /// Track substream last activity.
    last_activity: HashMap<(PeerId, ConnectionId), Instant>,

    /// Pending keep-alive timeouts.
    pending_keep_alive_timeouts: FuturesUnordered<BoxFuture<'static, (PeerId, ConnectionId)>>,

    /// Saved waker.
    waker: Option<Waker>,
}

impl KeepAliveTracker {
    /// Create new [`KeepAliveTracker`].
    pub fn new(keep_alive_timeout: Duration) -> Self {
        Self {
            keep_alive_timeout,
            last_activity: HashMap::new(),
            pending_keep_alive_timeouts: FuturesUnordered::new(),
            waker: None,
        }
    }

    /// Called on connection established event to add a new keep-alive timeout.
    pub fn on_connection_established(&mut self, peer: PeerId, connection_id: ConnectionId) {
        self.substream_activity(peer, connection_id);
    }

    /// Called on connection closed event.
    pub fn on_connection_closed(&mut self, peer: PeerId, connection_id: ConnectionId) {
        self.last_activity.remove(&(peer, connection_id));
    }

    /// Called on substream opened event to track the last activity.
    pub fn substream_activity(&mut self, peer: PeerId, connection_id: ConnectionId) {
        // Keep track of the connection ID and the time the substream was opened.
        if self.last_activity.insert((peer, connection_id), Instant::now()).is_none() {
            // Refill futures if there is no pending keep-alive timeout.
            let timeout = self.keep_alive_timeout;
            self.pending_keep_alive_timeouts.push(Box::pin(async move {
                tokio::time::sleep(timeout).await;
                (peer, connection_id)
            }));
        }

        tracing::trace!(
            target: LOG_TARGET,
            ?peer,
            ?connection_id,
            ?self.keep_alive_timeout,
            last_activity = ?self.last_activity.len(),
            pending_keep_alive_timeouts = ?self.pending_keep_alive_timeouts.len(),
            "substream activity",
        );

        // Wake any pending poll.
        if let Some(waker) = self.waker.take() {
            waker.wake()
        }
    }
}

impl Stream for KeepAliveTracker {
    type Item = (PeerId, ConnectionId);

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.pending_keep_alive_timeouts.is_empty() {
            // No pending keep-alive timeouts.
            self.waker = Some(cx.waker().clone());
            return Poll::Pending;
        }

        match self.pending_keep_alive_timeouts.poll_next_unpin(cx) {
            Poll::Ready(Some(key)) => {
                // Check last-activity time.
                let Some(last_activity) = self.last_activity.get(&key) else {
                    tracing::debug!(
                        target: LOG_TARGET,
                        peer = ?key.0,
                        connection_id = ?key.1,
                        "Last activity no longer tracks the connection (closed event triggered)",
                    );

                    // We have effectively ignored this `Poll::Ready` event. To prevent the
                    // future from getting stuck, we need to tell the executor to poll again
                    // for more events.
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                };

                // Keep-alive timeout not reached yet.
                let inactive_for = last_activity.elapsed();
                if inactive_for < self.keep_alive_timeout {
                    let timeout = self.keep_alive_timeout.saturating_sub(inactive_for);

                    tracing::trace!(
                        target: LOG_TARGET,
                        peer = ?key.0,
                        connection_id = ?key.1,
                        ?timeout,
                        "keep-alive timeout not yet reached",
                    );

                    // Refill the keep alive timeouts.
                    self.pending_keep_alive_timeouts.push(Box::pin(async move {
                        tokio::time::sleep(timeout).await;
                        key
                    }));

                    // This is similar to the `last_activity` check above, we need to inform
                    // the executor that this object may produce more events.
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }

                // Keep-alive timeout reached.
                tracing::debug!(
                    target: LOG_TARGET,
                    peer = ?key.0,
                    connection_id = ?key.1,
                    "keep-alive timeout triggered",
                );
                self.last_activity.remove(&key);
                Poll::Ready(Some(key))
            }
            Poll::Ready(None) | Poll::Pending => Poll::Pending,
        }
    }
}

/// Provides an interfaces for [`Litep2p`](crate::Litep2p) protocols to interact
/// with the underlying transport protocols.
#[derive(Debug)]
pub struct TransportService {
    /// Local peer ID.
    local_peer_id: PeerId,

    /// Protocol.
    protocol: ProtocolName,

    /// Fallback names for the protocol.
    fallback_names: Vec<ProtocolName>,

    /// Open connections.
    connections: HashMap<PeerId, ConnectionContext>,

    /// Transport handle.
    transport_handle: TransportManagerHandle,

    /// RX channel for receiving events from tranports and connections.
    rx: Receiver<InnerTransportEvent>,

    /// Next substream ID.
    next_substream_id: Arc<AtomicUsize>,

    /// Close the connection if no substreams are open within this time frame.
    keep_alive_tracker: KeepAliveTracker,
}

impl TransportService {
    /// Create new [`TransportService`].
    pub(crate) fn new(
        local_peer_id: PeerId,
        protocol: ProtocolName,
        fallback_names: Vec<ProtocolName>,
        next_substream_id: Arc<AtomicUsize>,
        transport_handle: TransportManagerHandle,
        keep_alive_timeout: Duration,
    ) -> (Self, Sender<InnerTransportEvent>) {
        let (tx, rx) = channel(DEFAULT_CHANNEL_SIZE);

        let keep_alive_tracker = KeepAliveTracker::new(keep_alive_timeout);

        (
            Self {
                rx,
                protocol,
                local_peer_id,
                fallback_names,
                transport_handle,
                next_substream_id,
                connections: HashMap::new(),
                keep_alive_tracker,
            },
            tx,
        )
    }

    /// Get the list of public addresses of the node.
    pub fn public_addresses(&self) -> PublicAddresses {
        self.transport_handle.public_addresses()
    }

    /// Get the list of listen addresses of the node.
    pub fn listen_addresses(&self) -> HashSet<Multiaddr> {
        self.transport_handle.listen_addresses()
    }

    /// Handle connection established event.
    fn on_connection_established(
        &mut self,
        peer: PeerId,
        endpoint: Endpoint,
        connection_id: ConnectionId,
        handle: ConnectionHandle,
    ) -> Option<TransportEvent> {
        tracing::debug!(
            target: LOG_TARGET,
            ?peer,
            ?endpoint,
            ?connection_id,
            protocol = %self.protocol,
            current_state = ?self.connections.get(&peer),
            "on connection established",
        );

        match self.connections.get_mut(&peer) {
            Some(context) => match context.secondary {
                Some(_) => {
                    tracing::debug!(
                        target: LOG_TARGET,
                        ?peer,
                        ?connection_id,
                        ?endpoint,
                        protocol = %self.protocol,
                        "ignoring third connection",
                    );
                    None
                }
                None => {
                    self.keep_alive_tracker.on_connection_established(peer, connection_id);

                    tracing::trace!(
                        target: LOG_TARGET,
                        ?peer,
                        ?endpoint,
                        ?connection_id,
                        protocol = %self.protocol,
                        "secondary connection established",
                    );

                    context.secondary = Some(handle);

                    None
                }
            },
            None => {
                tracing::trace!(
                    target: LOG_TARGET,
                    ?peer,
                    ?endpoint,
                    ?connection_id,
                    protocol = %self.protocol,
                    "primary connection established",
                );

                self.connections.insert(peer, ConnectionContext::new(handle));

                self.keep_alive_tracker.on_connection_established(peer, connection_id);

                Some(TransportEvent::ConnectionEstablished { peer, endpoint })
            }
        }
    }

    /// Handle connection closed event.
    fn on_connection_closed(
        &mut self,
        peer: PeerId,
        connection_id: ConnectionId,
    ) -> Option<TransportEvent> {
        tracing::debug!(
            target: LOG_TARGET,
            ?peer,
            ?connection_id,
            protocol = %self.protocol,
            current_state = ?self.connections.get(&peer),
            "on connection closed",
        );

        self.keep_alive_tracker.on_connection_closed(peer, connection_id);

        let Some(context) = self.connections.get_mut(&peer) else {
            tracing::warn!(
                target: LOG_TARGET,
                ?peer,
                ?connection_id,
                protocol = %self.protocol,
                "connection closed to a non-existent peer",
            );

            debug_assert!(false);
            return None;
        };

        // if the primary connection was closed, check if there exist a secondary connection
        // and if it does, convert the secondary connection a primary connection
        if context.primary.connection_id() == &connection_id {
            tracing::trace!(
                target: LOG_TARGET,
                ?peer,
                ?connection_id,
                protocol = %self.protocol,
                "primary connection closed"
            );

            match context.secondary.take() {
                None => {
                    self.connections.remove(&peer);
                    return Some(TransportEvent::ConnectionClosed { peer });
                }
                Some(handle) => {
                    tracing::debug!(
                        target: LOG_TARGET,
                        ?peer,
                        ?connection_id,
                        protocol = %self.protocol,
                        "switch to secondary connection",
                    );

                    context.primary = handle;
                    return None;
                }
            }
        }

        match context.secondary.take() {
            Some(handle) if handle.connection_id() == &connection_id => {
                tracing::trace!(
                    target: LOG_TARGET,
                    ?peer,
                    ?connection_id,
                    protocol = %self.protocol,
                    "secondary connection closed",
                );

                None
            }
            connection_state => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?peer,
                    ?connection_id,
                    ?connection_state,
                    protocol = %self.protocol,
                    "connection closed but it doesn't exist",
                );

                None
            }
        }
    }

    /// Dial `peer` using `PeerId`.
    ///
    /// Call fails if `Litep2p` doesn't have a known address for the peer.
    pub fn dial(&mut self, peer: &PeerId) -> Result<(), ImmediateDialError> {
        tracing::trace!(
            target: LOG_TARGET,
            ?peer,
            protocol = %self.protocol,
            "Dial peer requested",
        );

        self.transport_handle.dial(peer)
    }

    /// Dial peer using a `Multiaddr`.
    ///
    /// Call fails if the address is not in correct format or it contains an unsupported/disabled
    /// transport.
    ///
    /// Calling this function is only necessary for those addresses that are discovered out-of-band
    /// since `Litep2p` internally keeps track of all peer addresses it has learned through user
    /// calling this function, Kademlia peer discoveries and `Identify` responses.
    pub fn dial_address(&mut self, address: Multiaddr) -> Result<(), ImmediateDialError> {
        tracing::trace!(
            target: LOG_TARGET,
            ?address,
            protocol = %self.protocol,
            "Dial address requested",
        );

        self.transport_handle.dial_address(address)
    }

    /// Add one or more addresses for `peer`.
    ///
    /// The list is filtered for duplicates and unsupported transports.
    pub fn add_known_address(&mut self, peer: &PeerId, addresses: impl Iterator<Item = Multiaddr>) {
        let addresses: HashSet<Multiaddr> = addresses
            .filter_map(|address| {
                if !std::matches!(address.iter().last(), Some(Protocol::P2p(_))) {
                    Some(address.with(Protocol::P2p(Multihash::from_bytes(&peer.to_bytes()).ok()?)))
                } else {
                    Some(address)
                }
            })
            .collect();

        self.transport_handle.add_known_address(peer, addresses.into_iter());
    }

    /// Open substream to `peer`.
    ///
    /// Call fails if there is no connection open to `peer` or the channel towards
    /// the connection is clogged.
    pub fn open_substream(&mut self, peer: PeerId) -> Result<SubstreamId, SubstreamError> {
        // always prefer the primary connection
        let connection = &mut self
            .connections
            .get_mut(&peer)
            .ok_or(SubstreamError::PeerDoesNotExist(peer))?
            .primary;

        let connection_id = *connection.connection_id();

        let permit = connection.try_get_permit().ok_or(SubstreamError::ConnectionClosed)?;
        let substream_id =
            SubstreamId::from(self.next_substream_id.fetch_add(1usize, Ordering::Relaxed));

        tracing::trace!(
            target: LOG_TARGET,
            ?peer,
            protocol = %self.protocol,
            ?substream_id,
            ?connection_id,
            "open substream",
        );

        self.keep_alive_tracker.substream_activity(peer, connection_id);
        connection.try_upgrade();

        connection
            .open_substream(
                self.protocol.clone(),
                self.fallback_names.clone(),
                substream_id,
                permit,
            )
            .map(|_| substream_id)
    }

    /// Forcibly close the connection, even if other protocols have substreams open over it.
    pub fn force_close(&mut self, peer: PeerId) -> crate::Result<()> {
        let connection =
            &mut self.connections.get_mut(&peer).ok_or(Error::PeerDoesntExist(peer))?;

        tracing::trace!(
            target: LOG_TARGET,
            ?peer,
            protocol = %self.protocol,
            secondary = ?connection.secondary,
            "forcibly closing the connection",
        );

        if let Some(ref mut connection) = connection.secondary {
            let _ = connection.force_close();
        }

        connection.primary.force_close()
    }

    /// Get local peer ID.
    pub fn local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }

    /// Dynamically unregister a protocol.
    ///
    /// This must be called when a protocol is no longer needed (e.g. user dropped the protocol
    /// handle).
    pub fn unregister_protocol(&self) {
        self.transport_handle.unregister_protocol(self.protocol.clone());
    }
}

impl Stream for TransportService {
    type Item = TransportEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let protocol_name = self.protocol.clone();
        let duration = self.keep_alive_tracker.keep_alive_timeout;

        while let Poll::Ready(event) = self.rx.poll_recv(cx) {
            match event {
                None => {
                    tracing::warn!(
                        target: LOG_TARGET,
                        protocol = ?protocol_name,
                        "transport service closed"
                    );
                    return Poll::Ready(None);
                }
                Some(InnerTransportEvent::ConnectionEstablished {
                    peer,
                    endpoint,
                    sender,
                    connection,
                }) => {
                    if let Some(event) =
                        self.on_connection_established(peer, endpoint, connection, sender)
                    {
                        return Poll::Ready(Some(event));
                    }
                }
                Some(InnerTransportEvent::ConnectionClosed { peer, connection }) => {
                    if let Some(event) = self.on_connection_closed(peer, connection) {
                        return Poll::Ready(Some(event));
                    }
                }
                Some(InnerTransportEvent::SubstreamOpened {
                    peer,
                    protocol,
                    fallback,
                    direction,
                    substream,
                    connection_id,
                }) => {
                    if protocol == self.protocol {
                        self.keep_alive_tracker.substream_activity(peer, connection_id);
                        if let Some(context) = self.connections.get_mut(&peer) {
                            context.try_upgrade(&connection_id);
                        }
                    }

                    return Poll::Ready(Some(TransportEvent::SubstreamOpened {
                        peer,
                        protocol,
                        fallback,
                        direction,
                        substream,
                    }));
                }
                Some(event) => return Poll::Ready(Some(event.into())),
            }
        }

        while let Poll::Ready(Some((peer, connection_id))) =
            self.keep_alive_tracker.poll_next_unpin(cx)
        {
            if let Some(context) = self.connections.get_mut(&peer) {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?peer,
                    ?connection_id,
                    protocol = ?protocol_name,
                    ?duration,
                    "keep-alive timeout over, downgrade connection",
                );

                context.downgrade(&connection_id);
            }
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        protocol::{ProtocolCommand, TransportService},
        transport::{
            manager::{handle::InnerTransportManagerCommand, TransportManagerHandle},
            KEEP_ALIVE_TIMEOUT,
        },
    };
    use futures::StreamExt;
    use parking_lot::RwLock;
    use std::collections::HashSet;

    /// Create new `TransportService`
    fn transport_service() -> (
        TransportService,
        Sender<InnerTransportEvent>,
        Receiver<InnerTransportManagerCommand>,
    ) {
        let (cmd_tx, cmd_rx) = channel(64);
        let peer = PeerId::random();

        let handle = TransportManagerHandle::new(
            peer,
            Arc::new(RwLock::new(HashMap::new())),
            cmd_tx,
            HashSet::new(),
            Default::default(),
            PublicAddresses::new(peer),
        );

        let (service, sender) = TransportService::new(
            peer,
            ProtocolName::from("/notif/1"),
            Vec::new(),
            Arc::new(AtomicUsize::new(0usize)),
            handle,
            KEEP_ALIVE_TIMEOUT,
        );

        (service, sender, cmd_rx)
    }

    #[tokio::test]
    async fn secondary_connection_stored() {
        let (mut service, sender, _) = transport_service();
        let peer = PeerId::random();

        // register first connection
        let (cmd_tx1, _cmd_rx1) = channel(64);
        sender
            .send(InnerTransportEvent::ConnectionEstablished {
                peer,
                connection: ConnectionId::from(0usize),
                endpoint: Endpoint::listener(Multiaddr::empty(), ConnectionId::from(0usize)),
                sender: ConnectionHandle::new(ConnectionId::from(0usize), cmd_tx1),
            })
            .await
            .unwrap();

        if let Some(TransportEvent::ConnectionEstablished {
            peer: connected_peer,
            endpoint,
        }) = service.next().await
        {
            assert_eq!(connected_peer, peer);
            assert_eq!(endpoint.address(), &Multiaddr::empty());
        } else {
            panic!("expected event from `TransportService`");
        };

        // register secondary connection
        let (cmd_tx2, _cmd_rx2) = channel(64);
        sender
            .send(InnerTransportEvent::ConnectionEstablished {
                peer,
                connection: ConnectionId::from(1usize),
                endpoint: Endpoint::listener(Multiaddr::empty(), ConnectionId::from(1usize)),
                sender: ConnectionHandle::new(ConnectionId::from(1usize), cmd_tx2),
            })
            .await
            .unwrap();

        futures::future::poll_fn(|cx| match service.poll_next_unpin(cx) {
            std::task::Poll::Ready(_) => panic!("didn't expect event from `TransportService`"),
            std::task::Poll::Pending => std::task::Poll::Ready(()),
        })
        .await;

        let context = service.connections.get(&peer).unwrap();
        assert_eq!(context.primary.connection_id(), &ConnectionId::from(0usize));
        assert_eq!(
            context.secondary.as_ref().unwrap().connection_id(),
            &ConnectionId::from(1usize)
        );
    }

    #[tokio::test]
    async fn tertiary_connection_ignored() {
        let (mut service, sender, _) = transport_service();
        let peer = PeerId::random();

        // register first connection
        let (cmd_tx1, _cmd_rx1) = channel(64);
        sender
            .send(InnerTransportEvent::ConnectionEstablished {
                peer,
                connection: ConnectionId::from(0usize),
                endpoint: Endpoint::dialer(Multiaddr::empty(), ConnectionId::from(0usize)),
                sender: ConnectionHandle::new(ConnectionId::from(0usize), cmd_tx1),
            })
            .await
            .unwrap();

        if let Some(TransportEvent::ConnectionEstablished {
            peer: connected_peer,
            endpoint,
        }) = service.next().await
        {
            assert_eq!(connected_peer, peer);
            assert_eq!(endpoint.address(), &Multiaddr::empty());
        } else {
            panic!("expected event from `TransportService`");
        };

        // register secondary connection
        let (cmd_tx2, _cmd_rx2) = channel(64);
        sender
            .send(InnerTransportEvent::ConnectionEstablished {
                peer,
                connection: ConnectionId::from(1usize),
                endpoint: Endpoint::dialer(Multiaddr::empty(), ConnectionId::from(1usize)),
                sender: ConnectionHandle::new(ConnectionId::from(1usize), cmd_tx2),
            })
            .await
            .unwrap();

        futures::future::poll_fn(|cx| match service.poll_next_unpin(cx) {
            std::task::Poll::Ready(_) => panic!("didn't expect event from `TransportService`"),
            std::task::Poll::Pending => std::task::Poll::Ready(()),
        })
        .await;

        let context = service.connections.get(&peer).unwrap();
        assert_eq!(context.primary.connection_id(), &ConnectionId::from(0usize));
        assert_eq!(
            context.secondary.as_ref().unwrap().connection_id(),
            &ConnectionId::from(1usize)
        );

        // try to register tertiary connection and verify it's ignored
        let (cmd_tx3, mut cmd_rx3) = channel(64);
        sender
            .send(InnerTransportEvent::ConnectionEstablished {
                peer,
                connection: ConnectionId::from(2usize),
                endpoint: Endpoint::listener(Multiaddr::empty(), ConnectionId::from(2usize)),
                sender: ConnectionHandle::new(ConnectionId::from(2usize), cmd_tx3),
            })
            .await
            .unwrap();

        futures::future::poll_fn(|cx| match service.poll_next_unpin(cx) {
            std::task::Poll::Ready(_) => panic!("didn't expect event from `TransportService`"),
            std::task::Poll::Pending => std::task::Poll::Ready(()),
        })
        .await;

        let context = service.connections.get(&peer).unwrap();
        assert_eq!(context.primary.connection_id(), &ConnectionId::from(0usize));
        assert_eq!(
            context.secondary.as_ref().unwrap().connection_id(),
            &ConnectionId::from(1usize)
        );
        assert!(cmd_rx3.try_recv().is_err());
    }

    #[tokio::test]
    async fn secondary_closing_does_not_emit_event() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let (mut service, sender, _) = transport_service();
        let peer = PeerId::random();

        // register first connection
        let (cmd_tx1, _cmd_rx1) = channel(64);
        sender
            .send(InnerTransportEvent::ConnectionEstablished {
                peer,
                connection: ConnectionId::from(0usize),
                endpoint: Endpoint::dialer(Multiaddr::empty(), ConnectionId::from(0usize)),
                sender: ConnectionHandle::new(ConnectionId::from(0usize), cmd_tx1),
            })
            .await
            .unwrap();

        if let Some(TransportEvent::ConnectionEstablished {
            peer: connected_peer,
            endpoint,
        }) = service.next().await
        {
            assert_eq!(connected_peer, peer);
            assert_eq!(endpoint.address(), &Multiaddr::empty());
        } else {
            panic!("expected event from `TransportService`");
        };

        // register secondary connection
        let (cmd_tx2, _cmd_rx2) = channel(64);
        sender
            .send(InnerTransportEvent::ConnectionEstablished {
                peer,
                connection: ConnectionId::from(1usize),
                endpoint: Endpoint::dialer(Multiaddr::empty(), ConnectionId::from(1usize)),
                sender: ConnectionHandle::new(ConnectionId::from(1usize), cmd_tx2),
            })
            .await
            .unwrap();

        futures::future::poll_fn(|cx| match service.poll_next_unpin(cx) {
            std::task::Poll::Ready(_) => panic!("didn't expect event from `TransportService`"),
            std::task::Poll::Pending => std::task::Poll::Ready(()),
        })
        .await;

        let context = service.connections.get(&peer).unwrap();
        assert_eq!(context.primary.connection_id(), &ConnectionId::from(0usize));
        assert_eq!(
            context.secondary.as_ref().unwrap().connection_id(),
            &ConnectionId::from(1usize)
        );

        // close the secondary connection
        sender
            .send(InnerTransportEvent::ConnectionClosed {
                peer,
                connection: ConnectionId::from(1usize),
            })
            .await
            .unwrap();

        // verify that the protocol is not notified
        futures::future::poll_fn(|cx| match service.poll_next_unpin(cx) {
            std::task::Poll::Ready(_) => panic!("didn't expect event from `TransportService`"),
            std::task::Poll::Pending => std::task::Poll::Ready(()),
        })
        .await;

        // verify that the secondary connection doesn't exist anymore
        let context = service.connections.get(&peer).unwrap();
        assert_eq!(context.primary.connection_id(), &ConnectionId::from(0usize));
        assert!(context.secondary.is_none());
    }

    #[tokio::test]
    async fn convert_secondary_to_primary() {
        let (mut service, sender, _) = transport_service();
        let peer = PeerId::random();

        // register first connection
        let (cmd_tx1, mut cmd_rx1) = channel(64);
        sender
            .send(InnerTransportEvent::ConnectionEstablished {
                peer,
                connection: ConnectionId::from(0usize),
                endpoint: Endpoint::dialer(Multiaddr::empty(), ConnectionId::from(0usize)),
                sender: ConnectionHandle::new(ConnectionId::from(0usize), cmd_tx1),
            })
            .await
            .unwrap();

        if let Some(TransportEvent::ConnectionEstablished {
            peer: connected_peer,
            endpoint,
        }) = service.next().await
        {
            assert_eq!(connected_peer, peer);
            assert_eq!(endpoint.address(), &Multiaddr::empty());
        } else {
            panic!("expected event from `TransportService`");
        };

        // register secondary connection
        let (cmd_tx2, mut cmd_rx2) = channel(64);
        sender
            .send(InnerTransportEvent::ConnectionEstablished {
                peer,
                connection: ConnectionId::from(1usize),
                endpoint: Endpoint::listener(Multiaddr::empty(), ConnectionId::from(1usize)),
                sender: ConnectionHandle::new(ConnectionId::from(1usize), cmd_tx2),
            })
            .await
            .unwrap();

        futures::future::poll_fn(|cx| match service.poll_next_unpin(cx) {
            std::task::Poll::Ready(_) => panic!("didn't expect event from `TransportService`"),
            std::task::Poll::Pending => std::task::Poll::Ready(()),
        })
        .await;

        let context = service.connections.get(&peer).unwrap();
        assert_eq!(context.primary.connection_id(), &ConnectionId::from(0usize));
        assert_eq!(
            context.secondary.as_ref().unwrap().connection_id(),
            &ConnectionId::from(1usize)
        );

        // close the primary connection
        sender
            .send(InnerTransportEvent::ConnectionClosed {
                peer,
                connection: ConnectionId::from(0usize),
            })
            .await
            .unwrap();

        // verify that the protocol is not notified
        futures::future::poll_fn(|cx| match service.poll_next_unpin(cx) {
            std::task::Poll::Ready(_) => panic!("didn't expect event from `TransportService`"),
            std::task::Poll::Pending => std::task::Poll::Ready(()),
        })
        .await;

        // verify that the primary connection has been replaced
        let context = service.connections.get(&peer).unwrap();
        assert_eq!(context.primary.connection_id(), &ConnectionId::from(1usize));
        assert!(context.secondary.is_none());
        assert!(cmd_rx1.try_recv().is_err());

        // close the secondary connection as well
        sender
            .send(InnerTransportEvent::ConnectionClosed {
                peer,
                connection: ConnectionId::from(1usize),
            })
            .await
            .unwrap();

        if let Some(TransportEvent::ConnectionClosed {
            peer: disconnected_peer,
        }) = service.next().await
        {
            assert_eq!(disconnected_peer, peer);
        } else {
            panic!("expected event from `TransportService`");
        };

        // verify that the primary connection has been replaced
        assert!(service.connections.get(&peer).is_none());
        assert!(cmd_rx2.try_recv().is_err());
    }

    #[tokio::test]
    async fn keep_alive_timeout_expires_for_a_stale_connection() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let (mut service, sender, _) = transport_service();
        let peer = PeerId::random();

        // register first connection
        let (cmd_tx1, _cmd_rx1) = channel(64);
        sender
            .send(InnerTransportEvent::ConnectionEstablished {
                peer,
                connection: ConnectionId::from(1337usize),
                endpoint: Endpoint::dialer(Multiaddr::empty(), ConnectionId::from(1337usize)),
                sender: ConnectionHandle::new(ConnectionId::from(1337usize), cmd_tx1),
            })
            .await
            .unwrap();

        if let Some(TransportEvent::ConnectionEstablished {
            peer: connected_peer,
            endpoint,
        }) = service.next().await
        {
            assert_eq!(connected_peer, peer);
            assert_eq!(endpoint.address(), &Multiaddr::empty());
        } else {
            panic!("expected event from `TransportService`");
        };

        // verify the first connection state is correct
        assert_eq!(service.keep_alive_tracker.last_activity.len(), 1);
        match service.connections.get(&peer) {
            Some(context) => {
                assert_eq!(
                    context.primary.connection_id(),
                    &ConnectionId::from(1337usize)
                );
                assert!(context.secondary.is_none());
            }
            None => panic!("expected {peer} to exist"),
        }

        // close the primary connection
        sender
            .send(InnerTransportEvent::ConnectionClosed {
                peer,
                connection: ConnectionId::from(1337usize),
            })
            .await
            .unwrap();

        // verify that the protocols are notified of the connection closing as well
        if let Some(TransportEvent::ConnectionClosed {
            peer: connected_peer,
        }) = service.next().await
        {
            assert_eq!(connected_peer, peer);
        } else {
            panic!("expected event from `TransportService`");
        }

        // Because the connection was closed, the peer is no longer tracked for keep-alive.
        // This leads to better tracking overall since we don't have to track stale connections.
        assert!(service.keep_alive_tracker.last_activity.is_empty());
        assert!(service.connections.get(&peer).is_none());

        // Register new primary connection.
        let (cmd_tx1, _cmd_rx1) = channel(64);
        sender
            .send(InnerTransportEvent::ConnectionEstablished {
                peer,
                connection: ConnectionId::from(1338usize),
                endpoint: Endpoint::listener(Multiaddr::empty(), ConnectionId::from(1338usize)),
                sender: ConnectionHandle::new(ConnectionId::from(1338usize), cmd_tx1),
            })
            .await
            .unwrap();

        if let Some(TransportEvent::ConnectionEstablished {
            peer: connected_peer,
            endpoint,
        }) = service.next().await
        {
            assert_eq!(connected_peer, peer);
            assert_eq!(endpoint.address(), &Multiaddr::empty());
        } else {
            panic!("expected event from `TransportService`");
        };

        assert_eq!(service.keep_alive_tracker.last_activity.len(), 1);
        match service.connections.get(&peer) {
            Some(context) => {
                assert_eq!(
                    context.primary.connection_id(),
                    &ConnectionId::from(1338usize)
                );
                assert!(context.secondary.is_none());
            }
            None => panic!("expected {peer} to exist"),
        }

        match tokio::time::timeout(Duration::from_secs(10), service.next()).await {
            Ok(event) => panic!("didn't expect an event: {event:?}"),
            Err(_) => {}
        }
    }

    async fn poll_service(service: &mut TransportService) {
        futures::future::poll_fn(|cx| match service.poll_next_unpin(cx) {
            std::task::Poll::Ready(_) => panic!("didn't expect event from `TransportService`"),
            std::task::Poll::Pending => std::task::Poll::Ready(()),
        })
        .await;
    }

    #[tokio::test]
    async fn keep_alive_timeout_downgrades_connections() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let (mut service, sender, _) = transport_service();
        let peer = PeerId::random();

        // register first connection
        let (cmd_tx1, _cmd_rx1) = channel(64);
        sender
            .send(InnerTransportEvent::ConnectionEstablished {
                peer,
                connection: ConnectionId::from(1337usize),
                endpoint: Endpoint::dialer(Multiaddr::empty(), ConnectionId::from(1337usize)),
                sender: ConnectionHandle::new(ConnectionId::from(1337usize), cmd_tx1),
            })
            .await
            .unwrap();

        if let Some(TransportEvent::ConnectionEstablished {
            peer: connected_peer,
            endpoint,
        }) = service.next().await
        {
            assert_eq!(connected_peer, peer);
            assert_eq!(endpoint.address(), &Multiaddr::empty());
        } else {
            panic!("expected event from `TransportService`");
        };

        // verify the first connection state is correct
        assert_eq!(service.keep_alive_tracker.last_activity.len(), 1);
        match service.connections.get(&peer) {
            Some(context) => {
                assert_eq!(
                    context.primary.connection_id(),
                    &ConnectionId::from(1337usize)
                );
                // Check the connection is still active.
                assert!(context.primary.is_active());
                assert!(context.secondary.is_none());
            }
            None => panic!("expected {peer} to exist"),
        }

        poll_service(&mut service).await;
        tokio::time::sleep(KEEP_ALIVE_TIMEOUT + std::time::Duration::from_secs(1)).await;
        poll_service(&mut service).await;

        // Verify the connection is downgraded.
        match service.connections.get(&peer) {
            Some(context) => {
                assert_eq!(
                    context.primary.connection_id(),
                    &ConnectionId::from(1337usize)
                );
                // Check the connection is not active.
                assert!(!context.primary.is_active());
                assert!(context.secondary.is_none());
            }
            None => panic!("expected {peer} to exist"),
        }

        assert_eq!(service.keep_alive_tracker.last_activity.len(), 0);
    }

    #[tokio::test]
    async fn keep_alive_timeout_reset_when_user_opens_substream() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let (mut service, sender, _) = transport_service();
        let peer = PeerId::random();

        // register first connection
        let (cmd_tx1, _cmd_rx1) = channel(64);
        sender
            .send(InnerTransportEvent::ConnectionEstablished {
                peer,
                connection: ConnectionId::from(1337usize),
                endpoint: Endpoint::dialer(Multiaddr::empty(), ConnectionId::from(1337usize)),
                sender: ConnectionHandle::new(ConnectionId::from(1337usize), cmd_tx1),
            })
            .await
            .unwrap();

        if let Some(TransportEvent::ConnectionEstablished {
            peer: connected_peer,
            endpoint,
        }) = service.next().await
        {
            assert_eq!(connected_peer, peer);
            assert_eq!(endpoint.address(), &Multiaddr::empty());
        } else {
            panic!("expected event from `TransportService`");
        };

        // verify the first connection state is correct
        assert_eq!(service.keep_alive_tracker.last_activity.len(), 1);
        match service.connections.get(&peer) {
            Some(context) => {
                assert_eq!(
                    context.primary.connection_id(),
                    &ConnectionId::from(1337usize)
                );
                // Check the connection is still active.
                assert!(context.primary.is_active());
                assert!(context.secondary.is_none());
            }
            None => panic!("expected {peer} to exist"),
        }

        poll_service(&mut service).await;
        // Sleep for almost the entire keep-alive timeout.
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // This ensures we reset the keep-alive timer when other protocols
        // want to open a substream.
        // We are still tracking the same peer.
        service.open_substream(peer).unwrap();
        assert_eq!(service.keep_alive_tracker.last_activity.len(), 1);

        poll_service(&mut service).await;
        // The keep alive timeout should be advanced.
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        poll_service(&mut service).await;
        assert_eq!(service.keep_alive_tracker.last_activity.len(), 1);
        // If the `service.open_substream` wasn't called, the connection would have been downgraded.
        // Instead the keep-alive was forwarded `KEEP_ALIVE_TIMEOUT` seconds into the future.
        // Verify the connection is still active.
        match service.connections.get(&peer) {
            Some(context) => {
                assert_eq!(
                    context.primary.connection_id(),
                    &ConnectionId::from(1337usize)
                );
                assert!(context.primary.is_active());
                assert!(context.secondary.is_none());
            }
            None => panic!("expected {peer} to exist"),
        }

        poll_service(&mut service).await;
        tokio::time::sleep(KEEP_ALIVE_TIMEOUT).await;
        poll_service(&mut service).await;

        assert_eq!(service.keep_alive_tracker.last_activity.len(), 0);

        // The connection had no substream activity for `KEEP_ALIVE_TIMEOUT` seconds.
        // Verify the connection is downgraded.
        match service.connections.get(&peer) {
            Some(context) => {
                assert_eq!(
                    context.primary.connection_id(),
                    &ConnectionId::from(1337usize)
                );
                assert!(!context.primary.is_active());
                assert!(context.secondary.is_none());
            }
            None => panic!("expected {peer} to exist"),
        }
    }

    #[tokio::test]
    async fn downgraded_connection_without_substreams_is_closed() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let (mut service, sender, _) = transport_service();
        let peer = PeerId::random();

        // register first connection
        let (cmd_tx1, mut cmd_rx1) = channel(64);
        sender
            .send(InnerTransportEvent::ConnectionEstablished {
                peer,
                connection: ConnectionId::from(1337usize),
                endpoint: Endpoint::dialer(Multiaddr::empty(), ConnectionId::from(1337usize)),
                sender: ConnectionHandle::new(ConnectionId::from(1337usize), cmd_tx1),
            })
            .await
            .unwrap();

        if let Some(TransportEvent::ConnectionEstablished {
            peer: connected_peer,
            endpoint,
        }) = service.next().await
        {
            assert_eq!(connected_peer, peer);
            assert_eq!(endpoint.address(), &Multiaddr::empty());
        } else {
            panic!("expected event from `TransportService`");
        };

        // verify the first connection state is correct
        assert_eq!(service.keep_alive_tracker.last_activity.len(), 1);
        match service.connections.get(&peer) {
            Some(context) => {
                assert_eq!(
                    context.primary.connection_id(),
                    &ConnectionId::from(1337usize)
                );
                // Check the connection is still active.
                assert!(context.primary.is_active());
                assert!(context.secondary.is_none());
            }
            None => panic!("expected {peer} to exist"),
        }

        // Open substreams to the peer.
        let substream_id = service.open_substream(peer).unwrap();
        let second_substream_id = service.open_substream(peer).unwrap();

        // Simulate keep-alive timeout expiration.
        poll_service(&mut service).await;
        tokio::time::sleep(KEEP_ALIVE_TIMEOUT + std::time::Duration::from_secs(1)).await;
        poll_service(&mut service).await;

        let mut permits = Vec::new();

        // First substream.
        let protocol_command = cmd_rx1.recv().await.unwrap();
        match protocol_command {
            ProtocolCommand::OpenSubstream {
                protocol,
                substream_id: opened_substream_id,
                permit,
                ..
            } => {
                assert_eq!(protocol, ProtocolName::from("/notif/1"));
                assert_eq!(substream_id, opened_substream_id);

                // Save the substream permit for later.
                permits.push(permit);
            }
            _ => panic!("expected `ProtocolCommand::OpenSubstream`"),
        }

        // Second substream.
        let protocol_command = cmd_rx1.recv().await.unwrap();
        match protocol_command {
            ProtocolCommand::OpenSubstream {
                protocol,
                substream_id: opened_substream_id,
                permit,
                ..
            } => {
                assert_eq!(protocol, ProtocolName::from("/notif/1"));
                assert_eq!(second_substream_id, opened_substream_id);

                // Save the substream permit for later.
                permits.push(permit);
            }
            _ => panic!("expected `ProtocolCommand::OpenSubstream`"),
        }

        // Drop one permit.
        let permit = permits.pop();
        // Individual transports like TCP will open a substream
        // and then will generate a `SubstreamOpened` event via
        // the protocol-set handler.
        //
        // The substream is used by individual protocols and then
        // is closed. This simulates the substream being closed.
        drop(permit);

        // Open a new substream to the peer. This will succeed as long as we still have
        // one substream open.
        let substream_id = service.open_substream(peer).unwrap();
        // Handle the substream.
        let protocol_command = cmd_rx1.recv().await.unwrap();
        match protocol_command {
            ProtocolCommand::OpenSubstream {
                protocol,
                substream_id: opened_substream_id,
                permit,
                ..
            } => {
                assert_eq!(protocol, ProtocolName::from("/notif/1"));
                assert_eq!(substream_id, opened_substream_id);

                // Save the substream permit for later.
                permits.push(permit);
            }
            _ => panic!("expected `ProtocolCommand::OpenSubstream`"),
        }

        // Drop all substreams.
        drop(permits);

        poll_service(&mut service).await;
        tokio::time::sleep(KEEP_ALIVE_TIMEOUT + std::time::Duration::from_secs(1)).await;
        poll_service(&mut service).await;

        // Cannot open a new substream because:
        // 1. connection was downgraded by keep-alive timeout
        // 2. all substreams were dropped.
        assert_eq!(
            service.open_substream(peer),
            Err(SubstreamError::ConnectionClosed)
        );
    }

    #[tokio::test]
    async fn substream_opening_upgrades_connection_and_resets_keep_alive() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let (mut service, sender, _) = transport_service();
        let peer = PeerId::random();

        // register first connection
        let (cmd_tx1, mut cmd_rx1) = channel(64);
        sender
            .send(InnerTransportEvent::ConnectionEstablished {
                peer,
                connection: ConnectionId::from(1337usize),
                endpoint: Endpoint::dialer(Multiaddr::empty(), ConnectionId::from(1337usize)),
                sender: ConnectionHandle::new(ConnectionId::from(1337usize), cmd_tx1),
            })
            .await
            .unwrap();

        if let Some(TransportEvent::ConnectionEstablished {
            peer: connected_peer,
            endpoint,
        }) = service.next().await
        {
            assert_eq!(connected_peer, peer);
            assert_eq!(endpoint.address(), &Multiaddr::empty());
        } else {
            panic!("expected event from `TransportService`");
        };

        // verify the first connection state is correct
        assert_eq!(service.keep_alive_tracker.last_activity.len(), 1);
        match service.connections.get(&peer) {
            Some(context) => {
                assert_eq!(
                    context.primary.connection_id(),
                    &ConnectionId::from(1337usize)
                );
                // Check the connection is still active.
                assert!(context.primary.is_active());
                assert!(context.secondary.is_none());
            }
            None => panic!("expected {peer} to exist"),
        }

        // Open substreams to the peer.
        let substream_id = service.open_substream(peer).unwrap();
        let second_substream_id = service.open_substream(peer).unwrap();

        let mut permits = Vec::new();
        // First substream.
        let protocol_command = cmd_rx1.recv().await.unwrap();
        match protocol_command {
            ProtocolCommand::OpenSubstream {
                protocol,
                substream_id: opened_substream_id,
                permit,
                ..
            } => {
                assert_eq!(protocol, ProtocolName::from("/notif/1"));
                assert_eq!(substream_id, opened_substream_id);

                // Save the substream permit for later.
                permits.push(permit);
            }
            _ => panic!("expected `ProtocolCommand::OpenSubstream`"),
        }

        // Second substream.
        let protocol_command = cmd_rx1.recv().await.unwrap();
        match protocol_command {
            ProtocolCommand::OpenSubstream {
                protocol,
                substream_id: opened_substream_id,
                permit,
                ..
            } => {
                assert_eq!(protocol, ProtocolName::from("/notif/1"));
                assert_eq!(second_substream_id, opened_substream_id);

                // Save the substream permit for later.
                permits.push(permit);
            }
            _ => panic!("expected `ProtocolCommand::OpenSubstream`"),
        }

        // Sleep to trigger keep-alive timeout.
        poll_service(&mut service).await;
        tokio::time::sleep(KEEP_ALIVE_TIMEOUT + std::time::Duration::from_secs(1)).await;
        poll_service(&mut service).await;

        // Verify the connection is downgraded.
        match service.connections.get(&peer) {
            Some(context) => {
                assert_eq!(
                    context.primary.connection_id(),
                    &ConnectionId::from(1337usize)
                );
                // Check the connection is not active.
                assert!(!context.primary.is_active());
                assert!(context.secondary.is_none());
            }
            None => panic!("expected {peer} to exist"),
        }
        assert_eq!(service.keep_alive_tracker.last_activity.len(), 0);

        // Open a new substream to the peer. This will succeed as long as we still have
        // at least substream permit.
        let substream_id = service.open_substream(peer).unwrap();
        let protocol_command = cmd_rx1.recv().await.unwrap();
        match protocol_command {
            ProtocolCommand::OpenSubstream {
                protocol,
                substream_id: opened_substream_id,
                permit,
                ..
            } => {
                assert_eq!(protocol, ProtocolName::from("/notif/1"));
                assert_eq!(substream_id, opened_substream_id);

                // Save the substream permit for later.
                permits.push(permit);
            }
            _ => panic!("expected `ProtocolCommand::OpenSubstream`"),
        }

        poll_service(&mut service).await;

        // Verify the connection is upgraded and keep-alive is tracked.
        match service.connections.get(&peer) {
            Some(context) => {
                assert_eq!(
                    context.primary.connection_id(),
                    &ConnectionId::from(1337usize)
                );
                // Check the connection is active, because it was upgraded by the last substream.
                assert!(context.primary.is_active());
                assert!(context.secondary.is_none());
            }
            None => panic!("expected {peer} to exist"),
        }
        assert_eq!(service.keep_alive_tracker.last_activity.len(), 1);

        // Drop all substreams
        drop(permits);

        // The connection is still active, because it was upgraded by the last substream open.
        match service.connections.get(&peer) {
            Some(context) => {
                assert_eq!(
                    context.primary.connection_id(),
                    &ConnectionId::from(1337usize)
                );
                // Check the connection is active, because it was upgraded by the last substream.
                assert!(context.primary.is_active());
                assert!(context.secondary.is_none());
            }
            None => panic!("expected {peer} to exist"),
        }
        assert_eq!(service.keep_alive_tracker.last_activity.len(), 1);

        // Sleep to trigger keep-alive timeout.
        poll_service(&mut service).await;
        tokio::time::sleep(KEEP_ALIVE_TIMEOUT + std::time::Duration::from_secs(1)).await;
        poll_service(&mut service).await;

        match service.connections.get(&peer) {
            Some(context) => {
                assert_eq!(
                    context.primary.connection_id(),
                    &ConnectionId::from(1337usize)
                );
                // No longer active because it was downgraded by keep-alive and no
                // substream opens were made.
                assert!(!context.primary.is_active());
                assert!(context.secondary.is_none());
            }
            None => panic!("expected {peer} to exist"),
        }

        // Cannot open a new substream because:
        // 1. connection was downgraded by keep-alive timeout
        // 2. all substreams were dropped.
        assert_eq!(
            service.open_substream(peer),
            Err(SubstreamError::ConnectionClosed)
        );
    }

    #[tokio::test]
    async fn keep_alive_pop_elements() {
        let mut tracker = KeepAliveTracker::new(Duration::from_secs(1));

        let (peer1, connection1) = (PeerId::random(), ConnectionId::from(1usize));
        let (peer2, connection2) = (PeerId::random(), ConnectionId::from(2usize));
        let added_keys = HashSet::from([(peer1, connection1), (peer2, connection2)]);

        tracker.on_connection_established(peer1, connection1);
        tracker.on_connection_established(peer2, connection2);

        tokio::time::sleep(Duration::from_secs(2)).await;

        let key = tracker.next().await.unwrap();
        assert!(added_keys.contains(&key));

        let key = tracker.next().await.unwrap();
        assert!(added_keys.contains(&key));

        // No more elements.
        assert!(tracker.pending_keep_alive_timeouts.is_empty());
        assert!(tracker.last_activity.is_empty());
    }
}
