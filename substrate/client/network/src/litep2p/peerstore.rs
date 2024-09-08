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

//! `Peerstore` implementation for `litep2p`.
//!
//! `Peerstore` is responsible for storing information about remote peers
//! such as their addresses, reputations, supported protocols etc.

use crate::{
	peer_store::{PeerStoreProvider, ProtocolHandle},
	service::{metrics::PeerStoreMetrics, traits::PeerStore},
	ObservedRole, ReputationChange,
};

use parking_lot::Mutex;
use prometheus_endpoint::Registry;
use wasm_timer::Delay;

use sc_network_types::PeerId;

use std::{
	collections::{HashMap, HashSet},
	sync::Arc,
	time::{Duration, Instant},
};

/// Logging target for the file.
const LOG_TARGET: &str = "sub-libp2p::peerstore";

/// We don't accept nodes whose reputation is under this value.
pub const BANNED_THRESHOLD: i32 = 71 * (i32::MIN / 100);

/// Relative decrement of a reputation value that is applied every second. I.e., for inverse
/// decrement of 200 we decrease absolute value of the reputation by 1/200.
///
/// This corresponds to a factor of `k = 0.995`, where k = 1 - 1 / INVERSE_DECREMENT.
///
/// It takes ~ `ln(0.5) / ln(k)` seconds to reduce the reputation by half, or 138.63 seconds for the
/// values above.
///
/// In this setup:
/// - `i32::MAX` becomes 0 in exactly 3544 seconds, or approximately 59 minutes
/// - `i32::MIN` escapes the banned threshold in 69 seconds
const INVERSE_DECREMENT: i32 = 200;

/// Amount of time between the moment we last updated the [`PeerStore`] entry and the moment we
/// remove it, once the reputation value reaches 0.
const FORGET_AFTER: Duration = Duration::from_secs(3600);

/// Peer information.
#[derive(Debug, Clone, Copy)]
struct PeerInfo {
	/// Reputation of the peer.
	reputation: i32,

	/// Instant when the peer was last updated.
	last_updated: Instant,

	/// Role of the peer, if known.
	role: Option<ObservedRole>,
}

impl Default for PeerInfo {
	fn default() -> Self {
		Self { reputation: 0i32, last_updated: Instant::now(), role: None }
	}
}

impl PeerInfo {
	fn is_banned(&self) -> bool {
		self.reputation < BANNED_THRESHOLD
	}

	fn add_reputation(&mut self, increment: i32) {
		self.reputation = self.reputation.saturating_add(increment);
		self.bump_last_updated();
	}

	fn decay_reputation(&mut self, seconds_passed: u64) {
		// Note that decaying the reputation value happens "on its own",
		// so we don't do `bump_last_updated()`.
		for _ in 0..seconds_passed {
			let mut diff = self.reputation / INVERSE_DECREMENT;
			if diff == 0 && self.reputation < 0 {
				diff = -1;
			} else if diff == 0 && self.reputation > 0 {
				diff = 1;
			}

			self.reputation = self.reputation.saturating_sub(diff);

			if self.reputation == 0 {
				break
			}
		}
	}

	fn bump_last_updated(&mut self) {
		self.last_updated = Instant::now();
	}
}

#[derive(Debug, Default)]
pub struct PeerstoreHandleInner {
	peers: HashMap<PeerId, PeerInfo>,
	protocols: Vec<Arc<dyn ProtocolHandle>>,
	metrics: Option<PeerStoreMetrics>,
}

#[derive(Debug, Clone, Default)]
pub struct PeerstoreHandle(Arc<Mutex<PeerstoreHandleInner>>);

impl PeerstoreHandle {
	/// Constructs a new [`PeerstoreHandle`].
	fn new(
		peers: HashMap<PeerId, PeerInfo>,
		protocols: Vec<Arc<dyn ProtocolHandle>>,
		metrics: Option<PeerStoreMetrics>,
	) -> Self {
		Self(Arc::new(Mutex::new(PeerstoreHandleInner { peers, protocols, metrics })))
	}

	/// Add known peer to [`Peerstore`].
	pub fn add_known_peer(&self, peer: PeerId) {
		self.0
			.lock()
			.peers
			.insert(peer, PeerInfo { reputation: 0i32, last_updated: Instant::now(), role: None });
	}

	pub fn peer_count(&self) -> usize {
		self.0.lock().peers.len()
	}

	fn progress_time(&self, seconds_passed: u64) {
		if seconds_passed == 0 {
			return
		}

		let mut lock = self.0.lock();

		// Drive reputation values towards 0.
		lock.peers
			.iter_mut()
			.for_each(|(_, info)| info.decay_reputation(seconds_passed));

		// Retain only entries with non-zero reputation values or not expired ones.
		let now = Instant::now();
		let mut num_banned_peers = 0;
		lock.peers.retain(|_, info| {
			if info.is_banned() {
				num_banned_peers += 1;
			}
			info.reputation != 0 || info.last_updated + FORGET_AFTER > now
		});

		if let Some(metrics) = &lock.metrics {
			metrics.num_discovered.set(lock.peers.len() as u64);
			metrics.num_banned_peers.set(num_banned_peers);
		}
	}
}

impl PeerStoreProvider for PeerstoreHandle {
	fn is_banned(&self, peer: &PeerId) -> bool {
		self.0.lock().peers.get(peer).map_or(false, |info| info.is_banned())
	}

	/// Register a protocol handle to disconnect peers whose reputation drops below the threshold.
	fn register_protocol(&self, protocol_handle: Arc<dyn ProtocolHandle>) {
		self.0.lock().protocols.push(protocol_handle);
	}

	/// Report peer disconnection for reputation adjustment.
	fn report_disconnect(&self, _peer: PeerId) {
		unimplemented!();
	}

	/// Adjust peer reputation.
	fn report_peer(&self, peer_id: PeerId, change: ReputationChange) {
		let mut lock = self.0.lock();
		let peer_info = lock.peers.entry(peer_id).or_default();
		let was_banned = peer_info.is_banned();
		peer_info.add_reputation(change.value);
		let peer_reputation = peer_info.reputation;

		log::trace!(
			target: LOG_TARGET,
			"Report {}: {:+} to {}. Reason: {}.",
			peer_id,
			change.value,
			peer_reputation,
			change.reason,
		);

		if !peer_info.is_banned() {
			if was_banned {
				log::info!(
					target: LOG_TARGET,
					"Peer {} is now unbanned: {:+} to {}. Reason: {}.",
					peer_id,
					change.value,
					peer_reputation,
					change.reason,
				);
			}
			return;
		}

		// Peer is currently banned, disconnect it from all protocols.
		lock.protocols.iter().for_each(|handle| handle.disconnect_peer(peer_id.into()));

		// The peer is banned for the first time.
		if !was_banned {
			log::warn!(
				target: LOG_TARGET,
				"Report {}: {:+} to {}. Reason: {}. Banned, disconnecting.",
				peer_id,
				change.value,
				peer_reputation,
				change.reason,
			);
			return;
		}

		// The peer was already banned and it got another negative report.
		// This may happen during a batch report.
		if change.value < 0 {
			log::debug!(
				target: LOG_TARGET,
				"Report {}: {:+} to {}. Reason: {}. Misbehaved during the ban threshold.",
				peer_id,
				change.value,
				peer_reputation,
				change.reason,
			);
		}
	}

	/// Set peer role.
	fn set_peer_role(&self, peer: &PeerId, role: ObservedRole) {
		self.0.lock().peers.entry(*peer).or_default().role = Some(role);
	}

	/// Get peer reputation.
	fn peer_reputation(&self, peer: &PeerId) -> i32 {
		self.0.lock().peers.get(peer).map_or(0i32, |info| info.reputation)
	}

	/// Get peer role, if available.
	fn peer_role(&self, peer: &PeerId) -> Option<ObservedRole> {
		self.0.lock().peers.get(peer).and_then(|info| info.role)
	}

	/// Get candidates with highest reputations for initiating outgoing connections.
	fn outgoing_candidates(&self, count: usize, ignored: HashSet<PeerId>) -> Vec<PeerId> {
		let handle = self.0.lock();

		let mut candidates = handle
			.peers
			.iter()
			.filter_map(|(peer, info)| {
				(!ignored.contains(&peer) && !info.is_banned()).then_some((*peer, info.reputation))
			})
			.collect::<Vec<(PeerId, _)>>();
		candidates.sort_by(|(_, a), (_, b)| b.cmp(a));
		candidates
			.into_iter()
			.take(count)
			.map(|(peer, _score)| peer)
			.collect::<Vec<_>>()
	}

	/// Add known peer.
	fn add_known_peer(&self, peer: PeerId) {
		self.0.lock().peers.entry(peer).or_default().last_updated = Instant::now();
	}
}

/// `Peerstore` handle for testing.
///
/// This instance of `Peerstore` is not shared between protocols.
#[cfg(test)]
pub fn peerstore_handle_test() -> PeerstoreHandle {
	PeerstoreHandle(Arc::new(Mutex::new(Default::default())))
}

/// Peerstore implementation.
pub struct Peerstore {
	/// Handle to `Peerstore`.
	peerstore_handle: PeerstoreHandle,
}

impl Peerstore {
	/// Create new [`Peerstore`].
	pub fn new(bootnodes: Vec<PeerId>, metrics_registry: Option<Registry>) -> Self {
		let metrics = if let Some(registry) = &metrics_registry {
			PeerStoreMetrics::register(registry)
				.map_err(|err| {
					log::error!(target: LOG_TARGET, "Failed to register peer store metrics: {}", err);
					err
				})
				.ok()
		} else {
			None
		};

		let peerstore_handle = PeerstoreHandle::new(
			bootnodes.iter().map(|peer_id| (*peer_id, PeerInfo::default())).collect(),
			Vec::new(),
			metrics,
		);

		Self { peerstore_handle }
	}

	/// Get mutable reference to the underlying [`PeerstoreHandle`].
	pub fn handle(&mut self) -> &mut PeerstoreHandle {
		&mut self.peerstore_handle
	}

	/// Add known peer to [`Peerstore`].
	pub fn add_known_peer(&mut self, peer: PeerId) {
		self.peerstore_handle.add_known_peer(peer);
	}

	/// Start [`Peerstore`] event loop.
	async fn run(self) {
		let started = Instant::now();
		let mut latest_time_update = started;

		loop {
			let now = Instant::now();
			// We basically do `(now - self.latest_update).as_secs()`, except that by the way we do
			// it we know that we're not going to miss seconds because of rounding to integers.
			let seconds_passed = {
				let elapsed_latest = latest_time_update - started;
				let elapsed_now = now - started;
				latest_time_update = now;
				elapsed_now.as_secs() - elapsed_latest.as_secs()
			};

			self.peerstore_handle.progress_time(seconds_passed);
			let _ = Delay::new(Duration::from_secs(1)).await;
		}
	}
}

#[async_trait::async_trait]
impl PeerStore for Peerstore {
	/// Get handle to `PeerStore`.
	fn handle(&self) -> Arc<dyn PeerStoreProvider> {
		Arc::new(self.peerstore_handle.clone())
	}

	/// Start running `PeerStore` event loop.
	async fn run(self) {
		self.run().await;
	}
}

#[cfg(test)]
mod tests {
	use super::{PeerInfo, PeerStoreProvider, Peerstore};

	#[test]
	fn decaying_zero_reputation_yields_zero() {
		let mut peer_info = PeerInfo::default();
		assert_eq!(peer_info.reputation, 0);

		peer_info.decay_reputation(1);
		assert_eq!(peer_info.reputation, 0);

		peer_info.decay_reputation(100_000);
		assert_eq!(peer_info.reputation, 0);
	}

	#[test]
	fn decaying_positive_reputation_decreases_it() {
		const INITIAL_REPUTATION: i32 = 100;

		let mut peer_info = PeerInfo::default();
		peer_info.reputation = INITIAL_REPUTATION;

		peer_info.decay_reputation(1);
		assert!(peer_info.reputation >= 0);
		assert!(peer_info.reputation < INITIAL_REPUTATION);
	}

	#[test]
	fn decaying_negative_reputation_increases_it() {
		const INITIAL_REPUTATION: i32 = -100;

		let mut peer_info = PeerInfo::default();
		peer_info.reputation = INITIAL_REPUTATION;

		peer_info.decay_reputation(1);
		assert!(peer_info.reputation <= 0);
		assert!(peer_info.reputation > INITIAL_REPUTATION);
	}

	#[test]
	fn decaying_max_reputation_finally_yields_zero() {
		const INITIAL_REPUTATION: i32 = i32::MAX;
		const SECONDS: u64 = 3544;

		let mut peer_info = PeerInfo::default();
		peer_info.reputation = INITIAL_REPUTATION;

		peer_info.decay_reputation(SECONDS / 2);
		assert!(peer_info.reputation > 0);

		peer_info.decay_reputation(SECONDS / 2);
		assert_eq!(peer_info.reputation, 0);
	}

	#[test]
	fn decaying_min_reputation_finally_yields_zero() {
		const INITIAL_REPUTATION: i32 = i32::MIN;
		const SECONDS: u64 = 3544;

		let mut peer_info = PeerInfo::default();
		peer_info.reputation = INITIAL_REPUTATION;

		peer_info.decay_reputation(SECONDS / 2);
		assert!(peer_info.reputation < 0);

		peer_info.decay_reputation(SECONDS / 2);
		assert_eq!(peer_info.reputation, 0);
	}

	#[test]
	fn report_banned_peers() {
		let peer_a = sc_network_types::PeerId::random();
		let peer_b = sc_network_types::PeerId::random();
		let peer_c = sc_network_types::PeerId::random();

		let metrics_registry = prometheus_endpoint::Registry::new();
		let mut peerstore = Peerstore::new(
			vec![peer_a, peer_b, peer_c].into_iter().map(Into::into).collect(),
			Some(metrics_registry),
		);
		let metrics = peerstore.peerstore_handle.0.lock().metrics.as_ref().unwrap().clone();
		let handle = peerstore.handle();

		// Check initial state. Advance time to propagate peers.
		handle.progress_time(1);
		assert_eq!(metrics.num_discovered.get(), 3);
		assert_eq!(metrics.num_banned_peers.get(), 0);

		// Report 2 peers with a negative reputation.
		handle.report_peer(
			peer_a,
			sc_network_common::types::ReputationChange { value: i32::MIN, reason: "test".into() },
		);
		handle.report_peer(
			peer_b,
			sc_network_common::types::ReputationChange { value: i32::MIN, reason: "test".into() },
		);

		// Advance time to propagate peers.
		handle.progress_time(1);
		assert_eq!(metrics.num_discovered.get(), 3);
		assert_eq!(metrics.num_banned_peers.get(), 2);
	}
}
