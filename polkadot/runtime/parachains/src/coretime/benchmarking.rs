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

use super::*;
use frame_benchmarking::v2::*;
use frame_support::traits::OriginTrait;
use pallet_broker::CoreIndex as BrokerCoreIndex;

#[benchmarks]
mod benchmarks {
	use super::*;
	use assigner_coretime::PartsOf57600;

	#[benchmark]
	fn request_core_count() {
		// Setup
		let root_origin = <T as frame_system::Config>::RuntimeOrigin::root();

		#[extrinsic_call]
		_(
			root_origin as <T as frame_system::Config>::RuntimeOrigin,
			// random core count
			100,
		)
	}

	#[benchmark]
	fn assign_core(s: Linear<1, 100>) {
		// Setup
		let root_origin = <T as frame_system::Config>::RuntimeOrigin::root();

		// Use parameterized assignment count
		let mut assignments: Vec<(CoreAssignment, PartsOf57600)> = vec![0u16; s as usize - 1]
			.into_iter()
			.enumerate()
			.map(|(index, parts)| {
				(CoreAssignment::Task(index as u32), PartsOf57600::new_saturating(parts))
			})
			.collect();
		// Parts must add up to exactly 57600. Here we add all the parts in one assignment, as
		// it won't effect the weight and splitting up the parts into even groupings may not
		// work for every value `s`.
		assignments.push((CoreAssignment::Task(s as u32), PartsOf57600::FULL));

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
}
