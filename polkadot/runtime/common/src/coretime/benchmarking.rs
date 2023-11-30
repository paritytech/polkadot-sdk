// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! On demand assigner pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use super::{Pallet, *};
use assigner_bulk::MAX_ASSIGNMENTS_PER_SCHEDULE;
use frame_benchmarking::v2::*;
use frame_support::traits::OriginTrait;
use pallet_broker::CoreIndex as BrokerCoreIndex;

#[benchmarks]
mod benchmarks {
	use super::*;
	#[benchmark]
	fn assign_core(s: Linear<1, MAX_ASSIGNMENTS_PER_SCHEDULE>) {
		// Setup
		let root_origin = <T as frame_system::Config>::RuntimeOrigin::root();

		// Use parameterized assignment count
		let assignments: Vec<(CoreAssignment, PartsOf57600)> = vec![576u16; s as usize]
			.into_iter()
			.enumerate()
			.map(|(index, parts)| (CoreAssignment::Task(index as u32), parts))
			.collect();

		let core_index: BrokerCoreIndex = 0;

		#[extrinsic_call]
		_(
			root_origin as <T as frame_system::Config>::RuntimeOrigin,
			core_index,
			BlockNumberFor::<T>::from(5u32),
			assignments,
			Some(BlockNumberFor::<T>::from(20u32)),
		)
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(
			crate::assigner_bulk::mock_helpers::GenesisConfigBuilder::default().build()
		),
		crate::mock::Test
	);
}
