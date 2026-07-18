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
//! Unlike `eth_subscribe`, these methods are pull-based: a client installs a filter and later
//! polls it for whatever changed since the previous poll. The registry therefore only needs to
//! remember, per filter, the next block to report from; the actual log lookup is served on demand
//! by reusing [`crate::client::Client::logs`].

use crate::{client::SubstrateBlockNumber, *};
use std::{
	collections::HashMap,
	sync::{Arc, Mutex, MutexGuard},
	time::{Duration, Instant},
};

/// Filters that are not polled within this window are considered abandoned and evicted. This
/// mirrors go-ethereum's default filter deadline and bounds the memory an idle client can retain.
const FILTER_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// What a registered filter reports.
enum RegisteredFilter {
	/// A log filter. The original [`Filter`] is kept so `eth_getFilterLogs` can replay the whole
	/// range and `eth_getFilterChanges` can reuse its address/topics selection.
	Logs(Filter),
	/// A block filter, reporting the hashes of blocks added since the last poll.
	Block,
}

/// A registered filter together with the bookkeeping needed to serve incremental polls.
struct FilterEntry {
	kind: RegisteredFilter,
	/// Next block number (inclusive) that a poll should start reporting from.
	next_from: SubstrateBlockNumber,
	/// Inclusive upper bound for a log filter that pinned a concrete `toBlock`; `None` means the
	/// filter follows the chain head.
	to: Option<SubstrateBlockNumber>,
	/// Last time the filter was polled, used to evict abandoned filters.
	last_poll: Instant,
}

/// A snapshot of a filter taken while polling, so the (async) log or block lookup can run without
/// holding the registry lock across an `.await`.
pub enum FilterPoll {
	/// Poll a log filter, reporting from `from` up to the (head-capped) upper bound `to`.
	Logs { filter: Filter, from: SubstrateBlockNumber, to: Option<SubstrateBlockNumber> },
	/// Poll a block filter, reporting block hashes from `from` onwards.
	Block { from: SubstrateBlockNumber },
}

/// Thread-safe registry of installed filters, shared by the RPC server across all connections.
#[derive(Clone)]
pub struct FilterManager {
	inner: Arc<Mutex<HashMap<U256, FilterEntry>>>,
	timeout: Duration,
}

impl Default for FilterManager {
	fn default() -> Self {
		Self::new()
	}
}

impl FilterManager {
	/// Create an empty registry using the default filter timeout.
	pub fn new() -> Self {
		Self { inner: Arc::new(Mutex::new(HashMap::new())), timeout: FILTER_TIMEOUT }
	}

	fn filters(&self) -> MutexGuard<'_, HashMap<U256, FilterEntry>> {
		// The guarded sections only touch a `HashMap` and never panic, so the lock cannot actually
		// be poisoned; recover the guard regardless to keep the RPC handlers panic-free.
		self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
	}

	/// Install a log filter reporting from `next_from`, optionally bounded above by `to`.
	pub fn install_logs(
		&self,
		filter: Filter,
		next_from: SubstrateBlockNumber,
		to: Option<SubstrateBlockNumber>,
	) -> U256 {
		self.install(FilterEntry {
			kind: RegisteredFilter::Logs(filter),
			next_from,
			to,
			last_poll: Instant::now(),
		})
	}

	/// Install a block filter reporting from `next_from`.
	pub fn install_block(&self, next_from: SubstrateBlockNumber) -> U256 {
		self.install(FilterEntry {
			kind: RegisteredFilter::Block,
			next_from,
			to: None,
			last_poll: Instant::now(),
		})
	}

	fn install(&self, entry: FilterEntry) -> U256 {
		let mut filters = self.filters();
		filters.retain(|_, entry| entry.last_poll.elapsed() <= self.timeout);
		// Use an unpredictable id so a client cannot enumerate or tamper with filters it does not
		// own, matching go-ethereum's random filter ids.
		let id = loop {
			let candidate = U256::from(rand::random::<u128>());
			if !filters.contains_key(&candidate) {
				break candidate;
			}
		};
		filters.insert(id, entry);
		id
	}

	/// Snapshot a filter for polling and refresh its deadline. Returns `None` (removing the filter)
	/// if it is unknown or has expired.
	pub fn poll(&self, id: U256) -> Option<FilterPoll> {
		let mut filters = self.filters();
		if filters.get(&id)?.last_poll.elapsed() > self.timeout {
			filters.remove(&id);
			return None;
		}
		let entry = filters.get_mut(&id).expect("entry was present on the line above; qed");
		entry.last_poll = Instant::now();
		Some(match &entry.kind {
			RegisteredFilter::Logs(filter) => {
				FilterPoll::Logs { filter: filter.clone(), from: entry.next_from, to: entry.to }
			},
			RegisteredFilter::Block => FilterPoll::Block { from: entry.next_from },
		})
	}

	/// Advance a filter's cursor after a successful poll. A no-op if the filter is gone.
	pub fn advance(&self, id: U256, next_from: SubstrateBlockNumber) {
		if let Some(entry) = self.filters().get_mut(&id) {
			entry.next_from = next_from;
		}
	}

	/// Return the original [`Filter`] of a log filter and refresh its deadline. Returns `None` for
	/// an unknown, expired, or non-log filter.
	pub fn logs_filter(&self, id: U256) -> Option<Filter> {
		let mut filters = self.filters();
		if filters.get(&id)?.last_poll.elapsed() > self.timeout {
			filters.remove(&id);
			return None;
		}
		let entry = filters.get_mut(&id).expect("entry was present on the line above; qed");
		entry.last_poll = Instant::now();
		match &entry.kind {
			RegisteredFilter::Logs(filter) => Some(filter.clone()),
			RegisteredFilter::Block => None,
		}
	}

	/// Remove a filter, returning whether it existed.
	pub fn uninstall(&self, id: U256) -> bool {
		self.filters().remove(&id).is_some()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn manager() -> FilterManager {
		FilterManager::new()
	}

	#[test]
	fn install_returns_unique_ids() {
		let manager = manager();
		let a = manager.install_block(0);
		let b = manager.install_block(0);
		assert_ne!(a, b, "each installed filter should get a distinct id");
	}

	#[test]
	fn poll_log_filter_reports_cursor_then_advances() {
		let manager = manager();
		let id = manager.install_logs(Filter::new(), 10, Some(20));

		let Some(FilterPoll::Logs { from, to, .. }) = manager.poll(id) else {
			panic!("expected a log poll");
		};
		assert_eq!(from, 10);
		assert_eq!(to, Some(20));

		// The cursor only moves once the caller has served the range.
		manager.advance(id, 21);
		let Some(FilterPoll::Logs { from, .. }) = manager.poll(id) else {
			panic!("expected a log poll");
		};
		assert_eq!(from, 21);
	}

	#[test]
	fn poll_block_filter_reports_cursor() {
		let manager = manager();
		let id = manager.install_block(5);
		let Some(FilterPoll::Block { from }) = manager.poll(id) else {
			panic!("expected a block poll");
		};
		assert_eq!(from, 5);
	}

	#[test]
	fn logs_filter_returns_none_for_block_filter() {
		let manager = manager();
		let id = manager.install_block(0);
		assert!(manager.logs_filter(id).is_none(), "block filters have no replayable log range");
	}

	#[test]
	fn unknown_filter_is_not_found() {
		let manager = manager();
		assert!(manager.poll(U256::from(42u64)).is_none());
		assert!(manager.logs_filter(U256::from(42u64)).is_none());
		assert!(!manager.uninstall(U256::from(42u64)));
	}

	#[test]
	fn uninstall_removes_filter() {
		let manager = manager();
		let id = manager.install_logs(Filter::new(), 0, None);
		assert!(manager.uninstall(id));
		assert!(!manager.uninstall(id), "a filter can only be uninstalled once");
		assert!(manager.poll(id).is_none(), "a polled-after-uninstall filter is gone");
	}

	#[test]
	fn expired_filter_is_evicted_on_poll() {
		let manager =
			FilterManager { inner: Arc::new(Mutex::new(HashMap::new())), timeout: Duration::ZERO };
		let id = manager.install_block(0);
		// With a zero timeout any elapsed time counts as expired.
		std::thread::sleep(Duration::from_millis(1));
		assert!(manager.poll(id).is_none(), "an expired filter should not be polled");
		assert!(!manager.uninstall(id), "an expired filter should already be evicted");
	}
}
