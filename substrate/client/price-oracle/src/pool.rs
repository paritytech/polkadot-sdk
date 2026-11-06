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

//! The reports collected from the oracle nodes.

use parking_lot::RwLock;
use sp_price_oracle::{Anchor, SignedPriceReport};
use std::{collections::BTreeMap, sync::Arc};

/// The anchors a report may have to count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
	/// The oldest block accepted.
	pub oldest_block: u32,
	/// The newest block accepted. `None` accepts any.
	pub newest_block: Option<u32>,
	/// The oldest timestamp accepted, in milliseconds since the Unix epoch.
	pub oldest_timestamp_ms: u64,
}

impl Window {
	/// Whether `anchor` is within the window.
	pub fn contains(&self, anchor: &Anchor) -> bool {
		anchor.block >= self.oldest_block &&
			self.newest_block.map_or(true, |newest| anchor.block <= newest) &&
			anchor.timestamp_ms >= self.oldest_timestamp_ms
	}
}

/// Reports collected from the oracle nodes, the latest one per signer.
///
/// Reports enter the pool from two sides: the ones signed by this node, and the ones received
/// from peers and passed the acceptance rules. A report replaces the signer's previous one
/// unless the previous one has a greater anchor; at equal anchors the one inserted last wins, as
/// nothing tells which of two reports with the same anchor is fresher. Anchors compare by block,
/// then by timestamp. Reports leave the pool only by pruning, once their block or timestamp falls
/// out of the report windows. The block author reads the pool through [`select`](Self::select)
/// without removing anything, so a report stays available to the next block if the current one is
/// not included.
///
/// The pool holds no cryptographic or membership judgement of its own. Whoever inserts a report
/// is responsible for having verified it.
///
/// Cloning yields another handle to the same pool. It is safe to use from several tasks.
#[derive(Clone)]
pub struct ReportPool<Id, Signature> {
	inner: Arc<RwLock<BTreeMap<Id, SignedPriceReport<Id, Signature>>>>,
}

impl<Id: Ord + Clone, Signature: Clone> ReportPool<Id, Signature> {
	/// Create an empty pool.
	pub fn new() -> Self {
		Self { inner: Arc::new(RwLock::new(BTreeMap::new())) }
	}

	/// Store `report` unless the pool holds a report of the same signer with a greater anchor.
	/// Returns whether the pool changed.
	pub fn insert(&self, report: SignedPriceReport<Id, Signature>) -> bool {
		let mut pool = self.inner.write();
		match pool.get(&report.signer) {
			Some(held) if held.report.anchor > report.report.anchor => false,
			_ => {
				pool.insert(report.signer.clone(), report);
				true
			},
		}
	}

	/// Drop reports anchored outside `window`.
	pub fn prune(&self, window: &Window) {
		self.inner.write().retain(|_, r| window.contains(&r.report.anchor));
	}

	/// The reports to include in a block: all held reports anchored within `window`, except those
	/// anchored before the signer's vote already on chain, as given by `on_chain`. Nothing is
	/// removed from the pool.
	pub fn select(
		&self,
		on_chain: &[(Id, Anchor)],
		window: &Window,
	) -> Vec<SignedPriceReport<Id, Signature>> {
		let on_chain: BTreeMap<&Id, Anchor> = on_chain.iter().map(|(id, a)| (id, *a)).collect();
		let pool = self.inner.read();
		pool.values()
			.filter(|r| window.contains(&r.report.anchor))
			.filter(|r| on_chain.get(&r.signer).map_or(true, |a| r.report.anchor >= *a))
			.cloned()
			.collect()
	}

	/// The number of reports held.
	pub fn len(&self) -> usize {
		self.inner.read().len()
	}

	/// Whether the pool holds no reports.
	pub fn is_empty(&self) -> bool {
		self.inner.read().is_empty()
	}
}

impl<Id: Ord + Clone, Signature: Clone> Default for ReportPool<Id, Signature> {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_price_oracle::PriceReport;

	type Pool = ReportPool<u8, ()>;

	fn report_at(signer: u8, block: u32, timestamp_ms: u64) -> SignedPriceReport<u8, ()> {
		SignedPriceReport {
			report: PriceReport { anchor: Anchor::new(block, timestamp_ms), quotes: vec![] },
			signer,
			signature: (),
		}
	}
	fn report(signer: u8, block: u32) -> SignedPriceReport<u8, ()> {
		report_at(signer, block, 1_000)
	}

	fn window(oldest_block: u32, newest_block: Option<u32>, oldest_timestamp_ms: u64) -> Window {
		Window { oldest_block, newest_block, oldest_timestamp_ms }
	}

	fn anchors(reports: &[SignedPriceReport<u8, ()>]) -> Vec<(u8, u32)> {
		reports.iter().map(|r| (r.signer, r.report.anchor.block)).collect()
	}
	fn all(pool: &Pool) -> Vec<SignedPriceReport<u8, ()>> {
		pool.select(&[], &window(0, None, 0))
	}

	#[test]
	fn insert_keeps_the_latest_per_signer() {
		let pool = Pool::new();
		assert!(pool.insert(report(1, 10)));
		assert!(pool.insert(report(1, 11)), "greater block replaces");
		assert!(pool.insert(report(1, 11)), "equal anchor replaces: last wins");
		assert!(!pool.insert(report(1, 10)), "smaller block is dropped");
		assert!(pool.insert(report_at(1, 11, 2_000)), "same block, later timestamp replaces");
		assert!(!pool.insert(report_at(1, 11, 1_500)), "same block, earlier timestamp is dropped");
		assert!(pool.insert(report_at(1, 12, 500)), "later block replaces, whatever the timestamp");
		assert!(pool.insert(report(2, 5)));
		assert_eq!(pool.len(), 2);
		assert_eq!(anchors(&all(&pool)), vec![(1, 12), (2, 5)]);
	}

	#[test]
	fn prune_drops_reports_out_of_either_window() {
		let pool = Pool::new();
		pool.insert(report_at(1, 10, 1_000)); // block too old
		pool.insert(report_at(2, 11, 1_000));
		pool.insert(report_at(3, 12, 999)); // timestamp too old
		pool.insert(report_at(4, 12, 5_000));
		pool.prune(&window(11, None, 1_000));
		assert_eq!(anchors(&all(&pool)), vec![(2, 11), (4, 12)]);
	}

	#[test]
	fn select_skips_stale_and_already_on_chain() {
		let pool = Pool::new();
		pool.insert(report(1, 10)); // older than the vote on chain: skipped
		pool.insert(report(2, 11)); // same anchor as on chain: included, last wins
		pool.insert(report(3, 12)); // newer than on chain: included
		pool.insert(report(4, 12)); // not on chain: included
		pool.insert(report(5, 3)); // before the oldest block: skipped
		pool.insert(report(6, 13)); // after the newest block: skipped
		pool.insert(report_at(7, 12, 999)); // before the oldest timestamp: skipped
		let on_chain =
			[(1, Anchor::new(11, 1_000)), (2, Anchor::new(11, 1_000)), (3, Anchor::new(11, 1_000))];
		assert_eq!(
			anchors(&pool.select(&on_chain, &window(5, Some(12), 1_000))),
			vec![(2, 11), (3, 12), (4, 12)]
		);
		assert_eq!(pool.len(), 7, "select removes nothing");
	}

	#[test]
	fn clones_share_the_pool() {
		let pool = Pool::new();
		let other = pool.clone();
		other.insert(report(1, 1));
		assert_eq!(pool.len(), 1);
		assert!(!pool.is_empty());
	}
}
