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

//! Provides various curves and their evaluations.

use crate::{
	traits::{AtLeast32BitUnsigned, SaturatedConversion},
	Perbill,
};
use core::ops::Sub;
use scale_info::TypeInfo;

/// The step type for the stepped curve.
#[derive(PartialEq, Eq, sp_core::RuntimeDebug, TypeInfo, Clone)]
pub enum Step<V> {
	/// Increase the value by a percentage.
	PctInc(Perbill),
	/// Decrease the value by a percentage.
	PctDec(Perbill),
	/// Increment by a value.
	Add(V),
	/// Decrement by a value.
	Subtract(V),
}

/// A stepped curve.
///
/// The curve evaluates over the domain [`start`, `end`], clamping on either end.
/// The initial value is specified and will step every `period` from there.
/// The first step happens at `start` + `period`.
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
	P: AtLeast32BitUnsigned + Clone,
	V: AtLeast32BitUnsigned + Clone + From<P>,
{
	/// Creates a new `SteppedCurve`.
	pub fn new(start: P, end: Option<P>, initial_value: V, step: Step<V>, period: P) -> Self {
		Self { start, end, initial_value, step, period}
	}

	/// Evaluate the curve at a given point.
	pub fn evaluate(&self, point: P) -> V {
		// If the point is before the curve starts, return the initial value.
		if point < self.start {
			return self.initial_value.clone()
		}

		// If the period is zero, the value never changes.
		if self.period.is_zero() {
			return self.initial_value.clone()
		}

		// Determine the effective point for calculation, capped by the end point if it exists.
		let effective_point = self.end.clone().map_or(point.clone(), |e| point.min(e));

		// Calculate how many full periods have passed, capped by u32.
		let num_periods = (effective_point - self.start.clone()) / self.period.clone();
		let num_periods_u32 = num_periods.clone().saturated_into::<u32>();

		match self.step.clone() {
			Step::Add(step_value) => {
				// Initial_value + num_periods * step_value.
				let total_step = step_value.saturating_mul(num_periods.clone().saturated_into::<V>());
				self.initial_value.clone().saturating_add(total_step)
			},
			Step::Subtract(step_value) => {
				// Initial_value - num_periods * step_value
				let total_step = step_value.saturating_mul(num_periods.clone().saturated_into::<V>());
				self.initial_value.clone().saturating_sub(total_step)
			},
			Step::PctInc(percent) => {
				// initial_value * (1 + percent) ^ num_periods
				let mut current_value = self.initial_value.clone();
				for _ in 0..num_periods_u32 { //<-- need to fix this
					let increase = percent * current_value.clone();
					current_value = current_value.saturating_add(increase);
				}
				current_value
			},
			Step::PctDec(percent) => {
				// initial_value * (1 - percent) ^ num_periods
				let mut current_value = self.initial_value.clone();
				for _ in 0..num_periods_u32 {
					let decrease = percent * current_value.clone();
					current_value = current_value.saturating_sub(decrease);
				}
				current_value
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
	let curve_with_end = SteppedCurve::new(
		10u32,
		Some(20u32),
		100u32,
		Step::Add(100u32),
		2u32,
	);
	assert_eq!(curve_with_end.evaluate(20u32), 600u32);
	assert_eq!(curve_with_end.evaluate(30u32), 600u32);

	// Zero period curve.
	let zero_period_curve = SteppedCurve::new(
		10u32,
		None,
		100u32,
		Step::Add(100u32),
		0u32,
	);
	assert_eq!(zero_period_curve.evaluate(5u32), 100u32);
	assert_eq!(zero_period_curve.evaluate(11u32), 100u32);
	assert_eq!(zero_period_curve.evaluate(12u32), 100u32);
	assert_eq!(zero_period_curve.evaluate(20u32), 100u32);

	// Step::Add.
	let add_curve = SteppedCurve::new(
		10u32,
		None,
		100u32,
		Step::Add(100u32),
		2u32,
	);
	assert_eq!(add_curve.evaluate(5u32), 100u32);
	assert_eq!(add_curve.evaluate(11u32), 100u32);
	assert_eq!(add_curve.evaluate(12u32), 200u32);
	assert_eq!(add_curve.evaluate(20u32), 600u32);
	assert_eq!(add_curve.evaluate(u32::MAX), u32::MAX);

	// Step::Subtract.
	let subtract_curve = SteppedCurve::new(
		10u32,
		None,
		1000u32,
		Step::Subtract(100u32),
		2u32,
	);
	assert_eq!(subtract_curve.evaluate(5u32), 1000u32);
	assert_eq!(subtract_curve.evaluate(11u32), 1000u32);
	assert_eq!(subtract_curve.evaluate(12u32), 900u32);
	assert_eq!(subtract_curve.evaluate(20u32), 500u32);
	assert_eq!(subtract_curve.evaluate(u32::MAX), u32::MIN);

	// Step::PctInc.
	let pct_inc_curve = SteppedCurve::new(
		10u32,
		None,
		1000u32,
		Step::PctInc(Perbill::from_percent(10)),
		2u32,
	);
	assert_eq!(pct_inc_curve.evaluate(5u32), 1000u32);
	assert_eq!(pct_inc_curve.evaluate(11u32), 1000u32);
	assert_eq!(pct_inc_curve.evaluate(12u32), 1100u32);
	assert_eq!(pct_inc_curve.evaluate(20u32), 1611u32);
	// assert_eq!(pct_inc_curve.evaluate(u32::MAX), u32::MAX);

	// Step::PctDec.
	let pct_dec_curve = SteppedCurve::new(
		10u32,
		None,
		1000u32,
		Step::PctDec(Perbill::from_percent(10)),
		2u32,
	);
	assert_eq!(pct_dec_curve.evaluate(5u32), 1000u32);
	assert_eq!(pct_dec_curve.evaluate(11u32), 1000u32);
	assert_eq!(pct_dec_curve.evaluate(12u32), 900u32);
	assert_eq!(pct_dec_curve.evaluate(20u32), 590u32);
	// assert_eq!(pct_dec_curve.evaluate(u32::MAX), u32::MIN);
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
