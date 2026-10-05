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
//! Order book pricing.
//!
//! The price of a market is the [impact mid](impact_mid) of its order book. Markets that fail
//! their [`HealthLimits`] are not priced.

pub mod schema;

use crate::{pricing::median, registry::MaxQueries, MarketPricing};
use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::BoundedVec;
use scale_info::TypeInfo;
use schema::{Level, OrderBook, Parsed, ResponseSchema, ResponseSchemaError};
use sp_price_oracle::{market::QueryTag, Price};
use sp_runtime::{
	traits::{CheckedAdd, CheckedDiv, CheckedMul, CheckedSub, Zero},
	FixedU128, Permill,
};

/// Prices markets from their order book. See the [module docs](self).
pub struct OrderBookPricing;

/// The pricing parameters of a market.
#[derive(
	Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct OrderBookParams {
	/// How to read the response of each query, by query tag.
	pub schemas: BoundedVec<(QueryTag, ResponseSchema), MaxQueries>,
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
	/// Quote asset amount priced against each side of the book to obtain its impact mid.
	pub impact_size: Price,
	/// A book whose spread relative to its mid exceeds this is rejected.
	pub max_spread: Permill,
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
	/// The best bid is not below the best ask.
	CrossedBook,
	/// The spread exceeds [`HealthLimits::max_spread`].
	SpreadTooWide,
	/// The latest trade is older than [`HealthLimits::max_trade_age_ms`].
	StaleTrades,
	/// A side of the book cannot fill [`HealthLimits::impact_size`].
	BookTooThin,
	/// Arithmetic overflow.
	Overflow,
}

impl MarketPricing for OrderBookPricing {
	type Params = OrderBookParams;
	type Error = Error;

	fn validate(params: &OrderBookParams) -> bool {
		!params.contract_size.is_zero() && !params.limits.impact_size.is_zero()
	}

	/// Needs an order book and a trades response.
	fn price(
		params: &OrderBookParams,
		responses: &[(QueryTag, Vec<u8>)],
		now_ms: u64,
	) -> Result<Price, Error> {
		let mut book = None;
		let mut latest_trade_ms = None;
		for (tag, body) in responses {
			let Some((_, schema)) = params.schemas.iter().find(|(t, _)| t == tag) else {
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
					book = Some(b);
				},
				Parsed::LatestTradeMs(t) => latest_trade_ms = Some(t),
			}
		}
		let book = book.ok_or(Error::NoOrderBook)?;
		let latest_trade_ms = latest_trade_ms.ok_or(Error::NoTrades)?;
		price_market(&book, latest_trade_ms, now_ms, &params.limits).map_err(Error::Unhealthy)
	}
}

/// Price a market from its order book, if it passes `limits`.
pub fn price_market(
	book: &OrderBook,
	latest_trade_ms: u64,
	now_ms: u64,
	limits: &HealthLimits,
) -> Result<Price, HealthError> {
	let (Some(best_bid), Some(best_ask)) = (book.bids.first(), book.asks.first()) else {
		return Err(HealthError::BookTooThin);
	};
	if best_bid.price >= best_ask.price {
		return Err(HealthError::CrossedBook);
	}
	// spread / mid <= max_spread  <=>  2 * (ask - bid) <= max_spread * (ask + bid)
	let spread2 = best_ask
		.price
		.checked_sub(&best_bid.price)
		.and_then(|d| d.checked_mul(&Price::from_u32(2)))
		.ok_or(HealthError::Overflow)?;
	let sum = best_ask.price.checked_add(&best_bid.price).ok_or(HealthError::Overflow)?;
	let allowed = Price::from_inner(limits.max_spread.mul_floor(sum.into_inner()));
	if spread2 > allowed {
		return Err(HealthError::SpreadTooWide);
	}
	if now_ms.saturating_sub(latest_trade_ms) > limits.max_trade_age_ms as u64 {
		return Err(HealthError::StaleTrades);
	}
	impact_mid(book, limits.impact_size)
}

/// The average of the prices at which `size` quote units are bought from the asks and sold into
/// the bids.
pub fn impact_mid(book: &OrderBook, size: Price) -> Result<Price, HealthError> {
	let ask = fill_price(&book.asks, size)?;
	let bid = fill_price(&book.bids, size)?;
	let mut both = [ask, bid];
	median(&mut both).ok_or(HealthError::Overflow)
}

/// The average price at which `size` quote units are filled against `side`, walking the levels
/// in order. Fails with [`HealthError::BookTooThin`] if the side cannot fill `size`.
fn fill_price(side: &[Level], size: Price) -> Result<Price, HealthError> {
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
		return Err(HealthError::BookTooThin);
	}
	size.checked_div(&received).ok_or(HealthError::Overflow)
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
		HealthLimits {
			impact_size: p("10000"),
			max_spread: Permill::from_parts(5_000), // 0.5%
			max_trade_age_ms: 300_000,
		}
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
		assert_eq!(fill_price(&book().asks, p("10000")).unwrap(), p("10000") / received);
		// Exactly one level.
		assert_eq!(fill_price(&book().asks, p("4020")).unwrap(), p("4.02"));
		// Too thin.
		assert_eq!(fill_price(&book().asks, p("100000")), Err(HealthError::BookTooThin));
		assert_eq!(fill_price(&[], p("1")), Err(HealthError::BookTooThin));
	}

	#[test]
	fn impact_mid_is_mean_of_both_fills() {
		let ask = fill_price(&book().asks, p("10000")).unwrap();
		let bid = fill_price(&book().bids, p("10000")).unwrap();
		assert_eq!(impact_mid(&book(), p("10000")).unwrap(), median(&mut [ask, bid]).unwrap());
	}

	#[test]
	fn healthy_market_is_priced() {
		assert!(price_market(&book(), 1_000_000, 1_000_000 + 60_000, &limits()).is_ok());
	}

	#[test]
	fn health_checks_reject() {
		let s = limits();
		let now = 1_000_000;
		// Crossed.
		let mut b = book();
		b.bids[0].price = p("4.02");
		assert_eq!(price_market(&b, now, now, &s), Err(HealthError::CrossedBook));
		// Spread 4.00 / 4.03: 2 * 0.03 = 0.06 > 0.005 * 8.03 = 0.04015.
		let mut b = book();
		b.asks[0].price = p("4.03");
		assert_eq!(price_market(&b, now, now, &s), Err(HealthError::SpreadTooWide));
		// Spread 4.00 / 4.02 passes: 0.04 <= 0.005 * 8.02 = 0.0401.
		assert!(price_market(&book(), now, now, &s).is_ok());
		// Stale trades.
		assert_eq!(price_market(&book(), now, now + 300_001, &s), Err(HealthError::StaleTrades));
		// Thin.
		let thin = HealthLimits { impact_size: p("1000000"), ..s };
		assert_eq!(price_market(&book(), now, now, &thin), Err(HealthError::BookTooThin));
		// Empty side.
		let mut b = book();
		b.asks.clear();
		assert_eq!(price_market(&b, now, now, &s), Err(HealthError::BookTooThin));
	}
}

#[cfg(test)]
mod order_book_pricing_tests {
	use super::*;
	use crate::pricing::parse_decimal;
	use schema::{LevelLayout, PathStep, TimeFormat};

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
			schemas: vec![(BOOK, book), (TRADES, trades)].try_into().unwrap(),
			contract_size: p("1"),
			limits: HealthLimits {
				impact_size: p("10000"),
				max_spread: Permill::from_parts(5_000), // 0.5%
				max_trade_age_ms: 300_000,
			},
		}
	}

	fn price(
		params: &OrderBookParams,
		responses: &[(QueryTag, &[u8])],
		now_ms: u64,
	) -> Result<Price, Error> {
		let responses: Vec<_> = responses.iter().map(|(tag, body)| (*tag, body.to_vec())).collect();
		OrderBookPricing::price(params, &responses, now_ms)
	}

	#[test]
	fn prices_the_impact_mid_of_the_book() {
		let Ok(Parsed::OrderBook(book)) = params().schemas[0].1.read(BOOK_JSON) else {
			panic!("the book fixture is readable")
		};
		assert_eq!(
			price(&params(), &[(BOOK, BOOK_JSON), (TRADES, TRADES_JSON)], NOW_MS),
			impact_mid(&book, p("10000")).map_err(Error::Unhealthy),
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
			Err(Error::Unhealthy(HealthError::BookTooThin)),
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
