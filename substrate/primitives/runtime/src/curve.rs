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
use num_traits::{CheckedDiv, One, Zero};
use scale_info::TypeInfo;
use sp_arithmetic::{
	traits::{Bounded, Saturating},
	FixedPointNumber,
};

/// The step type for the stepped curve.
#[derive(PartialEq, Eq, sp_core::RuntimeDebug, TypeInfo, Clone)]
pub enum Step {
	/// Increase the value by a percentage of the current value at each step.
	PctInc {
		/// The percentage to increase by.
		pct: FixedU128,
	},
	/// Decrease the value by a percentage of the current value at each step.
	PctDec {
		/// The percentage to decrease by.
		pct: FixedU128,
	},
	/// Increment by a constant value at each step.
	Add {
		/// The amount to add.
		amount: FixedU128,
	},
	/// Decrement by a constant value at each step.
	Subtract {
		/// The amount to substract.
		amount: FixedU128,
	},
	/// Move towards a desired value by a percentage of the remaining difference at each step.
	///
	/// Step size will be (target_total - current_value) * pct.
	RemainingPct {
		/// The asymptote the curve will move towards.
		target: FixedU128,
		/// The percentage closer to the `target` at each step.
		pct: Perbill,
	},
}

/// A stepped curve.
///
/// Steps every `period` from the `initial_value` as defined by `step`.
/// First step from `initial_value` takes place at `start` + `period`.
#[derive(PartialEq, Eq, sp_core::RuntimeDebug, TypeInfo, Clone)]
pub struct SteppedCurve {
	/// The starting point for the curve.
	pub start: FixedU128,
	/// An optional point at which the curve ends. If `None`, the curve continues indefinitely.
	pub end: Option<FixedU128>,
	/// The initial value of the curve at the `start` point.
	pub initial_value: FixedU128,
	/// The change to apply at the end of each `period`.
	pub step: Step,
	/// The duration of each step.
	pub period: FixedU128,
}

impl SteppedCurve {
	/// Creates a new `SteppedCurve`.
	pub fn new(
		start: FixedU128,
		end: Option<FixedU128>,
		initial_value: FixedU128,
		step: Step,
		period: FixedU128,
	) -> Self {
		Self { start, end, initial_value, step, period }
	}

	/// Returns the magnitude of the step size occuring at the start of this point's period.
	/// If no step has occured, will return 0.
	///
	/// Ex. In period 4, the last step taken was 10 -> 7, it would return 3.
	pub fn last_step_size(&self, point: FixedU128) -> FixedU128 {
		// Already ended.
		if let Some(end_point) = self.end {
			if end_point < self.start {
				return Zero::zero();
			}
		}

		// No step taken yet.
		if point <= self.start {
			return Zero::zero();
		}

		// If the period is zero, the value never changes.
		if self.period.is_zero() {
			return Zero::zero();
		}

		// Calculate how many full periods have passed.
		let num_periods =
			(point - self.start).checked_div(&self.period).unwrap_or(FixedU128::max_value());

		// Full period has not passed.
		if num_periods < One::one() {
			return Zero::zero();
		}

		// Points for calculating step difference.
		let prev_period_point = self.start + (num_periods - One::one()) * self.period;
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
	///
	/// Number of steps capped at `u32::MAX`.
	pub fn evaluate(&self, point: FixedU128) -> FixedU128 {
		let initial = self.initial_value;

		// Already ended.
		if let Some(end_point) = self.end {
			if end_point < self.start {
				return initial;
			}
		}

		// If the point is before the curve starts, return the initial value.
		if point <= self.start {
			return initial;
		}

		// If the period is zero, the value never changes.
		if self.period.is_zero() {
			return initial;
		}

		// Determine the effective point for calculation, capped by the end point if it exists.
		let effective_point = self.end.map_or(point, |e| point.min(e));

		// Calculate how many full periods have passed, capped by u32::MAX.
		let num_periods = (effective_point - self.start)
			.checked_div(&self.period)
			.unwrap_or(FixedU128::max_value());
		let num_periods_u32 = (num_periods.into_inner() / FixedU128::DIV).saturated_into::<u32>();
		let num_periods_floor = FixedU128::saturating_from_integer(num_periods_u32);

		// No periods have passed.
		if num_periods_u32.is_zero() {
			return initial;
		}

		match self.step {
			Step::Add { amount: step_value } => {
				// Initial_value + num_periods * step_value.
				let total_step = step_value.saturating_mul(num_periods_floor);
				initial.saturating_add(total_step)
			},
			Step::Subtract { amount: step_value } => {
				// Initial_value - num_periods * step_value.
				let total_step = step_value.saturating_mul(num_periods_floor);
				initial.saturating_sub(total_step)
			},
			Step::PctInc { pct: percent } => {
				// Initial_value * (1 + percent) ^ num_periods.
				let ratio = FixedU128::one().saturating_add(percent);
				let scale = ratio.saturating_pow(num_periods_u32 as usize);
				initial.saturating_mul(scale)
			},
			Step::PctDec { pct: percent } => {
				// Initial_value * (1 - percent) ^ num_periods.
				let ratio = FixedU128::one().saturating_sub(percent);
				let scale = ratio.saturating_pow(num_periods_u32 as usize);
				initial.saturating_mul(scale)
			},
			Step::RemainingPct { target: asymptote, pct: percent } => {
				// Asymptote +/- diff(asymptote, initial_value) * (1-percent)^num_periods.
				let ratio = FixedU128::one().saturating_sub(FixedU128::from_perbill(percent));
				let scale = ratio.saturating_pow(num_periods_u32 as usize);

				if initial >= asymptote {
					let diff = initial.saturating_sub(asymptote);
					asymptote.saturating_add(diff.saturating_mul(scale))
				} else {
					let diff = asymptote.saturating_sub(initial);
					asymptote.saturating_sub(diff.saturating_mul(scale))
				}
			},
		}
	}
}

/// Piecewise Linear function in [0, 1] -> [0, 1].
#[derive(PartialEq, Eq, Debug, TypeInfo)]
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
	// u32 to Fixed.
	fn uf(n: u32) -> FixedU128 {
		FixedU128::saturating_from_integer(n)
	}

	// Fixed to u32.
	fn fu(n: FixedU128) -> u32 {
		n.saturating_mul_int(1u32)
	}

	// Curve with defined end.
	let curve_with_end =
		SteppedCurve::new(uf(10), Some(uf(20)), uf(100), Step::Add { amount: uf(100) }, uf(2));
	assert_eq!(fu(curve_with_end.evaluate(uf(20))), 600);
	assert_eq!(fu(curve_with_end.evaluate(uf(22))), 600);
	assert_eq!(fu(curve_with_end.last_step_size(uf(10))), 0);
	assert_eq!(fu(curve_with_end.last_step_size(uf(20))), 100);
	assert_eq!(fu(curve_with_end.last_step_size(uf(22))), 0);
	assert_eq!(fu(curve_with_end.last_step_size(uf(30))), 0);

	// End is less than start.
	let end_less_than_start =
		SteppedCurve::new(uf(10), Some(uf(0)), uf(100), Step::Add { amount: uf(100) }, uf(2));
	assert_eq!(fu(end_less_than_start.evaluate(uf(10))), 100);
	assert_eq!(fu(end_less_than_start.evaluate(uf(12))), 100);
	assert_eq!(fu(end_less_than_start.last_step_size(uf(10))), 0);
	assert_eq!(fu(end_less_than_start.last_step_size(uf(20))), 0);

	// End is start.
	let end_is_start =
		SteppedCurve::new(uf(10), Some(uf(10)), uf(100), Step::Add { amount: uf(100) }, uf(2));
	assert_eq!(fu(end_is_start.evaluate(uf(10))), 100);
	assert_eq!(fu(end_is_start.evaluate(uf(12))), 100);
	assert_eq!(fu(end_is_start.last_step_size(uf(10))), 0);
	assert_eq!(fu(end_is_start.last_step_size(uf(20))), 0);

	// Zero period curve.
	let zero_period_curve =
		SteppedCurve::new(uf(10), None, uf(100), Step::Add { amount: uf(100) }, uf(0));
	assert_eq!(fu(zero_period_curve.evaluate(uf(5))), 100);
	assert_eq!(fu(zero_period_curve.evaluate(uf(11))), 100);
	assert_eq!(fu(zero_period_curve.evaluate(uf(12))), 100);
	assert_eq!(fu(zero_period_curve.evaluate(uf(20))), 100);
	assert_eq!(fu(zero_period_curve.last_step_size(uf(20))), 0);

	// Step::Add.
	let add_curve = SteppedCurve::new(uf(10), None, uf(100), Step::Add { amount: uf(100) }, uf(2));
	assert_eq!(fu(add_curve.evaluate(uf(5))), 100);
	assert_eq!(fu(add_curve.evaluate(uf(11))), 100);
	assert_eq!(fu(add_curve.evaluate(uf(12))), 200);
	assert_eq!(fu(add_curve.evaluate(uf(20))), 600);
	assert_eq!(fu(add_curve.evaluate(uf(u32::MAX))), u32::MAX);
	assert_eq!(fu(add_curve.last_step_size(uf(11))), 0);
	assert_eq!(fu(add_curve.last_step_size(uf(12))), 100);
	assert_eq!(fu(add_curve.last_step_size(uf(20))), 100);

	// Step::Subtract.
	let subtract_curve =
		SteppedCurve::new(uf(10), None, uf(1000), Step::Subtract { amount: uf(100) }, uf(2));
	assert_eq!(fu(subtract_curve.evaluate(uf(5))), 1000);
	assert_eq!(fu(subtract_curve.evaluate(uf(11))), 1000);
	assert_eq!(fu(subtract_curve.evaluate(uf(12))), 900);
	assert_eq!(fu(subtract_curve.evaluate(uf(20))), 500);
	assert_eq!(fu(subtract_curve.evaluate(uf(u32::MAX))), u32::MIN);
	assert_eq!(fu(subtract_curve.last_step_size(uf(11))), 0);
	assert_eq!(fu(subtract_curve.last_step_size(uf(12))), 100);
	assert_eq!(fu(subtract_curve.last_step_size(uf(20))), 100);

	// Step::PctInc.
	let pct_inc_curve = SteppedCurve::new(
		uf(10),
		None,
		uf(1000),
		Step::PctInc { pct: FixedU128::from_rational(1, 10) },
		uf(2),
	);
	assert_eq!(fu(pct_inc_curve.evaluate(uf(5))), 1000);
	assert_eq!(fu(pct_inc_curve.evaluate(uf(11))), 1000);
	assert_eq!(fu(pct_inc_curve.evaluate(uf(12))), 1100);
	assert_eq!(fu(pct_inc_curve.evaluate(uf(20))), 1610);
	assert_eq!(fu(pct_inc_curve.evaluate(uf(u32::MAX))), u32::MAX);
	assert_eq!(fu(pct_inc_curve.last_step_size(uf(11))), 0);
	assert_eq!(fu(pct_inc_curve.last_step_size(uf(12))), 100);
	assert_eq!(fu(pct_inc_curve.last_step_size(uf(20))), 146);
	assert_eq!(fu(pct_inc_curve.last_step_size(uf(u32::MAX))), 0);

	// Step::PctDec.
	let pct_dec_curve = SteppedCurve::new(
		uf(10),
		None,
		uf(1000),
		Step::PctDec { pct: FixedU128::from_rational(1, 10) },
		uf(2),
	);
	assert_eq!(fu(pct_dec_curve.evaluate(uf(5))), 1000);
	assert_eq!(fu(pct_dec_curve.evaluate(uf(11))), 1000);
	assert_eq!(fu(pct_dec_curve.evaluate(uf(12))), 900);
	assert_eq!(fu(pct_dec_curve.evaluate(uf(20))), 590);
	assert_eq!(fu(pct_dec_curve.evaluate(uf(u32::MAX))), u32::MIN);
	assert_eq!(fu(pct_dec_curve.last_step_size(uf(11))), 0);
	assert_eq!(fu(pct_dec_curve.last_step_size(uf(12))), 100);
	assert_eq!(fu(pct_dec_curve.last_step_size(uf(20))), 65);
	assert_eq!(fu(pct_dec_curve.last_step_size(uf(u32::MAX))), 0);

	// Step::RemainingPct increasing.
	let asymptotic_increasing = SteppedCurve::new(
		uf(10),
		None,
		uf(0),
		Step::RemainingPct { target: uf(1000), pct: Perbill::from_percent(10) },
		uf(2),
	);
	assert_eq!(fu(asymptotic_increasing.evaluate(uf(5))), 0);
	assert_eq!(fu(asymptotic_increasing.evaluate(uf(11))), 0);
	assert_eq!(fu(asymptotic_increasing.evaluate(uf(12))), 100);
	assert_eq!(fu(asymptotic_increasing.evaluate(uf(14))), 190);
	assert_eq!(fu(asymptotic_increasing.evaluate(uf(16))), 271);
	assert_eq!(fu(asymptotic_increasing.evaluate(uf(u32::MAX))), 1000);
	assert_eq!(fu(asymptotic_increasing.last_step_size(uf(5))), 0);
	assert_eq!(fu(asymptotic_increasing.last_step_size(uf(11))), 0);
	assert_eq!(fu(asymptotic_increasing.last_step_size(uf(12))), 100);
	assert_eq!(fu(asymptotic_increasing.last_step_size(uf(14))), 90);
	assert_eq!(fu(asymptotic_increasing.last_step_size(uf(16))), 81);
	assert_eq!(fu(asymptotic_increasing.last_step_size(uf(u32::MAX))), 0);

	// Step::RemainingPct decreasing.
	let asymptotic_decreasing = SteppedCurve::new(
		uf(10),
		None,
		uf(1000),
		Step::RemainingPct { target: uf(0), pct: Perbill::from_percent(10) },
		uf(2),
	);
	assert_eq!(fu(asymptotic_decreasing.evaluate(uf(5))), 1000);
	assert_eq!(fu(asymptotic_decreasing.evaluate(uf(11))), 1000);
	assert_eq!(fu(asymptotic_decreasing.evaluate(uf(12))), 900);
	assert_eq!(fu(asymptotic_decreasing.evaluate(uf(14))), 810);
	assert_eq!(fu(asymptotic_decreasing.evaluate(uf(16))), 729);
	assert_eq!(fu(asymptotic_decreasing.evaluate(uf(u32::MAX))), 0);
	assert_eq!(fu(asymptotic_decreasing.last_step_size(uf(5))), 0);
	assert_eq!(fu(asymptotic_decreasing.last_step_size(uf(11))), 0);
	assert_eq!(fu(asymptotic_decreasing.last_step_size(uf(12))), 100);
	assert_eq!(fu(asymptotic_decreasing.last_step_size(uf(14))), 90);
	assert_eq!(fu(asymptotic_decreasing.last_step_size(uf(16))), 81);
	assert_eq!(fu(asymptotic_decreasing.last_step_size(uf(u32::MAX))), 0);

	// Step::RemainingPct stable.
	let asymptotic_stable = SteppedCurve::new(
		uf(10),
		None,
		uf(1000),
		Step::RemainingPct { target: uf(1000), pct: Perbill::from_percent(10) },
		uf(2),
	);
	assert_eq!(fu(asymptotic_stable.evaluate(uf(5))), 1000);
	assert_eq!(fu(asymptotic_stable.evaluate(uf(12))), 1000);
	assert_eq!(fu(asymptotic_stable.evaluate(uf(20))), 1000);
	assert_eq!(fu(asymptotic_stable.last_step_size(uf(5))), 0);
	assert_eq!(fu(asymptotic_stable.last_step_size(uf(12))), 0);
	assert_eq!(fu(asymptotic_stable.last_step_size(uf(20))), 0);

	// Step::RemainingPct capped end.
	let asymptotic_with_end = SteppedCurve::new(
		uf(10),
		Some(uf(14)),
		uf(0),
		Step::RemainingPct { target: uf(1000), pct: Perbill::from_percent(10) },
		uf(2),
	);
	assert_eq!(fu(asymptotic_with_end.evaluate(uf(5))), 0);
	assert_eq!(fu(asymptotic_with_end.evaluate(uf(11))), 0);
	assert_eq!(fu(asymptotic_with_end.evaluate(uf(12))), 100);
	assert_eq!(fu(asymptotic_with_end.evaluate(uf(14))), 190);
	assert_eq!(fu(asymptotic_with_end.evaluate(uf(16))), 190);
	assert_eq!(fu(asymptotic_with_end.last_step_size(uf(5))), 0);
	assert_eq!(fu(asymptotic_with_end.last_step_size(uf(11))), 0);
	assert_eq!(fu(asymptotic_with_end.last_step_size(uf(12))), 100);
	assert_eq!(fu(asymptotic_with_end.last_step_size(uf(14))), 90);
	assert_eq!(fu(asymptotic_with_end.last_step_size(uf(16))), 0);
	assert_eq!(fu(asymptotic_with_end.last_step_size(uf(18))), 0);

	// Converges on asymptote.
	let asymptote_converges = SteppedCurve::new(
		uf(10),
		None,
		uf(0),
		Step::RemainingPct { target: uf(1000), pct: Perbill::from_percent(10) },
		uf(2),
	);
	let final_value = asymptote_converges.evaluate(uf(u32::MAX));
	assert!(final_value == uf(1000));

	// Cumulative step sizes sum correctly.
	let target = uf(1000);
	let cumulative_curve = SteppedCurve::new(
		uf(0),
		None,
		uf(0),
		Step::RemainingPct { target, pct: Perbill::from_percent(10) },
		uf(1),
	);
	let mut sum = uf(0);
	for i in 1..=1000 {
		sum = sum.saturating_add(cumulative_curve.last_step_size(uf(i)));
	}
	assert!(sum == uf(1000));

	// Fractional add.
	let fractional_add_curve = SteppedCurve::new(
		uf(1),
		None,
		uf(10),
		Step::Add { amount: FixedU128::from_float(2.5) },
		FixedU128::from_float(0.5),
	);
	assert_eq!(
		fractional_add_curve.evaluate(FixedU128::from_float(3.5)),
		FixedU128::from_float(22.50)
	);

	// PctInc over 1.
	let rapid_inc_curve = SteppedCurve::new(
		uf(0),
		None,
		uf(100),
		Step::PctInc { pct: FixedU128::from_rational(15, 10) },
		uf(1),
	);
	assert_eq!(rapid_inc_curve.evaluate(uf(1)), uf(250));
	assert_eq!(rapid_inc_curve.evaluate(uf(2)), uf(625));

	// PctDec over 1.
	let over_decrease_curve =
		SteppedCurve::new(uf(0), None, uf(100), Step::PctDec { pct: uf(2) }, uf(1));
	assert_eq!(over_decrease_curve.evaluate(uf(1)), uf(0));
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
