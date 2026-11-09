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

//! Order book pricing: the execution band.
//!
//! **Inputs:** the active order books of a pair's markets, sorted best-first with amounts in base
//! asset units; latest trade times; this tick's prices of the cross rates; and each node's
//! previous price `p`, initially unset. Convert contract amounts to base asset units with
//! `contract_size` before calculating fill prices. Each configured spot or perpetual market
//! contributes one equally weighted [`Interval`].
//!
//! 1. **Select books.** Exclude missing books, books with no level on either side, invalid levels
//!    (zero price), stale trades (age > [`HealthLimits::max_trade_age_ms`]), markets of a cross
//!    rate whose `rate` has no price, and crossed books (best bid ≥ best ask). Ignore zero amounts
//!    after validation. Empty sides are allowed. An insufficiently filled side supplies an unknown
//!    edge rather than excluding the market.
//!
//! 2. **Fill each side independently at `Q`**, [`HealthLimits::impact_size`]. Start with remaining
//!    quote amount `R = Q` and base units `B = 0`. For each level `(price, amount)`, best-first:
//!
//!    ```text
//!    spent = min(R, price × amount)
//!    B += spent / price
//!    R -= spent
//!    stop when R = 0
//!    ```
//!
//!    If fully filled, the fill price is `Q / B`.
//!
//! 3. **Make intervals.** Lower edge = bid-side fill price, or **−∞** if unfilled. Upper edge =
//!    ask-side fill price, or **+∞** if unfilled. For a cross rate `(source, rate)`, multiply the
//!    finite edges of the `source` markets by the price of `rate`.
//!
//! 4. **Aggregate.** Let `N` be the number of markets retained after step 1, and `M` the pair's
//!    market quorum. Require `N ≥ M`, and require more than `N/2` finite lower edges and more than
//!    `N/2` finite upper edges. Set `L = median(lower edges)` and `U = median(upper edges)`,
//!    retaining the −∞/+∞ edges in the sorted lists. For an even count, average the middle two
//!    values. If any requirement fails, or `0 < L ≤ U < ∞` fails, report no new price and keep `p`
//!    unchanged.
//!
//! 5. **Report and remember one price.** If `p` is unset, use `(L + U) / 2`; otherwise use:
//!
//!    ```text
//!    p = max(L, min(p, U))
//!    ```
//!
//! At each tick, run steps 1–5 first for the pairs without cross rates, such as USDT/USD, then for
//! the others, such as DOT/USD. Each node keeps its own previous price `p`, the pricing state. Use
//! this tick's price of a `rate` to convert the `source` markets, even if the price is unchanged.
//! If the `rate` has no price, skip the `source` markets for that tick.
//!
//! The steps are in [`price_market`] (1 and 2) and [`band::aggregate`] (3 to 5).

pub mod band;
pub mod schema;

use crate::{
	registry::{MaxQueries, StoredQuery},
	MarketPricing,
};
use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::BoundedVec;
use scale_info::TypeInfo;
use schema::{Level, OrderBook, Parsed, ResponseSchema, ResponseSchemaError};
use sp_price_oracle::{market::QueryTag, PairAggregate, PairId, Price};
use sp_runtime::{
	traits::{CheckedAdd, CheckedDiv, CheckedMul, CheckedSub, Zero},
	FixedU128,
};

/// Prices markets from their order book. See the [module docs](self).
pub struct OrderBookPricing;

/// The prices at which a market fills [`HealthLimits::impact_size`] on each side of its book.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Encode, Decode)]
pub struct Interval {
	/// Selling into the bids. `None` if the bids cannot fill it.
	pub lower: Option<Price>,
	/// Buying from the asks. `None` if the asks cannot fill it.
	pub upper: Option<Price>,
}

/// The pricing parameters of a market.
#[derive(
	Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct OrderBookParams {
	/// The queries to fetch, each with the schema to read its response.
	pub queries: BoundedVec<(StoredQuery, ResponseSchema), MaxQueries>,
	/// The amount of the base asset that one unit of a level's amount represents. One for spot
	/// markets and for derivatives sized in the base asset. Only instruments whose amount is a
	/// fixed quantity of the base asset can be configured; inverse contracts, sized in the quote
	/// currency, cannot.
	pub contract_size: FixedU128,
	/// The limits the market must pass to be priced.
	pub limits: HealthLimits,
}

/// The limits a market must pass to be priced.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub struct HealthLimits {
	/// Quote asset amount filled against each side of the book.
	pub impact_size: Price,
	/// A market whose latest trade is older than this is rejected.
	pub max_trade_age_ms: u32,
}

/// Why a market could not be priced by [`OrderBookPricing`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
	/// There is no schema for a response.
	UnknownQuery,
	/// A response could not be read.
	Schema(ResponseSchemaError),
	/// Scaling the book by the contract size overflowed.
	Overflow,
	/// There is no order book response.
	NoOrderBook,
	/// There is no trades response.
	NoTrades,
	/// The market failed a health check.
	Unhealthy(HealthError),
}

/// Why a market failed a health check.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HealthError {
	/// The book has no level on either side.
	EmptyBook,
	/// The best bid is not below the best ask.
	CrossedBook,
	/// The latest trade is older than [`HealthLimits::max_trade_age_ms`].
	StaleTrades,
	/// Arithmetic overflow.
	Overflow,
}

impl MarketPricing for OrderBookPricing {
	type Params = OrderBookParams;
	type Output = Interval;
	type State = band::Prices;
	type Error = Error;

	fn validate(params: &OrderBookParams) -> bool {
		!params.contract_size.is_zero() && !params.limits.impact_size.is_zero()
	}

	fn queries(params: &OrderBookParams) -> Vec<StoredQuery> {
		params.queries.iter().map(|(query, _)| query.clone()).collect()
	}

	/// Needs an order book and a trades response.
	fn price(
		params: &OrderBookParams,
		responses: &[(QueryTag, Vec<u8>)],
		now_ms: u64,
	) -> Result<Interval, Error> {
		let mut book = None;
		let mut latest_trade_ms = None;
		for (tag, body) in responses {
			let Some((_, schema)) = params.queries.iter().find(|(query, _)| query.tag == *tag)
			else {
				return Err(Error::UnknownQuery);
			};
			match schema.read(body).map_err(Error::Schema)? {
				Parsed::OrderBook(mut b) => {
					// Convert amounts from contracts to base asset units.
					for level in b.bids.iter_mut().chain(b.asks.iter_mut()) {
						level.amount = level
							.amount
							.checked_mul(&params.contract_size)
							.ok_or(Error::Overflow)?;
					}
					// Leave out levels without a price or an amount. The schema rejects the first;
					// the contract size can leave a level too small to hold a base asset unit.
					let priced = |level: &Level| !level.price.is_zero() && !level.amount.is_zero();
					b.bids.retain(priced);
					b.asks.retain(priced);
					book = Some(b);
				},
				Parsed::LatestTradeMs(t) => latest_trade_ms = Some(t),
			}
		}
		let book = book.ok_or(Error::NoOrderBook)?;
		let latest_trade_ms = latest_trade_ms.ok_or(Error::NoTrades)?;
		price_market(&book, latest_trade_ms, now_ms, &params.limits).map_err(Error::Unhealthy)
	}

	/// The [execution band](band::aggregate) of each pair. The state is the last price of every
	/// pair.
	fn aggregate(
		markets: Vec<(PairId, Interval)>,
		pairs: &[PairId],
		cross_rates: impl Fn(PairId) -> Vec<(PairId, PairId)>,
		quorum: impl Fn(PairId) -> u32,
		previous: band::Prices,
	) -> (Vec<PairAggregate>, band::Prices) {
		band::aggregate(markets, pairs, cross_rates, quorum, previous)
	}
}

/// The interval of a market from its order book, if it passes `limits`.
///
/// One side may be empty or too thin to fill [`HealthLimits::impact_size`]; its edge is then
/// `None`. A book with no level on either side is rejected.
pub fn price_market(
	book: &OrderBook,
	latest_trade_ms: u64,
	now_ms: u64,
	limits: &HealthLimits,
) -> Result<Interval, HealthError> {
	match (book.bids.first(), book.asks.first()) {
		(None, None) => return Err(HealthError::EmptyBook),
		(Some(best_bid), Some(best_ask)) if best_bid.price >= best_ask.price => {
			return Err(HealthError::CrossedBook)
		},
		_ => {},
	}
	if now_ms.saturating_sub(latest_trade_ms) > limits.max_trade_age_ms as u64 {
		return Err(HealthError::StaleTrades);
	}
	Ok(Interval {
		lower: fill_price(&book.bids, limits.impact_size)?,
		upper: fill_price(&book.asks, limits.impact_size)?,
	})
}

/// The average price at which `size` quote units are filled against `side`, walking the levels
/// in order. `None` if the side cannot fill `size`.
fn fill_price(side: &[Level], size: Price) -> Result<Option<Price>, HealthError> {
	let mut remaining = size;
	let mut received = Price::zero();
	for level in side {
		let cost = level.price.checked_mul(&level.amount).ok_or(HealthError::Overflow)?;
		if cost >= remaining {
			let partial = remaining.checked_div(&level.price).ok_or(HealthError::Overflow)?;
			received = received.checked_add(&partial).ok_or(HealthError::Overflow)?;
			remaining = Price::zero();
			break;
		}
		received = received.checked_add(&level.amount).ok_or(HealthError::Overflow)?;
		remaining = remaining.checked_sub(&cost).ok_or(HealthError::Overflow)?;
	}
	if !remaining.is_zero() || received.is_zero() {
		return Ok(None);
	}
	size.checked_div(&received).map(Some).ok_or(HealthError::Overflow)
}

#[cfg(test)]
mod price_market_tests {
	use super::*;
	use crate::pricing::parse_decimal;

	fn p(s: &str) -> Price {
		parse_decimal(s).unwrap()
	}
	fn level(price: &str, amount: &str) -> Level {
		Level { price: p(price), amount: p(amount) }
	}
	fn limits() -> HealthLimits {
		HealthLimits { impact_size: p("10000"), max_trade_age_ms: 300_000 }
	}
	/// Bids 4.00 x 1000, 3.99 x 5000; asks 4.02 x 1000, 4.03 x 5000.
	fn book() -> OrderBook {
		OrderBook {
			bids: vec![level("4.00", "1000"), level("3.99", "5000")],
			asks: vec![level("4.02", "1000"), level("4.03", "5000")],
		}
	}

	#[test]
	fn fill_price_walks_levels() {
		// 10000 quote into asks: 1000 @ 4.02 = 4020, remaining 5980 @ 4.03 = 1483.87 units.
		let received = p("1000") + p("5980").checked_div(&p("4.03")).unwrap();
		assert_eq!(fill_price(&book().asks, p("10000")), Ok(Some(p("10000") / received)));
		// Exactly one level.
		assert_eq!(fill_price(&book().asks, p("4020")), Ok(Some(p("4.02"))));
		// Too thin, or empty: no fill.
		assert_eq!(fill_price(&book().asks, p("100000")), Ok(None));
		assert_eq!(fill_price(&[], p("1")), Ok(None));
	}

	#[test]
	fn interval_is_the_fill_of_each_side() {
		let interval = price_market(&book(), 0, 0, &limits()).unwrap();
		assert_eq!(interval.lower, fill_price(&book().bids, p("10000")).unwrap());
		assert_eq!(interval.upper, fill_price(&book().asks, p("10000")).unwrap());
		assert!(interval.lower.unwrap() < p("4.00") && interval.upper.unwrap() > p("4.02"));
	}

	#[test]
	fn a_side_that_cannot_fill_has_no_edge() {
		// Thin: neither side fills, the market still gives an interval.
		let thin = HealthLimits { impact_size: p("1000000"), ..limits() };
		let interval = price_market(&book(), 0, 0, &thin);
		assert_eq!(interval, Ok(Interval { lower: None, upper: None }));
		// Empty asks.
		let mut b = book();
		b.asks.clear();
		let interval = price_market(&b, 0, 0, &limits()).unwrap();
		assert!(interval.lower.is_some());
		assert_eq!(interval.upper, None);
	}

	#[test]
	fn a_wide_book_is_priced() {
		let mut b = book();
		b.asks = vec![level("8.00", "5000")];
		let interval = price_market(&b, 0, 0, &limits()).unwrap();
		assert_eq!(interval.upper, Some(p("8.00")));
	}

	#[test]
	fn health_checks_reject() {
		let s = limits();
		let now = 1_000_000;
		// No level on either side.
		assert_eq!(price_market(&OrderBook::default(), now, now, &s), Err(HealthError::EmptyBook));
		// Crossed, and touching.
		let mut b = book();
		b.bids[0].price = p("4.03");
		assert_eq!(price_market(&b, now, now, &s), Err(HealthError::CrossedBook));
		b.bids[0].price = p("4.02");
		assert_eq!(price_market(&b, now, now, &s), Err(HealthError::CrossedBook));
		// Stale trades; the limit itself passes.
		assert!(price_market(&book(), now, now + 300_000, &s).is_ok());
		assert_eq!(price_market(&book(), now, now + 300_001, &s), Err(HealthError::StaleTrades));
	}
}

#[cfg(test)]
mod order_book_pricing_tests {
	use super::*;
	use crate::{pricing::parse_decimal, registry::StoredRequest};
	use schema::{LevelLayout, PathStep, TimeFormat};
	use sp_price_oracle::market::Method;

	const BOOK: QueryTag = QueryTag(0);
	const TRADES: QueryTag = QueryTag(1);
	const NOW_MS: u64 = 1_000_000;
	/// Bids 4.00 x 1000, 3.99 x 5000; asks 4.02 x 1000, 4.03 x 5000.
	const BOOK_JSON: &[u8] = br#"{
		"bids": [["4.00", "1000"], ["3.99", "5000"]],
		"asks": [["4.02", "1000"], ["4.03", "5000"]]
	}"#;
	/// One trade at `NOW_MS`.
	const TRADES_JSON: &[u8] = br#"[{"time": 1000000}]"#;

	fn p(s: &str) -> Price {
		parse_decimal(s).unwrap()
	}

	/// A query with `tag`. Its request is never sent.
	fn query(tag: QueryTag) -> StoredQuery {
		let request = StoredRequest {
			method: Method::Get,
			host: Default::default(),
			path: Default::default(),
			query: Default::default(),
			headers: Default::default(),
			body: Default::default(),
			timeout_ms: 1_000,
			max_response_bytes: 1_024,
		};
		StoredQuery { tag, request }
	}

	fn params() -> OrderBookParams {
		let key = |k| PathStep::key(k).unwrap();
		let book = ResponseSchema::OrderBook {
			bids: vec![key("bids")].try_into().unwrap(),
			asks: vec![key("asks")].try_into().unwrap(),
			layout: LevelLayout::Array { price: 0, amount: 1 },
		};
		let trades = ResponseSchema::Trades {
			trades: Default::default(),
			time: vec![key("time")].try_into().unwrap(),
			format: TimeFormat::Millis,
		};
		OrderBookParams {
			queries: vec![(query(BOOK), book), (query(TRADES), trades)].try_into().unwrap(),
			contract_size: p("1"),
			limits: HealthLimits { impact_size: p("10000"), max_trade_age_ms: 300_000 },
		}
	}

	fn price(
		params: &OrderBookParams,
		responses: &[(QueryTag, &[u8])],
		now_ms: u64,
	) -> Result<Interval, Error> {
		let responses: Vec<_> = responses.iter().map(|(tag, body)| (*tag, body.to_vec())).collect();
		OrderBookPricing::price(params, &responses, now_ms)
	}

	#[test]
	fn prices_the_interval_of_the_book() {
		let Ok(Parsed::OrderBook(book)) = params().queries[0].1.read(BOOK_JSON) else {
			panic!("the book fixture is readable")
		};
		assert_eq!(
			price(&params(), &[(BOOK, BOOK_JSON), (TRADES, TRADES_JSON)], NOW_MS),
			price_market(&book, NOW_MS, NOW_MS, &params().limits).map_err(Error::Unhealthy),
		);
	}

	#[test]
	fn needs_a_book_and_trades() {
		assert_eq!(price(&params(), &[(TRADES, TRADES_JSON)], NOW_MS), Err(Error::NoOrderBook));
		assert_eq!(price(&params(), &[(BOOK, BOOK_JSON)], NOW_MS), Err(Error::NoTrades));
	}

	#[test]
	fn rejects_responses_it_cannot_read() {
		assert_eq!(price(&params(), &[(QueryTag(9), BOOK_JSON)], NOW_MS), Err(Error::UnknownQuery));
		assert_eq!(
			price(&params(), &[(BOOK, b"not json")], NOW_MS),
			Err(Error::Schema(ResponseSchemaError::NotJson)),
		);
	}

	#[test]
	fn contract_size_scales_the_book() {
		// At a thousandth of a unit per contract each side holds about 24 USDT, too thin for 10000.
		let mut params = params();
		params.contract_size = p("0.001");
		assert_eq!(
			price(&params, &[(BOOK, BOOK_JSON), (TRADES, TRADES_JSON)], NOW_MS),
			Ok(Interval { lower: None, upper: None }),
		);
	}

	#[test]
	fn levels_without_amount_after_the_contract_size_are_left_out() {
		// A best bid of 4.05 too small to hold a unit would cross the book if it were kept.
		let book = br#"{
			"bids": [["4.05", "0.000000000000000001"], ["4.00", "1000"], ["3.99", "5000"]],
			"asks": [["4.02", "1000"], ["4.03", "5000"]]
		}"#;
		let mut params = params();
		params.contract_size = p("0.1");
		let interval = price(&params, &[(BOOK, book), (TRADES, TRADES_JSON)], NOW_MS);
		assert!(interval.is_ok(), "{interval:?}");
	}

	#[test]
	fn a_book_of_levels_without_amount_is_empty() {
		let book = br#"{"bids": [["4.00", "0"]], "asks": [["4.02", "0"]]}"#;
		assert_eq!(
			price(&params(), &[(BOOK, book), (TRADES, TRADES_JSON)], NOW_MS),
			Err(Error::Unhealthy(HealthError::EmptyBook)),
		);
	}

	#[test]
	fn applies_the_health_limits() {
		let later = NOW_MS + 300_001;
		assert_eq!(
			price(&params(), &[(BOOK, BOOK_JSON), (TRADES, TRADES_JSON)], later),
			Err(Error::Unhealthy(HealthError::StaleTrades)),
		);
	}

	#[test]
	fn validate_rejects_zero_sizes() {
		assert!(OrderBookPricing::validate(&params()));

		let mut invalid = params();
		invalid.contract_size = Zero::zero();
		assert!(!OrderBookPricing::validate(&invalid));

		let mut invalid = params();
		invalid.limits.impact_size = Zero::zero();
		assert!(!OrderBookPricing::validate(&invalid));
	}
}
