// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! Runtime upgrade detection: nothing may build on a block applying a runtime upgrade until that
//! block is included.

use cumulus_client_consensus_common::ParentSearchResult;
use sp_runtime::{
	traits::{Block as BlockT, Header as HeaderT},
	DigestItem,
};

/// Whether `header` applies a runtime upgrade.
pub(crate) fn has_runtime_upgrade_digest<H: HeaderT>(header: &H) -> bool {
	header
		.digest()
		.logs
		.iter()
		.any(|it| matches!(it, DigestItem::RuntimeEnvironmentUpdated))
}

/// The oldest unincluded block applying a runtime upgrade, between the included head and the
/// parent search's build parent.
pub(crate) fn pending_runtime_upgrade<Block: BlockT>(
	search: &ParentSearchResult<Block>,
	get_header: impl Fn(Block::Hash) -> Option<Block::Header>,
) -> Option<Block::Header> {
	let included_number = *search.included_at_scheduling.number();
	let mut pending = None;
	let mut current = search.best_parent_header.clone();
	while *current.number() > included_number {
		if has_runtime_upgrade_digest(&current) {
			pending = Some(current.clone());
		}
		let Some(parent) = get_header(*current.parent_hash()) else { break };
		current = parent;
	}
	pending
}

#[cfg(test)]
mod tests {
	use super::*;
	use cumulus_test_client::runtime::Block as TestBlock;
	use std::collections::HashMap;

	type TestHeader = <TestBlock as BlockT>::Header;

	/// A chain of headers numbered `0..=len`, with the runtime upgrade digest at `upgrades`.
	fn chain(len: u32, upgrades: &[u32]) -> Vec<TestHeader> {
		let mut headers: Vec<TestHeader> = Vec::new();
		for number in 0..=len {
			let parent_hash = headers.last().map(|h| h.hash()).unwrap_or_default();
			let mut header = TestHeader::new(
				number,
				Default::default(),
				Default::default(),
				parent_hash,
				Default::default(),
			);
			if upgrades.contains(&number) {
				header.digest_mut().push(DigestItem::RuntimeEnvironmentUpdated);
			}
			headers.push(header);
		}
		headers
	}

	/// The pending upgrade's number for a search from `included` to `best` over `headers`.
	fn pending_number(headers: &[TestHeader], included: u32, best: u32) -> Option<u32> {
		let by_hash: HashMap<_, _> = headers.iter().map(|h| (h.hash(), h.clone())).collect();
		let search = ParentSearchResult::<TestBlock> {
			included_at_scheduling: headers[included as usize].clone(),
			best_parent_header: headers[best as usize].clone(),
			resubmittable_segment: Vec::new(),
		};
		pending_runtime_upgrade(&search, |hash| by_hash.get(&hash).cloned()).map(|h| *h.number())
	}

	#[test]
	fn no_upgrade_means_none_pending() {
		assert_eq!(pending_number(&chain(5, &[]), 1, 5), None);
	}

	#[test]
	fn unincluded_upgrade_is_pending() {
		let headers = chain(5, &[3]);
		assert_eq!(pending_number(&headers, 1, 5), Some(3));
		assert_eq!(pending_number(&headers, 1, 3), Some(3));
	}

	#[test]
	fn the_oldest_unincluded_upgrade_is_pending() {
		assert_eq!(pending_number(&chain(6, &[3, 5]), 1, 6), Some(3));
	}

	#[test]
	fn included_upgrade_is_not_pending() {
		let headers = chain(5, &[2]);
		assert_eq!(pending_number(&headers, 2, 5), None);
		assert_eq!(pending_number(&headers, 3, 5), None);
	}
}
