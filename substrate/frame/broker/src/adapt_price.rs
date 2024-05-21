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

#![deny(missing_docs)]

use crate::SaleInfoRecord;
use sp_arithmetic::{traits::One, FixedU64};
use sp_runtime::{FixedPointNumber, FixedPointOperand, Saturating};

/// Performance of a past sale.
#[derive(Copy, Clone)]
pub struct SalePerformance<Balance> {
	/// The price at which the last core was sold.
	///
	/// Will be `None` if no cores have been offered.
	pub sellout_price: Option<Balance>,

	/// The base price (lowest possible price) that was used in this sale.
	pub price: Balance,
}

/// Result of `AdaptPrice::adapt_price`.
#[derive(Copy, Clone)]
pub struct AdaptedPrices<Balance> {
	/// New base price to use.
	pub price: Balance,
	/// Price to use for renewals of leases.
	pub renewal_price: Balance,
}

impl<Balance: Copy> SalePerformance<Balance> {
	/// Construct performance via data from a `SaleInfoRecord`.
	pub fn from_sale<BlockNumber>(record: &SaleInfoRecord<Balance, BlockNumber>) -> Self {
		Self { sellout_price: record.sellout_price, price: record.price }
	}
}

/// Type for determining how to set price.
pub trait AdaptPrice<Balance> {
	/// Return the factor by which the regular price must be multiplied during the leadin period.
	///
	/// - `when`: The amount through the leadin period; between zero and one.
	fn leadin_factor_at(when: FixedU64) -> FixedU64;

	/// Return adapted prices for next sale.
	///
	/// Based on this sale's performance.
	fn adapt_price(performance: SalePerformance<Balance>) -> AdaptedPrices<Balance>;
}

impl<Balance: Copy> AdaptPrice<Balance> for () {
	fn leadin_factor_at(_: FixedU64) -> FixedU64 {
		FixedU64::one()
	}
	fn adapt_price(performance: SalePerformance<Balance>) -> AdaptedPrices<Balance> {
		let price = performance.sellout_price.unwrap_or(performance.price);
		AdaptedPrices { price, renewal_price: price }
	}
}

/// Simple implementation of `AdaptPrice` giving a monotonic leadin and a linear price change based
/// on cores sold.
pub struct Linear<Balance>(std::marker::PhantomData<Balance>);

impl<Balance: FixedPointOperand> AdaptPrice<Balance> for Linear<Balance> {
	fn leadin_factor_at(when: FixedU64) -> FixedU64 {
		FixedU64::from(101).saturating_sub(when.saturating_mul(100.into()))
	}

	fn adapt_price(performance: SalePerformance<Balance>) -> AdaptedPrices<Balance> {
		let leadin_max = Self::leadin_factor_at(0.into());
		let leadin_min = Self::leadin_factor_at(1.into());
		let spread = leadin_max.saturating_sub(leadin_min);

		let Some(sellout_price) = performance.sellout_price else {
			return AdaptedPrices {
				price: performance.price,
				renewal_price: spread
					.saturating_add(2.into())
					.div(2.into())
					.saturating_mul_int(performance.price),
			}
		};

		let price = FixedU64::from(2)
			.div(spread.saturating_add(2.into()))
			.saturating_mul_int(sellout_price);

		let price = if price == Balance::zero() {
			// We could not recover from a price equal 0 ever.
			sellout_price
		} else {
			price
		};

		AdaptedPrices { price, renewal_price: sellout_price }
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn linear_no_panic() {
		for sellout in 0..11 {
			for price in 0..10 {
				let sellout_price = if sellout == 11 { None } else { Some(sellout) };
				Linear::adapt_price(SalePerformance { sellout_price, price });
			}
		}
	}

	#[test]
	fn no_op_sale_is_good() {
		let prices = Linear::adapt_price(SalePerformance { sellout_price: None, price: 1 });
		assert_eq!(prices.renewal_price, 51);
		assert_eq!(prices.price, 1);
	}

	#[test]
	fn price_stays_stable_on_optimal_sale() {
		// Check price stays stable if sold at the optimal price:
		let mut performance = SalePerformance { sellout_price: Some(5100), price: 100 };
		for _ in 0..10 {
			let prices = Linear::adapt_price(performance);
			performance.sellout_price = Some(5100);
			performance.price = prices.price;

			assert!(prices.price <= 101);
			assert!(prices.price >= 99);
			assert!(prices.renewal_price <= 5101);
			assert!(prices.renewal_price >= 5099);
		}
	}

	#[test]
	fn price_never_goes_to_zero_and_recovers() {
		// Check price stays stable if sold at the optimal price:
		let sellout_price = 51;
		let mut performance = SalePerformance { sellout_price: Some(sellout_price), price: 1 };
		for _ in 0..11 {
			let prices = Linear::adapt_price(performance);
			performance.sellout_price = Some(sellout_price);
			performance.price = prices.price;

			assert!(prices.price <= sellout_price);
			assert!(prices.price > 0);
		}
	}
	// Using constraints from pallet implementation i.e. `limit >= sold`.
	//     Check extremes
	//     let limit = 10;
	//     let target = 5;

	//     Maximally sold: `sold == limit`
	//     assert_eq!(Linear::adapt_price(limit, target, limit), FixedU64::from_float(2.0));
	//     Ideally sold: `sold == target`
	//     assert_eq!(Linear::adapt_price(target, target, limit), FixedU64::one());
	//     Minimally sold: `sold == 0`
	//     assert_eq!(Linear::adapt_price(0, target, limit), FixedU64::from_float(0.5));
	//     Optimistic target: `target == limit`
	//     assert_eq!(Linear::adapt_price(limit, limit, limit), FixedU64::one());
	//     Pessimistic target: `target == 0`
	//     assert_eq!(Linear::adapt_price(limit, 0, limit), FixedU64::from_float(2.0));
	// }
}
