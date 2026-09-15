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

//! Runtime APIs of the price oracle, called by the oracle node.

use crate::{
	market::{Market, MarketId, QueryTag},
	Anchor, Price, Quote,
};
use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;

/// Reason a market could not be priced from the responses to its queries.
#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct ParseError(pub Vec<u8>);

sp_api::decl_runtime_apis! {
	/// Consensus side of the oracle: who may sign and what is already on chain.
	pub trait PriceOracleApi<Id: Decode> {
		/// Keys whose reports are accepted right now.
		fn signers() -> Vec<Id>;
		/// Reports anchored more than this many blocks ago are ignored.
		fn report_window() -> u32;
		/// Anchor of the most recent report already on chain, per signer.
		fn latest_anchors() -> Vec<(Id, Anchor)>;
	}

	/// Market side of the oracle: what to fetch and how to turn it into prices.
	///
	/// A tick is one pass of the node over all markets: fetch, parse each market, aggregate,
	/// sign one report.
	pub trait PriceOracleMarketApi {
		/// Time between two ticks.
		fn tick_interval_ms() -> u32;
		/// Active markets with their queries.
		fn markets() -> Vec<Market>;
		/// Price one market from the responses to its queries, given the node's clock.
		fn parse(
			market: MarketId,
			responses: Vec<(QueryTag, Vec<u8>)>,
			now_ms: u64,
		) -> Result<Price, ParseError>;
		/// Turn market prices into the pair prices to report.
		fn aggregate(prices: Vec<(MarketId, Price)>) -> Vec<Quote>;
	}
}
