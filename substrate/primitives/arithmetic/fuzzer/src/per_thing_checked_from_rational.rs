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
//! Running this fuzzer can be done with `cargo hfuzz run per_thing_checked_from_rational`.
//! `honggfuzz` CLI options can be used by setting `HFUZZ_RUN_ARGS`, such as `-n 4` to use 4
//! threads.
//!
//! # Debugging a panic
//! Once a panic is found, it can be debugged with
//! `cargo hfuzz run-debug per_thing_checked_from_rational
//! hfuzz_workspace/per_thing_checked_from_rational/*.fuzz`.
//!
//! # More information
//! More information about `honggfuzz` can be found
//! [here](https://docs.rs/honggfuzz/).

use honggfuzz::fuzz;
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

fn main() {
	loop {
		fuzz!(|data: (u128, u128, ArbitraryRounding, PerThingType)| {
			let (p_u128, q_u128, arbitrary_rounding, per_thing_type) = data;
			let r = arbitrary_rounding.0;

			// The PerThing::from_rational_with_rounding and its checked variant
			// are generic over N for p and q. We'll use u128 for this fuzzer.
			// If other types for N were needed, the input struct would need to change.
			match per_thing_type {
				PerThingType::Percent => check::<Percent>(p_u128, q_u128, r),
				PerThingType::Permill => check::<Permill>(p_u128, q_u128, r),
				PerThingType::Perbill => check::<Perbill>(p_u128, q_u128, r),
				PerThingType::PerU16 => check::<PerU16>(p_u128, q_u128, r),
				PerThingType::Perquintill => check::<Perquintill>(p_u128, q_u128, r),
			}
		});
	}
}

fn check<P: PerThing>(p: u128, q: u128, r: Rounding)
where
	P::Inner:
		Into<u128> + sp_arithmetic::traits::Zero + sp_arithmetic::traits::One + PartialOrd + Copy,
	P: core::fmt::Debug,
{
	let checked_res = P::checked_from_rational_with_rounding(p, q, r);
	let non_checked_res = P::from_rational_with_rounding(p, q, r);

	if q == 0 {
		assert_eq!(
			checked_res,
			Err(ArithmeticError::DivisionByZero),
			"Denominator 0: checked failed. p={}, q={}, r={:?}, PerThingType: {:?}",
			p,
			q,
			r,
			core::any::type_name::<P>()
		);
		assert_eq!(
			non_checked_res,
			Err(()),
			"Denominator 0: non-checked failed. p={}, q={}, r={:?}, PerThingType: {:?}",
			p,
			q,
			r,
			core::any::type_name::<P>()
		);
	} else if p > q {
		assert_eq!(
			checked_res,
			Err(ArithmeticError::Overflow),
			"p > q: checked failed. p={}, q={}, r={:?}, PerThingType: {:?}",
			p,
			q,
			r,
			core::any::type_name::<P>()
		);
		assert_eq!(
			non_checked_res,
			Err(()),
			"p > q: non-checked failed. p={}, q={}, r={:?}, PerThingType: {:?}",
			p,
			q,
			r,
			core::any::type_name::<P>()
		);
	} else {
		match non_checked_res {
			Ok(val_non_checked) => {
				assert_eq!(
					checked_res.map(|v| v.deconstruct()),
					Ok(val_non_checked.deconstruct()),
					"Non-checked Ok, Checked Ok mismatch. p={}, q={}, r={:?}, PerThingType: {:?}",
					p,
					q,
					r,
					core::any::type_name::<P>()
				);
			},
			Err(_) => {
				assert_eq!(
					checked_res,
					Err(ArithmeticError::Overflow),
					"Non-checked Err, Checked Overflow mismatch. p={}, q={}, r={:?}, PerThingType: {:?}",
					p, q, r, core::any::type_name::<P>()
				);
			},
		}
	}
}
