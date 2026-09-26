// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! In-memory registry backing the polling filter API (`eth_newFilter`, `eth_newBlockFilter`,
//! `eth_getFilterChanges`, `eth_getFilterLogs`, `eth_uninstallFilter`).
//!
//! Both `eth_subscribe` and this polling API are driven by the same source of truth: the block and
//! log broadcast channels the client publishes every imported block and log to (see
//! [`crate::client::Client::get_block_subscription_rx`] and
//! [`crate::client::Client::get_log_subscription_rx`]). `eth_subscribe` consumes a channel through
//! a jsonrpsee sink; a polling filter instead *owns* its channel here and drains it on each
//! `eth_getFilterChanges`. Sharing one event source keeps the two APIs from diverging.
//!
//! A filter not polled within [`FILTER_TIMEOUT`] is evicted by a background task ([`run_cleanup`]),
//! as are log filters whose fixed `toBlock` the chain has already passed. The trade-off versus
//! go-ethereum's unbounded per-filter backlog is that a filter polled too slowly can miss events
//! (its cursor lags the ring and the channel reports `Lagged`); bounded memory is preferred here to
//! unbounded growth.
//!
//! [`run_cleanup`]: FilterManager::run_cleanup

use crate::{
	client::{Client, SubstrateBlockNumber},
	*,
};
use serde::{Deserialize, Serialize};
use sp_core::ConstU32;
use sp_runtime::BoundedBTreeMap;
use std::{
	sync::{Arc, Mutex},
	time::{Duration, Instant},
};
use tokio::sync::broadcast;

/// Filters not polled within this window are considered abandoned and evicted, mirroring
/// go-ethereum's default filter deadline.
const FILTER_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// How often the background task sweeps expired and no-longer-applicable filters.
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

/// Upper bound on concurrently installed filters, so a client cannot exhaust memory by opening
/// filters it never polls or uninstalls.
///
/// The worst case is single-digit megabytes. On a 64-bit target a [`FilterEntry`] is 432 bytes — a
/// 16-byte broadcast cursor, a 16-byte [`Instant`], and, for a log filter, a 400-byte [`Filter`] —
/// so a full registry is ~3.6 MiB of entries and keys, plus the map's own node overhead. A filter
/// that names many addresses or topics adds roughly 20 bytes per address and 32 per topic on top of
/// that, and `eth_newFilter` requests are already bounded by the RPC server's max request size.
///
/// The events themselves are *not* multiplied by this number: each entry holds only a cursor into
/// the client's two shared ring buffers, which stay fixed at 256 blocks (1040 bytes each) and 1000
/// logs (232 bytes each) however many filters exist. Those are the same rings `eth_subscribe`
/// already consumes, so they are not new cost, and the `Vec` a single `eth_getFilterChanges` builds
/// is bounded by them.
pub const MAX_FILTERS: u32 = 8_192;

/// Identifier handed to a client for an installed filter.
///
/// A newtype rather than a bare [`U256`] so the filter API cannot be confused with the many other
/// `U256`-typed values in the eth-rpc, and so that ids are minted in exactly one place. It is
/// transparent over its [`U256`], so on the wire it is the `0x`-prefixed quantity clients expect.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Hash,
	Debug,
	Serialize,
	Deserialize,
	derive_more::From,
)]
#[serde(transparent)]
pub struct FilterId(U256);

impl FilterId {
	/// Mint an unpredictable id. 128 random bits make ids impractical to guess, so a client cannot
	/// enumerate or tamper with filters it does not own; go-ethereum uses random ids for the same
	/// reason.
	fn random() -> Self {
		Self(U256::from(rand::random::<u128>()))
	}
}

/// Installing a filter failed because [`MAX_FILTERS`] are already installed.
#[derive(Debug, PartialEq, Eq)]
pub struct TooManyFilters;

/// What a filter watches, and the cursor it reads from.
enum FilterState {
	/// A log filter: reports the streamed logs its filter selects. The filter is kept so
	/// `eth_getFilterLogs` can replay its whole range and cleanup can test its `toBlock`.
	Logs { receiver: broadcast::Receiver<Log>, filter: LogsSubscriptionFilter },
	/// A block filter: reports the hashes of blocks imported since the previous poll.
	Block { receiver: broadcast::Receiver<Block> },
}

/// An installed filter, and the deadline it is evicted at if left unpolled.
struct FilterEntry {
	state: FilterState,
	last_poll: Instant,
}

impl FilterEntry {
	fn new(state: FilterState) -> Self {
		Self { state, last_poll: Instant::now() }
	}

	/// The request behind a log filter, so `eth_getFilterLogs` can replay its whole range. `None`
	/// for a block filter.
	fn filter(&self) -> Option<&Filter> {
		match &self.state {
			FilterState::Logs { filter, .. } => Some(filter.as_ref()),
			FilterState::Block { .. } => None,
		}
	}

	/// Whether the filter has gone unpolled for longer than [`FILTER_TIMEOUT`].
	fn is_expired(&self) -> bool {
		self.last_poll.elapsed() > FILTER_TIMEOUT
	}

	/// Everything reported since the previous poll.
	fn drain(&mut self) -> FilterResults {
		match &mut self.state {
			FilterState::Logs { receiver, filter } =>
				FilterResults::Logs(buffered(receiver).filter(|log| filter.matches(log)).collect()),
			FilterState::Block { receiver } =>
				FilterResults::Hashes(buffered(receiver).map(|block| block.hash).collect()),
		}
	}

	/// Whether to keep this filter during a cleanup sweep at the given chain head.
	///
	/// Drops filters past their deadline, and — when `head` is known — log filters whose fixed
	/// `toBlock` the head has already passed, since no future streamed log can fall in their range
	/// and so a poll could only ever return empty.
	fn should_retain(&self, head: Option<SubstrateBlockNumber>) -> bool {
		if self.is_expired() {
			return false;
		}
		let (Some(head), Some(filter)) = (head, self.filter()) else { return true };
		!matches!(
			filter.block_option,
			FilterBlockOption::Range { to_block: Some(BlockNumberOrTag::Number(to)), .. }
				if to < head
		)
	}
}

/// Everything currently buffered for `receiver`, skipping past the gap a lagging cursor leaves: the
/// shared ring is fixed-capacity, so a filter polled too slowly loses the events it missed.
fn buffered<T: Clone>(receiver: &mut broadcast::Receiver<T>) -> impl Iterator<Item = T> {
	std::iter::from_fn(move || loop {
		match receiver.try_recv() {
			Ok(item) => return Some(item),
			Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
			Err(_) => return None,
		}
	})
}

/// The installed filters, bounded by [`MAX_FILTERS`].
type Filters = BoundedBTreeMap<FilterId, FilterEntry, ConstU32<MAX_FILTERS>>;

/// Where filters get their events from, and where cleanup gets the chain head.
#[derive(Clone)]
enum Events {
	Client(Client),
	/// Standalone channels, so the registry can be unit-tested without a live node.
	#[cfg(test)]
	Channels(broadcast::Sender<Block>, broadcast::Sender<Log>),
}

impl Events {
	fn blocks(&self) -> broadcast::Receiver<Block> {
		match self {
			Self::Client(client) => client.get_block_subscription_rx(),
			#[cfg(test)]
			Self::Channels(blocks, _) => blocks.subscribe(),
		}
	}

	fn logs(&self) -> broadcast::Receiver<Log> {
		match self {
			Self::Client(client) => client.get_log_subscription_rx(),
			#[cfg(test)]
			Self::Channels(_, logs) => logs.subscribe(),
		}
	}

	/// The latest block number, or `None` while it is momentarily unavailable.
	async fn head(&self) -> Option<SubstrateBlockNumber> {
		match self {
			Self::Client(client) => client.block_number().await.ok(),
			#[cfg(test)]
			Self::Channels(..) => None,
		}
	}
}

/// Thread-safe registry of installed filters, shared by the RPC server across all connections.
#[derive(Clone)]
pub struct FilterManager {
	filters: Arc<Mutex<Filters>>,
	events: Events,
}

impl FilterManager {
	/// Create an empty registry fed by `client`'s block and log channels.
	pub fn new(client: Client) -> Self {
		Self { filters: Default::default(), events: Events::Client(client) }
	}

	fn lock(&self) -> std::sync::MutexGuard<'_, Filters> {
		// The guarded sections never panic, so the lock cannot actually be poisoned; recover the
		// guard regardless to keep the RPC handlers panic-free.
		self.filters.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
	}

	/// Look up the entry under `id` and refresh its deadline.
	///
	/// Returns `None` — and evicts the entry — if it is unknown or has gone unpolled past
	/// [`FILTER_TIMEOUT`], matching go-ethereum, which treats a filter past its deadline as gone.
	///
	/// Takes a closure rather than returning the entry, which lives behind the registry [`Mutex`]
	/// and so cannot outlive its guard.
	fn get_mut<R>(&self, id: &FilterId, f: impl FnOnce(&mut FilterEntry) -> R) -> Option<R> {
		let mut filters = self.lock();
		let entry = filters.get_mut(id)?;
		if !entry.is_expired() {
			entry.last_poll = Instant::now();
			return Some(f(entry));
		}
		filters.remove(id);
		None
	}

	/// Install `state` under a freshly minted id. Fails if [`MAX_FILTERS`] are already installed.
	fn insert(&self, state: FilterState) -> Result<FilterId, TooManyFilters> {
		let mut filters = self.lock();
		let id = loop {
			let candidate = FilterId::random();
			if !filters.contains_key(&candidate) {
				break candidate;
			}
		};
		filters.try_insert(id, FilterEntry::new(state)).map_err(|_| TooManyFilters)?;
		Ok(id)
	}

	/// Remove the filter under `id`, returning whether it existed.
	pub fn remove(&self, id: &FilterId) -> bool {
		self.lock().remove(id).is_some()
	}

	/// Install a log filter selecting streamed logs by `filter`'s address and topics.
	///
	/// Only logs from blocks imported after installation are reported, because the subscription
	/// starts at the current channel head; `filter`'s `fromBlock`/`toBlock` bound only the
	/// historical replay served by `eth_getFilterLogs`.
	pub fn install_logs(&self, filter: Filter) -> Result<FilterId, TooManyFilters> {
		self.insert(FilterState::Logs { receiver: self.events.logs(), filter: filter.into() })
	}

	/// Install a block filter reporting the hashes of blocks imported after installation.
	pub fn install_block(&self) -> Result<FilterId, TooManyFilters> {
		self.insert(FilterState::Block { receiver: self.events.blocks() })
	}

	/// Drain everything reported since the previous poll: matching logs for a log filter, or block
	/// hashes for a block filter. `None` if the filter is unknown or expired.
	pub fn poll_changes(&self, id: &FilterId) -> Option<FilterResults> {
		self.get_mut(id, FilterEntry::drain)
	}

	/// The [`Filter`] behind a log filter, so `eth_getFilterLogs` can replay its whole range.
	/// `None` for an unknown, expired, or block filter.
	pub fn logs_filter(&self, id: &FilterId) -> Option<Filter> {
		self.get_mut(id, |entry| entry.filter().cloned()).flatten()
	}

	/// Sweep expired and no-longer-applicable filters.
	fn evict(&self, head: Option<SubstrateBlockNumber>) {
		self.lock().retain(|_, entry| entry.should_retain(head));
	}

	/// Run the periodic cleanup sweep; intended to be spawned as a long-lived task.
	pub async fn run_cleanup(self) {
		let mut interval = tokio::time::interval(CLEANUP_INTERVAL);
		interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
		loop {
			interval.tick().await;
			self.evict(self.events.head().await);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// A registry fed by standalone channels, returned alongside the senders that drive it.
	fn manager() -> (FilterManager, broadcast::Sender<Block>, broadcast::Sender<Log>) {
		let (blocks, _) = broadcast::channel(8);
		let (logs, _) = broadcast::channel(8);
		let manager = FilterManager {
			filters: Default::default(),
			events: Events::Channels(blocks.clone(), logs.clone()),
		};
		(manager, blocks, logs)
	}

	fn log_from(address: u8) -> Log {
		Log { address: H160::repeat_byte(address), ..Default::default() }
	}

	fn block_with_hash(hash: u8) -> Block {
		Block { hash: H256::repeat_byte(hash), ..Default::default() }
	}

	/// A log filter matching a single emitter address.
	fn filter_for(address: u8) -> Filter {
		Filter::new().address(alloy_primitives::Address::from([address; 20]))
	}

	/// Push `last_poll` far enough into the past that the filter counts as expired.
	fn expire(manager: &FilterManager, id: FilterId) {
		let past = Instant::now().checked_sub(FILTER_TIMEOUT * 2).expect("test clock in range");
		manager.lock().get_mut(&id).expect("filter exists").last_poll = past;
	}

	/// Keeps the memory footprint documented on [`MAX_FILTERS`] honest, since it is derived from
	/// the size of an entry.
	#[test]
	fn full_registry_stays_within_the_documented_bound() {
		let worst_case = MAX_FILTERS as usize * (size_of::<FilterEntry>() + size_of::<FilterId>());
		assert!(
			worst_case < 8 * 1024 * 1024,
			"a full registry is now {worst_case} bytes; revisit the note on MAX_FILTERS"
		);
	}

	#[test]
	fn install_returns_unique_ids() {
		let (manager, ..) = manager();
		let a = manager.install_block().unwrap();
		let b = manager.install_block().unwrap();
		assert_ne!(a, b, "each installed filter should get a distinct id");
	}

	#[test]
	fn unknown_filter_is_not_found() {
		let (manager, ..) = manager();
		let id = FilterId::from(U256::from(42u64));
		assert!(manager.poll_changes(&id).is_none());
		assert!(manager.logs_filter(&id).is_none());
		assert!(!manager.remove(&id));
	}

	#[test]
	fn remove_uninstalls_filter() {
		let (manager, ..) = manager();
		let id = manager.install_logs(Filter::new()).unwrap();
		assert!(manager.remove(&id));
		assert!(!manager.remove(&id), "a filter can only be uninstalled once");
		assert!(manager.poll_changes(&id).is_none(), "a polled-after-uninstall filter is gone");
	}

	#[test]
	fn log_filter_reports_only_matching_logs() {
		let (manager, _blocks, logs) = manager();
		let id = manager.install_logs(filter_for(0xAA)).unwrap();

		// Only logs sent after the filter subscribed are seen, and only matching ones are kept.
		logs.send(log_from(0xAA)).unwrap();
		logs.send(log_from(0xBB)).unwrap();
		logs.send(log_from(0xAA)).unwrap();

		let Some(FilterResults::Logs(matched)) = manager.poll_changes(&id) else {
			panic!("expected logs");
		};
		assert_eq!(matched.len(), 2, "only the two 0xAA logs match the filter");
		assert!(matched.iter().all(|log| log.address == H160::repeat_byte(0xAA)));

		// A second poll with nothing new returns an empty set rather than the previous logs.
		let Some(FilterResults::Logs(matched)) = manager.poll_changes(&id) else {
			panic!("expected logs");
		};
		assert!(matched.is_empty(), "changes since the last poll should be empty");
	}

	#[test]
	fn block_filter_reports_new_block_hashes() {
		let (manager, blocks, _logs) = manager();
		let id = manager.install_block().unwrap();

		blocks.send(block_with_hash(1)).unwrap();
		blocks.send(block_with_hash(2)).unwrap();

		let Some(FilterResults::Hashes(hashes)) = manager.poll_changes(&id) else {
			panic!("expected hashes");
		};
		assert_eq!(hashes, vec![H256::repeat_byte(1), H256::repeat_byte(2)]);
	}

	#[test]
	fn logs_filter_returns_none_for_block_filter() {
		let (manager, ..) = manager();
		let id = manager.install_block().unwrap();
		assert!(manager.logs_filter(&id).is_none(), "block filters have no replayable log range");
	}

	#[test]
	fn install_is_capped() {
		let (manager, ..) = manager();
		for _ in 0..MAX_FILTERS {
			manager.install_block().expect("under the cap");
		}
		assert_eq!(manager.install_block(), Err(TooManyFilters), "the cap is enforced");
	}

	#[test]
	fn expired_filter_is_evicted_on_poll() {
		let (manager, ..) = manager();
		let id = manager.install_block().unwrap();
		expire(&manager, id);
		assert!(manager.poll_changes(&id).is_none(), "an expired filter should not be polled");
		assert!(!manager.remove(&id), "an expired filter should already be evicted");
	}

	#[test]
	fn cleanup_drops_expired_and_stale_filters() {
		let (manager, ..) = manager();

		// Fresh block filter: kept.
		let fresh = manager.install_block().unwrap();
		// Expired filter: dropped on the deadline alone.
		let stale_deadline = manager.install_block().unwrap();
		expire(&manager, stale_deadline);
		// Log filter bounded to block 5: dropped once the head passes it, even though fresh.
		let bounded = manager.install_logs(Filter::new().to_block(5u64)).unwrap();

		manager.evict(Some(10));

		assert!(manager.poll_changes(&fresh).is_some(), "fresh filter survives");
		assert!(manager.poll_changes(&stale_deadline).is_none(), "expired filter dropped");
		assert!(manager.poll_changes(&bounded).is_none(), "filter past its toBlock dropped");
	}
}
