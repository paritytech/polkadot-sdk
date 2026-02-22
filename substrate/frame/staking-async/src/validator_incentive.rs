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

//! Validator self-stake incentive calculations.
//!
//! This module implements the reward curve for validator self-stake incentives based on
//! the design from https://hackmd.io/@jonasW3F/rkN6BXE2ex
//!
//! The reward weighting follows a piecewise function:
//! - Below optimum (0 ≤ s ≤ T): w(s) = √s
//! - Between optimum and cap (T < s ≤ C): w(s) = √(T + k² × (s - T))
//! - Above cap (s > C): w(s) = √(T + k² × (C - T)) (flat)
//!
//! Where:
//! - T = OptimumSelfStake (target self-stake for highest APY per DOT)
//! - C = HardCapSelfStake (maximum effective self-stake)
//! - k = SelfStakeSlopeFactor (controls discouragement rate, 0-1)

use crate::*;
use sp_arithmetic::helpers_128bit::sqrt;

/// Default implementation of the validator self-stake incentive calculator.
///
/// Implements the sqrt-based piecewise reward curve.
pub struct DefaultValidatorIncentiveCalculator;

impl<Balance> sp_staking::ValidatorIncentiveCalculator<Balance> for DefaultValidatorIncentiveCalculator
where
	Balance: sp_runtime::traits::AtLeast32BitUnsigned + Copy + Into<u128> + From<u128>,
{
	fn calculate_weight(
		self_stake: Balance,
		optimum: Balance,
		cap: Balance,
		slope_factor: Perbill,
	) -> Balance {
		// If self-stake is zero, return zero weight
		if self_stake.is_zero() {
			return Balance::zero();
		}

		// If config is not set (both optimum and cap are zero), return zero weight
		if optimum.is_zero() && cap.is_zero() {
			return Balance::zero();
		}

		// Convert to u128 for sqrt calculation
		let self_stake_u128: u128 = self_stake.into();
		let optimum_u128: u128 = optimum.into();
		let cap_u128: u128 = cap.into();

		// Calculate weight based on piecewise function
		let weight_u128 = if self_stake <= optimum {
			// Below optimum: w(s) = √s
			sqrt(self_stake_u128)
		} else if self_stake <= cap {
			// Between optimum and cap: w(s) = √(T + k² × (s - T))
			let k_squared = slope_factor.square();
			let excess = self_stake_u128.saturating_sub(optimum_u128);
			let k_squared_times_excess = k_squared.mul_floor(excess);
			let arg = optimum_u128.saturating_add(k_squared_times_excess);
			sqrt(arg)
		} else {
			// Above cap: w(s) = √(T + k² × (C - T)) (plateau)
			let k_squared = slope_factor.square();
			let excess = cap_u128.saturating_sub(optimum_u128);
			let k_squared_times_excess = k_squared.mul_floor(excess);
			let arg = optimum_u128.saturating_add(k_squared_times_excess);
			sqrt(arg)
		};

		// Convert back to Balance
		Balance::from(weight_u128)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_runtime::Perbill;
	use sp_staking::ValidatorIncentiveCalculator;

	#[test]
	fn zero_self_stake_returns_zero() {
		let weight = DefaultValidatorIncentiveCalculator::calculate_weight(
			0u128,
			30_000u128,
			100_000u128,
			Perbill::from_rational(1u32, 2u32),
		);
		assert_eq!(weight, 0);
	}

	#[test]
	fn zero_config_returns_zero() {
		let weight = DefaultValidatorIncentiveCalculator::calculate_weight(
			50_000u128,
			0u128,
			0u128,
			Perbill::from_rational(1u32, 2u32),
		);
		assert_eq!(weight, 0);
	}

	#[test]
	fn below_optimum_range_sqrt() {
		// Test: w(s) = √s for s < optimum
		// s = 10,000 → w = √10,000 = 100
		let weight = DefaultValidatorIncentiveCalculator::calculate_weight(
			10_000u128,
			30_000u128,
			100_000u128,
			Perbill::from_rational(1u32, 2u32),
		);
		assert_eq!(weight, 100);

		// s = 25,000 → w = √25,000 ≈ 158
		let weight = DefaultValidatorIncentiveCalculator::calculate_weight(
			25_000u128,
			30_000u128,
			100_000u128,
			Perbill::from_rational(1u32, 2u32),
		);
		assert_eq!(weight, 158);
	}

	#[test]
	fn at_optimum_threshold() {
		// At exactly optimum: w(30,000) = √30,000 ≈ 173
		let weight = DefaultValidatorIncentiveCalculator::calculate_weight(
			30_000u128,
			30_000u128,
			100_000u128,
			Perbill::from_rational(1u32, 2u32),
		);
		assert_eq!(weight, 173);
	}

	#[test]
	fn between_optimum_and_cap_range() {
		// Test: w(s) = √(T + k² × (s - T)) for optimum < s ≤ cap
		// s = 50,000, T = 30,000, k = 0.5
		// excess = 20,000
		// k² = 0.25
		// k² × excess = 0.25 × 20,000 = 5,000
		// arg = 30,000 + 5,000 = 35,000
		// w = √35,000 ≈ 187
		let weight = DefaultValidatorIncentiveCalculator::calculate_weight(
			50_000u128,
			30_000u128,
			100_000u128,
			Perbill::from_rational(1u32, 2u32),
		);
		assert_eq!(weight, 187);
	}

	#[test]
	fn at_cap_threshold() {
		// At exactly cap: w(100,000) = √(T + k² × (C - T))
		// T = 30,000, C = 100,000, k = 0.5
		// excess = 70,000
		// k² = 0.25
		// k² × excess = 0.25 × 70,000 = 17,500
		// arg = 30,000 + 17,500 = 47,500
		// w = √47,500 ≈ 217
		let weight = DefaultValidatorIncentiveCalculator::calculate_weight(
			100_000u128,
			30_000u128,
			100_000u128,
			Perbill::from_rational(1u32, 2u32),
		);
		assert_eq!(weight, 217);
	}

	#[test]
	fn above_cap_plateau() {
		// Above cap, weight should plateau at the cap value
		// Both should return the same weight as at_cap_threshold
		let weight_at_cap = DefaultValidatorIncentiveCalculator::calculate_weight(
			100_000u128,
			30_000u128,
			100_000u128,
			Perbill::from_rational(1u32, 2u32),
		);

		let weight_above_cap = DefaultValidatorIncentiveCalculator::calculate_weight(
			150_000u128,
			30_000u128,
			100_000u128,
			Perbill::from_rational(1u32, 2u32),
		);

		assert_eq!(weight_at_cap, weight_above_cap);
		assert_eq!(weight_above_cap, 217);
	}

	#[test]
	fn different_slope_factors() {
		// Test with k = 0 (maximum discouragement)
		let weight_k0 = DefaultValidatorIncentiveCalculator::calculate_weight(
			50_000u128,
			30_000u128,
			100_000u128,
			Perbill::from_percent(0),
		);
		// k² = 0, so arg = 30,000 + 0 = 30,000
		// w = √30,000 ≈ 173
		assert_eq!(weight_k0, 173);

		// Test with k = 1 (no discouragement)
		let weight_k1 = DefaultValidatorIncentiveCalculator::calculate_weight(
			50_000u128,
			30_000u128,
			100_000u128,
			Perbill::from_percent(100),
		);
		// k² = 1, so arg = 30,000 + 20,000 = 50,000
		// w = √50,000 ≈ 223
		assert_eq!(weight_k1, 223);

		// k=0 should give lower weight than k=1 (more discouragement)
		assert!(weight_k0 < weight_k1);
	}
}
