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

pub(crate) use crate::worker::addr_cache::AddrCache;
use crate::{
	error::{Error, Result},
	interval::ExpIncInterval,
	ServicetoWorkerMsg, WorkerConfig,
};

use std::{
	collections::{HashMap, HashSet},
	marker::PhantomData,
	path::PathBuf,
	sync::Arc,
	time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use futures::{channel::mpsc, future, stream::Fuse, FutureExt, Stream, StreamExt};

use codec::{Decode, Encode};
use ip_network::IpNetwork;
use linked_hash_set::LinkedHashSet;
use sc_network_types::kad::{Key, PeerRecord, Record};

use log::{debug, error, info, trace};
use prometheus_endpoint::{register, Counter, CounterVec, Gauge, Opts, U64};
use prost::Message;
use rand::{seq::SliceRandom, thread_rng};

use sc_network::{
	config::DEFAULT_KADEMLIA_REPLICATION_FACTOR, event::DhtEvent, multiaddr, KademliaKey,
	Multiaddr, NetworkDHTProvider, NetworkSigner, NetworkStateInfo,
};
use sc_network_types::{multihash::Code, PeerId};
use schema::PeerSignature;
use sp_api::{ApiError, ProvideRuntimeApi};
use sp_authority_discovery::{
	AuthorityDiscoveryApi, AuthorityId, AuthorityPair, AuthoritySignature,
};
use sp_blockchain::HeaderBackend;
use sp_core::{
	crypto::{key_types, ByteArray, Pair},
	traits::SpawnNamed,
};
use sp_keystore::{Keystore, KeystorePtr};
use sp_runtime::traits::Block as BlockT;

mod addr_cache;
/// Dht payload schemas generated from Protobuf definitions via Prost crate in build.rs.
mod schema {
	#[cfg(test)]
	mod tests;

	include!(concat!(env!("OUT_DIR"), "/authority_discovery_v3.rs"));
}
#[cfg(test)]
pub mod tests;

const LOG_TARGET: &str = "sub-authority-discovery";
pub(crate) const ADDR_CACHE_FILE_NAME: &str = "authority_discovery_addr_cache.json";
const ADDR_CACHE_PERSIST_INTERVAL: Duration = Duration::from_secs(60 * 10); // 10 minutes

/// Maximum number of addresses cached per authority. Additional addresses are discarded.
const MAX_ADDRESSES_PER_AUTHORITY: usize = 16;

/// Maximum number of global listen addresses published by the node.
const MAX_GLOBAL_LISTEN_ADDRESSES: usize = 4;

/// Maximum number of addresses to publish in a single record.
const MAX_ADDRESSES_TO_PUBLISH: usize = 32;

/// Maximum number of in-flight DHT lookups at any given point in time.
const MAX_IN_FLIGHT_LOOKUPS: usize = 8;

/// Role an authority discovery [`Worker`] can run as.
pub enum Role {
	/// Publish own addresses and discover addresses of others.
	PublishAndDiscover(KeystorePtr),
	/// Discover addresses of others.
	Discover,
}

/// An authority discovery [`Worker`] can publish the local node's addresses as well as discover
/// those of other nodes via a Kademlia DHT.
///
/// When constructed with [`Role::PublishAndDiscover`] a [`Worker`] will
///
///    1. Retrieve its external addresses (including peer id).
///
///    2. Get the list of keys owned by the local node participating in the current authority set.
///
///    3. Sign the addresses with the keys.
///
///    4. Put addresses and signature as a record with the authority id as a key on a Kademlia DHT.
///
/// When constructed with either [`Role::PublishAndDiscover`] or [`Role::Discover`] a [`Worker`]
/// will
///
///    1. Retrieve the current and next set of authorities.
///
///    2. Start DHT queries for the ids of the authorities.
///
///    3. Validate the signatures of the retrieved key value pairs.
///
///    4. Add the retrieved external addresses as priority nodes to the
///    network peerset.
///
///    5. Allow querying of the collected addresses via the [`crate::Service`].
pub struct Worker<Client, Block: BlockT, DhtEventStream> {
	/// Channel receiver for messages send by a [`crate::Service`].
	from_service: Fuse<mpsc::Receiver<ServicetoWorkerMsg>>,

	client: Arc<Client>,

	network: Arc<dyn NetworkProvider>,

	/// Channel we receive Dht events on.
	dht_event_rx: DhtEventStream,

	/// Interval to be proactive, publishing own addresses.
	publish_interval: ExpIncInterval,

	/// Pro-actively publish our own addresses at this interval, if the keys in the keystore
	/// have changed.
	publish_if_changed_interval: ExpIncInterval,

	/// List of keys onto which addresses have been published at the latest publication.
	/// Used to check whether they have changed.
	latest_published_keys: HashSet<AuthorityId>,
	/// List of the kademlia keys that have been published at the latest publication.
	/// Used to associate DHT events with our published records.
	latest_published_kad_keys: HashSet<KademliaKey>,

	/// Same value as in the configuration.
	publish_non_global_ips: bool,

	/// Public addresses set by the node operator to always publish first in the authority
	/// discovery DHT record.
	public_addresses: LinkedHashSet<Multiaddr>,

	/// Same value as in the configuration.
	strict_record_validation: bool,

	/// Interval at which to request addresses of authorities, refilling the pending lookups queue.
	query_interval: ExpIncInterval,

	/// Queue of throttled lookups pending to be passed to the network.
	pending_lookups: Vec<AuthorityId>,

	/// The list of all known authorities.
	known_authorities: HashMap<KademliaKey, AuthorityId>,

	/// The last time we requested the list of authorities.
	authorities_queried_at: Option<Block::Hash>,

	/// Set of in-flight lookups.
	in_flight_lookups: HashMap<KademliaKey, AuthorityId>,

	/// Set of lookups we can still receive records.
	/// These are the entries in the `in_flight_lookups` for which
	/// we got at least one successfull result.
	known_lookups: HashMap<KademliaKey, AuthorityId>,

	/// Last known record by key, here we always keep the record with
	/// the highest creation time and we don't accept records older than
	/// that.
	last_known_records: HashMap<KademliaKey, RecordInfo>,

	addr_cache: addr_cache::AddrCache,

	metrics: Option<Metrics>,

	/// Flag to ensure the warning about missing public addresses is only printed once.
	warn_public_addresses: bool,

	role: Role,

	phantom: PhantomData<Block>,

	/// A spawner of tasks
	spawner: Box<dyn SpawnNamed>,

	/// The directory of where the persisted AddrCache file is located,
	/// optional since NetworkConfiguration's `net_config_path` field
	/// is optional. If None, we won't persist the AddrCache at all.
	persisted_cache_file_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct RecordInfo {
	/// Time since UNIX_EPOCH in nanoseconds.
	creation_time: u128,
	/// Peers that we know have this record, bounded to no more than
	/// DEFAULT_KADEMLIA_REPLICATION_FACTOR(20).
	peers_with_record: HashSet<PeerId>,
	/// The record itself.
	record: Record,
}

/// Wrapper for [`AuthorityDiscoveryApi`](sp_authority_discovery::AuthorityDiscoveryApi). Can be
/// be implemented by any struct without dependency on the runtime.
#[async_trait::async_trait]
pub trait AuthorityDiscovery<Block: BlockT> {
	/// Retrieve authority identifiers of the current and next authority set.
	async fn authorities(&self, at: Block::Hash)
		-> std::result::Result<Vec<AuthorityId>, ApiError>;

	/// Retrieve best block hash
	async fn best_hash(&self) -> std::result::Result<Block::Hash, Error>;
}

#[async_trait::async_trait]
impl<Block, T> AuthorityDiscovery<Block> for T
where
	T: ProvideRuntimeApi<Block> + HeaderBackend<Block> + Send + Sync,
	T::Api: AuthorityDiscoveryApi<Block>,
	Block: BlockT,
{
	async fn authorities(
		&self,
		at: Block::Hash,
	) -> std::result::Result<Vec<AuthorityId>, ApiError> {
		self.runtime_api().authorities(at)
	}

	async fn best_hash(&self) -> std::result::Result<Block::Hash, Error> {
		Ok(self.info().best_hash)
	}
}

impl<Client, Block, DhtEventStream> Worker<Client, Block, DhtEventStream>
where
	Block: BlockT + Unpin + 'static,
	Client: AuthorityDiscovery<Block> + 'static,
	DhtEventStream: Stream<Item = DhtEvent> + Unpin,
{
	/// Construct a [`Worker`].
	pub(crate) fn new(
		from_service: mpsc::Receiver<ServicetoWorkerMsg>,
		client: Arc<Client>,
		network: Arc<dyn NetworkProvider>,
		dht_event_rx: DhtEventStream,
		role: Role,
		prometheus_registry: Option<prometheus_endpoint::Registry>,
		config: WorkerConfig,
		spawner: impl SpawnNamed + 'static,
	) -> Self {
		// When a node starts up publishing and querying might fail due to various reasons, for
		// example due to being not yet fully bootstrapped on the DHT. Thus one should retry rather
		// sooner than later. On the other hand, a long running node is likely well connected and
		// thus timely retries are not needed. For this reasoning use an exponentially increasing
		// interval for `publish_interval`, `query_interval` and `priority_group_set_interval`
		// instead of a constant interval.
		let publish_interval =
			ExpIncInterval::new(Duration::from_secs(2), config.max_publish_interval);
		let query_interval = ExpIncInterval::new(Duration::from_secs(2), config.max_query_interval);

		// An `ExpIncInterval` is overkill here because the interval is constant, but consistency
		// is more simple.
		let publish_if_changed_interval =
			ExpIncInterval::new(config.keystore_refresh_interval, config.keystore_refresh_interval);

		let maybe_persisted_cache_file_path =
			config.persisted_cache_directory.as_ref().map(|dir| {
				let mut path = dir.clone();
				path.push(ADDR_CACHE_FILE_NAME);
				path
			});

		// If we have a path to persisted cache file, then we will try to
		// load the contents of persisted cache from file, if it exists, and is valid.
		// Create a new one otherwise.
		let addr_cache: AddrCache = if let Some(persisted_cache_file_path) =
			maybe_persisted_cache_file_path.as_ref()
		{
			let loaded =
				AddrCache::try_from(persisted_cache_file_path.as_path()).unwrap_or_else(|e| {
					info!(target: LOG_TARGET, "Failed to load AddrCache from file, using empty instead: {}", e);
					AddrCache::new()
				});
			info!(target: LOG_TARGET, "Loaded persisted AddrCache with {} authority ids.", loaded.num_authority_ids());
			loaded
		} else {
			info!(target: LOG_TARGET, "No persisted cache file path provided, authority discovery will not persist the address cache to disk.");
			AddrCache::new()
		};

		let metrics = match prometheus_registry {
			Some(registry) => match Metrics::register(&registry) {
				Ok(metrics) => Some(metrics),
				Err(e) => {
					error!(target: LOG_TARGET, "Failed to register metrics: {}", e);
					None
				},
			},
			None => None,
		};

		let public_addresses = {
			let local_peer_id = network.local_peer_id();

			config
				.public_addresses
				.into_iter()
				.map(|address| AddressType::PublicAddress(address).without_p2p(local_peer_id))
				.collect()
		};

		Worker {
			from_service: from_service.fuse(),
			client,
			network,
			dht_event_rx,
			publish_interval,
			known_authorities: Default::default(),
			authorities_queried_at: None,
			publish_if_changed_interval,
			latest_published_keys: HashSet::new(),
			latest_published_kad_keys: HashSet::new(),
			publish_non_global_ips: config.publish_non_global_ips,
			public_addresses,
			strict_record_validation: config.strict_record_validation,
			query_interval,
			pending_lookups: Vec::new(),
			in_flight_lookups: HashMap::new(),
			known_lookups: HashMap::new(),
			addr_cache,
			role,
			metrics,
			warn_public_addresses: false,
			phantom: PhantomData,
			last_known_records: HashMap::new(),
			spawner: Box::new(spawner),
			persisted_cache_file_path: maybe_persisted_cache_file_path,
		}
	}

	/// Persists `AddrCache` to disk if the `persisted_cache_file_path` is set.
	pub fn persist_addr_cache_if_supported(&self) {
		let Some(path) = self.persisted_cache_file_path.as_ref().cloned() else {
			return;
		};
		let cloned_cache = self.addr_cache.clone();
		self.spawner.spawn_blocking(
			"persist-addr-cache",
			Some("authority-discovery-worker"),
			Box::pin(async move {
				cloned_cache.serialize_and_persist(path);
			}),
		)
	}

	/// Start the worker
	pub async fn run(mut self) {
		let mut persist_interval = tokio::time::interval(ADDR_CACHE_PERSIST_INTERVAL);

		loop {
			self.start_new_lookups();

			futures::select! {
				_ = persist_interval.tick().fuse() => {
					self.persist_addr_cache_if_supported();
				},
				// Process incoming events.
				event = self.dht_event_rx.next().fuse() => {
					if let Some(event) = event {
						self.handle_dht_event(event).await;
					} else {
						self.persist_addr_cache_if_supported();
						// This point is reached if the network has shut down, at which point there is not
						// much else to do than to shut down the authority discovery as well.
						return;
					}
				},
				// Handle messages from [`Service`]. Ignore if sender side is closed.
				msg = self.from_service.select_next_some() => {
					self.process_message_from_service(msg);
				},
				// Publish own addresses.
				only_if_changed = future::select(
					self.publish_interval.next().map(|_| false),
					self.publish_if_changed_interval.next().map(|_| true)
				).map(|e| e.factor_first().0).fuse() => {
					if let Err(e) = self.publish_ext_addresses(only_if_changed).await {
						error!(
							target: LOG_TARGET,
							"Failed to publish external addresses: {}", e,
						);
					}
				},
				// Request addresses of authorities.
				_ = self.query_interval.next().fuse() => {
					if let Err(e) = self.refill_pending_lookups_queue().await {
						error!(
							target: LOG_TARGET,
							"Failed to request addresses of authorities: {}", e,
						);
					}
				},
			}
		}
	}

	fn process_message_from_service(&self, msg: ServicetoWorkerMsg) {
		match msg {
			ServicetoWorkerMsg::GetAddressesByAuthorityId(authority, sender) => {
				let _ = sender.send(
					self.addr_cache.get_addresses_by_authority_id(&authority).map(Clone::clone),
				);
			},
			ServicetoWorkerMsg::GetAuthorityIdsByPeerId(peer_id, sender) => {
				let _ = sender
					.send(self.addr_cache.get_authority_ids_by_peer_id(&peer_id).map(Clone::clone));
			},
		}
	}

	fn addresses_to_publish(&mut self) -> impl Iterator<Item = Multiaddr> {
		let local_peer_id = self.network.local_peer_id();
		let publish_non_global_ips = self.publish_non_global_ips;

		// Checks that the address is global.
		let address_is_global = |address: &Multiaddr| {
			address.iter().all(|protocol| match protocol {
				// The `ip_network` library is used because its `is_global()` method is stable,
				// while `is_global()` in the standard library currently isn't.
				multiaddr::Protocol::Ip4(ip) => IpNetwork::from(ip).is_global(),
				multiaddr::Protocol::Ip6(ip) => IpNetwork::from(ip).is_global(),
				_ => true,
			})
		};

		// These are the addresses the node is listening for incoming connections,
		// as reported by installed protocols (tcp / websocket etc).
		//
		// We double check the address is global. In other words, we double check the node
		// is not running behind a NAT.
		// Note: we do this regardless of the `publish_non_global_ips` setting, since the
		// node discovers many external addresses via the identify protocol.
		let mut global_listen_addresses = self
			.network
			.listen_addresses()
			.into_iter()
			.filter_map(|address| {
				address_is_global(&address)
					.then(|| AddressType::GlobalListenAddress(address).without_p2p(local_peer_id))
			})
			.take(MAX_GLOBAL_LISTEN_ADDRESSES)
			.peekable();

		// Similar to listen addresses that takes into consideration `publish_non_global_ips`.
		let mut external_addresses = self
			.network
			.external_addresses()
			.into_iter()
			.filter_map(|address| {
				(publish_non_global_ips || address_is_global(&address))
					.then(|| AddressType::ExternalAddress(address).without_p2p(local_peer_id))
			})
			.peekable();

		let has_global_listen_addresses = global_listen_addresses.peek().is_some();
		trace!(
			target: LOG_TARGET,
			"Node has public addresses: {}, global listen addresses: {}, external addresses: {}",
			!self.public_addresses.is_empty(),
			has_global_listen_addresses,
			external_addresses.peek().is_some(),
		);

		let mut seen_addresses = HashSet::new();

		let addresses = self
			.public_addresses
			.clone()
			.into_iter()
			.chain(global_listen_addresses)
			.chain(external_addresses)
			// Deduplicate addresses.
			.filter(|address| seen_addresses.insert(address.clone()))
			.take(MAX_ADDRESSES_TO_PUBLISH)
			.collect::<Vec<_>>();

		if !addresses.is_empty() {
			debug!(
				target: LOG_TARGET,
				"Publishing authority DHT record peer_id='{local_peer_id}' with addresses='{addresses:?}'",
			);

			if !self.warn_public_addresses &&
				self.public_addresses.is_empty() &&
				!has_global_listen_addresses
			{
				self.warn_public_addresses = true;

				error!(
					target: LOG_TARGET,
					"No public addresses configured and no global listen addresses found. \
					Authority DHT record may contain unreachable addresses. \
					Consider setting `--public-addr` to the public IP address of this node. \
					This will become a hard requirement in future versions for authorities."
				);
			}
		}

		// The address must include the local peer id.
		addresses
			.into_iter()
			.map(move |a| a.with(multiaddr::Protocol::P2p(*local_peer_id.as_ref())))
	}

	/// Publish own public addresses.
	///
	/// If `only_if_changed` is true, the function has no effect if the list of keys to publish
	/// is equal to `self.latest_published_keys`.
	async fn publish_ext_addresses(&mut self, only_if_changed: bool) -> Result<()> {
		let key_store = match &self.role {
			Role::PublishAndDiscover(key_store) => key_store,
			Role::Discover => return Ok(()),
		}
		.clone();

		let addresses = serialize_addresses(self.addresses_to_publish());
		if addresses.is_empty() {
			trace!(
				target: LOG_TARGET,
				"No addresses to publish. Skipping publication."
			);

			self.publish_interval.set_to_start();
			return Ok(())
		}

		let keys =
			Worker::<Client, Block, DhtEventStream>::get_own_public_keys_within_authority_set(
				key_store.clone(),
				self.client.as_ref(),
			)
			.await?
			.into_iter()
			.collect::<HashSet<_>>();

		if only_if_changed {
			// If the authority keys did not change and the `publish_if_changed_interval` was
			// triggered then do nothing.
			if keys == self.latest_published_keys {
				return Ok(())
			}

			// We have detected a change in the authority keys, reset the timers to
			// publish and gather data faster.
			self.publish_interval.set_to_start();
			self.query_interval.set_to_start();
		}

		if let Some(metrics) = &self.metrics {
			metrics.publish.inc();
			metrics
				.amount_addresses_last_published
				.set(addresses.len().try_into().unwrap_or(std::u64::MAX));
		}

		let serialized_record = serialize_authority_record(addresses, Some(build_creation_time()))?;
		let peer_signature = sign_record_with_peer_id(&serialized_record, &self.network)?;

		let keys_vec = keys.iter().cloned().collect::<Vec<_>>();

		let kv_pairs = sign_record_with_authority_ids(
			serialized_record,
			Some(peer_signature),
			key_store.as_ref(),
			keys_vec,
		)?;

		self.latest_published_kad_keys = kv_pairs.iter().map(|(k, _)| k.clone()).collect();

		for (key, value) in kv_pairs.into_iter() {
			self.network.put_value(key, value);
		}

		self.latest_published_keys = keys;

		Ok(())
	}

	async fn refill_pending_lookups_queue(&mut self) -> Result<()> {
		let best_hash = self.client.best_hash().await?;

		let local_keys = match &self.role {
			Role::PublishAndDiscover(key_store) => key_store
				.sr25519_public_keys(key_types::AUTHORITY_DISCOVERY)
				.into_iter()
				.collect::<HashSet<_>>(),
			Role::Discover => HashSet::new(),
		};

		let mut authorities = self
			.client
			.authorities(best_hash)
			.await
			.map_err(|e| Error::CallingRuntime(e.into()))?
			.into_iter()
			.filter(|id| !local_keys.contains(id.as_ref()))
			.collect::<Vec<_>>();

		self.known_authorities = authorities
			.clone()
			.into_iter()
			.map(|authority| (hash_authority_id(authority.as_ref()), authority))
			.collect::<HashMap<_, _>>();
		self.authorities_queried_at = Some(best_hash);

		self.addr_cache.retain_ids(&authorities);
		let now = Instant::now();
		self.last_known_records.retain(|k, value| {
			self.known_authorities.contains_key(k) && !value.record.is_expired(now)
		});

		authorities.shuffle(&mut thread_rng());
		self.pending_lookups = authorities;
		// Ignore all still in-flight lookups. Those that are still in-flight are likely stalled as
		// query interval ticks are far enough apart for all lookups to succeed.
		self.in_flight_lookups.clear();
		self.known_lookups.clear();

		if let Some(metrics) = &self.metrics {
			metrics
				.requests_pending
				.set(self.pending_lookups.len().try_into().unwrap_or(std::u64::MAX));
		}

		Ok(())
	}

	fn start_new_lookups(&mut self) {
		while self.in_flight_lookups.len() < MAX_IN_FLIGHT_LOOKUPS {
			let authority_id = match self.pending_lookups.pop() {
				Some(authority) => authority,
				None => return,
			};
			let hash = hash_authority_id(authority_id.as_ref());
			self.network.get_value(&hash);
			self.in_flight_lookups.insert(hash, authority_id);

			if let Some(metrics) = &self.metrics {
				metrics.requests.inc();
				metrics
					.requests_pending
					.set(self.pending_lookups.len().try_into().unwrap_or(std::u64::MAX));
			}
		}
	}

	/// Handle incoming Dht events.
	async fn handle_dht_event(&mut self, event: DhtEvent) {
		match event {
			DhtEvent::ValueFound(v) => {
				if let Some(metrics) = &self.metrics {
					metrics.dht_event_received.with_label_values(&["value_found"]).inc();
				}

				debug!(target: LOG_TARGET, "Value for hash '{:?}' found on Dht.", v.record.key);

				if let Err(e) = self.handle_dht_value_found_event(v) {
					if let Some(metrics) = &self.metrics {
						metrics.handle_value_found_event_failure.inc();
					}
					debug!(target: LOG_TARGET, "Failed to handle Dht value found event: {}", e);
				}
			},
			DhtEvent::ValueNotFound(hash) => {
				if let Some(metrics) = &self.metrics {
					metrics.dht_event_received.with_label_values(&["value_not_found"]).inc();
				}

				if self.in_flight_lookups.remove(&hash).is_some() {
					debug!(target: LOG_TARGET, "Value for hash '{:?}' not found on Dht.", hash)
				} else {
					debug!(
						target: LOG_TARGET,
						"Received 'ValueNotFound' for unexpected hash '{:?}'.", hash
					)
				}
			},
			DhtEvent::ValuePut(hash) => {
				if !self.latest_published_kad_keys.contains(&hash) {
					return;
				}

				// Fast forward the exponentially increasing interval to the configured maximum. In
				// case this was the first successful address publishing there is no need for a
				// timely retry.
				self.publish_interval.set_to_max();

				if let Some(metrics) = &self.metrics {
					metrics.dht_event_received.with_label_values(&["value_put"]).inc();
				}

				debug!(target: LOG_TARGET, "Successfully put hash '{:?}' on Dht.", hash)
			},
			DhtEvent::ValuePutFailed(hash) => {
				if !self.latest_published_kad_keys.contains(&hash) {
					// Not a value we have published or received multiple times.
					return;
				}

				if let Some(metrics) = &self.metrics {
					metrics.dht_event_received.with_label_values(&["value_put_failed"]).inc();
				}

				debug!(target: LOG_TARGET, "Failed to put hash '{:?}' on Dht.", hash)
			},
			DhtEvent::PutRecordRequest(record_key, record_value, publisher, expires) => {
				if let Err(e) = self
					.handle_put_record_requested(record_key, record_value, publisher, expires)
					.await
				{
					debug!(target: LOG_TARGET, "Failed to handle put record request: {}", e)
				}

				if let Some(metrics) = &self.metrics {
					metrics.dht_event_received.with_label_values(&["put_record_req"]).inc();
				}
			},
			_ => {},
		}
	}

	async fn handle_put_record_requested(
		&mut self,
		record_key: Key,
		record_value: Vec<u8>,
		publisher: Option<PeerId>,
		expires: Option<std::time::Instant>,
	) -> Result<()> {
		let publisher = publisher.ok_or(Error::MissingPublisher)?;

		// Make sure we don't ever work with an outdated set of authorities
		// and that we do not update known_authorithies too often.
		let best_hash = self.client.best_hash().await?;
		if !self.known_authorities.contains_key(&record_key) &&
			self.authorities_queried_at
				.map(|authorities_queried_at| authorities_queried_at != best_hash)
				.unwrap_or(true)
		{
			let authorities = self
				.client
				.authorities(best_hash)
				.await
				.map_err(|e| Error::CallingRuntime(e.into()))?
				.into_iter()
				.collect::<Vec<_>>();

			self.known_authorities = authorities
				.into_iter()
				.map(|authority| (hash_authority_id(authority.as_ref()), authority))
				.collect::<HashMap<_, _>>();

			self.authorities_queried_at = Some(best_hash);
		}

		let authority_id =
			self.known_authorities.get(&record_key).ok_or(Error::UnknownAuthority)?;
		let signed_record =
			Self::check_record_signed_with_authority_id(record_value.as_slice(), authority_id)?;
		self.check_record_signed_with_network_key(
			&signed_record.record,
			signed_record.peer_signature,
			publisher,
			authority_id,
		)?;

		let records_creation_time: u128 =
			schema::AuthorityRecord::decode(signed_record.record.as_slice())
				.map_err(Error::DecodingProto)?
				.creation_time
				.map(|creation_time| {
					u128::decode(&mut &creation_time.timestamp[..]).unwrap_or_default()
				})
				.unwrap_or_default(); // 0 is a sane default for records that do not have creation time present.

		let current_record_info = self.last_known_records.get(&record_key);
		// If record creation time is older than the current record creation time,
		// we don't store it since we want to give higher priority to newer records.
		if let Some(current_record_info) = current_record_info {
			if records_creation_time < current_record_info.creation_time {
				debug!(
					target: LOG_TARGET,
					"Skip storing because record creation time {:?} is older than the current known record {:?}",
					records_creation_time,
					current_record_info.creation_time
				);
				return Ok(());
			}
		}

		self.network.store_record(record_key, record_value, Some(publisher), expires);
		Ok(())
	}

	fn check_record_signed_with_authority_id(
		record: &[u8],
		authority_id: &AuthorityId,
	) -> Result<schema::SignedAuthorityRecord> {
		let signed_record: schema::SignedAuthorityRecord =
			schema::SignedAuthorityRecord::decode(record).map_err(Error::DecodingProto)?;

		let auth_signature = AuthoritySignature::decode(&mut &signed_record.auth_signature[..])
			.map_err(Error::EncodingDecodingScale)?;

		if !AuthorityPair::verify(&auth_signature, &signed_record.record, &authority_id) {
			return Err(Error::VerifyingDhtPayload)
		}

		Ok(signed_record)
	}

	fn check_record_signed_with_network_key(
		&self,
		record: &Vec<u8>,
		peer_signature: Option<PeerSignature>,
		remote_peer_id: PeerId,
		authority_id: &AuthorityId,
	) -> Result<()> {
		if let Some(peer_signature) = peer_signature {
			match self.network.verify(
				remote_peer_id.into(),
				&peer_signature.public_key,
				&peer_signature.signature,
				record,
			) {
				Ok(true) => {},
				Ok(false) => return Err(Error::VerifyingDhtPayload),
				Err(error) => return Err(Error::ParsingLibp2pIdentity(error)),
			}
		} else if self.strict_record_validation {
			return Err(Error::MissingPeerIdSignature)
		} else {
			debug!(
				target: LOG_TARGET,
				"Received unsigned authority discovery record from {}", authority_id
			);
		}
		Ok(())
	}

	fn handle_dht_value_found_event(&mut self, peer_record: PeerRecord) -> Result<()> {
		// Ensure `values` is not empty and all its keys equal.
		let remote_key = peer_record.record.key.clone();

		let authority_id: AuthorityId =
			if let Some(authority_id) = self.in_flight_lookups.remove(&remote_key) {
				self.known_lookups.insert(remote_key.clone(), authority_id.clone());
				authority_id
			} else if let Some(authority_id) = self.known_lookups.get(&remote_key) {
				authority_id.clone()
			} else {
				return Err(Error::ReceivingUnexpectedRecord);
			};

		let local_peer_id = self.network.local_peer_id();

		let schema::SignedAuthorityRecord { record, peer_signature, .. } =
			Self::check_record_signed_with_authority_id(
				peer_record.record.value.as_slice(),
				&authority_id,
			)?;

		let authority_record =
			schema::AuthorityRecord::decode(record.as_slice()).map_err(Error::DecodingProto)?;

		let records_creation_time: u128 = authority_record
			.creation_time
			.as_ref()
			.map(|creation_time| {
				u128::decode(&mut &creation_time.timestamp[..]).unwrap_or_default()
			})
			.unwrap_or_default(); // 0 is a sane default for records that do not have creation time present.

		let addresses: Vec<Multiaddr> = authority_record
			.addresses
			.into_iter()
			.map(|a| a.try_into())
			.collect::<std::result::Result<_, _>>()
			.map_err(Error::ParsingMultiaddress)?;

		let get_peer_id = |a: &Multiaddr| match a.iter().last() {
			Some(multiaddr::Protocol::P2p(key)) => PeerId::from_multihash(key).ok(),
			_ => None,
		};

		// Ignore [`Multiaddr`]s without [`PeerId`] or with own addresses.
		let addresses: Vec<Multiaddr> = addresses
			.into_iter()
			.filter(|a| get_peer_id(&a).filter(|p| *p != local_peer_id).is_some())
			.collect();

		let remote_peer_id = single(addresses.iter().map(|a| get_peer_id(&a)))
			.map_err(|_| Error::ReceivingDhtValueFoundEventWithDifferentPeerIds)? // different peer_id in records
			.flatten()
			.ok_or(Error::ReceivingDhtValueFoundEventWithNoPeerIds)?; // no records with peer_id in them

		// At this point we know all the valid multiaddresses from the record, know that
		// each of them belong to the same PeerId, we just need to check if the record is
		// properly signed by the owner of the PeerId
		self.check_record_signed_with_network_key(
			&record,
			peer_signature,
			remote_peer_id,
			&authority_id,
		)?;

		let remote_addresses: Vec<Multiaddr> =
			addresses.into_iter().take(MAX_ADDRESSES_PER_AUTHORITY).collect();

		let answering_peer_id = peer_record.peer.map(|peer| peer.into());

		let addr_cache_needs_update = self.handle_new_record(
			&authority_id,
			remote_key.clone(),
			RecordInfo {
				creation_time: records_creation_time,
				peers_with_record: answering_peer_id.into_iter().collect(),
				record: peer_record.record,
			},
		);

		if !remote_addresses.is_empty() && addr_cache_needs_update {
			self.addr_cache.insert(authority_id, remote_addresses);
			if let Some(metrics) = &self.metrics {
				metrics
					.known_authorities_count
					.set(self.addr_cache.num_authority_ids().try_into().unwrap_or(std::u64::MAX));
			}
		}
		Ok(())
	}

	// Handles receiving a new DHT record for the authorithy.
	// Returns true if the record was new, false if the record was older than the current one.
	fn handle_new_record(
		&mut self,
		authority_id: &AuthorityId,
		kademlia_key: KademliaKey,
		new_record: RecordInfo,
	) -> bool {
		let current_record_info = self
			.last_known_records
			.entry(kademlia_key.clone())
			.or_insert_with(|| new_record.clone());

		if new_record.creation_time > current_record_info.creation_time {
			let peers_that_need_updating = current_record_info.peers_with_record.clone();
			self.network.put_record_to(
				new_record.record.clone(),
				peers_that_need_updating.clone(),
				// If this is empty it means we received the answer from our node local
				// storage, so we need to update that as well.
				current_record_info.peers_with_record.is_empty(),
			);
			debug!(
					target: LOG_TARGET,
					"Found a newer record for {:?} new record creation time {:?} old record creation time {:?}",
					authority_id, new_record.creation_time, current_record_info.creation_time
			);
			self.last_known_records.insert(kademlia_key, new_record);
			return true
		}

		if new_record.creation_time == current_record_info.creation_time {
			// Same record just update in case this is a record from old nodes that don't have
			// timestamp.
			debug!(
					target: LOG_TARGET,
					"Found same record for {:?} record creation time {:?}",
					authority_id, new_record.creation_time
			);
			if current_record_info.peers_with_record.len() + new_record.peers_with_record.len() <=
				DEFAULT_KADEMLIA_REPLICATION_FACTOR
			{
				current_record_info.peers_with_record.extend(new_record.peers_with_record);
			}
			return true
		}

		debug!(
				target: LOG_TARGET,
				"Found old record for {:?} received record creation time {:?} current record creation time {:?}",
				authority_id, new_record.creation_time, current_record_info.creation_time,
		);
		self.network.put_record_to(
			current_record_info.record.clone().into(),
			new_record.peers_with_record.clone(),
			// If this is empty it means we received the answer from our node local
			// storage, so we need to update that as well.
			new_record.peers_with_record.is_empty(),
		);
		return false
	}

	/// Retrieve our public keys within the current and next authority set.
	// A node might have multiple authority discovery keys within its keystore, e.g. an old one and
	// one for the upcoming session. In addition it could be participating in the current and (/ or)
	// next authority set with two keys. The function does not return all of the local authority
	// discovery public keys, but only the ones intersecting with the current or next authority set.
	async fn get_own_public_keys_within_authority_set(
		key_store: KeystorePtr,
		client: &Client,
	) -> Result<HashSet<AuthorityId>> {
		let local_pub_keys = key_store
			.sr25519_public_keys(key_types::AUTHORITY_DISCOVERY)
			.into_iter()
			.collect::<HashSet<_>>();

		let best_hash = client.best_hash().await?;
		let authorities = client
			.authorities(best_hash)
			.await
			.map_err(|e| Error::CallingRuntime(e.into()))?
			.into_iter()
			.map(Into::into)
			.collect::<HashSet<_>>();

		let intersection =
			local_pub_keys.intersection(&authorities).cloned().map(Into::into).collect();

		Ok(intersection)
	}
}

/// Removes the `/p2p/..` from the address if it is present.
#[derive(Debug, Clone, PartialEq, Eq)]
enum AddressType {
	/// The address is specified as a public address via the CLI.
	PublicAddress(Multiaddr),
	/// The address is a global listen address.
	GlobalListenAddress(Multiaddr),
	/// The address is discovered via the network (ie /identify protocol).
	ExternalAddress(Multiaddr),
}

impl AddressType {
	/// Removes the `/p2p/..` from the address if it is present.
	///
	/// In case the peer id in the address does not match the local peer id, an error is logged for
	/// `ExternalAddress` and `GlobalListenAddress`.
	fn without_p2p(self, local_peer_id: PeerId) -> Multiaddr {
		// Get the address and the source str for logging.
		let (mut address, source) = match self {
			AddressType::PublicAddress(address) => (address, "public address"),
			AddressType::GlobalListenAddress(address) => (address, "global listen address"),
			AddressType::ExternalAddress(address) => (address, "external address"),
		};

		if let Some(multiaddr::Protocol::P2p(peer_id)) = address.iter().last() {
			if peer_id != *local_peer_id.as_ref() {
				error!(
					target: LOG_TARGET,
					"Network returned '{source}' '{address}' with peer id \
					 not matching the local peer id '{local_peer_id}'.",
				);
			}
			address.pop();
		}
		address
	}
}

/// NetworkProvider provides [`Worker`] with all necessary hooks into the
/// underlying Substrate networking. Using this trait abstraction instead of
/// `sc_network::NetworkService` directly is necessary to unit test [`Worker`].
pub trait NetworkProvider:
	NetworkDHTProvider + NetworkStateInfo + NetworkSigner + Send + Sync
{
}

impl<T> NetworkProvider for T where
	T: NetworkDHTProvider + NetworkStateInfo + NetworkSigner + Send + Sync
{
}

fn hash_authority_id(id: &[u8]) -> KademliaKey {
	KademliaKey::new(&Code::Sha2_256.digest(id).digest())
}

// Makes sure all values are the same and returns it
//
// Returns Err(_) if not all values are equal. Returns Ok(None) if there are
// no values.
fn single<T>(values: impl IntoIterator<Item = T>) -> std::result::Result<Option<T>, ()>
where
	T: PartialEq<T>,
{
	values.into_iter().try_fold(None, |acc, item| match acc {
		None => Ok(Some(item)),
		Some(ref prev) if *prev != item => Err(()),
		Some(x) => Ok(Some(x)),
	})
}

fn serialize_addresses(addresses: impl Iterator<Item = Multiaddr>) -> Vec<Vec<u8>> {
	addresses.map(|a| a.to_vec()).collect()
}

fn build_creation_time() -> schema::TimestampInfo {
	let creation_time = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map(|time| time.as_nanos())
		.unwrap_or_default();
	schema::TimestampInfo { timestamp: creation_time.encode() }
}

fn serialize_authority_record(
	addresses: Vec<Vec<u8>>,
	creation_time: Option<schema::TimestampInfo>,
) -> Result<Vec<u8>> {
	let mut serialized_record = vec![];

	schema::AuthorityRecord { addresses, creation_time }
		.encode(&mut serialized_record)
		.map_err(Error::EncodingProto)?;
	Ok(serialized_record)
}

fn sign_record_with_peer_id(
	serialized_record: &[u8],
	network: &impl NetworkSigner,
) -> Result<schema::PeerSignature> {
	let signature = network
		.sign_with_local_identity(serialized_record.to_vec())
		.map_err(|e| Error::CannotSign(format!("{} (network packet)", e)))?;
	let public_key = signature.public_key.encode_protobuf();
	let signature = signature.bytes;
	Ok(schema::PeerSignature { signature, public_key })
}

fn sign_record_with_authority_ids(
	serialized_record: Vec<u8>,
	peer_signature: Option<schema::PeerSignature>,
	key_store: &dyn Keystore,
	keys: Vec<AuthorityId>,
) -> Result<Vec<(KademliaKey, Vec<u8>)>> {
	let mut result = Vec::with_capacity(keys.len());

	for key in keys.iter() {
		let auth_signature = key_store
			.sr25519_sign(key_types::AUTHORITY_DISCOVERY, key.as_ref(), &serialized_record)
			.map_err(|e| Error::CannotSign(format!("{}. Key: {:?}", e, key)))?
			.ok_or_else(|| {
				Error::CannotSign(format!("Could not find key in keystore. Key: {:?}", key))
			})?;

		// Scale encode
		let auth_signature = auth_signature.encode();
		let signed_record = schema::SignedAuthorityRecord {
			record: serialized_record.clone(),
			auth_signature,
			peer_signature: peer_signature.clone(),
		}
		.encode_to_vec();

		result.push((hash_authority_id(key.as_slice()), signed_record));
	}

	Ok(result)
}

/// Prometheus metrics for a [`Worker`].
#[derive(Clone)]
pub(crate) struct Metrics {
	publish: Counter<U64>,
	amount_addresses_last_published: Gauge<U64>,
	requests: Counter<U64>,
	requests_pending: Gauge<U64>,
	dht_event_received: CounterVec<U64>,
	handle_value_found_event_failure: Counter<U64>,
	known_authorities_count: Gauge<U64>,
}

impl Metrics {
	pub(crate) fn register(registry: &prometheus_endpoint::Registry) -> Result<Self> {
		Ok(Self {
			publish: register(
				Counter::new(
					"substrate_authority_discovery_times_published_total",
					"Number of times authority discovery has published external addresses.",
				)?,
				registry,
			)?,
			amount_addresses_last_published: register(
				Gauge::new(
					"substrate_authority_discovery_amount_external_addresses_last_published",
					"Number of external addresses published when authority discovery last \
					 published addresses.",
				)?,
				registry,
			)?,
			requests: register(
				Counter::new(
					"substrate_authority_discovery_authority_addresses_requested_total",
					"Number of times authority discovery has requested external addresses of a \
					 single authority.",
				)?,
				registry,
			)?,
			requests_pending: register(
				Gauge::new(
					"substrate_authority_discovery_authority_address_requests_pending",
					"Number of pending authority address requests.",
				)?,
				registry,
			)?,
			dht_event_received: register(
				CounterVec::new(
					Opts::new(
						"substrate_authority_discovery_dht_event_received",
						"Number of dht events received by authority discovery.",
					),
					&["name"],
				)?,
				registry,
			)?,
			handle_value_found_event_failure: register(
				Counter::new(
					"substrate_authority_discovery_handle_value_found_event_failure",
					"Number of times handling a dht value found event failed.",
				)?,
				registry,
			)?,
			known_authorities_count: register(
				Gauge::new(
					"substrate_authority_discovery_known_authorities_count",
					"Number of authorities known by authority discovery.",
				)?,
				registry,
			)?,
		})
	}
}

// Helper functions for unit testing.
#[cfg(test)]
impl<Block: BlockT, Client, DhtEventStream> Worker<Client, Block, DhtEventStream> {
	pub(crate) fn inject_addresses(&mut self, authority: AuthorityId, addresses: Vec<Multiaddr>) {
		self.addr_cache.insert(authority, addresses)
	}

	pub(crate) fn contains_authority(&self, authority: &AuthorityId) -> bool {
		self.addr_cache.get_addresses_by_authority_id(authority).is_some()
	}
}
