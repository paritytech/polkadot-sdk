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

//! Collator authority discovery for parachains is used to achieve `1 hop` block announcement
//! between collators.
//!
//! This requires the parachain runtime to implement
//! [`sp_authority_discovery::AuthorityDiscoveryApi`], and that the collator's keystore contains an
//! `AUTHORITY_DISCOVERY` key.
//!
//! Once the API is detected, it's assumed to remain available. The authority set is expected in
//! authoring (session/validator-index) order — adopting parachain runtimes should implement
//! [`sp_authority_discovery::AuthorityDiscoveryApi::authorities`] as
//! `pallet_authority_discovery::Pallet::current_authorities().to_vec()` rather than the
//! standard `Pallet::authorities()`, which sorts by pubkey and merges in next-session keys.
//! Each node reserves up to `max_reserved` of its nearest ring neighbors on both sides. The
//! immediate neighbors get a direct 1-hop block-announcement path, and more distant authors
//! receive blocks with enough lead time before they author.

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

use sc_network_sync::{block_announces_protocol_name, SyncingService};

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

	let protocol: ProtocolName =
		block_announces_protocol_name(genesis_hash, fork_id.as_deref()).into();

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

	// We could have spawned this task as essential, but we don't because the collators
	// should be able to continue to build blocks without running this task.
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
		// collators converge on the same ring across short-lived forks.
		let at = client.info().finalized_hash;
		if !ad_enabled {
			ad_enabled = client
				.runtime_api()
				.has_api::<dyn AuthorityDiscoveryApi<Block>>(at)
				.unwrap_or(false);
		}
		if ad_enabled {
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

/// Select up to `max_reserved` AURA neighbors of the node.
///
/// `authorities` is taken in authoring order, the order Aura uses to assign slots. We locate
/// the local authority on this ring and take its nearest neighbors on both sides, wrapping
/// around. Neighbors are added symmetrically (distance `d` on each side at once) so that
/// the relationship is mutual — if we reserve the peer `d` positions away, that peer reserves us
/// back. An odd `max_reserved` therefore leaves one slot unused. Once `max_reserved` reaches the
/// ring size the result is the full mesh.
///
/// The immediate neighbors (`N-1`, `N+1`) get a direct, 1-hop block-announcement path; more
/// distant authors receive blocks via the ring with enough lead time before they author.
fn select_ring_neighbors(
	authorities: Vec<AuthorityId>,
	local_pub_keys: &HashSet<AuthorityId>,
	max_reserved: usize,
) -> Vec<AuthorityId> {
	let n = authorities.len();
	if n == 0 || max_reserved == 0 {
		return Vec::new();
	}

	let Some(local_idx) = authorities.iter().position(|id| local_pub_keys.contains(id)) else {
		return Vec::new();
	};

	let mut selected = Vec::with_capacity(max_reserved.min(n.saturating_sub(1)));
	for d in 1..=(n / 2) {
		let right = (local_idx + d) % n;
		let left = (local_idx + n - d) % n;

		// At the antipode (even `n`, `d == n / 2`) both sides point at the same node.
		let mut shell: Vec<usize> = if right == left { vec![right] } else { vec![right, left] };
		// Never reserve our own keys (relevant only if this node holds several).
		shell.retain(|&idx| !local_pub_keys.contains(&authorities[idx]));
		if shell.is_empty() {
			continue;
		}
		// Only take a shell if it fits whole, so we never keep one side and drop its mirror.
		if selected.len() + shell.len() > max_reserved {
			break;
		}
		selected.extend(shell.into_iter().map(|idx| authorities[idx].clone()));
	}

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
			log::warn!(target: LOG_TARGET, "Failed to fetch parachain authorities at {at:?}: {e}");
			return;
		},
	};

	let selected = select_ring_neighbors(authorities, local_pub_keys, max_reserved);
	let target_count = selected.len();
	let (addrs, unresolved) =
		resolve_authority_addresses(&selected, authority_discovery_service, local_peer_id).await;
	let resolved = target_count.saturating_sub(unresolved);

	if let Some(m) = metrics {
		m.target_authorities.set(target_count as u64);
		m.unresolved_authorities.set(unresolved as u64);
		m.resolved_peers.set(addrs.len() as u64);
	}

	log_low_connectivity_if_stuck(target_count, resolved, &mut state.low_connectivity_since);

	// Skip pushing when both the authority set and resolved multiaddrs are unchanged.
	if state.last_authorities.as_ref() == Some(&selected) &&
		state.last_addrs.as_ref() == Some(&addrs)
	{
		log::trace!(target: LOG_TARGET, "No-op refresh at {at:?}");
		return;
	}

	let peer_ids: HashSet<PeerId> = addrs.iter().filter_map(PeerId::try_from_multiaddr).collect();
	log::debug!(
		target: LOG_TARGET,
		"Refreshing reserved peers at {at:?}: target={target_count} unresolved={unresolved} multiaddrs={} peer_ids={}",
		addrs.len(),
		peer_ids.len(),
	);

	match network.set_reserved_peers(protocol.clone(), addrs.clone()) {
		Ok(()) => {
			// Update the no-slot set only after the reserved set is updated,
			// so the two stay in sync.
			// If an error happens we retry the whole refresh on the next tick.
			sync_service.set_no_slot_peers(peer_ids);
			state.last_authorities = Some(selected);
			state.last_addrs = Some(addrs);
		},
		Err(e) => {
			log::warn!(
				target: LOG_TARGET,
				"set_reserved_peers failed at {at:?}: {e}; will retry on next tick",
			);
		},
	}
}

/// Resolve multiaddrs for each authority. Returns the deduplicated set of multiaddrs and the
/// count of authorities whose addresses could not be resolved.
///
/// Drops multiaddrs that resolve to our own libp2p PeerId.
async fn resolve_authority_addresses(
	authorities: &[AuthorityId],
	service: &mut sc_authority_discovery::Service,
	local_peer_id: PeerId,
) -> (HashSet<Multiaddr>, usize) {
	let mut addrs: HashSet<Multiaddr> = HashSet::new();
	let mut unresolved = 0;

	for id in authorities {
		let Some(raw) = service.get_addresses_by_authority_id(id.clone()).await else {
			unresolved += 1;
			log::debug!(target: LOG_TARGET, "Couldn't resolve addresses of authority: {id:?}");
			continue;
		};

		let original_len = raw.len();
		let filtered: Vec<Multiaddr> = raw
			.into_iter()
			.filter(|m| PeerId::try_from_multiaddr(m).map_or(true, |pid| pid != local_peer_id))
			.take(MAX_ADDRS_PER_AUTHORITY)
			.collect();

		if original_len > MAX_ADDRS_PER_AUTHORITY {
			log::debug!(
				target: LOG_TARGET,
				"Capped multiaddrs for authority {id:?}: {original_len} -> {MAX_ADDRS_PER_AUTHORITY}",
			);
		}
		log::debug!(target: LOG_TARGET, "Resolved authority {id:?}: {} multiaddr(s)", filtered.len());
		addrs.extend(filtered);
	}

	(addrs, unresolved)
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

	/// A ring `0..n` of authorities in authoring order, with `local` as our single local key.
	fn ring(n: u8) -> Vec<AuthorityId> {
		(0..n).map(authority_id).collect()
	}

	fn local_set(seed: u8) -> HashSet<AuthorityId> {
		let mut s = HashSet::new();
		s.insert(authority_id(seed));
		s
	}

	#[test]
	fn ring_picks_immediate_neighbors_with_min_budget() {
		// Local at index 2 in a ring of 5; budget 2 -> exactly N-1 (1) and N+1 (3).
		let selected = select_ring_neighbors(ring(5), &local_set(2), 2);
		assert_eq!(selected, vec![authority_id(3), authority_id(1)]);
	}

	#[test]
	fn ring_wraps_around_at_the_ends() {
		// Local at index 0; neighbors are index 1 (right) and index 4 (wrapped left).
		let selected = select_ring_neighbors(ring(5), &local_set(0), 2);
		assert_eq!(selected, vec![authority_id(1), authority_id(4)]);
	}

	#[test]
	fn ring_grows_balanced_on_both_sides() {
		// Budget 4 around index 2: shell d=1 {3,1}, shell d=2 {4,0}.
		let selected = select_ring_neighbors(ring(5), &local_set(2), 4);
		assert_eq!(
			selected,
			vec![authority_id(3), authority_id(1), authority_id(4), authority_id(0)]
		);
	}

	#[test]
	fn ring_odd_budget_leaves_one_slot_unused_to_stay_symmetric() {
		// Budget 3 around index 2: only the full d=1 shell {3,1} fits; the d=2 shell can't be
		// added whole, so we stop at 2 rather than break symmetry.
		let selected = select_ring_neighbors(ring(5), &local_set(2), 3);
		assert_eq!(selected, vec![authority_id(3), authority_id(1)]);
	}

	#[test]
	fn ring_becomes_full_mesh_when_budget_covers_everyone() {
		let selected = select_ring_neighbors(ring(5), &local_set(2), 100);
		let got: HashSet<_> = selected.into_iter().collect();
		let expected: HashSet<_> = [0u8, 1, 3, 4].into_iter().map(authority_id).collect();
		assert_eq!(got, expected);
	}

	#[test]
	fn ring_excludes_self() {
		let selected = select_ring_neighbors(ring(4), &local_set(1), 100);
		assert!(!selected.contains(&authority_id(1)));
	}

	#[test]
	fn ring_handles_even_antipode_without_duplicates() {
		// n = 4, local at 0: d=1 -> {1,3}, d=2 -> antipode {2} (single node, no dup).
		let selected = select_ring_neighbors(ring(4), &local_set(0), 100);
		assert_eq!(selected, vec![authority_id(1), authority_id(3), authority_id(2)]);
	}

	#[test]
	fn ring_alone_selects_nothing() {
		let selected = select_ring_neighbors(ring(1), &local_set(0), 100);
		assert!(selected.is_empty());
	}

	#[test]
	fn ring_empty_when_not_an_authority() {
		// Local key not in the set: nothing to reserve — non-authorities rely on gossip.
		let selected = select_ring_neighbors(ring(5), &local_set(99), 3);
		assert!(selected.is_empty());
	}

	#[test]
	fn ring_neighbor_relationships_are_mutual() {
		// For every node, the neighbors it reserves must reserve it back.
		let n = 7u8;
		let budget = 4;
		let pick = |me: u8| -> HashSet<u8> {
			select_ring_neighbors(ring(n), &local_set(me), budget)
				.into_iter()
				.map(|id| AsRef::<[u8]>::as_ref(&id)[0])
				.collect()
		};
		for me in 0..n {
			for peer in pick(me) {
				assert!(
					pick(peer).contains(&me),
					"{me} reserved {peer} but {peer} did not reserve {me} back",
				);
			}
		}
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
