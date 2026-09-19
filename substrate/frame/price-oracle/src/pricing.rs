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
//! Price arithmetic of the pallet: reading decimals, pricing order books, and aggregating
//! market prices into pair prices.

use crate::schema::OrderBook;
use alloc::{collections::BTreeMap, vec::Vec};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_price_oracle::{market::VenueId, PairId, Price, Quote};
use sp_runtime::{
	traits::{CheckedAdd, CheckedDiv, CheckedMul, CheckedSub, Zero},
	Permill,
};

/// Health limits applied to the markets of one pair.
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
pub struct PairSettings {
	/// A book whose spread relative to its mid exceeds this is rejected.
	pub max_spread: Permill,
	/// A market whose latest trade is older than this is rejected.
	pub max_trade_age_ms: u32,
	/// Quote asset amount priced against each side of a book to obtain its impact mid.
	pub impact_size: Price,
}

/// Why a market could not be priced.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HealthError {
	/// The best bid is not below the best ask.
	CrossedBook,
	/// The spread exceeds [`PairSettings::max_spread`].
	SpreadTooWide,
	/// The latest trade is older than [`PairSettings::max_trade_age_ms`].
	StaleTrades,
	/// A side of the book cannot fill [`PairSettings::impact_size`].
	BookTooThin,
	/// Arithmetic overflow.
	Overflow,
}

/// Price a market from its order book and the time of its latest trade.
pub fn market_price(
	book: &OrderBook,
	latest_trade_ms: u64,
	now_ms: u64,
	settings: &PairSettings,
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
	let allowed = Price::from_inner(settings.max_spread.mul_floor(sum.into_inner()));
	if spread2 > allowed {
		return Err(HealthError::SpreadTooWide);
	}
	if now_ms.saturating_sub(latest_trade_ms) > settings.max_trade_age_ms as u64 {
		return Err(HealthError::StaleTrades);
	}
	impact_mid(book, settings.impact_size)
}

/// The impact mid of a book: the mean of the prices at which `size` quote units are bought
/// from the asks and sold into the bids.
pub fn impact_mid(book: &OrderBook, size: Price) -> Result<Price, HealthError> {
	let ask = fill_price(&book.asks, size)?;
	let bid = fill_price(&book.bids, size)?;
	let mut both = [ask, bid];
	median(&mut both).ok_or(HealthError::Overflow)
}

/// The average price at which `size` quote units are filled against `side`, walking the levels
/// in order. `None` if the side cannot fill `size`.
fn fill_price(side: &[crate::schema::Level], size: Price) -> Result<Price, HealthError> {
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

/// A priced market: its venue, its pair and its price.
pub type MarketPrice = (VenueId, PairId, Price);

/// Aggregate market prices into one price per pair.
///
/// Every venue gets one vote per pair. A pair's votes are the prices of the markets quoting it
/// directly, plus, for every `(source, rate)` in `conversions(pair)`, the prices of the `source`
/// markets multiplied by the median of the direct `rate` prices. The price of a pair is the
/// median of its votes. Pairs without votes are omitted.
pub fn aggregate(
	prices: Vec<MarketPrice>,
	pairs: &[PairId],
	conversions: impl Fn(PairId) -> Vec<(PairId, PairId)>,
) -> Vec<Quote> {
	// Direct votes per pair, one per venue: the first market of a venue on a pair wins.
	let mut direct: BTreeMap<PairId, BTreeMap<VenueId, Price>> = BTreeMap::new();
	for (venue, pair, price) in prices {
		direct.entry(pair).or_default().entry(venue).or_insert(price);
	}
	let direct_median = |pair: PairId| -> Option<Price> {
		let mut votes: Vec<Price> = direct.get(&pair)?.values().copied().collect();
		median(&mut votes)
	};

	let mut quotes = Vec::new();
	for &pair in pairs {
		let mut votes: BTreeMap<VenueId, Price> = direct.get(&pair).cloned().unwrap_or_default();
		for (source, rate) in conversions(pair) {
			let (Some(sources), Some(rate)) = (direct.get(&source), direct_median(rate)) else {
				continue;
			};
			for (&venue, &price) in sources {
				if let Some(converted) = price.checked_mul(&rate) {
					votes.entry(venue).or_insert(converted);
				}
			}
		}
		let mut votes: Vec<Price> = votes.into_values().collect();
		if let Some(price) = median(&mut votes) {
			quotes.push(Quote { pair, price });
		}
	}
	quotes
}

/// Parse a non-negative decimal number such as `4.206`, `12`, `.5` or `1e-3` into a [`Price`].
///
/// Digits beyond the 18 decimal places of [`Price`] are dropped. Returns `None` for anything
/// that is not a plain decimal number or does not fit.
pub fn parse_decimal(s: &str) -> Option<Price> {
	const DECIMALS: i64 = 18;

	let (mantissa, exponent) = match s.find(['e', 'E']) {
		Some(i) => (&s[..i], s[i + 1..].parse::<i32>().ok()? as i64),
		None => (s, 0),
	};
	let (int_part, frac_part) = match mantissa.find('.') {
		Some(i) => (&mantissa[..i], &mantissa[i + 1..]),
		None => (mantissa, ""),
	};
	if int_part.is_empty() && frac_part.is_empty() {
		return None;
	}
	let digits: alloc::vec::Vec<u8> = int_part.bytes().chain(frac_part.bytes()).collect();
	if !digits.iter().all(u8::is_ascii_digit) {
		return None;
	}

	// The inner value is the first `keep` digits of `digits`, padded with zeros on the right.
	let keep = int_part.len() as i64 + exponent + DECIMALS;
	if keep <= 0 {
		return Some(Price::zero());
	}
	let keep = keep as usize;
	let used = keep.min(digits.len());
	let mut inner: u128 = 0;
	for d in &digits[..used] {
		inner = inner.checked_mul(10)?.checked_add((d - b'0') as u128)?;
	}
	if inner == 0 {
		return Some(Price::zero());
	}
	let padding = u32::try_from(keep - used).ok()?;
	Some(Price::from_inner(inner.checked_mul(10u128.checked_pow(padding)?)?))
}

/// The median of the given prices, or `None` if there are none.
///
/// For an even number of prices, the mean of the two middle ones. Sorts `prices` in place.
pub fn median(prices: &mut [Price]) -> Option<Price> {
	if prices.is_empty() {
		return None;
	}
	// Equal prices are indistinguishable, so a stable sort would give the same result.
	prices.sort_unstable();
	let mid = prices.len() / 2;
	if prices.len() % 2 == 1 {
		return Some(prices[mid]);
	}
	let (a, b) = (prices[mid - 1].into_inner(), prices[mid].into_inner());
	// Mean of two values that cannot overflow.
	Some(Price::from_inner(a / 2 + b / 2 + (a % 2 + b % 2) / 2))
}

#[cfg(test)]
mod parse_decimal_tests {
	use super::*;

	fn p(units: u128, thousandths: u128) -> Price {
		Price::from_rational(units * 1_000 + thousandths, 1_000)
	}

	const ACC: u128 = 1_000_000_000_000_000_000;

	/// Deterministic pseudo random `u128` values, spread across all magnitudes.
	fn pseudo_random_inners(n: usize) -> impl Iterator<Item = u128> {
		let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
		(0..n).map(move |i| {
			state ^= state << 13;
			state ^= state >> 7;
			state ^= state << 17;
			let wide = ((state as u128) << 64) | (state.rotate_left(29) as u128);
			// Vary the magnitude, so small and large values are both covered.
			wide >> (i % 128)
		})
	}

	#[test]
	fn parse_decimal_works() {
		assert_eq!(parse_decimal("4.206"), Some(p(4, 206)));
		assert_eq!(parse_decimal("4.20600000"), Some(p(4, 206)));
		assert_eq!(parse_decimal("12"), Some(p(12, 0)));
		assert_eq!(parse_decimal("0012.5"), Some(p(12, 500)));
		assert_eq!(parse_decimal(".5"), Some(p(0, 500)));
		assert_eq!(parse_decimal("1."), Some(p(1, 0)));
		assert_eq!(parse_decimal("0"), Some(Price::zero()));
		assert_eq!(parse_decimal("0.000"), Some(Price::zero()));
		assert_eq!(parse_decimal("1e-3"), Some(p(0, 1)));
		assert_eq!(parse_decimal("1e+3"), Some(p(1_000, 0)));
		assert_eq!(parse_decimal("4.206E2"), Some(p(420, 600)));
		assert_eq!(parse_decimal("4206e-3"), Some(p(4, 206)));
		assert_eq!(parse_decimal("0.4206e1"), Some(p(4, 206)));
		assert_eq!(parse_decimal("1e-18"), Some(Price::from_inner(1)));
		assert_eq!(parse_decimal("0.000000000000000001"), Some(Price::from_inner(1)));
		// Truncation past 18 places.
		assert_eq!(parse_decimal("1e-19"), Some(Price::zero()));
		assert_eq!(parse_decimal("0.0000000000000000019"), Some(Price::from_inner(1)));
		assert_eq!(
			parse_decimal("4.2069999999999999999999"),
			Some(Price::from_inner(4_206_999_999_999_999_999))
		);
		// Largest representable value.
		assert_eq!(
			parse_decimal("340282366920938463463.374607431768211455"),
			Some(Price::from_inner(u128::MAX))
		);
	}

	#[test]
	fn parse_decimal_rejects_garbage() {
		for s in [
			"", ".", "-1", "+1", "1.2.3", "abc", "1,5", " 1", "1 ", "1e", "e3", "1e1.5", "1e+",
			"0x10", "NaN", "inf", "1_000", "１", "1.-5", "--1", "1e--3",
		] {
			assert_eq!(parse_decimal(s), None, "{s:?}");
		}
	}

	#[test]
	fn parse_decimal_rejects_values_that_do_not_fit() {
		assert_eq!(parse_decimal("340282366920938463463.374607431768211456"), None);
		assert_eq!(parse_decimal("1e21"), None);
		assert_eq!(parse_decimal("1e30"), None);
		assert_eq!(parse_decimal("999999999999999999999999"), None);
	}

	#[test]
	fn parse_decimal_survives_hostile_exponents_and_lengths() {
		// Exponents at the edge of `i32` neither overflow nor loop.
		assert_eq!(parse_decimal("1e2147483647"), None);
		assert_eq!(parse_decimal("1e-2147483648"), Some(Price::zero()));
		assert_eq!(parse_decimal("0e2147483647"), Some(Price::zero()));
		assert_eq!(parse_decimal("0.0e-2147483648"), Some(Price::zero()));
		assert_eq!(parse_decimal("1e2147483648"), None); // does not parse as i32
												   // Long runs of zeros.
		let zeros = "0".repeat(10_000);
		assert_eq!(parse_decimal(&zeros), Some(Price::zero()));
		assert_eq!(parse_decimal(&alloc::format!("0.{zeros}")), Some(Price::zero()));
		assert_eq!(parse_decimal(&alloc::format!("{zeros}4.2")), Some(p(4, 200)));
		assert_eq!(parse_decimal(&alloc::format!("4.2{zeros}")), Some(p(4, 200)));
		assert_eq!(parse_decimal(&alloc::format!("0.{zeros}1")), Some(Price::zero()));
		assert_eq!(parse_decimal(&alloc::format!("1{zeros}")), None);
	}

	#[test]
	fn parse_decimal_round_trips_the_canonical_form() {
		for inner in pseudo_random_inners(5_000).chain([0, 1, ACC - 1, ACC, ACC + 1, u128::MAX]) {
			let s = alloc::format!("{}.{:018}", inner / ACC, inner % ACC);
			assert_eq!(parse_decimal(&s), Some(Price::from_inner(inner)), "{s}");
		}
	}

	#[test]
	fn parse_decimal_handles_every_exponent_position() {
		// The same value written with the decimal point at every possible position.
		for inner in pseudo_random_inners(500).chain([1, ACC, u128::MAX]) {
			let digits = alloc::format!("{inner:039}"); // 39 digits, zero padded
			for point in 0..=digits.len() {
				let (int_part, frac_part) = digits.split_at(point);
				// `digits` with the point at `point` is `inner * 10^(point - 39 + 18)`.
				let exponent = 39 - 18 - point as i64;
				let s = alloc::format!("{int_part}.{frac_part}e{exponent}");
				assert_eq!(parse_decimal(&s), Some(Price::from_inner(inner)), "{s}");
			}
		}
	}
}

#[cfg(test)]
mod median_tests {
	use super::*;

	fn p(units: u128, thousandths: u128) -> Price {
		Price::from_rational(units * 1_000 + thousandths, 1_000)
	}

	#[test]
	fn median_of_nothing_is_none() {
		assert_eq!(median(&mut []), None);
	}

	#[test]
	fn median_of_one_is_that_one() {
		assert_eq!(median(&mut [p(4, 200)]), Some(p(4, 200)));
	}

	#[test]
	fn median_of_odd_count_is_the_middle_one() {
		let mut prices = [p(4, 300), p(4, 100), p(9, 0), p(4, 200), p(0, 1)];
		assert_eq!(median(&mut prices), Some(p(4, 200)));
	}

	#[test]
	fn median_of_even_count_is_the_mean_of_the_middle_two() {
		let mut prices = [p(4, 400), p(4, 100), p(4, 200), p(9, 0)];
		assert_eq!(median(&mut prices), Some(p(4, 300)));
	}

	#[test]
	fn mean_of_two_rounds_down_on_odd_inner_sum() {
		let a = Price::from_inner(3);
		let b = Price::from_inner(4);
		assert_eq!(median(&mut [a, b]), Some(Price::from_inner(3)));
		assert_eq!(
			median(&mut [Price::from_inner(u128::MAX), Price::from_inner(u128::MAX)]),
			Some(Price::from_inner(u128::MAX))
		);
	}
}

#[cfg(test)]
mod market_price_tests {
	use super::*;
	use crate::schema::Level;

	fn p(s: &str) -> Price {
		parse_decimal(s).unwrap()
	}
	fn level(price: &str, amount: &str) -> Level {
		Level { price: p(price), amount: p(amount) }
	}
	fn settings() -> PairSettings {
		PairSettings {
			max_spread: Permill::from_parts(5_000), // 0.5%
			max_trade_age_ms: 300_000,
			impact_size: p("10000"),
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
		assert!(market_price(&book(), 1_000_000, 1_000_000 + 60_000, &settings()).is_ok());
	}

	#[test]
	fn health_checks_reject() {
		let s = settings();
		let now = 1_000_000;
		// Crossed.
		let mut b = book();
		b.bids[0].price = p("4.02");
		assert_eq!(market_price(&b, now, now, &s), Err(HealthError::CrossedBook));
		// Spread 4.00 / 4.03: 2 * 0.03 = 0.06 > 0.005 * 8.03 = 0.04015.
		let mut b = book();
		b.asks[0].price = p("4.03");
		assert_eq!(market_price(&b, now, now, &s), Err(HealthError::SpreadTooWide));
		// Spread 4.00 / 4.02 passes: 0.04 <= 0.005 * 8.02 = 0.0401.
		assert!(market_price(&book(), now, now, &s).is_ok());
		// Stale trades.
		assert_eq!(market_price(&book(), now, now + 300_001, &s), Err(HealthError::StaleTrades));
		// Thin.
		let thin = PairSettings { impact_size: p("1000000"), ..s };
		assert_eq!(market_price(&book(), now, now, &thin), Err(HealthError::BookTooThin));
		// Empty side.
		let mut b = book();
		b.asks.clear();
		assert_eq!(market_price(&b, now, now, &s), Err(HealthError::BookTooThin));
	}
}

#[cfg(test)]
mod aggregate_tests {
	use super::*;

	const DOT_USDT: PairId = PairId(1);
	const DOT_USD: PairId = PairId(2);
	const USDT_USD: PairId = PairId(3);
	const PAIRS: &[PairId] = &[DOT_USDT, DOT_USD, USDT_USD];

	fn conversions(pair: PairId) -> Vec<(PairId, PairId)> {
		if pair == DOT_USD {
			vec![(DOT_USDT, USDT_USD)]
		} else {
			vec![]
		}
	}
	fn p(s: &str) -> Price {
		parse_decimal(s).unwrap()
	}
	fn v(n: u32) -> VenueId {
		VenueId(n)
	}
	fn quote(quotes: &[Quote], pair: PairId) -> Option<Price> {
		quotes.iter().find(|q| q.pair == pair).map(|q| q.price)
	}

	#[test]
	fn direct_pairs_take_the_median_over_venues() {
		let quotes = aggregate(
			vec![(v(1), DOT_USDT, p("4.0")), (v(2), DOT_USDT, p("4.2")), (v(3), DOT_USDT, p("9"))],
			PAIRS,
			conversions,
		);
		assert_eq!(quote(&quotes, DOT_USDT), Some(p("4.2")));
		assert_eq!(quote(&quotes, DOT_USD), None);
		assert_eq!(quote(&quotes, USDT_USD), None);
	}

	#[test]
	fn derived_pair_pools_direct_and_converted_votes() {
		let quotes = aggregate(
			vec![
				(v(1), DOT_USDT, p("4.0")),
				(v(2), DOT_USDT, p("4.0")),
				(v(3), DOT_USD, p("5.0")),
				(v(4), USDT_USD, p("0.5")),
				(v(5), USDT_USD, p("0.5")),
			],
			PAIRS,
			conversions,
		);
		// Pool: 4.0 * 0.5, 4.0 * 0.5, 5.0 -> median 2.0.
		assert_eq!(quote(&quotes, DOT_USD), Some(p("2.0")));
		assert_eq!(quote(&quotes, USDT_USD), Some(p("0.5")));
	}

	#[test]
	fn no_rate_means_no_conversion() {
		let quotes = aggregate(
			vec![(v(1), DOT_USDT, p("4.0")), (v(3), DOT_USD, p("5.0"))],
			PAIRS,
			conversions,
		);
		// Only the direct DOT/USD vote remains.
		assert_eq!(quote(&quotes, DOT_USD), Some(p("5.0")));
	}

	#[test]
	fn one_vote_per_venue_prefers_the_direct_market() {
		let quotes = aggregate(
			vec![(v(1), DOT_USDT, p("4.0")), (v(1), DOT_USD, p("7.0")), (v(2), USDT_USD, p("1.0"))],
			PAIRS,
			conversions,
		);
		// Venue 1 votes 7.0 directly; its converted 4.0 is not a second vote.
		assert_eq!(quote(&quotes, DOT_USD), Some(p("7.0")));
	}

	#[test]
	fn duplicate_markets_of_a_venue_count_once() {
		let quotes = aggregate(
			vec![
				(v(1), DOT_USDT, p("4.0")),
				(v(1), DOT_USDT, p("9.0")),
				(v(2), DOT_USDT, p("5.0")),
			],
			PAIRS,
			conversions,
		);
		// Votes: 4.0 (first of venue 1), 5.0 -> 4.5.
		assert_eq!(quote(&quotes, DOT_USDT), Some(p("4.5")));
	}

	#[test]
	fn unknown_pairs_are_not_reported() {
		let quotes = aggregate(vec![(v(1), PairId(99), p("1"))], PAIRS, conversions);
		assert!(quotes.is_empty());
	}
}
