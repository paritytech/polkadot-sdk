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

//! The execution band: market intervals into pair prices.
//!
//! The band of a pair is the median of its markets' lower edges and the median of their upper
//! edges, taken separately. A pair's price is its previous price if that is inside the band,
//! otherwise the nearest edge of the band. Without a previous price, it is the middle of the
//! band.

use super::Interval;
use crate::pricing::median;
use alloc::{collections::BTreeMap, format, string::String, vec::Vec};
use sp_price_oracle::{PairAggregate, PairId, Price};
use sp_runtime::{traits::CheckedMul, FixedPointNumber};

/// The last price of every pair that had one.
pub type Prices = BTreeMap<PairId, Price>;

/// The price of every pair in `pairs` from the intervals of its markets, and the prices for the
/// next tick.
///
/// A pair is priced from its own markets and from converted ones. For a cross rate
/// `(source, rate)`, such as `(DOT/USDT, USDT/USD)` for DOT/USD, each `source` market is
/// converted by multiplying both its edges by this tick's price of `rate`. Pairs without cross
/// rates are priced first, so they can be rates. If a rate has no price in this tick, its
/// `source` markets are left out.
///
/// A pair has a band if it has at least `quorum(pair)` markets, more than half of them fill each
/// side, and the median lower edge is not above the median upper edge. The price is the previous
/// price if it is inside the band, otherwise the nearest edge of the band. Without a previous
/// price, it is the middle of the band. A pair without a band keeps its previous price for the
/// next tick.
pub fn aggregate(
	markets: Vec<(PairId, Interval)>,
	pairs: &[PairId],
	cross_rates: impl Fn(PairId) -> Vec<(PairId, PairId)>,
	quorum: impl Fn(PairId) -> u32,
	previous: Prices,
) -> (Vec<PairAggregate>, Prices) {
	let mut direct: BTreeMap<PairId, Vec<Interval>> = BTreeMap::new();
	for (pair, interval) in markets {
		direct.entry(pair).or_default().push(interval);
	}

	let price = |pair: PairId, intervals: &[Interval]| {
		price_pair(pair, intervals, quorum(pair), previous.get(&pair).copied())
	};
	let mut aggregates = BTreeMap::new();

	// Pairs priced from their own markets only, such as USDT/USD. They can be rates.
	for &pair in pairs.iter().filter(|&&pair| cross_rates(pair).is_empty()) {
		let intervals = direct.get(&pair).map_or(&[][..], Vec::as_slice);
		aggregates.insert(pair, price(pair, intervals));
	}
	let rates: Prices = aggregates.iter().filter_map(|(pair, a)| Some((*pair, a.price?))).collect();

	// Pairs with cross rates, such as DOT/USD, with the rates priced above.
	for &pair in pairs {
		let cross_rates = cross_rates(pair);
		if cross_rates.is_empty() {
			continue;
		}
		let mut intervals = direct.get(&pair).cloned().unwrap_or_default();
		for (source, rate) in cross_rates {
			let (Some(sources), Some(rate)) = (direct.get(&source), rates.get(&rate)) else {
				continue;
			};
			intervals.extend(sources.iter().filter_map(|interval| convert(interval, *rate)));
		}
		aggregates.insert(pair, price(pair, &intervals));
	}

	// Pairs without a band keep their previous price.
	let next = pairs
		.iter()
		.filter_map(|pair| {
			let price = aggregates.get(pair).and_then(|a| a.price).or(previous.get(pair).copied());
			Some((*pair, price?))
		})
		.collect();
	let aggregates = pairs.iter().filter_map(|pair| aggregates.remove(pair)).collect();
	(aggregates, next)
}

/// `interval` with both edges multiplied by `rate`. `None` if an edge overflows.
fn convert(interval: &Interval, rate: Price) -> Option<Interval> {
	let edge = |edge: Option<Price>| match edge {
		Some(price) => price.checked_mul(&rate).map(Some),
		None => Some(None),
	};
	Some(Interval { lower: edge(interval.lower)?, upper: edge(interval.upper)? })
}

/// The band and price of `pair` from its `intervals`.
fn price_pair(
	pair: PairId,
	intervals: &[Interval],
	quorum: u32,
	previous: Option<Price>,
) -> PairAggregate {
	let markets = intervals.len();
	// The filled edges, from the middle of the book outwards: the highest lower edge first, the
	// lowest upper edge first. An unfilled edge is further out than any filled one, so it would
	// come after them.
	let mut lowers: Vec<Price> = intervals.iter().filter_map(|i| i.lower).collect();
	let mut uppers: Vec<Price> = intervals.iter().filter_map(|i| i.upper).collect();
	// Highest first.
	lowers.sort_unstable_by(|a, b| b.cmp(a));
	// Lowest first.
	uppers.sort_unstable();
	let filled = format!("bids {}/{markets}, asks {}/{markets}", lowers.len(), uppers.len());
	let aggregate = |price: Option<Price>, note: String| PairAggregate {
		pair,
		inputs: markets as u32,
		quorum,
		price,
		note: note.into_bytes(),
	};

	if (markets as u32) < quorum {
		return aggregate(None, format!("markets {markets}/{quorum}"));
	}
	if lowers.len() * 2 <= markets || uppers.len() * 2 <= markets {
		return aggregate(None, format!("{filled}, need over half"));
	}
	// More than half filled, so the middle edges are filled.
	let (Some(lower), Some(upper)) = (median_edge(&lowers, markets), median_edge(&uppers, markets))
	else {
		return aggregate(None, format!("{filled}, no median"));
	};
	let band = format!("L {}, U {}, {filled}", decimal(lower), decimal(upper));
	if lower > upper {
		return aggregate(None, format!("{band}, crossed"));
	}
	let price = match previous {
		Some(previous) => previous.clamp(lower, upper),
		None => match median(&mut [lower, upper]) {
			Some(middle) => middle,
			None => return aggregate(None, band),
		},
	};
	aggregate(Some(price), band)
}

/// The median of `count` edges.
///
/// `filled` holds the filled ones: lower edges highest first, or upper edges lowest first. The
/// other edges are unfilled: an unfilled lower edge is below every filled one, and an unfilled
/// upper edge above, so they come after all of `filled`.
///
/// For an even count, the median is the mean of the two middle edges. `None` if a middle edge is
/// unfilled.
fn median_edge(filled: &[Price], count: usize) -> Option<Price> {
	// The positions of the two middle edges; the same one for an odd count.
	let first_middle = count.checked_sub(1)? / 2;
	let second_middle = count / 2;
	if first_middle == second_middle {
		return filled.get(first_middle).copied();
	}
	median(&mut [*filled.get(first_middle)?, *filled.get(second_middle)?])
}

/// `price` in decimal, for the logs. `Debug` of a fixed point number prints nothing in a runtime.
fn decimal(price: Price) -> String {
	// The inner value of one, and its number of decimal places.
	let unit = Price::DIV;
	let places = unit.ilog10() as usize;
	let inner = price.into_inner();
	let fraction = format!("{:0places$}", inner % unit);
	format!("{}.{}", inner / unit, fraction.trim_end_matches('0'))
		.trim_end_matches('.')
		.into()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::pricing::parse_decimal;

	const DOT_USDT: PairId = PairId(1);
	const DOT_USD: PairId = PairId(2);
	const USDT_USD: PairId = PairId(3);
	const PAIRS: &[PairId] = &[DOT_USDT, DOT_USD, USDT_USD];

	fn p(s: &str) -> Price {
		parse_decimal(s).unwrap()
	}
	/// An interval; `-` is an edge that does not fill.
	fn i(lower: &str, upper: &str) -> Interval {
		let edge = |s: &str| (s != "-").then(|| p(s));
		Interval { lower: edge(lower), upper: edge(upper) }
	}
	fn cross_rates(pair: PairId) -> Vec<(PairId, PairId)> {
		if pair == DOT_USD {
			vec![(DOT_USDT, USDT_USD)]
		} else {
			vec![]
		}
	}
	/// Aggregate `markets` of DOT/USD alone, quorum 1.
	fn dot_usd(markets: &[Interval], previous: Option<&str>) -> PairAggregate {
		let markets = markets.iter().map(|m| (DOT_USD, *m)).collect();
		let previous = previous.map(|s| (DOT_USD, p(s))).into_iter().collect();
		let (mut aggregates, _) = aggregate(markets, &[DOT_USD], |_| vec![], |_| 1, previous);
		aggregates.remove(0)
	}

	#[test]
	fn band_is_the_median_of_each_edge() {
		// The example of the design: E has no bids.
		let markets = [
			i("1.1487", "1.1513"),
			i("1.1480", "1.1520"),
			i("1.1490", "1.1510"),
			i("1.1470", "1.1540"),
			i("-", "1.1530"),
		];
		let a = dot_usd(&markets, None);
		// L 1.1480, U 1.1520: the price starts in the middle.
		assert_eq!(a.price, Some(p("1.15")));
		assert_eq!(a.note, b"L 1.148, U 1.152, bids 4/5, asks 5/5".to_vec());
		assert_eq!((a.inputs, a.quorum), (5, 1));
	}

	#[test]
	fn even_count_takes_the_mean_of_the_middle_two() {
		let markets = [i("1.10", "1.20"), i("1.12", "1.22"), i("1.14", "1.24"), i("-", "1.26")];
		// Lowers -, 1.10, 1.12, 1.14: L = 1.11. Uppers 1.20, 1.22, 1.24, 1.26: U = 1.23.
		assert_eq!(dot_usd(&markets, None).price, Some(p("1.17")));
	}

	#[test]
	fn previous_price_moves_into_the_band() {
		let markets = [i("1.148", "1.152")];
		// Inside: unchanged. Above: to U. Below: to L.
		assert_eq!(dot_usd(&markets, Some("1.150")).price, Some(p("1.150")));
		assert_eq!(dot_usd(&markets, Some("1.160")).price, Some(p("1.152")));
		assert_eq!(dot_usd(&markets, Some("1.140")).price, Some(p("1.148")));
	}

	#[test]
	fn ticks_of_the_design() {
		// The ticks of the design's table, each with the previous tick's price.
		let mut previous = Prices::new();
		for (lower, upper, want) in [
			("1.148", "1.152", "1.150"),
			("1.146", "1.151", "1.150"),
			("1.138", "1.144", "1.144"),
			("1.128", "1.134", "1.134"),
			("1.136", "1.142", "1.136"),
		] {
			let markets = vec![(DOT_USD, i(lower, upper))];
			let (aggregates, next) = aggregate(markets, &[DOT_USD], |_| vec![], |_| 1, previous);
			assert_eq!(aggregates[0].price, Some(p(want)));
			previous = next;
		}
	}

	#[test]
	fn more_than_half_must_fill_each_side() {
		// 3 of 5 fill both sides: a band, from the filled ones.
		let markets = [i("1.0", "1.2"), i("1.1", "1.3"), i("1.2", "1.4"), i("-", "-"), i("-", "-")];
		let a = dot_usd(&markets, None);
		assert_eq!(a.note, b"L 1, U 1.4, bids 3/5, asks 3/5".to_vec());
		// 2 of 4 is not more than half.
		let markets = [i("1.0", "1.2"), i("1.1", "1.3"), i("-", "1.4"), i("-", "1.5")];
		let a = dot_usd(&markets, None);
		assert_eq!(a.price, None);
		assert_eq!(a.note, b"bids 2/4, asks 4/4, need over half".to_vec());
	}

	#[test]
	fn quorum_counts_markets_that_fill_nothing() {
		let markets = vec![(DOT_USD, i("1.0", "1.2")), (DOT_USD, i("-", "-"))];
		let (a, _) = aggregate(markets.clone(), &[DOT_USD], |_| vec![], |_| 3, Prices::new());
		assert_eq!((a[0].price, a[0].note.clone()), (None, b"markets 2/3".to_vec()));
		let (a, _) = aggregate(markets, &[DOT_USD], |_| vec![], |_| 2, Prices::new());
		// Two markets pass the quorum, but one filled side of two is not more than half.
		assert_eq!(a[0].price, None);
	}

	#[test]
	fn no_markets_no_price() {
		let a = dot_usd(&[], None);
		assert_eq!((a.price, a.note), (None, b"markets 0/1".to_vec()));
	}

	#[test]
	fn cross_rate_converts_both_edges_with_the_rate_price() {
		let markets = vec![
			(USDT_USD, i("0.998", "1.002")),
			(DOT_USDT, i("4.00", "4.02")),
			(DOT_USD, i("3.99", "4.03")),
		];
		let (aggregates, next) = aggregate(markets, PAIRS, cross_rates, |_| 1, Prices::new());
		// USDT/USD starts at 1.000. DOT/USD: 3.99 and 4.00, 4.02 and 4.03: L 3.995, U 4.025.
		assert_eq!(aggregates[2].price, Some(p("1")));
		assert_eq!(aggregates[1].price, Some(p("4.01")));
		assert_eq!(aggregates[1].inputs, 2);
		// Every pair keeps its price, in the order of `pairs`.
		assert_eq!(aggregates.iter().map(|a| a.pair).collect::<Vec<_>>(), PAIRS.to_vec());
		assert_eq!(next.len(), 3);
	}

	#[test]
	fn rate_uses_its_price_of_this_tick() {
		// USDT/USD was 1.001; its band 0.998 to 1.000 moves it to 1.000, and that converts.
		let markets = vec![(USDT_USD, i("0.998", "1.000")), (DOT_USDT, i("4.00", "4.00"))];
		let previous = [(USDT_USD, p("1.001"))].into_iter().collect();
		let (aggregates, _) = aggregate(markets, PAIRS, cross_rates, |_| 1, previous);
		assert_eq!(aggregates[1].price, Some(p("4.00")));
	}

	#[test]
	fn no_rate_leaves_out_the_markets_it_converts() {
		// USDT/USD has no markets, so DOT/USDT does not count towards DOT/USD.
		let markets = vec![(DOT_USDT, i("4.00", "4.02")), (DOT_USD, i("3.99", "4.03"))];
		let (aggregates, _) = aggregate(markets, PAIRS, cross_rates, |_| 1, Prices::new());
		assert_eq!(aggregates[1].inputs, 1);
		assert_eq!(aggregates[1].price, Some(p("4.01")));
	}

	#[test]
	fn pair_without_a_band_keeps_its_previous_price() {
		let previous: Prices = [(DOT_USD, p("4.00"))].into_iter().collect();
		let (aggregates, next) = aggregate(vec![], &[DOT_USD], |_| vec![], |_| 1, previous);
		assert_eq!(aggregates[0].price, None);
		assert_eq!(next.get(&DOT_USD), Some(&p("4.00")));
		// Unregistered pairs are forgotten.
		let previous: Prices = [(PairId(99), p("1"))].into_iter().collect();
		let (_, next) = aggregate(vec![], &[DOT_USD], |_| vec![], |_| 1, previous);
		assert!(next.is_empty());
	}

	#[test]
	fn crossed_band_has_no_price() {
		// Converted with a rate far off, the edges of two pools of markets cross.
		let markets = [i("1.30", "1.40"), i("1.31", "1.41"), i("1.00", "1.10")];
		// Lowers 1.00, 1.30, 1.31: L 1.30. Uppers 1.10, 1.40, 1.41: U 1.40. Not crossed.
		assert_eq!(dot_usd(&markets, None).price, Some(p("1.35")));
		// One market with its lower edge above its upper one cannot come from a book, but is
		// still caught.
		let a = dot_usd(&[i("1.2", "1.1")], None);
		assert_eq!(a.price, None);
		assert!(a.note.ends_with(b"crossed"));
	}

	#[test]
	fn median_edge_counts_unfilled_edges_as_furthest_out() {
		// (filled edges from the middle outwards, count of all edges, median)
		let cases: &[(&[&str], usize, Option<&str>)] = &[
			// No markets.
			(&[], 0, None),
			// One market.
			(&["1.0"], 1, Some("1.0")),
			(&[], 1, None),
			// Two markets: the mean of both, so both must fill.
			(&["1.148", "1.146"], 2, Some("1.147")),
			(&["1.148"], 2, None),
			// Three markets: the second, so one may be unfilled.
			(&["3", "2", "1"], 3, Some("2")),
			(&["3", "2"], 3, Some("2")),
			(&["3"], 3, None),
			// Four markets: the mean of the second and third, so one may be unfilled.
			(&["4", "3", "2", "1"], 4, Some("2.5")),
			(&["4", "3", "2"], 4, Some("2.5")),
			(&["4", "3"], 4, None),
			// Five markets: the third, so two may be unfilled.
			(&["5", "4", "3", "2", "1"], 5, Some("3")),
			(&["5", "4", "3"], 5, Some("3")),
			(&["5", "4"], 5, None),
			// Six markets: the mean of the third and fourth, so two may be unfilled.
			(&["6", "5", "4", "3"], 6, Some("3.5")),
			(&["6", "5", "4"], 6, None),
		];
		for (filled, count, want) in cases {
			let filled: Vec<Price> = filled.iter().map(|s| p(s)).collect();
			assert_eq!(
				median_edge(&filled, *count),
				want.map(p),
				"{} filled of {count}",
				filled.len()
			);
		}
	}

	#[test]
	fn decimal_prints_the_price() {
		assert_eq!(decimal(p("4.01")), "4.01");
		assert_eq!(decimal(p("4")), "4");
		assert_eq!(decimal(p("0.000000000000000001")), "0.000000000000000001");
	}
}
