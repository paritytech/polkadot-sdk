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

//! Various implementations and utilities for matching and filtering `Location` and
//! `InteriorLocation` types.

use frame_support::traits::{Contains, Get};
use sp_runtime::traits::MaybeEquivalence;
use sp_std::marker::PhantomData;
use xcm::latest::{InteriorLocation, Location, NetworkId};

/// An implementation of `Contains` that checks for `Location` or
/// `InteriorLocation` if starts with the provided type `T`.
pub struct StartsWith<T, L = Location>(sp_std::marker::PhantomData<(T, L)>);
impl<T: Get<L>, L: TryInto<Location> + Clone> Contains<L> for StartsWith<T, L> {
	fn contains(location: &L) -> bool {
		let latest_location: Location =
			if let Ok(location) = (*location).clone().try_into() { location } else { return false };
		let latest_t = if let Ok(location) = T::get().try_into() { location } else { return false };
		latest_location.starts_with(&latest_t)
	}
}
impl<T: Get<InteriorLocation>> Contains<InteriorLocation> for StartsWith<T> {
	fn contains(t: &InteriorLocation) -> bool {
		t.starts_with(&T::get())
	}
}

/// An implementation of `Contains` that checks for `Location` or
/// `InteriorLocation` if starts with expected `GlobalConsensus(NetworkId)` provided as type
/// `T`.
pub struct StartsWithExplicitGlobalConsensus<T>(sp_std::marker::PhantomData<T>);
impl<T: Get<NetworkId>> Contains<Location> for StartsWithExplicitGlobalConsensus<T> {
	fn contains(location: &Location) -> bool {
		matches!(location.interior().global_consensus(), Ok(requested_network) if requested_network.eq(&T::get()))
	}
}
impl<T: Get<NetworkId>> Contains<InteriorLocation> for StartsWithExplicitGlobalConsensus<T> {
	fn contains(location: &InteriorLocation) -> bool {
		matches!(location.global_consensus(), Ok(requested_network) if requested_network.eq(&T::get()))
	}
}

/// An adapter implementation of `MaybeEquivalence` which can convert between the latest `Location`
/// and other versions that implement `TryInto<Location>` and `TryFrom<Location>`.
pub struct WithLatestLocationConverter<Target>(PhantomData<Target>);
impl<Target: TryInto<Location> + TryFrom<Location> + Clone> MaybeEquivalence<Location, Target>
	for WithLatestLocationConverter<Target>
{
	fn convert(old: &Location) -> Option<Target> {
		(*old).clone().try_into().ok()
	}

	fn convert_back(new: &Target) -> Option<Location> {
		new.clone().try_into().ok()
	}
}
