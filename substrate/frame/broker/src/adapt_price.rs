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

use crate::{CoreIndex, SaleInfoRecord};
use sp_arithmetic::{traits::One, FixedU64};
use sp_runtime::{FixedPointNumber, FixedPointOperand, Saturating};

/// Performance of a past sale.
#[derive(Copy, Clone)]
pub struct SalePerformance<Balance> {
	/// The price at which the last core was sold.
	///
	/// Will be `None` if no cores have been offered.
	pub sellout_price: Option<Balance>,

	/// The minimum price that was achieved in this sale.
	pub end_price: Balance,

	/// The number of cores we want to sell, ideally.
	pub ideal_cores_sold: CoreIndex,

	/// Number of cores which are/have been offered for sale.
	pub cores_offered: CoreIndex,

	/// Number of cores which have been sold; never more than cores_offered.
	pub cores_sold: CoreIndex,
}

/// Result of `AdaptPrice::adapt_price`.
#[derive(Copy, Clone)]
pub struct AdaptedPrices<Balance> {
	/// New minimum price to use.
	pub end_price: Balance,
	/// Price we optimize for.
	pub target_price: Balance,
}

impl<Balance: Copy> SalePerformance<Balance> {
	/// Construct performance via data from a `SaleInfoRecord`.
	pub fn from_sale<BlockNumber>(record: &SaleInfoRecord<Balance, BlockNumber>) -> Self {
		Self {
			sellout_price: record.sellout_price,
			end_price: record.end_price,
			ideal_cores_sold: record.ideal_cores_sold,
			cores_offered: record.cores_offered,
			cores_sold: record.cores_sold,
		}
	}

	#[cfg(test)]
	fn new(sellout_price: Option<Balance>, end_price: Balance) -> Self {
		Self { sellout_price, end_price, ideal_cores_sold: 0, cores_offered: 0, cores_sold: 0 }
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
	/// Based on the previous sale's performance.
	fn adapt_price(performance: SalePerformance<Balance>) -> AdaptedPrices<Balance>;
}

impl<Balance: Copy> AdaptPrice<Balance> for () {
	fn leadin_factor_at(_: FixedU64) -> FixedU64 {
		FixedU64::one()
	}
	fn adapt_price(performance: SalePerformance<Balance>) -> AdaptedPrices<Balance> {
		let price = performance.sellout_price.unwrap_or(performance.end_price);
		AdaptedPrices { end_price: price, target_price: price }
	}
}

/// Simple implementation of `AdaptPrice` with two linear phases.
///
/// One steep one downwards to the target price, which is 1/10 of the maximum price and a more flat
/// one down to the minimum price, which is 1/100 of the maximum price.
pub struct CenterTargetPrice<Balance>(core::marker::PhantomData<Balance>);

impl<Balance: FixedPointOperand> AdaptPrice<Balance> for CenterTargetPrice<Balance> {
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
				end_price: performance.end_price,
				target_price: FixedU64::from(10).saturating_mul_int(performance.end_price),
			}
		};

		let price = FixedU64::from_rational(1, 10).saturating_mul_int(sellout_price);
		let price = if price == Balance::zero() {
			// We could not recover from a price equal 0 ever.
			sellout_price
		} else {
			price
		};

		AdaptedPrices { end_price: price, target_price: sellout_price }
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
				CenterTargetPrice::adapt_price(SalePerformance::new(sellout_price, price));
			}
		}
	}

	#[test]
	fn no_op_sale_is_good() {
		let prices = CenterTargetPrice::adapt_price(SalePerformance::new(None, 1));
		assert_eq!(prices.target_price, 10);
		assert_eq!(prices.end_price, 1);
	}

	#[test]
	fn price_stays_stable_on_optimal_sale() {
		// Check price stays stable if sold at the optimal price:
		let mut performance = SalePerformance::new(Some(1000), 100);
		for _ in 0..10 {
			let prices = CenterTargetPrice::adapt_price(performance);
			performance.sellout_price = Some(1000);
			performance.end_price = prices.end_price;

			assert!(prices.end_price <= 101);
			assert!(prices.end_price >= 99);
			assert!(prices.target_price <= 1001);
			assert!(prices.target_price >= 999);
		}
	}

	#[test]
	fn price_adjusts_correctly_upwards() {
		let performance = SalePerformance::new(Some(10_000), 100);
		let prices = CenterTargetPrice::adapt_price(performance);
		assert_eq!(prices.target_price, 10_000);
		assert_eq!(prices.end_price, 1000);
	}

	#[test]
	fn price_adjusts_correctly_downwards() {
		let performance = SalePerformance::new(Some(100), 100);
		let prices = CenterTargetPrice::adapt_price(performance);
		assert_eq!(prices.target_price, 100);
		assert_eq!(prices.end_price, 10);
	}

	#[test]
	fn price_never_goes_to_zero_and_recovers() {
		// Check price stays stable if sold at the optimal price:
		let sellout_price = 1;
		let mut performance = SalePerformance::new(Some(sellout_price), 1);
		for _ in 0..11 {
			let prices = CenterTargetPrice::adapt_price(performance);
			performance.sellout_price = Some(sellout_price);
			performance.end_price = prices.end_price;

			assert!(prices.end_price <= sellout_price);
			assert!(prices.end_price > 0);
		}
	}

	#[test]
	fn renewal_price_is_correct_on_no_sale() {
		let performance = SalePerformance::new(None, 100);
		let prices = CenterTargetPrice::adapt_price(performance);
		assert_eq!(prices.target_price, 1000);
		assert_eq!(prices.end_price, 100);
	}

	#[test]
	fn renewal_price_is_sell_out() {
		let performance = SalePerformance::new(Some(1000), 100);
		let prices = CenterTargetPrice::adapt_price(performance);
		assert_eq!(prices.target_price, 1000);
	}
}
