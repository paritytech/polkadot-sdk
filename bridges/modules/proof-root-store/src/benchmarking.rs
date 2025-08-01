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
	// use frame_system::Pallet;

	use super::*;
	use crate::MaybeRootsToKeep;

	#[benchmark]
	fn note_new_roots() {
		let roots_to_keep = MaybeRootsToKeep::<T, I>::get().iter().count();
		let mut roots_store: BoundedVec<(T::Key, T::Value), T::RootsToKeep> = BoundedVec::new();

		// create data
		for id in 0..roots_to_keep {
			let (key, value) = T::BenchmarkHelper::create_key_value_for(id.try_into().unwrap());
			let _ = roots_store.try_push((key, value));
		}

		let caller = whitelisted_caller();
		#[block]
		{
			assert_ok!(Pallet::<T, I>::note_new_roots(
				RawOrigin::Signed(caller).into(),
				roots_store
			));
		}
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::TestRuntime,);
}
