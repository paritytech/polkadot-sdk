// This file is part of Substrate.

// Copyright (C) Amforc AG.
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

//! Price decay curves for Dutch auctions.
//!
//! This module provides the price curve that determines how auction
//! prices decrease over time. Based on MakerDAO Liquidation 2.0 design.

use frame_support::pallet_prelude::*;
use sp_runtime::{traits::Saturating, FixedPointNumber, FixedU128};

/// Price decay curve for Dutch auctions.
///
/// Stored in `AuctionConfig` and adjustable via governance.
/// The curve enforces a minimum price floor relative to the starting price.
#[derive(
	Clone,
	Copy,
	Debug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum PriceCurve {
	/// Slowed exponential decrease curve (O(1), non-recursive).
	///
	/// Uses a cubic polynomial that approximates slowed exponential decay behavior:
	/// slow near oracle price, faster far from it.
	///
	/// With δ (`center_ratio`), ε (`linear_coeff`), S (`scale_factor`), T (`center`):
	///
	/// ```text
	/// max { -oracle_price×(buffer-1)/S³ × (t-T)³ - oracle_price×ε × (t-T) + oracle_price×δ,
	///       starting_price × minimum_price }
	/// ```
	///
	/// The curve:
	/// - Starts above oracle price (when buffer > 1)
	/// - Inflects around the center block T (where price ≈ `oracle_price` × δ)
	/// - Decays faster far from center, slower near center
	/// - Respects the minimum price floor
	SlowedExponentialDecrease {
		/// Block number where price curve inflects (e.g., 10).
		/// At this point, price ≈ `oracle_price` × `center_ratio`.
		center: u64,
		/// Divisor for cubic coefficient (e.g., 1000). Higher = flatter curve.
		/// Controls how steep the price change is far from the center.
		scale_factor: FixedU128,
		/// Linear term coefficient (e.g., 0.0065). Controls slope at center.
		linear_coeff: FixedU128,
		/// Price ratio at center relative to `oracle_price` (e.g., 0.99).
		/// `center_price` = `oracle_price` × `center_ratio`
		center_ratio: FixedU128,
		/// Minimum price as ratio of `starting_price` (e.g., 0.65).
		minimum_price: FixedU128,
	},
}

impl Default for PriceCurve {
	fn default() -> Self {
		// Default: SlowedExponentialDecrease with DESIGNv5 recommended parameters
		Self::SlowedExponentialDecrease {
			center: 10,
			scale_factor: FixedU128::from(10),                // S = 10
			linear_coeff: FixedU128::from_rational(1, 1000),  // ε = 0.001
			center_ratio: FixedU128::from_rational(99, 100),  // δ = 0.99
			minimum_price: FixedU128::from_rational(65, 100), // 0.65 floor
		}
	}
}

impl PriceCurve {
	/// Calculate current price given starting price, buffer, and elapsed blocks.
	///
	/// The buffer is passed from `AuctionConfig.buffer` to derive `oracle_price`.
	/// Returns the price after `elapsed` blocks, respecting the minimum price floor.
	pub fn calculate_price(
		&self,
		starting_price: FixedU128,
		buffer: FixedU128,
		elapsed: u64,
	) -> FixedU128 {
		match self {
			Self::SlowedExponentialDecrease {
				center,
				scale_factor,
				linear_coeff,
				center_ratio,
				minimum_price,
			} => Self::calculate_slowed_exponential_decrease(
				starting_price,
				buffer,
				elapsed,
				*center,
				*scale_factor,
				*linear_coeff,
				*center_ratio,
				*minimum_price,
			),
		}
	}

	/// `SlowedExponentialDecrease`: O(1) closed-form price calculation.
	///
	/// Uses cubic polynomial that approximates slowed exponential decay:
	/// ```text
	/// max { -oracle_price×(buffer-1)/S³ × (t-T)³ - oracle_price×ε × (t-T) + oracle_price×δ,
	///       starting_price × minimum_price }
	/// ```
	#[allow(clippy::too_many_arguments)]
	fn calculate_slowed_exponential_decrease(
		starting_price: FixedU128,
		buffer: FixedU128,
		elapsed: u64,
		center: u64,
		scale_factor: FixedU128,
		linear_coeff: FixedU128,
		center_ratio: FixedU128,
		minimum_price: FixedU128,
	) -> FixedU128 {
		let floor = starting_price.saturating_mul(minimum_price);

		// Derive oracle_price from starting_price and buffer
		let oracle_price = if buffer.is_zero() {
			starting_price
		} else {
			starting_price.checked_div(&buffer).unwrap_or(starting_price)
		};

		// x = elapsed - center (signed offset from center)
		// We handle sign separately to avoid signed arithmetic
		let (x_abs, x_negative) =
			if elapsed >= center { (elapsed - center, false) } else { (center - elapsed, true) };

		// a = oracle_price × (buffer - 1) / scale_factor³
		let buffer_minus_one = buffer.saturating_sub(FixedU128::one());
		let a_numerator = oracle_price.saturating_mul(buffer_minus_one);
		let s_cubed = scale_factor.saturating_mul(scale_factor).saturating_mul(scale_factor);
		let a = if s_cubed.is_zero() {
			FixedU128::zero()
		} else {
			a_numerator.checked_div(&s_cubed).unwrap_or_else(FixedU128::zero)
		};

		// b = oracle_price × linear_coeff (scaled by oracle_price per formula)
		let b = oracle_price.saturating_mul(linear_coeff);

		// c = oracle_price × center_ratio
		let c = oracle_price.saturating_mul(center_ratio);

		// Compute x³ carefully to avoid overflow
		// x_abs is u64, max ~18×10^18, but x³ could overflow for large values
		// Use saturating multiplication on FixedU128 for safety
		let x_fixed = FixedU128::saturating_from_integer(x_abs);
		let x_squared = x_fixed.saturating_mul(x_fixed);
		let x_cubed = x_squared.saturating_mul(x_fixed);

		// cubic_term = a × x³
		let cubic_term = a.saturating_mul(x_cubed);

		// linear_term = b × x (where b is already scaled by oracle_price)
		let linear_term = b.saturating_mul(x_fixed);

		// Calculate price based on sign of x
		// When x < 0 (before center): price = c + cubic_term + linear_term
		// When x ≥ 0 (at or after center): price = c - cubic_term - linear_term
		let price = if x_negative {
			// Before center: price increases above c
			c.saturating_add(cubic_term).saturating_add(linear_term)
		} else {
			// At or after center: price decreases below c
			c.saturating_sub(cubic_term).saturating_sub(linear_term)
		};

		// Enforce minimum price floor
		price.max(floor)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn example_exp_curve() -> PriceCurve {
		PriceCurve::SlowedExponentialDecrease {
			center: 10,
			scale_factor: FixedU128::from(1000),               // 1000
			linear_coeff: FixedU128::from_rational(65, 10000), // 0.0065
			center_ratio: FixedU128::from_rational(99, 100),   // 0.99
			minimum_price: FixedU128::from_rational(65, 100),  // 0.65
		}
	}

	/// Helper: `starting_price` = 2.7, buffer = 1.2 (oracle = 2.25)
	fn test_params() -> (FixedU128, FixedU128) {
		let starting_price = FixedU128::from_rational(27, 10);
		let buffer = FixedU128::from_rational(12, 10);
		(starting_price, buffer)
	}

	#[test]
	fn default_curve_is_slowed_exponential_decrease() {
		let curve = PriceCurve::default();
		match curve {
			PriceCurve::SlowedExponentialDecrease { center, minimum_price, .. } => {
				assert_eq!(center, 10);
				assert_eq!(minimum_price, FixedU128::from_rational(65, 100));
			},
		}
	}

	#[test]
	fn slowed_exponential_decrease_at_center() {
		let curve = example_exp_curve();
		let (starting_price, buffer) = test_params();

		// At center (t=10), price should equal oracle_price × center_ratio
		// oracle_price = 2.7 / 1.2 = 2.25
		// = 2.25 × 0.99 = 2.2275
		let price = curve.calculate_price(starting_price, buffer, 10);
		let expected = FixedU128::from_rational(22275, 10000);
		assert_eq!(price, expected);
	}

	#[test]
	fn slowed_exponential_decrease_at_start() {
		let curve = example_exp_curve();
		let (starting_price, buffer) = test_params();

		// At t=0, x = -10, price = c + a×(-10)³ + oracle_price×b×10
		// With S³ = 1000³ = 1e9, the cubic term is negligible
		// oracle_price = 2.25, a = 2.25 × 0.2 / 1e9 ≈ 4.5e-10
		// c = 2.25 × 0.99 = 2.2275
		// cubic_term ≈ 0 (negligible with /S³)
		// linear_term = oracle_price × 0.0065 × 10 = 2.25 × 0.065 = 0.14625
		// price ≈ 2.2275 + 0.14625 = 2.37375
		let price = curve.calculate_price(starting_price, buffer, 0);

		// Price at t=0 is below starting_price due to negligible cubic term with /S³
		// Note: cubic term is tiny but not exactly zero: ~4.5e-7
		assert!(price > FixedU128::from_rational(23735, 10000)); // > 2.3735
		assert!(price < FixedU128::from_rational(2374, 1000)); // < 2.374
	}

	#[test]
	fn slowed_exponential_decrease_matches_formula() {
		let curve = example_exp_curve();
		let (starting_price, buffer) = test_params();

		// With /S³, the cubic term is negligible, curve is almost purely linear
		// linear_term per block = oracle_price × linear_coeff = 2.25 × 0.0065 = 0.014625
		// t=5: x = -5, linear_term = 0.073125, price ≈ 2.2275 + 0.0731 = 2.30
		let price_5 = curve.calculate_price(starting_price, buffer, 5);
		assert!(price_5 > FixedU128::from_rational(230, 100));
		assert!(price_5 < FixedU128::from_rational(231, 100));

		// t=10: price should be exactly c = 2.2275
		let price_10 = curve.calculate_price(starting_price, buffer, 10);
		assert_eq!(price_10, FixedU128::from_rational(22275, 10000));

		// t=15: x = 5, price ≈ 2.2275 - 0.0731 = 2.154
		let price_15 = curve.calculate_price(starting_price, buffer, 15);
		assert!(price_15 > FixedU128::from_rational(215, 100));
		assert!(price_15 < FixedU128::from_rational(216, 100));
	}

	#[test]
	fn slowed_exponential_decrease_respects_floor() {
		let curve = example_exp_curve();
		let (starting_price, buffer) = test_params();
		let floor = starting_price.saturating_mul(FixedU128::from_rational(65, 100)); // 1.755

		// After many blocks, price should hit the floor
		let price = curve.calculate_price(starting_price, buffer, 1000);
		assert_eq!(price, floor);
	}

	#[test]
	fn slowed_exponential_decrease_hits_floor_at_expected_time() {
		let curve = example_exp_curve();
		let (starting_price, buffer) = test_params();
		let floor = FixedU128::from_rational(1755, 1000); // 1.755

		// With oracle_price scaling on linear_coeff:
		// linear_term per block = oracle_price × linear_coeff = 2.25 × 0.0065 = 0.014625
		// Price at center (t=10) = 2.2275
		// Floor at 1.755, need to drop 0.4725
		// blocks to hit floor from center = 0.4725 / 0.014625 ≈ 32.3
		// So floor hit around t = 10 + 33 = 43

		// Verify price is still above floor at t=40
		let price_40 = curve.calculate_price(starting_price, buffer, 40);
		assert!(price_40 > floor);

		// And hits floor by t=50 (with some margin)
		let price_50 = curve.calculate_price(starting_price, buffer, 50);
		assert_eq!(price_50, floor);
	}

	#[test]
	fn slowed_exponential_decrease_monotonically_decreases_after_start() {
		let curve = example_exp_curve();
		let (starting_price, buffer) = test_params();

		// Price should decrease monotonically from t=0 onwards
		let mut prev_price = curve.calculate_price(starting_price, buffer, 0);
		for t in 1..=30 {
			let price = curve.calculate_price(starting_price, buffer, t);
			assert!(
				price <= prev_price,
				"Price should decrease: t={}, prev={:?}, curr={:?}",
				t,
				prev_price,
				price
			);
			prev_price = price;
		}
	}

	#[test]
	fn slowed_exponential_decrease_handles_zero_buffer() {
		let curve = example_exp_curve();
		let starting_price = FixedU128::from(100);
		let buffer = FixedU128::zero(); // Edge case: buffer = 0, oracle defaults to starting_price

		// Should not panic, with buffer=0, oracle_price defaults to starting_price
		let price = curve.calculate_price(starting_price, buffer, 10);
		// At center, price = oracle_price × center_ratio = 100 × 0.99 = 99
		assert_eq!(price, FixedU128::from(99));
	}

	#[test]
	fn slowed_exponential_decrease_handles_zero_scale_factor() {
		let curve = PriceCurve::SlowedExponentialDecrease {
			center: 10,
			scale_factor: FixedU128::zero(), // Edge case
			linear_coeff: FixedU128::from_rational(65, 10000),
			center_ratio: FixedU128::from_rational(99, 100),
			minimum_price: FixedU128::from_rational(65, 100),
		};

		// Should not panic, a=0 when scale_factor=0
		let starting_price = FixedU128::from(120);
		let buffer = FixedU128::from_rational(12, 10); // 1.2, oracle = 100
		let price = curve.calculate_price(starting_price, buffer, 10);
		// Only linear term applies after center
		assert!(price < starting_price);
	}

	#[test]
	fn slowed_exponential_decrease_is_o1_complexity() {
		let curve = example_exp_curve();
		let (starting_price, buffer) = test_params();

		// This should complete instantly even for very large elapsed values
		let price = curve.calculate_price(starting_price, buffer, 1_000_000);

		// Should hit floor
		let floor = starting_price.saturating_mul(FixedU128::from_rational(65, 100));
		assert_eq!(price, floor);
	}

	#[test]
	fn slowed_exponential_decrease_different_starting_prices() {
		// Verify the curve scales properly with different starting prices
		let curve = example_exp_curve();
		let buffer = FixedU128::from_rational(12, 10); // 1.2

		// With starting_price = 100 (buffer 1.2 → oracle = 83.33)
		let starting_100 = FixedU128::from(100);
		let price_100 = curve.calculate_price(starting_100, buffer, 10);
		// center_price = 83.33 × 0.99 ≈ 82.5
		assert!(price_100 > FixedU128::from(82));
		assert!(price_100 < FixedU128::from(83));

		// With starting_price = 1000 (buffer 1.2 → oracle = 833.33)
		let starting_1000 = FixedU128::from(1000);
		let price_1000 = curve.calculate_price(starting_1000, buffer, 10);
		// Should be 10× the price_100 value
		assert!(price_1000 > FixedU128::from(820));
		assert!(price_1000 < FixedU128::from(830));
	}

	#[test]
	fn slowed_exponential_decrease_with_different_parameters() {
		// Test with different curve parameters
		let curve = PriceCurve::SlowedExponentialDecrease {
			center: 20,                                      // Later inflection
			scale_factor: FixedU128::from(500),              // Steeper curve
			linear_coeff: FixedU128::from_rational(1, 100),  // 0.01
			center_ratio: FixedU128::from_rational(95, 100), // 0.95
			minimum_price: FixedU128::from_rational(5, 10),  // 0.5 floor
		};

		let starting_price = FixedU128::from(150);
		let buffer = FixedU128::from_rational(15, 10); // 1.5, oracle = 100

		// At center (t=20), price = 100 × 0.95 = 95
		let price_20 = curve.calculate_price(starting_price, buffer, 20);
		assert_eq!(price_20, FixedU128::from(95));

		// Floor = 150 × 0.5 = 75
		let price_far = curve.calculate_price(starting_price, buffer, 1000);
		assert_eq!(price_far, FixedU128::from(75));
	}
}
