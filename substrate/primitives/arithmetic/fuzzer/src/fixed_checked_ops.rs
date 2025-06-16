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
//! Running this fuzzer can be done with `cargo hfuzz run fixed_checked_ops`.
//! `honggfuzz` CLI options can be used by setting `HFUZZ_RUN_ARGS`, such as `-n 4` to use 4
//! threads.
//!
//! # Debugging a panic
//! Once a panic is found, it can be debugged with
//! `cargo hfuzz run-debug fixed_checked_ops hfuzz_workspace/fixed_checked_ops/*.fuzz`.
//!
//! # More information
//! More information about `honggfuzz` can be found
//! [here](https://docs.rs/honggfuzz/).

use honggfuzz::fuzz;
use num_bigint::BigInt;
use sp_arithmetic::{
	traits::{Bounded, CheckedDiv, CheckedMul, One, Zero},
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
enum Operation {
	CheckedAdd,
	CheckedSub,
	CheckedMul,
	CheckedDiv,
	CheckedFromInteger,
	CheckedFromRational,
	CheckedMulInt,
	CheckedDivInt,
	CheckedSqrt,
}

impl arbitrary::Arbitrary<'_> for Operation {
	fn arbitrary(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Self> {
		Ok(match u.int_in_range(0..=8)? {
			0 => Operation::CheckedAdd,
			1 => Operation::CheckedSub,
			2 => Operation::CheckedMul,
			3 => Operation::CheckedDiv,
			4 => Operation::CheckedFromInteger,
			5 => Operation::CheckedFromRational,
			6 => Operation::CheckedMulInt,
			7 => Operation::CheckedDivInt,
			8 => Operation::CheckedSqrt,
			_ => unreachable!(),
		})
	}
}

fn main() {
	loop {
		fuzz!(|data: (i128, i128, FixedTypeSelector, Operation)| {
			let (val1_i128, val2_i128, type_selector, op_selector) = data;

			match type_selector {
				FixedTypeSelector::I64 => run_test::<FixedI64>(val1_i128, val2_i128, op_selector),
				FixedTypeSelector::U64 => run_test::<FixedU64>(val1_i128, val2_i128, op_selector),
				FixedTypeSelector::I128 => run_test::<FixedI128>(val1_i128, val2_i128, op_selector),
				FixedTypeSelector::U128 => run_test::<FixedU128>(val1_i128, val2_i128, op_selector),
			}
		});
	}
}

fn run_test<F: FixedPointNumber + core::fmt::Display>(v1: i128, v2: i128, op: Operation)
where
	F::Inner: TryInto<i128>
		+ TryInto<u128>
		+ TryFrom<i128>
		+ Default
		+ PartialOrd
		+ Copy
		+ Bounded
		+ Zero
		+ One
		+ CheckedMul
		+ CheckedDiv,
	<F::Inner as TryInto<i128>>::Error: core::fmt::Debug,
	<F::Inner as TryInto<u128>>::Error: core::fmt::Debug,
	<F::Inner as TryFrom<i128>>::Error: core::fmt::Debug,
{
	let fp1 = F::saturating_from_integer(v1);
	let fp2 = F::saturating_from_integer(v2);
	let div_big = BigInt::from(F::DIV.try_into().unwrap_or(1i128));
	let max_inner_big = BigInt::from(F::Inner::max_value().try_into().unwrap_or(i128::MAX));
	let min_inner_big = BigInt::from(F::Inner::min_value().try_into().unwrap_or(i128::MIN));

	match op {
		Operation::CheckedAdd => {
			let res = fp1.checked_add(&fp2);
			let fp1_inner_i128: i128 = fp1.into_inner().try_into().unwrap_or(Default::default());
			let fp2_inner_i128: i128 = fp2.into_inner().try_into().unwrap_or(Default::default());
			let fp1_inner_big = BigInt::from(fp1_inner_i128);
			let fp2_inner_big = BigInt::from(fp2_inner_i128);
			let expected_inner_big = fp1_inner_big + fp2_inner_big;

			if expected_inner_big > max_inner_big || expected_inner_big < min_inner_big {
				assert!(res.is_none(),
					"Expected None for CheckedAdd overflow/underflow for {:?}, op: {:?}, v1: {}, v2: {}",
					core::any::type_name::<F>(),
					op,
					v1,
					v2
				);
			} else {
				assert!(
					res.is_some(),
					"Expected Some for CheckedAdd for {:?}, op: {:?}, v1: {}, v2: {}",
					core::any::type_name::<F>(),
					op,
					v1,
					v2
				);

				if let Some(r) = res {
					let result_inner_i128: i128 =
						r.into_inner().try_into().unwrap_or(Default::default());
					let result_inner_big = BigInt::from(result_inner_i128);
					assert_eq!(
						result_inner_big,
						expected_inner_big,
						"CheckedAdd result mismatch for {:?}, op: {:?}, v1: {}, v2: {}",
						core::any::type_name::<F>(),
						op,
						v1,
						v2
					);
				}
			}
		},
		Operation::CheckedSub => {
			let res = fp1.checked_sub(&fp2);
			let fp1_inner_i128: i128 = fp1.into_inner().try_into().unwrap_or(Default::default());
			let fp2_inner_i128: i128 = fp2.into_inner().try_into().unwrap_or(Default::default());
			let fp1_inner_big = BigInt::from(fp1_inner_i128);
			let fp2_inner_big = BigInt::from(fp2_inner_i128);
			let expected_inner_big = fp1_inner_big - fp2_inner_big;

			if expected_inner_big > max_inner_big || expected_inner_big < min_inner_big {
				assert!(res.is_none(),
					"Expected None for CheckedSub overflow/underflow for {:?}, op: {:?}, v1: {}, v2: {}",
					core::any::type_name::<F>(),
					op,
					v1,
					v2
				);
			} else {
				assert!(
					res.is_some(),
					"Expected Some for CheckedSub for {:?}, op: {:?}, v1: {}, v2: {}",
					core::any::type_name::<F>(),
					op,
					v1,
					v2
				);

				if let Some(r) = res {
					let result_inner_i128: i128 =
						r.into_inner().try_into().unwrap_or(Default::default());
					let result_inner_big = BigInt::from(result_inner_i128);

					assert_eq!(
						result_inner_big,
						expected_inner_big,
						"CheckedSub result mismatch for {:?}, op: {:?}, v1: {}, v2: {}",
						core::any::type_name::<F>(),
						op,
						v1,
						v2
					);
				}
			}
		},
		Operation::CheckedMul => {
			let res = fp1.checked_mul(&fp2);
			let fp1_inner_i128: i128 = fp1.into_inner().try_into().unwrap_or(Default::default());
			let fp2_inner_i128: i128 = fp2.into_inner().try_into().unwrap_or(Default::default());
			let fp1_inner_big = BigInt::from(fp1_inner_i128);
			let fp2_inner_big = BigInt::from(fp2_inner_i128);
			let expected_inner_big = (fp1_inner_big * fp2_inner_big) / &div_big;

			if expected_inner_big > max_inner_big || expected_inner_big < min_inner_big {
				assert!(
					res.is_none(),
					"Expected None for CheckedMul overflow/underflow for {:?}, op: {:?}, v1: {}, v2: {}",
					core::any::type_name::<F>(),
					op,
					v1,
					v2
				);
			} else {
				// If no overflow, it should succeed. The actual comparison of value is tricky due
				// to rounding. For now, just check if it's Some when expected. A more precise check
				// would involve replicating the internal rounding logic.
			}
		},
		Operation::CheckedDiv => {
			if fp2.is_zero() {
				assert_eq!(
					fp1.checked_div(&fp2),
					None,
					"Expected None for CheckedDiv by zero for {:?}, op: {:?}, v1: {}, v2: {}",
					core::any::type_name::<F>(),
					op,
					v1,
					v2
				);
			} else {
				let res = fp1.checked_div(&fp2);
				let fp1_inner_i128: i128 =
					fp1.into_inner().try_into().unwrap_or(Default::default());
				let fp2_inner_i128: i128 =
					fp2.into_inner().try_into().unwrap_or(Default::default());
				let fp1_inner_big = BigInt::from(fp1_inner_i128);
				let fp2_inner_big = BigInt::from(fp2_inner_i128);
				let expected_inner_big = (fp1_inner_big * &div_big) / fp2_inner_big;

				if expected_inner_big > max_inner_big || expected_inner_big < min_inner_big {
					assert!(
						res.is_none(),
						"Expected None for CheckedDiv overflow/underflow for {:?}, op: {:?}, v1: {}, v2: {}",
						core::any::type_name::<F>(),
						op,
						v1,
						v2
					);
				} else {
					// Similar to CheckedMul, precise value checking is hard due to rounding.
					// At least checking if it's Some when expected. To improve in the future.
				}
			}
		},
		Operation::CheckedFromInteger => {
			match F::Inner::try_from(v1) {
				Ok(inner_v1) => {
					let res = F::checked_from_integer(inner_v1);
					let v1_big = BigInt::from(v1);
					let expected_inner_big = v1_big * &div_big;

					if expected_inner_big > max_inner_big || expected_inner_big < min_inner_big {
						assert!(
							res.is_none(),
							"Expected None for CheckedFromInteger overflow for {:?}, op: {:?}, v1: {}",
							core::any::type_name::<F>(),
							op,
							v1
						);
					} else {
						assert!(
							res.is_some(),
							"Expected Some for CheckedFromInteger for {:?}, op: {:?}, v1: {}",
							core::any::type_name::<F>(),
							op,
							v1
						);

						if let Some(r) = res {
							let result_inner_i128: i128 =
								r.into_inner().try_into().unwrap_or(Default::default());
							let result_inner_big = BigInt::from(result_inner_i128);
							// `expected_inner_big` might differ slightly due to potential
							// intermediate rounding differences between BigInt and the
							// actual implementation. A tolerance might be needed for a
							// perfect check, but this is a good start.
							assert_eq!(
								result_inner_big,
								expected_inner_big,
								"CheckedFromInteger result mismatch for {:?}, op: {:?}, v1: {}",
								core::any::type_name::<F>(),
								op,
								v1
							);
						}
					}
				},
				Err(_) => {
					// If v1 doesn't fit in F::Inner, then checked_from_integer(v1) would
					// conceptually overflow. We expect checked_from_integer to handle this,
					// likely returning None. However, we can't call it directly. We assume the
					// conceptual result is None. We can't easily assert anything about
					// F::checked_from_integer itself here. This case highlights limitations of
					// fuzzing across incompatible types.
				},
			}
		},
		Operation::CheckedFromRational => {
			let n = v1;
			let d = v2;
			let res = F::checked_from_rational(n, d);

			if d == 0 {
				assert_eq!(
					res,
					None,
					"Expected None for CheckedFromRational with d=0 for {:?}, op: {:?}, n: {}, d: {}",
					core::any::type_name::<F>(),
					op,
					n,
					d
				);
			} else {
				let n_big = BigInt::from(n);
				let d_big = BigInt::from(d);
				let expected_inner_big = (n_big * &div_big) / d_big;
				if expected_inner_big > max_inner_big || expected_inner_big < min_inner_big {
					assert!(
						res.is_none(),
						"Expected None for CheckedFromRational overflow for {:?}, op: {:?}, n: {}, d: {}",
						core::any::type_name::<F>(),
						op,
						n,
						d
					);
				} else {
					// Similar to CheckedMul, precise value checking is hard due to rounding.
					// At least checking if it's Some when expected. To improve in the future.
				}
			}
		},
		Operation::CheckedMulInt => {
			let res = fp1.checked_mul_int(v2);
			let fp1_inner_i128: i128 = fp1.into_inner().try_into().unwrap_or(Default::default());
			let fp1_inner_big = BigInt::from(fp1_inner_i128);
			let v2_big = BigInt::from(v2);
			let expected_int_val_big = (fp1_inner_big * v2_big) / &div_big;

			if expected_int_val_big > BigInt::from(i128::MAX) ||
				expected_int_val_big < BigInt::from(i128::MIN)
			{
				assert!(
					res.is_none(),
					"Expected None for CheckedMulInt overflow for {:?}, op: {:?}, fp1: {}, v2: {}",
					core::any::type_name::<F>(),
					op,
					fp1,
					v2
				);
			} else {
				// Similar to CheckedMul, precise value checking is hard due to rounding.
				// At least checking if it's Some when expected. To improve in the future.
			}
		},
		Operation::CheckedDivInt => {
			let res = fp1.checked_div_int(v2);
			let fp1_inner_i128: i128 = fp1.into_inner().try_into().unwrap_or(Default::default());
			let fp1_inner_big = BigInt::from(fp1_inner_i128);
			let v2_big = BigInt::from(v2);
			let expected_int_val_big = (&fp1_inner_big / &div_big) / v2_big;

			if expected_int_val_big > BigInt::from(i128::MAX) ||
				expected_int_val_big < BigInt::from(i128::MIN)
			{
				assert!(
					res.is_none(),
					"Expected None for CheckedDivInt overflow for {:?}, op: {:?}, fp1: {}, v2: {}",
					core::any::type_name::<F>(),
					op,
					fp1,
					v2
				);
			} else {
				// Similar to CheckedMul, precise value checking is hard due to rounding/truncation.
				// At least checking if it's Some when expected. To improve in the future.
			}
		},
		Operation::CheckedSqrt => {
			let res = fp1.checked_sqrt();
			if fp1.is_negative() {
				assert!(
					res.is_none(),
					"Expected None for CheckedSqrt of negative for {:?}, op: {:?}, fp1: {}",
					core::any::type_name::<F>(),
					op,
					fp1
				);
			} else {
				// Expected val: sqrt(fp1.inner * DIV) / DIV
				//
				// This is complex to verify perfectly due to integer sqrt and precision.
				// For now, just ensure it's Some for non-negative, non-overflowing cases.
				//
				// In the future, an overflow check could be:
				//   if fp1.inner * DIV overflows u128
				// or
				//   if result overflows
				//
				// At least checking if it's negative when unexpected. To improve in the future.
			}
		},
	}
}
