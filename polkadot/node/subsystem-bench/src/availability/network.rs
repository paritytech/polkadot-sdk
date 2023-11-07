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
use futures::stream::FuturesOrdered;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

// An emulated node egress traffic rate_limiter.
#[derive(Debug)]
struct RateLimit {
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
// A network peer emulator
struct PeerEmulator {
	// The queue of requests waiting to be served by the emulator
	actions_tx: UnboundedSender<NetworkAction>,
}

impl PeerEmulator {
	pub fn new(bandwidth: usize, spawn_task_handle: SpawnTaskHandle) -> Self {
		let (actions_tx, mut actions_rx) = tokio::sync::mpsc::unbounded_channel();

		spawn_task_handle.spawn("peer-emulator", "test-environment", async move {
			let mut rate_limiter = RateLimit::new(20, bandwidth);
			loop {
				let maybe_action: Option<NetworkAction> = actions_rx.recv().await;
				if let Some(action) = maybe_action {
					let size = action.size();
					rate_limiter.reap(size).await;
					action.run().await;
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
// An network action to be completed by the emulator task.
pub struct NetworkAction {
	// The function that performs the action
	run: ActionFuture,
	// The payload size that we simulate sending from a peer
	size: usize,
	// Peer index
	index: usize,
}

impl NetworkAction {
	pub fn new(index: usize, run: ActionFuture, size: usize) -> Self {
		Self { run, size, index }
	}
	pub fn size(&self) -> usize {
		self.size
	}

	pub async fn run(self) {
		self.run.await;
	}

	pub fn index(&self) -> usize {
		self.index
	}
}

// Mocks the network bridge and an arbitrary number of connected peer nodes.
// Implements network latency, bandwidth and error.
pub struct NetworkEmulator {
	// Number of peers connected on validation protocol
	n_peers: usize,
	// The maximum Rx/Tx bandwidth in bytes per second.
	bandwidth: usize,
	// Per peer network emulation
	peers: Vec<PeerEmulator>,
}

impl NetworkEmulator {
	pub fn new(n_peers: usize, bandwidth: usize, spawn_task_handle: SpawnTaskHandle) -> Self {
		Self {
			n_peers,
			bandwidth,
			peers: (0..n_peers)
				.map(|index| PeerEmulator::new(bandwidth, spawn_task_handle.clone()))
				.collect::<Vec<_>>(),
		}
	}

	pub fn submit_peer_action(&mut self, index: usize, action: NetworkAction) {
		let _ = self.peers[index].send(action);
	}
}
