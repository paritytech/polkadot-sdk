// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! Parachain collator authority discovery — builds a reserved-peer mesh on the block-announce
//! protocol.
//!
//! Requires [`sp_authority_discovery::AuthorityDiscoveryApi`] on the parachain runtime
//! and an `AUTHORITY_DISCOVERY` key in the collator's keystore. API detection is
//! monotonic — once observed, assumed to stay. When over `max_reserved`, the authority
//! set is sorted by raw pubkey bytes and truncated, so every node converges to the same
//! subset without coordination.

use std::{
	collections::HashSet,
	path::PathBuf,
	sync::Arc,
	time::{Duration, Instant},
};

use futures_timer::Delay;

use sc_authority_discovery::AuthorityDiscovery;
use sc_network::{service::traits::NetworkService, DhtEvent, Multiaddr, PeerId, ProtocolName};
use sc_service::SpawnTaskHandle;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_authority_discovery::{AuthorityDiscoveryApi, AuthorityId};
use sp_blockchain::HeaderBackend;
use sp_core::crypto::key_types;
use sp_keystore::KeystorePtr;
use sp_runtime::traits::Block as BlockT;

use sc_network_sync::SyncingService;

const LOG_TARGET: &str = "collator-discovery";

/// Re-resolve authority addresses periodically.
const TRY_RERESOLVE_AUTHORITIES: Duration = Duration::from_secs(30);

/// Maximum number of multiaddrs accepted per authority. Bounds dial-attempt amplification
/// from a single authority publishing many multiaddrs.
const MAX_ADDRS_PER_AUTHORITY: usize = 4;

/// Warn when resolved connectivity stays below this percentage for [`LOW_CONNECTIVITY_WARN_DELAY`].
const LOW_CONNECTIVITY_WARN_THRESHOLD_PCT: usize = 85;
const LOW_CONNECTIVITY_WARN_DELAY: Duration = Duration::from_secs(600);

pub struct CollatorDiscoveryConfig {
	pub max_reserved: usize,
	pub protocol: ProtocolName,
}

/// Parameters for [`maybe_start_collator_discovery`].
pub struct StartCollatorDiscoveryParams<Block: BlockT, Client, AD, NetEventStream> {
	/// True if this node is a collator. Only collators must run AD.
	pub is_validator: bool,
	/// `0` disables collator discovery.
	pub max_reserved: usize,
	pub client: Arc<Client>,
	/// Usually the same `Arc` as `client`.
	pub authority_discovery: Arc<AD>,
	pub network: Arc<dyn NetworkService>,
	pub sync_service: Arc<SyncingService<Block>>,
	/// Raw network event stream; the worker filters for `Event::Dht`.
	pub network_event_stream: NetEventStream,
	/// Keystore with the local AD keys; used to sign DHT records and exclude this node
	/// from the reserved peer set.
	pub keystore: KeystorePtr,
	pub genesis_hash: Block::Hash,
	pub fork_id: Option<String>,
	/// Local/testing only.
	pub publish_non_global_ips: bool,
	pub public_addresses: Vec<Multiaddr>,
	pub persisted_cache_directory: Option<PathBuf>,
	pub prometheus_registry: Option<prometheus_endpoint::Registry>,
	pub spawn_handle: SpawnTaskHandle,
}

/// Start parachain collator discovery; no-op unless `is_validator` and `max_reserved > 0`.
pub fn maybe_start_collator_discovery<Block, Client, AD, NetEventStream>(
	params: StartCollatorDiscoveryParams<Block, Client, AD, NetEventStream>,
) -> Result<(), prometheus_endpoint::PrometheusError>
where
	Block: BlockT + Unpin + 'static,
	Client: HeaderBackend<Block> + ProvideRuntimeApi<Block> + Send + Sync + 'static,
	Client::Api: ApiExt<Block>,
	AD: AuthorityDiscovery<Block> + Send + Sync + 'static,
	NetEventStream: futures::Stream<Item = sc_network::Event> + Send + Unpin + 'static,
{
	let StartCollatorDiscoveryParams {
		is_validator,
		max_reserved,
		client,
		authority_discovery,
		network,
		sync_service,
		network_event_stream,
		keystore,
		genesis_hash,
		fork_id,
		publish_non_global_ips,
		public_addresses,
		persisted_cache_directory,
		prometheus_registry,
		spawn_handle,
	} = params;

	if !is_validator || max_reserved == 0 {
		return Ok(());
	}

	let genesis_hex = array_bytes::bytes2hex("", genesis_hash.as_ref());
	let protocol: ProtocolName = match fork_id.as_deref() {
		Some(f) => format!("/{}/{}/block-announces/1", genesis_hex, f).into(),
		None => format!("/{}/block-announces/1", genesis_hex).into(),
	};

	use futures::StreamExt;
	let dht_event_stream = network_event_stream.filter_map(|e| async move {
		match e {
			sc_network::Event::Dht(e) => Some(e),
			_ => None,
		}
	});

	start_collator_discovery::<Block, _, _, _>(
		CollatorDiscoveryConfig { max_reserved, protocol },
		client,
		authority_discovery,
		network,
		sync_service,
		Box::pin(dht_event_stream),
		keystore,
		publish_non_global_ips,
		public_addresses,
		persisted_cache_directory,
		prometheus_registry,
		spawn_handle,
	)
}

/// Spawn the authority-discovery worker and refresh task; returns immediately.
fn start_collator_discovery<Block, Client, AD, DhtStream>(
	config: CollatorDiscoveryConfig,
	client: Arc<Client>,
	authority_discovery: Arc<AD>,
	network: Arc<dyn NetworkService>,
	sync_service: Arc<SyncingService<Block>>,
	dht_event_stream: DhtStream,
	keystore: KeystorePtr,
	publish_non_global_ips: bool,
	public_addresses: Vec<Multiaddr>,
	persisted_cache_directory: Option<PathBuf>,
	prometheus_registry: Option<prometheus_endpoint::Registry>,
	spawn_handle: SpawnTaskHandle,
) -> Result<(), prometheus_endpoint::PrometheusError>
where
	Block: BlockT + Unpin + 'static,
	Client: HeaderBackend<Block> + ProvideRuntimeApi<Block> + Send + Sync + 'static,
	Client::Api: ApiExt<Block>,
	AD: AuthorityDiscovery<Block> + Send + Sync + 'static,
	DhtStream: futures::Stream<Item = DhtEvent> + Send + Unpin + 'static,
{
	let metrics = prometheus_registry.as_ref().map(Metrics::register).transpose()?;

	let (worker, authority_discovery_service) =
		sc_authority_discovery::new_worker_and_service_with_config(
			sc_authority_discovery::WorkerConfig {
				publish_non_global_ips,
				public_addresses,
				strict_record_validation: true,
				persisted_cache_directory,
				..Default::default()
			},
			authority_discovery.clone(),
			Arc::new(network.clone()),
			dht_event_stream,
			sc_authority_discovery::Role::PublishAndDiscover(keystore.clone()),
			prometheus_registry,
			spawn_handle.clone(),
		);

	spawn_handle.spawn(
		"para-authority-discovery-worker",
		Some("authority-discovery"),
		worker.run(),
	);

	log::info!(
		target: LOG_TARGET,
		"Starting collator discovery: max_reserved={}, protocol={}, reresolve_interval={:?}",
		config.max_reserved,
		config.protocol,
		TRY_RERESOLVE_AUTHORITIES,
	);

	spawn_handle.spawn(
		"collator-discovery",
		Some("collator-discovery"),
		discovery_refresh_loop::<Block, Client, AD>(
			config,
			client,
			authority_discovery,
			network,
			sync_service,
			authority_discovery_service,
			keystore,
			metrics,
		),
	);

	Ok(())
}

/// Refresh the reserved/no-slot peer sets every [`TRY_RERESOLVE_AUTHORITIES`].
async fn discovery_refresh_loop<Block, Client, AD>(
	config: CollatorDiscoveryConfig,
	client: Arc<Client>,
	authority_discovery: Arc<AD>,
	network: Arc<dyn NetworkService>,
	sync_service: Arc<SyncingService<Block>>,
	mut authority_discovery_service: sc_authority_discovery::Service,
	keystore: KeystorePtr,
	metrics: Option<Metrics>,
) where
	Block: BlockT,
	Client: HeaderBackend<Block> + ProvideRuntimeApi<Block> + Send + Sync + 'static,
	Client::Api: ApiExt<Block>,
	AD: AuthorityDiscovery<Block> + Send + Sync + 'static,
{
	let CollatorDiscoveryConfig { max_reserved, protocol } = config;

	let local_peer_id = network.local_peer_id();
	let mut state = LoopState::new();
	let mut ad_enabled = false;

	loop {
		// Read the authority set from the latest *finalized* parachain block so all
		// collators converge on the same subset across short-lived forks.
		let at = client.info().finalized_hash;
		if !ad_enabled {
			ad_enabled = client
				.runtime_api()
				.has_api::<dyn AuthorityDiscoveryApi<Block>>(at)
				.unwrap_or(false);
		}
		if ad_enabled {
			// This is cheap, it's all in memory.
			let local_pub_keys: HashSet<AuthorityId> = keystore
				.sr25519_public_keys(key_types::AUTHORITY_DISCOVERY)
				.into_iter()
				.map(AuthorityId::from)
				.collect();
			update_parachain_authorities(
				&*authority_discovery,
				&*network,
				&sync_service,
				&mut authority_discovery_service,
				&local_pub_keys,
				local_peer_id,
				max_reserved,
				&protocol,
				&mut state,
				metrics.as_ref(),
				at,
			)
			.await;
		}
		Delay::new(TRY_RERESOLVE_AUTHORITIES).await;
	}
}

/// Loop-local state: last applied snapshot + low-connectivity bookkeeping.
struct LoopState {
	last_authorities: Option<Vec<AuthorityId>>,
	last_addrs: Option<HashSet<Multiaddr>>,
	/// When we first dropped below the connectivity warning threshold; `None` if above it.
	low_connectivity_since: Option<Instant>,
}

impl LoopState {
	fn new() -> Self {
		Self { last_authorities: None, last_addrs: None, low_connectivity_since: None }
	}
}

/// Take the `max_reserved` smallest pubkeys by raw bytes, sorted.
fn select_authorities(
	authorities: Vec<AuthorityId>,
	local_pub_keys: &HashSet<AuthorityId>,
	max_reserved: usize,
) -> Vec<AuthorityId> {
	let mut selected: Vec<AuthorityId> =
		authorities.into_iter().filter(|id| !local_pub_keys.contains(id)).collect();

	let cmp_bytes = |a: &AuthorityId, b: &AuthorityId| {
		let a: &[u8] = a.as_ref();
		let b: &[u8] = b.as_ref();
		a.cmp(b)
	};
	if selected.len() > max_reserved {
		selected.select_nth_unstable_by(max_reserved, cmp_bytes);
		selected.truncate(max_reserved);
	}
	selected.sort_unstable_by(cmp_bytes);
	selected
}

/// Resolve authority multiaddrs and push updated reserved/no-slot peer sets if anything
/// changed since the last call.
async fn update_parachain_authorities<Block, AD>(
	authority_discovery: &AD,
	network: &dyn NetworkService,
	sync_service: &SyncingService<Block>,
	authority_discovery_service: &mut sc_authority_discovery::Service,
	local_pub_keys: &HashSet<AuthorityId>,
	local_peer_id: PeerId,
	max_reserved: usize,
	protocol: &ProtocolName,
	state: &mut LoopState,
	metrics: Option<&Metrics>,
	at: Block::Hash,
) where
	Block: BlockT,
	AD: AuthorityDiscovery<Block> + Send + Sync + 'static,
{
	let authorities = match authority_discovery.authorities(at).await {
		Ok(a) => a,
		Err(e) => {
			log::warn!(
				target: LOG_TARGET,
				"Failed to fetch parachain authorities at {:?}: {}",
				at,
				e,
			);
			return;
		},
	};

	let selected = select_authorities(authorities, local_pub_keys, max_reserved);

	let target_count = selected.len();
	let mut addrs: HashSet<Multiaddr> = HashSet::new();
	let mut unresolved = 0usize;
	for id in &selected {
		match authority_discovery_service.get_addresses_by_authority_id(id.clone()).await {
			Some(a) => {
				let original_len = a.len();
				// Drop multiaddrs that resolve to our own libp2p PeerId. `set_reserved_peers`
				// rejects the entire call if the input contains the local PeerId, which can
				// happen if our own AD record (or a stale one keyed under another authority)
				// reaches our DHT view.
				let a: Vec<Multiaddr> = a
					.into_iter()
					.filter(|m| {
						PeerId::try_from_multiaddr(m).map_or(true, |pid| pid != local_peer_id)
					})
					.take(MAX_ADDRS_PER_AUTHORITY)
					.collect();
				if original_len > MAX_ADDRS_PER_AUTHORITY {
					log::debug!(
						target: LOG_TARGET,
						"Capped multiaddrs for authority {:?}: {} -> {}",
						id,
						original_len,
						MAX_ADDRS_PER_AUTHORITY,
					);
				}
				let peer_ids: Vec<PeerId> =
					a.iter().filter_map(PeerId::try_from_multiaddr).collect();
				log::debug!(
					target: LOG_TARGET,
					"Resolved authority {:?}: {} multiaddr(s), peer_ids={:?}",
					id,
					a.len(),
					peer_ids,
				);
				addrs.extend(a);
			},
			None => {
				unresolved += 1;
				log::debug!(
					target: LOG_TARGET,
					"Couldn't resolve addresses of authority: {:?}",
					id,
				);
			},
		}
	}

	let resolved = target_count.saturating_sub(unresolved);
	if let Some(m) = metrics {
		m.target_authorities.set(target_count as u64);
		m.unresolved_authorities.set(unresolved as u64);
		m.resolved_peers.set(addrs.len() as u64);
	}

	log_low_connectivity_if_stuck(target_count, resolved, &mut state.low_connectivity_since);

	// Skip pushing when both the authority set and resolved multiaddrs are unchanged.
	let authorities_unchanged = state.last_authorities.as_ref() == Some(&selected);
	if authorities_unchanged && state.last_addrs.as_ref() == Some(&addrs) {
		log::trace!(
			target: LOG_TARGET,
			"No-op refresh at {:?}: authorities and resolved addresses unchanged",
			at,
		);
		return;
	}

	let previous_authority_count = state.last_authorities.as_ref().map(|a| a.len()).unwrap_or(0);
	let previous_addr_count = state.last_addrs.as_ref().map(|a| a.len()).unwrap_or(0);

	let peer_ids: HashSet<PeerId> = addrs.iter().filter_map(PeerId::try_from_multiaddr).collect();

	log::debug!(
		target: LOG_TARGET,
		"Refreshing reserved peers at {:?}: authorities {}->{} unresolved={} multiaddrs {}->{}, peer_ids={}",
		at,
		previous_authority_count,
		target_count,
		unresolved,
		previous_addr_count,
		addrs.len(),
		peer_ids.len(),
	);

	match network.set_reserved_peers(protocol.clone(), addrs.clone()) {
		Ok(()) => {
			// Only push the no-slot set after the reserved set is accepted so the two
			// stay in sync. On Err, neither side advances and we retry on the next tick.
			sync_service.set_no_slot_peers(peer_ids);
			state.last_authorities = Some(selected);
			state.last_addrs = Some(addrs);
		},
		Err(e) => {
			log::warn!(
				target: LOG_TARGET,
				"set_reserved_peers failed at {:?}: {}; will retry on next trigger",
				at,
				e,
			);
		},
	}
}

fn log_low_connectivity_if_stuck(target: usize, resolved: usize, since: &mut Option<Instant>) {
	if target == 0 {
		*since = None;
		return;
	}
	let pct = (resolved * 100) / target;
	if pct >= LOW_CONNECTIVITY_WARN_THRESHOLD_PCT {
		*since = None;
		return;
	}
	match *since {
		Some(t) if t.elapsed() >= LOW_CONNECTIVITY_WARN_DELAY => {
			log::warn!(
				target: LOG_TARGET,
				"Collator discovery: peer connectivity has been under {}% for more than {:?} \
				 ({resolved}/{target} authorities resolved). Check authority-discovery \
				 DHT reachability.",
				LOW_CONNECTIVITY_WARN_THRESHOLD_PCT, LOW_CONNECTIVITY_WARN_DELAY,
			);
			// Reset so we warn at most once per elapsed window.
			*since = Some(Instant::now());
		},
		Some(_) => {},
		None => *since = Some(Instant::now()),
	}
}

/// Prometheus metrics for the collator-discovery task.
#[derive(Clone)]
struct Metrics {
	target_authorities: prometheus_endpoint::Gauge<prometheus_endpoint::U64>,
	unresolved_authorities: prometheus_endpoint::Gauge<prometheus_endpoint::U64>,
	resolved_peers: prometheus_endpoint::Gauge<prometheus_endpoint::U64>,
}

impl Metrics {
	fn register(
		registry: &prometheus_endpoint::Registry,
	) -> Result<Self, prometheus_endpoint::PrometheusError> {
		use prometheus_endpoint::{register, Gauge, Opts};
		Ok(Self {
			target_authorities: register(
				Gauge::with_opts(Opts::new(
					"collator_discovery_target_authorities",
					"Number of parachain authorities currently targeted for reservation.",
				))?,
				registry,
			)?,
			unresolved_authorities: register(
				Gauge::with_opts(Opts::new(
					"collator_discovery_unresolved_authorities",
					"Number of targeted authorities we couldn't resolve a multiaddr for.",
				))?,
				registry,
			)?,
			resolved_peers: register(
				Gauge::with_opts(Opts::new(
					"collator_discovery_resolved_peers",
					"Number of multiaddrs pushed to the collator-sync reserved set.",
				))?,
				registry,
			)?,
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::sr25519;

	fn authority_id(seed_byte: u8) -> AuthorityId {
		AuthorityId::from(sr25519::Public::from_raw([seed_byte; 32]))
	}

	#[test]
	fn select_authorities_sorts_deterministically_and_truncates() {
		let inputs = vec![authority_id(3), authority_id(1), authority_id(2)];
		let selected = select_authorities(inputs, &HashSet::new(), 2);
		assert_eq!(selected, vec![authority_id(1), authority_id(2)]);
	}

	#[test]
	fn select_authorities_filters_self() {
		let me = authority_id(2);
		let mut local = HashSet::new();
		local.insert(me.clone());
		let selected = select_authorities(vec![authority_id(1), me, authority_id(3)], &local, 10);
		assert_eq!(selected, vec![authority_id(1), authority_id(3)]);
	}

	#[test]
	fn select_authorities_is_idempotent_on_equal_input() {
		let inputs = vec![authority_id(5), authority_id(1), authority_id(7), authority_id(1)]; // dup is permitted by API
		let first = select_authorities(inputs.clone(), &HashSet::new(), 10);
		let second = select_authorities(inputs, &HashSet::new(), 10);
		assert_eq!(first, second);
	}

	#[test]
	fn select_authorities_truncate_preserves_sort_order() {
		let inputs = vec![
			authority_id(9),
			authority_id(3),
			authority_id(6),
			authority_id(1),
			authority_id(4),
		];
		let selected = select_authorities(inputs, &HashSet::new(), 3);
		assert_eq!(selected, vec![authority_id(1), authority_id(3), authority_id(4)]);
	}

	#[test]
	fn low_connectivity_starts_quiet() {
		let mut since = None;
		// resolved == target, pct = 100 — no warning state.
		log_low_connectivity_if_stuck(4, 4, &mut since);
		assert!(since.is_none());
	}

	#[test]
	fn low_connectivity_remembers_drop_below_threshold() {
		let mut since = None;
		// 1/4 = 25% < 85%, below threshold
		log_low_connectivity_if_stuck(4, 1, &mut since);
		assert!(since.is_some());
	}

	#[test]
	fn low_connectivity_clears_on_recovery() {
		let mut since = Some(Instant::now());
		log_low_connectivity_if_stuck(4, 4, &mut since);
		assert!(since.is_none());
	}

	#[test]
	fn low_connectivity_noop_on_zero_target() {
		let mut since = Some(Instant::now());
		log_low_connectivity_if_stuck(0, 0, &mut since);
		assert!(since.is_none());
	}
}
