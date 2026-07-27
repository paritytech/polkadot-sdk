// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Parachain peer-set discovery & health management (Tier 1).
//!
//! ## Why a separate service
//! [`crate::BootnodeDiscovery`] is a bootstrap-*once* finder: it resolves the
//! ~20 relay-DHT bootnode providers of a parachain, injects their addresses, and
//! stops. A cross-parachain consumer — Speculative Messaging, though nothing here
//! assumes that — needs the opposite: a **continuously maintained, healthy** set
//! of a *source* parachain's peers. Different lifecycle ⇒ a separate service.
//!
//! ## What this does (Tier 1, no wire change)
//! [`run_parachain_peer_set`] runs two concurrent loops over a shared
//! [`PeerRegistry`]:
//! - **seed** — drains `seed_rx` (`(ParaId, PeerId, Vec<Multiaddr>)`) into the registry as
//!   candidates. The caller feeds this by running one [`crate::BootnodeDiscovery`] per source with
//!   a `discovered_tx` sink and tagging each `(PeerId, addrs)` with the source `ParaId`. Re-seeding
//!   (backfill) is the caller's job — re-run discovery on its own cadence (e.g. per new best block)
//!   and the healthy set self-heals.
//! - **probe** — every `probe_interval`, runs the pluggable [`CapabilityProbe`] over every known
//!   peer; promotes reachable+capable peers to *healthy*, drops ones that fail. Consumers read the
//!   healthy set via [`SourcePeers`].
//!
//! Whether a peer is *capable* (speaks the consumer's protocol) is entirely the
//! [`CapabilityProbe`]'s call, so this crate stays protocol-agnostic. For a
//! parachain running ≤ ~20 nodes this already yields a healthy set.
//!
//! ## Tier 2 (follow-up, RFC-0008 wire change)
//! Exceeding the ~20 needs peer-exchange — a `repeated bytes peers` field on the
//! `/paranode` `Response`, where an advertiser returns a bounded sample of its
//! parachain peers (from `NetworkService::network_state()`). Not implemented;
//! see [`ExpansionStrategy::PeerExchange`]. Needs networking-team / RFC sign-off.

use cumulus_primitives_core::ParaId;
use futures::{
	channel::mpsc::UnboundedReceiver, future::BoxFuture, stream::FuturesUnordered, StreamExt,
};
use log::debug;
use sc_network::{Multiaddr, PeerId};
use std::{
	collections::{HashMap, HashSet},
	sync::{Arc, RwLock},
	time::{Duration, Instant},
};

const LOG_TARGET: &str = "bootnodes::peer_set";

/// How long a `report_bad` peer is excluded for. A **cooldown, not a permanent
/// ban** — a one-off or edge-case verification failure (reorg, fork, version
/// skew) recovers instead of permanently draining the set. A peer that keeps
/// serving bad data just gets re-banned each time, so it stays out while
/// misbehaving but can rejoin once it stops.
const DEFAULT_BAN_COOLDOWN: Duration = Duration::from_secs(300);

/// Read view of the maintained peer set, shared with consumers (e.g. the
/// Speculative Messaging fetch pipeline). Generic — no protocol assumptions.
pub trait SourcePeers: Send + Sync {
	/// Currently-healthy peers for `source` (arbitrary order; consumers rotate).
	fn peers(&self, source: ParaId) -> Vec<PeerId>;

	/// A consumer saw `peer` return a *verified-bad* response for `source` — ban
	/// it. Transport failures (dial/timeout/refusal) must NOT come here; those
	/// are a plain rotate on the consumer side, never a ban.
	fn report_bad(&self, source: ParaId, peer: PeerId);
}

/// Pluggable liveness/capability check. The manager calls this to decide whether
/// a discovered peer belongs in the healthy set. Spec-msg injects one that opens
/// a cheap `/spec-msg/exchange` probe; another consumer injects its own. This is
/// what keeps the crate free of protocol specifics. Implementations should apply
/// their own timeout — a hung probe stalls that round.
pub trait CapabilityProbe: Send + Sync {
	/// Is `peer` reachable AND capable for `source`? `addrs` is borrowed only for
	/// the call; the returned future must be `'static` (copy what you need).
	fn is_healthy(
		&self,
		source: ParaId,
		peer: PeerId,
		addrs: &[Multiaddr],
	) -> BoxFuture<'static, bool>;
}

/// How the set is grown past the initial bootnode seed.
pub enum ExpansionStrategy {
	/// **Tier 1**: only the ~20 relay-DHT bootnode providers (the caller
	/// re-seeds). Sufficient when a parachain runs ≤ ~20 nodes.
	BootnodesOnly,
	/// **Tier 2** (RFC-0008 follow-up): peer-exchange over an extended
	/// `/paranode` response. Not implemented — currently treated as
	/// [`ExpansionStrategy::BootnodesOnly`] with a warning.
	PeerExchange,
}

/// Tuning for the peer-set manager.
pub struct PeerSetConfig {
	/// Desired number of healthy peers per source (informational — logged when
	/// below; the caller is responsible for feeding enough seeds to reach it).
	pub target_healthy: usize,
	/// How often to (re-)probe every known peer for liveness/capability.
	pub probe_interval: Duration,
	/// Growth strategy (Tier 1 vs Tier 2).
	pub expansion: ExpansionStrategy,
}

/// Inputs to [`run_parachain_peer_set`].
pub struct ParachainPeerSetParams {
	/// Tuning for the manager.
	pub config: PeerSetConfig,
	/// Continuously-fed `(source, peer, addrs)` from the caller's seed discovery
	/// (one [`crate::BootnodeDiscovery`] per source, tagged with its `ParaId`).
	pub seed_rx: UnboundedReceiver<(ParaId, PeerId, Vec<Multiaddr>)>,
	/// The liveness/capability probe (consumer-supplied).
	pub probe: Arc<dyn CapabilityProbe>,
	/// The shared registry the manager writes and consumers read.
	pub registry: Arc<PeerRegistry>,
}

/// Per-source peer state.
#[derive(Default)]
struct SourceState {
	/// Non-banned known peers → their addresses.
	addrs: HashMap<PeerId, Vec<Multiaddr>>,
	/// Subset of `addrs` that passed the most recent probe.
	healthy: HashSet<PeerId>,
	/// Peers that returned verified-bad data → excluded until the mapped
	/// `Instant` (a *cooldown*, not permanent — see [`DEFAULT_BAN_COOLDOWN`]).
	banned: HashMap<PeerId, Instant>,
}

/// Shared, health-tracked peer set: `ParaId -> peers + health`. Implements
/// [`SourcePeers`] for consumers; [`run_parachain_peer_set`] mutates it.
pub struct PeerRegistry {
	inner: RwLock<HashMap<ParaId, SourceState>>,
	ban_cooldown: Duration,
}

impl Default for PeerRegistry {
	fn default() -> Self {
		Self::new(DEFAULT_BAN_COOLDOWN)
	}
}

impl PeerRegistry {
	/// New registry that excludes `report_bad` peers for `ban_cooldown` (use
	/// [`PeerRegistry::default`] for [`DEFAULT_BAN_COOLDOWN`]).
	pub fn new(ban_cooldown: Duration) -> Self {
		Self { inner: RwLock::new(HashMap::new()), ban_cooldown }
	}

	/// Trusted seeding for consumers that don't run the prober
	/// ([`run_parachain_peer_set`]): **replace** the set for `source` with
	/// `peers`, marking them healthy without a probe and skipping any still in
	/// ban cooldown. Addresses are left empty — the caller makes the peers
	/// dialable out of band (e.g. `NetworkService::add_known_address`). A
	/// `report_bad` peer stays excluded across calls until its cooldown lapses.
	pub fn set_peers(&self, source: ParaId, peers: Vec<PeerId>) {
		let mut map = self.inner.write().expect("peer-set registry lock poisoned; qed");
		let state = map.entry(source).or_default();
		let now = Instant::now();
		state.healthy.clear();
		state.addrs.clear();
		for peer in peers {
			match state.banned.get(&peer) {
				Some(&until) if until > now => continue, // still cooling down
				Some(_) => {
					state.banned.remove(&peer);
				},
				None => {},
			}
			state.addrs.insert(peer, Vec::new());
			state.healthy.insert(peer);
		}
	}

	/// Merge freshly-seeded candidates, skipping peers still in ban cooldown
	/// (and clearing bans that have expired). Health is unaffected — a re-seeded
	/// peer keeps its healthy status until re-probed.
	fn set_candidates(&self, source: ParaId, peers: Vec<(PeerId, Vec<Multiaddr>)>) {
		let mut map = self.inner.write().expect("peer-set registry lock poisoned; qed");
		let state = map.entry(source).or_default();
		let now = Instant::now();
		for (peer, addrs) in peers {
			match state.banned.get(&peer) {
				Some(&until) if until > now => continue, // still cooling down
				Some(_) => {
					state.banned.remove(&peer); // cooldown elapsed — let it back in
				},
				None => {},
			}
			state.addrs.insert(peer, addrs);
		}
	}

	/// Every known peer for `source` and its addresses — the probe targets.
	fn probe_targets(&self, source: ParaId) -> Vec<(PeerId, Vec<Multiaddr>)> {
		let map = self.inner.read().expect("peer-set registry lock poisoned; qed");
		map.get(&source)
			.map(|state| state.addrs.iter().map(|(p, a)| (*p, a.clone())).collect())
			.unwrap_or_default()
	}

	/// Sources currently tracked.
	fn sources(&self) -> Vec<ParaId> {
		self.inner
			.read()
			.expect("peer-set registry lock poisoned; qed")
			.keys()
			.copied()
			.collect()
	}

	/// Apply a probe outcome: promote to healthy, or drop a peer that failed.
	fn set_health(&self, source: ParaId, peer: PeerId, healthy: bool) {
		let mut map = self.inner.write().expect("peer-set registry lock poisoned; qed");
		let Some(state) = map.get_mut(&source) else { return };
		if state.banned.contains_key(&peer) {
			return;
		}
		if healthy {
			state.healthy.insert(peer);
		} else {
			// Drop it entirely; a later seed re-adds it if it recovers.
			state.healthy.remove(&peer);
			state.addrs.remove(&peer);
		}
	}
}

impl SourcePeers for PeerRegistry {
	fn peers(&self, source: ParaId) -> Vec<PeerId> {
		let map = self.inner.read().expect("peer-set registry lock poisoned; qed");
		map.get(&source)
			.map(|state| state.healthy.iter().copied().collect())
			.unwrap_or_default()
	}

	fn report_bad(&self, source: ParaId, peer: PeerId) {
		let mut map = self.inner.write().expect("peer-set registry lock poisoned; qed");
		let state = map.entry(source).or_default();
		// Time-bounded exclusion, not permanent: transport/liveness failures
		// never reach here (those rotate or drop), so this only fires on
		// provably-bad data — and even then the peer can rejoin after cooldown.
		state.banned.insert(peer, Instant::now() + self.ban_cooldown);
		state.healthy.remove(&peer);
		state.addrs.remove(&peer);
	}
}

/// Run the peer-set manager: seed from `seed_rx` and keep a probed, healthy set
/// in `registry`. Runs until `seed_rx` closes. See the module docs for the
/// lifecycle.
pub async fn run_parachain_peer_set(params: ParachainPeerSetParams) {
	let ParachainPeerSetParams { config, seed_rx, probe, registry } = params;

	if matches!(config.expansion, ExpansionStrategy::PeerExchange) {
		debug!(
			target: LOG_TARGET,
			"peer-exchange (Tier 2) not implemented; falling back to bootnodes-only seeding",
		);
	}

	// Loop 1: drain seeds into the registry as candidates.
	let seeder = {
		let registry = registry.clone();
		async move {
			let mut seed_rx = seed_rx;
			while let Some((source, peer, addrs)) = seed_rx.next().await {
				registry.set_candidates(source, vec![(peer, addrs)]);
			}
			debug!(target: LOG_TARGET, "peer-set seed channel closed; stopping");
		}
	};

	// Loop 2: on each tick, probe every known peer and update health.
	let prober = {
		let registry = registry.clone();
		let target = config.target_healthy;
		let period = config.probe_interval;
		async move {
			let mut ticker = tokio::time::interval(period);
			loop {
				ticker.tick().await;

				let mut probes = FuturesUnordered::new();
				for source in registry.sources() {
					for (peer, addrs) in registry.probe_targets(source) {
						let probe = probe.clone();
						probes.push(async move {
							(source, peer, probe.is_healthy(source, peer, &addrs).await)
						});
					}
				}
				while let Some((source, peer, healthy)) = probes.next().await {
					registry.set_health(source, peer, healthy);
				}

				for source in registry.sources() {
					let n = registry.peers(source).len();
					if n < target {
						debug!(
							target: LOG_TARGET,
							"source {source:?}: {n}/{target} healthy peers (below target); \
							 awaiting more seeds",
						);
					}
				}
			}
		}
	};

	futures::future::join(seeder, prober).await;
}

#[cfg(test)]
mod tests {
	use super::*;

	fn peer() -> PeerId {
		PeerId::random()
	}
	fn src(id: u32) -> ParaId {
		ParaId::from(id)
	}
	fn seed(reg: &PeerRegistry, source: ParaId, peer: PeerId) {
		reg.set_candidates(source, vec![(peer, vec![])]);
	}

	#[test]
	fn seed_makes_a_probe_target_but_not_yet_healthy() {
		let reg = PeerRegistry::default();
		let (s, p) = (src(2000), peer());
		seed(&reg, s, p);
		assert_eq!(reg.probe_targets(s).len(), 1, "seeded peer is a probe target");
		assert!(reg.peers(s).is_empty(), "not healthy until probed");
	}

	#[test]
	fn passing_probe_promotes_to_healthy() {
		let reg = PeerRegistry::default();
		let (s, p) = (src(2000), peer());
		seed(&reg, s, p);
		reg.set_health(s, p, true);
		assert_eq!(reg.peers(s), vec![p]);
	}

	#[test]
	fn failing_probe_drops_but_never_bans() {
		let reg = PeerRegistry::default();
		let (s, p) = (src(2000), peer());
		seed(&reg, s, p);
		reg.set_health(s, p, true);
		reg.set_health(s, p, false); // liveness failure
		assert!(reg.peers(s).is_empty());
		assert!(reg.probe_targets(s).is_empty(), "dropped from the set");
		// A dropped peer is NOT banned — re-seed re-admits it immediately.
		seed(&reg, s, p);
		assert_eq!(reg.probe_targets(s).len(), 1);
	}

	#[test]
	fn report_bad_excludes_during_cooldown_and_blocks_reseed_and_resurrection() {
		let reg = PeerRegistry::new(Duration::from_secs(3600));
		let (s, p) = (src(2000), peer());
		seed(&reg, s, p);
		reg.set_health(s, p, true);
		reg.report_bad(s, p);
		assert!(reg.peers(s).is_empty());
		assert!(reg.probe_targets(s).is_empty());
		// Re-seed during cooldown is ignored…
		seed(&reg, s, p);
		assert!(reg.probe_targets(s).is_empty(), "banned peer not re-admitted while cooling down");
		// …and a stray passing probe can't resurrect a banned peer.
		reg.set_health(s, p, true);
		assert!(reg.peers(s).is_empty());
	}

	#[test]
	fn ban_expires_then_reseed_readmits() {
		let reg = PeerRegistry::new(Duration::from_millis(10));
		let (s, p) = (src(2000), peer());
		seed(&reg, s, p);
		reg.report_bad(s, p);
		assert!(reg.probe_targets(s).is_empty(), "banned immediately after report_bad");
		std::thread::sleep(Duration::from_millis(25)); // let the cooldown elapse
		seed(&reg, s, p);
		assert_eq!(reg.probe_targets(s).len(), 1, "re-admitted once the ban expired");
	}

	#[test]
	fn all_peers_banned_starves_then_recovers() {
		// The scenario from the review: verified-bad on every peer must not
		// permanently starve the set — after cooldown, re-seeding recovers it.
		let reg = PeerRegistry::new(Duration::from_millis(10));
		let s = src(2000);
		let peers: Vec<_> = (0..3).map(|_| peer()).collect();
		for p in &peers {
			seed(&reg, s, *p);
			reg.set_health(s, *p, true);
		}
		assert_eq!(reg.peers(s).len(), 3);
		for p in &peers {
			reg.report_bad(s, *p);
		}
		assert!(reg.peers(s).is_empty(), "all banned → empty (the starvation window)");
		std::thread::sleep(Duration::from_millis(25));
		for p in &peers {
			seed(&reg, s, *p);
			reg.set_health(s, *p, true);
		}
		assert_eq!(reg.peers(s).len(), 3, "set recovered after the cooldown");
	}

	#[test]
	fn sources_are_isolated() {
		let reg = PeerRegistry::default();
		let (a, b) = (src(2000), src(2001));
		let (pa, pb) = (peer(), peer());
		seed(&reg, a, pa);
		seed(&reg, b, pb);
		reg.set_health(a, pa, true);
		assert_eq!(reg.peers(a), vec![pa]);
		assert!(reg.peers(b).is_empty(), "b's peer unaffected by a's probe");
		let sources = reg.sources();
		assert_eq!(sources.len(), 2);
		assert!(sources.contains(&a) && sources.contains(&b));
	}

	#[test]
	fn set_peers_replaces_and_marks_healthy_without_probe() {
		let reg = PeerRegistry::default();
		let (s, p1, p2) = (src(2000), peer(), peer());
		reg.set_peers(s, vec![p1, p2]);
		// Trusted path: healthy immediately, no probe needed.
		assert_eq!(reg.peers(s).len(), 2);
		// Replace semantics: the previous set is gone.
		let p3 = peer();
		reg.set_peers(s, vec![p3]);
		assert_eq!(reg.peers(s), vec![p3]);
	}

	#[test]
	fn set_peers_skips_actively_banned_peers() {
		let reg = PeerRegistry::new(Duration::from_secs(3600));
		let (s, p) = (src(2000), peer());
		reg.set_peers(s, vec![p]);
		reg.report_bad(s, p);
		reg.set_peers(s, vec![p]); // re-seed during cooldown
		assert!(reg.peers(s).is_empty(), "banned peer not re-admitted by set_peers while cooling down");
	}
}
