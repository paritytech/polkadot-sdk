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

/// Reports collected from the oracle nodes, the latest one per signer.
///
/// Reports enter the pool from two sides: the ones signed by this node, and the ones received
/// from peers and accepted by the gossip validator. A report replaces the signer's previous one
/// unless the previous one has a greater anchor; at equal anchors the one inserted last wins, as
/// nothing tells which of two reports with the same anchor is fresher. Reports leave the pool only
/// by pruning, once their anchor falls out of the window the runtime accepts. The block author
/// reads the pool through [`select`](Self::select) without removing anything, so a report stays
/// available to the next block if the current one is not included.
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

	/// Drop reports anchored before `oldest`.
	pub fn prune(&self, oldest: Anchor) {
		self.inner.write().retain(|_, r| r.report.anchor >= oldest);
	}

	/// The reports to include in a block: all held reports anchored within `oldest..=newest`,
	/// except those anchored before the signer's vote already on chain, as given by `on_chain`.
	/// Nothing is removed from the pool.
	pub fn select(
		&self,
		on_chain: &[(Id, Anchor)],
		oldest: Anchor,
		newest: Anchor,
	) -> Vec<SignedPriceReport<Id, Signature>> {
		let on_chain: BTreeMap<&Id, Anchor> = on_chain.iter().map(|(id, a)| (id, *a)).collect();
		let pool = self.inner.read();
		pool.values()
			.filter(|r| r.report.anchor >= oldest && r.report.anchor <= newest)
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

	fn report(signer: u8, anchor: u32) -> SignedPriceReport<u8, ()> {
		SignedPriceReport {
			report: PriceReport { anchor: Anchor(anchor), quotes: vec![] },
			signer,
			signature: (),
		}
	}

	fn anchors(reports: &[SignedPriceReport<u8, ()>]) -> Vec<(u8, u32)> {
		reports.iter().map(|r| (r.signer, r.report.anchor.0)).collect()
	}

	#[test]
	fn insert_keeps_the_latest_per_signer() {
		let pool = Pool::new();
		assert!(pool.insert(report(1, 10)));
		assert!(pool.insert(report(1, 11)), "greater anchor replaces");
		assert!(pool.insert(report(1, 11)), "equal anchor replaces: last wins");
		assert!(!pool.insert(report(1, 10)), "smaller anchor is dropped");
		assert!(pool.insert(report(2, 5)));
		assert_eq!(pool.len(), 2);
		assert_eq!(anchors(&pool.select(&[], Anchor(0), Anchor(u32::MAX))), vec![(1, 11), (2, 5)]);
	}

	#[test]
	fn prune_drops_reports_before_oldest() {
		let pool = Pool::new();
		pool.insert(report(1, 10));
		pool.insert(report(2, 11));
		pool.insert(report(3, 12));
		pool.prune(Anchor(11));
		assert_eq!(anchors(&pool.select(&[], Anchor(0), Anchor(u32::MAX))), vec![(2, 11), (3, 12)]);
	}

	#[test]
	fn select_skips_stale_and_already_on_chain() {
		let pool = Pool::new();
		pool.insert(report(1, 10)); // older than the vote on chain: skipped
		pool.insert(report(2, 11)); // same anchor as on chain: included, last wins
		pool.insert(report(3, 12)); // newer than on chain: included
		pool.insert(report(4, 12)); // not on chain: included
		pool.insert(report(5, 3)); // before `oldest`: skipped
		pool.insert(report(6, 13)); // after `newest`: skipped
		let on_chain = [(1, Anchor(11)), (2, Anchor(11)), (3, Anchor(11))];
		assert_eq!(
			anchors(&pool.select(&on_chain, Anchor(5), Anchor(12))),
			vec![(2, 11), (3, 12), (4, 12)]
		);
		assert_eq!(pool.len(), 6, "select removes nothing");
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
