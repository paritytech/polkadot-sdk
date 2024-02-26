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

//! Testing helpers that can be re-used by external crates.

use crate::Instant;
use arbitrary::{Arbitrary, Unstructured};
use sp_runtime::traits::Zero;

pub struct InstantFuzzer<I>(core::marker::PhantomData<I>);

impl<I> InstantFuzzer<I>
where
	for<'a> I: Instant + Arbitrary<'a>,
	for<'a> I::Duration: Arbitrary<'a>,
{
	pub fn fuzz() {
		Self::prop_duration_is_not_negative();
	}

	/// Ensure that a `Duration` is never negative.
	fn prop_duration_is_not_negative() {
		Self::with_durations(1_000_000, |d| assert!(d >= I::Duration::zero()));
	}

	fn with_durations(reps: u32, f: impl Fn(I::Duration)) {
		for _ in 0..reps {
			let seed = u32::arbitrary(&mut Unstructured::new(&[0; 4])).unwrap();
			f(Self::duration(seed));
		}
	}

	fn duration(seed: u32) -> I::Duration {
		let seed = sp_core::blake2_256(&seed.to_le_bytes());
		let mut unstructured = Unstructured::new(&seed);
		I::Duration::arbitrary(&mut unstructured).unwrap()
	}
}
