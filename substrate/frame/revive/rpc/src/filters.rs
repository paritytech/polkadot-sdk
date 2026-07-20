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
//! Memory is bounded on two axes:
//! - the number of installed filters is capped at [`MAX_FILTERS`], so a client cannot exhaust
//!   memory by opening filters it never polls or uninstalls;
//! - each filter holds only a broadcast *cursor* (a handle into the shared, fixed-capacity ring
//!   buffer) rather than its own growing backlog, so a million filters cost a million cursors and
//!   one shared buffer, not a million buffers.
//!
//! A filter not polled within [`FILTER_TIMEOUT`] is evicted by a background task ([`run_cleanup`]),
//! as are log filters whose fixed `toBlock` the chain has already passed. The trade-off versus
//! go-ethereum's unbounded per-filter backlog is that a filter polled too slowly can miss events
//! (its cursor lags the ring and the channel reports `Lagged`); bounded memory is preferred here to
//! unbounded growth.
//!
//! [`run_cleanup`]: FilterManager::run_cleanup

use crate::{client::SubstrateBlockNumber, *};
use std::{
	collections::HashMap,
	fmt,
	sync::{Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard},
	time::{Duration, Instant},
};
use tokio::sync::broadcast;

/// Filters not polled within this window are considered abandoned and evicted, mirroring
/// go-ethereum's default filter deadline.
const FILTER_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// How often the background task sweeps expired and no-longer-applicable filters.
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

/// Upper bound on concurrently installed filters. Installing beyond this is rejected so that a
/// client cannot exhaust memory by opening filters without ever polling or uninstalling them.
pub const MAX_FILTERS: usize = 8_192;

/// Identifier handed to a client for an installed filter.
///
/// A newtype rather than a bare [`U256`] so the filter-manager interface cannot be confused with
/// the many other `U256`-typed values in the eth-rpc, and so that ids are minted in exactly one
/// place.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct FilterId(U256);

impl FilterId {
	/// Mint an unpredictable id. 128 random bits make ids impractical to guess, so a client cannot
	/// enumerate or tamper with filters it does not own; go-ethereum uses random ids for the same
	/// reason.
	fn random() -> Self {
		Self(U256::from(rand::random::<u128>()))
	}
}

impl From<U256> for FilterId {
	fn from(value: U256) -> Self {
		Self(value)
	}
}

impl From<FilterId> for U256 {
	fn from(value: FilterId) -> Self {
		value.0
	}
}

impl fmt::Display for FilterId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}

/// Installing a filter failed because [`MAX_FILTERS`] are already installed.
#[derive(Debug, PartialEq, Eq)]
pub struct TooManyFilters;

/// Poll state mutated on every `eth_getFilterChanges`. Guarded on its own so concurrent polls of
/// different filters only need a shared read lock on the registry.
enum PollState {
	/// A log filter: drains streamed logs and keeps those the `matcher` selects, using the same
	/// address/topics logic as `eth_subscribe`.
	Logs { receiver: broadcast::Receiver<Log>, matcher: LogsSubscriptionFilter, last_poll: Instant },
	/// A block filter: drains and reports the hashes of blocks imported since the previous poll.
	Block { receiver: broadcast::Receiver<Block>, last_poll: Instant },
}

impl PollState {
	fn last_poll(&self) -> Instant {
		match self {
			PollState::Logs { last_poll, .. } | PollState::Block { last_poll, .. } => *last_poll,
		}
	}

	/// Refresh the deadline; called whenever the filter is successfully polled.
	fn touch(&mut self) {
		let now = Instant::now();
		match self {
			PollState::Logs { last_poll, .. } | PollState::Block { last_poll, .. } => {
				*last_poll = now
			},
		}
	}

	/// Drain everything reported since the previous poll.
	fn drain(&mut self) -> FilterResults {
		match self {
			PollState::Logs { receiver, matcher, .. } => {
				let mut logs = Vec::new();
				drain_receiver(receiver, "log", |log| {
					if matcher.matches(&log) {
						logs.push(log);
					}
				});
				FilterResults::Logs(logs)
			},
			PollState::Block { receiver, .. } => {
				let mut hashes = Vec::new();
				drain_receiver(receiver, "block", |block| hashes.push(block.hash));
				FilterResults::Hashes(hashes)
			},
		}
	}
}

/// Drain a broadcast receiver of everything currently buffered, handing each item to `on_item`.
///
/// A `Lagged` cursor means the shared ring advanced past this filter (it was polled too slowly) and
/// the skipped items are lost for it; that is the deliberate cost of bounding memory rather than
/// keeping an unbounded per-filter backlog. Draining continues past the gap.
fn drain_receiver<T: Clone>(
	receiver: &mut broadcast::Receiver<T>,
	kind: &str,
	mut on_item: impl FnMut(T),
) {
	loop {
		match receiver.try_recv() {
			Ok(item) => on_item(item),
			Err(broadcast::error::TryRecvError::Empty) |
			Err(broadcast::error::TryRecvError::Closed) => break,
			Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
				log::warn!(target: LOG_TARGET, "{kind} filter lagged; {skipped} events dropped");
			},
		}
	}
}

/// An installed filter: its immutable definition plus the mutable poll state.
struct FilterEntry {
	/// The original request of a log filter, kept so `eth_getFilterLogs` can replay its whole
	/// range and cleanup can test its `toBlock`; `None` for a block filter. Immutable, so it is
	/// read directly under the registry lock without touching the per-entry [`Mutex`].
	filter: Option<Filter>,
	state: Mutex<PollState>,
}

impl FilterEntry {
	/// Whether to keep this filter during a cleanup sweep at the given chain head.
	///
	/// Drops filters not polled within [`FILTER_TIMEOUT`], and — when `head` is known — log filters
	/// whose fixed `toBlock` the head has already passed, since no future streamed log can fall in
	/// their range and so a poll could only ever return empty.
	fn should_retain(&self, head: Option<SubstrateBlockNumber>) -> bool {
		let last_poll = self.state.lock().unwrap_or_else(|p| p.into_inner()).last_poll();
		if last_poll.elapsed() > FILTER_TIMEOUT {
			return false;
		}
		if let (Some(head), Some(filter)) = (head, &self.filter) {
			if let FilterBlockOption::Range {
				to_block: Some(BlockNumberOrTag::Number(to)), ..
			} = &filter.block_option
			{
				if *to < head {
					return false;
				}
			}
		}
		true
	}
}

/// Thread-safe registry of installed filters, shared by the RPC server across all connections.
///
/// The registry lock is an [`RwLock`] because polls (reads) vastly outnumber installs and
/// uninstalls (writes): a poll takes the read lock to find its entry and then a per-entry
/// [`Mutex`], so different filters poll concurrently, while only structural changes take the write
/// lock.
#[derive(Clone)]
pub struct FilterManager {
	inner: Arc<RwLock<HashMap<FilterId, FilterEntry>>>,
}

impl Default for FilterManager {
	fn default() -> Self {
		Self::new()
	}
}

impl FilterManager {
	/// Create an empty registry.
	pub fn new() -> Self {
		Self { inner: Arc::new(RwLock::new(HashMap::new())) }
	}

	fn read(&self) -> RwLockReadGuard<'_, HashMap<FilterId, FilterEntry>> {
		// The guarded sections never panic, so the lock cannot actually be poisoned; recover the
		// guard regardless to keep the RPC handlers panic-free.
		self.inner.read().unwrap_or_else(|poisoned| poisoned.into_inner())
	}

	fn write(&self) -> RwLockWriteGuard<'_, HashMap<FilterId, FilterEntry>> {
		self.inner.write().unwrap_or_else(|poisoned| poisoned.into_inner())
	}

	/// Install a log filter fed by `receiver`, selecting streamed logs by `filter`'s address and
	/// topics. Fails if [`MAX_FILTERS`] are already installed.
	pub fn install_logs(
		&self,
		filter: Filter,
		receiver: broadcast::Receiver<Log>,
	) -> Result<FilterId, TooManyFilters> {
		let matcher = LogsSubscriptionFilter::from_filter(&filter);
		self.install(Some(filter), PollState::Logs { receiver, matcher, last_poll: Instant::now() })
	}

	/// Install a block filter fed by `receiver`. Only blocks imported after installation are
	/// reported, because the receiver starts at the current channel head — there is no manual
	/// `head + 1` bookkeeping. Fails if [`MAX_FILTERS`] are already installed.
	pub fn install_block(
		&self,
		receiver: broadcast::Receiver<Block>,
	) -> Result<FilterId, TooManyFilters> {
		self.install(None, PollState::Block { receiver, last_poll: Instant::now() })
	}

	fn install(
		&self,
		filter: Option<Filter>,
		state: PollState,
	) -> Result<FilterId, TooManyFilters> {
		let mut filters = self.write();
		if filters.len() >= MAX_FILTERS {
			return Err(TooManyFilters);
		}
		let id = loop {
			let candidate = FilterId::random();
			if !filters.contains_key(&candidate) {
				break candidate;
			}
		};
		filters.insert(id, FilterEntry { filter, state: Mutex::new(state) });
		Ok(id)
	}

	/// Drain everything reported since the previous poll: matching logs for a log filter, or block
	/// hashes for a block filter. Returns `None` (evicting the filter) if it is unknown or has not
	/// been polled within [`FILTER_TIMEOUT`].
	pub fn poll_changes(&self, id: FilterId) -> Option<FilterResults> {
		{
			let filters = self.read();
			let entry = filters.get(&id)?;
			let mut state = entry.state.lock().unwrap_or_else(|p| p.into_inner());
			if state.last_poll().elapsed() <= FILTER_TIMEOUT {
				state.touch();
				return Some(state.drain());
			}
		}
		// Expired: escalate to the write lock and remove it, matching go-ethereum, which treats a
		// filter past its deadline as gone.
		self.write().remove(&id);
		None
	}

	/// Return the original [`Filter`] of a log filter, so `eth_getFilterLogs` can replay its whole
	/// range. Returns `None` for an unknown, expired, or block filter.
	pub fn logs_filter(&self, id: FilterId) -> Option<Filter> {
		{
			let filters = self.read();
			let entry = filters.get(&id)?;
			let mut state = entry.state.lock().unwrap_or_else(|p| p.into_inner());
			if state.last_poll().elapsed() <= FILTER_TIMEOUT {
				state.touch();
				return entry.filter.clone();
			}
		}
		self.write().remove(&id);
		None
	}

	/// Remove a filter, returning whether it existed.
	pub fn uninstall(&self, id: FilterId) -> bool {
		self.write().remove(&id).is_some()
	}

	/// Sweep expired and no-longer-applicable filters. `head`, when known, additionally drops log
	/// filters whose fixed `toBlock` the chain has passed.
	pub fn evict(&self, head: Option<SubstrateBlockNumber>) {
		self.write().retain(|_, entry| entry.should_retain(head));
	}

	/// Run the periodic cleanup loop; intended to be spawned as a long-lived task. `current_head`
	/// yields the latest block number (or `None` while momentarily unavailable) for staleness
	/// checks.
	pub async fn run_cleanup<F, Fut>(self, current_head: F)
	where
		F: Fn() -> Fut,
		Fut: std::future::Future<Output = Option<SubstrateBlockNumber>>,
	{
		let mut interval = tokio::time::interval(CLEANUP_INTERVAL);
		interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
		loop {
			interval.tick().await;
			self.evict(current_head().await);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

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
		let filters = manager.write();
		let mut state = filters.get(&id).expect("filter exists").state.lock().unwrap();
		let past = Instant::now().checked_sub(FILTER_TIMEOUT * 2).expect("test clock in range");
		match &mut *state {
			PollState::Logs { last_poll, .. } | PollState::Block { last_poll, .. } => {
				*last_poll = past
			},
		}
	}

	#[test]
	fn install_returns_unique_ids() {
		let manager = FilterManager::new();
		let (tx, _rx) = broadcast::channel::<Block>(4);
		let a = manager.install_block(tx.subscribe()).unwrap();
		let b = manager.install_block(tx.subscribe()).unwrap();
		assert_ne!(a, b, "each installed filter should get a distinct id");
	}

	#[test]
	fn unknown_filter_is_not_found() {
		let manager = FilterManager::new();
		let id = FilterId::from(U256::from(42u64));
		assert!(manager.poll_changes(id).is_none());
		assert!(manager.logs_filter(id).is_none());
		assert!(!manager.uninstall(id));
	}

	#[test]
	fn uninstall_removes_filter() {
		let manager = FilterManager::new();
		let (tx, _rx) = broadcast::channel::<Log>(4);
		let id = manager.install_logs(Filter::new(), tx.subscribe()).unwrap();
		assert!(manager.uninstall(id));
		assert!(!manager.uninstall(id), "a filter can only be uninstalled once");
		assert!(manager.poll_changes(id).is_none(), "a polled-after-uninstall filter is gone");
	}

	#[test]
	fn log_filter_reports_only_matching_logs() {
		let manager = FilterManager::new();
		let (tx, _rx) = broadcast::channel::<Log>(8);
		let id = manager.install_logs(filter_for(0xAA), tx.subscribe()).unwrap();

		// Only logs sent after the filter subscribed are seen, and only matching ones are kept.
		tx.send(log_from(0xAA)).unwrap();
		tx.send(log_from(0xBB)).unwrap();
		tx.send(log_from(0xAA)).unwrap();

		let Some(FilterResults::Logs(logs)) = manager.poll_changes(id) else {
			panic!("expected logs");
		};
		assert_eq!(logs.len(), 2, "only the two 0xAA logs match the filter");
		assert!(logs.iter().all(|l| l.address == H160::repeat_byte(0xAA)));

		// A second poll with nothing new returns an empty set rather than the previous logs.
		let Some(FilterResults::Logs(logs)) = manager.poll_changes(id) else {
			panic!("expected logs");
		};
		assert!(logs.is_empty(), "changes since the last poll should be empty");
	}

	#[test]
	fn block_filter_reports_new_block_hashes() {
		let manager = FilterManager::new();
		let (tx, _rx) = broadcast::channel::<Block>(8);
		let id = manager.install_block(tx.subscribe()).unwrap();

		tx.send(block_with_hash(1)).unwrap();
		tx.send(block_with_hash(2)).unwrap();

		let Some(FilterResults::Hashes(hashes)) = manager.poll_changes(id) else {
			panic!("expected hashes");
		};
		assert_eq!(hashes, vec![H256::repeat_byte(1), H256::repeat_byte(2)]);
	}

	#[test]
	fn logs_filter_returns_none_for_block_filter() {
		let manager = FilterManager::new();
		let (tx, _rx) = broadcast::channel::<Block>(4);
		let id = manager.install_block(tx.subscribe()).unwrap();
		assert!(manager.logs_filter(id).is_none(), "block filters have no replayable log range");
	}

	#[test]
	fn install_is_capped() {
		let manager = FilterManager::new();
		let (tx, _rx) = broadcast::channel::<Block>(4);
		for _ in 0..MAX_FILTERS {
			manager.install_block(tx.subscribe()).expect("under the cap");
		}
		assert_eq!(
			manager.install_block(tx.subscribe()),
			Err(TooManyFilters),
			"installing beyond the cap is rejected"
		);
	}

	#[test]
	fn expired_filter_is_evicted_on_poll() {
		let manager = FilterManager::new();
		let (tx, _rx) = broadcast::channel::<Block>(4);
		let id = manager.install_block(tx.subscribe()).unwrap();
		expire(&manager, id);
		assert!(manager.poll_changes(id).is_none(), "an expired filter should not be polled");
		assert!(!manager.uninstall(id), "an expired filter should already be evicted");
	}

	#[test]
	fn cleanup_drops_expired_and_stale_filters() {
		let manager = FilterManager::new();
		let (blocks, _b) = broadcast::channel::<Block>(4);
		let (logs, _l) = broadcast::channel::<Log>(4);

		// Fresh block filter: kept.
		let fresh = manager.install_block(blocks.subscribe()).unwrap();
		// Expired filter: dropped on the deadline alone.
		let stale_deadline = manager.install_block(blocks.subscribe()).unwrap();
		expire(&manager, stale_deadline);
		// Log filter bounded to block 5: dropped once the head passes it, even though fresh.
		let bounded = manager.install_logs(Filter::new().to_block(5u64), logs.subscribe()).unwrap();

		manager.evict(Some(10));

		assert!(manager.poll_changes(fresh).is_some(), "fresh filter survives");
		assert!(manager.poll_changes(stale_deadline).is_none(), "expired filter dropped");
		assert!(manager.poll_changes(bounded).is_none(), "filter past its toBlock dropped");
	}
}
