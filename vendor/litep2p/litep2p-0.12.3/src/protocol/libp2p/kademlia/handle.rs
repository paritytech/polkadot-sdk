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
    protocol::libp2p::kademlia::{ContentProvider, PeerRecord, QueryId, Record, RecordKey},
    PeerId,
};

use futures::Stream;
use multiaddr::Multiaddr;
use tokio::sync::mpsc::{Receiver, Sender};

use std::{
    num::NonZeroUsize,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
};

/// Quorum.
///
/// Quorum defines how many peers must be successfully contacted
/// in order for the query to be considered successful.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "fuzz", derive(serde::Serialize, serde::Deserialize))]
pub enum Quorum {
    /// All peers must be successfully contacted.
    All,

    /// One peer must be successfully contacted.
    One,

    /// `N` peers must be successfully contacted.
    N(NonZeroUsize),
}

/// Routing table update mode.
#[derive(Debug, Copy, Clone)]
pub enum RoutingTableUpdateMode {
    /// Don't insert discovered peers automatically to the routing tables but
    /// allow user to do that by calling [`KademliaHandle::add_known_peer()`].
    Manual,

    /// Automatically add all discovered peers to routing tables.
    Automatic,
}

/// Incoming record validation mode.
#[derive(Debug, Copy, Clone)]
pub enum IncomingRecordValidationMode {
    /// Don't insert incoming records automatically to the local DHT store
    /// and let the user do that by calling [`KademliaHandle::store_record()`].
    Manual,

    /// Automatically accept all incoming records.
    Automatic,
}

/// Kademlia commands.
#[derive(Debug)]
#[cfg_attr(feature = "fuzz", derive(serde::Serialize, serde::Deserialize))]
pub enum KademliaCommand {
    /// Add known peer.
    AddKnownPeer {
        /// Peer ID.
        peer: PeerId,

        /// Addresses of peer.
        addresses: Vec<Multiaddr>,
    },

    /// Send `FIND_NODE` message.
    FindNode {
        /// Peer ID.
        peer: PeerId,

        /// Query ID for the query.
        query_id: QueryId,
    },

    /// Store record to DHT.
    PutRecord {
        /// Record.
        record: Record,

        /// [`Quorum`] for the query.
        quorum: Quorum,

        /// Query ID for the query.
        query_id: QueryId,
    },

    /// Store record to DHT to the given peers.
    ///
    /// Similar to [`KademliaCommand::PutRecord`] but allows user to specify the peers.
    PutRecordToPeers {
        /// Record.
        record: Record,

        /// [`Quorum`] for the query.
        quorum: Quorum,

        /// Query ID for the query.
        query_id: QueryId,

        /// Use the following peers for the put request.
        peers: Vec<PeerId>,

        /// Update local store.
        update_local_store: bool,
    },

    /// Get record from DHT.
    GetRecord {
        /// Record key.
        key: RecordKey,

        /// [`Quorum`] for the query.
        quorum: Quorum,

        /// Query ID for the query.
        query_id: QueryId,
    },

    /// Get providers from DHT.
    GetProviders {
        /// Provided key.
        key: RecordKey,

        /// Query ID for the query.
        query_id: QueryId,
    },

    /// Register as a content provider for `key`.
    StartProviding {
        /// Provided key.
        key: RecordKey,

        /// [`Quorum`] for the query.
        quorum: Quorum,

        /// Query ID for the query.
        query_id: QueryId,
    },

    /// Stop providing the key locally and refreshing the provider.
    StopProviding {
        /// Provided key.
        key: RecordKey,
    },

    /// Store record locally.
    StoreRecord {
        // Record.
        record: Record,
    },
}

/// Kademlia events.
#[derive(Debug, Clone)]
pub enum KademliaEvent {
    /// Result for the issued `FIND_NODE` query.
    FindNodeSuccess {
        /// Query ID.
        query_id: QueryId,

        /// Target of the query
        target: PeerId,

        /// Found nodes and their addresses.
        peers: Vec<(PeerId, Vec<Multiaddr>)>,
    },

    /// Routing table update.
    ///
    /// Kademlia has discovered one or more peers that should be added to the routing table.
    /// If [`RoutingTableUpdateMode`] is `Automatic`, user can ignore this event unless some
    /// upper-level protocols has user for this information.
    ///
    /// If the mode was set to `Manual`, user should call [`KademliaHandle::add_known_peer()`]
    /// in order to add the peers to routing table.
    RoutingTableUpdate {
        /// Discovered peers.
        peers: Vec<PeerId>,
    },

    /// `GET_VALUE` query succeeded.
    GetRecordSuccess {
        /// Query ID.
        query_id: QueryId,
    },

    /// `GET_VALUE` inflight query produced a result.
    ///
    /// This event is emitted when a peer responds to the query with a record.
    GetRecordPartialResult {
        /// Query ID.
        query_id: QueryId,

        /// Found record.
        record: PeerRecord,
    },

    /// `GET_PROVIDERS` query succeeded.
    GetProvidersSuccess {
        /// Query ID.
        query_id: QueryId,

        /// Provided key.
        provided_key: RecordKey,

        /// Found providers with cached addresses. Returned providers are sorted by distane to the
        /// provided key.
        providers: Vec<ContentProvider>,
    },

    /// `PUT_VALUE` query succeeded.
    PutRecordSuccess {
        /// Query ID.
        query_id: QueryId,

        /// Record key.
        key: RecordKey,
    },

    /// `ADD_PROVIDER` query succeeded.
    AddProviderSuccess {
        /// Query ID.
        query_id: QueryId,

        /// Provided key.
        provided_key: RecordKey,
    },

    /// Query failed.
    QueryFailed {
        /// Query ID.
        query_id: QueryId,
    },

    /// Incoming `PUT_VALUE` request received.
    ///
    /// In case of using [`IncomingRecordValidationMode::Manual`] and successful validation
    /// the record must be manually inserted into the local DHT store with
    /// [`KademliaHandle::store_record()`].
    IncomingRecord {
        /// Record.
        record: Record,
    },

    /// Incoming `ADD_PROVIDER` request received.
    IncomingProvider {
        /// Provided key.
        provided_key: RecordKey,

        /// Provider.
        provider: ContentProvider,
    },
}

/// Handle for communicating with the Kademlia protocol.
pub struct KademliaHandle {
    /// TX channel for sending commands to `Kademlia`.
    cmd_tx: Sender<KademliaCommand>,

    /// RX channel for receiving events from `Kademlia`.
    event_rx: Receiver<KademliaEvent>,

    /// Next query ID.
    next_query_id: Arc<AtomicUsize>,
}

impl KademliaHandle {
    /// Create new [`KademliaHandle`].
    pub(super) fn new(
        cmd_tx: Sender<KademliaCommand>,
        event_rx: Receiver<KademliaEvent>,
        next_query_id: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            cmd_tx,
            event_rx,
            next_query_id,
        }
    }

    /// Allocate next query ID.
    fn next_query_id(&mut self) -> QueryId {
        let query_id = self.next_query_id.fetch_add(1, Ordering::Relaxed);

        QueryId(query_id)
    }

    /// Add known peer.
    pub async fn add_known_peer(&self, peer: PeerId, addresses: Vec<Multiaddr>) {
        let _ = self.cmd_tx.send(KademliaCommand::AddKnownPeer { peer, addresses }).await;
    }

    /// Send `FIND_NODE` query to known peers.
    pub async fn find_node(&mut self, peer: PeerId) -> QueryId {
        let query_id = self.next_query_id();
        let _ = self.cmd_tx.send(KademliaCommand::FindNode { peer, query_id }).await;

        query_id
    }

    /// Store record to DHT.
    pub async fn put_record(&mut self, record: Record, quorum: Quorum) -> QueryId {
        let query_id = self.next_query_id();
        let _ = self
            .cmd_tx
            .send(KademliaCommand::PutRecord {
                record,
                quorum,
                query_id,
            })
            .await;

        query_id
    }

    /// Store record to DHT to the given peers.
    ///
    /// Returns [`Err`] only if `Kademlia` is terminating.
    pub async fn put_record_to_peers(
        &mut self,
        record: Record,
        peers: Vec<PeerId>,
        update_local_store: bool,
        quorum: Quorum,
    ) -> QueryId {
        let query_id = self.next_query_id();
        let _ = self
            .cmd_tx
            .send(KademliaCommand::PutRecordToPeers {
                record,
                query_id,
                peers,
                update_local_store,
                quorum,
            })
            .await;

        query_id
    }

    /// Get record from DHT.
    ///
    /// Returns [`Err`] only if `Kademlia` is terminating.
    pub async fn get_record(&mut self, key: RecordKey, quorum: Quorum) -> QueryId {
        let query_id = self.next_query_id();
        let _ = self
            .cmd_tx
            .send(KademliaCommand::GetRecord {
                key,
                quorum,
                query_id,
            })
            .await;

        query_id
    }

    /// Register as a content provider on the DHT.
    ///
    /// Register the local peer ID & its `public_addresses` as a provider for a given `key`.
    /// Returns [`Err`] only if `Kademlia` is terminating.
    pub async fn start_providing(&mut self, key: RecordKey, quorum: Quorum) -> QueryId {
        let query_id = self.next_query_id();
        let _ = self
            .cmd_tx
            .send(KademliaCommand::StartProviding {
                key,
                quorum,
                query_id,
            })
            .await;

        query_id
    }

    /// Stop providing the key on the DHT.
    ///
    /// This will stop republishing the provider, but won't
    /// remove it instantly from the nodes. It will be removed from them after the provider TTL
    /// expires, set by default to 48 hours.
    pub async fn stop_providing(&mut self, key: RecordKey) {
        let _ = self.cmd_tx.send(KademliaCommand::StopProviding { key }).await;
    }

    /// Get providers from DHT.
    ///
    /// Returns [`Err`] only if `Kademlia` is terminating.
    pub async fn get_providers(&mut self, key: RecordKey) -> QueryId {
        let query_id = self.next_query_id();
        let _ = self.cmd_tx.send(KademliaCommand::GetProviders { key, query_id }).await;

        query_id
    }

    /// Store the record in the local store. Used in combination with
    /// [`IncomingRecordValidationMode::Manual`].
    pub async fn store_record(&mut self, record: Record) {
        let _ = self.cmd_tx.send(KademliaCommand::StoreRecord { record }).await;
    }

    /// Try to add known peer and if the channel is clogged, return an error.
    pub fn try_add_known_peer(&self, peer: PeerId, addresses: Vec<Multiaddr>) -> Result<(), ()> {
        self.cmd_tx
            .try_send(KademliaCommand::AddKnownPeer { peer, addresses })
            .map_err(|_| ())
    }

    /// Try to initiate `FIND_NODE` query and if the channel is clogged, return an error.
    pub fn try_find_node(&mut self, peer: PeerId) -> Result<QueryId, ()> {
        let query_id = self.next_query_id();
        self.cmd_tx
            .try_send(KademliaCommand::FindNode { peer, query_id })
            .map(|_| query_id)
            .map_err(|_| ())
    }

    /// Try to initiate `PUT_VALUE` query and if the channel is clogged, return an error.
    pub fn try_put_record(&mut self, record: Record, quorum: Quorum) -> Result<QueryId, ()> {
        let query_id = self.next_query_id();
        self.cmd_tx
            .try_send(KademliaCommand::PutRecord {
                record,
                query_id,
                quorum,
            })
            .map(|_| query_id)
            .map_err(|_| ())
    }

    /// Try to initiate `PUT_VALUE` query to the given peers and if the channel is clogged,
    /// return an error.
    pub fn try_put_record_to_peers(
        &mut self,
        record: Record,
        peers: Vec<PeerId>,
        update_local_store: bool,
        quorum: Quorum,
    ) -> Result<QueryId, ()> {
        let query_id = self.next_query_id();
        self.cmd_tx
            .try_send(KademliaCommand::PutRecordToPeers {
                record,
                query_id,
                peers,
                update_local_store,
                quorum,
            })
            .map(|_| query_id)
            .map_err(|_| ())
    }

    /// Try to initiate `GET_VALUE` query and if the channel is clogged, return an error.
    pub fn try_get_record(&mut self, key: RecordKey, quorum: Quorum) -> Result<QueryId, ()> {
        let query_id = self.next_query_id();
        self.cmd_tx
            .try_send(KademliaCommand::GetRecord {
                key,
                quorum,
                query_id,
            })
            .map(|_| query_id)
            .map_err(|_| ())
    }

    /// Try to store the record in the local store, and if the channel is clogged, return an error.
    /// Used in combination with [`IncomingRecordValidationMode::Manual`].
    pub fn try_store_record(&mut self, record: Record) -> Result<(), ()> {
        self.cmd_tx.try_send(KademliaCommand::StoreRecord { record }).map_err(|_| ())
    }

    #[cfg(feature = "fuzz")]
    /// Expose functionality for fuzzing
    pub async fn fuzz_send_message(&mut self, command: KademliaCommand) -> crate::Result<()> {
        let _ = self.cmd_tx.send(command).await;
        Ok(())
    }
}

impl Stream for KademliaHandle {
    type Item = KademliaEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.event_rx.poll_recv(cx)
    }
}
