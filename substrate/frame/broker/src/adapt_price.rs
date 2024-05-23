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
	/// Price we optimize for.
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
		if when <= FixedU64::from_rational(1, 2) {
			FixedU64::from(100).saturating_sub(when.saturating_mul(180.into()))
		} else {
			FixedU64::from(19).saturating_sub(when.saturating_mul(18.into()))
		}
	}

	fn adapt_price(performance: SalePerformance<Balance>) -> AdaptedPrices<Balance> {
		let Some(sellout_price) = performance.sellout_price else {
			return AdaptedPrices {
				price: performance.price,
				renewal_price: FixedU64::from(10).saturating_mul_int(performance.price),
			}
		};

		let price = FixedU64::from_rational(1, 10).saturating_mul_int(sellout_price);
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
		assert_eq!(prices.renewal_price, 10);
		assert_eq!(prices.price, 1);
	}

	#[test]
	fn price_stays_stable_on_optimal_sale() {
		// Check price stays stable if sold at the optimal price:
		let mut performance = SalePerformance { sellout_price: Some(1000), price: 100 };
		for _ in 0..10 {
			let prices = Linear::adapt_price(performance);
			performance.sellout_price = Some(1000);
			performance.price = prices.price;

			assert!(prices.price <= 101);
			assert!(prices.price >= 99);
			assert!(prices.renewal_price <= 1001);
			assert!(prices.renewal_price >= 999);
		}
	}

	#[test]
	fn price_adjusts_correctly_upwards() {
		let performance = SalePerformance { sellout_price: Some(10_000), price: 100 };
		let prices = Linear::adapt_price(performance);
		assert_eq!(prices.renewal_price, 10_000);
		assert_eq!(prices.price, 1000);
	}

	#[test]
	fn price_adjusts_correctly_downwards() {
		let performance = SalePerformance { sellout_price: Some(100), price: 100 };
		let prices = Linear::adapt_price(performance);
		assert_eq!(prices.renewal_price, 100);
		assert_eq!(prices.price, 10);
	}

	#[test]
	fn price_never_goes_to_zero_and_recovers() {
		// Check price stays stable if sold at the optimal price:
		let sellout_price = 1;
		let mut performance = SalePerformance { sellout_price: Some(sellout_price), price: 1 };
		for _ in 0..11 {
			let prices = Linear::adapt_price(performance);
			performance.sellout_price = Some(sellout_price);
			performance.price = prices.price;

			assert!(prices.price <= sellout_price);
			assert!(prices.price > 0);
		}
	}

	#[test]
	fn renewal_price_is_correct_on_no_sale() {
		let performance = SalePerformance { sellout_price: None, price: 100 };
		let prices = Linear::adapt_price(performance);
		assert_eq!(prices.renewal_price, 1000);
		assert_eq!(prices.price, 100);
	}

	#[test]
	fn renewal_price_is_sell_out() {
		let performance = SalePerformance { sellout_price: Some(1000), price: 100 };
		let prices = Linear::adapt_price(performance);
		assert_eq!(prices.renewal_price, 1000);
	}
}
