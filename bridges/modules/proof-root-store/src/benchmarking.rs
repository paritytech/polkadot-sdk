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

//! Benchmarking for the `pallet-proof-root-store`.

#![cfg(feature = "runtime-benchmarks")]

use crate::{Config, Pallet};
use frame_benchmarking::v2::*;
use frame_support::{assert_ok, traits::Get, BoundedVec};
use frame_system::RawOrigin;

#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelper<Key, Value> {
	fn create_key_value_for(id: u32) -> (Key, Value);
}

#[instance_benchmarks]
mod benchmarks {

	use super::*;

	#[benchmark]
	fn note_new_roots() {
		// prepare data to store
		let mut roots_to_store: BoundedVec<(T::Key, T::Value), T::RootsToKeep> = BoundedVec::new();
		for id in 0..T::RootsToKeep {
			let (key, value) = T::BenchmarkHelper::create_key_value_for(id);
			let _ = roots_to_store.try_push((key, value));
		}

		#[extrinsic_call]
		_(RawOrigin::Signed(whitelisted_caller()), roots_to_store.clone());
		
		// TODO: add separate assert for iterating `roots_to_store` and check for all `Pallet::<T, I>::get_root(...)`
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::TestRuntime,);
}
