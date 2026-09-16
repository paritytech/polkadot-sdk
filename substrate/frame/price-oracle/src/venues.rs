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

//! Definitions of the markets of well-known exchanges.
//!
//! Each function builds the queries of one market, an order book query and a recent trades
//! query, as stored by [`Pallet::set_market`](crate::Pallet::set_market). The symbol is given
//! in the convention of the exchange, e.g. `DOTUSDT` on Binance.
//!
//! Runtimes use these definitions to register their initial markets. Each definition is tested
//! against a sample response of the exchange. A definition is `None` only if one of its values
//! does not fit the bounds of the registry.

use crate::{
	registry::{MaxParamName, MaxParamValue, MaxQueries, StoredQuery, StoredRequest},
	schema::{LevelLayout, Path, PathStep, ResponseSchema, TimeFormat},
};
use alloc::vec::Vec;
use frame_support::{traits::Get, BoundedVec};
use sp_price_oracle::market::{Method, QueryTag};

/// The tag of the order book query of every market defined in this module.
const BOOK: QueryTag = QueryTag(0);
/// The tag of the trades query of every market defined in this module.
const TRADES: QueryTag = QueryTag(1);

/// The time after which a request is abandoned. A venue that does not answer in time is left
/// out of the tick.
const TIMEOUT_MS: u32 = 1_000;
/// The maximum size of an order book response. About twice a book of [`DEPTH`] levels.
const MAX_BOOK_BYTES: u32 = 16 * 1024;
/// The maximum size of a trades response, a few times the size of a single trade.
const MAX_TRADES_BYTES: u32 = 1024;
/// The number of levels requested where the exchange allows choosing.
const DEPTH: &str = "100";
/// The number of trades requested where the exchange allows choosing. Only the latest trade is
/// needed, and exchanges return the most recent ones.
const TRADES_LIMIT: &str = "1";
/// The layout of a level given as a `[price, amount, ..]` array, used by most exchanges.
const ARRAY: LevelLayout = LevelLayout::Array { price: 0, amount: 1 };

/// The Binance spot market of `symbol`, e.g. `DOTUSDT`.
///
/// The book is `{"bids": [[price, qty], ..], "asks": [..]}` and the trades are
/// `[{"time": ms, ..}, ..]`.
pub fn binance_spot(symbol: &str) -> Option<BoundedVec<StoredQuery, MaxQueries>> {
	const HOST: &str = "api.binance.com";
	market(
		book(
			request(
				HOST,
				"/api/v3/depth",
				&[("symbol", symbol), ("limit", DEPTH)],
				MAX_BOOK_BYTES,
			)?,
			&[PathStep::key("bids")?],
			&[PathStep::key("asks")?],
			ARRAY,
		)?,
		trades(
			request(
				HOST,
				"/api/v3/trades",
				&[("symbol", symbol), ("limit", TRADES_LIMIT)],
				MAX_TRADES_BYTES,
			)?,
			&[],
			&[PathStep::key("time")?],
			TimeFormat::Millis,
		)?,
	)
}

/// Convert `s` to a bounded byte vector.
fn bounded<S: Get<u32>>(s: &str) -> Option<BoundedVec<u8, S>> {
	s.as_bytes().to_vec().try_into().ok()
}

/// Build a bounded JSON path from `steps`.
fn path(steps: &[PathStep]) -> Option<Path> {
	steps.to_vec().try_into().ok()
}

/// Build the stored `GET` request for `host` and `path` with the given query parameters.
fn request(
	host: &str,
	path: &str,
	query: &[(&str, &str)],
	max_response_bytes: u32,
) -> Option<StoredRequest> {
	let query: Vec<(BoundedVec<u8, MaxParamName>, BoundedVec<u8, MaxParamValue>)> = query
		.iter()
		.map(|(n, v)| Some((bounded(n)?, bounded(v)?)))
		.collect::<Option<_>>()?;
	Some(StoredRequest {
		method: Method::Get,
		host: bounded(host)?,
		path: bounded(path)?,
		query: query.try_into().ok()?,
		headers: BoundedVec::new(),
		body: BoundedVec::new(),
		timeout_ms: TIMEOUT_MS,
		max_response_bytes,
	})
}

/// Build the order book query of a market.
fn book(
	request: StoredRequest,
	bids: &[PathStep],
	asks: &[PathStep],
	layout: LevelLayout,
) -> Option<StoredQuery> {
	let schema = ResponseSchema::OrderBook { bids: path(bids)?, asks: path(asks)?, layout };
	Some(StoredQuery { tag: BOOK, request, schema })
}

/// Build the trades query of a market.
fn trades(
	request: StoredRequest,
	rows: &[PathStep],
	time: &[PathStep],
	format: TimeFormat,
) -> Option<StoredQuery> {
	let schema = ResponseSchema::Trades { trades: path(rows)?, time: path(time)?, format };
	Some(StoredQuery { tag: TRADES, request, schema })
}

/// Assemble the queries of a market.
fn market(book: StoredQuery, trades: StoredQuery) -> Option<BoundedVec<StoredQuery, MaxQueries>> {
	alloc::vec![book, trades].try_into().ok()
}
