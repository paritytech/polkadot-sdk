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

//! # Running
//! Running this fuzzer can be done with `cargo hfuzz run per_thing_checked_arith`.
//! `honggfuzz` CLI options can be used by setting `HFUZZ_RUN_ARGS`, such as `-n 4` to use 4
//! threads.
//!
//! # Debugging a panic
//! Once a panic is found, it can be debugged with
//! `cargo hfuzz run-debug per_thing_checked_arith hfuzz_workspace/per_thing_checked_arith/*.fuzz`.
//!
//! # More information
//! More information about `honggfuzz` can be found
//! [here](https://docs.rs/honggfuzz/).

use core::convert::{TryFrom, TryInto};
use honggfuzz::fuzz;
use num_bigint::BigUint;
use num_traits::{
	Bounded as NBounded, CheckedAdd as NCheckedAdd, CheckedDiv as NCheckedDiv,
	CheckedMul as NCheckedMul, One as NOne, ToPrimitive, Zero as NZero,
};
use sp_arithmetic::{
	ArithmeticError, PerThing, PerU16, Perbill, Percent, Permill, Perquintill, Rounding,
	Rounding::{Down, NearestPrefDown, NearestPrefUp, Up},
};

#[derive(Debug, Clone, Copy)]
struct ArbitraryRounding(Rounding);

impl arbitrary::Arbitrary<'_> for ArbitraryRounding {
	fn arbitrary(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Self> {
		Ok(Self(match u.int_in_range(0..=3)? {
			0 => Up,
			1 => NearestPrefUp,
			2 => Down,
			3 => NearestPrefDown,
			_ => unreachable!(),
		}))
	}
}

#[derive(Debug, Clone, Copy)]
enum PerThingType {
	Percent,
	Permill,
	Perbill,
	PerU16,
	Perquintill,
}

impl arbitrary::Arbitrary<'_> for PerThingType {
	fn arbitrary(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Self> {
		Ok(match u.int_in_range(0..=4)? {
			0 => PerThingType::Percent,
			1 => PerThingType::Permill,
			2 => PerThingType::Perbill,
			3 => PerThingType::PerU16,
			4 => PerThingType::Perquintill,
			_ => unreachable!(),
		})
	}
}

#[derive(Debug, Clone, Copy)]
enum PerThingArithOp {
	CheckedMulFloor,
	CheckedMulCeil,
	CheckedReciprocalMulFloor,
	CheckedReciprocalMulCeil,
	CheckedReciprocalMul,
	CheckedSquare,
	CheckedDivWithRounding,
	CheckedIntDiv,
}

impl arbitrary::Arbitrary<'_> for PerThingArithOp {
	fn arbitrary(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Self> {
		Ok(match u.int_in_range(0..=7)? {
			0 => PerThingArithOp::CheckedMulFloor,
			1 => PerThingArithOp::CheckedMulCeil,
			2 => PerThingArithOp::CheckedReciprocalMulFloor,
			3 => PerThingArithOp::CheckedReciprocalMulCeil,
			4 => PerThingArithOp::CheckedReciprocalMul,
			5 => PerThingArithOp::CheckedSquare,
			6 => PerThingArithOp::CheckedDivWithRounding,
			7 => PerThingArithOp::CheckedIntDiv,
			_ => unreachable!(),
		})
	}
}

// Simulates checked_rational_mul_correction using BigUint
fn oracle_checked_rational_mul_correction<P: PerThing>(
	x_u64: u64,
	numer_inner: P::Inner,
	denom_inner: P::Inner,
	rounding: Rounding,
) -> Result<BigUint, ArithmeticError>
where
	P::Inner: Into<u128> + NZero + NOne + Copy + NBounded,
	P::Upper: NBounded + TryFrom<BigUint> + Into<u128>,
	<P::Upper as TryFrom<BigUint>>::Error: core::fmt::Debug,
{
	let zero_inner = P::Inner::zero();

	if denom_inner == zero_inner {
		return Err(ArithmeticError::DivisionByZero);
	}

	let x_big = BigUint::from(x_u64);
	let numer_big = BigUint::from(numer_inner.into());
	let denom_big = BigUint::from(denom_inner.into());

	if denom_big.is_zero() {
		return Err(ArithmeticError::DivisionByZero);
	}

	let rem_big = &x_big % &denom_big;
	let rem_inner_big = rem_big;
	let numer_upper_big = numer_big.clone();
	let denom_upper_big = denom_big.clone();
	let upper_max_big = BigUint::from(P::Upper::max_value().into());
	let rem_mul_upper_big = &rem_inner_big * &numer_upper_big;

	if rem_mul_upper_big > upper_max_big {
		return Err(ArithmeticError::Overflow);
	}

	if denom_upper_big.is_zero() {
		return Err(ArithmeticError::DivisionByZero);
	}

	let rem_mul_div_upper_big = &rem_mul_upper_big / &denom_upper_big;
	let remainder_upper_big = &rem_mul_upper_big % &denom_upper_big;
	let inner_max_big = BigUint::from(P::Inner::max_value().into());
	let mut correction_big = rem_mul_div_upper_big;
	let one_big = BigUint::one();
	let two_big = BigUint::from(2u32);
	let threshold_big = &denom_upper_big / &two_big;
	let tie_breaker_big = &denom_upper_big % &two_big;

	match rounding {
		Rounding::Down => {},
		Rounding::Up =>
			if !remainder_upper_big.is_zero() {
				correction_big =
					correction_big.checked_add(&one_big).ok_or(ArithmeticError::Overflow)?;
			},
		Rounding::NearestPrefDown =>
			if remainder_upper_big > threshold_big {
				correction_big =
					correction_big.checked_add(&one_big).ok_or(ArithmeticError::Overflow)?;
			},
		Rounding::NearestPrefUp => {
			let threshold_with_tie =
				threshold_big.checked_add(&tie_breaker_big).ok_or(ArithmeticError::Overflow)?;
			if remainder_upper_big >= threshold_with_tie {
				correction_big =
					correction_big.checked_add(&one_big).ok_or(ArithmeticError::Overflow)?;
			}
		},
	}

	if correction_big > inner_max_big {
		return Err(ArithmeticError::Overflow);
	}

	Ok(correction_big)
}

// Simulates checked_overflow_prune_mul using BigUint
fn oracle_checked_mul<P: PerThing>(
	pt1: P,
	int_operand: u64,
	rounding: Rounding,
) -> Result<u64, ArithmeticError>
where
	P::Inner: Into<u128> + NZero + NOne + Copy + NBounded + TryInto<u64>,
	<P::Inner as TryInto<u64>>::Error: core::fmt::Debug,
	P::Upper: NBounded + TryFrom<BigUint> + Into<u128>, // Added Into<u128>
	<P::Upper as TryFrom<BigUint>>::Error: core::fmt::Debug,
{
	let accuracy_inner = P::ACCURACY;
	let part_inner = pt1.deconstruct();

	if accuracy_inner.is_zero() {
		return Err(ArithmeticError::DivisionByZero);
	}

	let accuracy_big = BigUint::from(accuracy_inner.into());
	let part_big = BigUint::from(part_inner.into());
	let x_big = BigUint::from(int_operand);

	// Double check
	if accuracy_big.is_zero() {
		return Err(ArithmeticError::DivisionByZero);
	}
	let correction_big = oracle_checked_rational_mul_correction::<P>(
		int_operand,
		part_inner,
		accuracy_inner,
		rounding,
	)?;

	let term1_big = &x_big / &accuracy_big;
	let term2_big = term1_big.checked_mul(&part_big).ok_or(ArithmeticError::Overflow)?;

	let final_result_big =
		term2_big.checked_add(&correction_big).ok_or(ArithmeticError::Overflow)?;

	final_result_big.to_u64().ok_or(ArithmeticError::Overflow)
}

// Simulates checked_saturating_reciprocal_mul using BigUint
fn oracle_checked_reciprocal_mul<P: PerThing>(
	pt1: P,
	int_operand: u64,
	rounding: Rounding,
) -> Result<u64, ArithmeticError>
where
	P::Inner: Into<u128> + NZero + NOne + Copy + NBounded + TryInto<u64>,
	<P::Inner as TryInto<u64>>::Error: core::fmt::Debug,
	P::Upper: NBounded + TryFrom<BigUint> + Into<u128>,
	<P::Upper as TryFrom<BigUint>>::Error: core::fmt::Debug,
{
	let part_inner = pt1.deconstruct();

	if part_inner.is_zero() {
		return Err(ArithmeticError::DivisionByZero);
	}

	let accuracy_inner = P::ACCURACY;
	let accuracy_big = BigUint::from(accuracy_inner.into());
	let part_big = BigUint::from(part_inner.into());
	let x_big = BigUint::from(int_operand);

	// Double check
	if part_big.is_zero() {
		return Err(ArithmeticError::DivisionByZero);
	}

	let correction_big = oracle_checked_rational_mul_correction::<P>(
		int_operand,
		accuracy_inner,
		part_inner,
		rounding,
	)?;
	let term1_big = &x_big / &part_big;
	let term2_big = term1_big.checked_mul(&accuracy_big).ok_or(ArithmeticError::Overflow)?;
	let final_result_big =
		term2_big.checked_add(&correction_big).ok_or(ArithmeticError::Overflow)?;

	Ok(final_result_big.to_u64().unwrap_or(u64::MAX))
}

// Simulates checked_from_rational_with_rounding using BigUint
fn oracle_checked_from_rational_with_rounding<P: PerThing>(
	p_big: BigUint,
	q_big: BigUint,
	rounding: Rounding,
) -> Result<P, ArithmeticError>
where
	P::Inner: Into<u128> + NZero + NOne + Copy + NBounded + TryFrom<u128>,
	<P::Inner as TryFrom<u128>>::Error: core::fmt::Debug,
{
	if q_big.is_zero() {
		return Err(ArithmeticError::DivisionByZero);
	}
	if p_big > q_big {
		return Err(ArithmeticError::Overflow);
	}

	let accuracy_big = BigUint::from(P::ACCURACY.into());
	let inner_max_big = BigUint::from(P::Inner::max_value().into());
	let num_intermediate = accuracy_big.checked_mul(&p_big).ok_or(ArithmeticError::Overflow)?;
	let quotient = num_intermediate.checked_div(&q_big).ok_or(ArithmeticError::DivisionByZero)?;
	let remainder = num_intermediate % &q_big;
	let mut result_big = quotient;
	let one_big = BigUint::one();
	let two_big = BigUint::from(2u32);
	let threshold_big = q_big.checked_div(&two_big).ok_or(ArithmeticError::Overflow)?;
	let tie_breaker_big = q_big % &two_big;

	match rounding {
		Rounding::Down => {},
		Rounding::Up =>
			if !remainder.is_zero() {
				result_big = result_big.checked_add(&one_big).ok_or(ArithmeticError::Overflow)?;
			},
		Rounding::NearestPrefDown =>
			if remainder > threshold_big {
				result_big = result_big.checked_add(&one_big).ok_or(ArithmeticError::Overflow)?;
			},
		Rounding::NearestPrefUp => {
			let threshold_with_tie =
				threshold_big.checked_add(&tie_breaker_big).ok_or(ArithmeticError::Overflow)?;
			if remainder >= threshold_with_tie {
				result_big = result_big.checked_add(&one_big).ok_or(ArithmeticError::Overflow)?;
			}
		},
	}

	if result_big > inner_max_big {
		return Err(ArithmeticError::Overflow);
	}

	let result_inner = P::Inner::try_from(result_big.to_u128().ok_or(ArithmeticError::Overflow)?)
		.map_err(|_| ArithmeticError::Overflow)?;

	Ok(P::from_parts(result_inner))
}

// Simulates checked_square using BigUint
fn oracle_checked_square<P: PerThing>(pt1: P) -> Result<P, ArithmeticError>
where
	P::Inner: Into<u128> + NZero + NOne + Copy + NBounded + TryFrom<u128>,
	<P::Inner as TryFrom<u128>>::Error: core::fmt::Debug,
	P::Upper: NBounded + TryFrom<BigUint> + Into<BigUint> + Copy + Into<u128>,
	<P::Upper as TryFrom<BigUint>>::Error: core::fmt::Debug,
{
	let p_inner = pt1.deconstruct();
	let q_inner = P::ACCURACY;
	let p_upper_as_u128 = <P::Upper as Into<u128>>::into(P::Upper::from(p_inner));
	let p_upper_big = BigUint::from(p_upper_as_u128);
	let upper_max_big = BigUint::from(<P::Upper as Into<u128>>::into(P::Upper::max_value()));
	let p_squared_big = p_upper_big.checked_mul(&p_upper_big).ok_or(ArithmeticError::Overflow)?;

	if p_squared_big > upper_max_big {
		return Err(ArithmeticError::Overflow);
	}

	let q_big = BigUint::from(q_inner.into());
	let p_inner_big = BigUint::from(p_inner.into());
	let p_inner_squared_big =
		p_inner_big.checked_mul(&p_inner_big).ok_or(ArithmeticError::Overflow)?;

	oracle_checked_from_rational_with_rounding::<P>(p_inner_squared_big, q_big, Rounding::Down)
}

fn main() {
	loop {
		fuzz!(|data: (
			u64,
			u64,
			u64,
			u64,
			u64,
			ArbitraryRounding,
			PerThingType,
			PerThingArithOp,
		)| {
			let (p1, q1, p2, q2, int_operand, arb_rounding, per_thing_type, operation) = data;
			let rounding_mode = arb_rounding.0;
			let safe_q1 = q1.max(1);
			let safe_p1 = p1.min(safe_q1);
			let safe_q2 = q2.max(1);
			let safe_p2 = p2.min(safe_q2);

			match per_thing_type {
				PerThingType::Percent => run_test::<Percent>(
					safe_p1,
					safe_q1,
					safe_p2,
					safe_q2,
					int_operand,
					rounding_mode,
					operation,
				),
				PerThingType::Permill => run_test::<Permill>(
					safe_p1,
					safe_q1,
					safe_p2,
					safe_q2,
					int_operand,
					rounding_mode,
					operation,
				),
				PerThingType::Perbill => run_test::<Perbill>(
					safe_p1,
					safe_q1,
					safe_p2,
					safe_q2,
					int_operand,
					rounding_mode,
					operation,
				),
				PerThingType::PerU16 => run_test::<PerU16>(
					safe_p1,
					safe_q1,
					safe_p2,
					safe_q2,
					int_operand,
					rounding_mode,
					operation,
				),
				PerThingType::Perquintill => run_test::<Perquintill>(
					safe_p1,
					safe_q1,
					safe_p2,
					safe_q2,
					int_operand,
					rounding_mode,
					operation,
				),
			}
		});
	}
}

fn run_test<P: PerThing>(
	p1: u64,
	q1: u64,
	p2: u64,
	q2: u64,
	int_operand: u64,
	rounding_mode: Rounding,
	op: PerThingArithOp,
) where
	P::Inner: Into<u128>
		+ sp_arithmetic::traits::Zero
		+ sp_arithmetic::traits::One
		+ PartialOrd
		+ Copy
		+ NBounded
		+ TryInto<u64>
		+ TryFrom<u128>,
	<P::Inner as TryInto<u64>>::Error: core::fmt::Debug,
	<P::Inner as TryFrom<u128>>::Error: core::fmt::Debug,
	P::Upper: NBounded + TryFrom<BigUint> + Into<BigUint> + Copy + Into<u128>,
	<P::Upper as TryFrom<BigUint>>::Error: core::fmt::Debug,
	P: core::fmt::Debug,
	u64: sp_arithmetic::per_things::MultiplyArg
		+ sp_arithmetic::traits::UniqueSaturatedInto<P::Inner>
		+ sp_arithmetic::traits::CheckedAdd
		+ sp_arithmetic::traits::CheckedSub
		+ sp_arithmetic::traits::CheckedMul
		+ sp_arithmetic::traits::CheckedDiv,
	P::Inner: Into<u64>
		+ sp_arithmetic::traits::CheckedAdd
		+ sp_arithmetic::traits::CheckedSub
		+ sp_arithmetic::traits::CheckedMul
		+ sp_arithmetic::traits::CheckedDiv,
	P::Upper: sp_arithmetic::traits::CheckedMul<Output = P::Upper>
		+ sp_arithmetic::traits::CheckedDiv<Output = P::Upper>
		+ sp_arithmetic::traits::CheckedRem<Output = P::Upper>,
	u64: sp_arithmetic::per_things::ReciprocalArg,
{
	let pt1 = P::from_rational_with_rounding(p1, q1, Rounding::Down).unwrap_or_else(|_| P::zero());
	let pt2 = P::from_rational_with_rounding(p2, q2, Rounding::Down).unwrap_or_else(|_| P::zero());

	match op {
		PerThingArithOp::CheckedMulFloor => {
			let res = pt1.checked_mul_floor(int_operand);
			let oracle_res = oracle_checked_mul::<P>(pt1, int_operand, Rounding::Down);
			assert_eq!(
				res, oracle_res,
				"CheckedMulFloor mismatch: pt1={:?}, int_operand={}, res={:?}, expected={:?}",
				pt1, int_operand, res, oracle_res
			);
		},
		PerThingArithOp::CheckedMulCeil => {
			let res = pt1.checked_mul_ceil(int_operand);
			let oracle_res = oracle_checked_mul::<P>(pt1, int_operand, Rounding::Up);
			assert_eq!(
				res, oracle_res,
				"CheckedMulCeil mismatch: pt1={:?}, int_operand={}, res={:?}, expected={:?}",
				pt1, int_operand, res, oracle_res
			);
		},
		PerThingArithOp::CheckedReciprocalMulFloor => {
			let res = pt1.checked_saturating_reciprocal_mul_floor(int_operand);
			let oracle_res = oracle_checked_reciprocal_mul::<P>(pt1, int_operand, Rounding::Down);
			assert_eq!(
				res,
				oracle_res,
				"CheckedReciprocalMulFloor mismatch: pt1={:?}, int_operand={}, res={:?}, expected={:?}",
				pt1,
				int_operand,
				res,
				oracle_res
			);
		},
		PerThingArithOp::CheckedReciprocalMulCeil => {
			let res = pt1.checked_saturating_reciprocal_mul_ceil(int_operand);
			let oracle_res = oracle_checked_reciprocal_mul::<P>(pt1, int_operand, Rounding::Up);
			assert_eq!(
				res,
				oracle_res,
				"CheckedReciprocalMulCeil mismatch: pt1={:?}, int_operand={}, res={:?}, expected={:?}",
				pt1,
				int_operand,
				res,
				oracle_res
			);
		},
		PerThingArithOp::CheckedReciprocalMul => {
			let res = pt1.checked_saturating_reciprocal_mul(int_operand);
			let oracle_res =
				oracle_checked_reciprocal_mul::<P>(pt1, int_operand, Rounding::NearestPrefUp);
			assert_eq!(
				res, oracle_res,
				"CheckedReciprocalMul mismatch: pt1={:?}, int_operand={}, res={:?}, expected={:?}",
				pt1, int_operand, res, oracle_res
			);
		},
		PerThingArithOp::CheckedSquare => {
			let res = pt1.checked_square();
			let oracle_res = oracle_checked_square::<P>(pt1);
			assert_eq!(
				res, oracle_res,
				"CheckedSquare mismatch: pt1={:?}, res={:?}, expected={:?}",
				pt1, res, oracle_res
			);
		},
		PerThingArithOp::CheckedDivWithRounding => {
			let res = pt1.checked_div_with_rounding(pt2, rounding_mode);
			let oracle_res = oracle_checked_from_rational_with_rounding::<P>(
				BigUint::from(<P::Inner as Into<u128>>::into(pt1.deconstruct())),
				BigUint::from(<P::Inner as Into<u128>>::into(pt2.deconstruct())),
				rounding_mode,
			);
			assert_eq!(res,
				oracle_res,
				"CheckedDivWithRounding mismatch: pt1={:?}, pt2={:?}, rounding={:?}, res={:?}, expected={:?}",
				pt1,
				pt2,
				rounding_mode,
				res,
				oracle_res
			);
		},
		PerThingArithOp::CheckedIntDiv => {
			let res = pt1.checked_int_div(pt2);
			if pt2.is_zero() {
				assert_eq!(res, Err(ArithmeticError::DivisionByZero));
			} else {
				let expected = pt1.deconstruct().checked_div(&pt2.deconstruct());
				assert_eq!(res.ok(), expected);
			}
		},
	}
}
