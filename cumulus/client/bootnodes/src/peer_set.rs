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

//! A shared, per-source peer registry for cross-parachain consumers.
//!
//! [`PeerRegistry`] maps a source `ParaId` to the peers currently serving it. A
//! consumer that resolves a source's peers (e.g. `cumulus-client-source-discovery`,
//! or Speculative Messaging) replaces the set with [`PeerRegistry::set_peers`] as
//! it re-resolves, reads it via [`SourcePeers::peers`], and drops a peer that
//! served *verified-bad* data via [`SourcePeers::report_bad`] — which excludes it
//! for a **cooldown** (not permanently, so a transient / edge-case failure can't
//! starve the set). Transport failures are the consumer's own rotate to the next
//! peer, never a ban.
//!
//! It is deliberately small: the peers come pre-vetted by the resolver (they are a
//! source parachain's own collators, made dialable out of band via the network's
//! `add_known_address`), so the registry holds no addresses and does no probing.

use cumulus_primitives_core::ParaId;
use sc_network::PeerId;
use std::{
	collections::HashMap,
	sync::RwLock,
	time::{Duration, Instant},
};

/// How long a `report_bad` peer is excluded for. A **cooldown, not a permanent
/// ban** — a one-off or edge-case verification failure (reorg, fork, version
/// skew) recovers instead of permanently draining the set. A peer that keeps
/// serving bad data just gets re-banned each time, so it stays out while
/// misbehaving but can rejoin once it stops.
const DEFAULT_BAN_COOLDOWN: Duration = Duration::from_secs(300);

/// Read view of the maintained peer set, shared with consumers. Generic — no
/// protocol assumptions.
pub trait SourcePeers: Send + Sync {
	/// Peers currently serving `source`, in the order they were resolved
	/// (`set_peers` order; consumers rotate through it).
	fn peers(&self, source: ParaId) -> Vec<PeerId>;

	/// A consumer saw `peer` return a *verified-bad* response for `source` — drop
	/// it for a cooldown. Transport failures (dial/timeout/refusal) must NOT come
	/// here; those are a plain rotate on the consumer side, never a ban.
	fn report_bad(&self, source: ParaId, peer: PeerId);
}

/// Per-source peer state.
#[derive(Default)]
struct SourceState {
	/// Peers currently serving the source, in resolve order (deduped).
	peers: Vec<PeerId>,
	/// Peers that returned verified-bad data → excluded until the mapped
	/// `Instant` (a *cooldown*, not permanent — see [`DEFAULT_BAN_COOLDOWN`]).
	banned: HashMap<PeerId, Instant>,
}

/// Shared, per-source peer registry: `ParaId -> peers`. Implements [`SourcePeers`]
/// for consumers; the resolver fills it with [`PeerRegistry::set_peers`].
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

	/// Replace the peer set for `source` with `peers`, skipping any still in ban
	/// cooldown (and clearing bans that have expired). Called by the resolver as
	/// it re-resolves the source's peers.
	pub fn set_peers(&self, source: ParaId, peers: Vec<PeerId>) {
		let mut map = self.inner.write().expect("peer registry lock poisoned; qed");
		let state = map.entry(source).or_default();
		let now = Instant::now();
		state.peers.clear();
		for peer in peers {
			match state.banned.get(&peer) {
				Some(&until) if until > now => continue, // still cooling down
				Some(_) => {
					state.banned.remove(&peer); // cooldown elapsed — let it back in
				},
				None => {},
			}
			if !state.peers.contains(&peer) {
				state.peers.push(peer);
			}
		}
	}
}

impl SourcePeers for PeerRegistry {
	fn peers(&self, source: ParaId) -> Vec<PeerId> {
		let map = self.inner.read().expect("peer registry lock poisoned; qed");
		map.get(&source).map(|s| s.peers.clone()).unwrap_or_default()
	}

	fn report_bad(&self, source: ParaId, peer: PeerId) {
		let mut map = self.inner.write().expect("peer registry lock poisoned; qed");
		let state = map.entry(source).or_default();
		// Time-bounded exclusion, not permanent: transport/liveness failures never
		// reach here (those rotate on the consumer side), so this only fires on
		// provably-bad data — and even then the peer can rejoin after the cooldown.
		state.banned.insert(peer, Instant::now() + self.ban_cooldown);
		state.peers.retain(|p| *p != peer);
	}
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

	#[test]
	fn set_peers_replaces_the_set() {
		let reg = PeerRegistry::default();
		let (s, p1, p2) = (src(2000), peer(), peer());
		reg.set_peers(s, vec![p1, p2]);
		assert_eq!(reg.peers(s).len(), 2);
		// Replace semantics: the previous set is gone.
		let p3 = peer();
		reg.set_peers(s, vec![p3]);
		assert_eq!(reg.peers(s), vec![p3]);
	}

	#[test]
	fn report_bad_excludes_during_cooldown_and_survives_reseed() {
		let reg = PeerRegistry::new(Duration::from_secs(3600));
		let (s, p) = (src(2000), peer());
		reg.set_peers(s, vec![p]);
		reg.report_bad(s, p);
		assert!(reg.peers(s).is_empty());
		// Re-resolve during the cooldown must not re-admit the banned peer.
		reg.set_peers(s, vec![p]);
		assert!(reg.peers(s).is_empty(), "banned peer stays out while cooling down");
	}

	#[test]
	fn ban_expires_then_reseed_readmits() {
		let reg = PeerRegistry::new(Duration::from_millis(10));
		let (s, p) = (src(2000), peer());
		reg.set_peers(s, vec![p]);
		reg.report_bad(s, p);
		assert!(reg.peers(s).is_empty());
		std::thread::sleep(Duration::from_millis(25)); // let the cooldown elapse
		reg.set_peers(s, vec![p]);
		assert_eq!(reg.peers(s), vec![p], "re-admitted once the ban expired");
	}

	#[test]
	fn all_peers_banned_then_recovers() {
		// Verified-bad on every peer must not permanently starve the set —
		// after cooldown, re-resolving recovers it.
		let reg = PeerRegistry::new(Duration::from_millis(10));
		let s = src(2000);
		let peers: Vec<_> = (0..3).map(|_| peer()).collect();
		reg.set_peers(s, peers.clone());
		for p in &peers {
			reg.report_bad(s, *p);
		}
		assert!(reg.peers(s).is_empty(), "all banned → empty (the starvation window)");
		std::thread::sleep(Duration::from_millis(25));
		reg.set_peers(s, peers.clone());
		assert_eq!(reg.peers(s).len(), 3, "set recovered after the cooldown");
	}
}
