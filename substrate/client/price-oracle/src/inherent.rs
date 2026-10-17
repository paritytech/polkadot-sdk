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

use crate::pool::ReportPool;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_price_oracle::{
	inherents::InherentDataProvider, runtime_api::PriceOracleApi, Anchor, SignedPriceReport,
};
use sp_runtime::{
	traits::{Block as BlockT, Header, SaturatedConversion},
	RuntimeAppPublic,
};

const LOG_TARGET: &str = "price-oracle";

/// Constructor of the price oracle inherent data provider.
pub struct PriceOracleInherentDataProvider;

impl PriceOracleInherentDataProvider {
	/// The inherent data provider for the block built on `parent`.
	///
	/// Selects from `pool` the reports the runtime will accept in that block, with
	/// `current = parent + 1`:
	/// - `current - window <= anchor <= current`,
	/// - `anchor >= latest anchor of the signer on chain`.
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
		log::debug!(target: LOG_TARGET, "Providing {} reports for the block on {parent:?}", reports.len());
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
				log::debug!(target: LOG_TARGET, "Header of {parent:?} not found, providing no reports");
				return None;
			},
			Err(e) => {
				log::debug!(target: LOG_TARGET, "Header of {parent:?} unavailable ({e}), providing no reports");
				return None;
			},
		};
		let parent_number = Anchor((*header.number()).saturated_into());

		let api = client.runtime_api();
		let (window, on_chain) = match (api.settings(parent), api.latest_anchors(parent)) {
			(Ok(settings), Ok(on_chain)) => (settings.report_window, on_chain),
			(Err(e), _) | (_, Err(e)) => {
				log::debug!(target: LOG_TARGET, "Price oracle API unavailable at {parent:?} ({e}), providing no reports");
				return None;
			},
		};
		let (oldest, newest) = anchor_bounds(parent_number, window);
		Some(pool.select(&on_chain, oldest, newest))
	}
}

/// The anchors the runtime accepts in the block built on `parent_number`:
/// `current - window ..= current` with `current = parent_number + 1`.
fn anchor_bounds(parent_number: Anchor, window: u32) -> (Anchor, Anchor) {
	let current = parent_number.0.saturating_add(1);
	(Anchor(current.saturating_sub(window)), Anchor(current))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn bounds_match_the_runtime_window() {
		// The runtime accepts `current - anchor <= window` with `current = parent + 1`.
		let accepted = |anchor: u32, parent: u32, window: u32| {
			let current = parent + 1;
			anchor <= current && current - anchor <= window
		};
		for (parent, window) in [(100, 10), (100, 1), (0, 5), (5, 0)] {
			let (oldest, newest) = anchor_bounds(Anchor(parent), window);
			// Every selected anchor is accepted by the runtime, and the ones just outside are not.
			for anchor in oldest.0..=newest.0 {
				assert!(accepted(anchor, parent, window), "{anchor} at {parent}/{window}");
			}
			if oldest.0 > 0 {
				assert!(!accepted(oldest.0 - 1, parent, window));
			}
			assert!(!accepted(newest.0 + 1, parent, window));
		}
		assert_eq!(anchor_bounds(Anchor(100), 10), (Anchor(91), Anchor(101)));
		// A zero window, as served while the parameters are unset, selects only the current
		// anchor, which the runtime accepts as well.
		assert_eq!(anchor_bounds(Anchor(5), 0), (Anchor(6), Anchor(6)));
		// No overflow at the top of the range.
		assert_eq!(anchor_bounds(Anchor(u32::MAX), 1).1, Anchor(u32::MAX));
	}
}
