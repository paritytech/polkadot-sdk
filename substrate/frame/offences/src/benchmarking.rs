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

//! Benchmarks for the offences pallet.
//!
//! The benchmarks measure an offence report through the runtime's `OnOffenceHandler`, and do not
//! need a staking pallet.

use crate::{Config as OffencesConfig, Pallet as Offences};
use alloc::{vec, vec::Vec};
use frame_benchmarking::v2::*;
use sp_runtime::Perbill;
use sp_staking::{
	offence::{Kind, Offence, ReportOffence},
	SessionIndex,
};

/// The pallet that the benchmarks run on.
pub struct Pallet<T: Config>(Offences<T>);

/// The runtime parts that the benchmarks need.
pub trait Config: OffencesConfig {
	/// Returns the offender that costs the runtime's `OnOffenceHandler` the most to process.
	///
	/// Set up all the state that the handler reads for this offender before you return it.
	fn offender() -> Self::IdentificationTuple;
}

/// An offence with one offender, as in a BABE or GRANDPA equivocation report.
struct BenchmarkOffence<Offender> {
	offender: Offender,
}

impl<Offender: Clone> Offence<Offender> for BenchmarkOffence<Offender> {
	const ID: Kind = *b"offences:benchmk";
	type Slot = u64;

	fn offenders(&self) -> Vec<Offender> {
		vec![self.offender.clone()]
	}

	fn session_index(&self) -> SessionIndex {
		0
	}

	fn validator_set_count(&self) -> u32 {
		1
	}

	fn slot(&self) -> Self::Slot {
		0
	}

	// The worst case: the handler slashes the full amount.
	fn slash_fraction(&self, _offenders_count: u32) -> Perbill {
		Perbill::one()
	}
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn report_offence() -> Result<(), BenchmarkError> {
		let reporters = vec![account("reporter", 0, 0)];
		let offence = BenchmarkOffence { offender: T::offender() };

		#[block]
		{
			Offences::<T>::report_offence(reporters, offence)
				.map_err(|_| BenchmarkError::Stop("the offence report failed"))?;
		}

		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Runtime);
}
