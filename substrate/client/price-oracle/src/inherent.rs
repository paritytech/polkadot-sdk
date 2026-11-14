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
//! Inherent data provider carrying the pooled reports into a block.

use crate::{
	pool::{ReportPool, Window},
	tick::now_ms,
	LOG_TARGET,
};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_price_oracle::{
	inherents::InherentDataProvider, runtime_api::PriceOracleApi, SignedPriceReport,
};
use sp_runtime::{
	traits::{Block as BlockT, Header, SaturatedConversion},
	RuntimeAppPublic,
};

/// Constructor of the price oracle inherent data provider.
pub struct PriceOracleInherentDataProvider;

impl PriceOracleInherentDataProvider {
	/// The inherent data provider for the block built on `parent`.
	///
	/// Selects from `pool` the reports the runtime accepts in that block, with
	/// `current = parent + 1`:
	/// - `current - report_window <= anchor.block <= current`,
	/// - `anchor >= latest anchor of the signer on chain`,
	/// and that are fresh by this node's clock: `now - report_window_ms <= anchor.timestamp_ms`.
	/// The runtime measures timestamps against the chain's time, which freezes while the chain
	/// stalls; this node's clock does not, so an honest author leaves out reports the runtime would
	/// still accept after a stall.
	///
	/// Nothing is selected if the header of `parent` is unavailable or the runtime at `parent`
	/// does not implement the price oracle API.
	pub fn create<Block, Client, Id, Signature>(
		client: &Client,
		pool: &ReportPool<Id, Signature>,
		parent: Block::Hash,
	) -> InherentDataProvider<Id, Signature>
	where
		Block: BlockT,
		Client: ProvideRuntimeApi<Block> + HeaderBackend<Block>,
		Client::Api: PriceOracleApi<Block, Id>,
		Id: RuntimeAppPublic<Signature = Signature> + Ord + Clone + codec::Decode,
		Signature: Clone,
	{
		let reports =
			Self::select::<Block, Client, Id, Signature>(client, pool, parent).unwrap_or_default();
		log::debug!(
			target: LOG_TARGET,
			"Providing {} reports for the block on {parent:?}",
			reports.len(),
		);
		InherentDataProvider::new(reports)
	}

	fn select<Block, Client, Id, Signature>(
		client: &Client,
		pool: &ReportPool<Id, Signature>,
		parent: Block::Hash,
	) -> Option<Vec<SignedPriceReport<Id, Signature>>>
	where
		Block: BlockT,
		Client: ProvideRuntimeApi<Block> + HeaderBackend<Block>,
		Client::Api: PriceOracleApi<Block, Id>,
		Id: RuntimeAppPublic<Signature = Signature> + Ord + Clone + codec::Decode,
		Signature: Clone,
	{
		let header = match client.header(parent) {
			Ok(Some(header)) => header,
			Ok(None) => {
				log::debug!(
					target: LOG_TARGET,
					"Header of {parent:?} not found, providing no reports",
				);
				return None;
			},
			Err(e) => {
				log::debug!(
					target: LOG_TARGET,
					"Header of {parent:?} unavailable ({e}), providing no reports",
				);
				return None;
			},
		};
		let parent_number: u32 = (*header.number()).saturated_into();

		let api = client.runtime_api();
		let (settings, on_chain) = match (api.settings(parent), api.latest_anchors(parent)) {
			(Ok(settings), Ok(on_chain)) => (settings, on_chain),
			(Err(e), _) | (_, Err(e)) => {
				log::debug!(
					target: LOG_TARGET,
					"Price oracle API unavailable at {parent:?} ({e}), providing no reports",
				);
				return None;
			},
		};
		let (oldest_block, newest_block) = block_bounds(parent_number, settings.report_window);
		let window = Window {
			oldest_block,
			newest_block: Some(newest_block),
			oldest_timestamp_ms: now_ms().saturating_sub(settings.report_window_ms),
		};
		Some(pool.select(&on_chain, &window))
	}
}

/// The oldest and newest block the runtime accepts in the block built on `parent_number`:
/// `current - window` and `current`, with `current = parent_number + 1`.
fn block_bounds(parent_number: u32, window: u32) -> (u32, u32) {
	let current = parent_number.saturating_add(1);
	(current.saturating_sub(window), current)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn bounds_match_the_runtime_window() {
		// The runtime accepts `current - anchor <= window` with `current = parent + 1`.
		let accepted = |block: u32, parent: u32, window: u32| {
			let current = parent + 1;
			block <= current && current - block <= window
		};
		for (parent, window) in [(100, 10), (100, 1), (0, 5), (5, 0)] {
			let (oldest, newest) = block_bounds(parent, window);
			// Every block within the bounds is accepted by the runtime, and the ones just outside
			// are not.
			for block in oldest..=newest {
				assert!(accepted(block, parent, window), "{block} at {parent}/{window}");
			}
			if oldest > 0 {
				assert!(!accepted(oldest - 1, parent, window));
			}
			assert!(!accepted(newest + 1, parent, window));
		}
		assert_eq!(block_bounds(100, 10), (91, 101));
		// A zero window, as served while the parameters are unset, takes only the current block,
		// which the runtime accepts as well.
		assert_eq!(block_bounds(5, 0), (6, 6));
		// No overflow at the top of the range.
		assert_eq!(block_bounds(u32::MAX, 1).1, u32::MAX);
	}
}
