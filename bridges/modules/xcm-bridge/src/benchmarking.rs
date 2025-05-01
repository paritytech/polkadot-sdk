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

//! XCM bridge hub pallet benchmarks.

#![cfg(feature = "runtime-benchmarks")]

use crate::{Call, Receiver, ThisChainOf};
use bp_runtime::BalanceOf;
use bp_xcm_bridge::BridgeLocations;
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	traits::{fungible::Unbalanced, tokens::Precision, Contains, Get},
};
use sp_std::boxed::Box;
use xcm_executor::traits::ConvertLocation;

use sp_runtime::Saturating;
use xcm::prelude::*;

/// Pallet we're benchmarking here.
pub struct Pallet<T: Config<I>, I: 'static = ()>(crate::Pallet<T, I>);

/// Trait that must be implemented by runtime to be able to benchmark pallet properly.
pub trait Config<I: 'static>: crate::Config<I> {
	/// Returns a valid origin along with the initial balance (e.g., existential deposit),
	/// required for operation `open_bridge`.
	/// If `None`, that means that `open_bridge` is not supported.
	fn open_bridge_origin() -> Option<(Self::RuntimeOrigin, BalanceOf<ThisChainOf<Self, I>>)>;
}

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	fn prepare_for_open_bridge<T: Config<I>, I: 'static>(
	) -> Result<(T::RuntimeOrigin, Box<BridgeLocations>), BenchmarkError> {
		let (origin, initial_balance) = T::open_bridge_origin()
			.ok_or(BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)))?;
		let bridge_destination_universal_location: Box<VersionedInteriorLocation> =
			Box::new([GlobalConsensus(crate::Pallet::<T, I>::bridged_network_id()?)].into());
		let expected = crate::Pallet::<T, I>::bridge_locations_from_origin(
			origin.clone(),
			bridge_destination_universal_location,
		)?;

		if !T::AllowWithoutBridgeDeposit::contains(expected.bridge_origin_relative_location()) {
			// fund origin's sovereign account
			let bridge_owner_account = T::BridgeOriginAccountIdConverter::convert_location(
				expected.bridge_origin_relative_location(),
			)
			.ok_or(BenchmarkError::Stop("InvalidBridgeOriginAccount"))?;

			T::Currency::increase_balance(
				&bridge_owner_account,
				initial_balance.saturating_add(T::BridgeDeposit::get()),
				Precision::BestEffort,
			)?;
		}

		Ok((origin, expected))
	}

	#[benchmark]
	fn open_bridge() -> Result<(), BenchmarkError> {
		let (origin, locations) = prepare_for_open_bridge::<T, I>()?;
		let bridge_destination_universal_location: Box<VersionedInteriorLocation> =
			Box::new(locations.bridge_destination_universal_location().clone().into());
		assert!(crate::Pallet::<T, I>::bridge(locations.bridge_id()).is_none());

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, bridge_destination_universal_location, None);

		assert!(crate::Pallet::<T, I>::bridge(locations.bridge_id()).is_some());
		Ok(())
	}

	#[benchmark]
	fn close_bridge() -> Result<(), BenchmarkError> {
		let (origin, locations) = prepare_for_open_bridge::<T, I>()?;
		let bridge_destination_universal_location: Box<VersionedInteriorLocation> =
			Box::new(locations.bridge_destination_universal_location().clone().into());
		assert!(crate::Pallet::<T, I>::bridge(locations.bridge_id()).is_none());

		// open bridge
		assert_ok!(crate::Pallet::<T, I>::open_bridge(
			origin.clone(),
			bridge_destination_universal_location.clone(),
			None,
		));
		assert!(crate::Pallet::<T, I>::bridge(locations.bridge_id()).is_some());

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, bridge_destination_universal_location, 10);

		assert!(crate::Pallet::<T, I>::bridge(locations.bridge_id()).is_none());
		Ok(())
	}

	#[benchmark]
	fn update_notification_receiver() -> Result<(), BenchmarkError> {
		let (origin, locations) = prepare_for_open_bridge::<T, I>()?;
		let bridge_destination_universal_location: Box<VersionedInteriorLocation> =
			Box::new(locations.bridge_destination_universal_location().clone().into());
		assert!(crate::Pallet::<T, I>::bridge(locations.bridge_id()).is_none());

		// open bridge with None
		assert_ok!(crate::Pallet::<T, I>::open_bridge(
			origin.clone(),
			bridge_destination_universal_location.clone(),
			None,
		));
		assert_eq!(
			crate::Pallet::<T, I>::bridge(locations.bridge_id()).map(|b| b.maybe_notify),
			Some(None)
		);

		#[extrinsic_call]
		_(
			origin as T::RuntimeOrigin,
			bridge_destination_universal_location,
			Some(Receiver::new(1, 5)),
		);

		assert_eq!(
			crate::Pallet::<T, I>::bridge(locations.bridge_id()).map(|b| b.maybe_notify),
			Some(Some(Receiver::new(1, 5)))
		);
		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::TestRuntime);
}
