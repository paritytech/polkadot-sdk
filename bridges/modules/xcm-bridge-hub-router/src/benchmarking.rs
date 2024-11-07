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
use frame_benchmarking::{benchmarks_instance_pallet, BenchmarkError, BenchmarkResult};
use frame_support::traits::{EnsureOriginWithArg, Hooks, UnfilteredDispatchable};
use sp_runtime::{traits::Zero, Saturating};
use xcm::prelude::*;

/// Pallet we're benchmarking here.
pub struct Pallet<T: Config<I>, I: 'static = ()>(crate::Pallet<T, I>);

/// Trait that must be implemented by runtime to be able to benchmark pallet properly.
pub trait Config<I: 'static>: crate::Config<I> {
	// /// Fill up queue so it becomes congested.
	// fn make_congested();
	//
	/// Returns destination which is valid for this router instance.
	fn ensure_bridged_target_destination() -> Result<Location, BenchmarkError>;
}

benchmarks_instance_pallet! {
	on_initialize_when_bridge_state_removed {
		let bridge_id = T::BridgeIdResolver::resolve_for_dest(&T::ensure_bridged_target_destination()?)
			.ok_or(BenchmarkError::Weightless)?;
		// uncongested and less than a minimal factor is removed
		Bridges::<T, I>::insert(&bridge_id, BridgeState {
			delivery_fee_factor: 0.into(),
			is_congested: false,
		});
		assert!(Bridges::<T, I>::get(&bridge_id).is_some());
	}: {
		crate::Pallet::<T, I>::on_initialize(Zero::zero())
	} verify {
		assert!(Bridges::<T, I>::get(bridge_id).is_none());
	}

	on_initialize_when_bridge_state_updated {
		let bridge_id = T::BridgeIdResolver::resolve_for_dest(&T::ensure_bridged_target_destination()?)
			.ok_or(BenchmarkError::Weightless)?;
		// uncongested and higher than a minimal factor is decreased
		let old_delivery_fee_factor = MINIMAL_DELIVERY_FEE_FACTOR.saturating_mul(1000.into());
		Bridges::<T, I>::insert(&bridge_id, BridgeState {
			delivery_fee_factor: old_delivery_fee_factor,
			is_congested: false,
		});
		assert!(Bridges::<T, I>::get(&bridge_id).is_some());
	}: {
		crate::Pallet::<T, I>::on_initialize(Zero::zero())
	} verify {
		assert!(Bridges::<T, I>::get(bridge_id).unwrap().delivery_fee_factor < old_delivery_fee_factor);
	}

	report_bridge_status {
		let bridge_id = T::BridgeIdResolver::resolve_for_dest(&T::ensure_bridged_target_destination()?)
			.ok_or(BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)))?;
		let origin: T::RuntimeOrigin = T::BridgeHubOrigin::try_successful_origin(&bridge_id).map_err(|_| BenchmarkError::Weightless)?;
		let is_congested = true;

		let call = Call::<T, I>::report_bridge_status { bridge_id: bridge_id.clone(), is_congested };
	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(
			Bridges::<T, I>::get(&bridge_id),
			Some(BridgeState {
				delivery_fee_factor: MINIMAL_DELIVERY_FEE_FACTOR,
				is_congested,
			})
		);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::TestRuntime)
}
