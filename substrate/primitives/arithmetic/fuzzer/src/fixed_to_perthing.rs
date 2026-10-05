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
//! Running this fuzzer can be done with `cargo hfuzz run fixed_to_perthing`.
//! `honggfuzz` CLI options can be used by setting `HFUZZ_RUN_ARGS`, such as `-n 4` to use 4
//! threads.
//!
//! # Debugging a panic
//! Once a panic is found, it can be debugged with
//! `cargo hfuzz run-debug fixed_to_perthing hfuzz_workspace/fixed_to_perthing/*.fuzz`.
//!
//! # More information
//! More information about `honggfuzz` can be found
//! [here](https://docs.rs/honggfuzz/).

use core::convert::TryInto;
use honggfuzz::fuzz;
use sp_arithmetic::{
	traits::{Bounded, One, Zero},
	FixedI128, FixedI64, FixedPointNumber, FixedU128, FixedU64, PerThing, PerU16, Perbill, Percent,
	Permill, Perquintill, Rounding,
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
enum PerThingTypeSelector {
	Percent,
	Permill,
	Perbill,
	PerU16,
	Perquintill,
}

impl arbitrary::Arbitrary<'_> for PerThingTypeSelector {
	fn arbitrary(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Self> {
		Ok(match u.int_in_range(0..=4)? {
			0 => PerThingTypeSelector::Percent,
			1 => PerThingTypeSelector::Permill,
			2 => PerThingTypeSelector::Perbill,
			3 => PerThingTypeSelector::PerU16,
			4 => PerThingTypeSelector::Perquintill,
			_ => unreachable!(),
		})
	}
}

fn main() {
	loop {
		fuzz!(|data: (i128, FixedTypeSelector, PerThingTypeSelector)| {
			let (val1_i128, fixed_type_selector, per_thing_selector) = data;

			match fixed_type_selector {
				FixedTypeSelector::I64 => run_test::<FixedI64>(val1_i128, per_thing_selector),
				FixedTypeSelector::U64 => run_test::<FixedU64>(val1_i128, per_thing_selector),
				FixedTypeSelector::I128 => run_test::<FixedI128>(val1_i128, per_thing_selector),
				FixedTypeSelector::U128 => run_test::<FixedU128>(val1_i128, per_thing_selector),
			}
		});
	}
}

// Need a helper trait because try_into_perthing is not on FixedPointNumber
trait TryIntoPerThingHelper: FixedPointNumber + Sized {
	fn try_into_perthing_inherent<P: PerThing>(self) -> Result<P, P>
	where
		Self::Inner: TryInto<u128>,
		<Self::Inner as TryInto<u128>>::Error: core::fmt::Debug,
		u128: TryInto<P::Inner> + TryInto<P::Upper>,
		P::Inner: Into<u128>;
}

impl TryIntoPerThingHelper for FixedI64 {
	fn try_into_perthing_inherent<P: PerThing>(self) -> Result<P, P>
	where
		Self::Inner: TryInto<u128>,
		<Self::Inner as TryInto<u128>>::Error: core::fmt::Debug,
		u128: TryInto<P::Inner> + TryInto<P::Upper>,
		P::Inner: Into<u128>,
	{
		self.try_into_perthing::<P>()
	}
}

impl TryIntoPerThingHelper for FixedU64 {
	fn try_into_perthing_inherent<P: PerThing>(self) -> Result<P, P>
	where
		Self::Inner: TryInto<u128>,
		<Self::Inner as TryInto<u128>>::Error: core::fmt::Debug,
		u128: TryInto<P::Inner> + TryInto<P::Upper>,
		P::Inner: Into<u128>,
	{
		self.try_into_perthing::<P>()
	}
}

impl TryIntoPerThingHelper for FixedI128 {
	fn try_into_perthing_inherent<P: PerThing>(self) -> Result<P, P>
	where
		Self::Inner: TryInto<u128>,
		<Self::Inner as TryInto<u128>>::Error: core::fmt::Debug,
		u128: TryInto<P::Inner> + TryInto<P::Upper>,
		P::Inner: Into<u128>,
	{
		self.try_into_perthing::<P>()
	}
}

impl TryIntoPerThingHelper for FixedU128 {
	fn try_into_perthing_inherent<P: PerThing>(self) -> Result<P, P>
	where
		Self::Inner: TryInto<u128>,
		<Self::Inner as TryInto<u128>>::Error: core::fmt::Debug,
		u128: TryInto<P::Inner> + TryInto<P::Upper>,
		P::Inner: Into<u128>,
	{
		self.try_into_perthing::<P>()
	}
}

fn run_test<F>(v1: i128, per_thing_selector: PerThingTypeSelector)
where
	F: FixedPointNumber + core::fmt::Display + TryIntoPerThingHelper,
	F::Inner: TryInto<u128> + PartialOrd + Copy + Bounded + Zero + One,
	<F::Inner as TryInto<u128>>::Error: core::fmt::Debug,
{
	match per_thing_selector {
		PerThingTypeSelector::Percent => test_conversion::<F, Percent>(v1),
		PerThingTypeSelector::Permill => test_conversion::<F, Permill>(v1),
		PerThingTypeSelector::Perbill => test_conversion::<F, Perbill>(v1),
		PerThingTypeSelector::PerU16 => test_conversion::<F, PerU16>(v1),
		PerThingTypeSelector::Perquintill => test_conversion::<F, Perquintill>(v1),
	}
}

fn test_conversion<F, P>(v1: i128)
where
	F: FixedPointNumber + core::fmt::Display + TryIntoPerThingHelper,
	F::Inner: TryInto<u128> + PartialOrd + Copy + Bounded + Zero + One,
	<F::Inner as TryInto<u128>>::Error: core::fmt::Debug,
	P: PerThing + Eq + core::fmt::Debug,
	u128: TryInto<P::Inner> + TryInto<P::Upper>,
	P::Inner: Into<u128> + PartialOrd + Copy + Zero + One,
{
	let fp1 = F::saturating_from_integer(v1);
	let res = fp1.try_into_perthing_inherent::<P>();

	if fp1 < F::zero() {
		assert_eq!(res, Err(P::zero()), "try_into_perthing failed for negative. fp1={}", fp1);
	} else if fp1 > F::one() {
		assert_eq!(res, Err(P::one()), "try_into_perthing failed for > 1. fp1={}", fp1);
	} else {
		let f_inner_u128 = fp1.into_inner().try_into().unwrap_or(0);
		let f_div_u128 = F::DIV.try_into().unwrap_or(1);
		let expected_res = P::from_rational_with_rounding(f_inner_u128, f_div_u128, Rounding::Down);

		// The P::from_rational call inside try_into_perthing might return Err(()) if
		// the internal multiply_rational overflows. We expect our oracle `expected_res`
		// to also return Err(()) in that case.
		assert_eq!(
			res.map_err(|_| ()),
			expected_res.map_err(|_| ()),
			"try_into_perthing mismatch. fp1={}",
			fp1
		);
	}
}
