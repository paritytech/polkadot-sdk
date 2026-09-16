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
//! Price arithmetic of the pallet.

use sp_price_oracle::Price;

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
mod tests {
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
