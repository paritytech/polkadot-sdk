// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

#![cfg(feature = "runtime-benchmarks")]

use frame_benchmarking::{impl_benchmark_test_suite, v2::*};

/// Pallet we're benchmarking here.
pub struct Pallet<T: Config>(sp_std::marker::PhantomData<T>);

pub trait Config:
	pallet_bridge_grandpa::Config
	+ pallet_bridge_messages::Config
	+ pallet_bridge_relayers::Config
	+ pallet_utility::Config
{
	fn setup_environment();
	fn run_extension(choice: u32);
	fn run_grandpa_extension(choice: u32);
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn parachain_extension(c: Linear<0, 1>) {
		T::setup_environment();

		#[block]
		{
			T::run_extension(c);
		}
	}

	#[benchmark]
	fn grandpa_extension(c: Linear<0, 1>) {
		T::setup_environment();

		#[block]
		{
			T::run_grandpa_extension(c);
		}
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::TestRuntime);
}
