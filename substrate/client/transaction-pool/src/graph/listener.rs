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

/// Single event used in aggregated stream. Tuple containing hash of transactions and its status.
pub type TransactionStatusEvent<H, BH> = (H, TransactionStatus<H, BH>);
/// Stream of events providing statuses of all the transactions within the pool.
pub type AggregatedStream<H, BH> = TracingUnboundedReceiver<TransactionStatusEvent<H, BH>>;

/// Warning threshold for (unbounded) channel used in aggregated stream.
const AGGREGATED_STREAM_WARN_THRESHOLD: usize = 100_000;

/// Extrinsic pool default listener.
pub struct Listener<H: hash::Hash + Eq, C: ChainApi> {
	/// Map containing per-transaction sinks for emitting transaction status events.
	watchers: HashMap<H, watcher::Sender<H, BlockHash<C>>>,
	finality_watchers: LinkedHashMap<ExtrinsicHash<C>, Vec<H>>,

	/// The sink used to notify dropped by enforcing limits or by being usurped, or invalid
	/// transactions.
	///
	/// Note: Ready and future statuses are alse communicated through this channel, enabling the
	/// stream consumer to track views that reference the transaction.
	dropped_stream_sink: Option<TracingUnboundedSender<TransactionStatusEvent<H, BlockHash<C>>>>,

	/// The sink of the single, merged stream providing updates for all the transactions in the
	/// associated pool.
	aggregated_stream_sink: Option<TracingUnboundedSender<TransactionStatusEvent<H, BlockHash<C>>>>,
}

/// Maximum number of blocks awaiting finality at any time.
const MAX_FINALITY_WATCHERS: usize = 512;

impl<H: hash::Hash + Eq + Debug, C: ChainApi> Default for Listener<H, C> {
	fn default() -> Self {
		Self {
			watchers: Default::default(),
			finality_watchers: Default::default(),
			dropped_stream_sink: None,
			aggregated_stream_sink: None,
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

	/// Creates a new single stream intended to watch dropped transactions only.
	///
	/// The stream can be used to subscribe to events related to dropping of all extrinsics in the
	/// pool.
	pub fn create_dropped_by_limits_stream(&mut self) -> AggregatedStream<H, BlockHash<C>> {
		let (sender, single_stream) =
			tracing_unbounded("mpsc_txpool_watcher", AGGREGATED_STREAM_WARN_THRESHOLD);
		self.dropped_stream_sink = Some(sender);
		single_stream
	}

	/// Creates a new single merged stream for all extrinsics in the associated pool.
	///
	/// The stream can be used to subscribe to life-cycle events of all extrinsics in the pool. For
	/// some implementations (e.g. fork-aware pool) this approach may be more efficient than using
	/// individual streams for every transaction.
	///
	/// Note: some of the events which are currently ignored on the other side of this channel
	/// (external watcher) are not sent.
	pub fn create_aggregated_stream(&mut self) -> AggregatedStream<H, BlockHash<C>> {
		let (sender, aggregated_stream) =
			tracing_unbounded("mpsc_txpool_aggregated_stream", AGGREGATED_STREAM_WARN_THRESHOLD);
		self.aggregated_stream_sink = Some(sender);
		aggregated_stream
	}

	/// Notify the listeners about the extrinsic broadcast.
	pub fn broadcasted(&mut self, hash: &H, peers: Vec<String>) {
		trace!(target: LOG_TARGET, "[{:?}] Broadcasted", hash);
		self.fire(hash, |watcher| watcher.broadcast(peers));
	}

	/// Sends given event to the `dropped_stream_sink`.
	fn send_to_dropped_stream_sink(&mut self, tx: &H, status: TransactionStatus<H, BlockHash<C>>) {
		if let Some(ref sink) = self.dropped_stream_sink {
			if let Err(e) = sink.unbounded_send((tx.clone(), status.clone())) {
				trace!(target: LOG_TARGET, "[{:?}] dropped_sink: {:?} send message failed: {:?}", tx, status, e);
			}
		}
	}

	/// Sends given event to the `aggregated_stream_sink`.
	fn send_to_aggregated_stream_sink(
		&mut self,
		tx: &H,
		status: TransactionStatus<H, BlockHash<C>>,
	) {
		if let Some(ref sink) = self.aggregated_stream_sink {
			if let Err(e) = sink.unbounded_send((tx.clone(), status.clone())) {
				trace!(target: LOG_TARGET, "[{:?}] aggregated_stream {:?} send message failed: {:?}", tx, status, e);
			}
		}
	}

	/// New transaction was added to the ready pool or promoted from the future pool.
	pub fn ready(&mut self, tx: &H, old: Option<&H>) {
		trace!(target: LOG_TARGET, "[{:?}] Ready (replaced with {:?})", tx, old);
		self.fire(tx, |watcher| watcher.ready());
		if let Some(old) = old {
			self.fire(old, |watcher| watcher.usurped(tx.clone()));
		}

		self.send_to_dropped_stream_sink(tx, TransactionStatus::Ready);
		self.send_to_aggregated_stream_sink(tx, TransactionStatus::Ready);
	}

	/// New transaction was added to the future pool.
	pub fn future(&mut self, tx: &H) {
		trace!(target: LOG_TARGET, "[{:?}] Future", tx);
		self.fire(tx, |watcher| watcher.future());

		self.send_to_dropped_stream_sink(tx, TransactionStatus::Future);
		self.send_to_aggregated_stream_sink(tx, TransactionStatus::Future);
	}

	/// Transaction was dropped from the pool because of enforcing the limit.
	pub fn limits_enforced(&mut self, tx: &H) {
		trace!(target: LOG_TARGET, "[{:?}] Dropped (limits enforced)", tx);
		self.fire(tx, |watcher| watcher.limit_enforced());

		self.send_to_dropped_stream_sink(tx, TransactionStatus::Dropped);
	}

	/// Transaction was replaced with other extrinsic.
	pub fn usurped(&mut self, tx: &H, by: &H) {
		trace!(target: LOG_TARGET, "[{:?}] Dropped (replaced with {:?})", tx, by);
		self.fire(tx, |watcher| watcher.usurped(by.clone()));

		self.send_to_dropped_stream_sink(tx, TransactionStatus::Usurped(by.clone()));
	}

	/// Transaction was dropped from the pool because of the failure during the resubmission of
	/// revalidate transactions or failure during pruning tags.
	pub fn dropped(&mut self, tx: &H) {
		trace!(target: LOG_TARGET, "[{:?}] Dropped", tx);
		self.fire(tx, |watcher| watcher.dropped());
	}

	/// Transaction was removed as invalid.
	pub fn invalid(&mut self, tx: &H) {
		trace!(target: LOG_TARGET, "[{:?}] Extrinsic invalid", tx);
		self.fire(tx, |watcher| watcher.invalid());

		self.send_to_dropped_stream_sink(tx, TransactionStatus::Invalid);
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
		self.send_to_aggregated_stream_sink(tx, TransactionStatus::InBlock((block_hash, tx_index)));

		while self.finality_watchers.len() > MAX_FINALITY_WATCHERS {
			if let Some((hash, txs)) = self.finality_watchers.pop_front() {
				for tx in txs {
					self.fire(&tx, |watcher| watcher.finality_timeout(hash));
					//todo: do we need this? [related issue: #5482]
					self.send_to_aggregated_stream_sink(
						&tx,
						TransactionStatus::FinalityTimeout(hash),
					);
				}
			}
		}
	}

	/// The block this transaction was included in has been retracted.
	pub fn retracted(&mut self, block_hash: BlockHash<C>) {
		if let Some(hashes) = self.finality_watchers.remove(&block_hash) {
			for hash in hashes {
				self.fire(&hash, |watcher| watcher.retracted(block_hash));
				// note: [#5479], we do not send to aggregated stream.
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
