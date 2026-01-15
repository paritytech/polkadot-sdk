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

//! [`/ipfs/kad/1.0.0`](https://github.com/libp2p/specs/blob/master/kad-dht/README.md) implementation.

use crate::{
    error::{Error, ImmediateDialError, SubstreamError},
    protocol::{
        libp2p::kademlia::{
            bucket::KBucketEntry,
            executor::{QueryContext, QueryExecutor, QueryResult},
            message::KademliaMessage,
            query::{QueryAction, QueryEngine},
            routing_table::RoutingTable,
            store::{MemoryStore, MemoryStoreAction},
            types::{ConnectionType, KademliaPeer, Key},
        },
        Direction, TransportEvent, TransportService,
    },
    substream::Substream,
    transport::Endpoint,
    types::SubstreamId,
    PeerId,
};

use bytes::{Bytes, BytesMut};
use futures::StreamExt;
use multiaddr::Multiaddr;
use tokio::sync::mpsc::{Receiver, Sender};

use std::{
    collections::{hash_map::Entry, HashMap},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub use config::{Config, ConfigBuilder};
pub use handle::{
    IncomingRecordValidationMode, KademliaCommand, KademliaEvent, KademliaHandle, Quorum,
    RoutingTableUpdateMode,
};
pub use query::QueryId;
pub use record::{ContentProvider, Key as RecordKey, PeerRecord, Record};

/// Logging target for the file.
const LOG_TARGET: &str = "litep2p::ipfs::kademlia";

/// Parallelism factor, `α`.
const PARALLELISM_FACTOR: usize = 3;

mod bucket;
mod config;
mod executor;
mod handle;
mod message;
mod query;
mod record;
mod routing_table;
mod store;
mod types;

mod schema {
    pub(super) mod kademlia {
        include!(concat!(env!("OUT_DIR"), "/kademlia.rs"));
    }
}

/// Peer action.
#[derive(Debug, Clone)]
#[allow(clippy::enum_variant_names)]
enum PeerAction {
    /// Find nodes (and values/providers) as part of `FIND_NODE`/`GET_VALUE`/`GET_PROVIDERS` query.
    // TODO: may be a better naming would be `SendFindRequest`?
    SendFindNode(QueryId),

    /// Send `PUT_VALUE` message to peer.
    SendPutValue(QueryId, Bytes),

    /// Send `ADD_PROVIDER` message to peer.
    SendAddProvider(QueryId, Bytes),
}

impl PeerAction {
    fn query_id(&self) -> QueryId {
        match self {
            PeerAction::SendFindNode(query_id) => *query_id,
            PeerAction::SendPutValue(query_id, _) => *query_id,
            PeerAction::SendAddProvider(query_id, _) => *query_id,
        }
    }
}

/// Peer context.
#[derive(Default)]
struct PeerContext {
    /// Pending action, if any.
    pending_actions: HashMap<SubstreamId, PeerAction>,
}

impl PeerContext {
    /// Create new [`PeerContext`].
    pub fn new() -> Self {
        Self {
            pending_actions: HashMap::new(),
        }
    }

    /// Add pending action for peer.
    pub fn add_pending_action(&mut self, substream_id: SubstreamId, action: PeerAction) {
        self.pending_actions.insert(substream_id, action);
    }
}

/// Main Kademlia object.
pub(crate) struct Kademlia {
    /// Transport service.
    service: TransportService,

    /// Local Kademlia key.
    local_key: Key<PeerId>,

    /// Connected peers,
    peers: HashMap<PeerId, PeerContext>,

    /// TX channel for sending events to `KademliaHandle`.
    event_tx: Sender<KademliaEvent>,

    /// RX channel for receiving commands from `KademliaHandle`.
    cmd_rx: Receiver<KademliaCommand>,

    /// Next query ID.
    next_query_id: Arc<AtomicUsize>,

    /// Routing table.
    routing_table: RoutingTable,

    /// Replication factor.
    replication_factor: usize,

    /// Record store.
    store: MemoryStore,

    /// Pending outbound substreams.
    pending_substreams: HashMap<SubstreamId, PeerId>,

    /// Pending dials.
    pending_dials: HashMap<PeerId, Vec<PeerAction>>,

    /// Routing table update mode.
    update_mode: RoutingTableUpdateMode,

    /// Incoming records validation mode.
    validation_mode: IncomingRecordValidationMode,

    /// Default record TTL.
    record_ttl: Duration,

    /// Query engine.
    engine: QueryEngine,

    /// Query executor.
    executor: QueryExecutor,
}

impl Kademlia {
    /// Create new [`Kademlia`].
    pub(crate) fn new(mut service: TransportService, config: Config) -> Self {
        let local_peer_id = service.local_peer_id();
        let local_key = Key::from(service.local_peer_id());
        let mut routing_table = RoutingTable::new(local_key.clone());

        for (peer, addresses) in config.known_peers {
            tracing::trace!(target: LOG_TARGET, ?peer, ?addresses, "add bootstrap peer");

            routing_table.add_known_peer(peer, addresses.clone(), ConnectionType::NotConnected);
            service.add_known_address(&peer, addresses.into_iter());
        }

        let store = MemoryStore::with_config(local_peer_id, config.memory_store_config);

        Self {
            service,
            routing_table,
            peers: HashMap::new(),
            cmd_rx: config.cmd_rx,
            next_query_id: config.next_query_id,
            store,
            event_tx: config.event_tx,
            local_key,
            pending_dials: HashMap::new(),
            executor: QueryExecutor::new(),
            pending_substreams: HashMap::new(),
            update_mode: config.update_mode,
            validation_mode: config.validation_mode,
            record_ttl: config.record_ttl,
            replication_factor: config.replication_factor,
            engine: QueryEngine::new(local_peer_id, config.replication_factor, PARALLELISM_FACTOR),
        }
    }

    /// Allocate next query ID.
    fn next_query_id(&mut self) -> QueryId {
        let query_id = self.next_query_id.fetch_add(1, Ordering::Relaxed);

        QueryId(query_id)
    }

    /// Connection established to remote peer.
    fn on_connection_established(&mut self, peer: PeerId, endpoint: Endpoint) -> crate::Result<()> {
        tracing::trace!(target: LOG_TARGET, ?peer, "connection established");

        match self.peers.entry(peer) {
            Entry::Vacant(entry) => {
                // Set the conenction type to connected and potentially save the address in the
                // table.
                //
                // Note: this happens regardless of the state of the kademlia managed peers, because
                // an already occupied entry in the `self.peers` map does not mean that we are
                // no longer interested in the address / connection type of the peer.
                self.routing_table.on_connection_established(Key::from(peer), endpoint);

                let Some(actions) = self.pending_dials.remove(&peer) else {
                    // Note that we do not add peer entry if we don't have any pending actions.
                    // This is done to not populate `self.peers` with peers that don't support
                    // our Kademlia protocol.
                    return Ok(());
                };

                // go over all pending actions, open substreams and save the state to `PeerContext`
                // from which it will be later queried when the substream opens
                let mut context = PeerContext::new();

                for action in actions {
                    match self.service.open_substream(peer) {
                        Ok(substream_id) => {
                            context.add_pending_action(substream_id, action);
                        }
                        Err(error) => {
                            tracing::debug!(
                                target: LOG_TARGET,
                                ?peer,
                                ?action,
                                ?error,
                                "connection established to peer but failed to open substream",
                            );

                            if let PeerAction::SendFindNode(query_id) = action {
                                self.engine.register_send_failure(query_id, peer);
                                self.engine.register_response_failure(query_id, peer);
                            }
                        }
                    }
                }

                entry.insert(context);
                Ok(())
            }
            Entry::Occupied(_) => {
                tracing::warn!(
                    target: LOG_TARGET,
                    ?peer,
                    ?endpoint,
                    "connection already exists, discarding opening substreams, this is unexpected"
                );

                // Update the connection in the routing table, similar as above. The function call
                // happens in two places to avoid unnecessary cloning of the endpoint for logging
                // purposes.
                self.routing_table.on_connection_established(Key::from(peer), endpoint);

                Err(Error::PeerAlreadyExists(peer))
            }
        }
    }

    /// Disconnect peer from `Kademlia`.
    ///
    /// Peer is disconnected either because the substream was detected closed
    /// or because the connection was closed.
    ///
    /// The peer is kept in the routing table but its connection state is set
    /// as `NotConnected`, meaning it can be evicted from a k-bucket if another
    /// peer that shares the bucket connects.
    async fn disconnect_peer(&mut self, peer: PeerId, query: Option<QueryId>) {
        tracing::trace!(target: LOG_TARGET, ?peer, ?query, "disconnect peer");

        if let Some(query) = query {
            self.engine.register_peer_failure(query, peer);
        }

        // Apart from the failing query, we need to fail all other pending queries for the peer
        // being disconnected.
        if let Some(PeerContext { pending_actions }) = self.peers.remove(&peer) {
            pending_actions.into_iter().for_each(|(_, action)| {
                // Don't report failure twice for the same `query_id` if it was already reported
                // above. (We can still have other pending queries for the peer that
                // need to be reported.)
                let query_id = action.query_id();
                if Some(query_id) != query {
                    self.engine.register_peer_failure(query_id, peer);
                }
            });
        }

        if let KBucketEntry::Occupied(entry) = self.routing_table.entry(Key::from(peer)) {
            entry.connection = ConnectionType::NotConnected;
        }
    }

    /// Local node opened a substream to remote node.
    async fn on_outbound_substream(
        &mut self,
        peer: PeerId,
        substream_id: SubstreamId,
        substream: Substream,
    ) -> crate::Result<()> {
        tracing::trace!(
            target: LOG_TARGET,
            ?peer,
            ?substream_id,
            "outbound substream opened",
        );
        let _ = self.pending_substreams.remove(&substream_id);

        let pending_action = &mut self
            .peers
            .get_mut(&peer)
            // If we opened an outbound substream, we must have pending actions for the peer.
            .ok_or(Error::PeerDoesntExist(peer))?
            .pending_actions
            .remove(&substream_id);

        match pending_action.take() {
            None => {
                tracing::trace!(
                    target: LOG_TARGET,
                    ?peer,
                    ?substream_id,
                    "pending action doesn't exist for peer, closing substream",
                );

                let _ = substream.close().await;
                return Ok(());
            }
            Some(PeerAction::SendFindNode(query)) => {
                match self.engine.next_peer_action(&query, &peer) {
                    Some(QueryAction::SendMessage {
                        query,
                        peer,
                        message,
                    }) => {
                        tracing::trace!(target: LOG_TARGET, ?peer, ?query, "start sending message to peer");

                        self.executor.send_request_read_response(
                            peer,
                            Some(query),
                            message,
                            substream,
                        );
                    }
                    // query finished while the substream was being opened
                    None => {
                        let _ = substream.close().await;
                    }
                    action => {
                        tracing::warn!(target: LOG_TARGET, ?query, ?peer, ?action, "unexpected action for `FIND_NODE`");
                        let _ = substream.close().await;
                        debug_assert!(false);
                    }
                }
            }
            Some(PeerAction::SendPutValue(query, message)) => {
                tracing::trace!(target: LOG_TARGET, ?peer, "send `PUT_VALUE` message");

                self.executor.send_request_eat_response_failure(
                    peer,
                    Some(query),
                    message,
                    substream,
                );
                // TODO: replace this with `send_request_read_response` as part of
                // https://github.com/paritytech/litep2p/issues/429.
            }
            Some(PeerAction::SendAddProvider(query, message)) => {
                tracing::trace!(target: LOG_TARGET, ?peer, "send `ADD_PROVIDER` message");

                self.executor.send_message(peer, Some(query), message, substream);
            }
        }

        Ok(())
    }

    /// Remote opened a substream to local node.
    async fn on_inbound_substream(&mut self, peer: PeerId, substream: Substream) {
        tracing::trace!(target: LOG_TARGET, ?peer, "inbound substream opened");

        // Ensure peer entry exists to treat peer as [`ConnectionType::Connected`].
        // when inserting into the routing table.
        self.peers.entry(peer).or_default();

        self.executor.read_message(peer, None, substream);
    }

    /// Update routing table if the routing table update mode was set to automatic.
    ///
    /// Inform user about the potential routing table, allowing them to update it manually if
    /// the mode was set to manual.
    async fn update_routing_table(&mut self, peers: &[KademliaPeer]) {
        let peers: Vec<_> =
            peers.iter().filter(|peer| peer.peer != self.service.local_peer_id()).collect();

        // inform user about the routing table update, regardless of what the routing table update
        // mode is
        let _ = self
            .event_tx
            .send(KademliaEvent::RoutingTableUpdate {
                peers: peers.iter().map(|peer| peer.peer).collect::<Vec<PeerId>>(),
            })
            .await;

        for info in peers {
            let addresses = info.addresses();
            self.service.add_known_address(&info.peer, addresses.clone().into_iter());

            if std::matches!(self.update_mode, RoutingTableUpdateMode::Automatic) {
                self.routing_table.add_known_peer(
                    info.peer,
                    addresses,
                    self.peers
                        .get(&info.peer)
                        .map_or(ConnectionType::NotConnected, |_| ConnectionType::Connected),
                );
            }
        }
    }

    /// Handle received message.
    async fn on_message_received(
        &mut self,
        peer: PeerId,
        query_id: Option<QueryId>,
        message: BytesMut,
        substream: Substream,
    ) -> crate::Result<()> {
        tracing::trace!(target: LOG_TARGET, ?peer, query = ?query_id, "handle message from peer");

        match KademliaMessage::from_bytes(message, self.replication_factor)
            .ok_or(Error::InvalidData)?
        {
            KademliaMessage::FindNode { target, peers } => {
                match query_id {
                    Some(query_id) => {
                        tracing::trace!(
                            target: LOG_TARGET,
                            ?peer,
                            ?target,
                            query = ?query_id,
                            "handle `FIND_NODE` response",
                        );

                        // update routing table and inform user about the update
                        self.update_routing_table(&peers).await;
                        self.engine.register_response(
                            query_id,
                            peer,
                            KademliaMessage::FindNode { target, peers },
                        );
                        substream.close().await;
                    }
                    None => {
                        tracing::trace!(
                            target: LOG_TARGET,
                            ?peer,
                            ?target,
                            "handle `FIND_NODE` request",
                        );

                        let message = KademliaMessage::find_node_response(
                            &target,
                            self.routing_table
                                .closest(&Key::new(target.as_ref()), self.replication_factor),
                        );
                        self.executor.send_message(peer, None, message.into(), substream);
                    }
                }
            }
            KademliaMessage::PutValue { record } => match query_id {
                Some(query_id) => {
                    tracing::trace!(
                        target: LOG_TARGET,
                        ?peer,
                        query = ?query_id,
                        record_key = ?record.key,
                        "handle `PUT_VALUE` response",
                    );

                    self.engine.register_response(
                        query_id,
                        peer,
                        KademliaMessage::PutValue { record },
                    );
                    substream.close().await;
                }
                None => {
                    tracing::trace!(
                        target: LOG_TARGET,
                        ?peer,
                        record_key = ?record.key,
                        "handle `PUT_VALUE` request",
                    );

                    if let IncomingRecordValidationMode::Automatic = self.validation_mode {
                        self.store.put(record.clone());
                    }

                    // Send ACK even if the record was/will be filtered out to not reveal any
                    // internal state.
                    let message = KademliaMessage::put_value_response(
                        record.key.clone(),
                        record.value.clone(),
                    );
                    self.executor.send_message_eat_failure(peer, None, message, substream);
                    // TODO: replace this with `send_message` as part of
                    // https://github.com/paritytech/litep2p/issues/429.

                    let _ = self.event_tx.send(KademliaEvent::IncomingRecord { record }).await;
                }
            },
            KademliaMessage::GetRecord { key, record, peers } => {
                match (query_id, key) {
                    (Some(query_id), key) => {
                        tracing::trace!(
                            target: LOG_TARGET,
                            ?peer,
                            query = ?query_id,
                            ?peers,
                            ?record,
                            "handle `GET_VALUE` response",
                        );

                        // update routing table and inform user about the update
                        self.update_routing_table(&peers).await;

                        self.engine.register_response(
                            query_id,
                            peer,
                            KademliaMessage::GetRecord { key, record, peers },
                        );

                        substream.close().await;
                    }
                    (None, Some(key)) => {
                        tracing::trace!(
                            target: LOG_TARGET,
                            ?peer,
                            ?key,
                            "handle `GET_VALUE` request",
                        );

                        let value = self.store.get(&key).cloned();
                        let closest_peers = self
                            .routing_table
                            .closest(&Key::new(key.as_ref()), self.replication_factor);

                        let message =
                            KademliaMessage::get_value_response(key, closest_peers, value);
                        self.executor.send_message(peer, None, message.into(), substream);
                    }
                    (None, None) => tracing::debug!(
                        target: LOG_TARGET,
                        ?peer,
                        ?record,
                        ?peers,
                        "unable to handle `GET_RECORD` request with empty key",
                    ),
                }
            }
            KademliaMessage::AddProvider { key, mut providers } => {
                tracing::trace!(
                    target: LOG_TARGET,
                    ?peer,
                    ?key,
                    ?providers,
                    "handle `ADD_PROVIDER` message",
                );

                match (providers.len(), providers.pop()) {
                    (1, Some(provider)) => {
                        let addresses = provider.addresses();

                        if provider.peer == peer {
                            self.store.put_provider(
                                key.clone(),
                                ContentProvider {
                                    peer,
                                    addresses: addresses.clone(),
                                },
                            );

                            let _ = self
                                .event_tx
                                .send(KademliaEvent::IncomingProvider {
                                    provided_key: key,
                                    provider: ContentProvider {
                                        peer: provider.peer,
                                        addresses,
                                    },
                                })
                                .await;
                        } else {
                            tracing::trace!(
                                target: LOG_TARGET,
                                publisher = ?peer,
                                provider = ?provider.peer,
                                "ignoring `ADD_PROVIDER` message with `publisher` != `provider`"
                            )
                        }
                    }
                    (n, _) => {
                        tracing::trace!(
                            target: LOG_TARGET,
                            publisher = ?peer,
                            ?n,
                            "ignoring `ADD_PROVIDER` message with `n` != 1 providers"
                        )
                    }
                }
            }
            KademliaMessage::GetProviders {
                key,
                peers,
                providers,
            } => {
                match (query_id, key) {
                    (Some(query_id), key) => {
                        // Note: key is not required, but can be non-empty. We just ignore it here.
                        tracing::trace!(
                            target: LOG_TARGET,
                            ?peer,
                            query = ?query_id,
                            ?key,
                            ?peers,
                            ?providers,
                            "handle `GET_PROVIDERS` response",
                        );

                        // update routing table and inform user about the update
                        self.update_routing_table(&peers).await;

                        self.engine.register_response(
                            query_id,
                            peer,
                            KademliaMessage::GetProviders {
                                key,
                                peers,
                                providers,
                            },
                        );

                        substream.close().await;
                    }
                    (None, Some(key)) => {
                        tracing::trace!(
                            target: LOG_TARGET,
                            ?peer,
                            ?key,
                            "handle `GET_PROVIDERS` request",
                        );

                        let mut providers = self.store.get_providers(&key);

                        // Make sure local provider addresses are up to date.
                        let local_peer_id = self.local_key.clone().into_preimage();
                        if let Some(p) =
                            providers.iter_mut().find(|p| p.peer == local_peer_id).as_mut()
                        {
                            p.addresses = self.service.public_addresses().get_addresses();
                        }

                        let closer_peers = self
                            .routing_table
                            .closest(&Key::new(key.as_ref()), self.replication_factor);

                        let message =
                            KademliaMessage::get_providers_response(providers, &closer_peers);
                        self.executor.send_message(peer, None, message.into(), substream);
                    }
                    (None, None) => tracing::debug!(
                        target: LOG_TARGET,
                        ?peer,
                        ?peers,
                        ?providers,
                        "unable to handle `GET_PROVIDERS` request with empty key",
                    ),
                }
            }
        }

        Ok(())
    }

    /// Failed to open substream to remote peer.
    async fn on_substream_open_failure(
        &mut self,
        substream_id: SubstreamId,
        error: SubstreamError,
    ) {
        tracing::trace!(
            target: LOG_TARGET,
            ?substream_id,
            ?error,
            "failed to open substream"
        );

        let Some(peer) = self.pending_substreams.remove(&substream_id) else {
            tracing::debug!(
                target: LOG_TARGET,
                ?substream_id,
                "outbound substream failed for non-existent peer"
            );
            return;
        };

        if let Some(context) = self.peers.get_mut(&peer) {
            let query =
                context.pending_actions.remove(&substream_id).as_ref().map(PeerAction::query_id);

            self.disconnect_peer(peer, query).await;
        }
    }

    /// Handle dial failure.
    fn on_dial_failure(&mut self, peer: PeerId, addresses: Vec<Multiaddr>) {
        tracing::trace!(target: LOG_TARGET, ?peer, ?addresses, "failed to dial peer");

        self.routing_table.on_dial_failure(Key::from(peer), &addresses);

        let Some(actions) = self.pending_dials.remove(&peer) else {
            return;
        };

        for action in actions {
            let query = action.query_id();

            tracing::trace!(
                target: LOG_TARGET,
                ?peer,
                ?query,
                ?addresses,
                "report failure for pending query",
            );

            // Fail both sending and receiving due to dial failure.
            self.engine.register_send_failure(query, peer);
            self.engine.register_response_failure(query, peer);
        }
    }

    /// Open a substream with a peer or dial the peer.
    fn open_substream_or_dial(
        &mut self,
        peer: PeerId,
        action: PeerAction,
        query: Option<QueryId>,
    ) -> Result<(), Error> {
        match self.service.open_substream(peer) {
            Ok(substream_id) => {
                self.pending_substreams.insert(substream_id, peer);
                self.peers.entry(peer).or_default().pending_actions.insert(substream_id, action);

                Ok(())
            }
            Err(err) => {
                tracing::trace!(target: LOG_TARGET, ?query, ?peer, ?err, "Failed to open substream. Dialing peer");

                match self.service.dial(&peer) {
                    Ok(()) => {
                        self.pending_dials.entry(peer).or_default().push(action);
                        Ok(())
                    }

                    // Already connected is a recoverable error.
                    Err(ImmediateDialError::AlreadyConnected) => {
                        // Dial returned `Error::AlreadyConnected`, retry opening the substream.
                        match self.service.open_substream(peer) {
                            Ok(substream_id) => {
                                self.pending_substreams.insert(substream_id, peer);
                                self.peers
                                    .entry(peer)
                                    .or_default()
                                    .pending_actions
                                    .insert(substream_id, action);
                                Ok(())
                            }
                            Err(err) => {
                                tracing::debug!(target: LOG_TARGET, ?query, ?peer, ?err, "Failed to open substream a second time");
                                Err(err.into())
                            }
                        }
                    }

                    Err(error) => {
                        tracing::trace!(target: LOG_TARGET, ?query, ?peer, ?error, "Failed to dial peer");
                        Err(error.into())
                    }
                }
            }
        }
    }

    /// Handle next query action.
    async fn on_query_action(&mut self, action: QueryAction) -> Result<(), (QueryId, PeerId)> {
        match action {
            QueryAction::SendMessage { query, peer, .. } => {
                // This action is used for `FIND_NODE`, `GET_VALUE` and `GET_PROVIDERS` queries.
                if self
                    .open_substream_or_dial(peer, PeerAction::SendFindNode(query), Some(query))
                    .is_err()
                {
                    // Announce the error to the query engine.
                    self.engine.register_send_failure(query, peer);
                    self.engine.register_response_failure(query, peer);
                }
                Ok(())
            }
            QueryAction::FindNodeQuerySucceeded {
                target,
                peers,
                query,
            } => {
                tracing::debug!(
                    target: LOG_TARGET,
                    ?query,
                    peer = ?target,
                    num_peers = ?peers.len(),
                    "`FIND_NODE` succeeded",
                );

                let _ = self
                    .event_tx
                    .send(KademliaEvent::FindNodeSuccess {
                        target,
                        query_id: query,
                        peers: peers
                            .into_iter()
                            .map(|info| (info.peer, info.addresses()))
                            .collect(),
                    })
                    .await;
                Ok(())
            }
            QueryAction::PutRecordToFoundNodes {
                query,
                record,
                peers,
                quorum,
            } => {
                tracing::trace!(
                    target: LOG_TARGET,
                    ?query,
                    record_key = ?record.key,
                    num_peers = ?peers.len(),
                    "store record to found peers",
                );
                let key = record.key.clone();
                let message: Bytes = KademliaMessage::put_value(record);

                for peer in &peers {
                    if let Err(error) = self.open_substream_or_dial(
                        peer.peer,
                        // `message` is cheaply clonable because of `Bytes` reference counting.
                        PeerAction::SendPutValue(query, message.clone()),
                        None,
                    ) {
                        tracing::debug!(
                            target: LOG_TARGET,
                            ?peer,
                            ?key,
                            ?error,
                            "failed to put record to peer",
                        );
                    }
                }

                self.engine.start_put_record_to_found_nodes_requests_tracking(
                    query,
                    key,
                    peers.into_iter().map(|peer| peer.peer).collect(),
                    quorum,
                );

                Ok(())
            }
            QueryAction::PutRecordQuerySucceeded { query, key } => {
                tracing::debug!(target: LOG_TARGET, ?query, "`PUT_VALUE` query succeeded");

                let _ = self
                    .event_tx
                    .send(KademliaEvent::PutRecordSuccess {
                        query_id: query,
                        key,
                    })
                    .await;
                Ok(())
            }
            QueryAction::AddProviderToFoundNodes {
                query,
                provided_key,
                provider,
                peers,
                quorum,
            } => {
                tracing::trace!(
                    target: LOG_TARGET,
                    ?provided_key,
                    num_peers = ?peers.len(),
                    "add provider record to found peers",
                );

                let message = KademliaMessage::add_provider(provided_key.clone(), provider);

                for peer in &peers {
                    if let Err(error) = self.open_substream_or_dial(
                        peer.peer,
                        PeerAction::SendAddProvider(query, message.clone()),
                        None,
                    ) {
                        tracing::debug!(
                            target: LOG_TARGET,
                            ?peer,
                            ?provided_key,
                            ?error,
                            "failed to add provider record to peer",
                        )
                    }
                }

                self.engine.start_add_provider_to_found_nodes_requests_tracking(
                    query,
                    provided_key,
                    peers.into_iter().map(|peer| peer.peer).collect(),
                    quorum,
                );

                Ok(())
            }
            QueryAction::AddProviderQuerySucceeded {
                query,
                provided_key,
            } => {
                tracing::debug!(target: LOG_TARGET, ?query, "`ADD_PROVIDER` query succeeded");

                let _ = self
                    .event_tx
                    .send(KademliaEvent::AddProviderSuccess {
                        query_id: query,
                        provided_key,
                    })
                    .await;
                Ok(())
            }
            QueryAction::GetRecordQueryDone { query_id } => {
                let _ = self.event_tx.send(KademliaEvent::GetRecordSuccess { query_id }).await;
                Ok(())
            }
            QueryAction::GetProvidersQueryDone {
                query_id,
                provided_key,
                providers,
            } => {
                let _ = self
                    .event_tx
                    .send(KademliaEvent::GetProvidersSuccess {
                        query_id,
                        provided_key,
                        providers,
                    })
                    .await;
                Ok(())
            }
            QueryAction::QueryFailed { query } => {
                tracing::debug!(target: LOG_TARGET, ?query, "query failed");

                let _ = self.event_tx.send(KademliaEvent::QueryFailed { query_id: query }).await;
                Ok(())
            }
            QueryAction::GetRecordPartialResult { query_id, record } => {
                let _ = self
                    .event_tx
                    .send(KademliaEvent::GetRecordPartialResult { query_id, record })
                    .await;
                Ok(())
            }
            QueryAction::QuerySucceeded { .. } => Ok(()),
        }
    }

    /// [`Kademlia`] event loop.
    pub async fn run(mut self) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, "starting kademlia event loop");

        loop {
            // poll `QueryEngine` for next actions.
            while let Some(action) = self.engine.next_action() {
                if let Err((query, peer)) = self.on_query_action(action).await {
                    self.disconnect_peer(peer, Some(query)).await;
                }
            }

            tokio::select! {
                event = self.service.next() => match event {
                    Some(TransportEvent::ConnectionEstablished { peer, endpoint }) => {
                        if let Err(error) = self.on_connection_established(peer, endpoint) {
                            tracing::debug!(
                                target: LOG_TARGET,
                                ?error,
                                "failed to handle established connection",
                            );
                        }
                    }
                    Some(TransportEvent::ConnectionClosed { peer }) => {
                        self.disconnect_peer(peer, None).await;
                    }
                    Some(TransportEvent::SubstreamOpened { peer, direction, substream, .. }) => {
                        match direction {
                            Direction::Inbound => self.on_inbound_substream(peer, substream).await,
                            Direction::Outbound(substream_id) => {
                                if let Err(error) = self
                                    .on_outbound_substream(peer, substream_id, substream)
                                    .await
                                {
                                    tracing::debug!(
                                        target: LOG_TARGET,
                                        ?peer,
                                        ?substream_id,
                                        ?error,
                                        "failed to handle outbound substream",
                                    );
                                }
                            }
                        }
                    },
                    Some(TransportEvent::SubstreamOpenFailure { substream, error }) => {
                        self.on_substream_open_failure(substream, error).await;
                    }
                    Some(TransportEvent::DialFailure { peer, addresses }) =>
                        self.on_dial_failure(peer, addresses),
                    None => return Err(Error::EssentialTaskClosed),
                },
                context = self.executor.next() => {
                    let QueryContext { peer, query_id, result } = context.unwrap();

                    match result {
                        QueryResult::SendSuccess { substream } => {
                            tracing::trace!(
                                target: LOG_TARGET,
                                ?peer,
                                query = ?query_id,
                                "message sent to peer",
                            );
                            let _ = substream.close().await;

                            if let Some(query_id) = query_id {
                                self.engine.register_send_success(query_id, peer);
                            }
                        }
                        // This is a workaround to gracefully handle older litep2p nodes not
                        // sending/receiving `PUT_VALUE` ACKs. This should eventually be removed.
                        // TODO: remove this as part of
                        // https://github.com/paritytech/litep2p/issues/429.
                        QueryResult::AssumeSendSuccess => {
                            tracing::trace!(
                                target: LOG_TARGET,
                                ?peer,
                                query = ?query_id,
                                "treating message as sent to peer",
                            );

                            if let Some(query_id) = query_id {
                                self.engine.register_send_success(query_id, peer);
                            }
                        }
                        QueryResult::SendFailure { reason } => {
                            tracing::debug!(
                                target: LOG_TARGET,
                                ?peer,
                                query = ?query_id,
                                ?reason,
                                "failed to send message to peer",
                            );

                            self.disconnect_peer(peer, query_id).await;
                        }
                        QueryResult::ReadSuccess { substream, message } => {
                            tracing::trace!(
                                target: LOG_TARGET,
                                ?peer,
                                query = ?query_id,
                                "message read from peer",
                            );

                            if let Some(query_id) = query_id {
                                // Read success for locally originating requests implies send
                                // success.
                                self.engine.register_send_success(query_id, peer);
                            }

                            if let Err(error) = self.on_message_received(
                                peer,
                                query_id,
                                message,
                                substream
                            ).await {
                                tracing::debug!(
                                    target: LOG_TARGET,
                                    ?peer,
                                    ?error,
                                    "failed to process message",
                                );
                            }
                        }
                        QueryResult::ReadFailure { reason } => {
                            tracing::debug!(
                                target: LOG_TARGET,
                                ?peer,
                                query = ?query_id,
                                ?reason,
                                "failed to read message from substream",
                            );

                            self.disconnect_peer(peer, query_id).await;
                        }
                    }
                },
                command = self.cmd_rx.recv() => {
                    match command {
                        Some(KademliaCommand::FindNode { peer, query_id }) => {
                            tracing::debug!(
                                target: LOG_TARGET,
                                ?peer,
                                query = ?query_id,
                                "starting `FIND_NODE` query",
                            );

                            self.engine.start_find_node(
                                query_id,
                                peer,
                                self.routing_table
                                    .closest(&Key::from(peer), self.replication_factor)
                                    .into()
                            );
                        }
                        Some(KademliaCommand::PutRecord { mut record, quorum, query_id }) => {
                            tracing::debug!(
                                target: LOG_TARGET,
                                query = ?query_id,
                                key = ?record.key,
                                "store record to DHT",
                            );

                            // For `PUT_VALUE` requests originating locally we are always the
                            // publisher.
                            record.publisher = Some(self.local_key.clone().into_preimage());

                            // Make sure TTL is set.
                            record.expires = record
                                .expires
                                .or_else(|| Some(Instant::now() + self.record_ttl));

                            let key = Key::new(record.key.clone());

                            self.store.put(record.clone());

                            self.engine.start_put_record(
                                query_id,
                                record,
                                self.routing_table.closest(&key, self.replication_factor).into(),
                                quorum,
                            );
                        }
                        Some(KademliaCommand::PutRecordToPeers {
                            mut record,
                            query_id,
                            peers,
                            update_local_store,
                            quorum,
                        }) => {
                            tracing::debug!(
                                target: LOG_TARGET,
                                query = ?query_id,
                                key = ?record.key,
                                "store record to DHT to specified peers",
                            );

                            // Make sure TTL is set.
                            record.expires = record
                                .expires
                                .or_else(|| Some(Instant::now() + self.record_ttl));

                            if update_local_store {
                                self.store.put(record.clone());
                            }

                            // Put the record to the specified peers.
                            let peers = peers.into_iter().filter_map(|peer| {
                                if peer == self.service.local_peer_id() {
                                    return None;
                                }

                                match self.routing_table.entry(Key::from(peer)) {
                                    KBucketEntry::Occupied(entry) => Some(entry.clone()),
                                    KBucketEntry::Vacant(entry) if !entry.address_store.is_empty() =>
                                        Some(entry.clone()),
                                    _ => None,
                                }
                            }).collect();

                            self.engine.start_put_record_to_peers(
                                query_id,
                                record,
                                peers,
                                quorum,
                            );
                        }
                        Some(KademliaCommand::StartProviding {
                            key,
                            quorum,
                            query_id
                        }) => {
                            tracing::debug!(
                                target: LOG_TARGET,
                                query = ?query_id,
                                ?key,
                                "register as a content provider",
                            );

                            let addresses = self.service.public_addresses().get_addresses();
                            let provider = ContentProvider {
                                peer: self.service.local_peer_id(),
                                addresses,
                            };

                            self.store.put_local_provider(key.clone(), quorum);

                            self.engine.start_add_provider(
                                query_id,
                                key.clone(),
                                provider,
                                self.routing_table
                                    .closest(&Key::new(key), self.replication_factor)
                                    .into(),
                                quorum,
                            );
                        }
                        Some(KademliaCommand::StopProviding {
                            key,
                        }) => {
                            tracing::debug!(
                                target: LOG_TARGET,
                                ?key,
                                "stop providing",
                            );

                            self.store.remove_local_provider(key);
                        }
                        Some(KademliaCommand::GetRecord { key, quorum, query_id }) => {
                            tracing::debug!(target: LOG_TARGET, ?key, "get record from DHT");

                            match (self.store.get(&key), quorum) {
                                (Some(record), Quorum::One) => {
                                    let _ = self
                                        .event_tx
                                        .send(KademliaEvent::GetRecordPartialResult { query_id, record: PeerRecord {
                                            peer: self.service.local_peer_id(),
                                            record: record.clone(),
                                        } })
                                        .await;

                                    let _ = self
                                        .event_tx
                                        .send(KademliaEvent::GetRecordSuccess {
                                            query_id,
                                        })
                                        .await;
                                }
                                (record, _) => {
                                    let local_record = record.is_some();
                                    if let Some(record) = record {
                                        let _ = self
                                            .event_tx
                                            .send(KademliaEvent::GetRecordPartialResult { query_id, record: PeerRecord {
                                                peer: self.service.local_peer_id(),
                                                record: record.clone(),
                                            } })
                                            .await;
                                    }

                                    self.engine.start_get_record(
                                        query_id,
                                        key.clone(),
                                        self.routing_table
                                            .closest(&Key::new(key), self.replication_factor)
                                            .into(),
                                        quorum,
                                        local_record,
                                    );
                                }
                            }

                        }
                        Some(KademliaCommand::GetProviders { key, query_id }) => {
                            tracing::debug!(target: LOG_TARGET, ?key, "get providers from DHT");

                            let known_providers = self.store.get_providers(&key);

                            self.engine.start_get_providers(
                                query_id,
                                key.clone(),
                                self.routing_table
                                    .closest(&Key::new(key), self.replication_factor)
                                    .into(),
                                known_providers,
                            );
                        }
                        Some(KademliaCommand::AddKnownPeer { peer, addresses }) => {
                            tracing::trace!(
                                target: LOG_TARGET,
                                ?peer,
                                ?addresses,
                                "add known peer",
                            );

                            self.routing_table.add_known_peer(
                                peer,
                                addresses.clone(),
                                self.peers
                                    .get(&peer)
                                    .map_or(
                                        ConnectionType::NotConnected,
                                        |_| ConnectionType::Connected,
                                    ),
                            );
                            self.service.add_known_address(&peer, addresses.into_iter());

                        }
                        Some(KademliaCommand::StoreRecord { mut record }) => {
                            tracing::debug!(
                                target: LOG_TARGET,
                                key = ?record.key,
                                "store record in local store",
                            );

                            // Make sure TTL is set.
                            record.expires =
                                record.expires.or_else(|| Some(Instant::now() + self.record_ttl));

                            self.store.put(record);
                        }
                        None => return Err(Error::EssentialTaskClosed),
                    }
                },
                action = self.store.next_action() => match action {
                    Some(MemoryStoreAction::RefreshProvider { provided_key, provider, quorum }) => {
                        tracing::trace!(
                            target: LOG_TARGET,
                            ?provided_key,
                            "republishing local provider",
                        );

                        self.store.put_local_provider(provided_key.clone(), quorum);

                        // We never update local provider addresses in the store during refresh,
                        // as this is done anyway when replying to `GET_PROVIDERS` request.

                        let query_id = self.next_query_id();
                        self.engine.start_add_provider(
                            query_id,
                            provided_key.clone(),
                            provider,
                            self.routing_table
                                .closest(&Key::new(provided_key), self.replication_factor)
                                .into(),
                            quorum,
                        );
                    }
                    None => {}
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
        transport::{
            manager::{TransportManager, TransportManagerBuilder},
            KEEP_ALIVE_TIMEOUT,
        },
        types::protocol::ProtocolName,
        ConnectionId,
    };
    use multiaddr::Protocol;
    use multihash::Multihash;
    use std::str::FromStr;
    use tokio::sync::mpsc::channel;

    #[allow(unused)]
    struct Context {
        _cmd_tx: Sender<KademliaCommand>,
        event_rx: Receiver<KademliaEvent>,
    }

    fn make_kademlia() -> (Kademlia, Context, TransportManager) {
        let manager = TransportManagerBuilder::new().build();

        let peer = PeerId::random();
        let (transport_service, _tx) = TransportService::new(
            peer,
            ProtocolName::from("/kad/1"),
            Vec::new(),
            Default::default(),
            manager.transport_manager_handle(),
            KEEP_ALIVE_TIMEOUT,
        );
        let (event_tx, event_rx) = channel(64);
        let (_cmd_tx, cmd_rx) = channel(64);
        let next_query_id = Arc::new(AtomicUsize::new(0usize));

        let config = Config {
            protocol_names: vec![ProtocolName::from("/kad/1")],
            known_peers: HashMap::new(),
            codec: ProtocolCodec::UnsignedVarint(Some(70 * 1024)),
            replication_factor: 20usize,
            update_mode: RoutingTableUpdateMode::Automatic,
            validation_mode: IncomingRecordValidationMode::Automatic,
            record_ttl: Duration::from_secs(36 * 60 * 60),
            memory_store_config: Default::default(),
            event_tx,
            cmd_rx,
            next_query_id,
        };

        (
            Kademlia::new(transport_service, config),
            Context { _cmd_tx, event_rx },
            manager,
        )
    }

    #[tokio::test]
    async fn check_get_records_update() {
        let (mut kademlia, _context, _manager) = make_kademlia();

        let key = RecordKey::from(vec![1, 2, 3]);
        let records = vec![
            // 2 peers backing the same record.
            PeerRecord {
                peer: PeerId::random(),
                record: Record::new(key.clone(), vec![0x1]),
            },
            PeerRecord {
                peer: PeerId::random(),
                record: Record::new(key.clone(), vec![0x1]),
            },
            // only 1 peer backing the record.
            PeerRecord {
                peer: PeerId::random(),
                record: Record::new(key.clone(), vec![0x2]),
            },
        ];

        for record in records {
            let action = QueryAction::GetRecordPartialResult {
                query_id: QueryId(1),
                record,
            };
            assert!(kademlia.on_query_action(action).await.is_ok());
        }

        let query_id = QueryId(1);
        let action = QueryAction::GetRecordQueryDone { query_id };
        assert!(kademlia.on_query_action(action).await.is_ok());

        // Check the local storage should not get updated.
        assert!(kademlia.store.get(&key).is_none());
    }

    #[tokio::test]
    async fn check_get_records_update_with_expired_records() {
        let (mut kademlia, _context, _manager) = make_kademlia();

        let key = RecordKey::from(vec![1, 2, 3]);
        let expired = std::time::Instant::now() - std::time::Duration::from_secs(10);
        let records = vec![
            // 2 peers backing the same record, one record is expired.
            PeerRecord {
                peer: PeerId::random(),
                record: Record {
                    key: key.clone(),
                    value: vec![0x1],
                    publisher: None,
                    expires: Some(expired),
                },
            },
            PeerRecord {
                peer: PeerId::random(),
                record: Record::new(key.clone(), vec![0x1]),
            },
            // 2 peer backing the record.
            PeerRecord {
                peer: PeerId::random(),
                record: Record::new(key.clone(), vec![0x2]),
            },
            PeerRecord {
                peer: PeerId::random(),
                record: Record::new(key.clone(), vec![0x2]),
            },
        ];

        for record in records {
            let action = QueryAction::GetRecordPartialResult {
                query_id: QueryId(1),
                record,
            };
            assert!(kademlia.on_query_action(action).await.is_ok());
        }

        kademlia
            .on_query_action(QueryAction::GetRecordQueryDone {
                query_id: QueryId(1),
            })
            .await
            .unwrap();

        // Check the local storage should not get updated.
        assert!(kademlia.store.get(&key).is_none());
    }

    #[tokio::test]
    async fn check_address_store_routing_table_updates() {
        let (mut kademlia, _context, _manager) = make_kademlia();

        let peer = PeerId::random();
        let address_a = Multiaddr::from_str("/dns/domain1.com/tcp/30333").unwrap().with(
            Protocol::P2p(Multihash::from_bytes(&peer.to_bytes()).unwrap()),
        );
        let address_b = Multiaddr::from_str("/dns/domain1.com/tcp/30334").unwrap().with(
            Protocol::P2p(Multihash::from_bytes(&peer.to_bytes()).unwrap()),
        );
        let address_c = Multiaddr::from_str("/dns/domain1.com/tcp/30339").unwrap().with(
            Protocol::P2p(Multihash::from_bytes(&peer.to_bytes()).unwrap()),
        );

        // Added only with address a.
        kademlia.routing_table.add_known_peer(
            peer,
            vec![address_a.clone()],
            ConnectionType::NotConnected,
        );

        // Check peer addresses.
        match kademlia.routing_table.entry(Key::from(peer)) {
            KBucketEntry::Occupied(entry) => {
                assert_eq!(entry.addresses(), vec![address_a.clone()]);
            }
            _ => panic!("Peer not found in routing table"),
        };

        // Report successful connection with address b via dialer endpoint.
        let _ = kademlia.on_connection_established(
            peer,
            Endpoint::Dialer {
                address: address_b.clone(),
                connection_id: ConnectionId::from(0),
            },
        );

        // Address B has a higher priority, as it was detected via the dialing mechanism of the
        // transport manager, while address A is not dialed yet.
        match kademlia.routing_table.entry(Key::from(peer)) {
            KBucketEntry::Occupied(entry) => {
                assert_eq!(
                    entry.addresses(),
                    vec![address_b.clone(), address_a.clone()]
                );
            }
            _ => panic!("Peer not found in routing table"),
        };

        // Report successful connection with a random address via listener endpoint.
        let _ = kademlia.on_connection_established(
            peer,
            Endpoint::Listener {
                address: address_c.clone(),
                connection_id: ConnectionId::from(0),
            },
        );
        // Address C was not added, as the peer has dialed us possibly on an ephemeral port.
        match kademlia.routing_table.entry(Key::from(peer)) {
            KBucketEntry::Occupied(entry) => {
                assert_eq!(
                    entry.addresses(),
                    vec![address_b.clone(), address_a.clone()]
                );
            }
            _ => panic!("Peer not found in routing table"),
        };

        // Address B fails two times (which gives it a lower score than A) and
        // makes it subject to removal.
        kademlia.on_dial_failure(peer, vec![address_b.clone(), address_b.clone()]);

        match kademlia.routing_table.entry(Key::from(peer)) {
            KBucketEntry::Occupied(entry) => {
                assert_eq!(
                    entry.addresses(),
                    vec![address_a.clone(), address_b.clone()]
                );
            }
            _ => panic!("Peer not found in routing table"),
        };
    }
}
