// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

#![warn(missing_docs)]

//! Substrate authority discovery.
//!
//! This crate enables Substrate authorities to directly connect to other authorities.
//! [`AuthorityDiscovery`] implements the Future trait. By polling [`AuthorityDiscovery`] an
//! authority:
//!
//!
//! 1. **Makes itself discoverable**
//!
//!    1. Retrieves its external addresses (including peer id) or the ones of its sentry nodes.
//!
//!    2. Signs the above.
//!
//!    3. Puts the signature and the addresses on the libp2p Kademlia DHT.
//!
//!
//! 2. **Discovers other authorities**
//!
//!    1. Retrieves the current set of authorities.
//!
//!    2. Starts DHT queries for the ids of the authorities.
//!
//!    3. Validates the signatures of the retrieved key value pairs.
//!
//!    4. Adds the retrieved external addresses as priority nodes to the peerset.
//!
//! When run as a sentry node, the authority discovery module does not
//! publish any addresses to the DHT but still discovers validators and
//! sentry nodes of validators, i.e. only step 2 (Discovers other authorities)
//! is executed.

use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::task::{Context, Poll};
use futures::{Future, FutureExt, ready, Stream, StreamExt};
use futures_timer::Delay;

use codec::{Decode, Encode};
use error::{Error, Result};
use log::{debug, error, log_enabled, warn};
use prometheus_endpoint::{Counter, CounterVec, Gauge, Opts, U64, register};
use prost::Message;
use sc_client_api::blockchain::HeaderBackend;
use sc_network::{Multiaddr, config::MultiaddrWithPeerId, DhtEvent, ExHashT, NetworkStateInfo};
use sp_authority_discovery::{AuthorityDiscoveryApi, AuthorityId, AuthoritySignature, AuthorityPair};
use sp_core::crypto::{key_types, Pair};
use sp_core::traits::BareCryptoStorePtr;
use sp_runtime::{traits::Block as BlockT, generic::BlockId};
use sp_api::ProvideRuntimeApi;
use addr_cache::AddrCache;

#[cfg(test)]
mod tests;

mod error;
mod addr_cache;
/// Dht payload schemas generated from Protobuf definitions via Prost crate in build.rs.
mod schema {
	include!(concat!(env!("OUT_DIR"), "/authority_discovery.rs"));
}

type Interval = Box<dyn Stream<Item = ()> + Unpin + Send + Sync>;

const LOG_TARGET: &'static str = "sub-authority-discovery";

/// Upper bound estimation on how long one should wait before accessing the Kademlia DHT.
const LIBP2P_KADEMLIA_BOOTSTRAP_TIME: Duration = Duration::from_secs(30);

/// Name of the Substrate peerset priority group for authorities discovered through the authority
/// discovery module.
const AUTHORITIES_PRIORITY_GROUP_NAME: &'static str = "authorities";

/// Role an authority discovery module can run as.
pub enum Role {
	/// Actual authority as well as a reference to its key store.
	Authority(BareCryptoStorePtr),
	/// Sentry node that guards an authority.
	///
	/// No reference to its key store needed, as sentry nodes don't have an identity to sign
	/// addresses with in the first place.
	Sentry,
}

/// An `AuthorityDiscovery` makes a given authority discoverable and discovers other authorities.
pub struct AuthorityDiscovery<Client, Network, Block>
where
	Block: BlockT + 'static,
	Network: NetworkProvider,
	Client: ProvideRuntimeApi<Block> + Send + Sync + 'static + HeaderBackend<Block>,
	<Client as ProvideRuntimeApi<Block>>::Api: AuthorityDiscoveryApi<Block>,
{
	client: Arc<Client>,

	network: Arc<Network>,
	/// List of sentry node public addresses.
	//
	// There are 3 states:
	//   - None: No addresses were specified.
	//   - Some(vec![]): Addresses were specified, but none could be parsed as proper
	//     Multiaddresses.
	//   - Some(vec![a, b, c, ...]): Valid addresses were specified.
	sentry_nodes: Option<Vec<Multiaddr>>,
	/// Channel we receive Dht events on.
	dht_event_rx: Pin<Box<dyn Stream<Item = DhtEvent> + Send>>,

	/// Interval to be proactive, publishing own addresses.
	publish_interval: Interval,
	/// Interval on which to query for addresses of other authorities.
	query_interval: Interval,

	addr_cache: addr_cache::AddrCache<AuthorityId, Multiaddr>,

	metrics: Option<Metrics>,

	role: Role,

	phantom: PhantomData<Block>,
}

impl<Client, Network, Block> AuthorityDiscovery<Client, Network, Block>
where
	Block: BlockT + Unpin + 'static,
	Network: NetworkProvider,
	Client: ProvideRuntimeApi<Block> + Send + Sync + 'static + HeaderBackend<Block>,
	<Client as ProvideRuntimeApi<Block>>::Api:
		AuthorityDiscoveryApi<Block, Error = sp_blockchain::Error>,
	Self: Future<Output = ()>,
{
	/// Return a new authority discovery.
	///
	/// Note: When specifying `sentry_nodes` this module will not advertise the public addresses of
	/// the node itself but only the public addresses of its sentry nodes.
	pub fn new(
		client: Arc<Client>,
		network: Arc<Network>,
		sentry_nodes: Vec<MultiaddrWithPeerId>,
		dht_event_rx: Pin<Box<dyn Stream<Item = DhtEvent> + Send>>,
		role: Role,
		prometheus_registry: Option<prometheus_endpoint::Registry>,
	) -> Self {
		// Kademlia's default time-to-live for Dht records is 36h, republishing records every 24h.
		// Given that a node could restart at any point in time, one can not depend on the
		// republishing process, thus publishing own external addresses should happen on an interval
		// < 36h.
		let publish_interval = interval_at(
			Instant::now() + LIBP2P_KADEMLIA_BOOTSTRAP_TIME,
			Duration::from_secs(12 * 60 * 60),
		);

		// External addresses of other authorities can change at any given point in time. The
		// interval on which to query for external addresses of other authorities is a trade off
		// between efficiency and performance.
		let query_interval = interval_at(
			Instant::now() + LIBP2P_KADEMLIA_BOOTSTRAP_TIME,
			Duration::from_secs(10 * 60),
		);

		let sentry_nodes = if !sentry_nodes.is_empty() {
			Some(sentry_nodes.into_iter().map(|ma| ma.concat()).collect::<Vec<_>>())
		} else {
			None
		};

		let addr_cache = AddrCache::new();

		let metrics = match prometheus_registry {
			Some(registry) => {
				match Metrics::register(&registry) {
					Ok(metrics) => Some(metrics),
					Err(e) => {
						error!(target: LOG_TARGET, "Failed to register metrics: {:?}", e);
						None
					},
				}
			},
			None => None,
		};

		AuthorityDiscovery {
			client,
			network,
			sentry_nodes,
			dht_event_rx,
			publish_interval,
			query_interval,
			addr_cache,
			role,
			metrics,
			phantom: PhantomData,
		}
	}

	/// Publish either our own or if specified the public addresses of our sentry nodes.
	fn publish_ext_addresses(&mut self) -> Result<()> {
		let key_store = match &self.role {
			Role::Authority(key_store) => key_store,
			// Only authority nodes can put addresses (their own or the ones of their sentry nodes)
			// on the Dht. Sentry nodes don't have a known identity to authenticate such addresses,
			// thus `publish_ext_addresses` becomes a no-op.
			Role::Sentry => return Ok(()),
		};

		if let Some(metrics) = &self.metrics {
			metrics.publish.inc()
		}

		let addresses: Vec<_> = match &self.sentry_nodes {
			Some(addrs) => addrs.clone().into_iter()
				.map(|a| a.to_vec())
				.collect(),
			None => self.network.external_addresses()
				.into_iter()
				.map(|a| a.with(libp2p::core::multiaddr::Protocol::P2p(
					self.network.local_peer_id().into(),
				)))
				.map(|a| a.to_vec())
				.collect(),
		};

		if let Some(metrics) = &self.metrics {
			metrics.amount_last_published.set(addresses.len() as u64);
		}

		let mut serialized_addresses = vec![];
		schema::AuthorityAddresses { addresses }
			.encode(&mut serialized_addresses)
			.map_err(Error::EncodingProto)?;

		let keys = AuthorityDiscovery::get_own_public_keys_within_authority_set(
			&key_store,
			&self.client,
		)?.into_iter().map(Into::into).collect::<Vec<_>>();

		let signatures = key_store.read()
			.sign_with_all(
				key_types::AUTHORITY_DISCOVERY,
				keys.clone(),
				serialized_addresses.as_slice(),
			)
			.map_err(|_| Error::Signing)?;

		for (sign_result, key) in signatures.iter().zip(keys) {
			let mut signed_addresses = vec![];

			// sign_with_all returns Result<Signature, Error> signature
			// is generated for a public key that is supported.
			// Verify that all signatures exist for all provided keys.
			let signature = sign_result.as_ref().map_err(|_| Error::MissingSignature(key.clone()))?;
			schema::SignedAuthorityAddresses {
				addresses: serialized_addresses.clone(),
				signature: Encode::encode(&signature),
			}
			.encode(&mut signed_addresses)
				.map_err(Error::EncodingProto)?;

			self.network.put_value(
				hash_authority_id(key.1.as_ref()),
				signed_addresses,
			);
		}

		Ok(())
	}

	fn request_addresses_of_others(&mut self) -> Result<()> {
		let id = BlockId::hash(self.client.info().best_hash);

		let authorities = self
			.client
			.runtime_api()
			.authorities(&id)
			.map_err(Error::CallingRuntime)?;

		for authority_id in authorities.iter() {
			if let Some(metrics) = &self.metrics {
				metrics.request.inc();
			}

			self.network
				.get_value(&hash_authority_id(authority_id.as_ref()));
		}

		Ok(())
	}

	/// Handle incoming Dht events.
	///
	/// Returns either:
	///   - Poll::Pending when there are no more events to handle or
	///   - Poll::Ready(()) when the dht event stream terminated.
	fn handle_dht_events(&mut self, cx: &mut Context) -> Poll<()>{
		loop {
			match ready!(self.dht_event_rx.poll_next_unpin(cx)) {
				Some(DhtEvent::ValueFound(v)) => {
					if let Some(metrics) = &self.metrics {
						metrics.dht_event_received.with_label_values(&["value_found"]).inc();
					}

					if log_enabled!(log::Level::Debug) {
						let hashes = v.iter().map(|(hash, _value)| hash.clone());
						debug!(
							target: LOG_TARGET,
							"Value for hash '{:?}' found on Dht.", hashes,
						);
					}

					if let Err(e) = self.handle_dht_value_found_event(v) {
						error!(
							target: LOG_TARGET,
							"Failed to handle Dht value found event: {:?}", e,
						);
					}
				}
				Some(DhtEvent::ValueNotFound(hash)) => {
					if let Some(metrics) = &self.metrics {
						metrics.dht_event_received.with_label_values(&["value_not_found"]).inc();
					}

					debug!(
						target: LOG_TARGET,
						"Value for hash '{:?}' not found on Dht.", hash
					)
				},
				Some(DhtEvent::ValuePut(hash)) => {
					if let Some(metrics) = &self.metrics {
						metrics.dht_event_received.with_label_values(&["value_put"]).inc();
					}

					debug!(
						target: LOG_TARGET,
						"Successfully put hash '{:?}' on Dht.", hash,
					)
				},
				Some(DhtEvent::ValuePutFailed(hash)) => {
					if let Some(metrics) = &self.metrics {
						metrics.dht_event_received.with_label_values(&["value_put_failed"]).inc();
					}

					warn!(
						target: LOG_TARGET,
						"Failed to put hash '{:?}' on Dht.", hash
					)
				},
				None => {
					debug!(target: LOG_TARGET, "Dht event stream terminated.");
					return Poll::Ready(());
				},
			}
		}
	}

	fn handle_dht_value_found_event(
		&mut self,
		values: Vec<(libp2p::kad::record::Key, Vec<u8>)>,
	) -> Result<()> {
		// Ensure `values` is not empty and all its keys equal.
		let remote_key = values.iter().fold(Ok(None), |acc, (key, _)| {
			match acc {
				Ok(None) => Ok(Some(key.clone())),
				Ok(Some(ref prev_key)) if prev_key != key => Err(
					Error::ReceivingDhtValueFoundEventWithDifferentKeys
				),
				x @ Ok(_) => x,
				Err(e) => Err(e),
			}
		})?.ok_or(Error::ReceivingDhtValueFoundEventWithNoRecords)?;

		let authorities = {
			let block_id = BlockId::hash(self.client.info().best_hash);
			// From the Dht we only get the hashed authority id. In order to retrieve the actual
			// authority id and to ensure it is actually an authority, we match the hash against the
			// hash of the authority id of all other authorities.
			let authorities = self.client.runtime_api().authorities(&block_id)?;
			self.addr_cache.retain_ids(&authorities);
			authorities
				.into_iter()
				.map(|id| (hash_authority_id(id.as_ref()), id))
				.collect::<HashMap<_, _>>()
		};

		// Check if the event origins from an authority in the current authority set.
		let authority_id: &AuthorityId = authorities
			.get(&remote_key)
			.ok_or(Error::MatchingHashedAuthorityIdWithAuthorityId)?;

		let remote_addresses: Vec<Multiaddr> = values.into_iter()
			.map(|(_k, v)| {
				let schema::SignedAuthorityAddresses { signature, addresses } =
					schema::SignedAuthorityAddresses::decode(v.as_slice())
					.map_err(Error::DecodingProto)?;

				let signature = AuthoritySignature::decode(&mut &signature[..])
					.map_err(Error::EncodingDecodingScale)?;

				if !AuthorityPair::verify(&signature, &addresses, authority_id) {
					return Err(Error::VerifyingDhtPayload);
				}

				let addresses = schema::AuthorityAddresses::decode(addresses.as_slice())
					.map(|a| a.addresses)
					.map_err(Error::DecodingProto)?
					.into_iter()
					.map(|a| a.try_into())
					.collect::<std::result::Result<_, _>>()
					.map_err(Error::ParsingMultiaddress)?;

				Ok(addresses)
			})
			.collect::<Result<Vec<Vec<Multiaddr>>>>()?
			.into_iter().flatten().collect();

		if !remote_addresses.is_empty() {
			self.addr_cache.insert(authority_id.clone(), remote_addresses);
			self.update_peer_set_priority_group()?;
		}

		Ok(())
	}

	/// Retrieve our public keys within the current authority set.
	//
	// A node might have multiple authority discovery keys within its keystore, e.g. an old one and
	// one for the upcoming session. In addition it could be participating in the current authority
	// set with two keys. The function does not return all of the local authority discovery public
	// keys, but only the ones intersecting with the current authority set.
	fn get_own_public_keys_within_authority_set(
		key_store: &BareCryptoStorePtr,
		client: &Client,
	) -> Result<HashSet<AuthorityId>> {
		let local_pub_keys = key_store.read()
			.sr25519_public_keys(key_types::AUTHORITY_DISCOVERY)
			.into_iter()
			.collect::<HashSet<_>>();

		let id = BlockId::hash(client.info().best_hash);
		let current_authorities = client.runtime_api()
			.authorities(&id)
			.map_err(Error::CallingRuntime)?
			.into_iter()
			.map(std::convert::Into::into)
			.collect::<HashSet<_>>();

		let intersection = local_pub_keys.intersection(&current_authorities)
			.cloned()
			.map(std::convert::Into::into)
			.collect();

		Ok(intersection)
	}

	/// Update the peer set 'authority' priority group.
	fn update_peer_set_priority_group(&self) -> Result<()> {
		let addresses = self.addr_cache.get_subset();

		debug!(
			target: LOG_TARGET,
			"Applying priority group {:?} to peerset.", addresses,
		);
		self.network
			.set_priority_group(
				AUTHORITIES_PRIORITY_GROUP_NAME.to_string(),
				addresses.into_iter().collect(),
			)
			.map_err(Error::SettingPeersetPriorityGroup)?;

		Ok(())
	}
}

impl<Client, Network, Block> Future for AuthorityDiscovery<Client, Network, Block>
where
	Block: BlockT + Unpin + 'static,
	Network: NetworkProvider,
	Client: ProvideRuntimeApi<Block> + Send + Sync + 'static + HeaderBackend<Block>,
	<Client as ProvideRuntimeApi<Block>>::Api:
		AuthorityDiscoveryApi<Block, Error = sp_blockchain::Error>,
{
	type Output = ();

	fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
		// Process incoming events.
		if let Poll::Ready(()) = self.handle_dht_events(cx) {
			// `handle_dht_events` returns `Poll::Ready(())` when the Dht event stream terminated.
			// Termination of the Dht event stream implies that the underlying network terminated,
			// thus authority discovery should terminate as well.
			return Poll::Ready(());
		}


		// Publish own addresses.
		if let Poll::Ready(_) = self.publish_interval.poll_next_unpin(cx) {
			// Register waker of underlying task for next interval.
			while let Poll::Ready(_) = self.publish_interval.poll_next_unpin(cx) {}

			if let Err(e) = self.publish_ext_addresses() {
				error!(
					target: LOG_TARGET,
					"Failed to publish external addresses: {:?}", e,
				);
			}
		}

		// Request addresses of authorities.
		if let Poll::Ready(_) = self.query_interval.poll_next_unpin(cx) {
			// Register waker of underlying task for next interval.
			while let Poll::Ready(_) = self.query_interval.poll_next_unpin(cx) {}

			if let Err(e) = self.request_addresses_of_others() {
				error!(
					target: LOG_TARGET,
					"Failed to request addresses of authorities: {:?}", e,
				);
			}
		}

		Poll::Pending
	}
}

/// NetworkProvider provides AuthorityDiscovery with all necessary hooks into the underlying
/// Substrate networking. Using this trait abstraction instead of NetworkService directly is
/// necessary to unit test AuthorityDiscovery.
pub trait NetworkProvider: NetworkStateInfo {
	/// Modify a peerset priority group.
	fn set_priority_group(
		&self,
		group_id: String,
		peers: HashSet<libp2p::Multiaddr>,
	) -> std::result::Result<(), String>;

	/// Start putting a value in the Dht.
	fn put_value(&self, key: libp2p::kad::record::Key, value: Vec<u8>);

	/// Start getting a value from the Dht.
	fn get_value(&self, key: &libp2p::kad::record::Key);
}

impl<B, H> NetworkProvider for sc_network::NetworkService<B, H>
where
	B: BlockT + 'static,
	H: ExHashT,
{
	fn set_priority_group(
		&self,
		group_id: String,
		peers: HashSet<libp2p::Multiaddr>,
	) -> std::result::Result<(), String> {
		self.set_priority_group(group_id, peers)
	}
	fn put_value(&self, key: libp2p::kad::record::Key, value: Vec<u8>) {
		self.put_value(key, value)
	}
	fn get_value(&self, key: &libp2p::kad::record::Key) {
		self.get_value(key)
	}
}

fn hash_authority_id(id: &[u8]) -> libp2p::kad::record::Key {
	libp2p::kad::record::Key::new(&libp2p::multihash::Sha2_256::digest(id))
}

fn interval_at(start: Instant, duration: Duration) -> Interval {
	let stream = futures::stream::unfold(start, move |next| {
		let time_until_next =  next.saturating_duration_since(Instant::now());

		Delay::new(time_until_next).map(move |_| Some(((), next + duration)))
	});

	Box::new(stream)
}

/// Prometheus metrics for an `AuthorityDiscovery`.
#[derive(Clone)]
pub(crate) struct Metrics {
	publish: Counter<U64>,
	amount_last_published: Gauge<U64>,
	request: Counter<U64>,
	dht_event_received: CounterVec<U64>,
}

impl Metrics {
	pub(crate) fn register(registry: &prometheus_endpoint::Registry) -> Result<Self> {
		Ok(Self {
			publish: register(
				Counter::new(
					"authority_discovery_times_published_total",
					"Number of times authority discovery has published external addresses."
				)?,
				registry,
			)?,
			amount_last_published: register(
				Gauge::new(
					"authority_discovery_amount_external_addresses_last_published",
					"Number of external addresses published when authority discovery last \
					 published addresses."
				)?,
				registry,
			)?,
			request: register(
				Counter::new(
					"authority_discovery_authority_addresses_requested_total",
					"Number of times authority discovery has requested external addresses of a \
					 single authority."
				)?,
				registry,
			)?,
			dht_event_received: register(
				CounterVec::new(
					Opts::new(
						"authority_discovery_dht_event_received",
						"Number of dht events received by authority discovery."
					),
					&["name"],
				)?,
				registry,
			)?,
		})
	}
}
