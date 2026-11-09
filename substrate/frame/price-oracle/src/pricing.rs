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
//! How markets are priced, and the price arithmetic of the pallet: reading decimals and medians.

use crate::registry::StoredQuery;
use alloc::vec::Vec;
use codec::{Decode, Encode, MaxEncodedLen};
use core::fmt::Debug;
use frame_support::Parameter;
use sp_price_oracle::{market::QueryTag, PairAggregate, PairId, Price};
use sp_runtime::traits::{Member, Zero};

/// How markets are priced, and how their outputs are aggregated into pair prices.
pub trait MarketPricing {
	/// The pricing parameters, stored with each market.
	type Params: Member + Parameter + MaxEncodedLen;

	/// What pricing a market gives.
	type Output: Encode + Decode;

	/// What is kept from one tick to the next, such as the last prices. A node holds it for the
	/// next tick only; it is the default when the node's last tick did not report, and after a
	/// restart.
	type State: Encode + Decode + Default;

	/// Why a market could not be priced.
	type Error: Debug;

	/// Whether `params` are valid. Checked when a market is set.
	fn validate(_params: &Self::Params) -> bool {
		true
	}

	/// The queries to fetch for a market.
	fn queries(params: &Self::Params) -> Vec<StoredQuery>;

	/// Price a market from the responses to its queries, at the node's time `now_ms`.
	fn price(
		params: &Self::Params,
		responses: &[(QueryTag, Vec<u8>)],
		now_ms: u64,
	) -> Result<Self::Output, Self::Error>;

	/// Price the pairs from the outputs of their markets.
	///
	/// `markets` holds the output of every priced market, with the pair the market quotes.
	/// Returns one entry for every pair in `pairs`, priced or not, and the state for the next
	/// tick.
	///
	/// `cross_rates(pair)` lists the `(source, rate)` pairs such that `pair = source * rate`, and
	/// `quorum(pair)` is the pair's market quorum.
	fn aggregate(
		markets: Vec<(PairId, Self::Output)>,
		pairs: &[PairId],
		cross_rates: impl Fn(PairId) -> Vec<(PairId, PairId)>,
		quorum: impl Fn(PairId) -> u32,
		state: Self::State,
	) -> (Vec<PairAggregate>, Self::State);
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
