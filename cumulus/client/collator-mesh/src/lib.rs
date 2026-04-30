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

//! Reserved-peer mesh for parachain collators on the canonical block-announce protocol.
//!
//! Each collator publishes its multiaddrs via the parachain DHT and resolves the rest into
//! a reserved set via [`NetworkService::set_reserved_peers`]. The same peer ids are also
//! registered with [`SyncingService::set_no_slot_peers`] so they don't consume `--in-peers`
//! budget — same semantics as `--reserved-nodes` but updateable at runtime.
//!
//! Requires [`sp_authority_discovery::AuthorityDiscoveryApi`] on the parachain runtime and
//! an `AUTHORITY_DISCOVERY` key in the collator's keystore.
//!
//! Spawns a parachain-side [`sc_authority_discovery`] worker and a refresh task driven by
//! block-import notifications and a periodic [`TRY_RERESOLVE_AUTHORITIES`] timer.
//!
//! When `collator_count <= max_reserved` every collator reserves every other (full mesh).
//! When over the limit, the set is sorted by raw pubkey bytes and truncated — deterministic
//! across all nodes.

use std::{
	collections::HashSet,
	marker::PhantomData,
	path::PathBuf,
	sync::Arc,
	time::{Duration, Instant},
};

use futures::{future::FutureExt, StreamExt};
use futures_timer::Delay;

use sc_authority_discovery::AuthorityDiscovery;
use sc_client_api::BlockchainEvents;
use sc_network::{
	service::traits::NetworkService,
	DhtEvent, Multiaddr, PeerId, ProtocolName,
};
use sc_service::SpawnTaskHandle;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_authority_discovery::{AuthorityDiscoveryApi, AuthorityId};
use sp_blockchain::HeaderBackend;
use sp_core::crypto::key_types;
use sp_keystore::KeystorePtr;
use sp_runtime::traits::Block as BlockT;

use sc_network_sync::SyncingService;

const LOG_TARGET: &str = "collator-mesh";

/// Re-resolve authority addresses even when the authority set is unchanged. Catches
/// peer-id rotations within a session; block-import notifications are the fast path.
const TRY_RERESOLVE_AUTHORITIES: Duration = Duration::from_secs(30);

/// Warn when resolved connectivity stays below this percentage for [`LOW_CONNECTIVITY_WARN_DELAY`].
const LOW_CONNECTIVITY_WARN_THRESHOLD_PCT: usize = 85;
const LOW_CONNECTIVITY_WARN_DELAY: Duration = Duration::from_secs(600);


/// Configuration knobs for the collator reserved-peer mesh.
pub struct CollatorMeshConfig {
	/// Maximum number of reserved peer slots. Typical: 32.
	pub max_reserved: usize,
	/// The block-announce protocol name to set reserved peers on.
	pub protocol: ProtocolName,
}

/// Parameters for [`start_collator_mesh`].
pub struct StartCollatorMeshParams<Block: BlockT, Client, AD, DhtStream> {
	/// Mesh configuration.
	pub config: CollatorMeshConfig,
	/// Parachain client — used for block import notifications and runtime API calls.
	pub client: Arc<Client>,
	/// Authority-discovery source. Typically the same `Arc<ParachainClient>` as `client`.
	pub authority_discovery: Arc<AD>,
	/// Parachain network handle. Reserved peers are set on this.
	pub network: Arc<dyn NetworkService>,
	/// Syncing engine handle.
	pub sync_service: Arc<SyncingService<Block>>,
	/// Pre-filtered DHT event stream (caller maps `Event::Dht`).
	pub dht_event_stream: DhtStream,
	/// Keystore with the local authority-discovery keys. Used to sign DHT records and to
	/// exclude this node from the peer set.
	pub keystore: KeystorePtr,
	/// Prometheus registry for mesh + worker metrics.
	pub prometheus_registry: Option<prometheus_endpoint::Registry>,
	/// Spawn handle for the worker and mesh refresh tasks.
	pub spawn_handle: SpawnTaskHandle,
	/// Allow publishing non-global IPs (local/testing only).
	pub publish_non_global_ips: bool,
	/// Public addresses advertised by the operator.
	pub public_addresses: Vec<Multiaddr>,
	/// Directory for persisting the worker's address cache.
	pub persisted_cache_directory: Option<PathBuf>,
	pub _marker: PhantomData<Block>,
}

/// Start the collator mesh. Returns immediately once the tasks are spawned.
pub fn start_collator_mesh<Block, Client, AD, DhtStream>(
	params: StartCollatorMeshParams<Block, Client, AD, DhtStream>,
) -> Result<(), prometheus_endpoint::PrometheusError>
where
	Block: BlockT + Unpin + 'static,
	Client: BlockchainEvents<Block>
		+ HeaderBackend<Block>
		+ ProvideRuntimeApi<Block>
		+ Send
		+ Sync
		+ 'static,
	Client::Api: ApiExt<Block>,
	AD: AuthorityDiscovery<Block> + Send + Sync + 'static,
	DhtStream: futures::Stream<Item = DhtEvent> + Send + Unpin + 'static,
{
	let StartCollatorMeshParams {
		config,
		client,
		authority_discovery,
		network,
		sync_service,
		dht_event_stream,
		keystore,
		prometheus_registry,
		spawn_handle,
		publish_non_global_ips,
		public_addresses,
		persisted_cache_directory,
		_marker,
	} = params;

	let metrics = prometheus_registry
		.as_ref()
		.map(Metrics::register)
		.transpose()?;

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
		"Starting collator mesh: max_reserved={}, protocol={}, reresolve_interval={:?}",
		config.max_reserved,
		config.protocol,
		TRY_RERESOLVE_AUTHORITIES,
	);

	let task = mesh_refresh_loop::<Block, Client, AD>(
		config,
		client,
		authority_discovery,
		network,
		sync_service,
		authority_discovery_service,
		keystore,
		metrics,
	);
	spawn_handle.spawn("collator-mesh", Some("collator-mesh"), task);

	Ok(())
}

/// Refreshes the reserved/no-slot peer sets on each new best block and every
/// [`TRY_RERESOLVE_AUTHORITIES`] tick.
async fn mesh_refresh_loop<Block, Client, AD>(
	config: CollatorMeshConfig,
	client: Arc<Client>,
	authority_discovery: Arc<AD>,
	network: Arc<dyn NetworkService>,
	sync_service: Arc<SyncingService<Block>>,
	mut authority_discovery_service: sc_authority_discovery::Service,
	keystore: KeystorePtr,
	metrics: Option<Metrics>,
) where
	Block: BlockT,
	Client: BlockchainEvents<Block>
		+ HeaderBackend<Block>
		+ ProvideRuntimeApi<Block>
		+ Send
		+ Sync
		+ 'static,
	Client::Api: ApiExt<Block>,
	AD: AuthorityDiscovery<Block> + Send + Sync + 'static,
{
	let CollatorMeshConfig { max_reserved, protocol } = config;

	let local_pub_keys: HashSet<AuthorityId> = keystore
		.sr25519_public_keys(key_types::AUTHORITY_DISCOVERY)
		.into_iter()
		.map(AuthorityId::from)
		.collect();

	let mut state = LoopState::new();

	// Initial refresh at startup.
	let at = client.info().best_hash;
	log::debug!(
		target: LOG_TARGET,
		"Performing initial collator mesh refresh at {:?}",
		at,
	);
	apply_once(
		&*client,
		&*authority_discovery,
		&*network,
		&sync_service,
		&mut authority_discovery_service,
		&local_pub_keys,
		max_reserved,
		&protocol,
		&mut state,
		metrics.as_ref(),
		at,
	)
	.await;

	let mut import_stream = client.import_notification_stream().fuse();
	let mut reresolve_timer = Delay::new(TRY_RERESOLVE_AUTHORITIES).fuse();

	loop {
		futures::select! {
			notification = import_stream.next() => {
				let Some(notification) = notification else {
					log::debug!(
						target: LOG_TARGET,
						"Block import stream ended; stopping collator mesh loop",
					);
					break;
				};
				if !notification.is_new_best {
					continue;
				}
				log::trace!(
					target: LOG_TARGET,
					"New best block at {:?} — evaluating collator mesh",
					notification.hash,
				);
				apply_once(
					&*client,
					&*authority_discovery,
					&*network,
					&sync_service,
					&mut authority_discovery_service,
					&local_pub_keys,
					max_reserved,
					&protocol,
					&mut state,
					metrics.as_ref(),
					notification.hash,
				).await;
			}
			_ = reresolve_timer => {
				reresolve_timer = Delay::new(TRY_RERESOLVE_AUTHORITIES).fuse();
				// Force a re-resolve to catch peer-id rotations even when the authority set
				// hasn't changed.
				state.force_resolve = true;
				let at = client.info().best_hash;
				log::debug!(
					target: LOG_TARGET,
					"Periodic timer fired — re-resolving authority addresses at {:?}",
					at,
				);
				apply_once(
					&*client,
					&*authority_discovery,
					&*network,
					&sync_service,
					&mut authority_discovery_service,
					&local_pub_keys,
					max_reserved,
					&protocol,
					&mut state,
					metrics.as_ref(),
					at,
				).await;
			}
		}
	}
}

/// Loop-local state: last applied snapshot + low-connectivity bookkeeping.
struct LoopState {
	last_authorities: Option<Vec<AuthorityId>>,
	last_addrs: Option<HashSet<Multiaddr>>,
	/// Force re-resolve on the next `apply_once` even if the authority set hasn't changed.
	/// Cleared after one pass.
	force_resolve: bool,
	/// When we first dropped below the connectivity warning threshold; `None` if above it.
	low_connectivity_since: Option<Instant>,
	/// Whether `AuthorityDiscoveryApi` was available on the last checked block. `None`
	/// before the first check; used to log transitions once per flip.
	ad_enabled: Option<bool>,
	/// Unresolved-authority count from the previous pass.
	last_unresolved: usize,
}

impl LoopState {
	fn new() -> Self {
		Self {
			last_authorities: None,
			last_addrs: None,
			force_resolve: false,
			low_connectivity_since: None,
			ad_enabled: None,
			last_unresolved: 0,
		}
	}
}

/// Filter `local_pub_keys` out of `authorities`, sort by raw pubkey bytes, truncate to
/// `max_reserved`.
fn select_authorities(
	authorities: Vec<AuthorityId>,
	local_pub_keys: &HashSet<AuthorityId>,
	max_reserved: usize,
) -> Vec<AuthorityId> {
	let mut selected: Vec<AuthorityId> =
		authorities.into_iter().filter(|id| !local_pub_keys.contains(id)).collect();
	selected.sort_by(|a, b| {
		let a: &[u8] = a.as_ref();
		let b: &[u8] = b.as_ref();
		a.cmp(b)
	});
	selected.truncate(max_reserved);
	selected
}

/// Single refresh pass: fetch authorities at `at`, resolve multiaddrs, and update the
/// reserved/no-slot peer sets if anything changed.
async fn apply_once<Block, Client, AD>(
	client: &Client,
	authority_discovery: &AD,
	network: &dyn NetworkService,
	sync_service: &SyncingService<Block>,
	authority_discovery_service: &mut sc_authority_discovery::Service,
	local_pub_keys: &HashSet<AuthorityId>,
	max_reserved: usize,
	protocol: &ProtocolName,
	state: &mut LoopState,
	metrics: Option<&Metrics>,
	at: Block::Hash,
) where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block>,
	Client::Api: ApiExt<Block>,
	AD: AuthorityDiscovery<Block> + Send + Sync + 'static,
{
	// Re-check API availability every pass so the task survives runtime upgrades that
	// add or remove `pallet-authority-discovery`. Log only on transitions.
	let ad_enabled = client
		.runtime_api()
		.has_api::<dyn AuthorityDiscoveryApi<Block>>(at)
		.unwrap_or(false);
	match (state.ad_enabled, ad_enabled) {
		(None, false) => log::warn!(
			target: LOG_TARGET,
			"Parachain runtime does not implement `AuthorityDiscoveryApi`; the collator mesh \
			 will stay idle until a runtime upgrade adds it.",
		),
		(Some(true), false) => log::warn!(
			target: LOG_TARGET,
			"Parachain runtime no longer implements `AuthorityDiscoveryApi` at {:?}; pausing \
			 the collator mesh until a future upgrade restores it.",
			at,
		),
		(Some(false), true) => log::info!(
			target: LOG_TARGET,
			"Parachain runtime started implementing `AuthorityDiscoveryApi` at {:?}; the \
			 collator mesh is now active.",
			at,
		),
		_ => {},
	}
	state.ad_enabled = Some(ad_enabled);
	if !ad_enabled {
		return;
	}

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

	// Skip the resolve pass only when the authority set is unchanged, no forced re-resolve,
	// and the previous pass had no unresolved authorities. The third condition prevents an
	// incomplete first refresh from stalling for a full timer period — import notifications
	// retry until the DHT cache is populated.
	let authorities_unchanged = state.last_authorities.as_ref() == Some(&selected);
	if authorities_unchanged && !state.force_resolve && state.last_unresolved == 0 {
		return;
	}

	let target_count = selected.len();
	let mut addrs: HashSet<Multiaddr> = HashSet::new();
	let mut unresolved = 0usize;
	for id in &selected {
		match authority_discovery_service.get_addresses_by_authority_id(id.clone()).await {
			Some(a) => {
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

	state.force_resolve = false;

	// Skip pushing when both the authority set and resolved multiaddrs are unchanged.
	if authorities_unchanged && state.last_addrs.as_ref() == Some(&addrs) {
		log::trace!(
			target: LOG_TARGET,
			"No-op refresh at {:?}: authorities and resolved addresses unchanged",
			at,
		);
		return;
	}

	let trigger = if !authorities_unchanged { "authority set changed" } else { "timer re-resolve" };
	let previous_authority_count = state.last_authorities.as_ref().map(|a| a.len()).unwrap_or(0);
	let previous_addr_count = state.last_addrs.as_ref().map(|a| a.len()).unwrap_or(0);

	let peer_ids: HashSet<PeerId> = addrs.iter().filter_map(PeerId::try_from_multiaddr).collect();

	log::debug!(
		target: LOG_TARGET,
		"Refreshing reserved peers at {:?} ({}): authorities {}->{} unresolved={} multiaddrs {}->{}, peer_ids={}",
		at,
		trigger,
		previous_authority_count,
		target_count,
		unresolved,
		previous_addr_count,
		addrs.len(),
		peer_ids.len(),
	);

	sync_service.set_no_slot_peers(peer_ids.clone());

	match network.set_reserved_peers(protocol.clone(), addrs.clone()) {
		Ok(()) => {
			state.last_authorities = Some(selected);
			state.last_addrs = Some(addrs);
			state.last_unresolved = unresolved;
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

fn log_low_connectivity_if_stuck(
	target: usize,
	resolved: usize,
	since: &mut Option<Instant>,
) {
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
				"Collator mesh connectivity has been under {}% for more than {:?} \
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

/// Prometheus metrics for the mesh task.
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
					"collator_mesh_target_authorities",
					"Number of parachain authorities currently targeted for reservation.",
				))?,
				registry,
			)?,
			unresolved_authorities: register(
				Gauge::with_opts(Opts::new(
					"collator_mesh_unresolved_authorities",
					"Number of targeted authorities we couldn't resolve a multiaddr for.",
				))?,
				registry,
			)?,
			resolved_peers: register(
				Gauge::with_opts(Opts::new(
					"collator_mesh_resolved_peers",
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
		let inputs = vec![authority_id(9), authority_id(3), authority_id(6), authority_id(1), authority_id(4)];
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
