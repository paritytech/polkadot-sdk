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

//! The pallet's benchmarks.

#![cfg(feature = "runtime-benchmarks")]

use crate::{Config, Pallet, RootsToSend};
use frame_benchmarking::v2::*;
use frame_support::{
	traits::{Get, Hooks},
	weights::Weight,
};

#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelper<Key, Value> {
	fn create_key_value_for(id: u32) -> (Key, Value);
}

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn on_idle() -> Result<(), BenchmarkError> {
		// create data
		for id in 0..T::MaxRootsToSend::get() {
			let (key, value) = T::BenchmarkHelper::create_key_value_for(id);
			Pallet::<T, I>::schedule_for_sync(key, value);
		}
		let number_to_send = RootsToSend::<T, I>::get().iter().count();

		#[block]
		{
			Pallet::<T, I>::on_idle(0u32.into(), Weight::MAX);
		}

		// check drained
		assert_eq!(
			RootsToSend::<T, I>::get().iter().count(),
			number_to_send.saturating_sub(T::MaxRootsToSend::get() as _)
		);
		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::TestRuntime);
}
