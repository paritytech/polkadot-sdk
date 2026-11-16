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
	Anchor, PairAggregate, Settings,
};
use alloc::{string::String, vec::Vec};
use codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;

/// Reason a market could not be priced from the responses to its queries, as text.
#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct ParseError(pub Vec<u8>);

impl core::fmt::Display for ParseError {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.write_str(&String::from_utf8_lossy(&self.0))
	}
}

/// What pricing a market gave, for [`PriceOracleApi::aggregate`]. Only the runtime reads it.
#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct MarketOutput(pub Vec<u8>);

/// What the pricing method keeps from one tick to the next, such as the last prices. The node
/// holds it for the next tick only, but only the runtime reads it. Empty when the node's last
/// tick did not report, and after a restart.
#[derive(Clone, PartialEq, Eq, Debug, Default, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct PricingState(pub Vec<u8>);

sp_api::decl_runtime_apis! {
	/// What an oracle node needs from the runtime.
	///
	/// A tick is one pass of the node over all markets: fetch, parse each market, aggregate,
	/// sign one report.
	pub trait PriceOracleApi<Id: Decode> {
		/// The rules in force: who may sign, how old a report may be, how often to tick.
		fn settings() -> Settings<Id>;
		/// Anchor of the most recent report already on chain, per signer.
		fn latest_anchors() -> Vec<(Id, Anchor)>;
		/// Active markets with their queries.
		fn markets() -> Vec<Market>;
		/// Price one market from the responses to its queries, given the node's clock.
		fn parse(
			market: MarketId,
			responses: Vec<(QueryTag, Vec<u8>)>,
			now_ms: u64,
		) -> Result<MarketOutput, ParseError>;
		/// Turn market outputs into pair prices, one entry per registered pair, and the state for
		/// the next tick. The pairs with a price are the ones to report.
		fn aggregate(
			outputs: Vec<(MarketId, MarketOutput)>,
			state: PricingState,
		) -> (Vec<PairAggregate>, PricingState);
	}
}
