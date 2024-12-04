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

use std::{collections::HashMap, fmt::Debug, hash};

use linked_hash_map::LinkedHashMap;
use log::trace;
use sc_transaction_pool_api::TransactionStatus;
use sc_utils::mpsc::{tracing_unbounded, TracingUnboundedReceiver, TracingUnboundedSender};
use serde::Serialize;
use sp_runtime::traits;

use super::{watcher, BlockHash, ChainApi, ExtrinsicHash};

static LOG_TARGET: &str = "txpool::watcher";

/// Single event used in dropped by limits stream. It is one of Ready/Future/Dropped.
pub type DroppedByLimitsEvent<H, BH> = (H, TransactionStatus<H, BH>);
/// Stream of events used to determine if a transaction was dropped.
pub type DroppedByLimitsStream<H, BH> = TracingUnboundedReceiver<DroppedByLimitsEvent<H, BH>>;

/// Extrinsic pool default listener.
pub struct Listener<H: hash::Hash + Eq, C: ChainApi> {
	watchers: HashMap<H, watcher::Sender<H, BlockHash<C>>>,
	finality_watchers: LinkedHashMap<ExtrinsicHash<C>, Vec<H>>,

	/// The sink used to notify dropped-by-enforcing-limits transactions. Also ready and future
	/// statuses are reported via this channel to allow consumer of the stream tracking actual
	/// drops.
	dropped_by_limits_sink: Option<TracingUnboundedSender<DroppedByLimitsEvent<H, BlockHash<C>>>>,
}

/// Maximum number of blocks awaiting finality at any time.
const MAX_FINALITY_WATCHERS: usize = 512;

impl<H: hash::Hash + Eq + Debug, C: ChainApi> Default for Listener<H, C> {
	fn default() -> Self {
		Self {
			watchers: Default::default(),
			finality_watchers: Default::default(),
			dropped_by_limits_sink: None,
		}
	}
}

impl<H: hash::Hash + traits::Member + Serialize + Clone, C: ChainApi> Listener<H, C> {
	fn fire<F>(&mut self, hash: &H, fun: F)
	where
		F: FnOnce(&mut watcher::Sender<H, ExtrinsicHash<C>>),
	{
		let clean = if let Some(h) = self.watchers.get_mut(hash) {
			fun(h);
			h.is_done()
		} else {
			false
		};

		if clean {
			self.watchers.remove(hash);
		}
	}

	/// Creates a new watcher for given verified extrinsic.
	///
	/// The watcher can be used to subscribe to life-cycle events of that extrinsic.
	pub fn create_watcher(&mut self, hash: H) -> watcher::Watcher<H, ExtrinsicHash<C>> {
		let sender = self.watchers.entry(hash.clone()).or_insert_with(watcher::Sender::default);
		sender.new_watcher(hash)
	}

	/// Creates a new single stream for entire pool.
	///
	/// The stream can be used to subscribe to life-cycle events of all extrinsics in the pool.
	pub fn create_dropped_by_limits_stream(&mut self) -> DroppedByLimitsStream<H, BlockHash<C>> {
		let (sender, single_stream) = tracing_unbounded("mpsc_txpool_watcher", 100_000);
		self.dropped_by_limits_sink = Some(sender);
		single_stream
	}

	/// Notify the listeners about extrinsic broadcast.
	pub fn broadcasted(&mut self, hash: &H, peers: Vec<String>) {
		trace!(target: LOG_TARGET, "[{:?}] Broadcasted", hash);
		self.fire(hash, |watcher| watcher.broadcast(peers));
	}

	/// New transaction was added to the ready pool or promoted from the future pool.
	pub fn ready(&mut self, tx: &H, old: Option<&H>) {
		trace!(target: LOG_TARGET, "[{:?}] Ready (replaced with {:?})", tx, old);
		self.fire(tx, |watcher| watcher.ready());
		if let Some(old) = old {
			self.fire(old, |watcher| watcher.usurped(tx.clone()));
		}

		if let Some(ref sink) = self.dropped_by_limits_sink {
			if let Err(e) = sink.unbounded_send((tx.clone(), TransactionStatus::Ready)) {
				trace!(target: LOG_TARGET, "[{:?}] dropped_sink/ready: send message failed: {:?}", tx, e);
			}
		}
	}

	/// New transaction was added to the future pool.
	pub fn future(&mut self, tx: &H) {
		trace!(target: LOG_TARGET, "[{:?}] Future", tx);
		self.fire(tx, |watcher| watcher.future());
		if let Some(ref sink) = self.dropped_by_limits_sink {
			if let Err(e) = sink.unbounded_send((tx.clone(), TransactionStatus::Future)) {
				trace!(target: LOG_TARGET, "[{:?}] dropped_sink/future: send message failed: {:?}", tx, e);
			}
		}
	}

	/// Transaction was dropped from the pool because of the limit.
	///
	/// If the function was actually called due to enforcing limits, the `limits_enforced` flag
	/// shall be set to true.
	pub fn dropped(&mut self, tx: &H, by: Option<&H>, limits_enforced: bool) {
		trace!(target: LOG_TARGET, "[{:?}] Dropped (replaced with {:?})", tx, by);
		self.fire(tx, |watcher| match by {
			Some(t) => watcher.usurped(t.clone()),
			None => watcher.dropped(),
		});

		//note: LimitEnforced could be introduced as new status to get rid of this flag.
		if limits_enforced {
			if let Some(ref sink) = self.dropped_by_limits_sink {
				if let Err(e) = sink.unbounded_send((tx.clone(), TransactionStatus::Dropped)) {
					trace!(target: LOG_TARGET, "[{:?}] dropped_sink/future: send message failed: {:?}", tx, e);
				}
			}
		}
	}

	/// Transaction was removed as invalid.
	pub fn invalid(&mut self, tx: &H) {
		trace!(target: LOG_TARGET, "[{:?}] Extrinsic invalid", tx);
		self.fire(tx, |watcher| watcher.invalid());
	}

	/// Transaction was pruned from the pool.
	pub fn pruned(&mut self, block_hash: BlockHash<C>, tx: &H) {
		trace!(target: LOG_TARGET, "[{:?}] Pruned at {:?}", tx, block_hash);
		// Get the transactions included in the given block hash.
		let txs = self.finality_watchers.entry(block_hash).or_insert(vec![]);
		txs.push(tx.clone());
		// Current transaction is the last one included.
		let tx_index = txs.len() - 1;

		self.fire(tx, |watcher| watcher.in_block(block_hash, tx_index));

		while self.finality_watchers.len() > MAX_FINALITY_WATCHERS {
			if let Some((hash, txs)) = self.finality_watchers.pop_front() {
				for tx in txs {
					self.fire(&tx, |watcher| watcher.finality_timeout(hash));
				}
			}
		}
	}

	/// The block this transaction was included in has been retracted.
	pub fn retracted(&mut self, block_hash: BlockHash<C>) {
		if let Some(hashes) = self.finality_watchers.remove(&block_hash) {
			for hash in hashes {
				self.fire(&hash, |watcher| watcher.retracted(block_hash))
			}
		}
	}

	/// Notify all watchers that transactions have been finalized
	pub fn finalized(&mut self, block_hash: BlockHash<C>) {
		if let Some(hashes) = self.finality_watchers.remove(&block_hash) {
			for (tx_index, hash) in hashes.into_iter().enumerate() {
				log::trace!(
					target: LOG_TARGET,
					"[{:?}] Sent finalization event (block {:?})",
					hash,
					block_hash,
				);
				self.fire(&hash, |watcher| watcher.finalized(block_hash, tx_index))
			}
		}
	}

	/// Provides hashes of all watched transactions.
	pub fn watched_transactions(&self) -> impl Iterator<Item = &H> {
		self.watchers.keys()
	}
}
