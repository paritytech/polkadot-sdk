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
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use assigner_bulk::MAX_ASSIGNMENTS_PER_SCHEDULE;
//use xcm::latest::prelude::*;

#[benchmarks]
mod benchmarks {
	use super::*;
	#[benchmark]
	fn assign_core(s: Linear<0, MAX_ASSIGNMENTS_PER_SCHEDULE>) {
		// Setup
		let caller: <T as frame_system::Config>::AccountId = whitelisted_caller();
		// TODO: Construct a proper Xcm broker parachain message origin
		// let broker_id: u32 = 1004; 
		// let broker_origin = <T as Config>::XcmPallet::Origin::Xcm(MultiLocation { parents: 0, interior: X1(Parachain(broker_id)) });
		
		// Use valid assignment set with maximum number of assignments to maximize work
		let assignments: Vec<(CoreAssignment, PartsOf57600)> = vec![576u16; s as usize]
			.into_iter()
			.enumerate()
			.map(|(index, parts)| (CoreAssignment::Task(index as u32), parts))
			.collect();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.into()), CoreIndex(0), BlockNumberFor::<T>::from(5u32), assignments, Some(BlockNumberFor::<T>::from(20u32)))
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(
			crate::assigner_bulk::mock_helpers::GenesisConfigBuilder::default().build()
		),
		crate::mock::Test
	);
}
