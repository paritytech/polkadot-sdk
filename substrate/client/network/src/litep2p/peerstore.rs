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
	litep2p::shim::notification::peerset::PeersetCommand, peer_store::PeerStoreProvider,
	protocol_controller::ProtocolHandle, service::traits::PeerStore, ObservedRole,
	ReputationChange,
};

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use rand::{seq::SliceRandom, thread_rng};
use wasm_timer::Delay;

use sc_network_types::PeerId;
use sc_utils::mpsc::TracingUnboundedSender;

use std::{
	collections::{HashMap, HashSet},
	sync::Arc,
	time::{Duration, Instant},
};

/// Logging target for the file.
const LOG_TARGET: &str = "sub-libp2p::peerstore";

/// We don't accept nodes whose reputation is under this value.
pub const BANNED_THRESHOLD: i32 = 82 * (i32::MIN / 100);

/// Reputation change for a node when we get disconnected from it.
const _DISCONNECT_REPUTATION_CHANGE: i32 = -256;

/// Relative decrement of a reputation value that is applied every second. I.e., for inverse
/// decrement of 50 we decrease absolute value of the reputation by 1/50. This corresponds to a
/// factor of `k = 0.98`. It takes ~ `ln(0.5) / ln(k)` seconds to reduce the reputation by half,
/// or 34.3 seconds for the values above. In this setup the maximum allowed absolute value of
/// `i32::MAX` becomes 0 in ~1100 seconds (actually less due to integer arithmetic).
const INVERSE_DECREMENT: i32 = 50;

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
}

#[derive(Debug, Default)]
pub struct PeerstoreHandleInner {
	peers: HashMap<PeerId, PeerInfo>,
	protocols: Vec<TracingUnboundedSender<PeersetCommand>>,
}

#[derive(Debug, Clone, Default)]
pub struct PeerstoreHandle(Arc<Mutex<PeerstoreHandleInner>>);

impl PeerstoreHandle {
	/// Register protocol to `PeerstoreHandle`.
	///
	/// This channel is only used to disconnect banned peers and may be replaced
	/// with something else in the future.
	pub fn register_protocol(&mut self, sender: TracingUnboundedSender<PeersetCommand>) {
		self.0.lock().protocols.push(sender);
	}

	/// Add known peer to [`Peerstore`].
	pub fn add_known_peer(&mut self, peer: PeerId) {
		self.0
			.lock()
			.peers
			.insert(peer, PeerInfo { reputation: 0i32, last_updated: Instant::now(), role: None });
	}

	/// Adjust peer reputation.
	pub fn report_peer(&mut self, peer: PeerId, reputation_change: i32) {
		let mut lock = self.0.lock();

		match lock.peers.get_mut(&peer) {
			Some(info) => {
				info.reputation = info.reputation.saturating_add(reputation_change);
			},
			None => {
				lock.peers.insert(
					peer,
					PeerInfo {
						reputation: reputation_change,
						last_updated: Instant::now(),
						role: None,
					},
				);
			},
		}

		if lock
			.peers
			.get(&peer)
			.expect("peer exist since it was just modified; qed")
			.is_banned()
		{
			for sender in &lock.protocols {
				let _ = sender.unbounded_send(PeersetCommand::DisconnectPeer { peer });
			}
		}
	}

	/// Get next outbound peers for connection attempts, ignoring all peers in `ignore`.
	///
	/// Returns `None` if there are no peers available.
	pub fn next_outbound_peers(
		&self,
		ignore: &HashSet<&PeerId>,
		num_peers: usize,
	) -> impl Iterator<Item = PeerId> {
		let handle = self.0.lock();

		let mut candidates = handle
			.peers
			.iter()
			.filter_map(|(peer, info)| {
				(!ignore.contains(&peer) && !info.is_banned()).then_some((*peer, info.reputation))
			})
			.collect::<Vec<(PeerId, _)>>();
		candidates.shuffle(&mut thread_rng());
		candidates.sort_by(|(_, a), (_, b)| b.cmp(a));
		candidates
			.into_iter()
			.take(num_peers)
			.map(|(peer, _score)| peer)
			.collect::<Vec<_>>()
			.into_iter()
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
		lock.peers
			.retain(|_, info| info.reputation != 0 || info.last_updated + FORGET_AFTER > now);
	}

	pub fn is_peer_banned(&self, peer: &PeerId) -> bool {
		self.0.lock().peers.get(peer).map_or(false, |info| info.is_banned())
	}
}

impl PeerStoreProvider for PeerstoreHandle {
	fn is_banned(&self, _peer: &PeerId) -> bool {
		unimplemented!();
	}

	/// Register a protocol handle to disconnect peers whose reputation drops below the threshold.
	fn register_protocol(&self, _protocol_handle: ProtocolHandle) {
		unimplemented!();
	}

	/// Report peer disconnection for reputation adjustment.
	fn report_disconnect(&self, _peer: PeerId) {
		unimplemented!();
	}

	/// Adjust peer reputation.
	fn report_peer(&self, peer: PeerId, reputation_change: ReputationChange) {
		let mut lock = self.0.lock();

		log::trace!(target: LOG_TARGET, "report peer {reputation_change:?}");

		match lock.peers.get_mut(&peer) {
			Some(info) => {
				info.reputation = info.reputation.saturating_add(reputation_change.value);
			},
			None => {
				lock.peers.insert(
					peer,
					PeerInfo {
						reputation: reputation_change.value,
						last_updated: Instant::now(),
						role: None,
					},
				);
			},
		}

		if lock
			.peers
			.get(&peer)
			.expect("peer exist since it was just modified; qed")
			.is_banned()
		{
			log::warn!(target: LOG_TARGET, "{peer:?} banned, disconnecting, reason: {}", reputation_change.reason);

			for sender in &lock.protocols {
				let _ = sender.unbounded_send(PeersetCommand::DisconnectPeer { peer });
			}
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
		self.0.lock().peers.get(peer).map(|info| info.role).flatten()
	}

	/// Get candidates with highest reputations for initiating outgoing connections.
	fn outgoing_candidates(&self, _count: usize, _ignored: HashSet<PeerId>) -> Vec<PeerId> {
		unimplemented!();
	}

	/// Get the number of known peers.
	///
	/// This number might not include some connected peers in rare cases when their reputation
	/// was not updated for one hour, because their entries in [`PeerStore`] were dropped.
	fn num_known_peers(&self) -> usize {
		self.0.lock().peers.len()
	}

	/// Add known peer.
	fn add_known_peer(&self, peer: PeerId) {
		self.0.lock().peers.entry(peer).or_default().last_updated = Instant::now();
	}
}

/// As notification protocols are initialized in the protocol implementations and
/// `NotificationService` provided by the litep2p backend also combines `Peerset` into the
/// implementation, the protocol must be able to acquire a handle to `Peerstore` when it's
/// initializing itself.
///
/// To make that possible, crate a global static variable which be used to acquire a handle
/// to `Peerstore` so protocols can initialize themselves without having `Litep2pNetworkBackend`
/// be the master object which initializes and polles `NotificationService`s.
static PEERSTORE_HANDLE: Lazy<PeerstoreHandle> =
	Lazy::new(|| PeerstoreHandle(Arc::new(Mutex::new(Default::default()))));

/// Get handle to `Peerstore`.
pub fn peerstore_handle() -> PeerstoreHandle {
	Lazy::force(&PEERSTORE_HANDLE).clone()
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
	pub fn new(bootnodes: Vec<PeerId>) -> Self {
		let peerstore_handle = peerstore_handle();

		for bootnode in bootnodes {
			peerstore_handle.add_known_peer(bootnode);
		}

		Self { peerstore_handle }
	}

	/// Create new [`Peerstore`] from a [`PeerstoreHandle`].
	pub fn from_handle(peerstore_handle: PeerstoreHandle, bootnodes: Vec<PeerId>) -> Self {
		for bootnode in bootnodes {
			peerstore_handle.add_known_peer(bootnode);
		}

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
		Arc::new(peerstore_handle())
	}

	/// Start running `PeerStore` event loop.
	async fn run(self) {
		self.run().await;
	}
}
