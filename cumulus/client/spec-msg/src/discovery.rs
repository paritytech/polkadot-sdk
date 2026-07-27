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

//! Dynamic discovery of a source parachain's exchange-serving peers, replacing
//! the static [`crate::PeerRegistry`] wiring (the MVP `--spec-msg-source-peer`
//! flag) with relay-chain-DHT discovery.
//!
//! The receiver fetches a source's streams from *that source's* collators, so
//! it must learn their `PeerId`s per source parachain. The mechanism is
//! RFC-0008 parachain bootnode discovery — the relay chain DHT holds each
//! parachain's advertised bootnodes under a `para_id + epoch` provider key, and
//! the `/{genesis}/paranode` request-response protocol returns their addresses
//! (see `cumulus-client-bootnodes`). Doing it for an *arbitrary* source
//! parachain (not the node's own) is what this module wires up.
//!
//! This half is the discovery *loop*: it derives the consumed sources exactly
//! like [`crate::run_relay_provides_monitor`] (`consumed_streams()` keys plus
//! `out_channels()` peers), resolves each source's peers on an interval, and
//! keeps the shared [`crate::PeerRegistry`] populated for the fetch pipeline.
//!
//! The actual DHT resolution sits behind [`SourceDiscovery`] — the same
//! trait-seam pattern as [`crate::ExchangeNetwork`] / [`crate::SourcePeers`].
//! The production impl reuses `cumulus-client-bootnodes`' discovery mechanism
//! (`get_providers` on the source's epoch key + the `/paranode` request), and,
//! as a side effect, registers the discovered multiaddrs with the network
//! service (`add_known_address`) so the exchange transport's `TryConnect` can
//! dial them; it then returns the `PeerId`s to pool here. That impl needs the
//! relay-chain interface + network handles and a genesis-hash source per
//! parachain, so it is wired at the node layer — a small addition to
//! `cumulus-client-bootnodes` to *return* the resolved addresses (rather than
//! only inject them into the own-parachain network) is the one upstream change
//! it needs.

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
use cumulus_primitives_core::{relay_chain::BlockId, ParaId, SpecMsgApi};
use cumulus_relay_chain_interface::RelayChainInterface;

use crate::{
	exchange::{PeerRegistry, SourcePeers},
	LOG_TARGET,
};

/// How often the discovery loop re-resolves each source's peers. Discovery is a
/// safety net over steady-state connectivity (the fetch loop reuses live peers
/// and evicts bad ones), so this is deliberately unhurried.
pub const DISCOVERY_REFRESH_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Resolves the exchange-serving peers of a source parachain over the relay
/// chain DHT, given that source's genesis hash. The production impl reuses
/// `cumulus-client-bootnodes` (RFC-0008 `/paranode` discovery) and registers
/// the discovered addresses with the network as a side effect, so the returned
/// `PeerId`s are dialable by the exchange transport.
#[async_trait]
pub trait SourceDiscovery: Send + Sync {
	/// The currently-discovered peers serving `source` (whose parachain genesis
	/// is `genesis_hash`, fork `fork_id`), best-effort. An empty result is not
	/// an error — discovery retries on the next refresh.
	async fn discover(
		&self,
		source: ParaId,
		genesis_hash: Vec<u8>,
		fork_id: Option<String>,
	) -> Vec<PeerId>;
}

/// Runs the discovery loop until the task is dropped. Keeps the shared
/// `registry` (the set the fetch pipeline reads) populated with each configured
/// source's peers, resolved over the relay-chain DHT. The source set comes from
/// `SpecMsgApi::source_discovery_info()` (set on-chain by governance via
/// `set_source_genesis`).
///
/// Event-driven: on every new best block it reads the (cheap) source set and
/// DHT-resolves only the sources whose reachability changed or that still have
/// no peers — so a `set_source_genesis` extrinsic takes effect within ~one block
/// — and once `fallback` has elapsed it re-resolves the whole set as a
/// steady-state safety net (peers that dropped, etc.). Spawn next to
/// [`crate::run_relay_provides_monitor`] / [`crate::run_spec_msg_fetcher`],
/// sharing their [`PeerRegistry`].
pub async fn run_spec_msg_discovery<Block, Client>(
	parachain: Arc<Client>,
	discovery: Arc<dyn SourceDiscovery>,
	registry: Arc<PeerRegistry>,
	fallback: Duration,
) where
	Block: BlockT,
	Client: BlockchainEvents<Block> + ProvideRuntimeApi<Block> + HeaderBackend<Block>,
	Client::Api: SpecMsgApi<Block>,
{
	let mut known: HashMap<ParaId, (Vec<u8>, Option<String>)> = HashMap::new();

	// Resolve whatever is already configured before the first block arrives.
	if let Err(error) = refresh(&*parachain, &*discovery, &registry, &mut known, true).await {
		tracing::warn!(target: LOG_TARGET, ?error, "Initial spec-msg peer discovery failed");
	}
	let mut last_full = Instant::now();

	let mut imports = parachain.import_notification_stream();
	while let Some(notification) = imports.next().await {
		if !notification.is_new_best {
			continue;
		}
		// Full re-resolve once `fallback` has elapsed (steady-state health);
		// otherwise only changed / peerless sources are re-resolved.
		let force = last_full.elapsed() >= fallback;
		match refresh(&*parachain, &*discovery, &registry, &mut known, force).await {
			Ok(()) => {
				if force {
					last_full = Instant::now();
				}
			},
			Err(error) => {
				tracing::warn!(target: LOG_TARGET, ?error, "Spec-msg peer discovery failed")
			},
		}
	}
}

/// The configured sources and their `(genesis_hash, fork_id)`, read from
/// `SpecMsgApi::source_discovery_info()`. Version-gated: no `SpecMsgApi` (or a
/// runtime without the method) means no configured sources.
fn source_genesis_map<Block, Client>(
	parachain: &Client,
) -> Result<HashMap<ParaId, (Vec<u8>, Option<String>)>, sp_api::ApiError>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + HeaderBackend<Block>,
	Client::Api: SpecMsgApi<Block>,
{
	let best = parachain.info().best_hash;
	if !parachain.runtime_api().has_api::<dyn SpecMsgApi<Block>>(best)? {
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
/// their registry entry cleared. `set_peers` replaces the whole set per source;
/// the fetch loop re-evicts any still-bad peer within its own round.
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
	Client::Api: SpecMsgApi<Block>,
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
				"Discovered spec-msg source peers",
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

/// Timeout for a single source's `/paranode` discovery round. Discovery
/// succeeds-once then stops; if a source has no reachable providers this bounds
/// how long we wait before returning what (if anything) resolved.
pub const DISCOVERY_ROUND_TIMEOUT: Duration = Duration::from_secs(30);

/// The production [`SourceDiscovery`]: resolves a source parachain's
/// exchange-serving peers via RFC-0008 relay-chain-DHT bootnode discovery,
/// reusing `cumulus-client-bootnodes` against the *source's* `para_id`/genesis
/// (the `discovered_tx` seam streams the resolved peers back). Resolved
/// addresses are also injected into the own parachain network by
/// `BootnodeDiscovery`, so the exchange transport's `TryConnect` can dial them.
pub struct BootnodeSourceDiscovery {
	/// Own parachain network — where the resolved addresses are made dialable
	/// and where `/spec-msg/exchange` requests are sent.
	parachain_network: Arc<dyn NetworkService>,
	/// Relay chain interface (drives the DHT provider query + epoch key).
	relay_chain_interface: Arc<dyn RelayChainInterface>,
	/// Relay chain network — the DHT the source's bootnodes are advertised on.
	relay_chain_network: Arc<dyn NetworkService>,
	/// Relay chain `fork_id` — part of the `/paranode` protocol name. The source
	/// side derives it from the relay chain spec (`config.chain_spec.fork_id()`),
	/// so the client must use the same value to match. `None` for every current
	/// relay.
	relay_chain_fork_id: Option<String>,
	/// Per-source discovery round timeout.
	timeout: Duration,
}

impl BootnodeSourceDiscovery {
	/// New resolver over the given relay/parachain network handles. The source
	/// genesis hashes are supplied per `discover` call (from
	/// `SpecMsgApi::source_discovery_info()` + CLI overrides), so no per-source
	/// config is held here.
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
		// The `/paranode` request-response protocol is keyed by the RELAY genesis —
		// that is what every collator registers and serves (see
		// `cumulus-client-bootnodes`: the config is built from the relay chain's
		// genesis, and own-para discovery names it likewise). Naming the request
		// with the *source's* genesis would target a protocol no node has
		// registered, so the send is rejected ("protocol doesn't exist"). Name it
		// with the relay genesis (+ the relay `fork_id`, matching the server's
		// `chain_spec.fork_id()`); the *source* genesis still rides in
		// `parachain_genesis_hash`, where `BootnodeDiscovery` verifies the responder
		// really is the expected parachain.
		let relay_genesis = match self.relay_chain_interface.header(BlockId::Number(0)).await {
			Ok(Some(header)) => header.hash().as_bytes().to_vec(),
			_ => {
				tracing::debug!(
					target: LOG_TARGET,
					source = %u32::from(source),
					"Spec-msg discovery: relay chain genesis hash unavailable this round",
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

		// Drive one discovery round, collecting resolved peers until it
		// completes (succeed-once) or the round times out.
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
	use crate::exchange::SourcePeers;
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
