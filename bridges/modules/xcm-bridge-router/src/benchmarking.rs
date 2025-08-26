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

//! XCM bridge hub router pallet benchmarks.

#![cfg(feature = "runtime-benchmarks")]

use crate::{BridgeState, Bridges, Call, ResolveBridgeId, MINIMAL_DELIVERY_FEE_FACTOR};
use frame_benchmarking::v2::*;
use frame_support::traits::EnsureOriginWithArg;
use xcm::prelude::*;

/// Pallet we're benchmarking here.
pub struct Pallet<T: Config<I>, I: 'static = ()>(crate::Pallet<T, I>);

/// Trait that must be implemented by runtime to be able to benchmark pallet properly.
pub trait Config<I: 'static>: crate::Config<I> {
	/// Returns destination which is valid for this router instance.
	fn ensure_bridged_target_destination() -> Result<Location, BenchmarkError>;
	/// Returns valid origin for `update_bridge_status` (if `T::UpdateBridgeStatusOrigin` is
	/// supported).
	fn update_bridge_status_origin() -> Option<Self::RuntimeOrigin>;
}

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn update_bridge_status() -> Result<(), BenchmarkError> {
		let bridge_id =
			T::BridgeIdResolver::resolve_for_dest(&T::ensure_bridged_target_destination()?)
				.ok_or(BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)))?;
		let origin = T::update_bridge_status_origin()
			.ok_or(BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)))?;
		let _ = T::UpdateBridgeStatusOrigin::try_origin(origin.clone(), &bridge_id)
			.map_err(|_| BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)))?;
		let is_congested = true;

		#[extrinsic_call]
		update_bridge_status(origin as T::RuntimeOrigin, bridge_id.clone(), is_congested);

		assert_eq!(
			Bridges::<T, I>::get(&bridge_id),
			BridgeState { delivery_fee_factor: MINIMAL_DELIVERY_FEE_FACTOR, is_congested }
		);
		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::TestRuntime);
}
