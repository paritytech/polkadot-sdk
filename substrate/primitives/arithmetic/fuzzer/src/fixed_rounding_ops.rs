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
//! Running this fuzzer can be done with `cargo hfuzz run fixed_rounding_ops`.
//! `honggfuzz` CLI options can be used by setting `HFUZZ_RUN_ARGS`, such as `-n 4` to use 4
//! threads.
//!
//! # Debugging a panic
//! Once a panic is found, it can be debugged with
//! `cargo hfuzz run-debug fixed_rounding_ops hfuzz_workspace/fixed_rounding_ops/*.fuzz`.
//!
//! # More information
//! More information about `honggfuzz` can be found
//! [here](https://docs.rs/honggfuzz/).

use honggfuzz::fuzz;
use num_bigint::BigInt;
use sp_arithmetic::{
	traits::{Bounded, One, Saturating, Zero},
	FixedI128, FixedI64, FixedPointNumber, FixedU128, FixedU64, SignedRounding,
	SignedRounding::*,
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
struct ArbitrarySignedRounding(SignedRounding);

impl arbitrary::Arbitrary<'_> for ArbitrarySignedRounding {
	fn arbitrary(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Self> {
		Ok(Self(match u.int_in_range(0..=7)? {
			0 => High,
			1 => Low,
			2 => NearestPrefHigh,
			3 => NearestPrefLow,
			4 => Major,
			5 => Minor,
			6 => NearestPrefMajor,
			7 => NearestPrefMinor,
			_ => unreachable!(),
		}))
	}
}

#[derive(Debug, Clone, Copy)]
enum RoundingOperationSelector {
	Mul,
	Div,
}

impl arbitrary::Arbitrary<'_> for RoundingOperationSelector {
	fn arbitrary(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Self> {
		Ok(match u.int_in_range(0..=1)? {
			0 => RoundingOperationSelector::Mul,
			1 => RoundingOperationSelector::Div,
			_ => unreachable!(),
		})
	}
}

fn main() {
	loop {
		fuzz!(|data: (
			i128,
			i128,
			FixedTypeSelector,
			ArbitrarySignedRounding,
			RoundingOperationSelector,
		)| {
			let (v1, v2, type_selector, arb_rounding, op_selector) = data;
			let rounding_mode = arb_rounding.0;

			match type_selector {
				FixedTypeSelector::I64 => run_test::<FixedI64>(v1, v2, rounding_mode, op_selector),
				FixedTypeSelector::U64 => run_test::<FixedU64>(v1, v2, rounding_mode, op_selector),
				FixedTypeSelector::I128 =>
					run_test::<FixedI128>(v1, v2, rounding_mode, op_selector),
				FixedTypeSelector::U128 =>
					run_test::<FixedU128>(v1, v2, rounding_mode, op_selector),
			}
		});
	}
}

// Helper trait to access the const checked methods, as they are not on FixedPointNumber
trait ConstCheckedRoundingOps<Rhs> {
	fn const_checked_mul_with_rounding(&self, other: Rhs, rounding: SignedRounding) -> Option<Self>
	where
		Self: Sized;
	fn checked_rounding_div(&self, other: Rhs, rounding: SignedRounding) -> Option<Self>
	where
		Self: Sized;
}

impl ConstCheckedRoundingOps<FixedI64> for FixedI64 {
	fn const_checked_mul_with_rounding(
		&self,
		other: FixedI64,
		rounding: SignedRounding,
	) -> Option<Self> {
		FixedI64::const_checked_mul_with_rounding(*self, other, rounding)
	}
	fn checked_rounding_div(&self, other: FixedI64, rounding: SignedRounding) -> Option<Self> {
		FixedI64::checked_rounding_div(*self, other, rounding)
	}
}

impl ConstCheckedRoundingOps<FixedU64> for FixedU64 {
	fn const_checked_mul_with_rounding(
		&self,
		other: FixedU64,
		rounding: SignedRounding,
	) -> Option<Self> {
		FixedU64::const_checked_mul_with_rounding(*self, other, rounding)
	}
	fn checked_rounding_div(&self, other: FixedU64, rounding: SignedRounding) -> Option<Self> {
		FixedU64::checked_rounding_div(*self, other, rounding)
	}
}

impl ConstCheckedRoundingOps<FixedI128> for FixedI128 {
	fn const_checked_mul_with_rounding(
		&self,
		other: FixedI128,
		rounding: SignedRounding,
	) -> Option<Self> {
		FixedI128::const_checked_mul_with_rounding(*self, other, rounding)
	}
	fn checked_rounding_div(&self, other: FixedI128, rounding: SignedRounding) -> Option<Self> {
		FixedI128::checked_rounding_div(*self, other, rounding)
	}
}

impl ConstCheckedRoundingOps<FixedU128> for FixedU128 {
	fn const_checked_mul_with_rounding(
		&self,
		other: FixedU128,
		rounding: SignedRounding,
	) -> Option<Self> {
		FixedU128::const_checked_mul_with_rounding(*self, other, rounding)
	}
	fn checked_rounding_div(&self, other: FixedU128, rounding: SignedRounding) -> Option<Self> {
		FixedU128::checked_rounding_div(*self, other, rounding)
	}
}

trait AbsExt {
	fn abs_ext(self) -> Self;
}
impl AbsExt for i64 {
	fn abs_ext(self) -> Self {
		i64::saturating_abs(self)
	}
}
impl AbsExt for u64 {
	fn abs_ext(self) -> Self {
		self
	}
}
impl AbsExt for i128 {
	fn abs_ext(self) -> Self {
		i128::saturating_abs(self)
	}
}
impl AbsExt for u128 {
	fn abs_ext(self) -> Self {
		self
	}
}

fn run_test<F: FixedPointNumber + core::fmt::Display>(
	v1: i128,
	v2: i128,
	rounding_mode: SignedRounding,
	op: RoundingOperationSelector,
) where
	F::Inner: TryInto<i128> + Default + PartialOrd + Copy + Bounded + Zero + One,
	<F::Inner as TryInto<i128>>::Error: core::fmt::Debug,
	F: ConstCheckedRoundingOps<F>,
	F::Inner: AbsExt,
{
	let fp1 = F::saturating_from_integer(v1);
	let fp2 = F::saturating_from_integer(v2);

	match op {
		RoundingOperationSelector::Mul => {
			let res = fp1.const_checked_mul_with_rounding(fp2, rounding_mode);
			let fp1_inner_big =
				BigInt::from(fp1.into_inner().try_into().unwrap_or(Default::default()));
			let fp2_inner_big =
				BigInt::from(fp2.into_inner().try_into().unwrap_or(Default::default()));
			let div_big = BigInt::from(F::DIV.try_into().unwrap_or(1i128));
			let expected_inner_big = (fp1_inner_big * fp2_inner_big) / &div_big;
			let max_inner_big = BigInt::from(F::Inner::max_value().try_into().unwrap_or(i128::MAX));
			let min_inner_big = BigInt::from(F::Inner::min_value().try_into().unwrap_or(i128::MIN));

			if expected_inner_big > max_inner_big || expected_inner_big < min_inner_big {
				assert!(
					res.is_none(),
					"Expected None for Mul overflow. fp1={}, fp2={}, rounding={:?}",
					fp1,
					fp2,
					rounding_mode
				);
			} else {
				if let Some(val) = res {
					let default_res = fp1.checked_mul(&fp2);
					if let Some(default_val) = default_res {
						let inner_diff =
							val.into_inner().saturating_sub(default_val.into_inner()).abs_ext();
						assert!(
							inner_diff <= F::Inner::one(),
							"Mul rounding diff > 1 epsilon. fp1={}, fp2={}, rounding={:?}, default_res={:?}, specific_res={:?}",
							fp1,
							fp2,
							rounding_mode,
							default_val,
							val);
					}
				}

				// More precise checks could be added here based on rounding mode properties
			}
		},
		RoundingOperationSelector::Div => {
			if fp2.is_zero() {
				assert_eq!(
					fp1.checked_rounding_div(fp2, rounding_mode),
					None,
					"Expected None for Div by zero. fp1={}, fp2={}, rounding={:?}",
					fp1,
					fp2,
					rounding_mode
				);
			} else {
				let res = fp1.checked_rounding_div(fp2, rounding_mode);
				let fp1_inner_big =
					BigInt::from(fp1.into_inner().try_into().unwrap_or(Default::default()));
				let fp2_inner_big =
					BigInt::from(fp2.into_inner().try_into().unwrap_or(Default::default()));
				let div_big = BigInt::from(F::DIV.try_into().unwrap_or(1i128));
				let expected_inner_big = (fp1_inner_big * &div_big) / fp2_inner_big;
				let max_inner_big =
					BigInt::from(F::Inner::max_value().try_into().unwrap_or(i128::MAX));
				let min_inner_big =
					BigInt::from(F::Inner::min_value().try_into().unwrap_or(i128::MIN));

				if expected_inner_big > max_inner_big || expected_inner_big < min_inner_big {
					assert!(
						res.is_none(),
						"Expected None for Div overflow. fp1={}, fp2={}, rounding={:?}",
						fp1,
						fp2,
						rounding_mode
					);
				} else {
					if let Some(val) = res {
						let default_res = fp1.checked_div(&fp2);
						if let Some(default_val) = default_res {
							let inner_diff =
								val.into_inner().saturating_sub(default_val.into_inner()).abs_ext();
							assert!(
								inner_diff <= F::Inner::one(),
								"Div rounding diff > 1 epsilon. fp1={}, fp2={}, rounding={:?}, default_res={:?}, specific_res={:?}",
								fp1,
								fp2,
								rounding_mode,
								default_val,
								val
							);
						}
					}

					// More precise checks could be added here based on rounding mode properties
				}
			}
		},
	}
}
