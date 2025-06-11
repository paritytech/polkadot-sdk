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

//! Implementation for `ContainsPair<Location, Location>`.

use core::marker::PhantomData;
use frame_support::traits::{Contains, ContainsPair, Get};
use xcm::latest::prelude::*;

/// Alias a Foreign `AccountId32` with a local `AccountId32` if the foreign `AccountId32` matches
/// the `Prefix` pattern.
///
/// Requires that the prefixed origin `AccountId32` matches the target `AccountId32`.
pub struct AliasForeignAccountId32<Prefix>(PhantomData<Prefix>);
impl<Prefix: Contains<Location>> ContainsPair<Location, Location>
	for AliasForeignAccountId32<Prefix>
{
	fn contains(origin: &Location, target: &Location) -> bool {
		if let (prefix, Some(account_id @ AccountId32 { .. })) =
			origin.clone().split_last_interior()
		{
			return Prefix::contains(&prefix) &&
				*target == Location { parents: 0, interior: [account_id].into() }
		}
		false
	}
}

/// Alias a descendant location of the original origin.
pub struct AliasChildLocation;
impl ContainsPair<Location, Location> for AliasChildLocation {
	fn contains(origin: &Location, target: &Location) -> bool {
		return target.starts_with(origin)
	}
}

/// Alias a location if it passes `Filter` and the original origin is root of `Origin`.
///
/// This can be used to allow (trusted) system chains root to alias into other locations.
/// **Warning**: do not use with untrusted `Origin` chains.
pub struct AliasOriginRootUsingFilter<Origin, Filter>(PhantomData<(Origin, Filter)>);
impl<Origin, Filter> ContainsPair<Location, Location> for AliasOriginRootUsingFilter<Origin, Filter>
where
	Origin: Get<Location>,
	Filter: Contains<Location>,
{
	fn contains(origin: &Location, target: &Location) -> bool {
		// check that `origin` is a root location
		match origin.unpack() {
			(1, [Parachain(_)]) |
			(2, [GlobalConsensus(_)]) |
			(2, [GlobalConsensus(_), Parachain(_)]) => (),
			_ => return false,
		};
		// check that `origin` matches `Origin` and `target` matches `Filter`
		return Origin::get().eq(origin) && Filter::contains(target)
	}
}
