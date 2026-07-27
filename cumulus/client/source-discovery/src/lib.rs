// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Dynamic discovery of a **source** parachain's peers over the relay-chain DHT.
//!
//! A cross-parachain consumer (e.g. a receiver fetching another parachain's
//! streams) must learn a *source* parachain's collator `PeerId`s per source. The
//! mechanism is RFC-0008 parachain bootnode discovery: the relay chain DHT holds
//! each parachain's advertised bootnodes under a `para_id + epoch` provider key,
//! and the `/{relay_genesis}/paranode` request-response returns their parachain
//! addresses (see `cumulus-client-bootnodes`). Doing it for an *arbitrary* source
//! parachain (not the node's own) is what this crate wires up.
//!
//! [`run_source_discovery`] is the loop: it reads the configured source set from
//! [`SourceDiscoveryApi::source_discovery_info`] (governance-set on-chain via
//! `cumulus-pallet-source-discovery`), resolves each source's peers on new best
//! blocks (+ a steady-state fallback), and keeps the shared
//! [`PeerRegistry`](cumulus_client_bootnodes::PeerRegistry) populated. It is
//! **version-gated**: a runtime without [`SourceDiscoveryApi`], or with no source
//! configured, does nothing — identical to a node without the feature.
//!
//! The DHT resolution sits behind the [`SourceDiscovery`] trait seam;
//! [`BootnodeSourceDiscovery`] is the production impl (reuses
//! `cumulus-client-bootnodes`' `discovered_tx` seam and registers the resolved
//! addresses with the network so the consumer's transport can dial them).

use std::{
	collections::HashMap,
	sync::Arc,
	time::{Duration, Instant},
};

use async_trait::async_trait;
use futures::{channel::mpsc, pin_mut, FutureExt, StreamExt};
use futures_timer::Delay;
use sc_client_api::BlockchainEvents;
use sc_network::{service::traits::NetworkService, PeerId};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;

use cumulus_client_bootnodes::{
	paranode_protocol_name, BootnodeDiscovery, BootnodeDiscoveryParams,
};
use cumulus_primitives_core::{relay_chain::BlockId, ParaId};
use cumulus_primitives_source_discovery::SourceDiscoveryApi;
use cumulus_relay_chain_interface::RelayChainInterface;

/// The health-tracked peer set consumers read, and the trait to read it —
/// re-exported from the foundation so consumers depend on one crate.
pub use cumulus_client_bootnodes::{PeerRegistry, SourcePeers};

/// Log target for this crate.
const LOG_TARGET: &str = "source-discovery";

/// How often the discovery loop re-resolves each source's peers as a steady-state
/// safety net (the consumer's fetch loop reuses live peers and evicts bad ones),
/// so this is deliberately unhurried.
pub const DISCOVERY_REFRESH_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Timeout for a single source's `/paranode` discovery round. Discovery
/// succeeds-once then stops; if a source has no reachable providers this bounds
/// how long we wait before returning what (if anything) resolved.
pub const DISCOVERY_ROUND_TIMEOUT: Duration = Duration::from_secs(30);

/// Resolves the peers of a source parachain over the relay chain DHT, given that
/// source's genesis hash. The production impl ([`BootnodeSourceDiscovery`])
/// reuses `cumulus-client-bootnodes` (RFC-0008 `/paranode` discovery) and
/// registers the discovered addresses with the network as a side effect, so the
/// returned `PeerId`s are dialable by the consumer's transport.
#[async_trait]
pub trait SourceDiscovery: Send + Sync {
	/// The currently-discovered peers serving `source` (whose parachain genesis
	/// is `genesis_hash`, fork `fork_id`), best-effort. An empty result is not an
	/// error — discovery retries on the next refresh.
	async fn discover(
		&self,
		source: ParaId,
		genesis_hash: Vec<u8>,
		fork_id: Option<String>,
	) -> Vec<PeerId>;
}

/// Runs the discovery loop until the task is dropped. Keeps the shared `registry`
/// (the set the consumer reads) populated with each configured source's peers,
/// resolved over the relay-chain DHT. The source set comes from
/// [`SourceDiscoveryApi::source_discovery_info`] (governance-set on-chain via
/// `set_source_genesis`).
///
/// Event-driven: on every new best block it reads the (cheap) source set and
/// DHT-resolves only the sources whose reachability changed or that still have no
/// peers — so a `set_source_genesis` extrinsic takes effect within ~one block —
/// and once `fallback` has elapsed it re-resolves the whole set as a steady-state
/// safety net.
pub async fn run_source_discovery<Block, Client>(
	parachain: Arc<Client>,
	discovery: Arc<dyn SourceDiscovery>,
	registry: Arc<PeerRegistry>,
	fallback: Duration,
) where
	Block: BlockT,
	Client: BlockchainEvents<Block> + ProvideRuntimeApi<Block> + HeaderBackend<Block>,
	Client::Api: SourceDiscoveryApi<Block>,
{
	let mut known: HashMap<ParaId, (Vec<u8>, Option<String>)> = HashMap::new();

	// Resolve whatever is already configured before the first block arrives.
	if let Err(error) = refresh(&*parachain, &*discovery, &registry, &mut known, true).await {
		tracing::warn!(target: LOG_TARGET, ?error, "Initial source peer discovery failed");
	}
	let mut last_full = Instant::now();

	let mut imports = parachain.import_notification_stream();
	while let Some(notification) = imports.next().await {
		if !notification.is_new_best {
			continue;
		}
		// Full re-resolve once `fallback` has elapsed; otherwise only changed /
		// peerless sources are re-resolved.
		let force = last_full.elapsed() >= fallback;
		match refresh(&*parachain, &*discovery, &registry, &mut known, force).await {
			Ok(()) => {
				if force {
					last_full = Instant::now();
				}
			},
			Err(error) => {
				tracing::warn!(target: LOG_TARGET, ?error, "Source peer discovery failed")
			},
		}
	}
}

/// The configured sources and their `(genesis_hash, fork_id)`, read from
/// [`SourceDiscoveryApi::source_discovery_info`]. Version-gated: no
/// `SourceDiscoveryApi` (or a runtime without the method) means no configured
/// sources.
fn source_genesis_map<Block, Client>(
	parachain: &Client,
) -> Result<HashMap<ParaId, (Vec<u8>, Option<String>)>, sp_api::ApiError>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + HeaderBackend<Block>,
	Client::Api: SourceDiscoveryApi<Block>,
{
	let best = parachain.info().best_hash;
	if !parachain.runtime_api().has_api::<dyn SourceDiscoveryApi<Block>>(best)? {
		return Ok(HashMap::new());
	}
	Ok(parachain
		.runtime_api()
		.source_discovery_info(best)?
		.into_iter()
		.map(|(source, (genesis, fork))| {
			(source, (genesis.to_vec(), fork.and_then(|f| String::from_utf8(f).ok())))
		})
		.collect())
}

/// One refresh pass. `force` re-resolves every configured source; otherwise only
/// those whose `(genesis, fork)` changed since `known` or that currently hold no
/// peers — the cheap per-block path. Sources dropped from the on-chain set have
/// their registry entry cleared.
async fn refresh<Block, Client>(
	parachain: &Client,
	discovery: &dyn SourceDiscovery,
	registry: &PeerRegistry,
	known: &mut HashMap<ParaId, (Vec<u8>, Option<String>)>,
	force: bool,
) -> Result<(), sp_api::ApiError>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + HeaderBackend<Block>,
	Client::Api: SourceDiscoveryApi<Block>,
{
	let current = source_genesis_map(parachain)?;
	for (source, info) in &current {
		let changed = known.get(source) != Some(info);
		let peerless = registry.peers(*source).is_empty();
		if force || changed || peerless {
			let peers = discovery.discover(*source, info.0.clone(), info.1.clone()).await;
			tracing::debug!(
				target: LOG_TARGET,
				source = %u32::from(*source),
				count = peers.len(),
				"Discovered source peers",
			);
			registry.set_peers(*source, peers);
		}
	}
	// Sources dropped from the on-chain set: clear their peers.
	for source in known.keys() {
		if !current.contains_key(source) {
			registry.set_peers(*source, Vec::new());
		}
	}
	*known = current;
	Ok(())
}

/// The production [`SourceDiscovery`]: resolves a source parachain's peers via
/// RFC-0008 relay-chain-DHT bootnode discovery, reusing `cumulus-client-bootnodes`
/// against the *source's* `para_id`/genesis (the `discovered_tx` seam streams the
/// resolved peers back). Resolved addresses are also injected into the own
/// parachain network by `BootnodeDiscovery`, so the consumer transport's
/// `TryConnect` can dial them.
pub struct BootnodeSourceDiscovery {
	/// Own parachain network — where the resolved addresses are made dialable and
	/// where the consumer's requests are sent.
	parachain_network: Arc<dyn NetworkService>,
	/// Relay chain interface (drives the DHT provider query + epoch key).
	relay_chain_interface: Arc<dyn RelayChainInterface>,
	/// Relay chain network — the DHT the source's bootnodes are advertised on.
	relay_chain_network: Arc<dyn NetworkService>,
	/// Relay chain `fork_id` — part of the `/paranode` protocol name. The source
	/// side derives it from the relay chain spec, so the client must match. `None`
	/// for every current relay.
	relay_chain_fork_id: Option<String>,
	/// Per-source discovery round timeout.
	timeout: Duration,
}

impl BootnodeSourceDiscovery {
	/// New resolver over the given relay/parachain network handles. Source genesis
	/// hashes are supplied per [`SourceDiscovery::discover`] call (from
	/// `SourceDiscoveryApi::source_discovery_info()`), so no per-source config is
	/// held here.
	pub fn new(
		parachain_network: Arc<dyn NetworkService>,
		relay_chain_interface: Arc<dyn RelayChainInterface>,
		relay_chain_network: Arc<dyn NetworkService>,
		relay_chain_fork_id: Option<String>,
	) -> Self {
		Self {
			parachain_network,
			relay_chain_interface,
			relay_chain_network,
			relay_chain_fork_id,
			timeout: DISCOVERY_ROUND_TIMEOUT,
		}
	}
}

#[async_trait]
impl SourceDiscovery for BootnodeSourceDiscovery {
	async fn discover(
		&self,
		source: ParaId,
		genesis_hash: Vec<u8>,
		fork_id: Option<String>,
	) -> Vec<PeerId> {
		// The `/paranode` protocol is keyed by the RELAY genesis — that is what
		// every collator registers and serves. Naming the request with the
		// *source's* genesis would target a protocol no node registered, so the
		// send is rejected. Name it with the relay genesis (+ relay `fork_id`); the
		// *source* genesis still rides in `parachain_genesis_hash`, where
		// `BootnodeDiscovery` verifies the responder really is that parachain.
		let relay_genesis = match self.relay_chain_interface.header(BlockId::Number(0)).await {
			Ok(Some(header)) => header.hash().as_bytes().to_vec(),
			_ => {
				tracing::debug!(
					target: LOG_TARGET,
					source = %u32::from(source),
					"Source discovery: relay chain genesis hash unavailable this round",
				);
				return Vec::new();
			},
		};

		let (tx, mut rx) = mpsc::unbounded();
		let discovery = BootnodeDiscovery::new(BootnodeDiscoveryParams {
			para_id: source,
			parachain_network: self.parachain_network.clone(),
			parachain_genesis_hash: genesis_hash.clone(),
			parachain_fork_id: fork_id.clone(),
			relay_chain_interface: self.relay_chain_interface.clone(),
			relay_chain_network: self.relay_chain_network.clone(),
			paranode_protocol_name: paranode_protocol_name(
				&relay_genesis,
				self.relay_chain_fork_id.as_deref(),
			),
			discovered_tx: Some(tx),
		});

		// Drive one discovery round, collecting resolved peers until it completes
		// (succeed-once) or the round times out.
		let mut peers = Vec::new();
		let run = discovery.run().fuse();
		let timeout = Delay::new(self.timeout).fuse();
		pin_mut!(run, timeout);
		loop {
			futures::select! {
				resolved = rx.next() => match resolved {
					Some((peer_id, _addrs)) if !peers.contains(&peer_id) => peers.push(peer_id),
					Some(_) => {},
					None => break,
				},
				result = run => {
					if let Err(error) = result {
						tracing::debug!(
							target: LOG_TARGET,
							?error,
							source = %u32::from(source),
							"Bootnode discovery round ended with error",
						);
					}
					break;
				},
				_ = timeout => {
					tracing::debug!(
						target: LOG_TARGET,
						source = %u32::from(source),
						"Bootnode discovery round timed out",
					);
					break;
				},
			}
		}
		// Drain any peers resolved-but-buffered before the round ended.
		while let Ok(Some((peer_id, _))) = rx.try_next() {
			if !peers.contains(&peer_id) {
				peers.push(peer_id);
			}
		}
		peers
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use futures::executor::block_on;

	/// A `SourceDiscovery` returning a fixed per-source peer set.
	struct StaticDiscovery(HashMap<ParaId, Vec<PeerId>>);

	#[async_trait]
	impl SourceDiscovery for StaticDiscovery {
		async fn discover(&self, source: ParaId, _: Vec<u8>, _: Option<String>) -> Vec<PeerId> {
			self.0.get(&source).cloned().unwrap_or_default()
		}
	}

	/// The resolve→register leg for `sources` (the runtime-derived source set is
	/// exercised via the node; this drives what `refresh` does per source).
	fn resolve_into(discovery: &dyn SourceDiscovery, registry: &PeerRegistry, sources: &[ParaId]) {
		for source in sources {
			let peers = block_on(discovery.discover(*source, vec![0u8; 32], None));
			registry.set_peers(*source, peers);
		}
	}

	#[test]
	fn refresh_populates_the_registry_per_source() {
		let a = ParaId::from(2000);
		let b = ParaId::from(2001);
		let (pa, pb) = (PeerId::random(), PeerId::random());
		let discovery = StaticDiscovery(HashMap::from([(a, vec![pa]), (b, vec![pb])]));
		let registry = PeerRegistry::default();

		resolve_into(&discovery, &registry, &[a, b]);

		assert_eq!(registry.peers(a), vec![pa]);
		assert_eq!(registry.peers(b), vec![pb]);
		assert!(registry.peers(ParaId::from(2002)).is_empty());
	}

	#[test]
	fn a_source_with_no_peers_yields_an_empty_entry() {
		let registry = PeerRegistry::default();
		let discovery = StaticDiscovery(HashMap::new());
		resolve_into(&discovery, &registry, &[ParaId::from(2000)]);
		assert!(registry.peers(ParaId::from(2000)).is_empty());
	}
}
