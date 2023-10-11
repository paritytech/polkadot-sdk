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

//! Various implementations and utilities for matching and filtering `MultiLocation` and
//! `InteriorMultiLocation` types.

use frame_support::traits::{Contains, Everything, Get};
use xcm::latest::{InteriorMultiLocation, MultiLocation};

/// Trait for matching location of type `T`.
pub trait MatchesLocation<T> {
	fn matches(&self, location: &T) -> bool;
}

/// A [`MatchesLocation`] implementation that matches every value.
impl<T> MatchesLocation<T> for Everything {
	fn matches(&self, _: &T) -> bool {
		true
	}
}

/// Adapter for using `trait Contains` with `trait MatchesLocation` were we can provide a tuple of
/// `Contains` implementations which are used for matching location of type `T`.
pub struct MatchesLocationAdapter<T, Filter> {
	_marker: sp_std::marker::PhantomData<(T, Filter)>,
}
impl<T, Filter> MatchesLocationAdapter<T, Filter>
where
	Filter: Contains<T>,
{
	pub fn new() -> Self {
		Self { _marker: sp_std::marker::PhantomData }
	}
}
impl<T, Filter> MatchesLocation<T> for MatchesLocationAdapter<T, Filter>
where
	Filter: Contains<T>,
{
	fn matches(&self, location: &T) -> bool {
		Filter::contains(location)
	}
}

/// Type alias for `MatchesLocationAdapter` implementation which works with `InteriorMultiLocation`.
pub type InteriorLocationMatcher<Filter> = MatchesLocationAdapter<InteriorMultiLocation, Filter>;

/// Type alias for `MatchesLocationAdapter` implementation which works with `MultiLocation`.
pub type LocationMatcher<Filter> = MatchesLocationAdapter<MultiLocation, Filter>;

/// An implementation of [frame_support::traits::Contains] that checks for `MultiLocation` or
/// `InteriorMultiLocation` if it starts with the provided type `T`.
pub struct StartsWith<T>(sp_std::marker::PhantomData<T>);
impl<T: Get<MultiLocation>> Contains<MultiLocation> for StartsWith<T> {
	fn contains(t: &MultiLocation) -> bool {
		t.starts_with(&T::get())
	}
}
impl<T: Get<InteriorMultiLocation>> Contains<InteriorMultiLocation> for StartsWith<T> {
	fn contains(t: &InteriorMultiLocation) -> bool {
		t.starts_with(&T::get())
	}
}

/// An implementation of [frame_support::traits::Contains] that checks the equality of MultiLocation
/// or InteriorMultiLocation with the provided type T.
pub struct Equals<T>(sp_std::marker::PhantomData<T>);
impl<T: Get<MultiLocation>> Contains<MultiLocation> for Equals<T> {
	fn contains(t: &MultiLocation) -> bool {
		t == &T::get()
	}
}
impl<T: Get<InteriorMultiLocation>> Contains<InteriorMultiLocation> for Equals<T> {
	fn contains(t: &InteriorMultiLocation) -> bool {
		t == &T::get()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn matches_location_adapter_with_contains_tuple_works() {
		type Loc = usize;
		frame_support::match_types! {
			pub type AllowOnly2000And3000And4000: impl Contains<Loc> = {
				2000 | 3000 | 4000
			};
		}
		struct Between5000And5003;
		impl Contains<Loc> for Between5000And5003 {
			fn contains(t: &Loc) -> bool {
				(&5000 < t) && (t < &5003)
			}
		}

		let test_data = vec![
			(1000, false),
			(2000, true),
			(3000, true),
			(4000, true),
			(5000, false),
			(5001, true),
			(5002, true),
			(5003, false),
		];

		for (location, expected_result) in test_data {
			assert_eq!(
				MatchesLocationAdapter::<
					Loc,
					(AllowOnly2000And3000And4000, Between5000And5003)
				>::new().matches(&location),
				expected_result,
			)
		}
	}
}
