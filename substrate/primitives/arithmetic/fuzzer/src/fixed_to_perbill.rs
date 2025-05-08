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
//! Running this fuzzer can be done with `cargo hfuzz run fixed_to_perbill`.
//! `honggfuzz` CLI options can be used by setting `HFUZZ_RUN_ARGS`, such as `-n 4` to use 4
//! threads.
//!
//! # Debugging a panic
//! Once a panic is found, it can be debugged with
//! `cargo hfuzz run-debug fixed_to_perbill hfuzz_workspace/fixed_to_perbill/*.fuzz`.
//!
//! # More information
//! More information about `honggfuzz` can be found
//! [here](https://docs.rs/honggfuzz/).

use honggfuzz::fuzz;
use sp_arithmetic::{
	traits::{One, Zero},
	ArithmeticError, FixedI128, FixedI64, FixedPointNumber, FixedU128, FixedU64, Perbill,
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

fn main() {
	loop {
		fuzz!(|data: (i128, FixedTypeSelector)| {
			let (val1_i128, type_selector) = data;

			match type_selector {
				FixedTypeSelector::I64 => run_test::<FixedI64>(val1_i128),
				FixedTypeSelector::U64 => run_test::<FixedU64>(val1_i128),
				FixedTypeSelector::I128 => run_test::<FixedI128>(val1_i128),
				FixedTypeSelector::U128 => run_test::<FixedU128>(val1_i128),
			}
		});
	}
}

// Need a helper trait because try_into_perbill is not on FixedPointNumber
trait TryIntoPerbillHelper: FixedPointNumber + Sized {
	fn try_into_perbill_inherent(self) -> Result<Perbill, ArithmeticError>;
}

impl TryIntoPerbillHelper for FixedI64 {
	fn try_into_perbill_inherent(self) -> Result<Perbill, ArithmeticError> {
		self.try_into_perbill()
	}
}

impl TryIntoPerbillHelper for FixedU64 {
	fn try_into_perbill_inherent(self) -> Result<Perbill, ArithmeticError> {
		self.try_into_perbill()
	}
}

impl TryIntoPerbillHelper for FixedI128 {
	fn try_into_perbill_inherent(self) -> Result<Perbill, ArithmeticError> {
		self.try_into_perbill()
	}
}

impl TryIntoPerbillHelper for FixedU128 {
	fn try_into_perbill_inherent(self) -> Result<Perbill, ArithmeticError> {
		self.try_into_perbill()
	}
}

fn run_test<F: FixedPointNumber + core::fmt::Display + TryIntoPerbillHelper>(v1: i128)
where
	F::Inner: PartialOrd + Copy + Zero + One,
{
	let fp1 = F::saturating_from_integer(v1);
	let res = fp1.try_into_perbill_inherent();

	if fp1 < F::zero() {
		assert_eq!(res, Ok(Perbill::zero()), "try_into_perbill failed for negative. fp1={}", fp1);
	} else if fp1 >= F::one() {
		assert_eq!(res, Ok(Perbill::one()), "try_into_perbill failed for >= 1. fp1={}", fp1);
	} else {
		match res {
			Ok(pb) => {
				// Basic check: result should be <= Perbill::one()
				assert!(
					pb <= Perbill::one(),
					"try_into_perbill result > 1. fp1={}, res={:?}",
					fp1,
					pb
				);
			},
			Err(e) => {
				// This error should only happen if the internal multiply_by_rational overflows,
				// even though the conceptual value is between 0 and 1.
				assert_eq!(
					e,
					ArithmeticError::Overflow,
					"try_into_perbill unexpected error. fp1={}",
					fp1
				);
			},
		}
	}
}
