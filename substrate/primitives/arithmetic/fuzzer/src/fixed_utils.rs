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
//! Running this fuzzer can be done with `cargo hfuzz run fixed_utils`.
//! `honggfuzz` CLI options can be used by setting `HFUZZ_RUN_ARGS`, such as `-n 4` to use 4
//! threads.
//!
//! # Debugging a panic
//! Once a panic is found, it can be debugged with
//! `cargo hfuzz run-debug fixed_utils hfuzz_workspace/fixed_utils/*.fuzz`.
//!
//! # More information
//! More information about `honggfuzz` can be found
//! [here](https://docs.rs/honggfuzz/).

use core::{convert::TryInto, ops::Rem};
use honggfuzz::fuzz;
use sp_arithmetic::{
	traits::{Bounded, One, Zero},
	FixedI128, FixedI64, FixedPointNumber, FixedU128, FixedU64,
};

#[derive(Debug, Clone, Copy)]
enum FixedTypeSelector {
	I64,
	U64,
	I128,
	U128,
}

impl arbitrary::Arbitrary<'_> for FixedTypeSelector {
	fn arbitrary(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Self> {
		Ok(match u.int_in_range(0..=3)? {
			0 => FixedTypeSelector::I64,
			1 => FixedTypeSelector::U64,
			2 => FixedTypeSelector::I128,
			3 => FixedTypeSelector::U128,
			_ => unreachable!(),
		})
	}
}

#[derive(Debug, Clone, Copy)]
enum FixedUtilOp {
	Trunc,
	Frac,
	Ceil,
	Floor,
	Round,
}

impl arbitrary::Arbitrary<'_> for FixedUtilOp {
	fn arbitrary(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Self> {
		Ok(match u.int_in_range(0..=4)? {
			0 => FixedUtilOp::Trunc,
			1 => FixedUtilOp::Frac,
			2 => FixedUtilOp::Ceil,
			3 => FixedUtilOp::Floor,
			4 => FixedUtilOp::Round,
			_ => unreachable!(),
		})
	}
}

fn main() {
	loop {
		fuzz!(|data: (i128, FixedTypeSelector, FixedUtilOp)| {
			let (val1_i128, type_selector, op_selector) = data;

			match type_selector {
				FixedTypeSelector::I64 => run_test::<FixedI64>(val1_i128, op_selector),
				FixedTypeSelector::U64 => run_test::<FixedU64>(val1_i128, op_selector),
				FixedTypeSelector::I128 => run_test::<FixedI128>(val1_i128, op_selector),
				FixedTypeSelector::U128 => run_test::<FixedU128>(val1_i128, op_selector),
			}
		});
	}
}

fn run_test<F: FixedPointNumber + core::fmt::Display>(v1: i128, op: FixedUtilOp)
where
	F::Inner: TryInto<u128> + PartialOrd + Copy + Bounded + Zero + One + Rem<Output = F::Inner>,
	<F::Inner as TryInto<u128>>::Error: core::fmt::Debug,
{
	let fp1 = F::saturating_from_integer(v1);

	match op {
		FixedUtilOp::Trunc => {
			let trunc_val = fp1.trunc();
			let is_multiple = trunc_val.into_inner() % F::DIV == F::Inner::zero();
			assert!(
				is_multiple,
				"trunc() result not multiple of DIV. fp1={}, trunc={}",
				fp1, trunc_val
			);

			let diff = fp1.saturating_sub(trunc_val).saturating_abs();
			assert!(
				diff < F::one(),
				"trunc() difference >= 1.0. fp1={}, trunc={}, diff={}",
				fp1,
				trunc_val,
				diff
			);
		},
		FixedUtilOp::Frac => {
			let frac_val = fp1.frac();
			assert!(
				frac_val >= F::zero(),
				"frac() result negative. fp1={}, frac={}",
				fp1,
				frac_val
			);
			assert!(frac_val < F::one(), "frac() result >= 1.0. fp1={}, frac={}", fp1, frac_val);
			assert!(fp1.trunc().frac().is_zero(), "trunc().frac() not zero. fp1={}", fp1);
		},
		FixedUtilOp::Ceil => {
			let ceil_val = fp1.ceil();
			assert!(ceil_val >= fp1, "ceil() result < input. fp1={}, ceil={}", fp1, ceil_val);

			let is_multiple = ceil_val.into_inner() % F::DIV == F::Inner::zero();
			assert!(
				is_multiple,
				"ceil() result not multiple of DIV. fp1={}, ceil={}",
				fp1, ceil_val
			);

			let diff = ceil_val.saturating_sub(fp1);
			assert!(
				diff < F::one(),
				"ceil() difference >= 1.0. fp1={}, ceil={}, diff={}",
				fp1,
				ceil_val,
				diff
			);
		},
		FixedUtilOp::Floor => {
			let floor_val = fp1.floor();
			assert!(floor_val <= fp1, "floor() result > input. fp1={}, floor={}", fp1, floor_val);

			let is_multiple = floor_val.into_inner() % F::DIV == F::Inner::zero();
			assert!(
				is_multiple,
				"floor() result not multiple of DIV. fp1={}, floor={}",
				fp1, floor_val
			);

			let diff = fp1.saturating_sub(floor_val);
			assert!(
				diff < F::one(),
				"floor() difference >= 1.0. fp1={}, floor={}, diff={}",
				fp1,
				floor_val,
				diff
			);
		},
		FixedUtilOp::Round => {
			let round_val = fp1.round();
			let is_multiple = round_val.into_inner() % F::DIV == F::Inner::zero();
			assert!(
				is_multiple,
				"round() result not multiple of DIV. fp1={}, round={}",
				fp1, round_val
			);
			assert!(
				round_val == fp1.floor() || round_val == fp1.ceil(),
				"round() not floor or ceil. fp1={}, round={}",
				fp1,
				round_val
			);

			let diff = fp1.saturating_sub(round_val).saturating_abs();
			assert!(
				diff <= F::saturating_from_rational(1, 2),
				"round() difference > 0.5. fp1={}, round={}, diff={}",
				fp1,
				round_val,
				diff
			);
		},
	}
}
