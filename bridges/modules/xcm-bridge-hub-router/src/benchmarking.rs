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

use crate::{DeliveryFeeFactor, MINIMAL_DELIVERY_FEE_FACTOR};
use frame_benchmarking::{benchmarks_instance_pallet, BenchmarkError};
use frame_support::traits::{Get, Hooks};
use sp_runtime::traits::Zero;
use xcm::prelude::*;

/// Pallet we're benchmarking here.
pub struct Pallet<T: Config<I>, I: 'static = ()>(crate::Pallet<T, I>);

/// Trait that must be implemented by runtime to be able to benchmark pallet properly.
pub trait Config<I: 'static>: crate::Config<I> {
	/// Fill up queue so it becomes congested.
	fn make_congested();

	/// Returns destination which is valid for this router instance.
	/// (Needs to pass `T::Bridges`)
	/// Make sure that `SendXcm` will pass.
	fn ensure_bridged_target_destination() -> Result<Location, BenchmarkError> {
		Ok(Location::new(
			Self::UniversalLocation::get().len() as u8,
			[GlobalConsensus(Self::BridgedNetworkId::get().unwrap())],
		))
	}
}

benchmarks_instance_pallet! {
	on_initialize_when_non_congested {
		DeliveryFeeFactor::<T, I>::put(MINIMAL_DELIVERY_FEE_FACTOR + MINIMAL_DELIVERY_FEE_FACTOR);
	}: {
		crate::Pallet::<T, I>::on_initialize(Zero::zero())
	}

	on_initialize_when_congested {
		DeliveryFeeFactor::<T, I>::put(MINIMAL_DELIVERY_FEE_FACTOR + MINIMAL_DELIVERY_FEE_FACTOR);
		let _ = T::ensure_bridged_target_destination()?;
		T::make_congested();
	}: {
		crate::Pallet::<T, I>::on_initialize(Zero::zero())
	}
}
