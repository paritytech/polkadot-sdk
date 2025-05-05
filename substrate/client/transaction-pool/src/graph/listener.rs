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
use tracing::trace;

use super::{watcher, BlockHash, ChainApi, ExtrinsicHash};

static LOG_TARGET: &str = "txpool::watcher";

/// The `EventHandler` trait provides a mechanism for clients to respond to various
/// transaction-related events. It offers a set of callback methods that are invoked by the
/// transaction pool's event dispatcher to notify about changes in the status of transactions.
///
/// This trait can be implemented by any component that needs to respond to transaction lifecycle
/// events, enabling custom logic and handling of these events.
pub trait EventHandler<C: ChainApi> {
	/// Called when a transaction is broadcasted.
	fn broadcasted(&self, _hash: ExtrinsicHash<C>, _peers: Vec<String>) {}

	/// Called when a transaction is ready for execution.
	fn ready(&self, _tx: ExtrinsicHash<C>) {}

	/// Called when a transaction is deemed to be executable in the future.
	fn future(&self, _tx: ExtrinsicHash<C>) {}

	/// Called when transaction pool limits result in a transaction being affected.
	fn limits_enforced(&self, _tx: ExtrinsicHash<C>) {}

	/// Called when a transaction is replaced by another.
	fn usurped(&self, _tx: ExtrinsicHash<C>, _by: ExtrinsicHash<C>) {}

	/// Called when a transaction is dropped from the pool.
	fn dropped(&self, _tx: ExtrinsicHash<C>) {}

	/// Called when a transaction is found to be invalid.
	fn invalid(&self, _tx: ExtrinsicHash<C>) {}

	/// Called when a transaction was pruned from the pool due to its presence in imported block.
	fn pruned(&self, _tx: ExtrinsicHash<C>, _block_hash: BlockHash<C>, _tx_index: usize) {}

	/// Called when a transaction is retracted from inclusion in a block.
	fn retracted(&self, _tx: ExtrinsicHash<C>, _block_hash: BlockHash<C>) {}

	/// Called when a transaction has not been finalized within a timeout period.
	fn finality_timeout(&self, _tx: ExtrinsicHash<C>, _hash: BlockHash<C>) {}

	/// Called when a transaction is finalized in a block.
	fn finalized(&self, _tx: ExtrinsicHash<C>, _block_hash: BlockHash<C>, _tx_index: usize) {}
}

impl<C: ChainApi> EventHandler<C> for () {}

/// The `EventDispatcher` struct is responsible for dispatching transaction-related events from the
/// validated pool to interested observers and an optional event handler. It acts as the primary
/// liaison between the transaction pool and clients that are monitoring transaction statuses.
pub struct EventDispatcher<H: hash::Hash + Eq, C: ChainApi, L: EventHandler<C>> {
	/// Map containing per-transaction sinks for emitting transaction status events.
	watchers: HashMap<H, watcher::Sender<H, BlockHash<C>>>,
	finality_watchers: LinkedHashMap<ExtrinsicHash<C>, Vec<H>>,

	/// Optional event handler (listener) that will be notified about all transactions status
	/// changes from the pool.
	event_handler: Option<L>,
}

/// Maximum number of blocks awaiting finality at any time.
const MAX_FINALITY_WATCHERS: usize = 512;

impl<H: hash::Hash + Eq + Debug, C: ChainApi, L: EventHandler<C>> Default
	for EventDispatcher<H, C, L>
{
	fn default() -> Self {
		Self {
			watchers: Default::default(),
			finality_watchers: Default::default(),
			event_handler: None,
		}
	}
}

impl<C: ChainApi, L: EventHandler<C>> EventDispatcher<ExtrinsicHash<C>, C, L> {
	/// Creates a new instance with provided event handler.
	pub fn new_with_event_handler(event_handler: Option<L>) -> Self {
		Self { event_handler, ..Default::default() }
	}

	fn fire<F>(&mut self, hash: &ExtrinsicHash<C>, fun: F)
	where
		F: FnOnce(&mut watcher::Sender<ExtrinsicHash<C>, ExtrinsicHash<C>>),
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
	pub fn create_watcher(
		&mut self,
		hash: ExtrinsicHash<C>,
	) -> watcher::Watcher<ExtrinsicHash<C>, ExtrinsicHash<C>> {
		let sender = self.watchers.entry(hash).or_insert_with(watcher::Sender::default);
		sender.new_watcher(hash)
	}

	/// Notify the listeners about the extrinsic broadcast.
	pub fn broadcasted(&mut self, tx_hash: &ExtrinsicHash<C>, peers: Vec<String>) {
		trace!(
			target: LOG_TARGET,
			?tx_hash,
			"Broadcasted."
		);
		self.fire(tx_hash, |watcher| watcher.broadcast(peers.clone()));
		self.event_handler.as_ref().map(|l| l.broadcasted(*tx_hash, peers));
	}

	/// New transaction was added to the ready pool or promoted from the future pool.
	pub fn ready(&mut self, tx: &ExtrinsicHash<C>, old: Option<&ExtrinsicHash<C>>) {
		trace!(
			target: LOG_TARGET,
			tx_hash = ?*tx,
			replaced_with = ?old,
			"Ready."
		);
		self.fire(tx, |watcher| watcher.ready());
		if let Some(old) = old {
			self.fire(old, |watcher| watcher.usurped(*tx));
		}

		self.event_handler.as_ref().map(|l| l.ready(*tx));
	}

	/// New transaction was added to the future pool.
	pub fn future(&mut self, tx_hash: &ExtrinsicHash<C>) {
		trace!(
			target: LOG_TARGET,
			?tx_hash,
			"Future."
		);
		self.fire(tx_hash, |watcher| watcher.future());

		self.event_handler.as_ref().map(|l| l.future(*tx_hash));
	}

	/// Transaction was dropped from the pool because of enforcing the limit.
	pub fn limits_enforced(&mut self, tx_hash: &ExtrinsicHash<C>) {
		trace!(
			target: LOG_TARGET,
			?tx_hash,
			"Dropped (limits enforced)."
		);
		self.fire(tx_hash, |watcher| watcher.limit_enforced());

		self.event_handler.as_ref().map(|l| l.limits_enforced(*tx_hash));
	}

	/// Transaction was replaced with other extrinsic.
	pub fn usurped(&mut self, tx: &ExtrinsicHash<C>, by: &ExtrinsicHash<C>) {
		trace!(
			target: LOG_TARGET,
			tx_hash = ?tx,
			?by,
			"Dropped (replaced)."
		);
		self.fire(tx, |watcher| watcher.usurped(*by));

		self.event_handler.as_ref().map(|l| l.usurped(*tx, *by));
	}

	/// Transaction was dropped from the pool because of the failure during the resubmission of
	/// revalidate transactions or failure during pruning tags.
	pub fn dropped(&mut self, tx_hash: &ExtrinsicHash<C>) {
		trace!(
			target: LOG_TARGET,
			   ?tx_hash,
			"Dropped."
		);
		self.fire(tx_hash, |watcher| watcher.dropped());
		self.event_handler.as_ref().map(|l| l.dropped(*tx_hash));
	}

	/// Transaction was removed as invalid.
	pub fn invalid(&mut self, tx_hash: &ExtrinsicHash<C>) {
		trace!(
			target: LOG_TARGET,
			?tx_hash,
			"Extrinsic invalid."
		);
		self.fire(tx_hash, |watcher| watcher.invalid());
		self.event_handler.as_ref().map(|l| l.invalid(*tx_hash));
	}

	/// Transaction was pruned from the pool.
	pub fn pruned(&mut self, block_hash: BlockHash<C>, tx_hash: &ExtrinsicHash<C>) {
		trace!(
			target: LOG_TARGET,
			?tx_hash,
			?block_hash,
			"Pruned at."
		);
		// Get the transactions included in the given block hash.
		let txs = self.finality_watchers.entry(block_hash).or_insert(vec![]);
		txs.push(*tx_hash);
		// Current transaction is the last one included.
		let tx_index = txs.len() - 1;

		self.fire(tx_hash, |watcher| watcher.in_block(block_hash, tx_index));
		self.event_handler.as_ref().map(|l| l.pruned(*tx_hash, block_hash, tx_index));

		while self.finality_watchers.len() > MAX_FINALITY_WATCHERS {
			if let Some((hash, txs)) = self.finality_watchers.pop_front() {
				for tx in txs {
					self.fire(&tx, |watcher| watcher.finality_timeout(hash));
					self.event_handler.as_ref().map(|l| l.finality_timeout(tx, block_hash));
				}
			}
		}
	}

	/// The block this transaction was included in has been retracted.
	pub fn retracted(&mut self, block_hash: BlockHash<C>) {
		if let Some(hashes) = self.finality_watchers.remove(&block_hash) {
			for hash in hashes {
				self.fire(&hash, |watcher| watcher.retracted(block_hash));
				self.event_handler.as_ref().map(|l| l.retracted(hash, block_hash));
			}
		}
	}

	/// Notify all watchers that transactions have been finalized
	pub fn finalized(&mut self, block_hash: BlockHash<C>) {
		if let Some(hashes) = self.finality_watchers.remove(&block_hash) {
			for (tx_index, tx_hash) in hashes.into_iter().enumerate() {
				trace!(
					target: LOG_TARGET,
					?tx_hash,
					?block_hash,
					"Sent finalization event."
				);
				self.fire(&tx_hash, |watcher| watcher.finalized(block_hash, tx_index));
				self.event_handler.as_ref().map(|l| l.finalized(tx_hash, block_hash, tx_index));
			}
		}
	}

	/// Provides hashes of all watched transactions.
	pub fn watched_transactions(&self) -> impl Iterator<Item = &ExtrinsicHash<C>> {
		self.watchers.keys()
	}
}
