// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Discovery mechanisms of Substrate.
//!
//! The `DiscoveryBehaviour` struct implements the `NetworkBehaviour` trait of libp2p and is
//! responsible for discovering other nodes that are part of the network.
//!
//! Substrate uses the following mechanisms in order to discover nodes that are part of the network:
//!
//! - Bootstrap nodes. These are hard-coded node identities and addresses passed in the constructor
//! of the `DiscoveryBehaviour`. You can also call `add_known_address` later to add an entry.
//!
//! - mDNS. Discovers nodes on the local network by broadcasting UDP packets.
//!
//! - Kademlia random walk. Once connected, we perform random Kademlia `FIND_NODE` requests on the
//! configured Kademlia DHTs in order for nodes to propagate to us their view of the network. This
//! is performed automatically by the `DiscoveryBehaviour`.
//!
//! Additionally, the `DiscoveryBehaviour` is also capable of storing and loading value in the
//! configured DHTs.
//!
//! ## Usage
//!
//! The `DiscoveryBehaviour` generates events of type `DiscoveryOut`, most notably
//! `DiscoveryOut::Discovered` that is generated whenever we discover a node.
//! Only the identity of the node is returned. The node's addresses are stored within the
//! `DiscoveryBehaviour` and can be queried through the `NetworkBehaviour` trait.
//!
//! **Important**: In order for the discovery mechanism to work properly, there needs to be an
//! active mechanism that asks nodes for the addresses they are listening on. Whenever we learn
//! of a node's address, you must call `add_self_reported_address`.

use crate::{
	config::{
		ProtocolId, KADEMLIA_MAX_PROVIDER_KEYS, KADEMLIA_PROVIDER_RECORD_TTL,
		KADEMLIA_PROVIDER_REPUBLISH_INTERVAL,
	},
	utils::LruHashSet,
};

use array_bytes::bytes2hex;
use futures::prelude::*;
use futures_timer::Delay;
use ip_network::IpNetwork;
use libp2p::{
	core::{transport::PortUse, Endpoint, Multiaddr},
	kad::{
		self,
		store::{MemoryStore, MemoryStoreConfig, RecordStore},
		Behaviour as Kademlia, BucketInserts, Config as KademliaConfig, Event as KademliaEvent,
		Event, GetClosestPeersError, GetClosestPeersOk, GetProvidersError, GetProvidersOk,
		GetRecordOk, PeerRecord, QueryId, QueryResult, Quorum, Record, RecordKey,
	},
	mdns::{self, tokio::Behaviour as TokioMdns},
	multiaddr::Protocol,
	swarm::{
		behaviour::{
			toggle::{Toggle, ToggleConnectionHandler},
			DialFailure, ExternalAddrConfirmed, FromSwarm,
		},
		ConnectionDenied, ConnectionId, DialError, NetworkBehaviour, StreamProtocol, THandler,
		THandlerInEvent, THandlerOutEvent, ToSwarm,
	},
	PeerId,
};
use linked_hash_set::LinkedHashSet;
use log::{debug, error, info, trace, warn};
use sp_core::hexdisplay::HexDisplay;
use std::{
	cmp,
	collections::{hash_map::Entry, HashMap, HashSet, VecDeque},
	num::NonZeroUsize,
	task::{Context, Poll},
	time::{Duration, Instant},
};

/// Logging target for the file.
const LOG_TARGET: &str = "sub-libp2p::discovery";

/// Maximum number of known external addresses that we will cache.
/// This only affects whether we will log whenever we (re-)discover
/// a given address.
const MAX_KNOWN_EXTERNAL_ADDRESSES: usize = 32;

/// Default value for Kademlia replication factor which  determines to how many closest peers a
/// record is replicated to.
pub const DEFAULT_KADEMLIA_REPLICATION_FACTOR: usize = 20;

/// The minimum number of peers we expect an answer before we terminate the request.
const GET_RECORD_REDUNDANCY_FACTOR: u32 = 4;

/// Query timeout for Kademlia requests. We need to increase this for record/provider publishing
/// to not timeout most of the time.
const KAD_QUERY_TIMEOUT: Duration = Duration::from_secs(300);

/// `DiscoveryBehaviour` configuration.
///
///
/// Note: In order to discover nodes or load and store values via Kademlia one has to add
///       Kademlia protocol via [`DiscoveryConfig::with_kademlia`].
pub struct DiscoveryConfig {
	local_peer_id: PeerId,
	permanent_addresses: Vec<(PeerId, Multiaddr)>,
	dht_random_walk: bool,
	allow_private_ip: bool,
	allow_non_globals_in_dht: bool,
	discovery_only_if_under_num: u64,
	enable_mdns: bool,
	kademlia_disjoint_query_paths: bool,
	kademlia_protocol: Option<StreamProtocol>,
	kademlia_legacy_protocol: Option<StreamProtocol>,
	kademlia_replication_factor: NonZeroUsize,
}

impl DiscoveryConfig {
	/// Create a default configuration with the given public key.
	pub fn new(local_peer_id: PeerId) -> Self {
		Self {
			local_peer_id,
			permanent_addresses: Vec::new(),
			dht_random_walk: true,
			allow_private_ip: true,
			allow_non_globals_in_dht: false,
			discovery_only_if_under_num: std::u64::MAX,
			enable_mdns: false,
			kademlia_disjoint_query_paths: false,
			kademlia_protocol: None,
			kademlia_legacy_protocol: None,
			kademlia_replication_factor: NonZeroUsize::new(DEFAULT_KADEMLIA_REPLICATION_FACTOR)
				.expect("value is a constant; constant is non-zero; qed."),
		}
	}

	/// Set the number of active connections at which we pause discovery.
	pub fn discovery_limit(&mut self, limit: u64) -> &mut Self {
		self.discovery_only_if_under_num = limit;
		self
	}

	/// Set custom nodes which never expire, e.g. bootstrap or reserved nodes.
	pub fn with_permanent_addresses<I>(&mut self, permanent_addresses: I) -> &mut Self
	where
		I: IntoIterator<Item = (PeerId, Multiaddr)>,
	{
		self.permanent_addresses.extend(permanent_addresses);
		self
	}

	/// Whether the discovery behaviour should periodically perform a random
	/// walk on the DHT to discover peers.
	pub fn with_dht_random_walk(&mut self, value: bool) -> &mut Self {
		self.dht_random_walk = value;
		self
	}

	/// Should private IPv4/IPv6 addresses be reported?
	pub fn allow_private_ip(&mut self, value: bool) -> &mut Self {
		self.allow_private_ip = value;
		self
	}

	/// Should non-global addresses be inserted to the DHT?
	pub fn allow_non_globals_in_dht(&mut self, value: bool) -> &mut Self {
		self.allow_non_globals_in_dht = value;
		self
	}

	/// Should MDNS discovery be supported?
	pub fn with_mdns(&mut self, value: bool) -> &mut Self {
		self.enable_mdns = value;
		self
	}

	/// Add discovery via Kademlia for the given protocol.
	///
	/// Currently accepts `protocol_id`. This should be removed once all the nodes
	/// are upgraded to genesis hash- and fork ID-based Kademlia protocol name.
	pub fn with_kademlia<Hash: AsRef<[u8]>>(
		&mut self,
		genesis_hash: Hash,
		fork_id: Option<&str>,
		protocol_id: &ProtocolId,
	) -> &mut Self {
		self.kademlia_protocol = Some(kademlia_protocol_name(genesis_hash, fork_id));
		self.kademlia_legacy_protocol = Some(legacy_kademlia_protocol_name(protocol_id));
		self
	}

	/// Require iterative Kademlia DHT queries to use disjoint paths for increased resiliency in the
	/// presence of potentially adversarial nodes.
	pub fn use_kademlia_disjoint_query_paths(&mut self, value: bool) -> &mut Self {
		self.kademlia_disjoint_query_paths = value;
		self
	}

	/// Sets Kademlia replication factor.
	pub fn with_kademlia_replication_factor(&mut self, value: NonZeroUsize) -> &mut Self {
		self.kademlia_replication_factor = value;
		self
	}

	/// Create a `DiscoveryBehaviour` from this config.
	pub fn finish(self) -> DiscoveryBehaviour {
		let Self {
			local_peer_id,
			permanent_addresses,
			dht_random_walk,
			allow_private_ip,
			allow_non_globals_in_dht,
			discovery_only_if_under_num,
			enable_mdns,
			kademlia_disjoint_query_paths,
			kademlia_protocol,
			kademlia_legacy_protocol: _,
			kademlia_replication_factor,
		} = self;

		let kademlia = if let Some(ref kademlia_protocol) = kademlia_protocol {
			let mut config = KademliaConfig::new(kademlia_protocol.clone());

			config.set_replication_factor(kademlia_replication_factor);

			config.set_record_filtering(libp2p::kad::StoreInserts::FilterBoth);

			config.set_query_timeout(KAD_QUERY_TIMEOUT);

			// By default Kademlia attempts to insert all peers into its routing table once a
			// dialing attempt succeeds. In order to control which peer is added, disable the
			// auto-insertion and instead add peers manually.
			config.set_kbucket_inserts(BucketInserts::Manual);
			config.disjoint_query_paths(kademlia_disjoint_query_paths);

			config.set_provider_record_ttl(Some(KADEMLIA_PROVIDER_RECORD_TTL));
			config.set_provider_publication_interval(Some(KADEMLIA_PROVIDER_REPUBLISH_INTERVAL));

			let store = MemoryStore::with_config(
				local_peer_id,
				MemoryStoreConfig {
					max_provided_keys: KADEMLIA_MAX_PROVIDER_KEYS,
					..Default::default()
				},
			);

			let mut kad = Kademlia::with_config(local_peer_id, store, config);
			kad.set_mode(Some(kad::Mode::Server));

			for (peer_id, addr) in &permanent_addresses {
				kad.add_address(peer_id, addr.clone());
			}

			Some(kad)
		} else {
			None
		};

		DiscoveryBehaviour {
			permanent_addresses,
			ephemeral_addresses: HashMap::new(),
			kademlia: Toggle::from(kademlia),
			next_kad_random_query: if dht_random_walk {
				Some(Delay::new(Duration::new(0, 0)))
			} else {
				None
			},
			duration_to_next_kad: Duration::from_secs(1),
			pending_events: VecDeque::new(),
			local_peer_id,
			num_connections: 0,
			allow_private_ip,
			discovery_only_if_under_num,
			mdns: if enable_mdns {
				match TokioMdns::new(mdns::Config::default(), local_peer_id) {
					Ok(mdns) => Toggle::from(Some(mdns)),
					Err(err) => {
						warn!(target: LOG_TARGET, "Failed to initialize mDNS: {:?}", err);
						Toggle::from(None)
					},
				}
			} else {
				Toggle::from(None)
			},
			allow_non_globals_in_dht,
			known_external_addresses: LruHashSet::new(
				NonZeroUsize::new(MAX_KNOWN_EXTERNAL_ADDRESSES)
					.expect("value is a constant; constant is non-zero; qed."),
			),
			records_to_publish: Default::default(),
			kademlia_protocol,
			provider_keys_requested: HashMap::new(),
		}
	}
}

/// Implementation of `NetworkBehaviour` that discovers the nodes on the network.
pub struct DiscoveryBehaviour {
	/// User-defined list of nodes and their addresses. Typically includes bootstrap nodes and
	/// reserved nodes.
	permanent_addresses: Vec<(PeerId, Multiaddr)>,
	/// Same as `permanent_addresses`, except that addresses that fail to reach a peer are
	/// removed.
	ephemeral_addresses: HashMap<PeerId, Vec<Multiaddr>>,
	/// Kademlia requests and answers. Even though it's wrapped in `Toggle`, currently
	/// it's always enabled in `NetworkWorker::new()`.
	kademlia: Toggle<Kademlia<MemoryStore>>,
	/// Discovers nodes on the local network.
	mdns: Toggle<TokioMdns>,
	/// Stream that fires when we need to perform the next random Kademlia query. `None` if
	/// random walking is disabled.
	next_kad_random_query: Option<Delay>,
	/// After `next_kad_random_query` triggers, the next one triggers after this duration.
	duration_to_next_kad: Duration,
	/// Events to return in priority when polled.
	pending_events: VecDeque<DiscoveryOut>,
	/// Identity of our local node.
	local_peer_id: PeerId,
	/// Number of nodes we're currently connected to.
	num_connections: u64,
	/// If false, `addresses_of_peer` won't return any private IPv4/IPv6 address, except for the
	/// ones stored in `permanent_addresses` or `ephemeral_addresses`.
	allow_private_ip: bool,
	/// Number of active connections over which we interrupt the discovery process.
	discovery_only_if_under_num: u64,
	/// Should non-global addresses be added to the DHT?
	allow_non_globals_in_dht: bool,
	/// A cache of discovered external addresses. Only used for logging purposes.
	known_external_addresses: LruHashSet<Multiaddr>,
	/// Records to publish per QueryId.
	///
	/// After finishing a Kademlia query, libp2p will return us a list of the closest peers that
	/// did not return the record(in `FinishedWithNoAdditionalRecord`). We will then put the record
	/// to these peers.
	records_to_publish: HashMap<QueryId, Record>,
	/// The chain based kademlia protocol name (including genesis hash and fork id).
	///
	/// Remove when all nodes are upgraded to genesis hash and fork ID-based Kademlia:
	/// <https://github.com/paritytech/polkadot-sdk/issues/504>.
	kademlia_protocol: Option<StreamProtocol>,
	/// Provider keys requested with `GET_PROVIDERS` queries.
	provider_keys_requested: HashMap<QueryId, RecordKey>,
}

impl DiscoveryBehaviour {
	/// Returns the list of nodes that we know exist in the network.
	pub fn known_peers(&mut self) -> HashSet<PeerId> {
		let mut peers = HashSet::new();
		if let Some(k) = self.kademlia.as_mut() {
			for b in k.kbuckets() {
				for e in b.iter() {
					if !peers.contains(e.node.key.preimage()) {
						peers.insert(*e.node.key.preimage());
					}
				}
			}
		}
		peers
	}

	/// Adds a hard-coded address for the given peer, that never expires.
	///
	/// This adds an entry to the parameter that was passed to `new`.
	///
	/// If we didn't know this address before, also generates a `Discovered` event.
	pub fn add_known_address(&mut self, peer_id: PeerId, addr: Multiaddr) {
		let addrs_list = self.ephemeral_addresses.entry(peer_id).or_default();
		if addrs_list.contains(&addr) {
			return
		}

		if let Some(k) = self.kademlia.as_mut() {
			k.add_address(&peer_id, addr.clone());
		}

		self.pending_events.push_back(DiscoveryOut::Discovered(peer_id));
		addrs_list.push(addr);
	}

	/// Add a self-reported address of a remote peer to the k-buckets of the DHT
	/// if it has compatible `supported_protocols`.
	///
	/// **Note**: It is important that you call this method. The discovery mechanism will not
	/// automatically add connecting peers to the Kademlia k-buckets.
	pub fn add_self_reported_address(
		&mut self,
		peer_id: &PeerId,
		supported_protocols: &[StreamProtocol],
		addr: Multiaddr,
	) {
		if let Some(kademlia) = self.kademlia.as_mut() {
			if !self.allow_non_globals_in_dht && !Self::can_add_to_dht(&addr) {
				trace!(
					target: LOG_TARGET,
					"Ignoring self-reported non-global address {} from {}.", addr, peer_id
				);
				return
			}

			// The supported protocols must include the chain-based Kademlia protocol.
			//
			// Extract the chain-based Kademlia protocol from `kademlia.protocol_name()`
			// when all nodes are upgraded to genesis hash and fork ID-based Kademlia:
			// https://github.com/paritytech/polkadot-sdk/issues/504.
			if !supported_protocols.iter().any(|p| {
				p == self
					.kademlia_protocol
					.as_ref()
					.expect("kademlia protocol was checked above to be enabled; qed")
			}) {
				trace!(
					target: LOG_TARGET,
					"Ignoring self-reported address {} from {} as remote node is not part of the \
					 Kademlia DHT supported by the local node.", addr, peer_id,
				);
				return
			}

			trace!(
				target: LOG_TARGET,
				"Adding self-reported address {} from {} to Kademlia DHT.",
				addr, peer_id
			);
			kademlia.add_address(peer_id, addr.clone());
		}
	}

	/// Start finding the closest peers to the given `PeerId`.
	///
	/// A corresponding `ClosestPeersFound` or `ClosestPeersNotFound` event will later be generated.
	pub fn find_closest_peers(&mut self, target: PeerId) {
		if let Some(k) = self.kademlia.as_mut() {
			k.get_closest_peers(target);
		}
	}

	/// Start fetching a record from the DHT.
	///
	/// A corresponding `ValueFound` or `ValueNotFound` event will later be generated.
	pub fn get_value(&mut self, key: RecordKey) {
		if let Some(k) = self.kademlia.as_mut() {
			k.get_record(key.clone());
		}
	}

	/// Start putting a record into the DHT. Other nodes can later fetch that value with
	/// `get_value`.
	///
	/// A corresponding `ValuePut` or `ValuePutFailed` event will later be generated.
	pub fn put_value(&mut self, key: RecordKey, value: Vec<u8>) {
		if let Some(k) = self.kademlia.as_mut() {
			if let Err(e) = k.put_record(Record::new(key.clone(), value.clone()), Quorum::All) {
				warn!(target: LOG_TARGET, "Libp2p => Failed to put record: {:?}", e);
				self.pending_events
					.push_back(DiscoveryOut::ValuePutFailed(key.clone(), Duration::from_secs(0)));
			}
		}
	}

	/// Puts a record into the DHT on the provided `peers`
	///
	/// If `update_local_storage` is true, the local storage is update as well.
	pub fn put_record_to(
		&mut self,
		record: Record,
		peers: HashSet<sc_network_types::PeerId>,
		update_local_storage: bool,
	) {
		if let Some(kad) = self.kademlia.as_mut() {
			if update_local_storage {
				if let Err(_e) = kad.store_mut().put(record.clone()) {
					warn!(target: LOG_TARGET, "Failed to update local starage");
				}
			}

			if !peers.is_empty() {
				kad.put_record_to(
					record,
					peers.into_iter().map(|peer_id| peer_id.into()),
					Quorum::All,
				);
			}
		}
	}

	/// Register as a content provider on the DHT for `key`.
	pub fn start_providing(&mut self, key: RecordKey) {
		if let Some(kad) = self.kademlia.as_mut() {
			if let Err(e) = kad.start_providing(key.clone()) {
				warn!(target: LOG_TARGET, "Libp2p => Failed to start providing {key:?}: {e}.");
				self.pending_events
					.push_back(DiscoveryOut::StartProvidingFailed(key, Duration::from_secs(0)));
			}
		}
	}

	/// Deregister as a content provider on the DHT for `key`.
	pub fn stop_providing(&mut self, key: &RecordKey) {
		if let Some(kad) = self.kademlia.as_mut() {
			kad.stop_providing(key);
		}
	}

	/// Get content providers for `key` from the DHT.
	pub fn get_providers(&mut self, key: RecordKey) {
		if let Some(kad) = self.kademlia.as_mut() {
			let query_id = kad.get_providers(key.clone());
			self.provider_keys_requested.insert(query_id, key);
		}
	}

	/// Store a record in the Kademlia record store.
	pub fn store_record(
		&mut self,
		record_key: RecordKey,
		record_value: Vec<u8>,
		publisher: Option<PeerId>,
		expires: Option<Instant>,
	) {
		if let Some(k) = self.kademlia.as_mut() {
			if let Err(err) = k.store_mut().put(Record {
				key: record_key,
				value: record_value,
				publisher: publisher.map(|publisher| publisher.into()),
				expires,
			}) {
				debug!(
					target: LOG_TARGET,
					"Failed to store record with key: {:?}",
					err
				);
			}
		}
	}

	/// Returns the number of nodes in each Kademlia kbucket for each Kademlia instance.
	///
	/// Identifies Kademlia instances by their [`ProtocolId`] and kbuckets by the base 2 logarithm
	/// of their lower bound.
	pub fn num_entries_per_kbucket(&mut self) -> Option<Vec<(u32, usize)>> {
		self.kademlia.as_mut().map(|kad| {
			kad.kbuckets()
				.map(|bucket| (bucket.range().0.ilog2().unwrap_or(0), bucket.iter().count()))
				.collect()
		})
	}

	/// Returns the number of records in the Kademlia record stores.
	pub fn num_kademlia_records(&mut self) -> Option<usize> {
		// Note that this code is ok only because we use a `MemoryStore`.
		self.kademlia.as_mut().map(|kad| kad.store_mut().records().count())
	}

	/// Returns the total size in bytes of all the records in the Kademlia record stores.
	pub fn kademlia_records_total_size(&mut self) -> Option<usize> {
		// Note that this code is ok only because we use a `MemoryStore`. If the records were
		// for example stored on disk, this would load every single one of them every single time.
		self.kademlia
			.as_mut()
			.map(|kad| kad.store_mut().records().fold(0, |tot, rec| tot + rec.value.len()))
	}

	/// Can the given `Multiaddr` be put into the DHT?
	///
	/// This test is successful only for global IP addresses and DNS names.
	// NB: Currently all DNS names are allowed and no check for TLD suffixes is done
	// because the set of valid domains is highly dynamic and would require frequent
	// updates, for example by utilising publicsuffix.org or IANA.
	pub fn can_add_to_dht(addr: &Multiaddr) -> bool {
		let ip = match addr.iter().next() {
			Some(Protocol::Ip4(ip)) => IpNetwork::from(ip),
			Some(Protocol::Ip6(ip)) => IpNetwork::from(ip),
			Some(Protocol::Dns(_)) | Some(Protocol::Dns4(_)) | Some(Protocol::Dns6(_)) =>
				return true,
			_ => return false,
		};
		ip.is_global()
	}
}

/// Event generated by the `DiscoveryBehaviour`.
#[derive(Debug)]
pub enum DiscoveryOut {
	/// We discovered a peer and currenlty have it's addresses stored either in the routing
	/// table or in the ephemeral addresses list, so a connection can be established.
	Discovered(PeerId),

	/// A peer connected to this node for whom no listen address is known.
	///
	/// In order for the peer to be added to the Kademlia routing table, a known
	/// listen address must be added via
	/// [`DiscoveryBehaviour::add_self_reported_address`], e.g. obtained through
	/// the `identify` protocol.
	UnroutablePeer(PeerId),

	/// `FIND_NODE` query yielded closest peers with their addresses. This event also delivers
	/// a partial result in case the query timed out, because it can contain the target peer's
	/// address.
	ClosestPeersFound(PeerId, Vec<(PeerId, Vec<Multiaddr>)>, Duration),

	/// The closest peers to the target `PeerId` have not been found.
	ClosestPeersNotFound(PeerId, Duration),

	/// The DHT yielded results for the record request.
	///
	/// Returning the result grouped in (key, value) pairs as well as the request duration.
	ValueFound(PeerRecord, Duration),

	/// The DHT received a put record request.
	PutRecordRequest(
		RecordKey,
		Vec<u8>,
		Option<sc_network_types::PeerId>,
		Option<std::time::Instant>,
	),

	/// The record requested was not found in the DHT.
	///
	/// Returning the corresponding key as well as the request duration.
	ValueNotFound(RecordKey, Duration),

	/// The record with a given key was successfully inserted into the DHT.
	///
	/// Returning the corresponding key as well as the request duration.
	ValuePut(RecordKey, Duration),

	/// Inserting a value into the DHT failed.
	///
	/// Returning the corresponding key as well as the request duration.
	ValuePutFailed(RecordKey, Duration),

	/// The content provider for a given key was successfully published.
	StartedProviding(RecordKey, Duration),

	/// Starting providing a key failed.
	StartProvidingFailed(RecordKey, Duration),

	/// The DHT yielded results for the providers request.
	ProvidersFound(RecordKey, HashSet<PeerId>, Duration),

	/// The DHT yielded no more providers for the key (`GET_PROVIDERS` query finished).
	NoMoreProviders(RecordKey, Duration),

	/// Providers for the requested key were not found in the DHT.
	ProvidersNotFound(RecordKey, Duration),

	/// Started a random Kademlia query.
	///
	/// Only happens if [`DiscoveryConfig::with_dht_random_walk`] has been configured to `true`.
	RandomKademliaStarted,
}

impl NetworkBehaviour for DiscoveryBehaviour {
	type ConnectionHandler =
		ToggleConnectionHandler<<Kademlia<MemoryStore> as NetworkBehaviour>::ConnectionHandler>;
	type ToSwarm = DiscoveryOut;

	fn handle_established_inbound_connection(
		&mut self,
		connection_id: ConnectionId,
		peer: PeerId,
		local_addr: &Multiaddr,
		remote_addr: &Multiaddr,
	) -> Result<THandler<Self>, ConnectionDenied> {
		self.kademlia.handle_established_inbound_connection(
			connection_id,
			peer,
			local_addr,
			remote_addr,
		)
	}

	fn handle_established_outbound_connection(
		&mut self,
		connection_id: ConnectionId,
		peer: PeerId,
		addr: &Multiaddr,
		role_override: Endpoint,
		port_use: PortUse,
	) -> Result<THandler<Self>, ConnectionDenied> {
		self.kademlia.handle_established_outbound_connection(
			connection_id,
			peer,
			addr,
			role_override,
			port_use,
		)
	}

	fn handle_pending_inbound_connection(
		&mut self,
		connection_id: ConnectionId,
		local_addr: &Multiaddr,
		remote_addr: &Multiaddr,
	) -> Result<(), ConnectionDenied> {
		self.kademlia
			.handle_pending_inbound_connection(connection_id, local_addr, remote_addr)
	}

	fn handle_pending_outbound_connection(
		&mut self,
		connection_id: ConnectionId,
		maybe_peer: Option<PeerId>,
		addresses: &[Multiaddr],
		effective_role: Endpoint,
	) -> Result<Vec<Multiaddr>, ConnectionDenied> {
		let Some(peer_id) = maybe_peer else { return Ok(Vec::new()) };

		// Collect addresses into [`LinkedHashSet`] to eliminate duplicate entries preserving the
		// order of addresses. Give priority to `permanent_addresses` (used with reserved nodes) and
		// `ephemeral_addresses` (used for addresses discovered from other sources, like authority
		// discovery DHT records).
		let mut list: LinkedHashSet<_> = self
			.permanent_addresses
			.iter()
			.filter_map(|(p, a)| (*p == peer_id).then(|| a.clone()))
			.collect();

		if let Some(ephemeral_addresses) = self.ephemeral_addresses.get(&peer_id) {
			ephemeral_addresses.iter().for_each(|address| {
				list.insert_if_absent(address.clone());
			});
		}

		{
			let mut list_to_filter = self.kademlia.handle_pending_outbound_connection(
				connection_id,
				maybe_peer,
				addresses,
				effective_role,
			)?;

			list_to_filter.extend(self.mdns.handle_pending_outbound_connection(
				connection_id,
				maybe_peer,
				addresses,
				effective_role,
			)?);

			if !self.allow_private_ip {
				list_to_filter.retain(|addr| match addr.iter().next() {
					Some(Protocol::Ip4(addr)) if !IpNetwork::from(addr).is_global() => false,
					Some(Protocol::Ip6(addr)) if !IpNetwork::from(addr).is_global() => false,
					_ => true,
				});
			}

			list_to_filter.into_iter().for_each(|address| {
				list.insert_if_absent(address);
			});
		}

		trace!(target: LOG_TARGET, "Addresses of {:?}: {:?}", peer_id, list);

		Ok(list.into_iter().collect())
	}

	fn on_swarm_event(&mut self, event: FromSwarm) {
		match event {
			FromSwarm::ConnectionEstablished(e) => {
				self.num_connections += 1;
				self.kademlia.on_swarm_event(FromSwarm::ConnectionEstablished(e));
			},
			FromSwarm::ConnectionClosed(e) => {
				self.num_connections -= 1;
				self.kademlia.on_swarm_event(FromSwarm::ConnectionClosed(e));
			},
			FromSwarm::DialFailure(e @ DialFailure { peer_id, error, .. }) => {
				if let Some(peer_id) = peer_id {
					if let DialError::Transport(errors) = error {
						if let Entry::Occupied(mut entry) = self.ephemeral_addresses.entry(peer_id)
						{
							for (addr, _error) in errors {
								entry.get_mut().retain(|a| a != addr);
							}
							if entry.get().is_empty() {
								entry.remove();
							}
						}
					}
				}

				self.kademlia.on_swarm_event(FromSwarm::DialFailure(e));
			},
			FromSwarm::ListenerClosed(e) => {
				self.kademlia.on_swarm_event(FromSwarm::ListenerClosed(e));
			},
			FromSwarm::ListenFailure(e) => {
				self.kademlia.on_swarm_event(FromSwarm::ListenFailure(e));
			},
			FromSwarm::ListenerError(e) => {
				self.kademlia.on_swarm_event(FromSwarm::ListenerError(e));
			},
			FromSwarm::ExternalAddrExpired(e) => {
				// We intentionally don't remove the element from `known_external_addresses` in
				// order to not print the log line again.

				self.kademlia.on_swarm_event(FromSwarm::ExternalAddrExpired(e));
			},
			FromSwarm::NewListener(e) => {
				self.kademlia.on_swarm_event(FromSwarm::NewListener(e));
			},
			FromSwarm::ExpiredListenAddr(e) => {
				self.kademlia.on_swarm_event(FromSwarm::ExpiredListenAddr(e));
			},
			FromSwarm::NewExternalAddrCandidate(e) => {
				self.kademlia.on_swarm_event(FromSwarm::NewExternalAddrCandidate(e));
			},
			FromSwarm::AddressChange(e) => {
				self.kademlia.on_swarm_event(FromSwarm::AddressChange(e));
			},
			FromSwarm::NewListenAddr(e) => {
				self.kademlia.on_swarm_event(FromSwarm::NewListenAddr(e));
				self.mdns.on_swarm_event(FromSwarm::NewListenAddr(e));
			},
			FromSwarm::ExternalAddrConfirmed(e @ ExternalAddrConfirmed { addr }) => {
				let mut address = addr.clone();

				if let Some(Protocol::P2p(peer_id)) = addr.iter().last() {
					if peer_id != self.local_peer_id {
						warn!(
							target: LOG_TARGET,
							"🔍 Discovered external address for a peer that is not us: {addr}",
						);
						// Ensure this address is not propagated to kademlia.
						return
					}
				} else {
					address.push(Protocol::P2p(self.local_peer_id));
				}

				if Self::can_add_to_dht(&address) {
					// NOTE: we might re-discover the same address multiple times
					// in which case we just want to refrain from logging.
					if self.known_external_addresses.insert(address.clone()) {
						info!(
						  target: LOG_TARGET,
						  "🔍 Discovered new external address for our node: {address}",
						);
					}
				}

				self.kademlia.on_swarm_event(FromSwarm::ExternalAddrConfirmed(e));
			},
			FromSwarm::NewExternalAddrOfPeer(e) => {
				self.kademlia.on_swarm_event(FromSwarm::NewExternalAddrOfPeer(e));
				self.mdns.on_swarm_event(FromSwarm::NewExternalAddrOfPeer(e));
			},
			event => {
				debug!(target: LOG_TARGET, "New unknown `FromSwarm` libp2p event: {event:?}");
				self.kademlia.on_swarm_event(event);
				self.mdns.on_swarm_event(event);
			},
		}
	}

	fn on_connection_handler_event(
		&mut self,
		peer_id: PeerId,
		connection_id: ConnectionId,
		event: THandlerOutEvent<Self>,
	) {
		self.kademlia.on_connection_handler_event(peer_id, connection_id, event);
	}

	fn poll(&mut self, cx: &mut Context) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
		// Immediately process the content of `discovered`.
		if let Some(ev) = self.pending_events.pop_front() {
			return Poll::Ready(ToSwarm::GenerateEvent(ev))
		}

		// Poll the stream that fires when we need to start a random Kademlia query.
		if let Some(kademlia) = self.kademlia.as_mut() {
			if let Some(next_kad_random_query) = self.next_kad_random_query.as_mut() {
				while next_kad_random_query.poll_unpin(cx).is_ready() {
					let actually_started =
						if self.num_connections < self.discovery_only_if_under_num {
							let random_peer_id = PeerId::random();
							debug!(
								target: LOG_TARGET,
								"Libp2p <= Starting random Kademlia request for {:?}",
								random_peer_id,
							);
							kademlia.get_closest_peers(random_peer_id);
							true
						} else {
							debug!(
								target: LOG_TARGET,
								"Kademlia paused due to high number of connections ({})",
								self.num_connections
							);
							false
						};

					// Schedule the next random query with exponentially increasing delay,
					// capped at 60 seconds.
					*next_kad_random_query = Delay::new(self.duration_to_next_kad);
					self.duration_to_next_kad =
						cmp::min(self.duration_to_next_kad * 2, Duration::from_secs(60));

					if actually_started {
						let ev = DiscoveryOut::RandomKademliaStarted;
						return Poll::Ready(ToSwarm::GenerateEvent(ev))
					}
				}
			}
		}

		while let Poll::Ready(ev) = self.kademlia.poll(cx) {
			match ev {
				ToSwarm::GenerateEvent(ev) => match ev {
					KademliaEvent::RoutingUpdated { peer, .. } => {
						let ev = DiscoveryOut::Discovered(peer);
						return Poll::Ready(ToSwarm::GenerateEvent(ev))
					},
					KademliaEvent::UnroutablePeer { peer, .. } => {
						let ev = DiscoveryOut::UnroutablePeer(peer);
						return Poll::Ready(ToSwarm::GenerateEvent(ev))
					},
					KademliaEvent::RoutablePeer { .. } => {
						// Generate nothing, because the address was not added to the routing table,
						// so we will not be able to connect to the peer.
					},
					KademliaEvent::PendingRoutablePeer { .. } => {
						// We are not interested in this event at the moment.
					},
					KademliaEvent::InboundRequest { request } => match request {
						libp2p::kad::InboundRequest::PutRecord { record: Some(record), .. } =>
							return Poll::Ready(ToSwarm::GenerateEvent(
								DiscoveryOut::PutRecordRequest(
									record.key,
									record.value,
									record.publisher.map(Into::into),
									record.expires,
								),
							)),
						_ => {},
					},
					KademliaEvent::OutboundQueryProgressed {
						result: QueryResult::GetClosestPeers(res),
						stats,
						..
					} => {
						let (key, peers, timeout) = match res {
							Ok(GetClosestPeersOk { key, peers }) => (key, peers, false),
							Err(GetClosestPeersError::Timeout { key, peers }) => (key, peers, true),
						};

						let target = match PeerId::from_bytes(&key.clone()) {
							Ok(peer_id) => peer_id,
							Err(_) => {
								warn!(
									target: LOG_TARGET,
									"Libp2p => FIND_NODE query finished for target that is not \
									 a peer ID: {:?}",
									HexDisplay::from(&key),
								);
								continue
							},
						};

						if timeout {
							debug!(
								target: LOG_TARGET,
								"Libp2p => Query for target {target:?} timed out and yielded {} peers",
								peers.len(),
							);
						} else {
							debug!(
								target: LOG_TARGET,
								"Libp2p => Query for target {target:?} yielded {} peers",
								peers.len(),
							);
						}

						let ev = if peers.is_empty() {
							DiscoveryOut::ClosestPeersNotFound(
								target,
								stats.duration().unwrap_or_default(),
							)
						} else {
							DiscoveryOut::ClosestPeersFound(
								target,
								peers.into_iter().map(|p| (p.peer_id, p.addrs)).collect(),
								stats.duration().unwrap_or_default(),
							)
						};

						return Poll::Ready(ToSwarm::GenerateEvent(ev))
					},
					KademliaEvent::OutboundQueryProgressed {
						result: QueryResult::GetRecord(res),
						stats,
						id,
						..
					} => {
						let ev = match res {
							Ok(GetRecordOk::FoundRecord(r)) => {
								debug!(
									target: LOG_TARGET,
									"Libp2p => Found record ({:?}) with value: {:?} id {:?} stats {:?}",
									r.record.key,
									r.record.value,
									id,
									stats,
								);

								// Let's directly finish the query if we are above 4.
								// This number is small enough to make sure we don't
								// unnecessarily flood the network with queries, but high
								// enough to make sure we also touch peers which might have
								// old record, so that we can update them once we notice
								// they have old records.
								if stats.num_successes() > GET_RECORD_REDUNDANCY_FACTOR {
									if let Some(kad) = self.kademlia.as_mut() {
										if let Some(mut query) = kad.query_mut(&id) {
											query.finish();
										}
									}
								}

								// Will be removed below when we receive
								// `FinishedWithNoAdditionalRecord`.
								self.records_to_publish.insert(id, r.record.clone());

								DiscoveryOut::ValueFound(r, stats.duration().unwrap_or_default())
							},
							Ok(GetRecordOk::FinishedWithNoAdditionalRecord {
								cache_candidates,
							}) => {
								debug!(
									target: LOG_TARGET,
									"Libp2p => Finished with no-additional-record {:?} stats {:?} took {:?} ms",
									id,
									stats,
									stats.duration().map(|val| val.as_millis())
								);
								// We always need to remove the record to not leak any data!
								if let Some(record) = self.records_to_publish.remove(&id) {
									if cache_candidates.is_empty() {
										continue
									}

									// Put the record to the `cache_candidates` that are nearest to
									// the record key from our point of view of the network.
									if let Some(kad) = self.kademlia.as_mut() {
										kad.put_record_to(
											record,
											cache_candidates.into_iter().map(|v| v.1),
											Quorum::One,
										);
									}
								}

								continue
							},
							Err(e @ libp2p::kad::GetRecordError::NotFound { .. }) => {
								trace!(
									target: LOG_TARGET,
									"Libp2p => Failed to get record: {:?}",
									e,
								);
								DiscoveryOut::ValueNotFound(
									e.into_key(),
									stats.duration().unwrap_or_default(),
								)
							},
							Err(e) => {
								debug!(
									target: LOG_TARGET,
									"Libp2p => Failed to get record: {:?}",
									e,
								);
								DiscoveryOut::ValueNotFound(
									e.into_key(),
									stats.duration().unwrap_or_default(),
								)
							},
						};
						return Poll::Ready(ToSwarm::GenerateEvent(ev))
					},
					KademliaEvent::OutboundQueryProgressed {
						result: QueryResult::GetProviders(res),
						stats,
						id,
						..
					} => {
						let ev = match res {
							Ok(GetProvidersOk::FoundProviders { key, providers }) => {
								debug!(
									target: LOG_TARGET,
									"Libp2p => Found providers {:?} for key {:?}, id {:?}, stats {:?}",
									providers,
									key,
									id,
									stats,
								);

								DiscoveryOut::ProvidersFound(
									key,
									providers,
									stats.duration().unwrap_or_default(),
								)
							},
							Ok(GetProvidersOk::FinishedWithNoAdditionalRecord {
								closest_peers: _,
							}) => {
								debug!(
									target: LOG_TARGET,
									"Libp2p => Finished with no additional providers {:?}, stats {:?}, took {:?} ms",
									id,
									stats,
									stats.duration().map(|val| val.as_millis())
								);

								if let Some(key) = self.provider_keys_requested.remove(&id) {
									DiscoveryOut::NoMoreProviders(
										key,
										stats.duration().unwrap_or_default(),
									)
								} else {
									error!(
										target: LOG_TARGET,
										"No key found for `GET_PROVIDERS` query {id:?}. This is a bug.",
									);
									continue
								}
							},
							Err(GetProvidersError::Timeout { key, closest_peers: _ }) => {
								debug!(
									target: LOG_TARGET,
									"Libp2p => Failed to get providers for {key:?} due to timeout.",
								);

								self.provider_keys_requested.remove(&id);

								DiscoveryOut::ProvidersNotFound(
									key,
									stats.duration().unwrap_or_default(),
								)
							},
						};
						return Poll::Ready(ToSwarm::GenerateEvent(ev))
					},
					KademliaEvent::OutboundQueryProgressed {
						result: QueryResult::PutRecord(res),
						stats,
						..
					} => {
						let ev = match res {
							Ok(ok) => {
								trace!(
									target: LOG_TARGET,
									"Libp2p => Put record for key: {:?}",
									ok.key,
								);
								DiscoveryOut::ValuePut(ok.key, stats.duration().unwrap_or_default())
							},
							Err(e) => {
								debug!(
									target: LOG_TARGET,
									"Libp2p => Failed to put record for key {:?}: {:?}",
									e.key(),
									e,
								);
								DiscoveryOut::ValuePutFailed(
									e.into_key(),
									stats.duration().unwrap_or_default(),
								)
							},
						};
						return Poll::Ready(ToSwarm::GenerateEvent(ev))
					},
					KademliaEvent::OutboundQueryProgressed {
						result: QueryResult::RepublishRecord(res),
						..
					} => match res {
						Ok(ok) => debug!(
							target: LOG_TARGET,
							"Libp2p => Record republished: {:?}",
							ok.key,
						),
						Err(e) => debug!(
							target: LOG_TARGET,
							"Libp2p => Republishing of record {:?} failed with: {:?}",
							e.key(), e,
						),
					},
					KademliaEvent::OutboundQueryProgressed {
						result: QueryResult::StartProviding(res),
						stats,
						..
					} => {
						let ev = match res {
							Ok(ok) => {
								trace!(
									target: LOG_TARGET,
									"Libp2p => Started providing key {:?}",
									ok.key,
								);
								DiscoveryOut::StartedProviding(
									ok.key,
									stats.duration().unwrap_or_default(),
								)
							},
							Err(e) => {
								debug!(
									target: LOG_TARGET,
									"Libp2p => Failed to start providing key {:?}: {:?}",
									e.key(),
									e,
								);
								DiscoveryOut::StartProvidingFailed(
									e.into_key(),
									stats.duration().unwrap_or_default(),
								)
							},
						};
						return Poll::Ready(ToSwarm::GenerateEvent(ev))
					},
					KademliaEvent::OutboundQueryProgressed {
						result: QueryResult::Bootstrap(res),
						..
					} => match res {
						Ok(ok) => debug!(
							target: LOG_TARGET,
							"Libp2p => DHT bootstrap progressed: {ok:?}",
						),
						Err(e) => warn!(
							target: LOG_TARGET,
							"Libp2p => DHT bootstrap error: {e:?}",
						),
					},
					// We never start any other type of query.
					KademliaEvent::OutboundQueryProgressed { result: e, .. } => {
						warn!(target: LOG_TARGET, "Libp2p => Unhandled Kademlia event: {:?}", e)
					},
					Event::ModeChanged { new_mode } => {
						debug!(target: LOG_TARGET, "Libp2p => Kademlia mode changed: {new_mode}")
					},
				},
				ToSwarm::Dial { opts } => return Poll::Ready(ToSwarm::Dial { opts }),
				event => {
					return Poll::Ready(event.map_out(|_| {
						unreachable!("`GenerateEvent` is handled in a branch above; qed")
					}));
				},
			}
		}

		// Poll mDNS.
		while let Poll::Ready(ev) = self.mdns.poll(cx) {
			match ev {
				ToSwarm::GenerateEvent(event) => match event {
					mdns::Event::Discovered(list) => {
						if self.num_connections >= self.discovery_only_if_under_num {
							continue
						}

						self.pending_events.extend(
							list.into_iter().map(|(peer_id, _)| DiscoveryOut::Discovered(peer_id)),
						);
						if let Some(ev) = self.pending_events.pop_front() {
							return Poll::Ready(ToSwarm::GenerateEvent(ev))
						}
					},
					mdns::Event::Expired(_) => {},
				},
				ToSwarm::Dial { .. } => {
					unreachable!("mDNS never dials!");
				},
				// `event` is an enum with no variant
				ToSwarm::NotifyHandler { event, .. } => match event {},
				event => {
					return Poll::Ready(
						event
							.map_in(|_| {
								unreachable!("`NotifyHandler` is handled in a branch above; qed")
							})
							.map_out(|_| {
								unreachable!("`GenerateEvent` is handled in a branch above; qed")
							}),
					);
				},
			}
		}

		Poll::Pending
	}
}

/// Legacy (fallback) Kademlia protocol name based on `protocol_id`.
fn legacy_kademlia_protocol_name(id: &ProtocolId) -> StreamProtocol {
	let name = format!("/{}/kad", id.as_ref());
	StreamProtocol::try_from_owned(name).expect("protocol name is valid. qed")
}

/// Kademlia protocol name based on `genesis_hash` and `fork_id`.
fn kademlia_protocol_name<Hash: AsRef<[u8]>>(
	genesis_hash: Hash,
	fork_id: Option<&str>,
) -> StreamProtocol {
	let genesis_hash_hex = bytes2hex("", genesis_hash.as_ref());
	let name = if let Some(fork_id) = fork_id {
		format!("/{genesis_hash_hex}/{fork_id}/kad")
	} else {
		format!("/{genesis_hash_hex}/kad")
	};

	StreamProtocol::try_from_owned(name).expect("protocol name is valid. qed")
}

#[cfg(test)]
mod tests {
	use super::{kademlia_protocol_name, legacy_kademlia_protocol_name, DiscoveryConfig};
	use crate::config::ProtocolId;
	use libp2p::{identity::Keypair, Multiaddr};
	use sp_core::hash::H256;

	#[cfg(ignore_flaky_test)] // https://github.com/paritytech/polkadot-sdk/issues/48
	#[tokio::test]
	async fn discovery_working() {
		use super::DiscoveryOut;
		use futures::prelude::*;
		use libp2p::{
			core::{
				transport::{MemoryTransport, Transport},
				upgrade,
			},
			noise,
			swarm::{Swarm, SwarmEvent},
			yamux,
		};
		use std::{collections::HashSet, task::Poll, time::Duration};
		let mut first_swarm_peer_id_and_addr = None;

		let genesis_hash = H256::from_low_u64_be(1);
		let fork_id = Some("test-fork-id");
		let protocol_id = ProtocolId::from("dot");

		// Build swarms whose behaviour is `DiscoveryBehaviour`, each aware of
		// the first swarm via `with_permanent_addresses`.
		let mut swarms = (0..25)
			.map(|i| {
				let mut swarm = libp2p::SwarmBuilder::with_new_identity()
					.with_tokio()
					.with_other_transport(|keypair| {
						MemoryTransport::new()
							.upgrade(upgrade::Version::V1)
							.authenticate(noise::Config::new(&keypair).unwrap())
							.multiplex(yamux::Config::default())
							.boxed()
					})
					.unwrap()
					.with_behaviour(|keypair| {
						let mut config = DiscoveryConfig::new(keypair.public().to_peer_id());
						config
							.with_permanent_addresses(first_swarm_peer_id_and_addr.clone())
							.allow_private_ip(true)
							.allow_non_globals_in_dht(true)
							.discovery_limit(50)
							.with_kademlia(genesis_hash, fork_id, &protocol_id);

						config.finish()
					})
					.unwrap()
					.with_swarm_config(|config| {
						// This is taken care of by notification protocols in non-test environment
						config.with_idle_connection_timeout(Duration::from_secs(10))
					})
					.build();

				let listen_addr: Multiaddr =
					format!("/memory/{}", rand::random::<u64>()).parse().unwrap();

				if i == 0 {
					first_swarm_peer_id_and_addr =
						Some((*swarm.local_peer_id(), listen_addr.clone()))
				}

				swarm.listen_on(listen_addr.clone()).unwrap();
				(swarm, listen_addr)
			})
			.collect::<Vec<_>>();

		// Build a `Vec<HashSet<PeerId>>` with the list of nodes remaining to be discovered.
		let mut to_discover = (0..swarms.len())
			.map(|n| {
				(0..swarms.len())
					// Skip the first swarm as all other swarms already know it.
					.skip(1)
					.filter(|p| *p != n)
					.map(|p| *Swarm::local_peer_id(&swarms[p].0))
					.collect::<HashSet<_>>()
			})
			.collect::<Vec<_>>();

		let fut = futures::future::poll_fn(move |cx| {
			'polling: loop {
				for swarm_n in 0..swarms.len() {
					match swarms[swarm_n].0.poll_next_unpin(cx) {
						Poll::Ready(Some(e)) => {
							match e {
								SwarmEvent::Behaviour(behavior) => {
									match behavior {
										DiscoveryOut::UnroutablePeer(other) |
										DiscoveryOut::Discovered(other) => {
											// Call `add_self_reported_address` to simulate identify
											// happening.
											let addr = swarms
												.iter()
												.find_map(|(s, a)| {
													if s.behaviour().local_peer_id == other {
														Some(a.clone())
													} else {
														None
													}
												})
												.unwrap();
											// Test both genesis hash-based and legacy
											// protocol names.
											let protocol_names = if swarm_n % 2 == 0 {
												vec![kademlia_protocol_name(genesis_hash, fork_id)]
											} else {
												vec![
													legacy_kademlia_protocol_name(&protocol_id),
													kademlia_protocol_name(genesis_hash, fork_id),
												]
											};
											swarms[swarm_n]
												.0
												.behaviour_mut()
												.add_self_reported_address(
													&other,
													protocol_names.as_slice(),
													addr,
												);

											to_discover[swarm_n].remove(&other);
										},
										DiscoveryOut::RandomKademliaStarted => {},
										DiscoveryOut::ClosestPeersFound(..) => {},
										// libp2p emits this event when it is not particularly
										// happy, but this doesn't break the discovery.
										DiscoveryOut::ClosestPeersNotFound(..) => {},
										e => {
											panic!("Unexpected event: {:?}", e)
										},
									}
								},
								// ignore non Behaviour events
								_ => {},
							}
							continue 'polling
						},
						_ => {},
					}
				}
				break
			}

			if to_discover.iter().all(|l| l.is_empty()) {
				Poll::Ready(())
			} else {
				Poll::Pending
			}
		});

		fut.await
	}

	#[test]
	fn discovery_ignores_peers_with_unknown_protocols() {
		let supported_genesis_hash = H256::from_low_u64_be(1);
		let unsupported_genesis_hash = H256::from_low_u64_be(2);
		let supported_protocol_id = ProtocolId::from("a");
		let unsupported_protocol_id = ProtocolId::from("b");

		let mut discovery = {
			let keypair = Keypair::generate_ed25519();
			let mut config = DiscoveryConfig::new(keypair.public().to_peer_id());
			config
				.allow_private_ip(true)
				.allow_non_globals_in_dht(true)
				.discovery_limit(50)
				.with_kademlia(supported_genesis_hash, None, &supported_protocol_id);
			config.finish()
		};

		let predictable_peer_id = |bytes: &[u8; 32]| {
			Keypair::ed25519_from_bytes(bytes.to_owned()).unwrap().public().to_peer_id()
		};

		let remote_peer_id = predictable_peer_id(b"00000000000000000000000000000001");
		let remote_addr: Multiaddr = "/memory/1".parse().unwrap();
		let another_peer_id = predictable_peer_id(b"00000000000000000000000000000002");
		let another_addr: Multiaddr = "/memory/2".parse().unwrap();

		// Try adding remote peers with unsupported protocols.
		discovery.add_self_reported_address(
			&remote_peer_id,
			&[kademlia_protocol_name(unsupported_genesis_hash, None)],
			remote_addr.clone(),
		);
		discovery.add_self_reported_address(
			&another_peer_id,
			&[legacy_kademlia_protocol_name(&unsupported_protocol_id)],
			another_addr.clone(),
		);

		{
			let kademlia = discovery.kademlia.as_mut().unwrap();
			assert!(
				kademlia
					.kbucket(remote_peer_id)
					.expect("Remote peer id not to be equal to local peer id.")
					.is_empty(),
				"Expect peer with unsupported protocol not to be added."
			);
			assert!(
				kademlia
					.kbucket(another_peer_id)
					.expect("Remote peer id not to be equal to local peer id.")
					.is_empty(),
				"Expect peer with unsupported protocol not to be added."
			);
		}

		// Add remote peers with supported protocols.
		discovery.add_self_reported_address(
			&remote_peer_id,
			&[kademlia_protocol_name(supported_genesis_hash, None)],
			remote_addr.clone(),
		);
		{
			let kademlia = discovery.kademlia.as_mut().unwrap();
			assert!(
				!kademlia
					.kbucket(remote_peer_id)
					.expect("Remote peer id not to be equal to local peer id.")
					.is_empty(),
				"Expect peer with supported protocol to be added."
			);
		}

		let unsupported_peer_id = predictable_peer_id(b"00000000000000000000000000000002");
		let unsupported_peer_addr: Multiaddr = "/memory/2".parse().unwrap();

		// Check the unsupported peer is not present before and after the call.
		{
			let kademlia = discovery.kademlia.as_mut().unwrap();
			assert!(
				kademlia
					.kbucket(unsupported_peer_id)
					.expect("Remote peer id not to be equal to local peer id.")
					.is_empty(),
				"Expect unsupported peer not to be added."
			);
		}
		// Note: legacy protocol is not supported without genesis hash and fork ID,
		// if the legacy is the only protocol supported, then the peer will not be added.
		discovery.add_self_reported_address(
			&unsupported_peer_id,
			&[legacy_kademlia_protocol_name(&supported_protocol_id)],
			unsupported_peer_addr.clone(),
		);
		{
			let kademlia = discovery.kademlia.as_mut().unwrap();
			assert!(
				kademlia
					.kbucket(unsupported_peer_id)
					.expect("Remote peer id not to be equal to local peer id.")
					.is_empty(),
				"Expect unsupported peer not to be added."
			);
		}

		// Supported legacy and genesis based protocols are allowed to be added.
		discovery.add_self_reported_address(
			&another_peer_id,
			&[
				legacy_kademlia_protocol_name(&supported_protocol_id),
				kademlia_protocol_name(supported_genesis_hash, None),
			],
			another_addr.clone(),
		);

		{
			let kademlia = discovery.kademlia.as_mut().unwrap();
			assert_eq!(
				2,
				kademlia.kbuckets().fold(0, |acc, bucket| acc + bucket.num_entries()),
				"Expect peers with supported protocol to be added."
			);
			assert!(
				!kademlia
					.kbucket(another_peer_id)
					.expect("Remote peer id not to be equal to local peer id.")
					.is_empty(),
				"Expect peer with supported protocol to be added."
			);
		}
	}
}
