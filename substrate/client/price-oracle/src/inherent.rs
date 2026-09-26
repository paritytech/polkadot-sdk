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
	/// Carries the reports of `pool` anchored within the report window at `parent` and at or
	/// below the height of `parent`, excluding reports anchored before the signer's vote already
	/// on chain at `parent`. Carries no reports if the header of `parent` is unavailable or the
	/// runtime at `parent` does not implement the price oracle API.
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
		let newest = Anchor((*header.number()).saturated_into());

		let api = client.runtime_api();
		let (window, on_chain) = match (api.report_window(parent), api.latest_anchors(parent)) {
			(Ok(window), Ok(on_chain)) => (window, on_chain),
			(Err(e), _) | (_, Err(e)) => {
				log::debug!(target: LOG_TARGET, "Price oracle API unavailable at {parent:?} ({e}), providing no reports");
				return None;
			},
		};
		let oldest = Anchor(newest.0.saturating_sub(window));
		Some(pool.select(&on_chain, oldest, newest))
	}
}
