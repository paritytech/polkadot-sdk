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
    codec::ProtocolCodec,
    protocol::libp2p::kademlia::{
        handle::{
            IncomingRecordValidationMode, KademliaCommand, KademliaEvent, KademliaHandle,
            RoutingTableUpdateMode,
        },
        store::MemoryStoreConfig,
    },
    types::protocol::ProtocolName,
    PeerId, DEFAULT_CHANNEL_SIZE,
};

use multiaddr::Multiaddr;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use std::{
    collections::HashMap,
    sync::{atomic::AtomicUsize, Arc},
    time::Duration,
};

/// Default TTL for the records.
const DEFAULT_TTL: Duration = Duration::from_secs(36 * 60 * 60);

/// Default max number of records.
pub(super) const DEFAULT_MAX_RECORDS: usize = 1024;

/// Default max record size.
pub(super) const DEFAULT_MAX_RECORD_SIZE_BYTES: usize = 65 * 1024;

/// Default max provider keys.
pub(super) const DEFAULT_MAX_PROVIDER_KEYS: usize = 1024;

/// Default max provider addresses.
pub(super) const DEFAULT_MAX_PROVIDER_ADDRESSES: usize = 30;

/// Default max providers per key.
pub(super) const DEFAULT_MAX_PROVIDERS_PER_KEY: usize = 20;

/// Default provider republish interval.
pub(super) const DEFAULT_PROVIDER_REFRESH_INTERVAL: Duration = Duration::from_secs(22 * 60 * 60);

/// Default provider record TTL.
pub(super) const DEFAULT_PROVIDER_TTL: Duration = Duration::from_secs(48 * 60 * 60);

/// Protocol name.
const PROTOCOL_NAME: &str = "/ipfs/kad/1.0.0";

/// Kademlia replication factor.
const REPLICATION_FACTOR: usize = 20usize;

/// Kademlia maximum message size. Should fit 64 KiB value + 4 KiB key.
const DEFAULT_MAX_MESSAGE_SIZE: usize = 70 * 1024;

/// Kademlia configuration.
#[derive(Debug)]
pub struct Config {
    // Protocol name.
    // pub(crate) protocol: ProtocolName,
    /// Protocol names.
    pub(crate) protocol_names: Vec<ProtocolName>,

    /// Protocol codec.
    pub(crate) codec: ProtocolCodec,

    /// Replication factor.
    pub(super) replication_factor: usize,

    /// Known peers.
    pub(super) known_peers: HashMap<PeerId, Vec<Multiaddr>>,

    /// Routing table update mode.
    pub(super) update_mode: RoutingTableUpdateMode,

    /// Incoming records validation mode.
    pub(super) validation_mode: IncomingRecordValidationMode,

    /// Default record TTL.
    pub(super) record_ttl: Duration,

    /// Provider record TTL.
    pub(super) memory_store_config: MemoryStoreConfig,

    /// TX channel for sending events to `KademliaHandle`.
    pub(super) event_tx: Sender<KademliaEvent>,

    /// RX channel for receiving commands from `KademliaHandle`.
    pub(super) cmd_rx: Receiver<KademliaCommand>,

    /// Next query ID counter shared with the handle.
    pub(super) next_query_id: Arc<AtomicUsize>,
}

impl Config {
    fn new(
        replication_factor: usize,
        known_peers: HashMap<PeerId, Vec<Multiaddr>>,
        mut protocol_names: Vec<ProtocolName>,
        update_mode: RoutingTableUpdateMode,
        validation_mode: IncomingRecordValidationMode,
        record_ttl: Duration,
        memory_store_config: MemoryStoreConfig,
        max_message_size: usize,
    ) -> (Self, KademliaHandle) {
        let (cmd_tx, cmd_rx) = channel(DEFAULT_CHANNEL_SIZE);
        let (event_tx, event_rx) = channel(DEFAULT_CHANNEL_SIZE);
        let next_query_id = Arc::new(AtomicUsize::new(0usize));

        // if no protocol names were provided, use the default protocol
        if protocol_names.is_empty() {
            protocol_names.push(ProtocolName::from(PROTOCOL_NAME));
        }

        (
            Config {
                protocol_names,
                update_mode,
                validation_mode,
                record_ttl,
                memory_store_config,
                codec: ProtocolCodec::UnsignedVarint(Some(max_message_size)),
                replication_factor,
                known_peers,
                cmd_rx,
                event_tx,
                next_query_id: next_query_id.clone(),
            },
            KademliaHandle::new(cmd_tx, event_rx, next_query_id),
        )
    }

    /// Build default Kademlia configuration.
    pub fn default() -> (Self, KademliaHandle) {
        Self::new(
            REPLICATION_FACTOR,
            HashMap::new(),
            Vec::new(),
            RoutingTableUpdateMode::Automatic,
            IncomingRecordValidationMode::Automatic,
            DEFAULT_TTL,
            Default::default(),
            DEFAULT_MAX_MESSAGE_SIZE,
        )
    }
}

/// Configuration builder for Kademlia.
#[derive(Debug)]
pub struct ConfigBuilder {
    /// Replication factor.
    pub(super) replication_factor: usize,

    /// Routing table update mode.
    pub(super) update_mode: RoutingTableUpdateMode,

    /// Incoming records validation mode.
    pub(super) validation_mode: IncomingRecordValidationMode,

    /// Known peers.
    pub(super) known_peers: HashMap<PeerId, Vec<Multiaddr>>,

    /// Protocol names.
    pub(super) protocol_names: Vec<ProtocolName>,

    /// Default TTL for the records.
    pub(super) record_ttl: Duration,

    /// Memory store configuration.
    pub(super) memory_store_config: MemoryStoreConfig,

    /// Maximum message size.
    pub(crate) max_message_size: usize,
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigBuilder {
    /// Create new [`ConfigBuilder`].
    pub fn new() -> Self {
        Self {
            replication_factor: REPLICATION_FACTOR,
            known_peers: HashMap::new(),
            protocol_names: Vec::new(),
            update_mode: RoutingTableUpdateMode::Automatic,
            validation_mode: IncomingRecordValidationMode::Automatic,
            record_ttl: DEFAULT_TTL,
            memory_store_config: Default::default(),
            max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
        }
    }

    /// Set replication factor.
    pub fn with_replication_factor(mut self, replication_factor: usize) -> Self {
        self.replication_factor = replication_factor;
        self
    }

    /// Seed Kademlia with one or more known peers.
    pub fn with_known_peers(mut self, peers: HashMap<PeerId, Vec<Multiaddr>>) -> Self {
        self.known_peers = peers;
        self
    }

    /// Set routing table update mode.
    pub fn with_routing_table_update_mode(mut self, mode: RoutingTableUpdateMode) -> Self {
        self.update_mode = mode;
        self
    }

    /// Set incoming records validation mode.
    pub fn with_incoming_records_validation_mode(
        mut self,
        mode: IncomingRecordValidationMode,
    ) -> Self {
        self.validation_mode = mode;
        self
    }

    /// Set Kademlia protocol names, overriding the default protocol name.
    ///
    /// The order of the protocol names signifies preference so if, for example, there are two
    /// protocols:
    ///  * `/kad/2.0.0`
    ///  * `/kad/1.0.0`
    ///
    /// Where `/kad/2.0.0` is the preferred version, then that should be in `protocol_names` before
    /// `/kad/1.0.0`.
    pub fn with_protocol_names(mut self, protocol_names: Vec<ProtocolName>) -> Self {
        self.protocol_names = protocol_names;
        self
    }

    /// Set default TTL for the records.
    ///
    /// If unspecified, the default TTL is 36 hours.
    pub fn with_record_ttl(mut self, record_ttl: Duration) -> Self {
        self.record_ttl = record_ttl;
        self
    }

    /// Set maximum number of records in the memory store.
    ///
    /// If unspecified, the default maximum number of records is 1024.
    pub fn with_max_records(mut self, max_records: usize) -> Self {
        self.memory_store_config.max_records = max_records;
        self
    }

    /// Set maximum record size in bytes.
    ///
    /// If unspecified, the default maximum record size is 65 KiB.
    pub fn with_max_record_size(mut self, max_record_size_bytes: usize) -> Self {
        self.memory_store_config.max_record_size_bytes = max_record_size_bytes;
        self
    }

    /// Set maximum number of provider keys in the memory store.
    ///
    /// If unspecified, the default maximum number of provider keys is 1024.
    pub fn with_max_provider_keys(mut self, max_provider_keys: usize) -> Self {
        self.memory_store_config.max_provider_keys = max_provider_keys;
        self
    }

    /// Set maximum number of provider addresses per provider in the memory store.
    ///
    /// If unspecified, the default maximum number of provider addresses is 30.
    pub fn with_max_provider_addresses(mut self, max_provider_addresses: usize) -> Self {
        self.memory_store_config.max_provider_addresses = max_provider_addresses;
        self
    }

    /// Set maximum number of providers per key in the memory store.
    ///
    /// If unspecified, the default maximum number of providers per key is 20.
    pub fn with_max_providers_per_key(mut self, max_providers_per_key: usize) -> Self {
        self.memory_store_config.max_providers_per_key = max_providers_per_key;
        self
    }

    /// Set TTL for the provider records. Recommended value is 2 * (refresh interval) + 10%.
    ///
    /// If unspecified, the default TTL is 48 hours.
    pub fn with_provider_record_ttl(mut self, provider_record_ttl: Duration) -> Self {
        self.memory_store_config.provider_ttl = provider_record_ttl;
        self
    }

    /// Set the refresh (republish) interval for provider records.
    ///
    /// If unspecified, the default interval is 22 hours.
    pub fn with_provider_refresh_interval(mut self, provider_refresh_interval: Duration) -> Self {
        self.memory_store_config.provider_refresh_interval = provider_refresh_interval;
        self
    }

    /// Set the maximum Kademlia message size.
    ///
    /// Should fit `MemoryStore` max record size. If unspecified, the default maximum message size
    /// is 70 KiB.
    pub fn with_max_message_size(mut self, max_message_size: usize) -> Self {
        self.max_message_size = max_message_size;
        self
    }

    /// Build Kademlia [`Config`].
    pub fn build(self) -> (Config, KademliaHandle) {
        Config::new(
            self.replication_factor,
            self.known_peers,
            self.protocol_names,
            self.update_mode,
            self.validation_mode,
            self.record_ttl,
            self.memory_store_config,
            self.max_message_size,
        )
    }
}
