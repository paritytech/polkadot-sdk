// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.
use super::*;
use prometheus_endpoint::U64;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc::UnboundedSender;
// An emulated node egress traffic rate_limiter.
#[derive(Debug)]
pub struct RateLimit {
	// How often we refill credits in buckets
	tick_rate: usize,
	// Total ticks
	total_ticks: usize,
	// Max refill per tick
	max_refill: usize,
	// Available credit. We allow for bursts over 1/tick_rate of `cps` budget, but we
	// account it by negative credit.
	credits: isize,
	// When last refilled.
	last_refill: Instant,
}

impl RateLimit {
	// Create a new `RateLimit` from a `cps` (credits per second) budget and
	// `tick_rate`.
	pub fn new(tick_rate: usize, cps: usize) -> Self {
		// Compute how much refill for each tick
		let max_refill = cps / tick_rate;
		RateLimit {
			tick_rate,
			total_ticks: 0,
			max_refill,
			// A fresh start
			credits: max_refill as isize,
			last_refill: Instant::now(),
		}
	}

	pub async fn refill(&mut self) {
		// If this is called to early, we need to sleep until next tick.
		let now = Instant::now();
		let next_tick_delta =
			(self.last_refill + Duration::from_millis(1000 / self.tick_rate as u64)) - now;

		// Sleep until next tick.
		if !next_tick_delta.is_zero() {
			gum::trace!(target: LOG_TARGET, "need to sleep {}ms", next_tick_delta.as_millis());
			tokio::time::sleep(next_tick_delta).await;
		}

		self.total_ticks += 1;
		self.credits += self.max_refill as isize;
		self.last_refill = Instant::now();
	}

	// Reap credits from the bucket.
	// Blocks if credits budged goes negative during call.
	pub async fn reap(&mut self, amount: usize) {
		self.credits -= amount as isize;

		if self.credits >= 0 {
			return
		}

		while self.credits < 0 {
			gum::trace!(target: LOG_TARGET, "Before refill: {:?}", &self);
			self.refill().await;
			gum::trace!(target: LOG_TARGET, "After refill: {:?}", &self);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use polkadot_node_metrics::metered::CoarseDuration;
	use std::time::Instant;

	use super::RateLimit;

	#[tokio::test]
	async fn test_expected_rate() {
		let tick_rate = 200;
		let budget = 1_000_000;
		// rate must not exceeed 100 credits per second
		let mut rate_limiter = RateLimit::new(tick_rate, budget);
		let mut total_sent = 0usize;
		let start = Instant::now();

		let mut reap_amount = 0;
		while rate_limiter.total_ticks < tick_rate {
			reap_amount += 1;
			reap_amount = reap_amount % 100;

			rate_limiter.reap(reap_amount).await;
			total_sent += reap_amount;
		}

		let end = Instant::now();

		// assert_eq!(end - start, Duration::from_secs(1));
		println!("duration: {}", (end - start).as_millis());

		// Allow up to `budget/max_refill` error tolerance
		let lower_bound = budget as u128 * ((end - start).as_millis() / 1000u128);
		let upper_bound = budget as u128 *
			((end - start).as_millis() / 1000u128 + rate_limiter.max_refill as u128);
		assert!(total_sent as u128 >= lower_bound);
		assert!(total_sent as u128 <= upper_bound);
	}
}

// A network peer emulator. It spawns a task that accepts `NetworkActions` and
// executes them with a configurable delay and bandwidth constraints. Tipically
// these actions wrap a future that performs a channel send to the subsystem(s) under test.
#[derive(Clone)]
struct PeerEmulator {
	// The queue of requests waiting to be served by the emulator
	actions_tx: UnboundedSender<NetworkAction>,
}

impl PeerEmulator {
	pub fn new(
		bandwidth: usize,
		spawn_task_handle: SpawnTaskHandle,
		stats: Arc<PeerEmulatorStats>,
	) -> Self {
		let (actions_tx, mut actions_rx) = tokio::sync::mpsc::unbounded_channel();

		spawn_task_handle
			.clone()
			.spawn("peer-emulator", "test-environment", async move {
				// Rate limit peer send.
				let mut rate_limiter = RateLimit::new(10, bandwidth);
				loop {
					let stats_clone = stats.clone();
					let maybe_action: Option<NetworkAction> = actions_rx.recv().await;
					if let Some(action) = maybe_action {
						let size = action.size();
						rate_limiter.reap(size).await;
						if let Some(latency) = action.latency {
							spawn_task_handle.spawn(
								"peer-emulator-latency",
								"test-environment",
								async move {
									tokio::time::sleep(latency).await;
									action.run().await;
									stats_clone.inc_sent(size);
								},
							)
						} else {
							action.run().await;
							stats_clone.inc_sent(size);
						}
					} else {
						break
					}
				}
			});

		Self { actions_tx }
	}

	// Queue a send request from the emulated peer.
	pub fn send(&mut self, action: NetworkAction) {
		self.actions_tx.send(action).expect("peer emulator task lives");
	}
}

pub type ActionFuture = std::pin::Pin<Box<dyn futures::Future<Output = ()> + std::marker::Send>>;
/// An network action to be completed by the emulator task.
pub struct NetworkAction {
	// The function that performs the action
	run: ActionFuture,
	// The payload size that we simulate sending from a peer
	size: usize,
	// Peer which should run the action.
	peer: AuthorityDiscoveryId,
	// The amount of time to delay the polling `run`
	latency: Option<Duration>,
}

unsafe impl Send for NetworkAction {}

/// Book keeping of sent and received bytes.
pub struct PeerEmulatorStats {
	rx_bytes_total: AtomicU64,
	tx_bytes_total: AtomicU64,
	metrics: Metrics,
	peer_index: usize,
}

impl PeerEmulatorStats {
	pub(crate) fn new(peer_index: usize, metrics: Metrics) -> Self {
		Self {
			metrics,
			rx_bytes_total: AtomicU64::from(0),
			tx_bytes_total: AtomicU64::from(0),
			peer_index,
		}
	}

	pub fn inc_sent(&self, bytes: usize) {
		self.tx_bytes_total.fetch_add(bytes as u64, Ordering::Relaxed);
		self.metrics.on_peer_sent(self.peer_index, bytes as u64);
	}

	pub fn inc_received(&self, bytes: usize) {
		self.rx_bytes_total.fetch_add(bytes as u64, Ordering::Relaxed);
		self.metrics.on_peer_received(self.peer_index, bytes as u64);
	}

	pub fn sent(&self) -> u64 {
		self.tx_bytes_total.load(Ordering::Relaxed)
	}

	pub fn received(&self) -> u64 {
		self.rx_bytes_total.load(Ordering::Relaxed)
	}
}

#[derive(Debug, Default)]
pub struct PeerStats {
	pub rx_bytes_total: u64,
	pub tx_bytes_total: u64,
}
impl NetworkAction {
	pub fn new(
		peer: AuthorityDiscoveryId,
		run: ActionFuture,
		size: usize,
		latency: Option<Duration>,
	) -> Self {
		Self { run, size, peer, latency }
	}

	pub fn size(&self) -> usize {
		self.size
	}

	pub async fn run(self) {
		self.run.await;
	}

	pub fn peer(&self) -> AuthorityDiscoveryId {
		self.peer.clone()
	}
}

/// Mocks the network bridge and an arbitrary number of connected peer nodes.
/// Implements network latency, bandwidth and connection errors.
#[derive(Clone)]
pub struct NetworkEmulator {
	// Per peer network emulation.
	peers: Vec<PeerEmulator>,
	/// Per peer stats.
	stats: Vec<Arc<PeerEmulatorStats>>,
	/// Network throughput metrics
	metrics: Metrics,
	/// Each emulated peer is a validator.
	validator_authority_ids: HashMap<AuthorityDiscoveryId, usize>,
}

impl NetworkEmulator {
	pub fn new(
		n_peers: usize,
		validator_authority_ids: Vec<AuthorityDiscoveryId>,
		bandwidth: usize,
		spawn_task_handle: SpawnTaskHandle,
		registry: &Registry,
	) -> Self {
		let metrics = Metrics::new(&registry).expect("Metrics always register succesfully");
		let mut validator_authority_id_mapping = HashMap::new();

		// Create a `PeerEmulator` for each peer.
		let (stats, peers) = (0..n_peers)
			.zip(validator_authority_ids.into_iter())
			.map(|(peer_index, authority_id)| {
				validator_authority_id_mapping.insert(authority_id, peer_index);
				let stats = Arc::new(PeerEmulatorStats::new(peer_index, metrics.clone()));
				(stats.clone(), PeerEmulator::new(bandwidth, spawn_task_handle.clone(), stats))
			})
			.unzip();

		Self { peers, stats, metrics, validator_authority_ids: validator_authority_id_mapping }
	}

	pub fn submit_peer_action(&mut self, peer: AuthorityDiscoveryId, action: NetworkAction) {
		let index = self
			.validator_authority_ids
			.get(&peer)
			.expect("all test authorities are valid; qed");
		self.peers[*index].send(action);
	}

	// Returns the sent/received stats for all peers.
	pub fn peer_stats(&mut self, peer_index: usize) -> Arc<PeerEmulatorStats> {
		self.stats[peer_index].clone()
	}

	// Returns the sent/received stats for all peers.
	pub fn stats(&mut self) -> Vec<PeerStats> {
		let r = self
			.stats
			.iter()
			.map(|stats| PeerStats {
				rx_bytes_total: stats.received(),
				tx_bytes_total: stats.sent(),
			})
			.collect::<Vec<_>>();
		r
	}

	// Increment bytes sent by our node (the node that contains the subsystem under test)
	pub fn inc_sent(&self, bytes: u64) {
		// Our node always is peer 0.
		self.metrics.on_peer_sent(0, bytes);
	}

	// Increment bytes received by our node (the node that contains the subsystem under test)
	pub fn inc_received(&self, bytes: u64) {
		// Our node always is peer 0.
		self.metrics.on_peer_received(0, bytes);
	}
}

use polkadot_node_subsystem_util::metrics::{
	prometheus::{CounterVec, Opts, PrometheusError, Registry},
};

/// Emulated network metrics.
#[derive(Clone)]
pub(crate) struct Metrics {
	/// Number of bytes sent per peer.
	peer_total_sent: CounterVec<U64>,
	/// Number of received sent per peer.
	peer_total_received: CounterVec<U64>,
}

impl Metrics {
	pub fn new(registry: &Registry) -> Result<Self, PrometheusError> {
		Ok(Self {
			peer_total_sent: prometheus::register(
				CounterVec::new(
					Opts::new(
						"subsystem_benchmark_network_peer_total_bytes_sent",
						"Total number of bytes a peer has sent.",
					),
					&["peer"],
				)?,
				registry,
			)?,
			peer_total_received: prometheus::register(
				CounterVec::new(
					Opts::new(
						"subsystem_benchmark_network_peer_total_bytes_received",
						"Total number of bytes a peer has received.",
					),
					&["peer"],
				)?,
				registry,
			)?,
		})
	}

	/// Increment total sent for a peer.
	pub fn on_peer_sent(&self, peer_index: usize, bytes: u64) {
		self.peer_total_sent
			.with_label_values(vec![format!("node{}", peer_index).as_str()].as_slice())
			.inc_by(bytes);
	}

	/// Increment total receioved for a peer.
	pub fn on_peer_received(&self, peer_index: usize, bytes: u64) {
		self.peer_total_received
			.with_label_values(vec![format!("node{}", peer_index).as_str()].as_slice())
			.inc_by(bytes);
	}
}
