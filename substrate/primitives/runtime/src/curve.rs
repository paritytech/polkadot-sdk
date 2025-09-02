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

//! Provides utilities for various curves.

use crate::{
	traits::{AtLeast32BitUnsigned, SaturatedConversion},
	FixedU128, Perbill,
};
use core::ops::Sub;
use num_traits::One;
use scale_info::TypeInfo;
use sp_arithmetic::{traits::Saturating, FixedPointNumber};

/// The step type for the stepped curve.
#[derive(PartialEq, Eq, sp_core::RuntimeDebug, TypeInfo, Clone)]
pub enum Step<V> {
	/// Increase the value by a percentage of the current value at each step.
	PctInc(Perbill),
	/// Decrease the value by a percentage of the current value at each step.
	PctDec(Perbill),
	/// Increment by a constant value at each step.
	Add(V),
	/// Decrement by a constant value at each step.
	Subtract(V),
	/// Move towards a desired value by a percentage of the remaining difference at each step.
	///
	/// Step size will be (target_total - current_value) * pct.
	RemainingPct(V, Perbill),
}

/// A stepped curve.
///
/// Steps every `period` from the `initial_value` as defined by `step`.
/// First step from `initial_value` takes place at `start` + `period`.
#[derive(PartialEq, Eq, sp_core::RuntimeDebug, TypeInfo, Clone)]
pub struct SteppedCurve<P, V> {
	/// The starting point for the curve.
	pub start: P,
	/// An optional point at which the curve ends. If `None`, the curve continues indefinitely.
	pub end: Option<P>,
	/// The initial value of the curve at the `start` point.
	pub initial_value: V,
	/// The change to apply at the end of each `period`.
	pub step: Step<V>,
	/// The duration of each step.
	pub period: P,
}

impl<P, V> SteppedCurve<P, V>
where
	P: AtLeast32BitUnsigned + Copy,
	V: AtLeast32BitUnsigned + Copy + From<P>,
{
	/// Creates a new `SteppedCurve`.
	pub fn new(start: P, end: Option<P>, initial_value: V, step: Step<V>, period: P) -> Self {
		Self { start, end, initial_value, step, period }
	}

	/// Returns the magnitude of the step size occuring at the start of this point's period.
	/// If no step has occured, will return 0.
	///
	/// Ex. In period 4, the last step taken was 10 -> 7, it would return 3.
	pub fn last_step_size(&self, point: P) -> V {
		// No step taken yet.
		if point < self.start {
			return V::zero();
		}

		// If the period is zero, the value never changes.
		if self.period.is_zero() {
			return V::zero();
		}

		// Determine the effective point for calculation, capped by the end point if it exists.
		let _effective_point = self.end.map_or(point, |e| point.min(e));

		// Calculate how many full periods have passed.
		let num_periods = (point - self.start) / self.period;

		if num_periods.is_zero() {
			return V::zero();
		}

		// Points for calculating step difference.
		let prev_period_point = self.start + (num_periods - P::one()) * self.period;
		let curr_period_point = self.start + num_periods * self.period;

		// Evaluate the curve at those two points.
		let val_prev = self.evaluate(prev_period_point);
		let val_curr = self.evaluate(curr_period_point);

		if val_curr >= val_prev {
			return val_curr.saturating_sub(val_prev);
		} else {
			return val_prev.saturating_sub(val_curr);
		}
	}

	/// Evaluate the curve at a given point.
	pub fn evaluate(&self, point: P) -> V {
		let initial = self.initial_value;

		// If the point is before the curve starts, return the initial value.
		if point < self.start {
			return initial;
		}

		// If the period is zero, the value never changes.
		if self.period.is_zero() {
			return initial;
		}

		// Determine the effective point for calculation, capped by the end point if it exists.
		let effective_point = self.end.map_or(point, |e| point.min(e));

		// Calculate how many full periods have passed, capped by usize.
		let num_periods = (effective_point - self.start) / self.period;
		let num_periods_usize = num_periods.saturated_into::<usize>();

		if num_periods.is_zero() {
			return initial;
		}

		match self.step {
			Step::Add(step_value) => {
				// Initial_value + num_periods * step_value.
				let total_step = step_value.saturating_mul(num_periods.saturated_into::<V>());
				initial.saturating_add(total_step)
			},
			Step::Subtract(step_value) => {
				// Initial_value - num_periods * step_value.
				let total_step = step_value.saturating_mul(num_periods.saturated_into::<V>());
				initial.saturating_sub(total_step)
			},
			Step::PctInc(percent) => {
				// Initial_value * (1 + percent) ^ num_periods.
				let mut ratio = FixedU128::from(percent);
				ratio = FixedU128::one().saturating_add(ratio);
				let scale = ratio.saturating_pow(num_periods_usize);
				let initial_fp = FixedU128::saturating_from_integer(initial);
				let res = initial_fp.saturating_mul(scale);
				(res.into_inner() / FixedU128::DIV).saturated_into::<V>()
			},
			Step::PctDec(percent) => {
				// Initial_value * (1 - percent) ^ num_periods.
				let mut ratio = FixedU128::from(percent);
				ratio = FixedU128::one().saturating_sub(ratio);
				let scale = ratio.saturating_pow(num_periods_usize);
				let initial_fp = FixedU128::saturating_from_integer(initial);
				let res = initial_fp.saturating_mul(scale);
				(res.into_inner() / FixedU128::DIV).saturated_into::<V>()
			},
			Step::RemainingPct(asymptote, percent) => {
				// asymptote +/- diff(asymptote, initial_value) * (1-percent)^num_periods.
				let ratio = FixedU128::one().saturating_sub(FixedU128::from(percent));
				let scale = ratio.saturating_pow(num_periods_usize);

				let initial_fp = FixedU128::saturating_from_integer(initial);
				let asymptote_fp = FixedU128::saturating_from_integer(asymptote);

				let res = if initial >= asymptote {
					let diff = initial_fp.saturating_sub(asymptote_fp);
					asymptote_fp.saturating_add(diff.saturating_mul(scale))
				} else {
					let diff = asymptote_fp.saturating_sub(initial_fp);
					asymptote_fp.saturating_sub(diff.saturating_mul(scale))
				};

				(res.into_inner() / FixedU128::DIV).saturated_into::<V>()
			},
		}
	}
}

/// Piecewise Linear function in [0, 1] -> [0, 1].
#[derive(PartialEq, Eq, sp_core::RuntimeDebug, TypeInfo)]
pub struct PiecewiseLinear<'a> {
	/// Array of points. Must be in order from the lowest abscissas to the highest.
	pub points: &'a [(Perbill, Perbill)],
	/// The maximum value that can be returned.
	pub maximum: Perbill,
}

fn abs_sub<N: Ord + Sub<Output = N> + Clone>(a: N, b: N) -> N where {
	a.clone().max(b.clone()) - a.min(b)
}

impl<'a> PiecewiseLinear<'a> {
	/// Compute `f(n/d)*d` with `n <= d`. This is useful to avoid loss of precision.
	pub fn calculate_for_fraction_times_denominator<N>(&self, n: N, d: N) -> N
	where
		N: AtLeast32BitUnsigned + Clone,
	{
		let n = n.min(d.clone());

		if self.points.is_empty() {
			return N::zero()
		}

		let next_point_index = self.points.iter().position(|p| n < p.0 * d.clone());

		let (prev, next) = if let Some(next_point_index) = next_point_index {
			if let Some(previous_point_index) = next_point_index.checked_sub(1) {
				(self.points[previous_point_index], self.points[next_point_index])
			} else {
				// There is no previous points, take first point ordinate
				return self.points.first().map(|p| p.1).unwrap_or_else(Perbill::zero) * d
			}
		} else {
			// There is no next points, take last point ordinate
			return self.points.last().map(|p| p.1).unwrap_or_else(Perbill::zero) * d
		};

		let delta_y = multiply_by_rational_saturating(
			abs_sub(n.clone(), prev.0 * d.clone()),
			abs_sub(next.1.deconstruct(), prev.1.deconstruct()),
			// Must not saturate as prev abscissa > next abscissa
			next.0.deconstruct().saturating_sub(prev.0.deconstruct()),
		);

		// If both subtractions are same sign then result is positive
		if (n > prev.0 * d.clone()) == (next.1.deconstruct() > prev.1.deconstruct()) {
			(prev.1 * d).saturating_add(delta_y)
		// Otherwise result is negative
		} else {
			(prev.1 * d).saturating_sub(delta_y)
		}
	}
}

// Compute value * p / q.
// This is guaranteed not to overflow on whatever values nor lose precision.
// `q` must be superior to zero.
fn multiply_by_rational_saturating<N>(value: N, p: u32, q: u32) -> N
where
	N: AtLeast32BitUnsigned + Clone,
{
	let q = q.max(1);

	// Mul can saturate if p > q
	let result_divisor_part = (value.clone() / q.into()).saturating_mul(p.into());

	let result_remainder_part = {
		let rem = value % q.into();

		// Fits into u32 because q is u32 and remainder < q
		let rem_u32 = rem.saturated_into::<u32>();

		// Multiplication fits into u64 as both term are u32
		let rem_part = rem_u32 as u64 * p as u64 / q as u64;

		// Can saturate if p > q
		rem_part.saturated_into::<N>()
	};

	// Can saturate if p > q
	result_divisor_part.saturating_add(result_remainder_part)
}

#[test]
fn stepped_curve_works() {
	// Curve with defined end.
	let curve_with_end = SteppedCurve::new(10u32, Some(20u32), 100u32, Step::Add(100u32), 2u32);
	assert_eq!(curve_with_end.evaluate(20u32), 600u32);
	assert_eq!(curve_with_end.evaluate(30u32), 600u32);
	assert_eq!(curve_with_end.last_step_size(10u32), 0u32);
	assert_eq!(curve_with_end.last_step_size(20u32), 100u32);
	assert_eq!(curve_with_end.last_step_size(22u32), 0u32);
	assert_eq!(curve_with_end.last_step_size(30u32), 0u32);

	// Zero period curve.
	let zero_period_curve = SteppedCurve::new(10u32, None, 100u32, Step::Add(100u32), 0u32);
	assert_eq!(zero_period_curve.evaluate(5u32), 100u32);
	assert_eq!(zero_period_curve.evaluate(11u32), 100u32);
	assert_eq!(zero_period_curve.evaluate(12u32), 100u32);
	assert_eq!(zero_period_curve.evaluate(20u32), 100u32);
	assert_eq!(zero_period_curve.last_step_size(20u32), 0u32);

	// Curve with different types.
	let diff_types_curve = SteppedCurve::new(10u32, None, 100u64, Step::Add(100u64), 2u32);
	assert_eq!(diff_types_curve.evaluate(5u32), 100u64);
	assert_eq!(diff_types_curve.evaluate(11u32), 100u64);
	assert_eq!(diff_types_curve.evaluate(12u32), 200u64);
	assert_eq!(diff_types_curve.evaluate(20u32), 600u64);
	assert_eq!(diff_types_curve.last_step_size(20u32), 100u64);

	// Step::Add.
	let add_curve = SteppedCurve::new(10u32, None, 100u32, Step::Add(100u32), 2u32);
	assert_eq!(add_curve.evaluate(5u32), 100u32);
	assert_eq!(add_curve.evaluate(11u32), 100u32);
	assert_eq!(add_curve.evaluate(12u32), 200u32);
	assert_eq!(add_curve.evaluate(20u32), 600u32);
	assert_eq!(add_curve.evaluate(u32::MAX), u32::MAX);
	assert_eq!(add_curve.last_step_size(11u32), 0u32);
	assert_eq!(add_curve.last_step_size(12u32), 100u32);
	assert_eq!(add_curve.last_step_size(20u32), 100u32);

	// Step::Subtract.
	let subtract_curve = SteppedCurve::new(10u32, None, 1000u32, Step::Subtract(100u32), 2u32);
	assert_eq!(subtract_curve.evaluate(5u32), 1000u32);
	assert_eq!(subtract_curve.evaluate(11u32), 1000u32);
	assert_eq!(subtract_curve.evaluate(12u32), 900u32);
	assert_eq!(subtract_curve.evaluate(20u32), 500u32);
	assert_eq!(subtract_curve.evaluate(u32::MAX), u32::MIN);
	assert_eq!(subtract_curve.last_step_size(11u32), 0u32);
	assert_eq!(subtract_curve.last_step_size(12u32), 100u32);
	assert_eq!(subtract_curve.last_step_size(20u32), 100u32);

	// Step::PctInc.
	let pct_inc_curve =
		SteppedCurve::new(10u32, None, 1000u32, Step::PctInc(Perbill::from_percent(10)), 2u32);
	assert_eq!(pct_inc_curve.evaluate(5u32), 1000u32);
	assert_eq!(pct_inc_curve.evaluate(11u32), 1000u32);
	assert_eq!(pct_inc_curve.evaluate(12u32), 1100u32);
	assert_eq!(pct_inc_curve.evaluate(20u32), 1610u32);
	assert_eq!(pct_inc_curve.evaluate(u32::MAX), u32::MAX);
	assert_eq!(pct_inc_curve.last_step_size(11u32), 0u32);
	assert_eq!(pct_inc_curve.last_step_size(12u32), 100u32);
	assert_eq!(pct_inc_curve.last_step_size(20u32), 146u32);
	assert_eq!(pct_inc_curve.last_step_size(u32::MAX), 0u32);

	// Step::PctDec.
	let pct_dec_curve =
		SteppedCurve::new(10u32, None, 1000u32, Step::PctDec(Perbill::from_percent(10)), 2u32);
	assert_eq!(pct_dec_curve.evaluate(5u32), 1000u32);
	assert_eq!(pct_dec_curve.evaluate(11u32), 1000u32);
	assert_eq!(pct_dec_curve.evaluate(12u32), 900u32);
	assert_eq!(pct_dec_curve.evaluate(20u32), 590u32);
	assert_eq!(pct_dec_curve.evaluate(u32::MAX), u32::MIN);
	assert_eq!(pct_dec_curve.last_step_size(11u32), 0u32);
	assert_eq!(pct_dec_curve.last_step_size(12u32), 100u32);
	assert_eq!(pct_dec_curve.last_step_size(20u32), 66u32);
	assert_eq!(pct_dec_curve.last_step_size(u32::MAX), 0u32);

	// Step::RemainingPct increasing.
	let asymptotic_increasing = SteppedCurve::new(
		10u32,
		None,
		0u32,
		Step::RemainingPct(1000u32, Perbill::from_percent(10)),
		2u32,
	);
	assert_eq!(asymptotic_increasing.evaluate(5u32), 0u32);
	assert_eq!(asymptotic_increasing.evaluate(11u32), 0u32);
	assert_eq!(asymptotic_increasing.evaluate(12u32), 100u32);
	assert_eq!(asymptotic_increasing.evaluate(14u32), 190u32);
	assert_eq!(asymptotic_increasing.evaluate(16u32), 271u32);
	assert_eq!(asymptotic_increasing.evaluate(u32::MAX), 1000u32);
	assert_eq!(asymptotic_increasing.last_step_size(5u32), 0u32);
	assert_eq!(asymptotic_increasing.last_step_size(11u32), 0u32);
	assert_eq!(asymptotic_increasing.last_step_size(12u32), 100u32);
	assert_eq!(asymptotic_increasing.last_step_size(14u32), 90u32);
	assert_eq!(asymptotic_increasing.last_step_size(16u32), 81u32);
	assert_eq!(asymptotic_increasing.last_step_size(u32::MAX), 0u32);

	// Step::RemainingPct decreasing.
	let asymptotic_decreasing = SteppedCurve::new(
		10u32,
		None,
		1000u32,
		Step::RemainingPct(0u32, Perbill::from_percent(10)),
		2u32,
	);
	assert_eq!(asymptotic_decreasing.evaluate(5u32), 1000u32);
	assert_eq!(asymptotic_decreasing.evaluate(11u32), 1000u32);
	assert_eq!(asymptotic_decreasing.evaluate(12u32), 900u32);
	assert_eq!(asymptotic_decreasing.evaluate(14u32), 810u32);
	assert_eq!(asymptotic_decreasing.evaluate(16u32), 729u32);
	assert_eq!(asymptotic_decreasing.evaluate(u32::MAX), 0u32);
	assert_eq!(asymptotic_decreasing.last_step_size(5u32), 0u32);
	assert_eq!(asymptotic_decreasing.last_step_size(11u32), 0u32);
	assert_eq!(asymptotic_decreasing.last_step_size(12u32), 100u32);
	assert_eq!(asymptotic_decreasing.last_step_size(14u32), 90u32);
	assert_eq!(asymptotic_decreasing.last_step_size(16u32), 81u32);
	assert_eq!(asymptotic_decreasing.last_step_size(u32::MAX), 0u32);

	// Step::RemainingPct stable.
	let asymptotic_stable = SteppedCurve::new(
		10u32,
		None,
		1000u32,
		Step::RemainingPct(1000u32, Perbill::from_percent(10)),
		2u32,
	);
	assert_eq!(asymptotic_stable.evaluate(5u32), 1000u32);
	assert_eq!(asymptotic_stable.evaluate(12u32), 1000u32);
	assert_eq!(asymptotic_stable.evaluate(20u32), 1000u32);
	assert_eq!(asymptotic_stable.last_step_size(5u32), 0u32);
	assert_eq!(asymptotic_stable.last_step_size(12u32), 0u32);
	assert_eq!(asymptotic_stable.last_step_size(20u32), 0u32);

	// Step::RemainingPct capped end.
	let asymptotic_with_end = SteppedCurve::new(
		10u32,
		Some(14u32),
		0u32,
		Step::RemainingPct(1000u32, Perbill::from_percent(10)),
		2u32,
	);
	assert_eq!(asymptotic_with_end.evaluate(5u32), 0u32);
	assert_eq!(asymptotic_with_end.evaluate(11u32), 0u32);
	assert_eq!(asymptotic_with_end.evaluate(12u32), 100u32);
	assert_eq!(asymptotic_with_end.evaluate(14u32), 190u32);
	assert_eq!(asymptotic_with_end.evaluate(16u32), 190u32);
	assert_eq!(asymptotic_with_end.last_step_size(5u32), 0u32);
	assert_eq!(asymptotic_with_end.last_step_size(11u32), 0u32);
	assert_eq!(asymptotic_with_end.last_step_size(12u32), 100u32);
	assert_eq!(asymptotic_with_end.last_step_size(14u32), 90u32);
	assert_eq!(asymptotic_with_end.last_step_size(16u32), 0u32);
	assert_eq!(asymptotic_with_end.last_step_size(18u32), 0u32);
}

#[test]
fn test_multiply_by_rational_saturating() {
	let div = 100u32;
	for value in 0..=div {
		for p in 0..=div {
			for q in 1..=div {
				let value: u64 =
					(value as u128 * u64::MAX as u128 / div as u128).try_into().unwrap();
				let p = (p as u64 * u32::MAX as u64 / div as u64).try_into().unwrap();
				let q = (q as u64 * u32::MAX as u64 / div as u64).try_into().unwrap();

				assert_eq!(
					multiply_by_rational_saturating(value, p, q),
					(value as u128 * p as u128 / q as u128).try_into().unwrap_or(u64::MAX)
				);
			}
		}
	}
}

#[test]
fn test_calculate_for_fraction_times_denominator() {
	let curve = PiecewiseLinear {
		points: &[
			(Perbill::from_parts(0_000_000_000), Perbill::from_parts(0_500_000_000)),
			(Perbill::from_parts(0_500_000_000), Perbill::from_parts(1_000_000_000)),
			(Perbill::from_parts(1_000_000_000), Perbill::from_parts(0_000_000_000)),
		],
		maximum: Perbill::from_parts(1_000_000_000),
	};

	pub fn formal_calculate_for_fraction_times_denominator(n: u64, d: u64) -> u64 {
		if n <= Perbill::from_parts(0_500_000_000) * d {
			n + d / 2
		} else {
			(d as u128 * 2 - n as u128 * 2).try_into().unwrap()
		}
	}

	let div = 100u32;
	for d in 0..=div {
		for n in 0..=d {
			let d: u64 = (d as u128 * u64::MAX as u128 / div as u128).try_into().unwrap();
			let n: u64 = (n as u128 * u64::MAX as u128 / div as u128).try_into().unwrap();

			let res = curve.calculate_for_fraction_times_denominator(n, d);
			let expected = formal_calculate_for_fraction_times_denominator(n, d);

			assert!(abs_sub(res, expected) <= 1);
		}
	}
}
