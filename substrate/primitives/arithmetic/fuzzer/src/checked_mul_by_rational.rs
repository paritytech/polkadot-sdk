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
//! Running this fuzzer can be done with `cargo hfuzz run checked_mul_by_rational`.
//! `honggfuzz` CLI options can be used by setting `HFUZZ_RUN_ARGS`, such as `-n 4` to use 4
//! threads.
//!
//! # Debugging a panic
//! Once a panic is found, it can be debugged with
//! `cargo hfuzz run-debug checked_mul_by_rational hfuzz_workspace/checked_mul_by_rational/*.fuzz`.
//!
//! # More information
//! More information about `honggfuzz` can be found
//! [here](https://docs.rs/honggfuzz/).

use honggfuzz::fuzz;
use sp_arithmetic::{
	helpers_128bit::{
		checked_multiply_by_rational_with_rounding, multiply_by_rational_with_rounding,
	},
	ArithmeticError, Rounding,
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

fn main() {
	loop {
		fuzz!(|data: (u128, u128, u128, ArbitraryRounding)| {
			let (a, b, c, arbitrary_rounding) = data;
			let r = arbitrary_rounding.0;

			let checked_res = checked_multiply_by_rational_with_rounding(a, b, c, r);
			let non_checked_res = multiply_by_rational_with_rounding(a, b, c, r);

			if c == 0 {
				assert_eq!(
					checked_res,
					Err(ArithmeticError::DivisionByZero),
					"Denominator 0: checked failed. a={}, b={}, c={}, r={:?}",
					a,
					b,
					c,
					r
				);
				assert_eq!(
					non_checked_res, None,
					"Denominator 0: non-checked failed. a={}, b={}, c={}, r={:?}",
					a, b, c, r
				);
			} else {
				match non_checked_res {
					Some(val_non_checked) => {
						assert_eq!(
							checked_res,
							Ok(val_non_checked),
							"Non-checked Some, Checked Ok mismatch. a={}, b={}, c={}, r={:?}",
							a,
							b,
							c,
							r
						);
					},
					None => {
						// This implies an overflow for the non-checked version.
						assert_eq!(
							checked_res,
							Err(ArithmeticError::Overflow),
							"Non-checked None, Checked Overflow mismatch. a={}, b={}, c={}, r={:?}",
							a,
							b,
							c,
							r
						);
					},
				}
			}
		});
	}
}
